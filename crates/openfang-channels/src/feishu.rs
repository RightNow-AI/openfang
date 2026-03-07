//! Feishu/Lark Open Platform channel adapter.
//!
//! Uses the Feishu Open API for sending messages. Supports two modes for receiving inbound events:
//! 1. Webhook mode: HTTP server for receiving event callbacks
//! 2. WebSocket mode: WebSocket long connection for receiving events (no public IP required)
//!
//! Authentication is performed via a tenant access token obtained from
//! `https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal`.
//! The token is cached and refreshed automatically (2-hour expiry).

use crate::types::{
    split_message, ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser,
};
use async_trait::async_trait;
use chrono::Utc;
use futures::{SinkExt, Stream, StreamExt};
use prost::Message as ProstMessage;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch, RwLock};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{debug, error, info, warn};
use url::Url;
use zeroize::Zeroizing;

/// Feishu tenant access token endpoint.
const FEISHU_TOKEN_URL: &str =
    "https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal";

/// Feishu send message endpoint.
const FEISHU_SEND_URL: &str = "https://open.feishu.cn/open-apis/im/v1/messages";

/// Feishu bot info endpoint.
const FEISHU_BOT_INFO_URL: &str = "https://open.feishu.cn/open-apis/bot/v3/info";

/// Feishu websocket endpoint discovery API.
const FEISHU_WS_ENDPOINT_URL: &str = "https://open.feishu.cn/callback/ws/endpoint";

/// Maximum Feishu message text length (characters).
const MAX_MESSAGE_LEN: usize = 4096;

/// Token refresh buffer — refresh 5 minutes before actual expiry.
const TOKEN_REFRESH_BUFFER_SECS: u64 = 300;

const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(60);
const DEFAULT_WS_PING_INTERVAL_SECS: u64 = 30;

/// Feishu websocket frame header.
#[derive(Clone, PartialEq, ::prost::Message)]
struct FeishuWsHeader {
    #[prost(string, tag = "1")]
    key: String,
    #[prost(string, tag = "2")]
    value: String,
}

/// Feishu websocket frame (pbbp2.proto compatible).
#[derive(Clone, PartialEq, ::prost::Message)]
struct FeishuWsFrame {
    #[prost(uint64, tag = "1")]
    seq_id: u64,
    #[prost(uint64, tag = "2")]
    log_id: u64,
    #[prost(int32, tag = "3")]
    service: i32,
    #[prost(int32, tag = "4")]
    method: i32,
    #[prost(message, repeated, tag = "5")]
    headers: Vec<FeishuWsHeader>,
    #[prost(string, optional, tag = "6")]
    payload_encoding: Option<String>,
    #[prost(string, optional, tag = "7")]
    payload_type: Option<String>,
    #[prost(bytes, optional, tag = "8")]
    payload: Option<Vec<u8>>,
    #[prost(string, optional, tag = "9")]
    log_id_new: Option<String>,
}

#[derive(Debug, Clone)]
struct FeishuWsEndpoint {
    url: String,
    ping_interval_secs: u64,
}

/// Feishu connection mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeishuConnectionMode {
    /// Webhook mode: HTTP server receives event callbacks.
    Webhook,
    /// WebSocket mode: Long connection receives events (no public IP required).
    WebSocket,
}

/// Feishu/Lark Open Platform adapter.
///
/// Inbound messages arrive via either a webhook HTTP server or WebSocket long connection.
/// Outbound messages are sent via the Feishu IM API with a tenant access token for authentication.
pub struct FeishuAdapter {
    /// Feishu app ID.
    app_id: String,
    /// SECURITY: Feishu app secret, zeroized on drop.
    app_secret: Zeroizing<String>,
    /// Connection mode (Webhook or WebSocket).
    connection_mode: FeishuConnectionMode,
    /// Port on which the inbound webhook HTTP server listens (Webhook mode only).
    webhook_port: u16,
    /// Optional verification token for webhook event validation (Webhook mode only).
    verification_token: Option<String>,
    /// Optional encrypt key for webhook event decryption (Webhook mode only).
    encrypt_key: Option<String>,
    /// HTTP client for API calls.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Cached tenant access token and its expiry instant.
    cached_token: Arc<RwLock<Option<(String, Instant)>>>,
}

impl FeishuAdapter {
    /// Create a new Feishu adapter in Webhook mode.
    ///
    /// # Arguments
    /// * `app_id` - Feishu application ID.
    /// * `app_secret` - Feishu application secret.
    /// * `webhook_port` - Local port for the inbound webhook HTTP server.
    pub fn new(app_id: String, app_secret: String, webhook_port: u16) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            app_id,
            app_secret: Zeroizing::new(app_secret),
            connection_mode: FeishuConnectionMode::Webhook,
            webhook_port,
            verification_token: None,
            encrypt_key: None,
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            cached_token: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a new Feishu adapter in Webhook mode with verification.
    pub fn with_verification(
        app_id: String,
        app_secret: String,
        webhook_port: u16,
        verification_token: Option<String>,
        encrypt_key: Option<String>,
    ) -> Self {
        let mut adapter = Self::new(app_id, app_secret, webhook_port);
        adapter.verification_token = verification_token;
        adapter.encrypt_key = encrypt_key;
        adapter
    }

    /// Create a new Feishu adapter in WebSocket mode.
    ///
    /// WebSocket mode does not require a public IP or webhook configuration.
    ///
    /// # Arguments
    /// * `app_id` - Feishu application ID.
    /// * `app_secret` - Feishu application secret.
    pub fn new_websocket(app_id: String, app_secret: String) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            app_id,
            app_secret: Zeroizing::new(app_secret),
            connection_mode: FeishuConnectionMode::WebSocket,
            webhook_port: 0,
            verification_token: None,
            encrypt_key: None,
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            cached_token: Arc::new(RwLock::new(None)),
        }
    }

    /// Obtain a valid tenant access token, refreshing if expired or missing.
    async fn get_token(&self) -> Result<String, Box<dyn std::error::Error>> {
        {
            let guard = self.cached_token.read().await;
            if let Some((ref token, expiry)) = *guard {
                if Instant::now() < expiry {
                    return Ok(token.clone());
                }
            }
        }

        let body = serde_json::json!({
            "app_id": self.app_id,
            "app_secret": self.app_secret.as_str(),
        });

        let resp = self
            .client
            .post(FEISHU_TOKEN_URL)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let resp_body = resp.text().await.unwrap_or_default();
            return Err(format!("Feishu token request failed {status}: {resp_body}").into());
        }

        let resp_body: serde_json::Value = resp.json().await?;
        let code = resp_body["code"].as_i64().unwrap_or(-1);
        if code != 0 {
            let msg = resp_body["msg"].as_str().unwrap_or("unknown error");
            return Err(format!("Feishu token error: {msg}").into());
        }

        let tenant_access_token = resp_body["tenant_access_token"]
            .as_str()
            .ok_or("Missing tenant_access_token")?
            .to_string();
        let expire = resp_body["expire"].as_u64().unwrap_or(7200);

        let expiry =
            Instant::now() + Duration::from_secs(expire.saturating_sub(TOKEN_REFRESH_BUFFER_SECS));
        *self.cached_token.write().await = Some((tenant_access_token.clone(), expiry));

        Ok(tenant_access_token)
    }

    /// Validate credentials by fetching bot info.
    async fn validate(&self) -> Result<String, Box<dyn std::error::Error>> {
        let token = self.get_token().await?;

        let resp = self
            .client
            .get(FEISHU_BOT_INFO_URL)
            .bearer_auth(&token)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Feishu authentication failed {status}: {body}").into());
        }

        let body: serde_json::Value = resp.json().await?;
        let code = body["code"].as_i64().unwrap_or(-1);
        if code != 0 {
            let msg = body["msg"].as_str().unwrap_or("unknown error");
            return Err(format!("Feishu bot info error: {msg}").into());
        }

        let bot_name = body["bot"]["app_name"]
            .as_str()
            .unwrap_or("Feishu Bot")
            .to_string();
        Ok(bot_name)
    }

    /// Send a text message to a Feishu chat.
    async fn api_send_message(
        &self,
        receive_id: &str,
        receive_id_type: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let token = self.get_token().await?;
        let url = format!("{}?receive_id_type={}", FEISHU_SEND_URL, receive_id_type);

        let chunks = split_message(text, MAX_MESSAGE_LEN);

        for chunk in chunks {
            let content = serde_json::json!({
                "text": chunk,
            });

            let body = serde_json::json!({
                "receive_id": receive_id,
                "msg_type": "text",
                "content": content.to_string(),
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
                let resp_body = resp.text().await.unwrap_or_default();
                return Err(format!("Feishu send message error {status}: {resp_body}").into());
            }

            let resp_body: serde_json::Value = resp.json().await?;
            let code = resp_body["code"].as_i64().unwrap_or(-1);
            if code != 0 {
                let msg = resp_body["msg"].as_str().unwrap_or("unknown error");
                warn!("Feishu send message API error: {msg}");
            }
        }

        Ok(())
    }

    /// Send an interactive card with approve/reject buttons.
    async fn api_send_card(
        &self,
        receive_id: &str,
        request_id: &str,
        agent_id: &str,
        tool_name: &str,
        action_summary: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let token = self.get_token().await?;
        let url = format!("{}?receive_id_type=chat_id", FEISHU_SEND_URL);

        // Build interactive card JSON
        let card = serde_json::json!({
            "config": {
                "wide_screen_mode": true
            },
            "header": {
                "title": { "tag": "plain_text", "content": "⏳ 待审批请求" },
                "template": "blue"
            },
            "elements": [
                {
                    "tag": "div",
                    "text": {
                        "tag": "plain_text",
                        "content": format!("Agent: {}\n操作: {} — {}", agent_id, tool_name, action_summary)
                    }
                },
                {
                    "tag": "div",
                    "text": {
                        "tag": "plain_text",
                        "content": format!("ID: `{}`", &request_id[..8.min(request_id.len())]),
                        "language": "markdown"
                    }
                },
                {
                    "tag": "action",
                    "actions": [
                        {
                            "tag": "button",
                            "text": { "tag": "plain_text", "content": "✅ 批准" },
                            "type": "primary",
                            "value": { "action": "approve", "request_id": request_id }
                        },
                        {
                            "tag": "button",
                            "text": { "tag": "plain_text", "content": "❌ 拒绝" },
                            "type": "danger",
                            "value": { "action": "reject", "request_id": request_id }
                        }
                    ]
                }
            ]
        });

        let body = serde_json::json!({
            "receive_id": receive_id,
            "msg_type": "interactive",
            "content": card.to_string(),
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
            let resp_body = resp.text().await.unwrap_or_default();
            warn!("Feishu card send error {status}: {resp_body}");
            // Fall back to text message
            return self.api_send_message(
                receive_id,
                "chat_id",
                &format!("待审批请求 [{}]\nAgent: {}\n操作: {} — {}\n\n回复 /approve {} 或 /reject {}",
                    &request_id[..8.min(request_id.len())],
                    agent_id,
                    tool_name,
                    action_summary,
                    &request_id[..8.min(request_id.len())],
                    &request_id[..8.min(request_id.len())]
                )
            ).await;
        }
        Ok(())
    }

    /// Start webhook server (Webhook mode).
    async fn start_webhook(
        &self,
        tx: mpsc::Sender<ChannelMessage>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let port = self.webhook_port;
        let verification_token = self.verification_token.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let verification_token = Arc::new(verification_token);
            let tx = Arc::new(tx);

            let app = axum::Router::new().route(
                "/feishu/webhook",
                axum::routing::post({
                    let vt = Arc::clone(&verification_token);
                    let tx = Arc::clone(&tx);
                    move |body: axum::extract::Json<serde_json::Value>| {
                        let vt = Arc::clone(&vt);
                        let tx = Arc::clone(&tx);
                        async move {
                            if let Some(challenge) = body.0.get("challenge") {
                                if let Some(ref expected_token) = *vt {
                                    let token = body.0["token"].as_str().unwrap_or("");
                                    if token != expected_token {
                                        warn!("Feishu: invalid verification token");
                                        return (
                                            axum::http::StatusCode::FORBIDDEN,
                                            axum::Json(serde_json::json!({})),
                                        );
                                    }
                                }
                                return (
                                    axum::http::StatusCode::OK,
                                    axum::Json(serde_json::json!({
                                        "challenge": challenge,
                                    })),
                                );
                            }

                            let parsed = if let Some(schema) = body.0["schema"].as_str() {
                                if schema == "2.0" {
                                    if let Some(msg) = parse_feishu_event(&body.0) {
                                        let _ = tx.send(msg).await;
                                        true
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                }
                            } else {
                                let event_type = body.0["event"]["type"].as_str().unwrap_or("");
                                if event_type == "message" {
                                    let event = &body.0["event"];
                                    let text = event["text"].as_str().unwrap_or("");
                                    if !text.is_empty() {
                                        let open_id =
                                            event["open_id"].as_str().unwrap_or("").to_string();
                                        let chat_id = event["open_chat_id"]
                                            .as_str()
                                            .unwrap_or("")
                                            .to_string();
                                        let msg_id = event["open_message_id"]
                                            .as_str()
                                            .unwrap_or("")
                                            .to_string();
                                        let is_group =
                                            event["chat_type"].as_str().unwrap_or("") == "group";

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
                                            ChannelContent::Text(text.to_string())
                                        };

                                        let channel_msg = ChannelMessage {
                                            channel: ChannelType::Custom("feishu".to_string()),
                                            platform_message_id: msg_id,
                                            sender: ChannelUser {
                                                platform_id: chat_id,
                                                display_name: open_id,
                                                openfang_user: None,
                                            },
                                            content,
                                            target_agent: None,
                                            timestamp: Utc::now(),
                                            is_group,
                                            thread_id: None,
                                            metadata: HashMap::new(),
                                        };

                                        let _ = tx.send(channel_msg).await;
                                        true
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                }
                            };

                            (
                                axum::http::StatusCode::OK,
                                axum::Json(build_feishu_webhook_response(&body.0, parsed)),
                            )
                        }
                    }
                }),
            );

            let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
            info!("Feishu webhook server listening on {addr}");

            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    warn!("Feishu webhook bind failed: {e}");
                    return;
                }
            };

            let server = axum::serve(listener, app);

            tokio::select! {
                result = server => {
                    if let Err(e) = result {
                        warn!("Feishu webhook server error: {e}");
                    }
                }
                _ = shutdown_rx.changed() => {
                    info!("Feishu adapter shutting down");
                }
            }
        });

        Ok(())
    }

    /// Start WebSocket connection loop (WebSocket mode).
    async fn start_websocket_loop(
        &self,
        tx: mpsc::Sender<ChannelMessage>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let adapter = Arc::new(self.clone_adapter());

        tokio::spawn(async move {
            info!("Starting Feishu WebSocket mode");
            let mut shutdown_rx = adapter.shutdown_rx.clone();
            let mut backoff = INITIAL_BACKOFF;

            loop {
                if *shutdown_rx.borrow() {
                    break;
                }

                if let Err(e) = Self::run_websocket_inner(adapter.clone(), tx.clone()).await {
                    error!("Feishu WebSocket error: {e}");
                } else {
                    info!("Feishu WebSocket connection closed");
                }

                if *shutdown_rx.borrow() {
                    break;
                }

                warn!("Feishu WebSocket reconnecting in {backoff:?}");
                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {}
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            break;
                        }
                    }
                }

                backoff = (backoff * 2).min(MAX_BACKOFF);
            }

            info!("Feishu WebSocket loop stopped");
        });

        Ok(())
    }

    fn clone_adapter(&self) -> FeishuAdapterClone {
        FeishuAdapterClone {
            app_id: self.app_id.clone(),
            app_secret: self.app_secret.clone(),
            client: self.client.clone(),
            shutdown_rx: self.shutdown_rx.clone(),
        }
    }

    async fn run_websocket_inner(
        adapter: Arc<FeishuAdapterClone>,
        tx: mpsc::Sender<ChannelMessage>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let endpoint = adapter.get_websocket_endpoint().await?;
        let ws_url = endpoint.url;
        let service_id = parse_service_id(&ws_url);

        info!("Connecting to Feishu WebSocket endpoint: {ws_url}");
        let (ws_stream, _) = connect_async(&ws_url).await?;
        info!("Feishu WebSocket connected successfully");

        let (mut write, mut read) = ws_stream.split();
        let mut shutdown_rx = adapter.shutdown_rx.clone();
        let mut ping_interval =
            tokio::time::interval(Duration::from_secs(endpoint.ping_interval_secs));
        // consume first immediate tick
        ping_interval.tick().await;

        let mut frame_parts: HashMap<String, Vec<Vec<u8>>> = HashMap::new();

        loop {
            tokio::select! {
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Binary(data))) => {
                            let frame = match FeishuWsFrame::decode(data.as_slice()) {
                                Ok(f) => f,
                                Err(e) => {
                                    warn!("Feishu WS decode frame failed: {e}");
                                    continue;
                                }
                            };

                            match frame.method {
                                0 => {
                                    if let Some(new_interval) = parse_pong_interval(&frame) {
                                        if new_interval > 0 {
                                            debug!("Feishu WS update ping interval to {}s", new_interval);
                                            ping_interval = tokio::time::interval(Duration::from_secs(new_interval));
                                            ping_interval.tick().await;
                                        }
                                    }
                                }
                                1 => {
                                    Self::handle_data_frame(frame, &mut write, &tx, &mut frame_parts).await?;
                                }
                                method => {
                                    debug!("Feishu WS unhandled frame method: {method}");
                                }
                            }
                        }
                        Some(Ok(Message::Text(text))) => {
                            // Feishu WS should be binary protobuf frames; keep this for diagnostics.
                            debug!("Feishu WS unexpected text message: {text}");
                        }
                        Some(Ok(Message::Close(frame))) => {
                            info!("Feishu WebSocket closed by server: {frame:?}");
                            break;
                        }
                        Some(Ok(Message::Ping(payload))) => {
                            let _ = write.send(Message::Pong(payload)).await;
                        }
                        Some(Ok(Message::Pong(_))) => {
                            debug!("Feishu WebSocket pong");
                        }
                        Some(Ok(_)) => {}
                        Some(Err(e)) => return Err(format!("Feishu WebSocket stream error: {e}").into()),
                        None => break,
                    }
                }
                _ = ping_interval.tick() => {
                    let ping_frame = build_ping_frame(service_id);
                    write.send(Message::Binary(ping_frame.encode_to_vec())).await?;
                }
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        info!("Feishu WebSocket shutting down");
                        let _ = write.close().await;
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_data_frame<S>(
        mut frame: FeishuWsFrame,
        write: &mut S,
        tx: &mpsc::Sender<ChannelMessage>,
        frame_parts: &mut HashMap<String, Vec<Vec<u8>>>,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        S: SinkExt<Message> + Unpin,
        <S as futures::Sink<Message>>::Error: std::error::Error + Send + Sync + 'static,
    {
        let frame_type = ws_header(&frame.headers, "type").unwrap_or_default();
        if frame_type != "event" && frame_type != "card" {
            return Ok(());
        }

        let payload = match frame.payload.take() {
            Some(p) => p,
            None => return Ok(()),
        };

        let payload = match combine_payload(&frame.headers, payload, frame_parts) {
            Some(p) => p,
            None => return Ok(()),
        };

        let mut code = 200;
        match serde_json::from_slice::<serde_json::Value>(&payload) {
            Ok(event) => {
                if let Some(msg) = parse_feishu_event(&event) {
                    if tx.send(msg).await.is_err() {
                        return Ok(());
                    }
                }
            }
            Err(e) => {
                warn!("Feishu WS event payload parse failed: {e}");
                code = 500;
            }
        }

        let ack_frame = build_ack_frame(&frame, code);
        write
            .send(Message::Binary(ack_frame.encode_to_vec()))
            .await?;
        Ok(())
    }
}

/// Cloneable Feishu adapter parts for use in async tasks.
struct FeishuAdapterClone {
    app_id: String,
    app_secret: Zeroizing<String>,
    client: reqwest::Client,
    shutdown_rx: watch::Receiver<bool>,
}

impl FeishuAdapterClone {
    /// Get WebSocket endpoint from Feishu API.
    async fn get_websocket_endpoint(&self) -> Result<FeishuWsEndpoint, Box<dyn std::error::Error>> {
        let resp = self
            .client
            .post(FEISHU_WS_ENDPOINT_URL)
            .json(&serde_json::json!({
                "AppID": self.app_id,
                "AppSecret": self.app_secret.as_str(),
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let resp_body = resp.text().await.unwrap_or_default();
            return Err(
                format!("Feishu WebSocket endpoint request failed {status}: {resp_body}").into(),
            );
        }

        let resp_body: serde_json::Value = resp.json().await?;
        parse_ws_endpoint_response(&resp_body)
    }
}

fn parse_ws_endpoint_response(
    resp_body: &serde_json::Value,
) -> Result<FeishuWsEndpoint, Box<dyn std::error::Error>> {
    let code = resp_body["code"].as_i64().unwrap_or(-1);
    if code != 0 {
        let msg = resp_body["msg"].as_str().unwrap_or("unknown error");
        return Err(format!("Feishu WebSocket endpoint error: {msg}").into());
    }

    let data = &resp_body["data"];
    let ws_url = data
        .get("url")
        .or_else(|| data.get("URL"))
        .and_then(|v| v.as_str())
        .ok_or("Missing WebSocket URL in response")?
        .to_string();

    let ping_interval = data
        .get("client_config")
        .or_else(|| data.get("ClientConfig"))
        .and_then(|cfg| cfg.get("ping_interval").or_else(|| cfg.get("PingInterval")))
        .and_then(|v| v.as_u64())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_WS_PING_INTERVAL_SECS);

    Ok(FeishuWsEndpoint {
        url: ws_url,
        ping_interval_secs: ping_interval,
    })
}

fn parse_service_id(ws_url: &str) -> i32 {
    Url::parse(ws_url)
        .ok()
        .and_then(|u| {
            u.query_pairs()
                .find(|(k, _)| k == "service_id")
                .and_then(|(_, v)| v.parse::<i32>().ok())
        })
        .unwrap_or(0)
}

fn ws_header(headers: &[FeishuWsHeader], key: &str) -> Option<String> {
    headers
        .iter()
        .find(|h| h.key == key)
        .map(|h| h.value.clone())
}

fn parse_pong_interval(frame: &FeishuWsFrame) -> Option<u64> {
    let frame_type = ws_header(&frame.headers, "type")?;
    if frame_type != "pong" {
        return None;
    }

    let payload = frame.payload.as_ref()?;
    let value: serde_json::Value = serde_json::from_slice(payload).ok()?;
    value
        .get("ping_interval")
        .or_else(|| value.get("PingInterval"))
        .and_then(|v| v.as_u64())
}

fn combine_payload(
    headers: &[FeishuWsHeader],
    payload: Vec<u8>,
    frame_parts: &mut HashMap<String, Vec<Vec<u8>>>,
) -> Option<Vec<u8>> {
    let sum = ws_header(headers, "sum")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1);
    if sum <= 1 {
        return Some(payload);
    }

    let seq = ws_header(headers, "seq")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let msg_id = ws_header(headers, "message_id")?;

    if seq >= sum {
        return None;
    }

    let entry = frame_parts
        .entry(msg_id.clone())
        .or_insert_with(|| vec![Vec::new(); sum]);

    if entry.len() != sum {
        *entry = vec![Vec::new(); sum];
    }

    entry[seq] = payload;

    if entry.iter().any(|part| part.is_empty()) {
        return None;
    }

    let mut combined = Vec::new();
    for part in entry.iter() {
        combined.extend_from_slice(part);
    }
    frame_parts.remove(&msg_id);
    Some(combined)
}

fn build_ping_frame(service_id: i32) -> FeishuWsFrame {
    FeishuWsFrame {
        seq_id: 0,
        log_id: 0,
        service: service_id,
        method: 0,
        headers: vec![FeishuWsHeader {
            key: "type".to_string(),
            value: "ping".to_string(),
        }],
        payload_encoding: None,
        payload_type: None,
        payload: None,
        log_id_new: None,
    }
}

fn build_ack_frame(request: &FeishuWsFrame, code: u16) -> FeishuWsFrame {
    let payload = serde_json::json!({
        "code": code,
        "headers": {},
        "data": []
    });

    FeishuWsFrame {
        seq_id: request.seq_id,
        log_id: request.log_id,
        service: request.service,
        method: request.method,
        headers: request.headers.clone(),
        payload_encoding: None,
        payload_type: None,
        payload: Some(serde_json::to_vec(&payload).unwrap_or_default()),
        log_id_new: request.log_id_new.clone(),
    }
}

/// Parse a Feishu webhook event into a `ChannelMessage`.
fn parse_feishu_event(event: &serde_json::Value) -> Option<ChannelMessage> {
    parse_feishu_text_message_event(event).or_else(|| parse_feishu_card_action_event(event))
}

fn parse_feishu_text_message_event(event: &serde_json::Value) -> Option<ChannelMessage> {
    let header = event.get("header")?;
    let event_type = header["event_type"].as_str().unwrap_or("");

    if event_type != "im.message.receive_v1" {
        return None;
    }

    let event_data = event.get("event")?;
    let message = event_data.get("message")?;
    let sender = event_data.get("sender")?;

    let msg_type = message["message_type"].as_str().unwrap_or("");
    if msg_type != "text" {
        return None;
    }

    let content_str = message["content"].as_str().unwrap_or("{}");
    let content_json: serde_json::Value = serde_json::from_str(content_str).unwrap_or_default();
    let text = content_json["text"].as_str().unwrap_or("");
    if text.is_empty() {
        return None;
    }

    let message_id = message["message_id"].as_str().unwrap_or("").to_string();
    let chat_id = message["chat_id"].as_str().unwrap_or("").to_string();
    let chat_type = message["chat_type"].as_str().unwrap_or("p2p");
    let root_id = message["root_id"].as_str().map(|s| s.to_string());

    let sender_id = sender
        .get("sender_id")
        .and_then(|s| s.get("open_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let sender_type = sender["sender_type"].as_str().unwrap_or("user");

    if sender_type == "bot" {
        return None;
    }

    let is_group = chat_type == "group";

    let msg_content = if text.starts_with('/') {
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
        "chat_id".to_string(),
        serde_json::Value::String(chat_id.clone()),
    );
    metadata.insert(
        "message_id".to_string(),
        serde_json::Value::String(message_id.clone()),
    );
    metadata.insert(
        "chat_type".to_string(),
        serde_json::Value::String(chat_type.to_string()),
    );
    metadata.insert(
        "sender_id".to_string(),
        serde_json::Value::String(sender_id.clone()),
    );
    if let Some(mentions) = message.get("mentions") {
        metadata.insert("mentions".to_string(), mentions.clone());
    }

    Some(ChannelMessage {
        channel: ChannelType::Custom("feishu".to_string()),
        platform_message_id: message_id,
        sender: ChannelUser {
            platform_id: chat_id,
            display_name: sender_id,
            openfang_user: None,
        },
        content: msg_content,
        target_agent: None,
        timestamp: Utc::now(),
        is_group,
        thread_id: root_id,
        metadata,
    })
}

fn parse_feishu_card_action_event(event: &serde_json::Value) -> Option<ChannelMessage> {
    let event_type = event
        .get("header")
        .and_then(|h| h.get("event_type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if event_type != "application.bot.menu_v6" && event_type != "card.action.trigger" {
        return None;
    }

    let action_value = event
        .get("event")?
        .get("action")?
        .get("value")?;

    let action = action_value.get("action")?.as_str()?;
    if action != "approve" && action != "reject" {
        return None;
    }

    let request_id = action_value.get("request_id")?.as_str()?;
    if request_id.is_empty() {
        return None;
    }

    let event_data = event.get("event")?;
    let context = event_data.get("context");

    let chat_id = event_data
        .get("open_chat_id")
        .or_else(|| context.and_then(|c| c.get("open_chat_id")))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let message_id = event_data
        .get("open_message_id")
        .or_else(|| context.and_then(|c| c.get("open_message_id")))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let operator_id = event_data
        .get("operator")
        .and_then(|v| {
            // card.action.trigger: operator.open_id
            // application.bot.menu_v6: operator.operator_id.open_id
            v.get("open_id")
                .or_else(|| v.get("operator_id").and_then(|oid| oid.get("open_id")))
        })
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mut metadata = HashMap::new();
    metadata.insert(
        "event_source".to_string(),
        serde_json::Value::String("feishu_card_action".to_string()),
    );
    if !message_id.is_empty() {
        metadata.insert(
            "open_message_id".to_string(),
            serde_json::Value::String(message_id.clone()),
        );
    }
    if !operator_id.is_empty() {
        metadata.insert(
            "sender_id".to_string(),
            serde_json::Value::String(operator_id.clone()),
        );
    }
    if !chat_id.is_empty() {
        metadata.insert(
            "chat_id".to_string(),
            serde_json::Value::String(chat_id.clone()),
        );
    }

    Some(ChannelMessage {
        channel: ChannelType::Custom("feishu".to_string()),
        platform_message_id: if message_id.is_empty() {
            format!("card-action-{action}-{request_id}")
        } else {
            message_id
        },
        sender: ChannelUser {
            platform_id: chat_id,
            display_name: operator_id,
            openfang_user: None,
        },
        content: ChannelContent::Command {
            name: action.to_string(),
            args: vec![request_id.to_string()],
        },
        target_agent: None,
        timestamp: Utc::now(),
        is_group: true,
        thread_id: None,
        metadata,
    })
}

fn build_feishu_webhook_response(body: &serde_json::Value, parsed: bool) -> serde_json::Value {
    if !parsed {
        return serde_json::json!({});
    }

    let event_type = body
        .get("header")
        .and_then(|h| h.get("event_type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if event_type == "application.bot.menu_v6" || event_type == "card.action.trigger" {
        serde_json::json!({ "code": 0 })
    } else {
        serde_json::json!({})
    }
}

#[async_trait]
impl ChannelAdapter for FeishuAdapter {
    fn name(&self) -> &str {
        "feishu"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("feishu".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        let bot_name = self.validate().await?;
        info!("Feishu adapter authenticated as {bot_name}");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);

        match self.connection_mode {
            FeishuConnectionMode::Webhook => self.start_webhook(tx).await?,
            FeishuConnectionMode::WebSocket => self.start_websocket_loop(tx).await?,
        }

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn send(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match content {
            ChannelContent::Text(text) => {
                self.api_send_message(&user.platform_id, "chat_id", &text)
                    .await?;
            }
            ChannelContent::ApprovalRequest {
                request_id,
                agent_id,
                tool_name,
                action_summary,
            } => {
                self.api_send_card(
                    &user.platform_id,
                    &request_id,
                    &agent_id,
                    &tool_name,
                    &action_summary,
                )
                .await?;
            }
            _ => {
                self.api_send_message(&user.platform_id, "chat_id", "(Unsupported content type)")
                    .await?;
            }
        }
        Ok(())
    }

    async fn send_typing(&self, _user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
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

    fn header(key: &str, value: &str) -> FeishuWsHeader {
        FeishuWsHeader {
            key: key.to_string(),
            value: value.to_string(),
        }
    }

    #[test]
    fn test_feishu_adapter_creation() {
        let adapter =
            FeishuAdapter::new("cli_abc123".to_string(), "app-secret-456".to_string(), 9000);
        assert_eq!(adapter.name(), "feishu");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("feishu".to_string())
        );
        assert_eq!(adapter.webhook_port, 9000);
        assert_eq!(adapter.connection_mode, FeishuConnectionMode::Webhook);
    }

    #[test]
    fn test_feishu_websocket_adapter_creation() {
        let adapter =
            FeishuAdapter::new_websocket("cli_abc123".to_string(), "app-secret-456".to_string());
        assert_eq!(adapter.name(), "feishu");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("feishu".to_string())
        );
        assert_eq!(adapter.connection_mode, FeishuConnectionMode::WebSocket);
    }

    #[test]
    fn test_feishu_with_verification() {
        let adapter = FeishuAdapter::with_verification(
            "cli_abc123".to_string(),
            "secret".to_string(),
            9000,
            Some("verify-token".to_string()),
            Some("encrypt-key".to_string()),
        );
        assert_eq!(adapter.verification_token, Some("verify-token".to_string()));
        assert_eq!(adapter.encrypt_key, Some("encrypt-key".to_string()));
    }

    #[test]
    fn test_feishu_app_id_stored() {
        let adapter = FeishuAdapter::new("cli_test".to_string(), "secret".to_string(), 8080);
        assert_eq!(adapter.app_id, "cli_test");
    }

    #[test]
    fn test_parse_ws_endpoint_response_lowercase() {
        let body = serde_json::json!({
            "code": 0,
            "msg": "ok",
            "data": {
                "url": "wss://example/ws?service_id=123",
                "client_config": {
                    "ping_interval": 42
                }
            }
        });

        let endpoint = parse_ws_endpoint_response(&body).unwrap();
        assert_eq!(endpoint.url, "wss://example/ws?service_id=123");
        assert_eq!(endpoint.ping_interval_secs, 42);
    }

    #[test]
    fn test_parse_ws_endpoint_response_uppercase() {
        let body = serde_json::json!({
            "code": 0,
            "msg": "ok",
            "data": {
                "URL": "wss://example/ws?service_id=321",
                "ClientConfig": {
                    "PingInterval": 24
                }
            }
        });

        let endpoint = parse_ws_endpoint_response(&body).unwrap();
        assert_eq!(endpoint.url, "wss://example/ws?service_id=321");
        assert_eq!(endpoint.ping_interval_secs, 24);
    }

    #[test]
    fn test_combine_payload_multi_package() {
        let mut frame_parts = HashMap::new();

        let headers_1 = vec![
            header("message_id", "msg-1"),
            header("sum", "2"),
            header("seq", "0"),
        ];
        let headers_2 = vec![
            header("message_id", "msg-1"),
            header("sum", "2"),
            header("seq", "1"),
        ];

        let r1 = combine_payload(&headers_1, b"Hello ".to_vec(), &mut frame_parts);
        assert!(r1.is_none());
        let r2 = combine_payload(&headers_2, b"World".to_vec(), &mut frame_parts).unwrap();
        assert_eq!(r2, b"Hello World".to_vec());
    }

    #[test]
    fn test_parse_service_id() {
        assert_eq!(parse_service_id("wss://foo/bar?service_id=123"), 123);
        assert_eq!(parse_service_id("wss://foo/bar"), 0);
    }

    #[test]
    fn test_parse_feishu_event_v2_text() {
        let event = serde_json::json!({
            "schema": "2.0",
            "header": {
                "event_id": "evt-001",
                "event_type": "im.message.receive_v1",
                "create_time": "1234567890000",
                "token": "verify-token",
                "app_id": "cli_abc123",
                "tenant_key": "tenant-key-1"
            },
            "event": {
                "sender": {
                    "sender_id": {
                        "open_id": "ou_abc123",
                        "user_id": "user-1"
                    },
                    "sender_type": "user"
                },
                "message": {
                    "message_id": "om_abc123",
                    "root_id": null,
                    "chat_id": "oc_chat123",
                    "chat_type": "p2p",
                    "message_type": "text",
                    "content": "{\"text\":\"Hello from Feishu!\"}"
                }
            }
        });

        let msg = parse_feishu_event(&event).unwrap();
        assert_eq!(msg.channel, ChannelType::Custom("feishu".to_string()));
        assert_eq!(msg.platform_message_id, "om_abc123");
        assert!(!msg.is_group);
        assert!(matches!(msg.content, ChannelContent::Text(ref t) if t == "Hello from Feishu!"));
    }

    #[test]
    fn test_parse_feishu_event_group_message() {
        let event = serde_json::json!({
            "schema": "2.0",
            "header": {
                "event_id": "evt-002",
                "event_type": "im.message.receive_v1"
            },
            "event": {
                "sender": {
                    "sender_id": {
                        "open_id": "ou_abc123"
                    },
                    "sender_type": "user"
                },
                "message": {
                    "message_id": "om_grp1",
                    "chat_id": "oc_grp123",
                    "chat_type": "group",
                    "message_type": "text",
                    "content": "{\"text\":\"Group message\"}"
                }
            }
        });

        let msg = parse_feishu_event(&event).unwrap();
        assert!(msg.is_group);
    }

    #[test]
    fn test_parse_feishu_event_command() {
        let event = serde_json::json!({
            "schema": "2.0",
            "header": {
                "event_id": "evt-003",
                "event_type": "im.message.receive_v1"
            },
            "event": {
                "sender": {
                    "sender_id": {
                        "open_id": "ou_abc123"
                    },
                    "sender_type": "user"
                },
                "message": {
                    "message_id": "om_cmd1",
                    "chat_id": "oc_chat1",
                    "chat_type": "p2p",
                    "message_type": "text",
                    "content": "{\"text\":\"/help all\"}"
                }
            }
        });

        let msg = parse_feishu_event(&event).unwrap();
        match &msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "help");
                assert_eq!(args, &["all"]);
            }
            other => panic!("Expected Command, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_feishu_event_skips_bot() {
        let event = serde_json::json!({
            "schema": "2.0",
            "header": {
                "event_id": "evt-004",
                "event_type": "im.message.receive_v1"
            },
            "event": {
                "sender": {
                    "sender_id": {
                        "open_id": "ou_bot"
                    },
                    "sender_type": "bot"
                },
                "message": {
                    "message_id": "om_bot1",
                    "chat_id": "oc_chat1",
                    "chat_type": "p2p",
                    "message_type": "text",
                    "content": "{\"text\":\"Bot message\"}"
                }
            }
        });

        assert!(parse_feishu_event(&event).is_none());
    }

    #[test]
    fn test_parse_feishu_event_non_text() {
        let event = serde_json::json!({
            "schema": "2.0",
            "header": {
                "event_id": "evt-005",
                "event_type": "im.message.receive_v1"
            },
            "event": {
                "sender": {
                    "sender_id": {
                        "open_id": "ou_user1"
                    },
                    "sender_type": "user"
                },
                "message": {
                    "message_id": "om_img1",
                    "chat_id": "oc_chat1",
                    "chat_type": "p2p",
                    "message_type": "image",
                    "content": "{\"image_key\":\"img_v2_abc123\"}"
                }
            }
        });

        assert!(parse_feishu_event(&event).is_none());
    }

    #[test]
    fn test_parse_feishu_event_wrong_type() {
        let event = serde_json::json!({
            "schema": "2.0",
            "header": {
                "event_id": "evt-006",
                "event_type": "im.chat.member_bot.added_v1"
            },
            "event": {}
        });

        assert!(parse_feishu_event(&event).is_none());
    }

    #[test]
    fn test_parse_feishu_event_thread_id() {
        let event = serde_json::json!({
            "schema": "2.0",
            "header": {
                "event_id": "evt-007",
                "event_type": "im.message.receive_v1"
            },
            "event": {
                "sender": {
                    "sender_id": {
                        "open_id": "ou_user1"
                    },
                    "sender_type": "user"
                },
                "message": {
                    "message_id": "om_thread1",
                    "root_id": "om_root1",
                    "chat_id": "oc_chat1",
                    "chat_type": "group",
                    "message_type": "text",
                    "content": "{\"text\":\"Thread reply\"}"
                }
            }
        });

        let msg = parse_feishu_event(&event).unwrap();
        assert_eq!(msg.thread_id, Some("om_root1".to_string()));
    }

    #[test]
    fn test_parse_feishu_event_text_command_message() {
        let event = serde_json::json!({
            "header": { "event_type": "im.message.receive_v1" },
            "event": {
                "message": {
                    "message_type": "text",
                    "message_id": "om_x",
                    "chat_id": "oc_x",
                    "chat_type": "group",
                    "content": "{\"text\":\"/approve abc123\"}"
                },
                "sender": {
                    "sender_id": { "open_id": "ou_x" },
                    "sender_type": "user"
                }
            }
        });

        let msg = parse_feishu_event(&event).expect("message parsed");
        match msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "approve");
                assert_eq!(args, vec!["abc123"]);
            }
            other => panic!("unexpected content: {other:?}"),
        }
    }

    #[test]
    fn test_parse_feishu_event_card_approve_action() {
        let event = serde_json::json!({
            "schema": "2.0",
            "header": { "event_type": "application.bot.menu_v6" },
            "event": {
                "operator": {
                    "operator_id": { "open_id": "ou_operator" }
                },
                "token": "card_callback_token",
                "open_message_id": "om_card",
                "open_chat_id": "oc_group",
                "action": {
                    "value": {
                        "action": "approve",
                        "request_id": "550e8400-e29b-41d4-a716-446655440000"
                    }
                }
            }
        });

        let msg = parse_feishu_event(&event).expect("card callback parsed");
        match msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "approve");
                assert_eq!(args, vec!["550e8400-e29b-41d4-a716-446655440000"]);
            }
            other => panic!("unexpected content: {other:?}"),
        }
        assert_eq!(msg.sender.platform_id, "oc_group");
    }

    #[test]
    fn test_parse_feishu_event_card_reject_action() {
        let event = serde_json::json!({
            "schema": "2.0",
            "header": { "event_type": "application.bot.menu_v6" },
            "event": {
                "operator": {
                    "operator_id": { "open_id": "ou_operator" }
                },
                "open_message_id": "om_card",
                "open_chat_id": "oc_group",
                "action": {
                    "value": {
                        "action": "reject",
                        "request_id": "deadbeef"
                    }
                }
            }
        });

        let msg = parse_feishu_event(&event).expect("card callback parsed");
        match msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "reject");
                assert_eq!(args, vec!["deadbeef"]);
            }
            other => panic!("unexpected content: {other:?}"),
        }
    }

    #[test]
    fn test_parse_feishu_event_card_action_preserves_metadata() {
        let event = serde_json::json!({
            "schema": "2.0",
            "header": { "event_type": "application.bot.menu_v6" },
            "event": {
                "operator": {
                    "operator_id": { "open_id": "ou_operator" }
                },
                "open_message_id": "om_card",
                "open_chat_id": "oc_group",
                "action": {
                    "value": {
                        "action": "reject",
                        "request_id": "deadbeef"
                    }
                }
            }
        });

        let msg = parse_feishu_event(&event).expect("callback parsed");
        assert_eq!(msg.metadata["open_message_id"], "om_card");
        assert_eq!(msg.metadata["event_source"], "feishu_card_action");
    }

    #[test]
    fn test_parse_feishu_event_card_action_invalid_payload() {
        let event = serde_json::json!({
            "schema": "2.0",
            "header": { "event_type": "application.bot.menu_v6" },
            "event": {
                "operator": {
                    "operator_id": { "open_id": "ou_operator" }
                },
                "open_chat_id": "oc_group",
                "action": {
                    "value": {
                        "action": "approve"
                    }
                }
            }
        });

        assert!(parse_feishu_event(&event).is_none());
    }

    #[test]
    fn test_feishu_webhook_response_for_card_action_acknowledges_success() {
        let event = serde_json::json!({
            "schema": "2.0",
            "header": { "event_type": "application.bot.menu_v6" },
            "event": {
                "action": { "value": { "action": "approve", "request_id": "abc123" } },
                "open_chat_id": "oc_group"
            }
        });

        let response = build_feishu_webhook_response(&event, true);
        assert_eq!(response["code"], 0);
    }

    #[test]
    fn test_feishu_webhook_response_for_text_event_is_empty() {
        let event = serde_json::json!({
            "schema": "2.0",
            "header": { "event_type": "im.message.receive_v1" },
            "event": {
                "message": {
                    "message_type": "text",
                    "message_id": "om_x",
                    "chat_id": "oc_x",
                    "chat_type": "group",
                    "content": "{\"text\":\"hello\"}"
                },
                "sender": {
                    "sender_id": { "open_id": "ou_x" },
                    "sender_type": "user"
                }
            }
        });

        let response = build_feishu_webhook_response(&event, true);
        assert_eq!(response, serde_json::json!({}));
    }

    #[test]
    fn test_parse_feishu_event_card_action_trigger_approve() {
        let event = serde_json::json!({
            "schema": "2.0",
            "header": { "event_type": "card.action.trigger" },
            "event": {
                "operator": {
                    "operator_id": { "open_id": "ou_operator" }
                },
                "open_message_id": "om_card",
                "open_chat_id": "oc_group",
                "action": {
                    "value": {
                        "action": "approve",
                        "request_id": "abc123"
                    }
                }
            }
        });

        let msg = parse_feishu_event(&event).expect("callback parsed");
        match msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "approve");
                assert_eq!(args, vec!["abc123"]);
            }
            other => panic!("unexpected content: {other:?}"),
        }
    }

    #[test]
    fn test_parse_feishu_event_card_action_trigger_preserves_metadata() {
        let event = serde_json::json!({
            "schema": "2.0",
            "header": { "event_type": "card.action.trigger" },
            "event": {
                "operator": {
                    "operator_id": { "open_id": "ou_operator" }
                },
                "open_message_id": "om_card",
                "open_chat_id": "oc_group",
                "action": {
                    "value": {
                        "action": "reject",
                        "request_id": "deadbeef"
                    }
                }
            }
        });

        let msg = parse_feishu_event(&event).expect("callback parsed");
        assert_eq!(msg.metadata["event_source"], "feishu_card_action");
        assert_eq!(msg.metadata["open_message_id"], "om_card");
    }

    #[test]
    fn test_parse_feishu_event_ignores_unknown_card_callback_type() {
        let event = serde_json::json!({
            "schema": "2.0",
            "header": { "event_type": "card.action.unknown" },
            "event": {
                "action": {
                    "value": {
                        "action": "approve",
                        "request_id": "abc123"
                    }
                }
            }
        });

        assert!(parse_feishu_event(&event).is_none());
    }

    #[test]
    fn test_feishu_webhook_response_for_card_action_trigger() {
        let event = serde_json::json!({
            "schema": "2.0",
            "header": { "event_type": "card.action.trigger" },
            "event": {
                "action": { "value": { "action": "approve", "request_id": "abc123" } },
                "open_chat_id": "oc_group"
            }
        });

        let response = build_feishu_webhook_response(&event, true);
        assert_eq!(response["code"], 0);
    }

    #[test]
    fn test_build_ack_frame_returns_empty_success_payload() {
        let request = FeishuWsFrame {
            seq_id: 1,
            log_id: 2,
            service: 3,
            method: 1,
            headers: vec![header("type", "event")],
            payload_encoding: None,
            payload_type: None,
            payload: None,
            log_id_new: None,
        };

        let frame = build_ack_frame(&request, 200);
        let payload: serde_json::Value =
            serde_json::from_slice(frame.payload.as_ref().unwrap()).unwrap();
        assert_eq!(payload["code"], 200);
        assert_eq!(payload["data"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn test_handle_data_frame_dispatches_card_action_trigger() {
        use prost::Message as ProstMessage;

        let card_event = serde_json::json!({
            "schema": "2.0",
            "header": { "event_type": "card.action.trigger" },
            "event": {
                "operator": { "operator_id": { "open_id": "ou_op" } },
                "open_message_id": "om_card",
                "open_chat_id": "oc_chat",
                "action": {
                    "value": {
                        "action": "approve",
                        "request_id": "req_ws_1"
                    }
                }
            }
        });
        let payload_bytes = serde_json::to_vec(&card_event).unwrap();

        let frame = FeishuWsFrame {
            seq_id: 10,
            log_id: 20,
            service: 1,
            method: 0,
            headers: vec![header("type", "card")],
            payload_encoding: None,
            payload_type: None,
            payload: Some(payload_bytes),
            log_id_new: None,
        };

        let (tx, mut rx) = mpsc::channel::<ChannelMessage>(16);
        let mut frame_parts = HashMap::new();

        // Use futures unbounded channel as sink for ACK frames
        let (mut ws_tx, mut ws_rx) =
            futures::channel::mpsc::unbounded::<tokio_tungstenite::tungstenite::Message>();

        FeishuAdapter::handle_data_frame(frame, &mut ws_tx, &tx, &mut frame_parts)
            .await
            .expect("handle_data_frame should succeed");

        // Verify: message dispatched to channel
        let msg = rx.try_recv().expect("should have received a channel message");
        match msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "approve");
                assert_eq!(args, vec!["req_ws_1"]);
            }
            other => panic!("unexpected content: {other:?}"),
        }

        // Verify: ACK frame was sent back
        let ack_msg = ws_rx.try_recv().expect("should have ACK frame");
        if let tokio_tungstenite::tungstenite::Message::Binary(data) = ack_msg {
            let ack_frame = FeishuWsFrame::decode(data.as_ref()).unwrap();
            let ack_payload: serde_json::Value =
                serde_json::from_slice(ack_frame.payload.as_ref().unwrap()).unwrap();
            assert_eq!(ack_payload["code"], 200);
        } else {
            panic!("expected binary ACK message");
        }
    }

    #[test]
    fn test_parse_feishu_event_card_action_trigger_context_nested() {
        // Real card.action.trigger events nest open_chat_id under event.context
        let event = serde_json::json!({
            "schema": "2.0",
            "header": { "event_type": "card.action.trigger" },
            "event": {
                "operator": {
                    "open_id": "ou_operator"
                },
                "action": {
                    "value": {
                        "action": "approve",
                        "request_id": "real_req_1"
                    },
                    "tag": "button"
                },
                "host": "im_message",
                "context": {
                    "open_message_id": "om_real_card",
                    "open_chat_id": "oc_real_chat"
                }
            }
        });

        let msg = parse_feishu_event(&event).expect("callback parsed");
        match msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "approve");
                assert_eq!(args, vec!["real_req_1"]);
            }
            other => panic!("unexpected content: {other:?}"),
        }
        // sender.platform_id must be the chat_id for send() to work
        assert_eq!(msg.sender.platform_id, "oc_real_chat");
        assert_eq!(msg.metadata["open_message_id"], "om_real_card");
        assert_eq!(msg.metadata["event_source"], "feishu_card_action");
    }
}
