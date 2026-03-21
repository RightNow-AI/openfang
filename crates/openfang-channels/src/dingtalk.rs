//! DingTalk Robot channel adapter.
//!
//! Supports two modes:
//! - **Webhook mode**: Receives messages via an HTTP webhook callback server.
//! - **Stream mode**: Receives messages via WebSocket connection to DingTalk servers
//!   (no public IP required).
//!
//! Outbound messages are posted to the robot send endpoint with HMAC-SHA256 signature.

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
use tracing::{debug, error, info, warn};
use zeroize::Zeroizing;

const MAX_MESSAGE_LEN: usize = 20000;
const DINGTALK_SEND_URL: &str = "https://oapi.dingtalk.com/robot/send";
const DINGTALK_STREAM_OPEN_URL: &str = "https://api.dingtalk.com/v1.0/gateway/connections/open";
const MAX_BACKOFF: Duration = Duration::from_secs(60);
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);

/// Connection mode for DingTalk adapter.
#[derive(Debug, Clone, PartialEq)]
pub enum DingTalkMode {
    /// Webhook mode: requires a public HTTP endpoint for callbacks.
    Webhook {
        /// Port for the incoming webhook HTTP server.
        port: u16,
    },
    /// Stream mode: WebSocket connection to DingTalk (no public IP needed).
    Stream {
        /// DingTalk AppKey (Client ID) for Stream mode authentication.
        client_id: String,
        /// DingTalk AppSecret (Client Secret) for Stream mode authentication.
        client_secret: String,
    },
}

/// DingTalk Robot channel adapter.
///
/// Uses a webhook listener to receive incoming messages from DingTalk
/// conversations and posts replies via the signed Robot Send API.
pub struct DingTalkAdapter {
    /// SECURITY: Robot access token is zeroized on drop.
    access_token: Zeroizing<String>,
    /// SECURITY: Signing secret for HMAC-SHA256 verification.
    secret: Zeroizing<String>,
    /// Connection mode (Webhook or Stream).
    mode: DingTalkMode,
    /// HTTP client for outbound requests.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Bot's own union ID for filtering own messages in Stream mode.
    bot_union_id: Arc<RwLock<Option<String>>>,
}

impl DingTalkAdapter {
    /// Create a new DingTalk Robot adapter in Webhook mode.
    ///
    /// # Arguments
    /// * `access_token` - Robot access token from DingTalk.
    /// * `secret` - Signing secret for request verification.
    /// * `webhook_port` - Local port to listen for DingTalk callbacks.
    pub fn new(access_token: String, secret: String, webhook_port: u16) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            access_token: Zeroizing::new(access_token),
            secret: Zeroizing::new(secret),
            mode: DingTalkMode::Webhook { port: webhook_port },
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            bot_union_id: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a new DingTalk Robot adapter in Stream mode.
    ///
    /// # Arguments
    /// * `access_token` - Robot access token from DingTalk.
    /// * `secret` - Signing secret for HMAC-SHA256 verification.
    /// * `client_id` - DingTalk AppKey for Stream mode.
    /// * `client_secret` - DingTalk AppSecret for Stream mode.
    pub fn new_stream(
        access_token: String,
        secret: String,
        client_id: String,
        client_secret: String,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            access_token: Zeroizing::new(access_token),
            secret: Zeroizing::new(secret),
            mode: DingTalkMode::Stream {
                client_id,
                client_secret,
            },
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            bot_union_id: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a new DingTalk Robot adapter with explicit mode.
    ///
    /// # Arguments
    /// * `access_token` - Robot access token from DingTalk.
    /// * `secret` - Signing secret for HMAC-SHA256 verification.
    /// * `mode` - Connection mode (Webhook or Stream).
    pub fn with_mode(access_token: String, secret: String, mode: DingTalkMode) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            access_token: Zeroizing::new(access_token),
            secret: Zeroizing::new(secret),
            mode,
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            bot_union_id: Arc::new(RwLock::new(None)),
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

    /// Start the webhook server (HTTP callback mode).
    async fn start_webhook(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        let port = match &self.mode {
            DingTalkMode::Webhook { port } => *port,
            _ => return Err("Not in webhook mode".into()),
        };

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
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
                                        reply_url: None,
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

    /// Start the Stream mode WebSocket connection.
    async fn start_stream(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        let (client_id, client_secret) = match &self.mode {
            DingTalkMode::Stream {
                client_id,
                client_secret,
            } => (client_id.clone(), client_secret.clone()),
            _ => return Err("Not in stream mode".into()),
        };

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);

        let client = self.client.clone();
        let bot_union_id = self.bot_union_id.clone();
        let mut shutdown = self.shutdown_rx.clone();

        info!("DingTalk adapter starting Stream mode");

        tokio::spawn(async move {
            let mut backoff = INITIAL_BACKOFF;

            loop {
                if *shutdown.borrow() {
                    break;
                }

                // Get WebSocket connection URL from DingTalk API
                let ws_url_result = get_stream_websocket_url(&client, &client_id, &client_secret)
                    .await
                    .map_err(|e| e.to_string());

                let (ws_url, token) = match ws_url_result {
                    Ok(result) => result,
                    Err(err_msg) => {
                        warn!("DingTalk Stream: failed to get WebSocket URL: {err_msg}, retrying in {backoff:?}");
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(MAX_BACKOFF);
                        continue;
                    }
                };

                // Build WebSocket URL with ticket parameter
                let ws_url_with_ticket = format!("{}?ticket={}", ws_url, token);
                info!("DingTalk Stream: connecting to WebSocket: {}", ws_url_with_ticket);

                let ws_result = tokio_tungstenite::connect_async(&ws_url_with_ticket).await;
                let ws_stream = match ws_result {
                    Ok((stream, _)) => stream,
                    Err(e) => {
                        warn!("DingTalk Stream: WebSocket connection failed: {e}, retrying in {backoff:?}");
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(MAX_BACKOFF);
                        continue;
                    }
                };

                backoff = INITIAL_BACKOFF;
                info!("DingTalk Stream: WebSocket connected, waiting for messages...");

                let (mut ws_tx, mut ws_rx) = ws_stream.split();

                // No need to send subscribe message - subscriptions are already specified in connections.open request
                // DingTalk will push messages automatically based on registered subscriptions

                let should_reconnect = 'inner: loop {
                    let msg = tokio::select! {
                        msg = ws_rx.next() => msg,
                        _ = shutdown.changed() => {
                            if *shutdown.borrow() {
                                let _ = ws_tx.close().await;
                                return;
                            }
                            continue;
                        }
                    };

                    let msg = match msg {
                        Some(Ok(m)) => m,
                        Some(Err(e)) => {
                            warn!("DingTalk Stream: WebSocket error: {e}");
                            break 'inner true;
                        }
                        None => {
                            info!("DingTalk Stream: WebSocket closed");
                            break 'inner true;
                        }
                    };

                    let text = match msg {
                        tokio_tungstenite::tungstenite::Message::Text(t) => t,
                        tokio_tungstenite::tungstenite::Message::Ping(data) => {
                            let _ = ws_tx
                                .send(tokio_tungstenite::tungstenite::Message::Pong(data))
                                .await;
                            continue;
                        }
                        tokio_tungstenite::tungstenite::Message::Close(_) => {
                            info!("DingTalk Stream: closed by server");
                            break 'inner true;
                        }
                        _ => continue,
                    };

                    let payload: serde_json::Value = match serde_json::from_str(&text) {
                        Ok(v) => v,
                        Err(e) => {
                            warn!("DingTalk Stream: failed to parse message: {e}");
                            continue;
                        }
                    };

                    warn!("DingTalk Stream: received message (debug): {}", text.chars().take(200).collect::<String>());

                    // DingTalk Stream protocol message format:
                    // { "specVersion": "1.0", "type": "SYSTEM|EVENT|CALLBACK", "headers": {...}, "data": "..." }
                    let msg_type = payload["type"].as_str().unwrap_or("");
                    let topic = payload["headers"]["topic"].as_str().unwrap_or("");
                    let message_id = payload["headers"]["messageId"].as_str().unwrap_or("");

                    match msg_type {
                        "SYSTEM" => {
                            match topic {
                                "ping" => {
                                    // Respond to ping with ACK
                                    let opaque = payload["data"]["opaque"].as_str().unwrap_or("");
                                    let ack = serde_json::json!({
                                        "code": 200,
                                        "headers": {
                                            "contentType": "application/json",
                                            "messageId": message_id
                                        },
                                        "message": "OK",
                                        "data": format!("{{\"opaque\": \"{}\"}}", opaque)
                                    });
                                    if let Err(e) = ws_tx
                                        .send(tokio_tungstenite::tungstenite::Message::Text(
                                            serde_json::to_string(&ack).unwrap(),
                                        ))
                                        .await
                                    {
                                        warn!("DingTalk Stream: failed to send ping ack: {e}");
                                    }
                                    debug!("DingTalk Stream: ping acknowledged");
                                }
                                "disconnect" => {
                                    let reason = payload["data"]["reason"].as_str().unwrap_or("unknown");
                                    info!("DingTalk Stream: disconnect request: {reason}");
                                    // Wait 10s before reconnecting (per protocol)
                                    tokio::time::sleep(Duration::from_secs(10)).await;
                                    break 'inner true;
                                }
                                _ => {
                                    debug!("DingTalk Stream: unknown system topic: {topic}");
                                }
                            }
                        }

                        "CALLBACK" => {
                            // Send ACK first
                            let ack = serde_json::json!({
                                "code": 200,
                                "headers": {
                                    "contentType": "application/json",
                                    "messageId": message_id
                                },
                                "message": "OK",
                                "data": "{\"response\": null}"
                            });
                            if let Err(e) = ws_tx
                                .send(tokio_tungstenite::tungstenite::Message::Text(
                                    serde_json::to_string(&ack).unwrap(),
                                ))
                                .await
                            {
                                warn!("DingTalk Stream: failed to send callback ack: {e}");
                            }

                            // Process incoming message callback
                            if let Some(msg) = parse_stream_callback(
                                &payload,
                                &bot_union_id,
                            )
                            .await
                            {
                                info!(
                                    "DingTalk Stream: message from {}: {:?}",
                                    msg.sender.display_name, msg.content
                                );
                                if tx.send(msg).await.is_err() {
                                    return;
                                }
                            }
                        }

                        "EVENT" => {
                            // Send ACK for events
                            let ack = serde_json::json!({
                                "code": 200,
                                "headers": {
                                    "contentType": "application/json",
                                    "messageId": message_id
                                },
                                "message": "OK",
                                "data": "{\"status\": \"SUCCESS\", \"message\": \"success\"}"
                            });
                            if let Err(e) = ws_tx
                                .send(tokio_tungstenite::tungstenite::Message::Text(
                                    serde_json::to_string(&ack).unwrap(),
                                ))
                                .await
                            {
                                warn!("DingTalk Stream: failed to send event ack: {e}");
                            }
                            debug!("DingTalk Stream: event acknowledged, topic: {topic}");
                        }

                        _ => {
                            warn!("DingTalk Stream: unknown message type: {msg_type}");
                        }
                    }
                };

                if !should_reconnect || *shutdown.borrow() {
                    break;
                }

                warn!("DingTalk Stream: reconnecting in {backoff:?}");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }

            info!("DingTalk Stream mode loop stopped");
        });

        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        Ok(Box::pin(stream))
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
        match &self.mode {
            DingTalkMode::Webhook { .. } => self.start_webhook().await,
            DingTalkMode::Stream { .. } => self.start_stream().await,
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
            // Use sessionWebhook (reply_url) if available (Stream mode), otherwise use access_token
            let url = if let Some(ref reply_url) = user.reply_url {
                info!("DingTalk: using sessionWebhook for reply (length={})", reply_url.len());
                reply_url.clone()
            } else {
                info!("DingTalk: using access_token for reply (Webhook mode)");
                self.build_send_url()
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

            // DingTalk returns {"errcode": 0, "errmsg": "ok"} on success
            let result: serde_json::Value = resp.json().await?;
            if result["errcode"].as_i64() != Some(0) {
                warn!("DingTalk send failed: {:?}", result);
                return Err(format!(
                    "DingTalk error: {}",
                    result["errmsg"].as_str().unwrap_or("unknown")
                )
                .into());
            }
            info!("DingTalk: message sent successfully");

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

/// Get the WebSocket URL for Stream mode.
///
/// Returns (websocket_url, token) on success.
async fn get_stream_websocket_url(
    client: &reqwest::Client,
    client_id: &str,
    client_secret: &str,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    // Get access token using client credentials
    let token_url = "https://api.dingtalk.com/v1.0/oauth2/accessToken";
    let token_body = serde_json::json!({
        "appKey": client_id,
        "appSecret": client_secret,
    });

    let token_resp: serde_json::Value = client
        .post(token_url)
        .json(&token_body)
        .send()
        .await?
        .json()
        .await?;

    let access_token = token_resp["accessToken"]
        .as_str()
        .ok_or("Missing accessToken in response")?
        .to_string();

    // Get WebSocket connection endpoint with subscriptions
    // According to DingTalk Stream protocol, subscriptions must be specified in connections.open request
    let conn_resp: serde_json::Value = client
        .post(DINGTALK_STREAM_OPEN_URL)
        .header("x-acs-dingtalk-access-token", &access_token)
        .json(&serde_json::json!({
            "clientId": client_id,
            "clientSecret": client_secret,
            "subscriptions": [
                {
                    "topic": "/v1.0/im/bot/messages/get",
                    "type": "CALLBACK"
                }
            ],
            "ua": "openfang-stream/1.0"
        }))
        .send()
        .await?
        .json()
        .await?;

    let endpoint = conn_resp["endpoint"]
        .as_str()
        .ok_or("Missing endpoint in connections.open response")?
        .to_string();
    // DingTalk returns "ticket", not "token"
    let ticket = conn_resp["ticket"]
        .as_str()
        .map(String::from)
        .unwrap_or_default();

    debug!("DingTalk Stream: connections.open response: {:?}", conn_resp);
    info!("DingTalk Stream: got endpoint={}, ticket length={}", endpoint, ticket.len());

    Ok((endpoint, ticket))
}

/// Parse a Stream mode callback into a ChannelMessage.
async fn parse_stream_callback(
    payload: &serde_json::Value,
    bot_union_id: &Arc<RwLock<Option<String>>>,
) -> Option<ChannelMessage> {
    // DingTalk Stream protocol: data field is a JSON string, need to parse it first
    let data_str = payload.get("data")?.as_str()?;
    let data: serde_json::Value = serde_json::from_str(data_str).ok()?;

    let msg_type = data["msgtype"].as_str()?;

    // Only handle text messages for now
    if msg_type != "text" {
        return None;
    }

    let text = data["text"]["content"].as_str()?.trim().to_string();
    if text.is_empty() {
        return None;
    }

    // Filter out bot's own messages
    let sender_union_id = data["senderUnionId"].as_str().unwrap_or("");
    if let Some(ref bot_id) = *bot_union_id.read().await {
        if sender_union_id == bot_id {
            return None;
        }
    }

    let sender_id = data["senderId"]
        .as_str()
        .or_else(|| data["staffId"].as_str())
        .unwrap_or("unknown")
        .to_string();
    let sender_nick = data["senderNick"]
        .as_str()
        .unwrap_or("Unknown")
        .to_string();
    let conversation_id = data["conversationId"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let conversation_type = data["conversationType"].as_str().unwrap_or("1");
    let is_group = conversation_type == "2";
    let msg_id = data["msgId"]
        .as_str()
        .unwrap_or(&format!("dt-{}", Utc::now().timestamp_millis()))
        .to_string();

    // Parse timestamp
    let timestamp = data["createA"]
        .as_i64()
        .or_else(|| data["createTime"].as_i64())
        .map(|ts| {
            chrono::DateTime::from_timestamp_millis(ts).unwrap_or_else(Utc::now)
        })
        .unwrap_or_else(Utc::now);

    // Parse commands (messages starting with /)
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

    let mut metadata = HashMap::new();
    if !conversation_id.is_empty() {
        metadata.insert(
            "conversation_id".to_string(),
            serde_json::Value::String(conversation_id),
        );
    }
    if !sender_union_id.is_empty() {
        metadata.insert(
            "sender_union_id".to_string(),
            serde_json::Value::String(sender_union_id.to_string()),
        );
    }

    // Extract sessionWebhook for Stream mode replies
    let session_webhook = data["sessionWebhook"]
        .as_str()
        .map(String::from);
    
    if let Some(ref webhook) = session_webhook {
        info!("DingTalk Stream: extracted sessionWebhook (length={})", webhook.len());
    } else {
        warn!("DingTalk Stream: no sessionWebhook in message, will fall back to access_token");
    }

    Some(ChannelMessage {
        channel: ChannelType::Custom("dingtalk".to_string()),
        platform_message_id: msg_id,
        sender: ChannelUser {
            platform_id: sender_id,
            display_name: sender_nick,
            openfang_user: None,
            reply_url: session_webhook,
        },
        content,
        target_agent: None,
        timestamp,
        is_group,
        thread_id: None,
        metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dingtalk_adapter_creation() {
        let adapter =
            DingTalkAdapter::new("test-token".to_string(), "test-secret".to_string(), 8080);
        assert_eq!(adapter.name(), "dingtalk");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("dingtalk".to_string())
        );
        assert_eq!(adapter.mode, DingTalkMode::Webhook { port: 8080 });
    }

    #[test]
    fn test_dingtalk_adapter_stream_creation() {
        let adapter = DingTalkAdapter::new_stream(
            "test-token".to_string(),
            "test-secret".to_string(),
            "client-id".to_string(),
            "client-secret".to_string(),
        );
        assert_eq!(adapter.name(), "dingtalk");
        assert_eq!(
            adapter.mode,
            DingTalkMode::Stream {
                client_id: "client-id".to_string(),
                client_secret: "client-secret".to_string(),
            }
        );
    }

    #[test]
    fn test_dingtalk_adapter_with_mode() {
        let adapter = DingTalkAdapter::with_mode(
            "token".to_string(),
            "secret".to_string(),
            DingTalkMode::Webhook { port: 9090 },
        );
        assert_eq!(adapter.mode, DingTalkMode::Webhook { port: 9090 });
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

    #[tokio::test]
    async fn test_parse_stream_callback_text() {
        let bot_union_id = Arc::new(RwLock::new(Some("bot-union-123".to_string())));
        let payload = serde_json::json!({
            "type": "callback",
            "data": {
                "msgtype": "text",
                "text": { "content": "Hello stream bot" },
                "senderId": "user456",
                "senderNick": "StreamUser",
                "senderUnionId": "user-union-789",
                "conversationId": "conv-stream-001",
                "conversationType": "2",
                "msgId": "msg-stream-001",
                "createTime": 1700000000000_i64,
            }
        });

        let msg = parse_stream_callback(&payload, &bot_union_id).await.unwrap();
        assert_eq!(msg.channel, ChannelType::Custom("dingtalk".to_string()));
        assert_eq!(msg.sender.display_name, "StreamUser");
        assert!(msg.is_group);
        assert!(matches!(msg.content, ChannelContent::Text(ref t) if t == "Hello stream bot"));
    }

    #[tokio::test]
    async fn test_parse_stream_callback_filters_bot() {
        let bot_union_id = Arc::new(RwLock::new(Some("bot-union-123".to_string())));
        let payload = serde_json::json!({
            "type": "callback",
            "data": {
                "msgtype": "text",
                "text": { "content": "Bot message" },
                "senderId": "bot-id",
                "senderNick": "Bot",
                "senderUnionId": "bot-union-123",
                "conversationId": "conv-001",
                "conversationType": "1",
            }
        });

        let msg = parse_stream_callback(&payload, &bot_union_id).await;
        assert!(msg.is_none());
    }

    #[tokio::test]
    async fn test_parse_stream_callback_command() {
        let bot_union_id = Arc::new(RwLock::new(None));
        let payload = serde_json::json!({
            "type": "callback",
            "data": {
                "msgtype": "text",
                "text": { "content": "/agent hello-world" },
                "senderId": "user1",
                "senderNick": "Commander",
                "senderUnionId": "union-1",
                "conversationId": "conv-1",
                "conversationType": "1",
            }
        });

        let msg = parse_stream_callback(&payload, &bot_union_id).await.unwrap();
        match &msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "agent");
                assert_eq!(args, &["hello-world"]);
            }
            other => panic!("Expected Command, got {other:?}"),
        }
    }

    #[test]
    fn test_dingtalk_mode_equality() {
        let webhook = DingTalkMode::Webhook { port: 8080 };
        let stream = DingTalkMode::Stream {
            client_id: "id".to_string(),
            client_secret: "secret".to_string(),
        };

        assert_eq!(webhook, DingTalkMode::Webhook { port: 8080 });
        assert_ne!(webhook, stream);
    }
}
