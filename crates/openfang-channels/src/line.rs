//! LINE Messaging API channel adapter.
//!
//! Uses the LINE Messaging API v2 for sending push/reply messages and a lightweight
//! axum HTTP webhook server for receiving inbound events. Webhook signature
//! verification uses HMAC-SHA256 with the channel secret. Authentication for
//! outbound calls uses `Authorization: Bearer {channel_access_token}`.

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
use tracing::{debug, info, warn};
use zeroize::Zeroizing;

/// LINE push message API endpoint.
const LINE_PUSH_URL: &str = "https://api.line.me/v2/bot/message/push";

/// LINE reply message API endpoint.
const LINE_REPLY_URL: &str = "https://api.line.me/v2/bot/message/reply";

/// LINE profile API endpoint.
#[allow(dead_code)]
const LINE_PROFILE_URL: &str = "https://api.line.me/v2/bot/profile";

/// Maximum LINE message text length (characters).
const MAX_MESSAGE_LEN: usize = 5000;

/// LINE Messaging API adapter.
///
/// Inbound messages arrive via an axum HTTP webhook server that accepts POST
/// requests from the LINE Platform. Each request body is validated using
/// HMAC-SHA256 (`X-Line-Signature` header) with the channel secret.
///
/// Outbound messages are sent via the push message API with a bearer token.
pub struct LineAdapter {
    /// SECURITY: Channel secret for webhook signature verification, zeroized on drop.
    channel_secret: Zeroizing<String>,
    /// SECURITY: Channel access token for outbound API calls, zeroized on drop.
    access_token: Zeroizing<String>,
    /// Port on which the inbound webhook HTTP server listens.
    webhook_port: u16,
    /// HTTP client for outbound API calls.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
}

impl LineAdapter {
    /// Create a new LINE adapter.
    ///
    /// # Arguments
    /// * `channel_secret` - Channel secret for HMAC-SHA256 signature verification.
    /// * `access_token` - Long-lived channel access token for sending messages.
    /// * `webhook_port` - Local port for the inbound webhook HTTP server.
    pub fn new(channel_secret: String, access_token: String, webhook_port: u16) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            channel_secret: Zeroizing::new(channel_secret.trim().to_string()),
            access_token: Zeroizing::new(access_token.trim().to_string()),
            webhook_port,
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
        }
    }

    /// Verify the X-Line-Signature header using HMAC-SHA256.
    ///
    /// The signature is computed as `Base64(HMAC-SHA256(channel_secret, body))`.
    fn verify_signature(&self, body: &[u8], signature: &str) -> bool {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        type HmacSha256 = Hmac<Sha256>;

        let Ok(mut mac) = HmacSha256::new_from_slice(self.channel_secret.as_bytes()) else {
            warn!("LINE: failed to create HMAC instance");
            return false;
        };
        mac.update(body);
        let result = mac.finalize().into_bytes();

        // Compare with constant-time base64 decode + verify
        use base64::Engine;
        let Ok(expected) = base64::engine::general_purpose::STANDARD.decode(signature) else {
            warn!("LINE: invalid base64 in X-Line-Signature");
            return false;
        };

        // Constant-time comparison to prevent timing attacks
        if result.len() != expected.len() {
            debug!(
                "LINE: signature length mismatch: computed={} received={}",
                result.len(),
                expected.len()
            );
            return false;
        }
        let mut diff = 0u8;
        for (a, b) in result.iter().zip(expected.iter()) {
            diff |= a ^ b;
        }
        if diff != 0 {
            let computed = base64::engine::general_purpose::STANDARD.encode(result);
            // Log first/last 4 chars of each signature for debugging without leaking full HMAC
            let comp_redacted = format!(
                "{}...{}",
                &computed[..4.min(computed.len())],
                &computed[computed.len().saturating_sub(4)..]
            );
            let recv_redacted = format!(
                "{}...{}",
                &signature[..4.min(signature.len())],
                &signature[signature.len().saturating_sub(4)..]
            );
            debug!(
                "LINE: signature mismatch: computed={comp_redacted} received={recv_redacted} body_len={}",
                body.len()
            );
        }
        diff == 0
    }

    /// Validate the channel access token by fetching the bot's own profile.
    async fn validate(&self) -> Result<String, Box<dyn std::error::Error>> {
        // Verify token by calling the bot info endpoint
        let resp = self
            .client
            .get("https://api.line.me/v2/bot/info")
            .bearer_auth(self.access_token.as_str())
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("LINE authentication failed {status}: {body}").into());
        }

        let body: serde_json::Value = resp.json().await?;
        let display_name = body["displayName"]
            .as_str()
            .unwrap_or("LINE Bot")
            .to_string();
        Ok(display_name)
    }

    /// Fetch a user's display name from the LINE profile API.
    #[allow(dead_code)]
    async fn get_user_display_name(&self, user_id: &str) -> String {
        let url = format!("{}/{}", LINE_PROFILE_URL, user_id);
        match self
            .client
            .get(&url)
            .bearer_auth(self.access_token.as_str())
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                let body: serde_json::Value = resp.json().await.unwrap_or_default();
                body["displayName"]
                    .as_str()
                    .unwrap_or("Unknown")
                    .to_string()
            }
            _ => "Unknown".to_string(),
        }
    }

    /// Send a push message to a LINE user or group.
    async fn api_push_message(
        &self,
        to: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let chunks = split_message(text, MAX_MESSAGE_LEN);

        for chunk in chunks {
            let body = serde_json::json!({
                "to": to,
                "messages": [
                    {
                        "type": "text",
                        "text": chunk,
                    }
                ]
            });

            let resp = self
                .client
                .post(LINE_PUSH_URL)
                .bearer_auth(self.access_token.as_str())
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let resp_body = resp.text().await.unwrap_or_default();
                return Err(format!("LINE push API error {status}: {resp_body}").into());
            }
        }

        Ok(())
    }

    /// Send a reply message using a reply token (must be used within 30s).
    #[allow(dead_code)]
    async fn api_reply_message(
        &self,
        reply_token: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let chunks = split_message(text, MAX_MESSAGE_LEN);
        // LINE reply API allows up to 5 messages per reply
        let messages: Vec<serde_json::Value> = chunks
            .into_iter()
            .take(5)
            .map(|chunk| {
                serde_json::json!({
                    "type": "text",
                    "text": chunk,
                })
            })
            .collect();

        let body = serde_json::json!({
            "replyToken": reply_token,
            "messages": messages,
        });

        let resp = self
            .client
            .post(LINE_REPLY_URL)
            .bearer_auth(self.access_token.as_str())
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let resp_body = resp.text().await.unwrap_or_default();
            return Err(format!("LINE reply API error {status}: {resp_body}").into());
        }

        Ok(())
    }
}

/// Parse a LINE webhook event into a `ChannelMessage`.
///
/// Handles `message` events with text type. Returns `None` for unsupported
/// event types (follow, unfollow, postback, beacon, etc.).
fn parse_line_event(event: &serde_json::Value) -> Option<ChannelMessage> {
    let event_type = event["type"].as_str().unwrap_or("");
    if event_type != "message" {
        return None;
    }

    let message = event.get("message")?;
    let msg_type = message["type"].as_str().unwrap_or("");

    // Only handle text messages for now
    if msg_type != "text" {
        return None;
    }

    let text = message["text"].as_str().unwrap_or("");
    if text.is_empty() {
        return None;
    }

    let source = event.get("source")?;
    let source_type = source["type"].as_str().unwrap_or("user");
    let user_id = source["userId"].as_str().unwrap_or("").to_string();

    // Determine the target (user, group, or room) for replies
    let (reply_to, is_group) = match source_type {
        "group" => {
            let group_id = source["groupId"].as_str().unwrap_or("").to_string();
            (group_id, true)
        }
        "room" => {
            let room_id = source["roomId"].as_str().unwrap_or("").to_string();
            (room_id, true)
        }
        _ => (user_id.clone(), false),
    };

    let msg_id = message["id"].as_str().unwrap_or("").to_string();
    let reply_token = event["replyToken"].as_str().unwrap_or("").to_string();

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
        "user_id".to_string(),
        serde_json::Value::String(user_id.clone()),
    );
    metadata.insert(
        "reply_to".to_string(),
        serde_json::Value::String(reply_to.clone()),
    );
    if !reply_token.is_empty() {
        metadata.insert(
            "reply_token".to_string(),
            serde_json::Value::String(reply_token),
        );
    }
    metadata.insert(
        "source_type".to_string(),
        serde_json::Value::String(source_type.to_string()),
    );

    Some(ChannelMessage {
        channel: ChannelType::Custom("line".to_string()),
        platform_message_id: msg_id,
        sender: ChannelUser {
            platform_id: reply_to,
            display_name: user_id,
            openfang_user: None,
        },
        content,
        target_agent: None,
        timestamp: Utc::now(),
        is_group,
        thread_id: None,
        metadata,
    })
}

#[async_trait]
impl ChannelAdapter for LineAdapter {
    fn name(&self) -> &str {
        "line"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("line".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        // Validate credentials
        let bot_name = self.validate().await?;
        info!("LINE adapter authenticated as {bot_name}");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let port = self.webhook_port;
        let channel_secret = self.channel_secret.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let channel_secret = Arc::new(channel_secret);
            let tx = Arc::new(tx);

            let app = axum::Router::new().route(
                "/webhook",
                axum::routing::post({
                    let secret = Arc::clone(&channel_secret);
                    let tx = Arc::clone(&tx);
                    move |headers: axum::http::HeaderMap, body: axum::body::Bytes| {
                        let secret = Arc::clone(&secret);
                        let tx = Arc::clone(&tx);
                        async move {
                            // Verify X-Line-Signature using the raw request
                            // body bytes — NOT re-serialized JSON — because the
                            // HMAC must be computed over the exact bytes LINE sent.
                            let signature = headers
                                .get("x-line-signature")
                                .and_then(|v| v.to_str().ok())
                                .unwrap_or("");

                            // Create a temporary adapter-like verifier
                            let adapter = LineAdapter {
                                channel_secret: secret.as_ref().clone(),
                                access_token: Zeroizing::new(String::new()),
                                webhook_port: 0,
                                client: reqwest::Client::new(),
                                shutdown_tx: Arc::new(watch::channel(false).0),
                                shutdown_rx: watch::channel(false).1,
                            };

                            if !signature.is_empty() && !adapter.verify_signature(&body, signature)
                            {
                                warn!("LINE: invalid webhook signature");
                                return axum::http::StatusCode::UNAUTHORIZED;
                            }

                            // Parse the raw bytes into JSON after signature verification
                            let parsed: serde_json::Value = match serde_json::from_slice(&body) {
                                Ok(v) => v,
                                Err(e) => {
                                    warn!("LINE: failed to parse webhook body as JSON: {e}");
                                    return axum::http::StatusCode::BAD_REQUEST;
                                }
                            };

                            // Parse events array
                            if let Some(events) = parsed["events"].as_array() {
                                for event in events {
                                    if let Some(msg) = parse_line_event(event) {
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
            info!("LINE webhook server listening on {addr}");

            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    warn!("LINE webhook bind failed: {e}");
                    return;
                }
            };

            let server = axum::serve(listener, app);

            tokio::select! {
                result = server => {
                    if let Err(e) = result {
                        warn!("LINE webhook server error: {e}");
                    }
                }
                _ = shutdown_rx.changed() => {
                    info!("LINE adapter shutting down");
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
                self.api_push_message(&user.platform_id, &text).await?;
            }
            ChannelContent::Image { url, caption } => {
                // LINE supports image messages with a preview
                let body = serde_json::json!({
                    "to": user.platform_id,
                    "messages": [
                        {
                            "type": "image",
                            "originalContentUrl": url,
                            "previewImageUrl": url,
                        }
                    ]
                });

                let resp = self
                    .client
                    .post(LINE_PUSH_URL)
                    .bearer_auth(self.access_token.as_str())
                    .json(&body)
                    .send()
                    .await?;

                if !resp.status().is_success() {
                    let status = resp.status();
                    let resp_body = resp.text().await.unwrap_or_default();
                    warn!("LINE image push error {status}: {resp_body}");
                }

                // Send caption as separate text if present
                if let Some(cap) = caption {
                    if !cap.is_empty() {
                        self.api_push_message(&user.platform_id, &cap).await?;
                    }
                }
            }
            _ => {
                self.api_push_message(&user.platform_id, "(Unsupported content type)")
                    .await?;
            }
        }
        Ok(())
    }

    async fn send_typing(&self, _user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        // LINE does not support typing indicators via REST API
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
    fn test_line_adapter_creation() {
        let adapter = LineAdapter::new(
            "channel-secret-123".to_string(),
            "access-token-456".to_string(),
            8080,
        );
        assert_eq!(adapter.name(), "line");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("line".to_string())
        );
        assert_eq!(adapter.webhook_port, 8080);
    }

    #[test]
    fn test_line_adapter_both_tokens() {
        let adapter = LineAdapter::new("secret".to_string(), "token".to_string(), 9000);
        // Verify both secrets are stored as Zeroizing
        assert_eq!(adapter.channel_secret.as_str(), "secret");
        assert_eq!(adapter.access_token.as_str(), "token");
    }

    #[test]
    fn test_parse_line_event_text_message() {
        let event = serde_json::json!({
            "type": "message",
            "replyToken": "reply-token-123",
            "source": {
                "type": "user",
                "userId": "U1234567890"
            },
            "message": {
                "id": "msg-001",
                "type": "text",
                "text": "Hello from LINE!"
            }
        });

        let msg = parse_line_event(&event).unwrap();
        assert_eq!(msg.channel, ChannelType::Custom("line".to_string()));
        assert_eq!(msg.platform_message_id, "msg-001");
        assert!(!msg.is_group);
        assert!(matches!(msg.content, ChannelContent::Text(ref t) if t == "Hello from LINE!"));
        assert!(msg.metadata.contains_key("reply_token"));
    }

    #[test]
    fn test_parse_line_event_group_message() {
        let event = serde_json::json!({
            "type": "message",
            "replyToken": "reply-token-456",
            "source": {
                "type": "group",
                "groupId": "C1234567890",
                "userId": "U1234567890"
            },
            "message": {
                "id": "msg-002",
                "type": "text",
                "text": "Group message"
            }
        });

        let msg = parse_line_event(&event).unwrap();
        assert!(msg.is_group);
        assert_eq!(msg.sender.platform_id, "C1234567890");
    }

    #[test]
    fn test_parse_line_event_command() {
        let event = serde_json::json!({
            "type": "message",
            "replyToken": "rt",
            "source": {
                "type": "user",
                "userId": "U123"
            },
            "message": {
                "id": "msg-003",
                "type": "text",
                "text": "/status all"
            }
        });

        let msg = parse_line_event(&event).unwrap();
        match &msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "status");
                assert_eq!(args, &["all"]);
            }
            other => panic!("Expected Command, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_line_event_non_message() {
        let event = serde_json::json!({
            "type": "follow",
            "replyToken": "rt",
            "source": {
                "type": "user",
                "userId": "U123"
            }
        });

        assert!(parse_line_event(&event).is_none());
    }

    #[test]
    fn test_parse_line_event_non_text() {
        let event = serde_json::json!({
            "type": "message",
            "replyToken": "rt",
            "source": {
                "type": "user",
                "userId": "U123"
            },
            "message": {
                "id": "msg-004",
                "type": "sticker",
                "packageId": "1",
                "stickerId": "1"
            }
        });

        assert!(parse_line_event(&event).is_none());
    }

    #[test]
    fn test_verify_signature_with_raw_body() {
        // Verify that HMAC-SHA256 signature validation works with raw body bytes
        let secret = "test-channel-secret";
        let adapter = LineAdapter::new(secret.to_string(), "token".to_string(), 9000);

        // Compute the expected signature manually
        use base64::Engine;
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        type HmacSha256 = Hmac<Sha256>;

        let body = br#"{"events":[{"type":"message"}]}"#;
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let expected_sig =
            base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());

        assert!(adapter.verify_signature(body, &expected_sig));

        // Re-serialized JSON should NOT match (this was the bug)
        let parsed: serde_json::Value = serde_json::from_slice(body).unwrap();
        let reserialized = serde_json::to_vec(&parsed).unwrap();
        // The re-serialized form may differ in whitespace/key order
        // If it happens to be identical for this input, the test still validates
        // the core mechanism works with raw bytes
        if reserialized != body.to_vec() {
            assert!(!adapter.verify_signature(&reserialized, &expected_sig));
        }
    }

    #[test]
    fn test_channel_secret_trimmed() {
        // Environment variables often have trailing newlines or spaces
        let adapter = LineAdapter::new(
            "  my-secret\n".to_string(),
            "  my-token\r\n".to_string(),
            9000,
        );
        assert_eq!(adapter.channel_secret.as_str(), "my-secret");
        assert_eq!(adapter.access_token.as_str(), "my-token");
    }

    #[test]
    fn test_verify_signature_bad_base64() {
        let adapter = LineAdapter::new("secret".to_string(), "token".to_string(), 9000);
        assert!(!adapter.verify_signature(b"body", "not-valid-base64!!!"));
    }

    #[test]
    fn test_verify_signature_wrong_secret() {
        use base64::Engine;
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        type HmacSha256 = Hmac<Sha256>;

        let body = b"test body";
        let mut mac = HmacSha256::new_from_slice(b"wrong-secret").unwrap();
        mac.update(body);
        let sig = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());

        let adapter = LineAdapter::new("correct-secret".to_string(), "token".to_string(), 9000);
        assert!(!adapter.verify_signature(body, &sig));
    }

    #[test]
    fn test_parse_line_event_room_source() {
        let event = serde_json::json!({
            "type": "message",
            "replyToken": "rt",
            "source": {
                "type": "room",
                "roomId": "R1234567890",
                "userId": "U123"
            },
            "message": {
                "id": "msg-005",
                "type": "text",
                "text": "Room message"
            }
        });

        let msg = parse_line_event(&event).unwrap();
        assert!(msg.is_group);
        assert_eq!(msg.sender.platform_id, "R1234567890");
    }
}
