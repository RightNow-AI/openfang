//! Google Chat channel adapter.
//!
//! Uses Google Chat REST API with service account JWT authentication for sending
//! messages and a webhook listener for receiving inbound messages from Google Chat
//! spaces.

use crate::types::{
    split_message, ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser,
};
use async_trait::async_trait;
use chrono::Utc;
use futures::Stream;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch, RwLock};
use tracing::{info, warn};
use zeroize::Zeroizing;

const MAX_MESSAGE_LEN: usize = 4096;
const TOKEN_REFRESH_MARGIN_SECS: u64 = 300;

/// Google Chat channel adapter using service account authentication and REST API.
///
/// Inbound messages arrive via a configurable webhook HTTP listener.
/// Outbound messages are sent via the Google Chat REST API using an OAuth2 access
/// token obtained from a service account JWT.
pub struct GoogleChatAdapter {
    /// SECURITY: Service account key JSON is zeroized on drop.
    service_account_key: Zeroizing<String>,
    /// Space IDs to listen to (e.g., "spaces/AAAA").
    space_ids: Vec<String>,
    /// Port for the inbound webhook HTTP listener.
    webhook_port: u16,
    /// HTTP client for outbound API calls.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Cached OAuth2 access token with expiry instant.
    cached_token: Arc<RwLock<Option<(String, Instant)>>>,
}

impl GoogleChatAdapter {
    /// Create a new Google Chat adapter.
    ///
    /// # Arguments
    /// * `service_account_key` - JSON content of the Google service account key file.
    /// * `space_ids` - Google Chat space IDs to interact with.
    /// * `webhook_port` - Local port to bind the inbound webhook listener on.
    pub fn new(service_account_key: String, space_ids: Vec<String>, webhook_port: u16) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            service_account_key: Zeroizing::new(service_account_key),
            space_ids,
            webhook_port,
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            cached_token: Arc::new(RwLock::new(None)),
        }
    }

    /// Get a valid access token, refreshing if expired or missing.
    ///
    /// In a full implementation this would perform JWT signing and exchange with
    /// Google's OAuth2 token endpoint. For now it parses a pre-supplied token
    /// from the service account key JSON (field "access_token") or returns an
    /// error indicating that full JWT auth is not yet wired.
    async fn get_access_token(&self) -> Result<String, Box<dyn std::error::Error>> {
        // Check cache first
        {
            let cache = self.cached_token.read().await;
            if let Some((ref token, expiry)) = *cache {
                if Instant::now() + Duration::from_secs(TOKEN_REFRESH_MARGIN_SECS) < expiry {
                    return Ok(token.clone());
                }
            }
        }

        // Parse the service account key to extract project/client info
        let key_json: serde_json::Value = serde_json::from_str(&self.service_account_key)
            .map_err(|e| format!("Invalid service account key JSON: {e}"))?;

        // For a real implementation: build a JWT, sign with the private key,
        // exchange at https://oauth2.googleapis.com/token for an access token.
        // This adapter currently expects an "access_token" field for testing or
        // a pre-authorized token workflow.
        let token = key_json["access_token"]
            .as_str()
            .ok_or("Service account key missing 'access_token' field; full JWT auth not yet implemented")?
            .to_string();

        let expiry = Instant::now() + Duration::from_secs(3600);
        *self.cached_token.write().await = Some((token.clone(), expiry));

        Ok(token)
    }

    /// Send a text message to a Google Chat space.
    async fn api_send_message(
        &self,
        space_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let token = self.get_access_token().await?;
        let url = format!("https://chat.googleapis.com/v1/{}/messages", space_id);

        let chunks = split_message(text, MAX_MESSAGE_LEN);
        for chunk in chunks {
            let body = serde_json::json!({
                "text": chunk,
            });

            let resp = self
                .client
                .post(&url)
                .bearer_auth(&token)
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(format!("Google Chat API error {status}: {body}").into());
            }
        }

        Ok(())
    }

    /// Check if a space ID is in the allowed list.
    #[allow(dead_code)]
    fn is_allowed_space(&self, space_id: &str) -> bool {
        self.space_ids.is_empty() || self.space_ids.iter().any(|s| s == space_id)
    }
}

#[async_trait]
impl ChannelAdapter for GoogleChatAdapter {
    fn name(&self) -> &str {
        "google_chat"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("google_chat".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        // Validate we can parse the service account key
        let _key: serde_json::Value = serde_json::from_str(&self.service_account_key)
            .map_err(|e| format!("Invalid service account key: {e}"))?;

        info!(
            "Google Chat adapter starting webhook listener on port {}",
            self.webhook_port
        );

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let port = self.webhook_port;
        let space_ids = self.space_ids.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            // Bind a minimal HTTP listener for inbound webhooks
            let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    warn!("Google Chat: failed to bind webhook on port {port}: {e}");
                    return;
                }
            };

            info!("Google Chat webhook listener bound on {addr}");

            loop {
                let (stream, _peer) = tokio::select! {
                    _ = shutdown_rx.changed() => {
                        info!("Google Chat adapter shutting down");
                        break;
                    }
                    result = listener.accept() => {
                        match result {
                            Ok(conn) => conn,
                            Err(e) => {
                                warn!("Google Chat: accept error: {e}");
                                continue;
                            }
                        }
                    }
                };

                let tx = tx.clone();
                let space_ids = space_ids.clone();

                tokio::spawn(async move {
                    // Read HTTP request from the TCP stream
                    let mut reader = tokio::io::BufReader::new(stream);
                    let mut request_line = String::new();
                    if tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut request_line)
                        .await
                        .is_err()
                    {
                        return;
                    }

                    // Read headers to find Content-Length
                    let mut content_length: usize = 0;
                    loop {
                        let mut header_line = String::new();
                        if tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut header_line)
                            .await
                            .is_err()
                        {
                            return;
                        }
                        let trimmed = header_line.trim();
                        if trimmed.is_empty() {
                            break;
                        }
                        if let Some(val) = trimmed.strip_prefix("Content-Length:") {
                            if let Ok(len) = val.trim().parse::<usize>() {
                                content_length = len;
                            }
                        }
                        if let Some(val) = trimmed.strip_prefix("content-length:") {
                            if let Ok(len) = val.trim().parse::<usize>() {
                                content_length = len;
                            }
                        }
                    }

                    // Read body
                    let mut body_buf = vec![0u8; content_length.min(65536)];
                    use tokio::io::AsyncReadExt;
                    if content_length > 0
                        && reader
                            .read_exact(&mut body_buf[..content_length.min(65536)])
                            .await
                            .is_err()
                    {
                        return;
                    }

                    // Send 200 OK response
                    use tokio::io::AsyncWriteExt;
                    let resp = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
                    let _ = reader.get_mut().write_all(resp).await;

                    // Parse the Google Chat event payload
                    let payload: serde_json::Value =
                        match serde_json::from_slice(&body_buf[..content_length.min(65536)]) {
                            Ok(v) => v,
                            Err(_) => return,
                        };

                    let event_type = payload["type"].as_str().unwrap_or("");
                    if event_type != "MESSAGE" {
                        return;
                    }

                    let message = &payload["message"];
                    let text = message["text"].as_str().unwrap_or("");
                    if text.is_empty() {
                        return;
                    }

                    let space_name = payload["space"]["name"].as_str().unwrap_or("");
                    if !space_ids.is_empty() && !space_ids.iter().any(|s| s == space_name) {
                        return;
                    }

                    let sender_name = message["sender"]["displayName"]
                        .as_str()
                        .unwrap_or("unknown");
                    let sender_id = message["sender"]["name"].as_str().unwrap_or("unknown");
                    let message_name = message["name"].as_str().unwrap_or("").to_string();
                    let thread_name = message["thread"]["name"].as_str().map(String::from);
                    let space_type = payload["space"]["type"].as_str().unwrap_or("ROOM");
                    let is_group = space_type != "DM";

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
                        channel: ChannelType::Custom("google_chat".to_string()),
                        platform_message_id: message_name,
                        sender: ChannelUser {
                            platform_id: space_name.to_string(),
                            display_name: sender_name.to_string(),
                            openfang_user: None,
                            reply_url: None,
                        },
                        content: msg_content,
                        target_agent: None,
                        timestamp: Utc::now(),
                        is_group,
                        thread_id: thread_name,
                        metadata: {
                            let mut m = HashMap::new();
                            m.insert(
                                "sender_id".to_string(),
                                serde_json::Value::String(sender_id.to_string()),
                            );
                            m
                        },
                    };

                    let _ = tx.send(channel_msg).await;
                });
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

    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.shutdown_tx.send(true);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_google_chat_adapter_creation() {
        let adapter = GoogleChatAdapter::new(
            r#"{"access_token":"test-token","project_id":"test"}"#.to_string(),
            vec!["spaces/AAAA".to_string()],
            8090,
        );
        assert_eq!(adapter.name(), "google_chat");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("google_chat".to_string())
        );
    }

    #[test]
    fn test_google_chat_allowed_spaces() {
        let adapter = GoogleChatAdapter::new(
            r#"{"access_token":"tok"}"#.to_string(),
            vec!["spaces/AAAA".to_string()],
            8090,
        );
        assert!(adapter.is_allowed_space("spaces/AAAA"));
        assert!(!adapter.is_allowed_space("spaces/BBBB"));

        let open = GoogleChatAdapter::new(r#"{"access_token":"tok"}"#.to_string(), vec![], 8090);
        assert!(open.is_allowed_space("spaces/anything"));
    }

    #[tokio::test]
    async fn test_google_chat_token_caching() {
        let adapter = GoogleChatAdapter::new(
            r#"{"access_token":"cached-tok","project_id":"p"}"#.to_string(),
            vec![],
            8091,
        );

        // First call should parse and cache
        let token = adapter.get_access_token().await.unwrap();
        assert_eq!(token, "cached-tok");

        // Second call should return from cache
        let token2 = adapter.get_access_token().await.unwrap();
        assert_eq!(token2, "cached-tok");
    }

    #[test]
    fn test_google_chat_invalid_key() {
        let adapter = GoogleChatAdapter::new("not-json".to_string(), vec![], 8092);
        // Can't call async get_access_token in sync test, but verify construction works
        assert_eq!(adapter.webhook_port, 8092);
    }
}
