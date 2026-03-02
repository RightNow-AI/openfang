//! Viber Bot API channel adapter.
//!
//! Uses the Viber REST API for sending messages and a webhook HTTP server for
//! receiving inbound events. Authentication is performed via the `X-Viber-Auth-Token`
//! header on all outbound API calls. The webhook is registered on startup via
//! `POST https://chatapi.viber.com/pa/set_webhook`.

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

/// Viber set webhook endpoint.
const VIBER_SET_WEBHOOK_URL: &str = "https://chatapi.viber.com/pa/set_webhook";

/// Viber send message endpoint.
const VIBER_SEND_MESSAGE_URL: &str = "https://chatapi.viber.com/pa/send_message";

/// Viber get account info endpoint (used for validation).
const VIBER_ACCOUNT_INFO_URL: &str = "https://chatapi.viber.com/pa/get_account_info";

/// Maximum Viber message text length (characters).
const MAX_MESSAGE_LEN: usize = 7000;

/// Sender name shown in Viber messages from the bot.
const DEFAULT_SENDER_NAME: &str = "OpenFang";

/// Viber Bot API adapter.
///
/// Inbound messages arrive via a webhook HTTP server that Viber pushes events to.
/// Outbound messages are sent via the Viber send_message REST API with the
/// `X-Viber-Auth-Token` header for authentication.
pub struct ViberAdapter {
    /// SECURITY: Auth token is zeroized on drop to prevent memory disclosure.
    auth_token: Zeroizing<String>,
    /// Public webhook URL that Viber will POST events to.
    webhook_url: String,
    /// Port on which the inbound webhook HTTP server listens.
    webhook_port: u16,
    /// Sender name displayed in outbound messages.
    sender_name: String,
    /// Optional sender avatar URL for outbound messages.
    sender_avatar: Option<String>,
    /// HTTP client for outbound API calls.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
}

impl ViberAdapter {
    /// Create a new Viber adapter.
    ///
    /// # Arguments
    /// * `auth_token` - Viber bot authentication token.
    /// * `webhook_url` - Public URL where Viber will send webhook events.
    /// * `webhook_port` - Local port for the inbound webhook HTTP server.
    pub fn new(auth_token: String, webhook_url: String, webhook_port: u16, client: reqwest::Client) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let webhook_url = webhook_url.trim_end_matches('/').to_string();
        Self {
            auth_token: Zeroizing::new(auth_token),
            webhook_url,
            webhook_port,
            sender_name: DEFAULT_SENDER_NAME.to_string(),
            sender_avatar: None,
            client,
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
        }
    }

    /// Create a new Viber adapter with a custom sender name and avatar.
    pub fn with_sender(
        auth_token: String,
        webhook_url: String,
        webhook_port: u16,
        sender_name: String,
        sender_avatar: Option<String>,
        client: reqwest::Client,
    ) -> Self {
        let mut adapter = Self::new(auth_token, webhook_url, webhook_port, client);
        adapter.sender_name = sender_name;
        adapter.sender_avatar = sender_avatar;
        adapter
    }

    /// Add the Viber auth token header to a request builder.
    fn auth_header(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        builder.header("X-Viber-Auth-Token", self.auth_token.as_str())
    }

    /// Validate the auth token by calling the get_account_info endpoint.
    async fn validate(&self) -> Result<String, Box<dyn std::error::Error>> {
        let resp = self
            .auth_header(self.client.post(VIBER_ACCOUNT_INFO_URL))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Viber authentication failed {status}: {body}").into());
        }

        let body: serde_json::Value = resp.json().await?;
        let status = body["status"].as_u64().unwrap_or(1);
        if status != 0 {
            let msg = body["status_message"].as_str().unwrap_or("unknown error");
            return Err(format!("Viber API error: {msg}").into());
        }

        let name = body["name"].as_str().unwrap_or("Viber Bot").to_string();
        Ok(name)
    }

    /// Register the webhook URL with Viber.
    async fn register_webhook(&self) -> Result<(), Box<dyn std::error::Error>> {
        let body = serde_json::json!({
            "url": self.webhook_url,
            "event_types": [
                "delivered",
                "seen",
                "failed",
                "subscribed",
                "unsubscribed",
                "conversation_started",
                "message"
            ],
            "send_name": true,
            "send_photo": true,
        });

        let resp = self
            .auth_header(self.client.post(VIBER_SET_WEBHOOK_URL))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let resp_body = resp.text().await.unwrap_or_default();
            return Err(format!("Viber set_webhook failed {status}: {resp_body}").into());
        }

        let resp_body: serde_json::Value = resp.json().await?;
        let status = resp_body["status"].as_u64().unwrap_or(1);
        if status != 0 {
            let msg = resp_body["status_message"]
                .as_str()
                .unwrap_or("unknown error");
            return Err(format!("Viber set_webhook error: {msg}").into());
        }

        info!("Viber webhook registered at {}", self.webhook_url);
        Ok(())
    }

    /// Send a text message to a Viber user.
    async fn api_send_message(
        &self,
        receiver: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let chunks = split_message(text, MAX_MESSAGE_LEN);

        for chunk in chunks {
            let mut sender = serde_json::json!({
                "name": self.sender_name,
            });
            if let Some(ref avatar) = self.sender_avatar {
                sender["avatar"] = serde_json::Value::String(avatar.clone());
            }

            let body = serde_json::json!({
                "receiver": receiver,
                "min_api_version": 1,
                "sender": sender,
                "tracking_data": "openfang",
                "type": "text",
                "text": chunk,
            });

            let resp = self
                .auth_header(self.client.post(VIBER_SEND_MESSAGE_URL))
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let resp_body = resp.text().await.unwrap_or_default();
                return Err(format!("Viber send_message error {status}: {resp_body}").into());
            }

            let resp_body: serde_json::Value = resp.json().await?;
            let api_status = resp_body["status"].as_u64().unwrap_or(1);
            if api_status != 0 {
                let msg = resp_body["status_message"]
                    .as_str()
                    .unwrap_or("unknown error");
                warn!("Viber send_message API error: {msg}");
            }
        }

        Ok(())
    }
}

/// Parse a Viber webhook event into a `ChannelMessage`.
///
/// Handles `message` events with text type. Returns `None` for non-message
/// events (delivered, seen, subscribed, conversation_started, etc.).
fn parse_viber_event(event: &serde_json::Value) -> Option<ChannelMessage> {
    let event_type = event["event"].as_str().unwrap_or("");
    if event_type != "message" {
        return None;
    }

    let message = event.get("message")?;
    let msg_type = message["type"].as_str().unwrap_or("");

    // Only handle text messages
    if msg_type != "text" {
        return None;
    }

    let text = message["text"].as_str().unwrap_or("");
    if text.is_empty() {
        return None;
    }

    let sender = event.get("sender")?;
    let sender_id = sender["id"].as_str().unwrap_or("").to_string();
    let sender_name = sender["name"].as_str().unwrap_or("Unknown").to_string();
    let sender_avatar = sender["avatar"].as_str().unwrap_or("").to_string();

    let message_token = event["message_token"]
        .as_u64()
        .map(|t| t.to_string())
        .unwrap_or_default();

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
    if !sender_avatar.is_empty() {
        metadata.insert(
            "sender_avatar".to_string(),
            serde_json::Value::String(sender_avatar),
        );
    }
    if let Some(tracking) = message["tracking_data"].as_str() {
        metadata.insert(
            "tracking_data".to_string(),
            serde_json::Value::String(tracking.to_string()),
        );
    }

    Some(ChannelMessage {
        channel: ChannelType::Custom("viber".to_string()),
        platform_message_id: message_token,
        sender: ChannelUser {
            platform_id: sender_id,
            display_name: sender_name,
            openfang_user: None,
        },
        content,
        target_agent: None,
        timestamp: Utc::now(),
        is_group: false, // Viber bot API messages are always 1:1
        thread_id: None,
        metadata,
    })
}

#[async_trait]
impl ChannelAdapter for ViberAdapter {
    fn name(&self) -> &str {
        "viber"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("viber".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        // Validate credentials
        let bot_name = self.validate().await?;
        info!("Viber adapter authenticated as {bot_name}");

        // Register webhook
        self.register_webhook().await?;

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let port = self.webhook_port;
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let tx = Arc::new(tx);

            let app = axum::Router::new().route(
                "/viber/webhook",
                axum::routing::post({
                    let tx = Arc::clone(&tx);
                    move |body: axum::extract::Json<serde_json::Value>| {
                        let tx = Arc::clone(&tx);
                        async move {
                            if let Some(msg) = parse_viber_event(&body.0) {
                                let _ = tx.send(msg).await;
                            }
                            axum::http::StatusCode::OK
                        }
                    }
                }),
            );

            let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
            info!("Viber webhook server listening on {addr}");

            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    warn!("Viber webhook bind failed: {e}");
                    return;
                }
            };

            let server = axum::serve(listener, app);

            tokio::select! {
                result = server => {
                    if let Err(e) = result {
                        warn!("Viber webhook server error: {e}");
                    }
                }
                _ = shutdown_rx.changed() => {
                    info!("Viber adapter shutting down");
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
                let mut sender = serde_json::json!({
                    "name": self.sender_name,
                });
                if let Some(ref avatar) = self.sender_avatar {
                    sender["avatar"] = serde_json::Value::String(avatar.clone());
                }

                let body = serde_json::json!({
                    "receiver": user.platform_id,
                    "min_api_version": 1,
                    "sender": sender,
                    "type": "picture",
                    "text": caption.unwrap_or_default(),
                    "media": url,
                });

                let resp = self
                    .auth_header(self.client.post(VIBER_SEND_MESSAGE_URL))
                    .json(&body)
                    .send()
                    .await?;

                if !resp.status().is_success() {
                    let status = resp.status();
                    let resp_body = resp.text().await.unwrap_or_default();
                    warn!("Viber image send error {status}: {resp_body}");
                }
            }
            _ => {
                self.api_send_message(&user.platform_id, "(Unsupported content type)")
                    .await?;
            }
        }
        Ok(())
    }

    async fn send_typing(&self, _user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        // Viber does not support typing indicators via REST API
        Ok(())
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
    fn test_viber_adapter_creation() {
        let adapter = ViberAdapter::new(
            "auth-token-123".to_string(),
            "https://example.com/viber/webhook".to_string(),
            8443,
            reqwest::Client::new(),
        );
        assert_eq!(adapter.name(), "viber");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("viber".to_string())
        );
        assert_eq!(adapter.webhook_port, 8443);
    }

    #[test]
    fn test_viber_url_normalization() {
        let adapter = ViberAdapter::new(
            "tok".to_string(),
            "https://example.com/viber/webhook/".to_string(),
            8443,
            reqwest::Client::new(),
        );
        assert_eq!(adapter.webhook_url, "https://example.com/viber/webhook");
    }

    #[test]
    fn test_viber_with_sender() {
        let adapter = ViberAdapter::with_sender(
            "tok".to_string(),
            "https://example.com".to_string(),
            8443,
            "MyBot".to_string(),
            Some("https://example.com/avatar.png".to_string()),
            reqwest::Client::new(),
        );
        assert_eq!(adapter.sender_name, "MyBot");
        assert_eq!(
            adapter.sender_avatar,
            Some("https://example.com/avatar.png".to_string())
        );
    }

    #[test]
    fn test_viber_auth_header() {
        let adapter = ViberAdapter::new(
            "my-viber-token".to_string(),
            "https://example.com".to_string(),
            8443,
            reqwest::Client::new(),
        );
        let builder = adapter.client.post("https://example.com");
        let builder = adapter.auth_header(builder);
        let request = builder.build().unwrap();
        assert_eq!(
            request.headers().get("X-Viber-Auth-Token").unwrap(),
            "my-viber-token"
        );
    }

    #[test]
    fn test_parse_viber_event_text_message() {
        let event = serde_json::json!({
            "event": "message",
            "timestamp": 1457764197627_u64,
            "message_token": 4912661846655238145_u64,
            "sender": {
                "id": "01234567890A=",
                "name": "Alice",
                "avatar": "https://example.com/avatar.jpg"
            },
            "message": {
                "type": "text",
                "text": "Hello from Viber!"
            }
        });

        let msg = parse_viber_event(&event).unwrap();
        assert_eq!(msg.channel, ChannelType::Custom("viber".to_string()));
        assert_eq!(msg.sender.display_name, "Alice");
        assert_eq!(msg.sender.platform_id, "01234567890A=");
        assert!(!msg.is_group);
        assert!(matches!(msg.content, ChannelContent::Text(ref t) if t == "Hello from Viber!"));
    }

    #[test]
    fn test_parse_viber_event_command() {
        let event = serde_json::json!({
            "event": "message",
            "message_token": 123_u64,
            "sender": {
                "id": "sender-1",
                "name": "Bob"
            },
            "message": {
                "type": "text",
                "text": "/help agents"
            }
        });

        let msg = parse_viber_event(&event).unwrap();
        match &msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "help");
                assert_eq!(args, &["agents"]);
            }
            other => panic!("Expected Command, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_viber_event_non_message() {
        let event = serde_json::json!({
            "event": "delivered",
            "timestamp": 1457764197627_u64,
            "message_token": 123_u64,
            "user_id": "user-1"
        });

        assert!(parse_viber_event(&event).is_none());
    }

    #[test]
    fn test_parse_viber_event_non_text() {
        let event = serde_json::json!({
            "event": "message",
            "message_token": 123_u64,
            "sender": {
                "id": "sender-1",
                "name": "Bob"
            },
            "message": {
                "type": "picture",
                "media": "https://example.com/image.jpg"
            }
        });

        assert!(parse_viber_event(&event).is_none());
    }

    #[test]
    fn test_parse_viber_event_empty_text() {
        let event = serde_json::json!({
            "event": "message",
            "message_token": 123_u64,
            "sender": {
                "id": "sender-1",
                "name": "Bob"
            },
            "message": {
                "type": "text",
                "text": ""
            }
        });

        assert!(parse_viber_event(&event).is_none());
    }
}
