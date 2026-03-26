//! Rocket.Chat channel adapter.
//!
//! Uses the Rocket.Chat REST API for sending messages and long-polling
//! `channels.history` for receiving new messages. Authentication is performed
//! via personal access token with `X-Auth-Token` and `X-User-Id` headers.

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
use tokio::sync::{mpsc, watch, RwLock};
use tracing::{info, warn};
use zeroize::Zeroizing;

const POLL_INTERVAL_SECS: u64 = 2;
const MAX_MESSAGE_LEN: usize = 4096;

/// Rocket.Chat channel adapter using REST API with long-polling.
pub struct RocketChatAdapter {
    /// Unique identifier for this adapter instance.
    id: String,
    /// Rocket.Chat server URL (e.g., `"https://chat.example.com"`).
    server_url: String,
    /// SECURITY: Auth token is zeroized on drop.
    token: Zeroizing<String>,
    /// User ID for API authentication.
    user_id: String,
    /// Channel IDs (room IDs) to poll (empty = all).
    allowed_channels: Vec<String>,
    /// HTTP client.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Last polled timestamp per channel for incremental history fetch.
    last_timestamps: Arc<RwLock<HashMap<String, String>>>,
}

impl RocketChatAdapter {
    /// Create a new Rocket.Chat adapter.
    ///
    /// # Arguments
    /// * `id` - Unique identifier for this adapter instance.
    /// * `server_url` - Base URL of the Rocket.Chat instance.
    /// * `token` - Personal access token for authentication.
    /// * `user_id` - User ID associated with the token.
    /// * `allowed_channels` - Room IDs to listen on (empty = discover from server).
    pub fn new(
        id: String,
        server_url: String,
        token: String,
        user_id: String,
        allowed_channels: Vec<String>,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let server_url = server_url.trim_end_matches('/').to_string();
        Self {
            id,
            server_url,
            token: Zeroizing::new(token),
            user_id,
            allowed_channels,
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            last_timestamps: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add auth headers to a request builder.
    fn auth_headers(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        builder
            .header("X-Auth-Token", self.token.as_str())
            .header("X-User-Id", &self.user_id)
    }

    /// Validate credentials by calling `/api/v1/me`.
    async fn validate(&self) -> Result<String, Box<dyn std::error::Error>> {
        let url = format!("{}/api/v1/me", self.server_url);
        let resp = self.auth_headers(self.client.get(&url)).send().await?;

        if !resp.status().is_success() {
            return Err("Rocket.Chat authentication failed".into());
        }

        let body: serde_json::Value = resp.json().await?;
        let username = body["username"].as_str().unwrap_or("unknown").to_string();
        Ok(username)
    }

    /// Send a text message to a Rocket.Chat room.
    async fn api_send_message(
        &self,
        room_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/api/v1/chat.sendMessage", self.server_url);
        let chunks = split_message(text, MAX_MESSAGE_LEN);

        for chunk in chunks {
            let body = serde_json::json!({
                "message": {
                    "rid": room_id,
                    "msg": chunk,
                }
            });

            let resp = self
                .auth_headers(self.client.post(&url))
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(format!("Rocket.Chat API error {status}: {body}").into());
            }
        }

        Ok(())
    }

    /// Check if a channel is in the allowed list.
    #[allow(dead_code)]
    fn is_allowed_channel(&self, channel_id: &str) -> bool {
        self.allowed_channels.is_empty() || self.allowed_channels.iter().any(|c| c == channel_id)
    }
}

#[async_trait]
impl ChannelAdapter for RocketChatAdapter {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        "rocketchat"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("rocketchat".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        // Validate credentials
        let username = self.validate().await?;
        info!("Rocket.Chat adapter authenticated as {username}");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let server_url = self.server_url.clone();
        let token = self.token.clone();
        let user_id = self.user_id.clone();
        let own_username = username;
        let allowed_channels = self.allowed_channels.clone();
        let client = self.client.clone();
        let last_timestamps = Arc::clone(&self.last_timestamps);
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            // Determine channels to poll
            let channels_to_poll = if allowed_channels.is_empty() {
                // Fetch joined channels
                let url = format!("{server_url}/api/v1/channels.list.joined?count=100");
                match client
                    .get(&url)
                    .header("X-Auth-Token", token.as_str())
                    .header("X-User-Id", &user_id)
                    .send()
                    .await
                {
                    Ok(resp) => {
                        let body: serde_json::Value = resp.json().await.unwrap_or_default();
                        body["channels"]
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|c| c["_id"].as_str().map(String::from))
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default()
                    }
                    Err(e) => {
                        warn!("Rocket.Chat: failed to list channels: {e}");
                        return;
                    }
                }
            } else {
                allowed_channels
            };

            if channels_to_poll.is_empty() {
                warn!("Rocket.Chat: no channels to poll");
                return;
            }

            info!("Rocket.Chat: polling {} channel(s)", channels_to_poll.len());

            // Initialize timestamps to "now" so we only get new messages
            {
                let now = Utc::now().to_rfc3339();
                let mut ts = last_timestamps.write().await;
                for ch in &channels_to_poll {
                    ts.entry(ch.clone()).or_insert_with(|| now.clone());
                }
            }

            let poll_interval = Duration::from_secs(POLL_INTERVAL_SECS);

            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        info!("Rocket.Chat adapter shutting down");
                        break;
                    }
                    _ = tokio::time::sleep(poll_interval) => {}
                }

                if *shutdown_rx.borrow() {
                    break;
                }

                for channel_id in &channels_to_poll {
                    let oldest = {
                        let ts = last_timestamps.read().await;
                        ts.get(channel_id).cloned().unwrap_or_default()
                    };

                    let url = format!(
                        "{}/api/v1/channels.history?roomId={}&oldest={}&count=50",
                        server_url, channel_id, oldest
                    );

                    let resp = match client
                        .get(&url)
                        .header("X-Auth-Token", token.as_str())
                        .header("X-User-Id", &user_id)
                        .send()
                        .await
                    {
                        Ok(r) => r,
                        Err(e) => {
                            warn!("Rocket.Chat: history fetch error for {channel_id}: {e}");
                            continue;
                        }
                    };

                    if !resp.status().is_success() {
                        warn!(
                            "Rocket.Chat: history fetch returned {} for {channel_id}",
                            resp.status()
                        );
                        continue;
                    }

                    let body: serde_json::Value = match resp.json().await {
                        Ok(b) => b,
                        Err(e) => {
                            warn!("Rocket.Chat: failed to parse history: {e}");
                            continue;
                        }
                    };

                    let messages = match body["messages"].as_array() {
                        Some(arr) => arr,
                        None => continue,
                    };

                    let mut newest_ts = oldest.clone();

                    for msg in messages {
                        let sender_username = msg["u"]["username"].as_str().unwrap_or("");
                        // Skip own messages
                        if sender_username == own_username {
                            continue;
                        }

                        let text = msg["msg"].as_str().unwrap_or("");
                        if text.is_empty() {
                            continue;
                        }

                        let msg_id = msg["_id"].as_str().unwrap_or("").to_string();
                        let msg_ts = msg["ts"].as_str().unwrap_or("").to_string();
                        let sender_id = msg["u"]["_id"].as_str().unwrap_or("").to_string();
                        let thread_id = msg["tmid"].as_str().map(String::from);

                        // Track newest timestamp
                        if msg_ts > newest_ts {
                            newest_ts = msg_ts;
                        }

                        let msg_content = if text.starts_with('/') {
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

                        let channel_msg = ChannelMessage {
                            channel: ChannelType::Custom("rocketchat".to_string()),
                            platform_message_id: msg_id,
                            sender: ChannelUser {
                                platform_id: channel_id.clone(),
                                display_name: sender_username.to_string(),
                                openfang_user: None,
                            },
                            content: msg_content,
                            target_agent: None,
                            timestamp: Utc::now(),
                            is_group: true,
                            thread_id,
                            metadata: {
                                let mut m = HashMap::new();
                                m.insert(
                                    "sender_id".to_string(),
                                    serde_json::Value::String(sender_id),
                                );
                                m
                            },
                        };

                        if tx.send(channel_msg).await.is_err() {
                            return;
                        }
                    }

                    // Update the last timestamp for this channel
                    if newest_ts != oldest {
                        last_timestamps
                            .write()
                            .await
                            .insert(channel_id.clone(), newest_ts);
                    }
                }
            }

            info!("Rocket.Chat polling loop stopped");
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

    async fn send_typing(&self, user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        // Rocket.Chat supports typing notifications via REST
        let url = format!("{}/api/v1/chat.sendMessage", self.server_url);
        // There's no dedicated typing endpoint in REST; this is a no-op.
        // Real typing would need the realtime API (WebSocket/DDP).
        let _ = url;
        let _ = user;
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
    fn test_rocketchat_adapter_creation() {
        let adapter = RocketChatAdapter::new(
            "rocketchat-1".to_string(),
            "https://chat.example.com".to_string(),
            "test-token".to_string(),
            "user123".to_string(),
            vec!["room1".to_string()],
        );
        assert_eq!(adapter.name(), "rocketchat");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("rocketchat".to_string())
        );
    }

    #[test]
    fn test_rocketchat_server_url_normalization() {
        let adapter = RocketChatAdapter::new(
            "rocketchat-2".to_string(),
            "https://chat.example.com/".to_string(),
            "tok".to_string(),
            "uid".to_string(),
            vec![],
        );
        assert_eq!(adapter.server_url, "https://chat.example.com");
    }

    #[test]
    fn test_rocketchat_allowed_channels() {
        let adapter = RocketChatAdapter::new(
            "rocketchat-3".to_string(),
            "https://chat.example.com".to_string(),
            "tok".to_string(),
            "uid".to_string(),
            vec!["room1".to_string()],
        );
        assert!(adapter.is_allowed_channel("room1"));
        assert!(!adapter.is_allowed_channel("room2"));

        let open = RocketChatAdapter::new(
            "rocketchat-4".to_string(),
            "https://chat.example.com".to_string(),
            "tok".to_string(),
            "uid".to_string(),
            vec![],
        );
        assert!(open.is_allowed_channel("any-room"));
    }

    #[test]
    fn test_rocketchat_auth_headers() {
        let adapter = RocketChatAdapter::new(
            "rocketchat-5".to_string(),
            "https://chat.example.com".to_string(),
            "my-token".to_string(),
            "user-42".to_string(),
            vec![],
        );
        // Verify the builder can be constructed (headers are added internally)
        let builder = adapter.client.get("https://example.com");
        let builder = adapter.auth_headers(builder);
        let request = builder.build().unwrap();
        assert_eq!(request.headers().get("X-Auth-Token").unwrap(), "my-token");
        assert_eq!(request.headers().get("X-User-Id").unwrap(), "user-42");
    }
}
