//! DingTalk Robot channel adapter.
//!
//! Integrates with the DingTalk (Alibaba) custom robot API. Incoming messages
//! are received via an HTTP webhook callback server, and outbound messages are
//! posted to the robot send endpoint with HMAC-SHA256 signature verification.

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

const MAX_MESSAGE_LEN: usize = 20000;
const DINGTALK_SEND_URL: &str = "https://oapi.dingtalk.com/robot/send";

/// DingTalk Robot channel adapter.
///
/// Uses a webhook listener to receive incoming messages from DingTalk
/// conversations and posts replies via the signed Robot Send API.
pub struct DingTalkAdapter {
    /// SECURITY: Robot access token is zeroized on drop.
    access_token: Zeroizing<String>,
    /// SECURITY: Signing secret for HMAC-SHA256 verification.
    secret: Zeroizing<String>,
    /// Port for the incoming webhook HTTP server.
    webhook_port: u16,
    /// HTTP client for outbound requests.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
}

impl DingTalkAdapter {
    /// Create a new DingTalk Robot adapter.
    ///
    /// # Arguments
    /// * `access_token` - Robot access token from DingTalk.
    /// * `secret` - Signing secret for request verification.
    /// * `webhook_port` - Local port to listen for DingTalk callbacks.
    pub fn new(access_token: String, secret: String, webhook_port: u16, client: reqwest::Client) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            access_token: Zeroizing::new(access_token),
            secret: Zeroizing::new(secret),
            webhook_port,
            client,
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
        }
    }

    /// Compute the HMAC-SHA256 signature for a DingTalk request.
    ///
    /// DingTalk signature = Base64(HMAC-SHA256(secret, timestamp + "\n" + secret))
    fn compute_signature(secret: &str, timestamp: i64) -> String {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let string_to_sign = format!("{}\n{}", timestamp, secret);
        let mut mac =
            Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key size");
        mac.update(string_to_sign.as_bytes());
        let result = mac.finalize();
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(result.into_bytes())
    }

    /// Verify an incoming DingTalk callback signature.
    fn verify_signature(secret: &str, timestamp: i64, signature: &str) -> bool {
        let expected = Self::compute_signature(secret, timestamp);
        // Constant-time comparison
        if expected.len() != signature.len() {
            return false;
        }
        let mut diff = 0u8;
        for (a, b) in expected.bytes().zip(signature.bytes()) {
            diff |= a ^ b;
        }
        diff == 0
    }

    /// Build the signed send URL with access_token, timestamp, and signature.
    fn build_send_url(&self) -> String {
        let timestamp = Utc::now().timestamp_millis();
        let sign = Self::compute_signature(&self.secret, timestamp);
        let encoded_sign = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("sign", &sign)
            .finish();
        format!(
            "{}?access_token={}&timestamp={}&{}",
            DINGTALK_SEND_URL,
            self.access_token.as_str(),
            timestamp,
            encoded_sign
        )
    }

    /// Parse a DingTalk webhook JSON body into extracted fields.
    fn parse_callback(body: &serde_json::Value) -> Option<(String, String, String, String, bool)> {
        let msg_type = body["msgtype"].as_str()?;
        let text = match msg_type {
            "text" => body["text"]["content"].as_str()?.trim().to_string(),
            _ => return None,
        };
        if text.is_empty() {
            return None;
        }

        let sender_id = body["senderId"].as_str().unwrap_or("unknown").to_string();
        let sender_nick = body["senderNick"].as_str().unwrap_or("Unknown").to_string();
        let conversation_id = body["conversationId"].as_str().unwrap_or("").to_string();
        let is_group = body["conversationType"].as_str() == Some("2");

        Some((text, sender_id, sender_nick, conversation_id, is_group))
    }
}

#[async_trait]
impl ChannelAdapter for DingTalkAdapter {
    fn name(&self) -> &str {
        "dingtalk"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("dingtalk".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let port = self.webhook_port;
        let secret = self.secret.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        info!("DingTalk adapter starting webhook server on port {port}");

        tokio::spawn(async move {
            let tx_shared = Arc::new(tx);
            let secret_shared = Arc::new(secret);

            let app = axum::Router::new().route(
                "/",
                axum::routing::post({
                    let tx = Arc::clone(&tx_shared);
                    let secret = Arc::clone(&secret_shared);
                    move |headers: axum::http::HeaderMap,
                          body: axum::extract::Json<serde_json::Value>| {
                        let tx = Arc::clone(&tx);
                        let secret = Arc::clone(&secret);
                        async move {
                            // Extract timestamp and sign from headers
                            let timestamp_str = headers
                                .get("timestamp")
                                .and_then(|v| v.to_str().ok())
                                .unwrap_or("0");
                            let signature = headers
                                .get("sign")
                                .and_then(|v| v.to_str().ok())
                                .unwrap_or("");

                            // Verify signature
                            if let Ok(ts) = timestamp_str.parse::<i64>() {
                                if !DingTalkAdapter::verify_signature(&secret, ts, signature) {
                                    warn!("DingTalk: invalid signature");
                                    return axum::http::StatusCode::FORBIDDEN;
                                }

                                // Check timestamp freshness (1 hour window)
                                let now = Utc::now().timestamp_millis();
                                if (now - ts).unsigned_abs() > 3_600_000 {
                                    warn!("DingTalk: stale timestamp");
                                    return axum::http::StatusCode::FORBIDDEN;
                                }
                            }

                            if let Some((text, sender_id, sender_nick, conv_id, is_group)) =
                                DingTalkAdapter::parse_callback(&body)
                            {
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
                                    ChannelContent::Text(text)
                                };

                                let msg = ChannelMessage {
                                    channel: ChannelType::Custom("dingtalk".to_string()),
                                    platform_message_id: format!(
                                        "dt-{}",
                                        Utc::now().timestamp_millis()
                                    ),
                                    sender: ChannelUser {
                                        platform_id: sender_id,
                                        display_name: sender_nick,
                                        openfang_user: None,
                                    },
                                    content,
                                    target_agent: None,
                                    timestamp: Utc::now(),
                                    is_group,
                                    thread_id: None,
                                    metadata: {
                                        let mut m = HashMap::new();
                                        m.insert(
                                            "conversation_id".to_string(),
                                            serde_json::Value::String(conv_id),
                                        );
                                        m
                                    },
                                };

                                let _ = tx.send(msg).await;
                            }

                            axum::http::StatusCode::OK
                        }
                    }
                }),
            );

            let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
            info!("DingTalk webhook server listening on {addr}");

            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    warn!("DingTalk: failed to bind port {port}: {e}");
                    return;
                }
            };

            let server = axum::serve(listener, app);

            tokio::select! {
                result = server => {
                    if let Err(e) = result {
                        warn!("DingTalk webhook server error: {e}");
                    }
                }
                _ = shutdown_rx.changed() => {
                    info!("DingTalk adapter shutting down");
                }
            }

            info!("DingTalk webhook server stopped");
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

        let chunks = split_message(&text, MAX_MESSAGE_LEN);
        let num_chunks = chunks.len();

        for chunk in chunks {
            let url = self.build_send_url();
            let body = serde_json::json!({
                "msgtype": "text",
                "text": {
                    "content": chunk,
                }
            });

            let resp = self.client.post(&url).json(&body).send().await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let err_body = resp.text().await.unwrap_or_default();
                return Err(format!("DingTalk API error {status}: {err_body}").into());
            }

            // DingTalk returns {"errcode": 0, "errmsg": "ok"} on success
            let result: serde_json::Value = resp.json().await?;
            if result["errcode"].as_i64() != Some(0) {
                return Err(format!(
                    "DingTalk error: {}",
                    result["errmsg"].as_str().unwrap_or("unknown")
                )
                .into());
            }

            // Rate limit: small delay between chunks
            if num_chunks > 1 {
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }

        Ok(())
    }

    async fn send_typing(&self, _user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        // DingTalk Robot API does not support typing indicators.
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
    fn test_dingtalk_adapter_creation() {
        let adapter =
            DingTalkAdapter::new("test-token".to_string(), "test-secret".to_string(), 8080, reqwest::Client::new());
        assert_eq!(adapter.name(), "dingtalk");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("dingtalk".to_string())
        );
    }

    #[test]
    fn test_dingtalk_signature_computation() {
        let timestamp: i64 = 1700000000000;
        let secret = "my-secret";
        let sig = DingTalkAdapter::compute_signature(secret, timestamp);
        assert!(!sig.is_empty());
        // Verify deterministic output
        let sig2 = DingTalkAdapter::compute_signature(secret, timestamp);
        assert_eq!(sig, sig2);
    }

    #[test]
    fn test_dingtalk_signature_verification() {
        let secret = "test-secret-123";
        let timestamp: i64 = 1700000000000;
        let sig = DingTalkAdapter::compute_signature(secret, timestamp);
        assert!(DingTalkAdapter::verify_signature(secret, timestamp, &sig));
        assert!(!DingTalkAdapter::verify_signature(
            secret, timestamp, "bad-sig"
        ));
        assert!(!DingTalkAdapter::verify_signature(
            "wrong-secret",
            timestamp,
            &sig
        ));
    }

    #[test]
    fn test_dingtalk_parse_callback_text() {
        let body = serde_json::json!({
            "msgtype": "text",
            "text": { "content": "Hello bot" },
            "senderId": "user123",
            "senderNick": "Alice",
            "conversationId": "conv456",
            "conversationType": "2",
        });
        let result = DingTalkAdapter::parse_callback(&body);
        assert!(result.is_some());
        let (text, sender_id, sender_nick, conv_id, is_group) = result.unwrap();
        assert_eq!(text, "Hello bot");
        assert_eq!(sender_id, "user123");
        assert_eq!(sender_nick, "Alice");
        assert_eq!(conv_id, "conv456");
        assert!(is_group);
    }

    #[test]
    fn test_dingtalk_parse_callback_unsupported_type() {
        let body = serde_json::json!({
            "msgtype": "image",
            "image": { "downloadCode": "abc" },
        });
        assert!(DingTalkAdapter::parse_callback(&body).is_none());
    }

    #[test]
    fn test_dingtalk_parse_callback_dm() {
        let body = serde_json::json!({
            "msgtype": "text",
            "text": { "content": "DM message" },
            "senderId": "u1",
            "senderNick": "Bob",
            "conversationId": "c1",
            "conversationType": "1",
        });
        let result = DingTalkAdapter::parse_callback(&body);
        assert!(result.is_some());
        let (_, _, _, _, is_group) = result.unwrap();
        assert!(!is_group);
    }

    #[test]
    fn test_dingtalk_send_url_contains_token_and_sign() {
        let adapter = DingTalkAdapter::new("my-token".to_string(), "my-secret".to_string(), 8080, reqwest::Client::new());
        let url = adapter.build_send_url();
        assert!(url.contains("access_token=my-token"));
        assert!(url.contains("timestamp="));
        assert!(url.contains("sign="));
    }
}
