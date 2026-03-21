//! Pumble Bot channel adapter.
//!
//! Uses the Pumble Bot API with a local webhook HTTP server for receiving
//! inbound event subscriptions and the REST API for sending messages.
//! Authentication is performed via a Bot Bearer token. Inbound events arrive
//! as JSON POST requests to the configured webhook port.

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

/// Pumble REST API base URL.
const PUMBLE_API_BASE: &str = "https://api.pumble.com/v1";

/// Maximum message length for Pumble messages.
const MAX_MESSAGE_LEN: usize = 4000;

/// Pumble Bot channel adapter using webhook for receiving and REST API for sending.
///
/// Listens for inbound events via a configurable HTTP webhook server and sends
/// outbound messages via the Pumble REST API. Supports Pumble's event subscription
/// model including URL verification challenges.
pub struct PumbleAdapter {
    /// SECURITY: Bot token is zeroized on drop.
    bot_token: Zeroizing<String>,
    /// Port for the inbound webhook HTTP listener.
    webhook_port: u16,
    /// HTTP client for outbound API calls.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
}

impl PumbleAdapter {
    /// Create a new Pumble adapter.
    ///
    /// # Arguments
    /// * `bot_token` - Pumble Bot access token.
    /// * `webhook_port` - Local port to bind the webhook listener on.
    pub fn new(bot_token: String, webhook_port: u16) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            bot_token: Zeroizing::new(bot_token),
            webhook_port,
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
        }
    }

    /// Validate credentials by fetching bot info from the Pumble API.
    async fn validate(&self) -> Result<String, Box<dyn std::error::Error>> {
        let url = format!("{}/auth.test", PUMBLE_API_BASE);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(self.bot_token.as_str())
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err("Pumble authentication failed".into());
        }

        let body: serde_json::Value = resp.json().await?;
        let bot_id = body["user_id"]
            .as_str()
            .or_else(|| body["bot_id"].as_str())
            .unwrap_or("unknown")
            .to_string();
        Ok(bot_id)
    }

    /// Send a text message to a Pumble channel.
    async fn api_send_message(
        &self,
        channel_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/messages", PUMBLE_API_BASE);
        let chunks = split_message(text, MAX_MESSAGE_LEN);

        for chunk in chunks {
            let body = serde_json::json!({
                "channel": channel_id,
                "text": chunk,
            });

            let resp = self
                .client
                .post(&url)
                .bearer_auth(self.bot_token.as_str())
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let resp_body = resp.text().await.unwrap_or_default();
                return Err(format!("Pumble API error {status}: {resp_body}").into());
            }
        }

        Ok(())
    }
}

/// Parse an inbound Pumble event JSON into a `ChannelMessage`.
///
/// Returns `None` for non-message events, URL verification challenges,
/// or messages from the bot itself.
fn parse_pumble_event(event: &serde_json::Value, own_bot_id: &str) -> Option<ChannelMessage> {
    let event_type = event["type"].as_str().unwrap_or("");

    // Handle URL verification challenge
    if event_type == "url_verification" {
        return None;
    }

    // Only process message events
    if event_type != "message" && event_type != "message.new" {
        return None;
    }

    let text = event["text"]
        .as_str()
        .or_else(|| event["message"]["text"].as_str())
        .unwrap_or("");
    if text.is_empty() {
        return None;
    }

    let user_id = event["user"]
        .as_str()
        .or_else(|| event["user_id"].as_str())
        .unwrap_or("");

    // Skip messages from the bot itself
    if user_id == own_bot_id {
        return None;
    }

    let channel_id = event["channel"]
        .as_str()
        .or_else(|| event["channel_id"].as_str())
        .unwrap_or("")
        .to_string();
    let ts = event["ts"]
        .as_str()
        .or_else(|| event["timestamp"].as_str())
        .unwrap_or("")
        .to_string();
    let thread_ts = event["thread_ts"].as_str().map(String::from);
    let user_name = event["user_name"].as_str().unwrap_or("unknown");
    let channel_type = event["channel_type"].as_str().unwrap_or("channel");
    let is_group = channel_type != "im";

    let content = if text.starts_with('/') {
        let parts: Vec<&str> = text.splitn(2, ' ').collect();
        let cmd = parts[0].trim_start_matches('/');
        let args: Vec<String> = parts
            .get(1)
            .map(|a| a.split_whitespace().map(String::from).collect())
            .unwrap_or_default();
        ChannelContent::Command {
            name: cmd.to_string(),
            args,
        }
    } else {
        ChannelContent::Text(text.to_string())
    };

    let mut metadata = HashMap::new();
    metadata.insert(
        "user_id".to_string(),
        serde_json::Value::String(user_id.to_string()),
    );
    if !ts.is_empty() {
        metadata.insert("ts".to_string(), serde_json::Value::String(ts.clone()));
    }

    Some(ChannelMessage {
        channel: ChannelType::Custom("pumble".to_string()),
        platform_message_id: ts,
        sender: ChannelUser {
            platform_id: channel_id,
            display_name: user_name.to_string(),
            openfang_user: None,
            reply_url: None,
        },
        content,
        target_agent: None,
        timestamp: Utc::now(),
        is_group,
        thread_id: thread_ts,
        metadata,
    })
}

#[async_trait]
impl ChannelAdapter for PumbleAdapter {
    fn name(&self) -> &str {
        "pumble"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("pumble".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        // Validate credentials
        let bot_id = self.validate().await?;
        info!("Pumble adapter authenticated (bot_id: {bot_id})");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let port = self.webhook_port;
        let own_bot_id = bot_id;
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            // Build the axum webhook router
            let bot_id_shared = Arc::new(own_bot_id);
            let tx_shared = Arc::new(tx);

            let app = axum::Router::new().route(
                "/pumble/events",
                axum::routing::post({
                    let bot_id = Arc::clone(&bot_id_shared);
                    let tx = Arc::clone(&tx_shared);
                    move |body: axum::extract::Json<serde_json::Value>| {
                        let bot_id = Arc::clone(&bot_id);
                        let tx = Arc::clone(&tx);
                        async move {
                            // Handle URL verification challenge
                            if body["type"].as_str() == Some("url_verification") {
                                let challenge =
                                    body["challenge"].as_str().unwrap_or("").to_string();
                                return (
                                    axum::http::StatusCode::OK,
                                    axum::Json(serde_json::json!({ "challenge": challenge })),
                                );
                            }

                            if let Some(msg) = parse_pumble_event(&body, &bot_id) {
                                let _ = tx.send(msg).await;
                            }

                            (
                                axum::http::StatusCode::OK,
                                axum::Json(serde_json::json!({})),
                            )
                        }
                    }
                }),
            );

            let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
            info!("Pumble webhook server listening on {addr}");

            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    warn!("Pumble webhook bind failed: {e}");
                    return;
                }
            };

            let server = axum::serve(listener, app);

            tokio::select! {
                result = server => {
                    if let Err(e) = result {
                        warn!("Pumble webhook server error: {e}");
                    }
                }
                _ = shutdown_rx.changed() => {
                    info!("Pumble adapter shutting down");
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
            _ => {
                self.api_send_message(&user.platform_id, "(Unsupported content type)")
                    .await?;
            }
        }
        Ok(())
    }

    async fn send_in_thread(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
        thread_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let text = match content {
            ChannelContent::Text(text) => text,
            _ => "(Unsupported content type)".to_string(),
        };

        let url = format!("{}/messages", PUMBLE_API_BASE);
        let chunks = split_message(&text, MAX_MESSAGE_LEN);

        for chunk in chunks {
            let body = serde_json::json!({
                "channel": user.platform_id,
                "text": chunk,
                "thread_ts": thread_id,
            });

            let resp = self
                .client
                .post(&url)
                .bearer_auth(self.bot_token.as_str())
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let resp_body = resp.text().await.unwrap_or_default();
                return Err(format!("Pumble thread reply error {status}: {resp_body}").into());
            }
        }

        Ok(())
    }

    async fn send_typing(&self, _user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        // Pumble does not expose a public typing indicator API for bots
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
    fn test_pumble_adapter_creation() {
        let adapter = PumbleAdapter::new("test-bot-token".to_string(), 8080);
        assert_eq!(adapter.name(), "pumble");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("pumble".to_string())
        );
    }

    #[test]
    fn test_pumble_token_zeroized() {
        let adapter = PumbleAdapter::new("secret-pumble-token".to_string(), 8080);
        assert_eq!(adapter.bot_token.as_str(), "secret-pumble-token");
    }

    #[test]
    fn test_pumble_webhook_port() {
        let adapter = PumbleAdapter::new("token".to_string(), 9999);
        assert_eq!(adapter.webhook_port, 9999);
    }

    #[test]
    fn test_parse_pumble_event_message() {
        let event = serde_json::json!({
            "type": "message",
            "text": "Hello from Pumble!",
            "user": "U12345",
            "channel": "C67890",
            "ts": "1234567890.123456",
            "user_name": "alice",
            "channel_type": "channel"
        });

        let msg = parse_pumble_event(&event, "BOT001").unwrap();
        assert_eq!(msg.sender.display_name, "alice");
        assert_eq!(msg.sender.platform_id, "C67890");
        assert!(msg.is_group);
        assert!(matches!(msg.content, ChannelContent::Text(ref t) if t == "Hello from Pumble!"));
    }

    #[test]
    fn test_parse_pumble_event_command() {
        let event = serde_json::json!({
            "type": "message",
            "text": "/help agents",
            "user": "U12345",
            "channel": "C67890",
            "ts": "ts1",
            "user_name": "bob"
        });

        let msg = parse_pumble_event(&event, "BOT001").unwrap();
        match &msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "help");
                assert_eq!(args, &["agents"]);
            }
            other => panic!("Expected Command, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_pumble_event_skip_bot() {
        let event = serde_json::json!({
            "type": "message",
            "text": "Bot message",
            "user": "BOT001",
            "channel": "C67890",
            "ts": "ts1"
        });

        let msg = parse_pumble_event(&event, "BOT001");
        assert!(msg.is_none());
    }

    #[test]
    fn test_parse_pumble_event_url_verification() {
        let event = serde_json::json!({
            "type": "url_verification",
            "challenge": "abc123"
        });

        let msg = parse_pumble_event(&event, "BOT001");
        assert!(msg.is_none());
    }

    #[test]
    fn test_parse_pumble_event_dm() {
        let event = serde_json::json!({
            "type": "message",
            "text": "Direct message",
            "user": "U12345",
            "channel": "D11111",
            "ts": "ts2",
            "user_name": "carol",
            "channel_type": "im"
        });

        let msg = parse_pumble_event(&event, "BOT001").unwrap();
        assert!(!msg.is_group);
    }

    #[test]
    fn test_parse_pumble_event_with_thread() {
        let event = serde_json::json!({
            "type": "message",
            "text": "Thread reply",
            "user": "U12345",
            "channel": "C67890",
            "ts": "ts3",
            "thread_ts": "ts1",
            "user_name": "dave"
        });

        let msg = parse_pumble_event(&event, "BOT001").unwrap();
        assert_eq!(msg.thread_id.as_deref(), Some("ts1"));
    }
}
