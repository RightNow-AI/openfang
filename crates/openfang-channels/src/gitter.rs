//! Gitter channel adapter.
//!
//! Connects to the Gitter Streaming API for real-time messages and posts
//! replies via the REST API. Uses Bearer token authentication and
//! newline-delimited JSON streaming.

use crate::supervisor::{self, SupervisorConfig};
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

const MAX_MESSAGE_LEN: usize = 4096;
const GITTER_STREAM_URL: &str = "https://stream.gitter.im/v1/rooms";
const GITTER_API_URL: &str = "https://api.gitter.im/v1/rooms";

/// Gitter streaming channel adapter.
///
/// Receives messages via the Gitter Streaming API (newline-delimited JSON)
/// and sends replies via the REST API.
pub struct GitterAdapter {
    /// SECURITY: Bearer token is zeroized on drop.
    token: Zeroizing<String>,
    /// Gitter room ID to listen on.
    room_id: String,
    /// HTTP client.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
}

impl GitterAdapter {
    /// Create a new Gitter adapter.
    ///
    /// # Arguments
    /// * `token` - Gitter personal access token.
    /// * `room_id` - Gitter room ID to listen on and send to.
    pub fn new(token: String, room_id: String) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            token: Zeroizing::new(token),
            room_id,
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
        }
    }

    /// Validate token by fetching the authenticated user.
    async fn validate(&self) -> Result<String, Box<dyn std::error::Error>> {
        let url = "https://api.gitter.im/v1/user";
        let resp = self
            .client
            .get(url)
            .bearer_auth(self.token.as_str())
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(format!("Gitter auth failed (HTTP {})", resp.status()).into());
        }

        let body: serde_json::Value = resp.json().await?;
        // /v1/user returns an array with a single user object
        let username = body
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|u| u["username"].as_str())
            .unwrap_or("unknown")
            .to_string();
        Ok(username)
    }

    /// Fetch room info to resolve display name.
    async fn get_room_name(&self) -> Result<String, Box<dyn std::error::Error>> {
        let url = format!("{}/{}", GITTER_API_URL, self.room_id);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(self.token.as_str())
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(format!("Gitter: failed to fetch room (HTTP {})", resp.status()).into());
        }

        let body: serde_json::Value = resp.json().await?;
        let name = body["name"].as_str().unwrap_or("unknown-room").to_string();
        Ok(name)
    }

    /// Send a text message to the room via REST API.
    async fn api_send_message(&self, text: &str) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/{}/chatMessages", GITTER_API_URL, self.room_id);
        let chunks = split_message(text, MAX_MESSAGE_LEN);

        for chunk in chunks {
            let body = serde_json::json!({
                "text": chunk,
            });

            let resp = self
                .client
                .post(&url)
                .bearer_auth(self.token.as_str())
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let err_body = resp.text().await.unwrap_or_default();
                return Err(format!("Gitter API error {status}: {err_body}").into());
            }
        }

        Ok(())
    }

    /// Parse a newline-delimited JSON message from the streaming API.
    fn parse_stream_message(line: &str) -> Option<(String, String, String, String)> {
        let val: serde_json::Value = serde_json::from_str(line).ok()?;
        let id = val["id"].as_str()?.to_string();
        let text = val["text"].as_str()?.to_string();
        let username = val["fromUser"]["username"].as_str()?.to_string();
        let display_name = val["fromUser"]["displayName"]
            .as_str()
            .unwrap_or(&username)
            .to_string();

        if text.is_empty() {
            return None;
        }

        Some((id, text, username, display_name))
    }
}

/// Run a single Gitter streaming connection cycle.
async fn run_gitter_stream(
    room_id: &str,
    token: &Zeroizing<String>,
    own_username: &str,
    tx: &mpsc::Sender<ChannelMessage>,
    shutdown_rx: &mut watch::Receiver<bool>,
) -> Result<bool, String> {
    use futures::StreamExt;
    use std::time::Duration;

    let stream_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(0))
        .build()
        .unwrap_or_default();

    let url = format!("{}/{}/chatMessages", GITTER_STREAM_URL, room_id);

    let response = stream_client
        .get(&url)
        .bearer_auth(token.as_str())
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Gitter stream connection error: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("Gitter stream returned HTTP {}", response.status()));
    }

    info!("Gitter: streaming connection established for room {room_id}");

    let mut stream = response.bytes_stream();
    let mut line_buffer = String::new();

    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    info!("Gitter adapter shutting down");
                    return Ok(false);
                }
            }
            chunk = stream.next() => {
                match chunk {
                    Some(Ok(bytes)) => {
                        let text = String::from_utf8_lossy(&bytes);
                        line_buffer.push_str(&text);

                        while let Some(newline_pos) = line_buffer.find('\n') {
                            let line = line_buffer[..newline_pos].trim().to_string();
                            line_buffer = line_buffer[newline_pos + 1..].to_string();

                            if line.is_empty() || line.chars().all(|c| c.is_whitespace()) {
                                continue;
                            }

                            if let Some((id, text, username, display_name)) =
                                GitterAdapter::parse_stream_message(&line)
                            {
                                if username == own_username {
                                    continue;
                                }

                                let content = if text.starts_with('/') {
                                    let parts: Vec<&str> = text.splitn(2, ' ').collect();
                                    let cmd = parts[0].trim_start_matches('/');
                                    let args: Vec<String> = parts
                                        .get(1)
                                        .map(|a| {
                                            a.split_whitespace()
                                                .map(String::from)
                                                .collect()
                                        })
                                        .unwrap_or_default();
                                    ChannelContent::Command {
                                        name: cmd.to_string(),
                                        args,
                                    }
                                } else {
                                    ChannelContent::Text(text)
                                };

                                let msg = ChannelMessage {
                                    channel: ChannelType::Custom("gitter".to_string()),
                                    platform_message_id: id,
                                    sender: ChannelUser {
                                        platform_id: username.clone(),
                                        display_name,
                                        openfang_user: None,
                                    },
                                    content,
                                    target_agent: None,
                                    timestamp: Utc::now(),
                                    is_group: true,
                                    thread_id: None,
                                    metadata: {
                                        let mut m = HashMap::new();
                                        m.insert(
                                            "room_id".to_string(),
                                            serde_json::Value::String(room_id.to_string()),
                                        );
                                        m
                                    },
                                };

                                if tx.send(msg).await.is_err() {
                                    return Ok(false);
                                }
                            }
                        }
                    }
                    Some(Err(e)) => {
                        warn!("Gitter: stream read error: {e}");
                        return Ok(true);
                    }
                    None => {
                        info!("Gitter: stream ended");
                        return Ok(true);
                    }
                }
            }
        }
    }
}

#[async_trait]
impl ChannelAdapter for GitterAdapter {
    fn name(&self) -> &str {
        "gitter"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("gitter".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        let own_username = self.validate().await?;
        let room_name = self.get_room_name().await.unwrap_or_default();
        info!("Gitter adapter authenticated as {own_username} in room {room_name}");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(supervisor::DEFAULT_CHANNEL_BUFFER);
        let room_id = self.room_id.clone();
        let token = self.token.clone();
        let shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            supervisor::run_supervised_loop_reset_on_connect(
                SupervisorConfig::new("Gitter"),
                shutdown_rx.clone(),
                || {
                    let room_id = room_id.clone();
                    let token = token.clone();
                    let own_username = own_username.clone();
                    let tx = tx.clone();
                    let mut shutdown_rx = shutdown_rx.clone();
                    async move {
                        run_gitter_stream(
                            &room_id, &token, &own_username,
                            &tx, &mut shutdown_rx,
                        ).await
                    }
                },
            ).await;
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn send(
        &self,
        _user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let text = match content {
            ChannelContent::Text(t) => t,
            _ => "(Unsupported content type)".to_string(),
        };
        self.api_send_message(&text).await
    }

    async fn send_typing(&self, _user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        // Gitter does not have a typing indicator API.
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
    fn test_gitter_adapter_creation() {
        let adapter = GitterAdapter::new("test-token".to_string(), "abc123room".to_string());
        assert_eq!(adapter.name(), "gitter");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("gitter".to_string())
        );
    }

    #[test]
    fn test_gitter_room_id() {
        let adapter = GitterAdapter::new("tok".to_string(), "my-room-id".to_string());
        assert_eq!(adapter.room_id, "my-room-id");
    }

    #[test]
    fn test_gitter_parse_stream_message() {
        let json = r#"{"id":"msg1","text":"Hello world","fromUser":{"username":"alice","displayName":"Alice B"}}"#;
        let result = GitterAdapter::parse_stream_message(json);
        assert!(result.is_some());
        let (id, text, username, display_name) = result.unwrap();
        assert_eq!(id, "msg1");
        assert_eq!(text, "Hello world");
        assert_eq!(username, "alice");
        assert_eq!(display_name, "Alice B");
    }

    #[test]
    fn test_gitter_parse_stream_message_missing_fields() {
        let json = r#"{"id":"msg1"}"#;
        assert!(GitterAdapter::parse_stream_message(json).is_none());
    }

    #[test]
    fn test_gitter_parse_stream_message_empty_text() {
        let json =
            r#"{"id":"msg1","text":"","fromUser":{"username":"alice","displayName":"Alice"}}"#;
        assert!(GitterAdapter::parse_stream_message(json).is_none());
    }

    #[test]
    fn test_gitter_parse_stream_message_no_display_name() {
        let json = r#"{"id":"msg1","text":"hi","fromUser":{"username":"bob"}}"#;
        let result = GitterAdapter::parse_stream_message(json);
        assert!(result.is_some());
        let (_, _, username, display_name) = result.unwrap();
        assert_eq!(username, "bob");
        assert_eq!(display_name, "bob"); // Falls back to username
    }

    #[test]
    fn test_gitter_parse_invalid_json() {
        assert!(GitterAdapter::parse_stream_message("not json").is_none());
        assert!(GitterAdapter::parse_stream_message("").is_none());
    }
}
