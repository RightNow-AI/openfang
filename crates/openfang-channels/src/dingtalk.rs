//! DingTalk Robot channel adapter.
//!
//! Supports two modes for receiving messages:
//! - **Webhook mode** (default): Listens on a local HTTP port for DingTalk callbacks.
//!   Requires a public-facing URL (nginx, tunnel, etc.).
//! - **Stream mode**: Opens a WebSocket long-connection to DingTalk's gateway.
//!   No public IP or port required — ideal for NAT/internal deployments.
//!
//! Outbound messages are posted via the Robot Send API (webhook mode) or the
//! DingTalk Open API reply endpoint (stream mode).

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
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};
use zeroize::Zeroizing;

const MAX_MESSAGE_LEN: usize = 20000;
const DINGTALK_SEND_URL: &str = "https://oapi.dingtalk.com/robot/send";
const DINGTALK_GATEWAY_URL: &str = "https://api.dingtalk.com/v1.0/gateway/connections/open";
/// Connection mode for the DingTalk adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DingTalkMode {
    /// HTTP webhook callback mode — requires a public-facing port.
    Webhook,
    /// WebSocket stream mode — no public IP needed.
    Stream,
}

/// DingTalk Robot channel adapter.
pub struct DingTalkAdapter {
    mode: DingTalkMode,
    // ── Webhook mode fields ──
    /// SECURITY: Robot access token is zeroized on drop.
    access_token: Zeroizing<String>,
    /// SECURITY: Signing secret for HMAC-SHA256 verification.
    secret: Zeroizing<String>,
    /// Port for the incoming webhook HTTP server.
    webhook_port: u16,
    // ── Stream mode fields ──
    /// SECURITY: Client ID (AppKey) for stream mode.
    client_id: Zeroizing<String>,
    /// SECURITY: Client Secret (AppSecret) for stream mode.
    client_secret: Zeroizing<String>,
    /// HTTP client for outbound requests.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
}

impl DingTalkAdapter {
    /// Create a new DingTalk Robot adapter in **webhook** mode.
    pub fn new(access_token: String, secret: String, webhook_port: u16) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            mode: DingTalkMode::Webhook,
            access_token: Zeroizing::new(access_token),
            secret: Zeroizing::new(secret),
            webhook_port,
            client_id: Zeroizing::new(String::new()),
            client_secret: Zeroizing::new(String::new()),
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
        }
    }

    /// Create a new DingTalk Robot adapter in **stream** mode.
    pub fn new_stream(client_id: String, client_secret: String) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            mode: DingTalkMode::Stream,
            access_token: Zeroizing::new(String::new()),
            secret: Zeroizing::new(String::new()),
            webhook_port: 0,
            client_id: Zeroizing::new(client_id),
            client_secret: Zeroizing::new(client_secret),
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
        }
    }

    // ── Webhook helpers ──

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

    fn stream_reply_url(&self, user: &ChannelUser) -> Option<String> {
        user.openfang_user.as_ref().and_then(|v| {
            if v.is_empty() {
                None
            } else {
                Some(v.clone())
            }
        })
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

    // ── Stream mode helpers ──

    /// Register a WebSocket connection with the DingTalk gateway.
    /// Returns `(ws_endpoint, ticket)` on success.
    async fn register_stream_connection(
        client: &reqwest::Client,
        client_id: &str,
        client_secret: &str,
    ) -> Result<(String, String), Box<dyn std::error::Error + Send + Sync>> {
        let body = serde_json::json!({
            "clientId": client_id,
            "clientSecret": client_secret,
            "subscriptions": [
                { "type": "CALLBACK", "topic": "/v1.0/im/bot/messages/get" }
            ],
            "ua": "openfang"
        });

        let resp = client
            .post(DINGTALK_GATEWAY_URL)
            .json(&body)
            .timeout(Duration::from_secs(15))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            return Err(format!("DingTalk gateway registration failed ({status}): {err_body}").into());
        }

        let result: serde_json::Value = resp.json().await?;
        info!(response = %result, "DingTalk stream: gateway registration response");
        let endpoint = result["endpoint"]
            .as_str()
            .ok_or("DingTalk gateway: missing endpoint in response")?
            .to_string();
        let ticket = result["ticket"]
            .as_str()
            .ok_or("DingTalk gateway: missing ticket in response")?
            .to_string();

        Ok((endpoint, ticket))
    }

    /// Parse a stream event payload into a ChannelMessage.
    fn parse_stream_event(data: &serde_json::Value) -> Option<ChannelMessage> {
        // Stream events have a wrapper with headers + payload.
        // The actual message data is in the "data" field as a JSON string.
        let data_str = data["data"].as_str()?;
        let payload: serde_json::Value = serde_json::from_str(data_str).ok()?;

        let msg_type = payload["msgtype"].as_str().unwrap_or("text");
        info!(payload = %payload, "DingTalk stream: parsed callback payload");
        let text = match msg_type {
            "text" => payload["text"]["content"]
                .as_str()
                .unwrap_or("")
                .trim()
                .to_string(),
            _ => return None,
        };
        if text.is_empty() {
            return None;
        }

        let sender_id = payload["senderStaffId"]
            .as_str()
            .or_else(|| payload["senderId"].as_str())
            .unwrap_or("unknown")
            .to_string();
        let sender_nick = payload["senderNick"].as_str().unwrap_or("Unknown").to_string();
        let session_webhook = payload["sessionWebhook"].as_str().unwrap_or("").to_string();
        let session_webhook_expired_time = payload["sessionWebhookExpiredTime"]
            .as_i64()
            .unwrap_or_default();
        let conversation_id = payload["conversationId"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let is_group = payload["conversationType"].as_str() == Some("2");
        let was_mentioned = payload["isInAtList"].as_bool().unwrap_or_else(|| {
            payload["atUsers"]
                .as_array()
                .map(|users| !users.is_empty())
                .unwrap_or(false)
        });

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

        Some(ChannelMessage {
            channel: ChannelType::Custom("dingtalk".to_string()),
            platform_message_id: payload["msgId"]
                .as_str()
                .or_else(|| data["headers"]["messageId"].as_str())
                .map(str::to_string)
                .unwrap_or_else(|| format!("dt-{}", Utc::now().timestamp_millis())),
            sender: ChannelUser {
                platform_id: sender_id,
                display_name: sender_nick,
                openfang_user: if session_webhook.is_empty() {
                    None
                } else {
                    Some(session_webhook)
                },
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
                    serde_json::Value::String(conversation_id),
                );
                if session_webhook_expired_time > 0 {
                    m.insert(
                        "session_webhook_expired_time".to_string(),
                        serde_json::Value::Number(session_webhook_expired_time.into()),
                    );
                }
                if was_mentioned {
                    m.insert("was_mentioned".to_string(), serde_json::Value::Bool(true));
                }
                m
            },
        })
    }

    /// Start the webhook-based listener (original behavior).
    async fn start_webhook(
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
                                        .map(|a| {
                                            a.split_whitespace().map(String::from).collect()
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

    /// Start the stream-based WebSocket listener.
    async fn start_stream(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let client = self.client.clone();
        let client_id = self.client_id.clone();
        let client_secret = self.client_secret.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        info!("DingTalk adapter starting in stream mode (WebSocket long-connection)");

        tokio::spawn(async move {
            // Reconnect loop — re-registers and reconnects on disconnection.
            loop {
                // Check shutdown before attempting connection
                if *shutdown_rx.borrow() {
                    break;
                }

                // Step 1: Register with the gateway to get a WebSocket endpoint + ticket.
                let (endpoint, ticket) =
                    match Self::register_stream_connection(&client, &client_id, &client_secret)
                        .await
                    {
                        Ok(v) => v,
                        Err(e) => {
                            warn!("DingTalk stream: gateway registration failed: {e}");
                            // Back off before retrying.
                            tokio::select! {
                                _ = tokio::time::sleep(Duration::from_secs(10)) => continue,
                                _ = shutdown_rx.changed() => break,
                            }
                        }
                    };

                info!("DingTalk stream: registered, connecting to WebSocket");

                // Step 2: Build WebSocket URL with ticket as query param.
                let ws_url = format!("{}?ticket={}", endpoint, ticket);

                let ws_stream = match tokio_tungstenite::connect_async(&ws_url).await {
                    Ok((stream, _)) => stream,
                    Err(e) => {
                        warn!("DingTalk stream: WebSocket connection failed: {e}");
                        tokio::select! {
                            _ = tokio::time::sleep(Duration::from_secs(10)) => continue,
                            _ = shutdown_rx.changed() => break,
                        }
                    }
                };

                info!("DingTalk stream: WebSocket connected");

                let (mut ws_write, mut ws_read) = ws_stream.split();

                // Step 3: Read messages from the WebSocket.
                loop {
                    tokio::select! {
                        msg = ws_read.next() => {
                            match msg {
                                Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                                    info!("DingTalk stream: received text frame: {}", &text[..text.len().min(500)]);

                                    match serde_json::from_str::<serde_json::Value>(&text) {
                                        Ok(frame) => {
                                            let frame_type = frame["type"].as_str().unwrap_or("");
                                            let topic = frame["headers"]["topic"].as_str().unwrap_or("");
                                            let msg_id = frame["headers"]["messageId"].as_str().unwrap_or("");
                                            info!(frame_type, topic, msg_id, frame = %frame, "DingTalk stream: received frame");

                                            match frame_type {
                                                "SYSTEM" => {
                                                    // System events: ping/pong heartbeat
                                                    let topic = frame["headers"]["topic"].as_str().unwrap_or("");
                                                    if topic == "ping" {
                                                        let msg_id = frame["headers"]["messageId"]
                                                            .as_str()
                                                            .unwrap_or("");
                                                        let pong = serde_json::json!({
                                                            "code": 200,
                                                            "headers": {
                                                                "contentType": "application/json",
                                                                "messageId": msg_id,
                                                            },
                                                            "message": "OK",
                                                            "data": frame["data"],
                                                        });
                                                        if let Err(e) = ws_write
                                                            .send(tokio_tungstenite::tungstenite::Message::Text(
                                                                pong.to_string(),
                                                            ))
                                                            .await
                                                        {
                                                            warn!("DingTalk stream: failed to send pong: {e}");
                                                            break;
                                                        }
                                                        info!("DingTalk stream: pong sent");
                                                    }
                                                }
                                                "CALLBACK" => {
                                                    // Bot message callback
                                                    let msg_id = frame["headers"]["messageId"]
                                                        .as_str()
                                                        .unwrap_or("")
                                                        .to_string();

                                                    if let Some(channel_msg) =
                                                        Self::parse_stream_event(&frame)
                                                    {
                                                        let _ = tx.send(channel_msg).await;
                                                    }

                                                    // ACK the message so DingTalk doesn't redeliver.
                                                    let ack = serde_json::json!({
                                                        "code": 200,
                                                        "headers": {
                                                            "contentType": "application/json",
                                                            "messageId": msg_id,
                                                        },
                                                        "message": "OK",
                                                        "data": "{\"response\": null}",
                                                    });
                                                    if let Err(e) = ws_write
                                                        .send(tokio_tungstenite::tungstenite::Message::Text(
                                                            ack.to_string(),
                                                        ))
                                                        .await
                                                    {
                                                        warn!("DingTalk stream: failed to send ACK: {e}");
                                                        break;
                                                    }
                                                }
                                                _ => {
                                                    info!(frame_type, topic, "DingTalk stream: unhandled frame type/topic");
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            warn!("DingTalk stream: invalid JSON frame: {e}");
                                        }
                                    }
                                }
                                Some(Ok(tokio_tungstenite::tungstenite::Message::Ping(payload))) => {
                                    let _ = ws_write
                                        .send(tokio_tungstenite::tungstenite::Message::Pong(payload))
                                        .await;
                                }
                                Some(Ok(tokio_tungstenite::tungstenite::Message::Pong(_))) => {}
                                Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) => {
                                    info!("DingTalk stream: WebSocket closed by server, reconnecting...");
                                    break;
                                }
                                Some(Err(e)) => {
                                    warn!("DingTalk stream: WebSocket error: {e}");
                                    break;
                                }
                                None => {
                                    info!("DingTalk stream: WebSocket stream ended, reconnecting...");
                                    break;
                                }
                                _ => {}
                            }
                        }
                        _ = shutdown_rx.changed() => {
                            info!("DingTalk stream adapter shutting down");
                            let _ = ws_write
                                .send(tokio_tungstenite::tungstenite::Message::Close(None))
                                .await;
                            return;
                        }
                    }
                }

                // Brief delay before reconnecting.
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(3)) => {}
                    _ = shutdown_rx.changed() => break,
                }
                info!("DingTalk stream: reconnecting...");
            }

            info!("DingTalk stream adapter stopped");
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
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
        match self.mode {
            DingTalkMode::Webhook => self.start_webhook().await,
            DingTalkMode::Stream => self.start_stream().await,
        }
    }

    async fn send(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let text = match content {
            ChannelContent::Text(t) => t,
            _ => "(Unsupported content type)".to_string(),
        };

        let chunks = split_message(&text, MAX_MESSAGE_LEN);
        let num_chunks = chunks.len();

        for chunk in chunks {
            let url = match self.mode {
                DingTalkMode::Webhook => self.build_send_url(),
                DingTalkMode::Stream => self
                    .stream_reply_url(user)
                    .ok_or("DingTalk stream reply missing sessionWebhook")?,
            };
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

            let result: serde_json::Value = resp.json().await?;
            if self.mode == DingTalkMode::Webhook {
                if result["errcode"].as_i64() != Some(0) {
                    return Err(format!(
                        "DingTalk error: {}",
                        result["errmsg"].as_str().unwrap_or("unknown")
                    )
                    .into());
                }
            }

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
    fn test_dingtalk_adapter_creation_webhook() {
        let adapter =
            DingTalkAdapter::new("test-token".to_string(), "test-secret".to_string(), 8080);
        assert_eq!(adapter.name(), "dingtalk");
        assert_eq!(adapter.mode, DingTalkMode::Webhook);
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("dingtalk".to_string())
        );
    }

    #[test]
    fn test_dingtalk_adapter_creation_stream() {
        let adapter =
            DingTalkAdapter::new_stream("client-id".to_string(), "client-secret".to_string());
        assert_eq!(adapter.name(), "dingtalk");
        assert_eq!(adapter.mode, DingTalkMode::Stream);
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
        let adapter = DingTalkAdapter::new("my-token".to_string(), "my-secret".to_string(), 8080);
        let url = adapter.build_send_url();
        assert!(url.contains("access_token=my-token"));
        assert!(url.contains("timestamp="));
        assert!(url.contains("sign="));
    }

    #[test]
    fn test_dingtalk_parse_stream_event() {
        let data_payload = serde_json::json!({
            "msgtype": "text",
            "text": { "content": "Hello from stream" },
            "senderStaffId": "staff123",
            "senderNick": "StreamUser",
            "conversationId": "conv789",
            "conversationType": "1",
        });
        let frame = serde_json::json!({
            "type": "CALLBACK",
            "data": data_payload.to_string(),
        });
        let result = DingTalkAdapter::parse_stream_event(&frame);
        assert!(result.is_some());
        let msg = result.unwrap();
        assert_eq!(msg.sender.platform_id, "staff123");
        assert_eq!(msg.sender.display_name, "StreamUser");
        assert!(!msg.is_group);
        match msg.content {
            ChannelContent::Text(t) => assert_eq!(t, "Hello from stream"),
            _ => panic!("Expected text content"),
        }
    }

    #[test]
    fn test_dingtalk_parse_stream_event_command() {
        let data_payload = serde_json::json!({
            "msgtype": "text",
            "text": { "content": "/help arg1 arg2" },
            "senderStaffId": "s1",
            "senderNick": "Cmd",
            "conversationId": "c1",
            "conversationType": "2",
        });
        let frame = serde_json::json!({
            "type": "CALLBACK",
            "data": data_payload.to_string(),
        });
        let result = DingTalkAdapter::parse_stream_event(&frame);
        assert!(result.is_some());
        let msg = result.unwrap();
        assert!(msg.is_group);
        match msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "help");
                assert_eq!(args, vec!["arg1", "arg2"]);
            }
            _ => panic!("Expected command content"),
        }
    }

    #[test]
    fn test_dingtalk_parse_stream_event_preserves_message_id_and_mentions() {
        let data_payload = serde_json::json!({
            "msgtype": "text",
            "msgId": "msg-123",
            "text": { "content": "@openfang hello" },
            "senderStaffId": "s1",
            "senderNick": "Mentioned",
            "conversationId": "c1",
            "conversationType": "2",
            "isInAtList": true,
            "atUsers": [{ "dingtalkId": "$:LWCP_v1:$abc" }],
        });
        let frame = serde_json::json!({
            "type": "CALLBACK",
            "headers": {
                "messageId": "frame-456",
            },
            "data": data_payload.to_string(),
        });
        let result = DingTalkAdapter::parse_stream_event(&frame);
        assert!(result.is_some());
        let msg = result.unwrap();
        assert_eq!(msg.platform_message_id, "msg-123");
        assert_eq!(
            msg.metadata.get("was_mentioned").and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn test_dingtalk_parse_stream_event_empty_text() {
        let data_payload = serde_json::json!({
            "msgtype": "text",
            "text": { "content": "   " },
            "senderStaffId": "s1",
            "senderNick": "Empty",
            "conversationId": "c1",
            "conversationType": "1",
        });
        let frame = serde_json::json!({
            "type": "CALLBACK",
            "data": data_payload.to_string(),
        });
        assert!(DingTalkAdapter::parse_stream_event(&frame).is_none());
    }

    #[test]
    fn test_dingtalk_parse_stream_event_unsupported_type() {
        let data_payload = serde_json::json!({
            "msgtype": "image",
            "image": {},
            "senderStaffId": "s1",
            "senderNick": "Img",
            "conversationId": "c1",
            "conversationType": "1",
        });
        let frame = serde_json::json!({
            "type": "CALLBACK",
            "data": data_payload.to_string(),
        });
        assert!(DingTalkAdapter::parse_stream_event(&frame).is_none());
    }
}
