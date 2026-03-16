//! Guilded Bot channel adapter.
//!
//! Connects to the Guilded Bot API via WebSocket for receiving real-time events
//! and uses the REST API for sending messages. Authentication is performed via
//! Bearer token. The WebSocket gateway at `wss://www.guilded.gg/websocket/v1`
//! delivers `ChatMessageCreated` events for incoming messages.

use crate::types::{
    split_message, ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser,
};
use async_trait::async_trait;
use chrono::Utc;
use futures::Stream;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};
use zeroize::Zeroizing;

/// Guilded REST API base URL.
const GUILDED_API_BASE: &str = "https://www.guilded.gg/api/v1";

/// Guilded WebSocket gateway URL.
const GUILDED_WS_URL: &str = "wss://www.guilded.gg/websocket/v1";

/// Maximum message length for Guilded messages.
const MAX_MESSAGE_LEN: usize = 4000;

/// Guilded Bot API channel adapter using WebSocket for events and REST for sending.
///
/// Connects to the Guilded WebSocket gateway for real-time message events and
/// sends replies via the REST API. Supports filtering by server (guild) IDs.
pub struct GuildedAdapter {
    /// SECURITY: Bot token is zeroized on drop.
    bot_token: Zeroizing<String>,
    /// Server (guild) IDs to listen on (empty = all servers the bot is in).
    server_ids: Vec<String>,
    /// HTTP client for REST API calls.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
}

impl GuildedAdapter {
    /// Create a new Guilded adapter.
    ///
    /// # Arguments
    /// * `bot_token` - Guilded bot authentication token.
    /// * `server_ids` - Server IDs to filter events for (empty = all).
    pub fn new(bot_token: String, server_ids: Vec<String>) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            bot_token: Zeroizing::new(bot_token),
            server_ids,
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
        }
    }

    /// Validate credentials by fetching the bot's own user info.
    async fn validate(&self) -> Result<String, Box<dyn std::error::Error>> {
        let url = format!("{}/users/@me", GUILDED_API_BASE);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(self.bot_token.as_str())
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err("Guilded authentication failed".into());
        }

        let body: serde_json::Value = resp.json().await?;
        let bot_id = body["user"]["id"].as_str().unwrap_or("unknown").to_string();
        Ok(bot_id)
    }

    /// Send a text message to a Guilded channel via REST API.
    async fn api_send_message(
        &self,
        channel_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/channels/{}/messages", GUILDED_API_BASE, channel_id);
        let chunks = split_message(text, MAX_MESSAGE_LEN);

        for chunk in chunks {
            let body = serde_json::json!({
                "content": chunk,
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
                return Err(format!("Guilded API error {status}: {resp_body}").into());
            }
        }

        Ok(())
    }

    /// Check if a server ID is in the allowed list.
    #[allow(dead_code)]
    fn is_allowed_server(&self, server_id: &str) -> bool {
        self.server_ids.is_empty() || self.server_ids.iter().any(|s| s == server_id)
    }
}

#[async_trait]
impl ChannelAdapter for GuildedAdapter {
    fn name(&self) -> &str {
        "guilded"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("guilded".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        // Validate credentials
        let bot_id = self.validate().await?;
        info!("Guilded adapter authenticated as bot {bot_id}");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let bot_token = self.bot_token.clone();
        let server_ids = self.server_ids.clone();
        let own_bot_id = bot_id;
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let mut backoff = Duration::from_secs(1);

            loop {
                if *shutdown_rx.borrow() {
                    break;
                }

                // Build WebSocket request with auth header
                let mut request =
                    match tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(GUILDED_WS_URL) {
                        Ok(r) => r,
                        Err(e) => {
                            warn!("Guilded: failed to build WS request: {e}");
                            return;
                        }
                    };

                request.headers_mut().insert(
                    "Authorization",
                    format!("Bearer {}", bot_token.as_str()).parse().unwrap(),
                );

                // Connect to WebSocket
                let ws_stream = match tokio_tungstenite::connect_async(request).await {
                    Ok((stream, _resp)) => stream,
                    Err(e) => {
                        warn!("Guilded: WebSocket connection failed: {e}, retrying in {backoff:?}");
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_secs(60));
                        continue;
                    }
                };

                info!("Guilded WebSocket connected");
                backoff = Duration::from_secs(1);

                use futures::StreamExt;
                let (mut _write, mut read) = ws_stream.split();

                // Read events from WebSocket
                let should_reconnect = loop {
                    let msg = tokio::select! {
                        _ = shutdown_rx.changed() => {
                            info!("Guilded adapter shutting down");
                            return;
                        }
                        msg = read.next() => msg,
                    };

                    let msg = match msg {
                        Some(Ok(m)) => m,
                        Some(Err(e)) => {
                            warn!("Guilded WS read error: {e}");
                            break true;
                        }
                        None => {
                            info!("Guilded WS stream ended");
                            break true;
                        }
                    };

                    // Only process text messages
                    let text = match msg {
                        tokio_tungstenite::tungstenite::Message::Text(t) => t,
                        tokio_tungstenite::tungstenite::Message::Ping(_) => continue,
                        tokio_tungstenite::tungstenite::Message::Close(_) => {
                            info!("Guilded WS received close frame");
                            break true;
                        }
                        _ => continue,
                    };

                    let event: serde_json::Value = match serde_json::from_str(&text) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    let event_type = event["t"].as_str().unwrap_or("");

                    // Handle welcome event (op 1) — contains heartbeat interval
                    let op = event["op"].as_i64().unwrap_or(0);
                    if op == 1 {
                        info!("Guilded: received welcome event");
                        continue;
                    }

                    // Only process ChatMessageCreated events
                    if event_type != "ChatMessageCreated" {
                        continue;
                    }

                    let message = &event["d"]["message"];
                    let msg_server_id = event["d"]["serverId"].as_str().unwrap_or("");

                    // Filter by server ID if configured
                    if !server_ids.is_empty() && !server_ids.iter().any(|s| s == msg_server_id) {
                        continue;
                    }

                    let created_by = message["createdBy"].as_str().unwrap_or("");
                    // Skip messages from the bot itself
                    if created_by == own_bot_id {
                        continue;
                    }

                    let content = message["content"].as_str().unwrap_or("");
                    if content.is_empty() {
                        continue;
                    }

                    let msg_id = message["id"].as_str().unwrap_or("").to_string();
                    let channel_id = message["channelId"].as_str().unwrap_or("").to_string();

                    let msg_content = if content.starts_with('/') {
                        let parts: Vec<&str> = content.splitn(2, ' ').collect();
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
                        ChannelContent::Text(content.to_string())
                    };

                    let channel_msg = ChannelMessage {
                        channel: ChannelType::Custom("guilded".to_string()),
                        platform_message_id: msg_id,
                        sender: ChannelUser {
                            platform_id: channel_id,
                            display_name: created_by.to_string(),
                            openfang_user: None,
                            metadata: None,
                        },
                        content: msg_content,
                        target_agent: None,
                        timestamp: Utc::now(),
                        is_group: true,
                        thread_id: None,
                        metadata: {
                            let mut m = HashMap::new();
                            m.insert(
                                "server_id".to_string(),
                                serde_json::Value::String(msg_server_id.to_string()),
                            );
                            m.insert(
                                "created_by".to_string(),
                                serde_json::Value::String(created_by.to_string()),
                            );
                            m
                        },
                    };

                    if tx.send(channel_msg).await.is_err() {
                        return;
                    }
                };

                if !should_reconnect || *shutdown_rx.borrow() {
                    break;
                }

                warn!("Guilded: reconnecting in {backoff:?}");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(60));
            }

            info!("Guilded WebSocket loop stopped");
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

    async fn send_typing(&self, _user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        // Guilded does not expose a public typing indicator API for bots
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
    fn test_guilded_adapter_creation() {
        let adapter =
            GuildedAdapter::new("test-bot-token".to_string(), vec!["server1".to_string()]);
        assert_eq!(adapter.name(), "guilded");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("guilded".to_string())
        );
    }

    #[test]
    fn test_guilded_allowed_servers() {
        let adapter = GuildedAdapter::new(
            "tok".to_string(),
            vec!["srv-1".to_string(), "srv-2".to_string()],
        );
        assert!(adapter.is_allowed_server("srv-1"));
        assert!(adapter.is_allowed_server("srv-2"));
        assert!(!adapter.is_allowed_server("srv-3"));

        let open = GuildedAdapter::new("tok".to_string(), vec![]);
        assert!(open.is_allowed_server("any-server"));
    }

    #[test]
    fn test_guilded_token_zeroized() {
        let adapter = GuildedAdapter::new("secret-bot-token".to_string(), vec![]);
        assert_eq!(adapter.bot_token.as_str(), "secret-bot-token");
    }

    #[test]
    fn test_guilded_constants() {
        assert_eq!(MAX_MESSAGE_LEN, 4000);
        assert_eq!(GUILDED_WS_URL, "wss://www.guilded.gg/websocket/v1");
    }
}
