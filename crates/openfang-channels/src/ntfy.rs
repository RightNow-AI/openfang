//! ntfy.sh channel adapter.
//!
//! Subscribes to a ntfy topic via Server-Sent Events (SSE) for receiving
//! messages and publishes replies by POSTing to the same topic endpoint.
//! Supports self-hosted ntfy instances and optional Bearer token auth.

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

const MAX_MESSAGE_LEN: usize = 4096;
const DEFAULT_SERVER_URL: &str = "https://ntfy.sh";

/// ntfy.sh pub/sub channel adapter.
///
/// Subscribes to notifications via SSE and publishes replies as new
/// notifications. Supports authentication for protected topics.
pub struct NtfyAdapter {
    /// Unique identifier for this adapter instance.
    id: String,
    /// ntfy server URL (default: `"https://ntfy.sh"`).
    server_url: String,
    /// Topic name to subscribe and publish to.
    topic: String,
    /// SECURITY: Bearer token is zeroized on drop (empty = no auth).
    token: Zeroizing<String>,
    /// HTTP client.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
}

impl NtfyAdapter {
    /// Create a new ntfy adapter.
    ///
    /// # Arguments
    /// * `id` - Unique identifier for this adapter instance.
    /// * `server_url` - ntfy server URL (empty = default `"https://ntfy.sh"`).
    /// * `topic` - Topic name to subscribe/publish to.
    /// * `token` - Bearer token for authentication (empty = no auth).
    pub fn new(id: String, server_url: String, topic: String, token: String) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let server_url = if server_url.is_empty() {
            DEFAULT_SERVER_URL.to_string()
        } else {
            server_url.trim_end_matches('/').to_string()
        };
        Self {
            id,
            server_url,
            topic,
            token: Zeroizing::new(token),
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
        }
    }

    /// Build an authenticated request builder.
    fn auth_request(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if self.token.is_empty() {
            builder
        } else {
            builder.bearer_auth(self.token.as_str())
        }
    }

    /// Parse an SSE data line into a ntfy message.
    ///
    /// ntfy SSE format:
    /// ```text
    /// event: message
    /// data: {"id":"abc","time":1234,"event":"message","topic":"test","message":"Hello"}
    /// ```
    fn parse_sse_data(data: &str) -> Option<(String, String, String, Option<String>)> {
        let val: serde_json::Value = serde_json::from_str(data).ok()?;

        // Only process "message" events (skip "open", "keepalive", etc.)
        let event = val["event"].as_str().unwrap_or("");
        if event != "message" {
            return None;
        }

        let id = val["id"].as_str()?.to_string();
        let message = val["message"].as_str()?.to_string();
        let topic = val["topic"].as_str().unwrap_or("").to_string();

        if message.is_empty() {
            return None;
        }

        // ntfy messages can have a title (used as sender hint)
        let title = val["title"].as_str().map(String::from);

        Some((id, message, topic, title))
    }

    /// Publish a message to the topic.
    async fn publish(
        &self,
        text: &str,
        title: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/{}", self.server_url, self.topic);
        let chunks = split_message(text, MAX_MESSAGE_LEN);

        for chunk in chunks {
            let mut builder = self.client.post(&url);
            builder = self.auth_request(builder);

            // ntfy supports plain-text body publishing
            builder = builder.header("Content-Type", "text/plain");

            if let Some(t) = title {
                builder = builder.header("Title", t);
            }

            // Mark as UTF-8
            builder = builder.header("X-Message", chunk);
            let resp = builder.body(chunk.to_string()).send().await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let err_body = resp.text().await.unwrap_or_default();
                return Err(format!("ntfy publish error {status}: {err_body}").into());
            }
        }

        Ok(())
    }
}

#[async_trait]
impl ChannelAdapter for NtfyAdapter {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        "ntfy"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("ntfy".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        info!(
            "ntfy adapter subscribing to {}/{}",
            self.server_url, self.topic
        );

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let server_url = self.server_url.clone();
        let topic = self.topic.clone();
        let token = self.token.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let sse_client = reqwest::Client::builder()
                .timeout(Duration::from_secs(0)) // No timeout for SSE
                .build()
                .unwrap_or_default();

            let mut backoff = Duration::from_secs(1);

            loop {
                if *shutdown_rx.borrow() {
                    break;
                }

                let url = format!("{}/{}/sse", server_url, topic);
                let mut builder = sse_client.get(&url);
                if !token.is_empty() {
                    builder = builder.bearer_auth(token.as_str());
                }

                let response = match builder.send().await {
                    Ok(r) => {
                        if !r.status().is_success() {
                            warn!("ntfy: SSE returned HTTP {}", r.status());
                            tokio::time::sleep(backoff).await;
                            backoff = (backoff * 2).min(Duration::from_secs(120));
                            continue;
                        }
                        backoff = Duration::from_secs(1);
                        r
                    }
                    Err(e) => {
                        warn!("ntfy: SSE connection error: {e}, backing off {backoff:?}");
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_secs(120));
                        continue;
                    }
                };

                info!("ntfy: SSE stream connected for topic {topic}");

                let mut stream = response.bytes_stream();
                use futures::StreamExt;

                let mut line_buffer = String::new();
                let mut current_data = String::new();

                loop {
                    tokio::select! {
                        _ = shutdown_rx.changed() => {
                            if *shutdown_rx.borrow() {
                                info!("ntfy adapter shutting down");
                                return;
                            }
                        }
                        chunk = stream.next() => {
                            match chunk {
                                Some(Ok(bytes)) => {
                                    let text = String::from_utf8_lossy(&bytes);
                                    line_buffer.push_str(&text);

                                    // SSE parsing: process complete lines
                                    while let Some(newline_pos) = line_buffer.find('\n') {
                                        let line = line_buffer[..newline_pos].trim_end_matches('\r').to_string();
                                        line_buffer = line_buffer[newline_pos + 1..].to_string();

                                        if let Some(data) = line.strip_prefix("data: ") {
                                            current_data = data.to_string();
                                        } else if line.is_empty() && !current_data.is_empty() {
                                            // Empty line = end of SSE event
                                            if let Some((id, message, _topic, title)) =
                                                Self::parse_sse_data(&current_data)
                                            {
                                                let sender_name = title
                                                    .as_deref()
                                                    .unwrap_or("ntfy-user");

                                                let content = if message.starts_with('/') {
                                                    let parts: Vec<&str> =
                                                        message.splitn(2, ' ').collect();
                                                    let cmd =
                                                        parts[0].trim_start_matches('/');
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
                                                    ChannelContent::Text(message)
                                                };

                                                let msg = ChannelMessage {
                                                    channel: ChannelType::Custom(
                                                        "ntfy".to_string(),
                                                    ),
                                                    platform_message_id: id,
                                                    sender: ChannelUser {
                                                        platform_id: sender_name.to_string(),
                                                        display_name: sender_name.to_string(),
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
                                                            "topic".to_string(),
                                                            serde_json::Value::String(
                                                                topic.clone(),
                                                            ),
                                                        );
                                                        m
                                                    },
                                                };

                                                if tx.send(msg).await.is_err() {
                                                    return;
                                                }
                                            }
                                            current_data.clear();
                                        }
                                    }
                                }
                                Some(Err(e)) => {
                                    warn!("ntfy: SSE read error: {e}");
                                    break;
                                }
                                None => {
                                    info!("ntfy: SSE stream ended, reconnecting...");
                                    break;
                                }
                            }
                        }
                    }
                }

                // Backoff before reconnect
                if !*shutdown_rx.borrow() {
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(Duration::from_secs(60));
                }
            }

            info!("ntfy SSE loop stopped");
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
        self.publish(&text, Some("OpenFang")).await
    }

    async fn send_typing(&self, _user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        // ntfy has no typing indicator concept.
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
    fn test_ntfy_adapter_creation() {
        let adapter = NtfyAdapter::new(
            "ntfy-1".to_string(),
            "".to_string(),
            "my-topic".to_string(),
            "".to_string(),
        );
        assert_eq!(adapter.name(), "ntfy");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("ntfy".to_string())
        );
        assert_eq!(adapter.server_url, DEFAULT_SERVER_URL);
    }

    #[test]
    fn test_ntfy_custom_server_url() {
        let adapter = NtfyAdapter::new(
            "ntfy-2".to_string(),
            "https://ntfy.internal.corp/".to_string(),
            "alerts".to_string(),
            "token-123".to_string(),
        );
        assert_eq!(adapter.server_url, "https://ntfy.internal.corp");
        assert_eq!(adapter.topic, "alerts");
    }

    #[test]
    fn test_ntfy_auth_request_with_token() {
        let adapter = NtfyAdapter::new(
            "ntfy-3".to_string(),
            "".to_string(),
            "test".to_string(),
            "my-bearer-token".to_string(),
        );
        let builder = adapter.client.get("https://ntfy.sh/test");
        let builder = adapter.auth_request(builder);
        let request = builder.build().unwrap();
        assert!(request.headers().contains_key("authorization"));
    }

    #[test]
    fn test_ntfy_auth_request_without_token() {
        let adapter = NtfyAdapter::new(
            "ntfy-4".to_string(),
            "".to_string(),
            "test".to_string(),
            "".to_string(),
        );
        let builder = adapter.client.get("https://ntfy.sh/test");
        let builder = adapter.auth_request(builder);
        let request = builder.build().unwrap();
        assert!(!request.headers().contains_key("authorization"));
    }

    #[test]
    fn test_ntfy_parse_sse_message_event() {
        let data = r#"{"id":"abc123","time":1700000000,"event":"message","topic":"test","message":"Hello from ntfy","title":"Alice"}"#;
        let result = NtfyAdapter::parse_sse_data(data);
        assert!(result.is_some());
        let (id, message, topic, title) = result.unwrap();
        assert_eq!(id, "abc123");
        assert_eq!(message, "Hello from ntfy");
        assert_eq!(topic, "test");
        assert_eq!(title.as_deref(), Some("Alice"));
    }

    #[test]
    fn test_ntfy_parse_sse_keepalive_event() {
        let data = r#"{"id":"ka1","time":1700000000,"event":"keepalive","topic":"test"}"#;
        assert!(NtfyAdapter::parse_sse_data(data).is_none());
    }

    #[test]
    fn test_ntfy_parse_sse_open_event() {
        let data = r#"{"id":"o1","time":1700000000,"event":"open","topic":"test"}"#;
        assert!(NtfyAdapter::parse_sse_data(data).is_none());
    }

    #[test]
    fn test_ntfy_parse_sse_empty_message() {
        let data = r#"{"id":"e1","time":1700000000,"event":"message","topic":"test","message":""}"#;
        assert!(NtfyAdapter::parse_sse_data(data).is_none());
    }

    #[test]
    fn test_ntfy_parse_sse_no_title() {
        let data =
            r#"{"id":"nt1","time":1700000000,"event":"message","topic":"test","message":"Hi"}"#;
        let result = NtfyAdapter::parse_sse_data(data);
        assert!(result.is_some());
        let (_, _, _, title) = result.unwrap();
        assert!(title.is_none());
    }

    #[test]
    fn test_ntfy_parse_invalid_json() {
        assert!(NtfyAdapter::parse_sse_data("not json").is_none());
    }
}
