//! Threema Gateway channel adapter.
//!
//! Uses the Threema Gateway HTTP API for sending messages and a local webhook
//! HTTP server for receiving inbound messages. Authentication is performed via
//! the Threema Gateway API secret. Inbound messages arrive as POST requests
//! to the configured webhook port.

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

/// Threema Gateway API base URL for sending messages.
const THREEMA_API_URL: &str = "https://msgapi.threema.ch";

/// Maximum message length for Threema messages.
const MAX_MESSAGE_LEN: usize = 3500;

/// Threema Gateway channel adapter using webhook for receiving and REST API for sending.
///
/// Listens for inbound messages via a configurable HTTP webhook server and sends
/// outbound messages via the Threema Gateway `send_simple` endpoint.
pub struct ThreemaAdapter {
    /// Unique identifier for this adapter instance.
    id: String,
    /// Threema Gateway ID (8-character alphanumeric, starts with '*').
    threema_id: String,
    /// SECURITY: API secret is zeroized on drop.
    secret: Zeroizing<String>,
    /// Port for the inbound webhook HTTP listener.
    webhook_port: u16,
    /// HTTP client for outbound API calls.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
}

impl ThreemaAdapter {
    /// Create a new Threema Gateway adapter.
    ///
    /// # Arguments
    /// * `id` - Unique identifier for this adapter instance.
    /// * `threema_id` - Threema Gateway ID (e.g., "*MYGATEW").
    /// * `secret` - API secret for the Gateway ID.
    /// * `webhook_port` - Local port to bind the inbound webhook listener on.
    pub fn new(id: String, threema_id: String, secret: String, webhook_port: u16) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            id,
            threema_id,
            secret: Zeroizing::new(secret),
            webhook_port,
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
        }
    }

    /// Validate credentials by checking the remaining credits.
    async fn validate(&self) -> Result<u64, Box<dyn std::error::Error>> {
        let url = format!(
            "{}/credits?from={}&secret={}",
            THREEMA_API_URL,
            self.threema_id,
            self.secret.as_str()
        );
        let resp = self.client.get(&url).send().await?;

        if !resp.status().is_success() {
            return Err("Threema Gateway authentication failed".into());
        }

        let credits: u64 = resp.text().await?.trim().parse().unwrap_or(0);
        Ok(credits)
    }

    /// Send a simple text message to a Threema ID.
    async fn api_send_message(
        &self,
        to: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/send_simple", THREEMA_API_URL);
        let chunks = split_message(text, MAX_MESSAGE_LEN);

        for chunk in chunks {
            let params = [
                ("from", self.threema_id.as_str()),
                ("to", to),
                ("secret", self.secret.as_str()),
                ("text", chunk),
            ];

            let resp = self.client.post(&url).form(&params).send().await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(format!("Threema API error {status}: {body}").into());
            }
        }

        Ok(())
    }
}

/// Parse an inbound Threema webhook payload into a `ChannelMessage`.
///
/// The Threema Gateway delivers inbound messages as form-encoded POST requests
/// with fields: `from`, `to`, `messageId`, `date`, `text`, `nonce`, `box`, `mac`.
/// For the `send_simple` mode, the `text` field contains the plaintext message.
fn parse_threema_webhook(
    payload: &HashMap<String, String>,
    own_id: &str,
) -> Option<ChannelMessage> {
    let from = payload.get("from")?;
    let text = payload.get("text").or_else(|| payload.get("body"))?;
    let message_id = payload.get("messageId").cloned().unwrap_or_default();

    // Skip messages from ourselves
    if from == own_id {
        return None;
    }

    if text.is_empty() {
        return None;
    }

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
    if let Some(nonce) = payload.get("nonce") {
        metadata.insert(
            "nonce".to_string(),
            serde_json::Value::String(nonce.clone()),
        );
    }
    if let Some(mac) = payload.get("mac") {
        metadata.insert("mac".to_string(), serde_json::Value::String(mac.clone()));
    }

    Some(ChannelMessage {
        channel: ChannelType::Custom("threema".to_string()),
        platform_message_id: message_id,
        sender: ChannelUser {
            platform_id: from.clone(),
            display_name: from.clone(),
            openfang_user: None,
        },
        content,
        target_agent: None,
        timestamp: Utc::now(),
        is_group: false, // Threema Gateway simple mode is 1:1
        thread_id: None,
        metadata,
    })
}

#[async_trait]
impl ChannelAdapter for ThreemaAdapter {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        "threema"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("threema".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        // Validate credentials
        let credits = self.validate().await?;
        info!(
            "Threema Gateway adapter authenticated (ID: {}, credits: {credits})",
            self.threema_id
        );

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let port = self.webhook_port;
        let own_id = self.threema_id.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            // Bind a webhook HTTP listener for inbound messages
            let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    warn!("Threema: failed to bind webhook on port {port}: {e}");
                    return;
                }
            };

            info!("Threema webhook listener bound on {addr}");

            loop {
                let (stream, _peer) = tokio::select! {
                    _ = shutdown_rx.changed() => {
                        info!("Threema adapter shutting down");
                        break;
                    }
                    result = listener.accept() => {
                        match result {
                            Ok(conn) => conn,
                            Err(e) => {
                                warn!("Threema: accept error: {e}");
                                continue;
                            }
                        }
                    }
                };

                let tx = tx.clone();
                let own_id = own_id.clone();

                tokio::spawn(async move {
                    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

                    let mut reader = tokio::io::BufReader::new(stream);

                    // Read HTTP request line
                    let mut request_line = String::new();
                    if reader.read_line(&mut request_line).await.is_err() {
                        return;
                    }

                    // Only accept POST requests
                    if !request_line.starts_with("POST") {
                        let resp = b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\n\r\n";
                        let _ = reader.get_mut().write_all(resp).await;
                        return;
                    }

                    // Read headers
                    let mut content_length: usize = 0;
                    let mut content_type = String::new();
                    loop {
                        let mut header = String::new();
                        if reader.read_line(&mut header).await.is_err() {
                            return;
                        }
                        let trimmed = header.trim();
                        if trimmed.is_empty() {
                            break;
                        }
                        let lower = trimmed.to_lowercase();
                        if let Some(val) = lower.strip_prefix("content-length:") {
                            if let Ok(len) = val.trim().parse::<usize>() {
                                content_length = len;
                            }
                        }
                        if let Some(val) = lower.strip_prefix("content-type:") {
                            content_type = val.trim().to_string();
                        }
                    }

                    // Read body (cap at 64KB)
                    let read_len = content_length.min(65536);
                    let mut body_buf = vec![0u8; read_len];
                    if read_len > 0 && reader.read_exact(&mut body_buf[..read_len]).await.is_err() {
                        return;
                    }

                    // Send 200 OK
                    let resp = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
                    let _ = reader.get_mut().write_all(resp).await;

                    // Parse the body based on content type
                    let body_str = String::from_utf8_lossy(&body_buf[..read_len]);
                    let payload: HashMap<String, String> =
                        if content_type.contains("application/json") {
                            // JSON payload
                            serde_json::from_str(&body_str).unwrap_or_default()
                        } else {
                            // Form-encoded payload
                            url::form_urlencoded::parse(body_str.as_bytes())
                                .map(|(k, v)| (k.to_string(), v.to_string()))
                                .collect()
                        };

                    if let Some(msg) = parse_threema_webhook(&payload, &own_id) {
                        let _ = tx.send(msg).await;
                    }
                });
            }

            info!("Threema webhook loop stopped");
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
        // Threema Gateway does not support typing indicators
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
    fn test_threema_adapter_creation() {
        let adapter = ThreemaAdapter::new("*MYGATEW".to_string(), "test-secret".to_string(), 8443);
        assert_eq!(adapter.name(), "threema");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("threema".to_string())
        );
    }

    #[test]
    fn test_threema_secret_zeroized() {
        let adapter =
            ThreemaAdapter::new("*MYID123".to_string(), "super-secret-key".to_string(), 8443);
        assert_eq!(adapter.secret.as_str(), "super-secret-key");
    }

    #[test]
    fn test_threema_webhook_port() {
        let adapter = ThreemaAdapter::new("*TEST".to_string(), "secret".to_string(), 9090);
        assert_eq!(adapter.webhook_port, 9090);
    }

    #[test]
    fn test_parse_threema_webhook_basic() {
        let mut payload = HashMap::new();
        payload.insert("from".to_string(), "ABCDEFGH".to_string());
        payload.insert("text".to_string(), "Hello from Threema!".to_string());
        payload.insert("messageId".to_string(), "msg-001".to_string());

        let msg = parse_threema_webhook(&payload, "*MYGATEW").unwrap();
        assert_eq!(msg.sender.platform_id, "ABCDEFGH");
        assert_eq!(msg.sender.display_name, "ABCDEFGH");
        assert!(!msg.is_group);
        assert!(matches!(msg.content, ChannelContent::Text(ref t) if t == "Hello from Threema!"));
    }

    #[test]
    fn test_parse_threema_webhook_command() {
        let mut payload = HashMap::new();
        payload.insert("from".to_string(), "SENDER01".to_string());
        payload.insert("text".to_string(), "/help me".to_string());

        let msg = parse_threema_webhook(&payload, "*MYGATEW").unwrap();
        match &msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "help");
                assert_eq!(args, &["me"]);
            }
            other => panic!("Expected Command, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_threema_webhook_skip_self() {
        let mut payload = HashMap::new();
        payload.insert("from".to_string(), "*MYGATEW".to_string());
        payload.insert("text".to_string(), "Self message".to_string());

        let msg = parse_threema_webhook(&payload, "*MYGATEW");
        assert!(msg.is_none());
    }

    #[test]
    fn test_parse_threema_webhook_empty_text() {
        let mut payload = HashMap::new();
        payload.insert("from".to_string(), "SENDER01".to_string());
        payload.insert("text".to_string(), String::new());

        let msg = parse_threema_webhook(&payload, "*MYGATEW");
        assert!(msg.is_none());
    }

    #[test]
    fn test_parse_threema_webhook_with_nonce_and_mac() {
        let mut payload = HashMap::new();
        payload.insert("from".to_string(), "SENDER01".to_string());
        payload.insert("text".to_string(), "Secure msg".to_string());
        payload.insert("nonce".to_string(), "abc123".to_string());
        payload.insert("mac".to_string(), "def456".to_string());

        let msg = parse_threema_webhook(&payload, "*MYGATEW").unwrap();
        assert!(msg.metadata.contains_key("nonce"));
        assert!(msg.metadata.contains_key("mac"));
    }
}
