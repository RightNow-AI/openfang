//! Facebook Messenger Platform channel adapter.
//!
//! Uses the Facebook Messenger Platform Send API (Graph API v18.0) for sending
//! messages and a webhook HTTP server for receiving inbound events. The webhook
//! supports both GET (verification challenge) and POST (message events).
//! Authentication uses the page access token as a query parameter on the Send API.

use crate::types::{
    split_message, ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser,
};
use async_trait::async_trait;
use chrono::Utc;
use futures::Stream;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};
use zeroize::Zeroizing;

/// Facebook Graph API base URL for sending messages.
const GRAPH_API_BASE: &str = "https://graph.facebook.com/v18.0";

/// Maximum Messenger message text length (characters).
const MAX_MESSAGE_LEN: usize = 2000;

/// Facebook Messenger Platform adapter.
///
/// Inbound messages arrive via a webhook HTTP server that supports:
/// - GET requests for Facebook's webhook verification challenge
/// - POST requests for incoming message events
///
/// Outbound messages are sent via the Messenger Send API using
/// the page access token for authentication.
pub struct MessengerAdapter {
    /// SECURITY: Page access token for the Send API, zeroized on drop.
    page_token: Zeroizing<String>,
    /// SECURITY: Verify token for webhook registration, zeroized on drop.
    verify_token: Zeroizing<String>,
    /// Port on which the inbound webhook HTTP server listens.
    webhook_port: u16,
    /// HTTP client for outbound API calls.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
}

impl MessengerAdapter {
    /// Create a new Messenger adapter.
    ///
    /// # Arguments
    /// * `page_token` - Facebook page access token for the Send API.
    /// * `verify_token` - Token used to verify the webhook during Facebook's setup.
    /// * `webhook_port` - Local port for the inbound webhook HTTP server.
    pub fn new(page_token: String, verify_token: String, webhook_port: u16, client: reqwest::Client) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            page_token: Zeroizing::new(page_token),
            verify_token: Zeroizing::new(verify_token),
            webhook_port,
            client,
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
        }
    }

    /// Validate the page token by calling the Graph API to get page info.
    async fn validate(&self) -> Result<String, Box<dyn std::error::Error>> {
        let url = format!(
            "{}/me?access_token={}",
            GRAPH_API_BASE,
            self.page_token.as_str()
        );

        let resp = self.client.get(&url).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Messenger authentication failed {status}: {body}").into());
        }

        let body: serde_json::Value = resp.json().await?;
        let page_name = body["name"].as_str().unwrap_or("Messenger Bot").to_string();
        Ok(page_name)
    }

    /// Send a text message to a Messenger user via the Send API.
    async fn api_send_message(
        &self,
        recipient_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!(
            "{}/me/messages?access_token={}",
            GRAPH_API_BASE,
            self.page_token.as_str()
        );

        let chunks = split_message(text, MAX_MESSAGE_LEN);

        for chunk in chunks {
            let body = serde_json::json!({
                "recipient": {
                    "id": recipient_id,
                },
                "message": {
                    "text": chunk,
                },
                "messaging_type": "RESPONSE",
            });

            let resp = self.client.post(&url).json(&body).send().await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let resp_body = resp.text().await.unwrap_or_default();
                return Err(format!("Messenger Send API error {status}: {resp_body}").into());
            }
        }

        Ok(())
    }

    /// Send a typing indicator (sender action) to a Messenger user.
    async fn api_send_action(
        &self,
        recipient_id: &str,
        action: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!(
            "{}/me/messages?access_token={}",
            GRAPH_API_BASE,
            self.page_token.as_str()
        );

        let body = serde_json::json!({
            "recipient": {
                "id": recipient_id,
            },
            "sender_action": action,
        });

        let _ = self.client.post(&url).json(&body).send().await;
        Ok(())
    }

    /// Mark a message as seen via sender action.
    #[allow(dead_code)]
    async fn mark_seen(&self, recipient_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.api_send_action(recipient_id, "mark_seen").await
    }
}

/// Parse Facebook Messenger webhook entry into `ChannelMessage` values.
///
/// A single webhook POST can contain multiple entries, each with multiple
/// messaging events. This function processes one entry and returns all
/// valid messages found.
fn parse_messenger_entry(entry: &serde_json::Value) -> Vec<ChannelMessage> {
    let mut messages = Vec::new();

    let messaging = match entry["messaging"].as_array() {
        Some(arr) => arr,
        None => return messages,
    };

    for event in messaging {
        // Only handle message events (not delivery, read, postback, etc.)
        let message = match event.get("message") {
            Some(m) => m,
            None => continue,
        };

        // Skip echo messages (sent by the page itself)
        if message["is_echo"].as_bool().unwrap_or(false) {
            continue;
        }

        let text = match message["text"].as_str() {
            Some(t) if !t.is_empty() => t,
            _ => continue,
        };

        let sender_id = event["sender"]["id"].as_str().unwrap_or("").to_string();
        let recipient_id = event["recipient"]["id"].as_str().unwrap_or("").to_string();
        let msg_id = message["mid"].as_str().unwrap_or("").to_string();
        let timestamp = event["timestamp"].as_u64().unwrap_or(0);

        let content = if text.starts_with('/') {
            let parts: Vec<&str> = text.splitn(2, ' ').collect();
            let cmd_name = parts[0].trim_start_matches('/');
            let args: Vec<String> = parts
                .get(1)
                .map(|a| a.split_whitespace().map(String::from).collect())
                .unwrap_or_default();
            ChannelContent::Command {
                name: cmd_name.to_string(),
                args,
            }
        } else {
            ChannelContent::Text(text.to_string())
        };

        let mut metadata = HashMap::new();
        metadata.insert(
            "sender_id".to_string(),
            serde_json::Value::String(sender_id.clone()),
        );
        metadata.insert(
            "recipient_id".to_string(),
            serde_json::Value::String(recipient_id),
        );
        metadata.insert(
            "timestamp".to_string(),
            serde_json::Value::Number(serde_json::Number::from(timestamp)),
        );

        // Check for quick reply payload
        if let Some(qr) = message.get("quick_reply") {
            if let Some(payload) = qr["payload"].as_str() {
                metadata.insert(
                    "quick_reply_payload".to_string(),
                    serde_json::Value::String(payload.to_string()),
                );
            }
        }

        // Check for NLP entities (if enabled on the page)
        if let Some(nlp) = message.get("nlp") {
            if let Some(entities) = nlp.get("entities") {
                metadata.insert("nlp_entities".to_string(), entities.clone());
            }
        }

        messages.push(ChannelMessage {
            channel: ChannelType::Custom("messenger".to_string()),
            platform_message_id: msg_id,
            sender: ChannelUser {
                platform_id: sender_id,
                display_name: String::new(), // Messenger doesn't include name in webhook
                openfang_user: None,
            },
            content,
            target_agent: None,
            timestamp: Utc::now(),
            is_group: false, // Messenger Bot API is always 1:1
            thread_id: None,
            metadata,
        });
    }

    messages
}

#[async_trait]
impl ChannelAdapter for MessengerAdapter {
    fn name(&self) -> &str {
        "messenger"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("messenger".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        // Validate credentials
        let page_name = self.validate().await?;
        info!("Messenger adapter authenticated as {page_name}");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let port = self.webhook_port;
        let verify_token = self.verify_token.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let verify_token = Arc::new(verify_token);
            let tx = Arc::new(tx);

            let app = axum::Router::new().route(
                "/webhook",
                axum::routing::get({
                    // Facebook webhook verification handler
                    let vt = Arc::clone(&verify_token);
                    move |query: axum::extract::Query<HashMap<String, String>>| {
                        let vt = Arc::clone(&vt);
                        async move {
                            let mode = query.get("hub.mode").map(|s| s.as_str()).unwrap_or("");
                            let token = query
                                .get("hub.verify_token")
                                .map(|s| s.as_str())
                                .unwrap_or("");
                            let challenge = query.get("hub.challenge").cloned().unwrap_or_default();

                            if mode == "subscribe" && token == vt.as_str() {
                                info!("Messenger webhook verified");
                                (axum::http::StatusCode::OK, challenge)
                            } else {
                                warn!("Messenger webhook verification failed");
                                (axum::http::StatusCode::FORBIDDEN, String::new())
                            }
                        }
                    }
                })
                .post({
                    // Incoming message handler
                    let tx = Arc::clone(&tx);
                    move |body: axum::extract::Json<serde_json::Value>| {
                        let tx = Arc::clone(&tx);
                        async move {
                            let object = body.0["object"].as_str().unwrap_or("");
                            if object != "page" {
                                return axum::http::StatusCode::OK;
                            }

                            if let Some(entries) = body.0["entry"].as_array() {
                                for entry in entries {
                                    let msgs = parse_messenger_entry(entry);
                                    for msg in msgs {
                                        let _ = tx.send(msg).await;
                                    }
                                }
                            }

                            axum::http::StatusCode::OK
                        }
                    }
                }),
            );

            let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
            info!("Messenger webhook server listening on {addr}");

            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    warn!("Messenger webhook bind failed: {e}");
                    return;
                }
            };

            let server = axum::serve(listener, app);

            tokio::select! {
                result = server => {
                    if let Err(e) = result {
                        warn!("Messenger webhook server error: {e}");
                    }
                }
                _ = shutdown_rx.changed() => {
                    info!("Messenger adapter shutting down");
                }
            }
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn send(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match content {
            ChannelContent::Text(text) => {
                self.api_send_message(&user.platform_id, &text).await?;
            }
            ChannelContent::Image { url, caption } => {
                // Send image attachment via Messenger
                let api_url = format!(
                    "{}/me/messages?access_token={}",
                    GRAPH_API_BASE,
                    self.page_token.as_str()
                );

                let body = serde_json::json!({
                    "recipient": {
                        "id": user.platform_id,
                    },
                    "message": {
                        "attachment": {
                            "type": "image",
                            "payload": {
                                "url": url,
                                "is_reusable": true,
                            }
                        }
                    },
                    "messaging_type": "RESPONSE",
                });

                let resp = self.client.post(&api_url).json(&body).send().await?;
                if !resp.status().is_success() {
                    let status = resp.status();
                    let resp_body = resp.text().await.unwrap_or_default();
                    warn!("Messenger image send error {status}: {resp_body}");
                }

                // Send caption as a separate text message
                if let Some(cap) = caption {
                    if !cap.is_empty() {
                        self.api_send_message(&user.platform_id, &cap).await?;
                    }
                }
            }
            _ => {
                self.api_send_message(&user.platform_id, "(Unsupported content type)")
                    .await?;
            }
        }
        Ok(())
    }

    async fn send_typing(&self, user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        self.api_send_action(&user.platform_id, "typing_on").await
    }

    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.shutdown_tx.send(true);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_messenger_adapter_creation() {
        let adapter = MessengerAdapter::new(
            "page-token-123".to_string(),
            "verify-token-456".to_string(),
            8080,
            reqwest::Client::new(),
        );
        assert_eq!(adapter.name(), "messenger");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("messenger".to_string())
        );
        assert_eq!(adapter.webhook_port, 8080);
    }

    #[test]
    fn test_messenger_both_tokens() {
        let adapter = MessengerAdapter::new("page-tok".to_string(), "verify-tok".to_string(), 9000, reqwest::Client::new());
        assert_eq!(adapter.page_token.as_str(), "page-tok");
        assert_eq!(adapter.verify_token.as_str(), "verify-tok");
    }

    #[test]
    fn test_parse_messenger_entry_text_message() {
        let entry = serde_json::json!({
            "id": "page-id-123",
            "time": 1458692752478_u64,
            "messaging": [
                {
                    "sender": { "id": "user-123" },
                    "recipient": { "id": "page-456" },
                    "timestamp": 1458692752478_u64,
                    "message": {
                        "mid": "mid.123",
                        "text": "Hello from Messenger!"
                    }
                }
            ]
        });

        let msgs = parse_messenger_entry(&entry);
        assert_eq!(msgs.len(), 1);
        assert_eq!(
            msgs[0].channel,
            ChannelType::Custom("messenger".to_string())
        );
        assert_eq!(msgs[0].sender.platform_id, "user-123");
        assert!(
            matches!(msgs[0].content, ChannelContent::Text(ref t) if t == "Hello from Messenger!")
        );
    }

    #[test]
    fn test_parse_messenger_entry_command() {
        let entry = serde_json::json!({
            "id": "page-id",
            "messaging": [
                {
                    "sender": { "id": "user-1" },
                    "recipient": { "id": "page-1" },
                    "timestamp": 0,
                    "message": {
                        "mid": "mid.456",
                        "text": "/models list"
                    }
                }
            ]
        });

        let msgs = parse_messenger_entry(&entry);
        assert_eq!(msgs.len(), 1);
        match &msgs[0].content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "models");
                assert_eq!(args, &["list"]);
            }
            other => panic!("Expected Command, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_messenger_entry_skips_echo() {
        let entry = serde_json::json!({
            "id": "page-id",
            "messaging": [
                {
                    "sender": { "id": "page-1" },
                    "recipient": { "id": "user-1" },
                    "timestamp": 0,
                    "message": {
                        "mid": "mid.789",
                        "text": "Echo message",
                        "is_echo": true,
                        "app_id": 12345
                    }
                }
            ]
        });

        let msgs = parse_messenger_entry(&entry);
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_parse_messenger_entry_skips_delivery() {
        let entry = serde_json::json!({
            "id": "page-id",
            "messaging": [
                {
                    "sender": { "id": "user-1" },
                    "recipient": { "id": "page-1" },
                    "timestamp": 0,
                    "delivery": {
                        "mids": ["mid.123"],
                        "watermark": 1458668856253_u64
                    }
                }
            ]
        });

        let msgs = parse_messenger_entry(&entry);
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_parse_messenger_entry_quick_reply() {
        let entry = serde_json::json!({
            "id": "page-id",
            "messaging": [
                {
                    "sender": { "id": "user-1" },
                    "recipient": { "id": "page-1" },
                    "timestamp": 0,
                    "message": {
                        "mid": "mid.qr",
                        "text": "Red",
                        "quick_reply": {
                            "payload": "DEVELOPER_DEFINED_PAYLOAD_FOR_RED"
                        }
                    }
                }
            ]
        });

        let msgs = parse_messenger_entry(&entry);
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].metadata.contains_key("quick_reply_payload"));
    }

    #[test]
    fn test_parse_messenger_entry_empty_text() {
        let entry = serde_json::json!({
            "id": "page-id",
            "messaging": [
                {
                    "sender": { "id": "user-1" },
                    "recipient": { "id": "page-1" },
                    "timestamp": 0,
                    "message": {
                        "mid": "mid.empty",
                        "text": ""
                    }
                }
            ]
        });

        let msgs = parse_messenger_entry(&entry);
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_parse_messenger_entry_multiple_messages() {
        let entry = serde_json::json!({
            "id": "page-id",
            "messaging": [
                {
                    "sender": { "id": "user-1" },
                    "recipient": { "id": "page-1" },
                    "timestamp": 0,
                    "message": { "mid": "mid.1", "text": "First" }
                },
                {
                    "sender": { "id": "user-2" },
                    "recipient": { "id": "page-1" },
                    "timestamp": 0,
                    "message": { "mid": "mid.2", "text": "Second" }
                }
            ]
        });

        let msgs = parse_messenger_entry(&entry);
        assert_eq!(msgs.len(), 2);
    }
}
