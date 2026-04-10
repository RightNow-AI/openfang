//! Revolt API channel adapter.
//!
//! Uses the Revolt REST API for sending messages and WebSocket (Bonfire protocol)
//! for real-time message reception. Authentication uses the bot token via
//! `x-bot-token` header on REST calls and `Authenticate` frame on WebSocket.
//! Revolt is an open-source, Discord-like chat platform.

use crate::types::{
    split_message, ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser,
};
use async_trait::async_trait;
use chrono::Utc;
use futures::{SinkExt, Stream, StreamExt};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch, RwLock};
use tracing::{debug, info, warn};
use zeroize::Zeroizing;

/// Default Revolt API URL.
const DEFAULT_API_URL: &str = "https://api.revolt.chat";

/// Default Revolt WebSocket URL.
const DEFAULT_WS_URL: &str = "wss://ws.revolt.chat";

/// Maximum Revolt message text length (characters).
const MAX_MESSAGE_LEN: usize = 2000;

/// Maximum backoff duration for WebSocket reconnection.
const MAX_BACKOFF_SECS: u64 = 60;

/// WebSocket heartbeat interval (seconds). Revolt expects pings every 30s.
const HEARTBEAT_INTERVAL_SECS: u64 = 20;

/// Revolt API adapter using WebSocket (Bonfire) + REST.
///
/// Inbound messages are received via WebSocket connection to the Revolt
/// Bonfire gateway. Outbound messages are sent via the REST API.
/// The adapter handles automatic reconnection with exponential backoff.
pub struct RevoltAdapter {
    /// SECURITY: Bot token is zeroized on drop to prevent memory disclosure.
    bot_token: Zeroizing<String>,
    /// Revolt API URL (default: `"https://api.revolt.chat"`).
    api_url: String,
    /// Revolt WebSocket URL (default: "wss://ws.revolt.chat").
    ws_url: String,
    /// Restrict to specific channel IDs (empty = all channels the bot is in).
    allowed_channels: Vec<String>,
    /// HTTP client for outbound REST API calls.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Bot's own user ID (populated after authentication).
    bot_user_id: Arc<RwLock<Option<String>>>,
}

impl RevoltAdapter {
    /// Create a new Revolt adapter with default API and WebSocket URLs.
    ///
    /// # Arguments
    /// * `bot_token` - Revolt bot token for authentication.
    pub fn new(bot_token: String) -> Self {
        Self::with_urls(
            bot_token,
            DEFAULT_API_URL.to_string(),
            DEFAULT_WS_URL.to_string(),
        )
    }

    /// Create a new Revolt adapter with custom API and WebSocket URLs.
    pub fn with_urls(bot_token: String, api_url: String, ws_url: String) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let api_url = api_url.trim_end_matches('/').to_string();
        let ws_url = ws_url.trim_end_matches('/').to_string();
        Self {
            bot_token: Zeroizing::new(bot_token),
            api_url,
            ws_url,
            allowed_channels: Vec::new(),
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            bot_user_id: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a new Revolt adapter with channel restrictions.
    pub fn with_channels(bot_token: String, allowed_channels: Vec<String>) -> Self {
        let mut adapter = Self::new(bot_token);
        adapter.allowed_channels = allowed_channels;
        adapter
    }

    /// Set allowed channel IDs (empty = all channels the bot is in).
    pub fn set_allowed_channels(&mut self, channels: Vec<String>) {
        self.allowed_channels = channels;
    }

    /// Add the bot token header to a request builder.
    fn auth_header(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        builder.header("x-bot-token", self.bot_token.as_str())
    }

    /// Validate the bot token by fetching the bot's own user info.
    async fn validate(&self) -> Result<String, Box<dyn std::error::Error>> {
        let url = format!("{}/users/@me", self.api_url);
        let resp = self.auth_header(self.client.get(&url)).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Revolt authentication failed {status}: {body}").into());
        }

        let body: serde_json::Value = resp.json().await?;
        let user_id = body["_id"].as_str().unwrap_or("").to_string();
        let username = body["username"].as_str().unwrap_or("unknown").to_string();

        *self.bot_user_id.write().await = Some(user_id.clone());

        Ok(format!("{username} ({user_id})"))
    }

    /// Send a text message to a Revolt channel via REST API.
    async fn api_send_message(
        &self,
        channel_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/channels/{}/messages", self.api_url, channel_id);
        let chunks = split_message(text, MAX_MESSAGE_LEN);

        for chunk in chunks {
            let body = serde_json::json!({
                "content": chunk,
            });

            let resp = self
                .auth_header(self.client.post(&url))
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let resp_body = resp.text().await.unwrap_or_default();
                return Err(format!("Revolt send message error {status}: {resp_body}").into());
            }
        }

        Ok(())
    }

    /// Send a reply to a specific message in a Revolt channel.
    #[allow(dead_code)]
    async fn api_reply_message(
        &self,
        channel_id: &str,
        message_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/channels/{}/messages", self.api_url, channel_id);
        let chunks = split_message(text, MAX_MESSAGE_LEN);

        for (i, chunk) in chunks.iter().enumerate() {
            let mut body = serde_json::json!({
                "content": chunk,
            });

            // Only add reply reference to the first message
            if i == 0 {
                body["replies"] = serde_json::json!([{
                    "id": message_id,
                    "mention": false,
                }]);
            }

            let resp = self
                .auth_header(self.client.post(&url))
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let resp_body = resp.text().await.unwrap_or_default();
                warn!("Revolt reply error {status}: {resp_body}");
            }
        }

        Ok(())
    }

    /// Check if a channel is in the allowed list (empty = allow all).
    #[allow(dead_code)]
    fn is_allowed_channel(&self, channel_id: &str) -> bool {
        self.allowed_channels.is_empty() || self.allowed_channels.iter().any(|c| c == channel_id)
    }
}

/// Parse a Revolt WebSocket "Message" event into a `ChannelMessage`.
fn parse_revolt_message(
    data: &serde_json::Value,
    bot_user_id: &str,
    allowed_channels: &[String],
) -> Option<ChannelMessage> {
    let msg_type = data["type"].as_str().unwrap_or("");
    if msg_type != "Message" {
        return None;
    }

    let author = data["author"].as_str().unwrap_or("");
    // Skip own messages
    if author == bot_user_id {
        return None;
    }

    // Skip system messages (author = "00000000000000000000000000")
    if author.chars().all(|c| c == '0') {
        return None;
    }

    let channel_id = data["channel"].as_str().unwrap_or("").to_string();
    // Channel filter
    if !allowed_channels.is_empty() && !allowed_channels.iter().any(|c| c == &channel_id) {
        return None;
    }

    let content = data["content"].as_str().unwrap_or("");
    if content.is_empty() {
        return None;
    }

    let msg_id = data["_id"].as_str().unwrap_or("").to_string();
    let nonce = data["nonce"].as_str().unwrap_or("").to_string();

    let msg_content = if content.starts_with('/') {
        let parts: Vec<&str> = content.splitn(2, ' ').collect();
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
        ChannelContent::Text(content.to_string())
    };

    let mut metadata = HashMap::new();
    metadata.insert(
        "channel_id".to_string(),
        serde_json::Value::String(channel_id.clone()),
    );
    metadata.insert(
        "author_id".to_string(),
        serde_json::Value::String(author.to_string()),
    );
    if !nonce.is_empty() {
        metadata.insert("nonce".to_string(), serde_json::Value::String(nonce));
    }

    // Check for reply references
    if let Some(replies) = data.get("replies") {
        metadata.insert("replies".to_string(), replies.clone());
    }

    // Check for attachments
    if let Some(attachments) = data.get("attachments") {
        if let Some(arr) = attachments.as_array() {
            if !arr.is_empty() {
                metadata.insert("attachments".to_string(), attachments.clone());
            }
        }
    }

    Some(ChannelMessage {
        channel: ChannelType::Custom("revolt".to_string()),
        platform_message_id: msg_id,
        sender: ChannelUser {
            platform_id: channel_id,
            display_name: author.to_string(),
            openfang_user: None,
        },
        content: msg_content,
        target_agent: None,
        timestamp: Utc::now(),
        is_group: true, // Revolt channels are inherently group-based
        thread_id: None,
        metadata,
    })
}

#[async_trait]
impl ChannelAdapter for RevoltAdapter {
    fn name(&self) -> &str {
        "revolt"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("revolt".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        // Validate credentials
        let bot_info = self.validate().await?;
        info!("Revolt adapter authenticated as {bot_info}");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let ws_url = self.ws_url.clone();
        let bot_token = self.bot_token.clone();
        let bot_user_id = Arc::clone(&self.bot_user_id);
        let allowed_channels = self.allowed_channels.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let mut backoff = Duration::from_secs(1);

            loop {
                if *shutdown_rx.borrow() {
                    break;
                }

                let own_id = {
                    let guard = bot_user_id.read().await;
                    guard.clone().unwrap_or_default()
                };

                // Connect to WebSocket
                let ws_connect_url = format!("{}/?format=json", ws_url);

                let ws_stream = match tokio_tungstenite::connect_async(&ws_connect_url).await {
                    Ok((stream, _)) => {
                        info!("Revolt WebSocket connected");
                        backoff = Duration::from_secs(1);
                        stream
                    }
                    Err(e) => {
                        warn!("Revolt WebSocket connection failed: {e}");
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_secs(MAX_BACKOFF_SECS));
                        continue;
                    }
                };

                let (mut ws_sink, mut ws_stream_rx) = ws_stream.split();

                // Send Authenticate frame
                let auth_msg = serde_json::json!({
                    "type": "Authenticate",
                    "token": bot_token.as_str(),
                });

                if let Err(e) = ws_sink
                    .send(tokio_tungstenite::tungstenite::Message::Text(
                        auth_msg.to_string(),
                    ))
                    .await
                {
                    warn!("Revolt: failed to send auth frame: {e}");
                    continue;
                }

                let mut heartbeat_interval =
                    tokio::time::interval(Duration::from_secs(HEARTBEAT_INTERVAL_SECS));

                loop {
                    tokio::select! {
                        _ = shutdown_rx.changed() => {
                            info!("Revolt adapter shutting down");
                            let _ = ws_sink.close().await;
                            return;
                        }
                        _ = heartbeat_interval.tick() => {
                            // Send Ping to keep connection alive
                            let ping = serde_json::json!({
                                "type": "Ping",
                                "data": 0,
                            });
                            if let Err(e) = ws_sink
                                .send(tokio_tungstenite::tungstenite::Message::Text(
                                    ping.to_string(),
                                ))
                                .await
                            {
                                warn!("Revolt: heartbeat send failed: {e}");
                                break;
                            }
                        }
                        msg = ws_stream_rx.next() => {
                            match msg {
                                Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                                    let data: serde_json::Value = match serde_json::from_str(&text) {
                                        Ok(v) => v,
                                        Err(_) => continue,
                                    };

                                    let event_type = data["type"].as_str().unwrap_or("");

                                    match event_type {
                                        "Authenticated" => {
                                            info!("Revolt: successfully authenticated");
                                        }
                                        "Ready" => {
                                            info!("Revolt: ready, receiving events");
                                        }
                                        "Pong" => {
                                            debug!("Revolt: pong received");
                                        }
                                        "Message" => {
                                            if let Some(channel_msg) = parse_revolt_message(
                                                &data,
                                                &own_id,
                                                &allowed_channels,
                                            ) {
                                                if tx.send(channel_msg).await.is_err() {
                                                    return;
                                                }
                                            }
                                        }
                                        "Error" => {
                                            let error = data["error"].as_str().unwrap_or("unknown");
                                            warn!("Revolt WebSocket error: {error}");
                                            if error == "InvalidSession" || error == "NotAuthenticated" {
                                                break; // Reconnect
                                            }
                                        }
                                        _ => {
                                            // Ignore other event types (typing, presence, etc.)
                                        }
                                    }
                                }
                                Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) => {
                                    info!("Revolt WebSocket closed by server");
                                    break;
                                }
                                Some(Err(e)) => {
                                    warn!("Revolt WebSocket error: {e}");
                                    break;
                                }
                                None => {
                                    info!("Revolt WebSocket stream ended");
                                    break;
                                }
                                _ => {} // Binary, Ping, Pong frames
                            }
                        }
                    }
                }

                // Backoff before reconnection
                warn!(
                    "Revolt WebSocket disconnected, reconnecting in {}s",
                    backoff.as_secs()
                );
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(MAX_BACKOFF_SECS));
            }

            info!("Revolt WebSocket loop stopped");
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
                // Revolt supports embedding images in messages via markdown
                let markdown = if let Some(cap) = caption {
                    format!("![{}]({})", cap, url)
                } else {
                    format!("![image]({})", url)
                };
                self.api_send_message(&user.platform_id, &markdown).await?;
            }
            _ => {
                self.api_send_message(&user.platform_id, "(Unsupported content type)")
                    .await?;
            }
        }
        Ok(())
    }

    async fn send_typing(&self, user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        // Revolt typing indicator via REST
        let url = format!("{}/channels/{}/typing", self.api_url, user.platform_id);

        let _ = self.auth_header(self.client.post(&url)).send().await;

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
    fn test_revolt_adapter_creation() {
        let adapter = RevoltAdapter::new("bot-token-123".to_string());
        assert_eq!(adapter.name(), "revolt");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("revolt".to_string())
        );
    }

    #[test]
    fn test_revolt_default_urls() {
        let adapter = RevoltAdapter::new("tok".to_string());
        assert_eq!(adapter.api_url, "https://api.revolt.chat");
        assert_eq!(adapter.ws_url, "wss://ws.revolt.chat");
    }

    #[test]
    fn test_revolt_custom_urls() {
        let adapter = RevoltAdapter::with_urls(
            "tok".to_string(),
            "https://api.revolt.example.com/".to_string(),
            "wss://ws.revolt.example.com/".to_string(),
        );
        assert_eq!(adapter.api_url, "https://api.revolt.example.com");
        assert_eq!(adapter.ws_url, "wss://ws.revolt.example.com");
    }

    #[test]
    fn test_revolt_with_channels() {
        let adapter = RevoltAdapter::with_channels(
            "tok".to_string(),
            vec!["ch1".to_string(), "ch2".to_string()],
        );
        assert!(adapter.is_allowed_channel("ch1"));
        assert!(adapter.is_allowed_channel("ch2"));
        assert!(!adapter.is_allowed_channel("ch3"));
    }

    #[test]
    fn test_revolt_empty_channels_allows_all() {
        let adapter = RevoltAdapter::new("tok".to_string());
        assert!(adapter.is_allowed_channel("any-channel"));
    }

    #[test]
    fn test_revolt_auth_header() {
        let adapter = RevoltAdapter::new("my-revolt-token".to_string());
        let builder = adapter.client.get("https://example.com");
        let builder = adapter.auth_header(builder);
        let request = builder.build().unwrap();
        assert_eq!(
            request.headers().get("x-bot-token").unwrap(),
            "my-revolt-token"
        );
    }

    #[test]
    fn test_parse_revolt_message_basic() {
        let data = serde_json::json!({
            "type": "Message",
            "_id": "msg-123",
            "channel": "ch-456",
            "author": "user-789",
            "content": "Hello from Revolt!",
            "nonce": "nonce-abc"
        });

        let msg = parse_revolt_message(&data, "bot-id", &[]).unwrap();
        assert_eq!(msg.channel, ChannelType::Custom("revolt".to_string()));
        assert_eq!(msg.platform_message_id, "msg-123");
        assert_eq!(msg.sender.platform_id, "ch-456");
        assert!(msg.is_group);
        assert!(matches!(msg.content, ChannelContent::Text(ref t) if t == "Hello from Revolt!"));
    }

    #[test]
    fn test_parse_revolt_message_skips_bot() {
        let data = serde_json::json!({
            "type": "Message",
            "_id": "msg-1",
            "channel": "ch-1",
            "author": "bot-id",
            "content": "Bot message"
        });

        assert!(parse_revolt_message(&data, "bot-id", &[]).is_none());
    }

    #[test]
    fn test_parse_revolt_message_skips_system() {
        let data = serde_json::json!({
            "type": "Message",
            "_id": "msg-1",
            "channel": "ch-1",
            "author": "00000000000000000000000000",
            "content": "System message"
        });

        assert!(parse_revolt_message(&data, "bot-id", &[]).is_none());
    }

    #[test]
    fn test_parse_revolt_message_channel_filter() {
        let data = serde_json::json!({
            "type": "Message",
            "_id": "msg-1",
            "channel": "ch-not-allowed",
            "author": "user-1",
            "content": "Filtered out"
        });

        assert!(parse_revolt_message(&data, "bot-id", &["ch-allowed".to_string()]).is_none());

        // Same message but with allowed channel
        let data2 = serde_json::json!({
            "type": "Message",
            "_id": "msg-2",
            "channel": "ch-allowed",
            "author": "user-1",
            "content": "Allowed"
        });

        assert!(parse_revolt_message(&data2, "bot-id", &["ch-allowed".to_string()]).is_some());
    }

    #[test]
    fn test_parse_revolt_message_command() {
        let data = serde_json::json!({
            "type": "Message",
            "_id": "msg-cmd",
            "channel": "ch-1",
            "author": "user-1",
            "content": "/agent deploy-bot"
        });

        let msg = parse_revolt_message(&data, "bot-id", &[]).unwrap();
        match &msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "agent");
                assert_eq!(args, &["deploy-bot"]);
            }
            other => panic!("Expected Command, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_revolt_message_non_message_type() {
        let data = serde_json::json!({
            "type": "ChannelStartTyping",
            "id": "ch-1",
            "user": "user-1"
        });

        assert!(parse_revolt_message(&data, "bot-id", &[]).is_none());
    }

    #[test]
    fn test_parse_revolt_message_empty_content() {
        let data = serde_json::json!({
            "type": "Message",
            "_id": "msg-empty",
            "channel": "ch-1",
            "author": "user-1",
            "content": ""
        });

        assert!(parse_revolt_message(&data, "bot-id", &[]).is_none());
    }

    #[test]
    fn test_parse_revolt_message_metadata() {
        let data = serde_json::json!({
            "type": "Message",
            "_id": "msg-meta",
            "channel": "ch-1",
            "author": "user-1",
            "content": "With metadata",
            "nonce": "nonce-1",
            "replies": ["msg-replied-to"],
            "attachments": [{"_id": "att-1", "filename": "file.txt"}]
        });

        let msg = parse_revolt_message(&data, "bot-id", &[]).unwrap();
        assert!(msg.metadata.contains_key("channel_id"));
        assert!(msg.metadata.contains_key("author_id"));
        assert!(msg.metadata.contains_key("nonce"));
        assert!(msg.metadata.contains_key("replies"));
        assert!(msg.metadata.contains_key("attachments"));
    }
}
