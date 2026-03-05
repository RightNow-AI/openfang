//! Feishu/Lark Open Platform channel adapter.
//!
//! Supports two connection modes:
//! - **Webhook mode**: HTTP server for receiving events (requires public URL)
//! - **WebSocket mode**: Long connection to Feishu server (works from local environment)
//!
//! Authentication is performed via a tenant access token obtained from
//! `https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal`.
//! The token is cached and refreshed automatically (2-hour expiry).

use crate::types::{
    split_message, ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser,
};
use async_trait::async_trait;
use chrono::Utc;
use flate2::read::GzDecoder;
use futures::Stream;
use openfang_types::config::FeishuConnectionMode;
use std::collections::HashMap;
use std::io::Read;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch, RwLock};
use tracing::{debug, error, info, trace, warn};
use zeroize::Zeroizing;

/// Feishu tenant access token endpoint.
const FEISHU_TOKEN_URL: &str =
    "https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal";

/// Feishu send message endpoint.
const FEISHU_SEND_URL: &str = "https://open.feishu.cn/open-apis/im/v1/messages";

/// Feishu bot info endpoint.
const FEISHU_BOT_INFO_URL: &str = "https://open.feishu.cn/open-apis/bot/v3/info";

/// Maximum Feishu message text length (characters).
const MAX_MESSAGE_LEN: usize = 4096;

/// Token refresh buffer — refresh 5 minutes before actual expiry.
const TOKEN_REFRESH_BUFFER_SECS: u64 = 300;

/// Feishu/Lark Open Platform adapter.
///
/// Inbound messages arrive via a webhook HTTP server that receives event
/// callbacks from the Feishu platform, or via WebSocket long connection.
/// Outbound messages are sent via the Feishu IM API with a tenant access
/// token for authentication.
pub struct FeishuAdapter {
    /// Feishu app ID.
    app_id: String,
    /// SECURITY: Feishu app secret, zeroized on drop.
    app_secret: Zeroizing<String>,
    /// Connection mode (webhook or WebSocket).
    connection_mode: FeishuConnectionMode,
    /// Port on which the inbound webhook HTTP server listens.
    webhook_port: u16,
    /// Optional verification token for webhook event validation.
    verification_token: Option<String>,
    /// Optional encrypt key for webhook event decryption.
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
    /// Create a new Feishu adapter.
    ///
    /// # Arguments
    /// * `app_id` - Feishu application ID.
    /// * `app_secret` - Feishu application secret.
    /// * `connection_mode` - Connection mode (webhook or WebSocket).
    pub fn new(app_id: String, app_secret: String, connection_mode: FeishuConnectionMode) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            app_id,
            app_secret: Zeroizing::new(app_secret),
            connection_mode,
            webhook_port: 8453,
            verification_token: None,
            encrypt_key: None,
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            cached_token: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a new Feishu adapter with webhook verification.
    pub fn with_verification(
        app_id: String,
        app_secret: String,
        connection_mode: FeishuConnectionMode,
        webhook_port: u16,
        verification_token: Option<String>,
        encrypt_key: Option<String>,
    ) -> Self {
        let mut adapter = Self::new(app_id, app_secret, connection_mode);
        adapter.webhook_port = webhook_port;
        adapter.verification_token = verification_token;
        adapter.encrypt_key = encrypt_key;
        adapter
    }

    /// Obtain a valid tenant access token, refreshing if expired or missing.
    async fn get_token(&self) -> Result<String, Box<dyn std::error::Error>> {
        // Check cache first
        {
            let guard = self.cached_token.read().await;
            if let Some((ref token, expiry)) = *guard {
                if Instant::now() < expiry {
                    return Ok(token.clone());
                }
            }
        }

        // Fetch a new tenant access token
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

        // Cache with safety buffer
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

    /// Run WebSocket long connection to Feishu server.
    async fn run_websocket(
        &self,
        tx: mpsc::Sender<ChannelMessage>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Feishu: Initializing WebSocket long connection");

        // Use the correct WebSocket protocol implementation
        start_feishu_websocket(
            self.app_id.clone(),
            self.app_secret.as_str().to_string(),
            tx,
        )
        .await
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

    /// Reply to a message in a thread.
    #[allow(dead_code)]
    async fn api_reply_message(
        &self,
        message_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let token = self.get_token().await?;
        let url = format!(
            "https://open.feishu.cn/open-apis/im/v1/messages/{}/reply",
            message_id
        );

        let content = serde_json::json!({
            "text": text,
        });

        let body = serde_json::json!({
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
            return Err(format!("Feishu reply message error {status}: {resp_body}").into());
        }

        Ok(())
    }
}

/// Parse a Feishu webhook event into a `ChannelMessage`.
///
/// Handles `im.message.receive_v1` events with text message type.
fn parse_feishu_event(event: &serde_json::Value) -> Option<ChannelMessage> {
    // Feishu v2 event schema
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

    // Parse the content JSON string
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

    // Skip bot messages
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
        // Validate credentials
        let bot_name = self.validate().await?;
        info!("Feishu adapter authenticated as {bot_name}");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let mut shutdown_rx = self.shutdown_rx.clone();

        // Route based on connection mode
        match self.connection_mode {
            FeishuConnectionMode::WebSocket => {
                // WebSocket long connection mode
                info!("Feishu: Using WebSocket long connection mode");
                let adapter_clone = FeishuAdapter {
                    app_id: self.app_id.clone(),
                    app_secret: Zeroizing::new(self.app_secret.as_str().to_string()),
                    connection_mode: self.connection_mode,
                    webhook_port: self.webhook_port,
                    verification_token: self.verification_token.clone(),
                    encrypt_key: self.encrypt_key.clone(),
                    client: self.client.clone(),
                    shutdown_tx: self.shutdown_tx.clone(),
                    shutdown_rx: self.shutdown_rx.clone(),
                    cached_token: self.cached_token.clone(),
                };

                tokio::spawn(async move {
                    info!("Feishu: Starting WebSocket connection task");

                    // Auto-reconnect loop
                    loop {
                        tokio::select! {
                            result = adapter_clone.run_websocket(tx.clone()) => {
                                match result {
                                    Ok(_) => {
                                        info!("Feishu: WebSocket connection ended normally");
                                    }
                                    Err(e) => {
                                        error!("Feishu: WebSocket error: {e}");
                                    }
                                }
                                // Check if we should stop reconnecting
                                if *shutdown_rx.borrow() {
                                    info!("Feishu: Shutdown requested, not reconnecting");
                                    break;
                                }
                                // Wait before reconnecting
                                info!("Feishu: Reconnecting in 5 seconds...");
                                tokio::time::sleep(Duration::from_secs(5)).await;
                            }
                            _ = shutdown_rx.changed() => {
                                info!("Feishu: WebSocket shutdown requested");
                                break;
                            }
                        }
                    }
                });
            }
            FeishuConnectionMode::Webhook => {
                // HTTP webhook mode (default)
                info!("Feishu: Using HTTP webhook mode");
                let port = self.webhook_port;
                let verification_token = self.verification_token.clone();

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
                                    // Handle URL verification challenge
                                    if let Some(challenge) = body.0.get("challenge") {
                                        // Verify token if configured
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

                                    // Handle event callback
                                    if let Some(schema) = body.0["schema"].as_str() {
                                        if schema == "2.0" {
                                            // V2 event format
                                            if let Some(msg) = parse_feishu_event(&body.0) {
                                                let _ = tx.send(msg).await;
                                            }
                                        }
                                    } else {
                                        // V1 event format (legacy)
                                        let event_type = body.0["event"]["type"].as_str().unwrap_or("");
                                        if event_type == "message" {
                                            // Legacy format handling
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
                                            }
                                        }
                                    }

                                    (
                                        axum::http::StatusCode::OK,
                                        axum::Json(serde_json::json!({})),
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
            }
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
                // Use chat_id as receive_id with chat_id type
                self.api_send_message(&user.platform_id, "chat_id", &text)
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
        // Feishu does not support typing indicators via REST API
        Ok(())
    }

    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.shutdown_tx.send(true);
        Ok(())
    }
}

// ============================================================================
// Feishu WebSocket Long Connection Implementation
// ============================================================================

use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};
use futures::{stream::StreamExt, sink::SinkExt};

// Protobuf definitions for Feishu WebSocket protocol
pub mod pbbp2 {
    use bytes::Buf;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Header {
        pub key: String,
        pub value: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Frame {
        pub SeqID: u32,
        pub LogID: u32,
        pub service: u32,
        pub method: u32,
        pub headers: Vec<Header>,
        pub payload_encoding: String,
        pub payload_type: String,
        pub payload: Vec<u8>,
        pub LogIDNew: String,
    }

    impl Default for Frame {
        fn default() -> Self {
            Self {
                SeqID: 0,
                LogID: 0,
                service: 0,
                method: 0,
                headers: Vec::new(),
                payload_encoding: String::new(),
                payload_type: String::new(),
                payload: Vec::new(),
                LogIDNew: String::new(),
            }
        }
    }

    // Simple protobuf encoding helpers
    fn encode_varint(mut value: u64, buf: &mut Vec<u8>) {
        while value >= 0x80 {
            buf.push((value as u8) | 0x80);
            value >>= 7;
        }
        buf.push(value as u8);
    }

    fn encode_tag(field_number: u32, wire_type: u32, buf: &mut Vec<u8>) {
        encode_varint((field_number as u64) << 3 | (wire_type as u64), buf);
    }

    fn encode_string(field_number: u32, value: &str, buf: &mut Vec<u8>) {
        let bytes = value.as_bytes();
        encode_tag(field_number, 2, buf); // Length-delimited
        encode_varint(bytes.len() as u64, buf);
        buf.extend_from_slice(bytes);
    }

    fn encode_bytes(field_number: u32, value: &[u8], buf: &mut Vec<u8>) {
        encode_tag(field_number, 2, buf); // Length-delimited
        encode_varint(value.len() as u64, buf);
        buf.extend_from_slice(value);
    }

    fn encode_uint32(field_number: u32, value: u32, buf: &mut Vec<u8>) {
        if value == 0 {
            return;
        }
        encode_tag(field_number, 0, buf); // Varint
        encode_varint(value as u64, buf);
    }

    fn encode_header(field_number: u32, header: &Header, buf: &mut Vec<u8>) {
        let mut nested = Vec::new();
        encode_string(1, &header.key, &mut nested);
        encode_string(2, &header.value, &mut nested);
        encode_bytes(field_number, &nested, buf);
    }

    impl Frame {
        pub fn encode(&self) -> Vec<u8> {
            let mut buf = Vec::new();

            encode_uint32(1, self.SeqID, &mut buf);
            encode_uint32(2, self.LogID, &mut buf);
            encode_uint32(3, self.service, &mut buf);
            encode_uint32(4, self.method, &mut buf);

            for header in &self.headers {
                encode_header(5, header, &mut buf);
            }

            if !self.payload_encoding.is_empty() {
                encode_string(6, &self.payload_encoding, &mut buf);
            }
            if !self.payload_type.is_empty() {
                encode_string(7, &self.payload_type, &mut buf);
            }
            if !self.payload.is_empty() {
                encode_bytes(8, &self.payload, &mut buf);
            }
            if !self.LogIDNew.is_empty() {
                encode_string(9, &self.LogIDNew, &mut buf);
            }

            buf
        }

        pub fn decode(data: &[u8]) -> Result<Self, String> {
            let mut frame = Frame::default();
            let mut cursor = std::io::Cursor::new(data);

            while cursor.has_remaining() {
                let tag = read_varint(&mut cursor)?;
                let field_number = (tag >> 3) as u32;
                let wire_type = (tag & 0x07) as u32;

                match field_number {
                    1 => frame.SeqID = read_varint(&mut cursor)? as u32,
                    2 => frame.LogID = read_varint(&mut cursor)? as u32,
                    3 => frame.service = read_varint(&mut cursor)? as u32,
                    4 => frame.method = read_varint(&mut cursor)? as u32,
                    5 => {
                        let len = read_varint(&mut cursor)? as usize;
                        let start = cursor.position() as usize;
                        let mut header = Header { key: String::new(), value: String::new() };

                        while (cursor.position() as usize - start) < len {
                            let tag = read_varint(&mut cursor)?;
                            let field_number = (tag >> 3) as u32;

                            if field_number == 1 {
                                let str_len = read_varint(&mut cursor)? as usize;
                                header.key = String::from_utf8_lossy(&data[cursor.position() as usize..cursor.position() as usize + str_len]).to_string();
                                cursor.advance(str_len);
                            } else if field_number == 2 {
                                let str_len = read_varint(&mut cursor)? as usize;
                                header.value = String::from_utf8_lossy(&data[cursor.position() as usize..cursor.position() as usize + str_len]).to_string();
                                cursor.advance(str_len);
                            }
                        }
                        frame.headers.push(header);
                    }
                    6 => {
                        let len = read_varint(&mut cursor)? as usize;
                        let pos = cursor.position() as usize;
                        frame.payload_encoding = String::from_utf8_lossy(&data[pos..pos + len]).to_string();
                        cursor.advance(len);
                    }
                    7 => {
                        let len = read_varint(&mut cursor)? as usize;
                        let pos = cursor.position() as usize;
                        frame.payload_type = String::from_utf8_lossy(&data[pos..pos + len]).to_string();
                        cursor.advance(len);
                    }
                    8 => {
                        let len = read_varint(&mut cursor)? as usize;
                        let pos = cursor.position() as usize;
                        frame.payload = data[pos..pos + len].to_vec();
                        cursor.advance(len);
                    }
                    9 => {
                        let len = read_varint(&mut cursor)? as usize;
                        let pos = cursor.position() as usize;
                        frame.LogIDNew = String::from_utf8_lossy(&data[pos..pos + len]).to_string();
                        cursor.advance(len);
                    }
                    _ => {
                        // Skip unknown field
                        if wire_type == 2 {
                            let len = read_varint(&mut cursor)? as usize;
                            cursor.advance(len);
                        }
                    }
                }
            }

            Ok(frame)
        }
    }

    fn read_varint<B: Buf>(cursor: &mut B) -> Result<u64, String> {
        let mut result = 0;
        let mut shift = 0;

        loop {
            if !cursor.has_remaining() {
                return Err("Unexpected end of buffer".to_string());
            }

            let byte = cursor.get_u8();
            result |= ((byte & 0x7F) as u64) << shift;

            if byte & 0x80 == 0 {
                break;
            }

            shift += 7;
            if shift >= 64 {
                return Err("Varint too large".to_string());
            }
        }

        Ok(result)
    }
}

const WS_CONFIG_ENDPOINT: &str = "https://open.feishu.cn/callback/ws/endpoint";
const FRAME_TYPE_CONTROL: u32 = 0;
const FRAME_TYPE_DATA: u32 = 1;

#[derive(Debug, Clone, serde::Deserialize)]
struct WsConfigResponse {
    code: i32,
    data: WsConfigData,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct WsConfigData {
    URL: String,
    #[serde(rename = "ClientConfig")]
    client_config: ClientConfig,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ClientConfig {
    #[serde(rename = "PingInterval")]
    ping_interval: u64,
    #[serde(rename = "ReconnectCount")]
    reconnect_count: i32,
    #[serde(rename = "ReconnectInterval")]
    reconnect_interval: u64,
    #[serde(rename = "ReconnectNonce")]
    reconnect_nonce: u32,
}

/// Cache for fragmented message data
#[derive(Debug, Default)]
struct DataCache {
    fragments: HashMap<u32, Vec<Vec<u8>>>,
    expected_seqs: HashMap<u32, u32>,
}

impl DataCache {
    fn add_fragment(&mut self, seq: u32, data: Vec<u8>) -> Option<Vec<u8>> {
        self.fragments.entry(seq).or_default().push(data);
        None
    }

    fn complete(&mut self, seq: u32) -> Option<Vec<u8>> {
        if let Some(fragments) = self.fragments.remove(&seq) {
            let total_len = fragments.iter().map(|f| f.len()).sum();
            let mut result = Vec::with_capacity(total_len);
            for fragment in fragments {
                result.extend_from_slice(&fragment);
            }
            Some(result)
        } else {
            None
        }
    }
}

/// Extract service_id from WebSocket URL
fn extract_service_id(ws_url: &str) -> Option<String> {
    url::Url::parse(ws_url)
        .ok()?
        .query_pairs()
        .find(|(k, _)| k == "service_id")
        .map(|(_, v)| v.to_string())
}

/// Create a protobuf frame for WebSocket communication
fn create_frame(
    seq_id: u32,
    service: u32,
    method: u32,
    headers: Vec<pbbp2::Header>,
    payload: Vec<u8>,
) -> Vec<u8> {
    let frame = pbbp2::Frame {
        SeqID: seq_id,
        LogID: 0,
        service,
        method,
        headers,
        payload_encoding: "gzip".to_string(),
        payload_type: "json".to_string(),
        payload,
        LogIDNew: String::new(),
    };

    frame.encode()
}

/// Send ping frame to keep connection alive
fn send_ping(seq_id: u32, service_id: &str) -> Vec<u8> {
    let headers = vec![
        pbbp2::Header {
            key: "type".to_string(),
            value: "ping".to_string(),
        },
        pbbp2::Header {
            key: "service_id".to_string(),
            value: service_id.to_string(),
        },
    ];

    create_frame(seq_id, 1, FRAME_TYPE_CONTROL, headers, vec![])
}

/// Start Feishu WebSocket long connection
pub async fn start_feishu_websocket(
    app_id: String,
    app_secret: String,
    mut tx: mpsc::Sender<ChannelMessage>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("Feishu: Fetching WebSocket configuration");

    // Step 1: Fetch WebSocket configuration
    let client = reqwest::Client::new();
    let config_response = client
        .post(WS_CONFIG_ENDPOINT)
        .json(&serde_json::json!({
            "AppID": app_id,
            "AppSecret": app_secret
        }))
        .send()
        .await?;

    if !config_response.status().is_success() {
        return Err(format!("Failed to fetch WebSocket config: {}", config_response.status()).into());
    }

    let config_data: WsConfigResponse = config_response.json().await?;
    if config_data.code != 0 {
        return Err(format!("WebSocket config returned error code: {}", config_data.code).into());
    }

    let ws_url = config_data.data.URL;
    let ping_interval = config_data.data.client_config.ping_interval;
    info!("Feishu: Connecting to WebSocket at {} (ping_interval: {}s)", ws_url, ping_interval);

    // Extract service_id from URL
    let service_id = extract_service_id(&ws_url)
        .ok_or("Failed to extract service_id from WebSocket URL")?;

    // Step 2: Connect to WebSocket
    let (ws_stream, _) = connect_async(&ws_url).await?;
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Step 3: Send initial handshake
    let seq_id = 1u32;
    let headers = vec![
        pbbp2::Header {
            key: "type".to_string(),
            value: "register".to_string(),
        },
        pbbp2::Header {
            key: "service_id".to_string(),
            value: service_id.clone(),
        },
    ];

    let register_frame = create_frame(
        seq_id,
        1,
        FRAME_TYPE_CONTROL,
        headers,
        vec![],
    );

    ws_sender.send(WsMessage::Binary(register_frame)).await?;
    info!("Feishu: WebSocket registration sent");

    // Step 4: Create channel for ping sending
    let (ping_tx, mut ping_rx) = mpsc::channel::<Vec<u8>>(10);

    // Spawn ping task
    let service_id_clone = service_id.clone();
    tokio::spawn(async move {
        info!("Feishu: Ping task started, interval={}s", ping_interval);
        let mut seq = 2u32;
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(ping_interval)).await;
            info!("Feishu: Sending ping frame (seq={})", seq);
            let ping = send_ping(seq, &service_id_clone);
            if ping_tx.send(ping).await.is_err() {
                error!("Feishu: Failed to send ping to channel");
                break;
            }
            seq = seq.wrapping_add(1);
        }
    });

    // Step 5: Message loop with ping handling
    let mut data_cache = DataCache::default();

    info!("Feishu: Entering WebSocket message loop");
    let mut loop_count = 0u64;

    loop {
        loop_count += 1;
        if loop_count % 100 == 0 {
            debug!("Feishu: Message loop iteration {}", loop_count);
        }

        tokio::select! {
            // Handle WebSocket messages
            msg_result = ws_receiver.next() => {
                debug!("Feishu: ws_receiver.next() returned");
                match msg_result {
                    Some(Ok(WsMessage::Binary(data))) => {
                        info!("Feishu: Received binary WebSocket message, {} bytes", data.len());
                        // Decode protobuf frame
                        match pbbp2::Frame::decode(&*data) {
                            Ok(frame) => {
                                info!("Feishu: Received frame: service={}, method={}", frame.service, frame.method);

                                // Handle different frame types
                                if frame.method == FRAME_TYPE_CONTROL {
                                    // Control message (ping/pong/register response)
                                    let frame_type = frame.headers.iter()
                                        .find(|h| h.key == "type")
                                        .map(|h| h.value.as_str());

                                    match frame_type {
                                        Some("pong") => {
                                            debug!("Feishu: Received pong");
                                        }
                                        Some("register") => {
                                            info!("Feishu: Registration confirmed");
                                        }
                                        _ => {
                                            debug!("Feishu: Unknown control frame type: {:?}", frame_type);
                                        }
                                    }
                                } else if frame.method == FRAME_TYPE_DATA {
                                    // Data message (actual event)
                                    if !frame.payload.is_empty() {
                                        // Check for fragmented messages
                                        let sum = frame.headers.iter()
                                            .find(|h| h.key == "sum")
                                            .and_then(|h| h.value.parse::<u32>().ok());

                                        let current_seq = frame.SeqID;

                                        if let Some(total) = sum {
                                            if total > 1 {
                                                // Fragmented message
                                                data_cache.add_fragment(current_seq, frame.payload.to_vec());

                                                if let Some(complete) = data_cache.complete(current_seq) {
                                                    // All fragments received, process the complete message
                                                    if let Err(e) = process_event_payload(&complete, &mut tx).await {
                                                        error!("Feishu: Failed to process event: {}", e);
                                                    }
                                                }
                                            } else {
                                                // Single fragment
                                                if let Err(e) = process_event_payload(&frame.payload, &mut tx).await {
                                                    error!("Feishu: Failed to process event: {}", e);
                                                }
                                            }
                                        } else {
                                            // No sum header, treat as complete message
                                            if let Err(e) = process_event_payload(&frame.payload, &mut tx).await {
                                                error!("Feishu: Failed to process event: {}", e);
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Feishu: Failed to decode protobuf frame: {}", e);
                            }
                        }
                    }
                    Some(Ok(WsMessage::Close(_))) => {
                        info!("Feishu: WebSocket closed by server");
                        break;
                    }
                    Some(Ok(WsMessage::Ping(data))) => {
                        info!("Feishu: Received WebSocket ping, {} bytes", data.len());
                        ws_sender.send(WsMessage::Pong(data)).await?;
                    }
                    Some(Ok(WsMessage::Pong(_))) => {
                        info!("Feishu: Received WebSocket pong");
                    }
                    Some(Ok(WsMessage::Text(text))) => {
                        info!("Feishu: Received unexpected text message: {}", text);
                    }
                    Some(Err(e)) => {
                        error!("Feishu: WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        warn!("Feishu: WebSocket channel closed (None received)");
                        break;
                    }
                    _ => {
                        warn!("Feishu: Unhandled WebSocket message type");
                    }
                }
            }
            // Handle ping messages from the ping task
            Some(ping_data) = ping_rx.recv() => {
                debug!("Feishu: Sending ping frame to WebSocket");
                if let Err(e) = ws_sender.send(WsMessage::Binary(ping_data)).await {
                    error!("Feishu: Failed to send ping: {}", e);
                    break;
                }
            }
            // Both channels closed, exit loop
            else => {
                warn!("Feishu: Both WebSocket and ping channels closed");
                break;
            }
        }
    }

    info!("Feishu: WebSocket connection ended");
    Ok(())
}

/// Process an event payload from Feishu
async fn process_event_payload(
    payload: &[u8],
    tx: &mut mpsc::Sender<ChannelMessage>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Try to decompress if gzipped
    let json_str = if is_gzipped(payload) {
        let mut decoder = GzDecoder::new(payload);
        let mut decompressed = String::new();
        decoder.read_to_string(&mut decompressed)?;
        decompressed
    } else {
        String::from_utf8(payload.to_vec())?
    };

    info!("Feishu: Event payload: {}", json_str);

    // Parse as JSON to extract event details
    let event: serde_json::Value = serde_json::from_str(&json_str)?;

    // Convert to ChannelMessage
    if let Err(e) = convert_feishu_event(&event, tx).await {
        error!("Feishu: Failed to convert event: {}", e);
    }

    Ok(())
}

/// Check if bytes are gzipped
fn is_gzipped(data: &[u8]) -> bool {
    data.len() > 1 && data[0] == 0x1f && data[1] == 0x8b
}

/// Convert Feishu event to ChannelMessage
async fn convert_feishu_event(
    event: &serde_json::Value,
    tx: &mut mpsc::Sender<ChannelMessage>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // This is a simplified conversion - real implementation would handle all event types
    let header = event.get("header")
        .and_then(|v| v.as_object())
        .ok_or("Missing event header")?;

    let event_type = header.get("event_type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    if event_type == "im.message.receive_v1" {
        let event_data = event.get("event")
            .and_then(|v| v.as_object())
            .ok_or("Missing event data")?;

        let message = event_data.get("message")
            .and_then(|v| v.as_object())
            .ok_or("Missing message")?;

        let chat_id = message.get("chat_id")
            .and_then(|v| v.as_str())
            .ok_or("Missing chat_id")?;

        let content = message.get("content")
            .and_then(|v| v.as_str())
            .ok_or("Missing content")?;

        let sender = event_data.get("sender")
            .and_then(|v| v.as_object())
            .ok_or("Missing sender")?;

        let sender_id = sender.get("sender_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let user = ChannelUser {
            platform_id: chat_id.to_string(),  // Use chat_id so responses go to the right place
            display_name: sender.get("nickname")
                .and_then(|v| v.as_str())
                .unwrap_or(sender_id)
                .to_string(),
            openfang_user: None,
        };

        let msg = ChannelMessage {
            channel: ChannelType::Custom("feishu".to_string()),
            platform_message_id: message.get("message_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            sender: user,
            content: ChannelContent::Text(content.to_string()),
            target_agent: None,
            timestamp: chrono::Utc::now(),
            is_group: false,
            thread_id: None,
            metadata: HashMap::new(),
        };

        tx.send(msg).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feishu_adapter_creation() {
        let adapter =
            FeishuAdapter::new("cli_abc123".to_string(), "app-secret-456".to_string(), FeishuConnectionMode::Webhook);
        assert_eq!(adapter.name(), "feishu");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("feishu".to_string())
        );
        assert_eq!(adapter.webhook_port, 8453); // default
    }

    #[test]
    fn test_feishu_adapter_websocket_mode() {
        let adapter =
            FeishuAdapter::new("cli_abc123".to_string(), "app-secret-456".to_string(), FeishuConnectionMode::WebSocket);
        assert_eq!(adapter.connection_mode, FeishuConnectionMode::WebSocket);
    }

    #[test]
    fn test_feishu_with_verification() {
        let adapter = FeishuAdapter::with_verification(
            "cli_abc123".to_string(),
            "secret".to_string(),
            FeishuConnectionMode::Webhook,
            9000,
            Some("verify-token".to_string()),
            Some("encrypt-key".to_string()),
        );
        assert_eq!(adapter.verification_token, Some("verify-token".to_string()));
        assert_eq!(adapter.encrypt_key, Some("encrypt-key".to_string()));
        assert_eq!(adapter.webhook_port, 9000);
    }

    #[test]
    fn test_feishu_app_id_stored() {
        let adapter = FeishuAdapter::new("cli_test".to_string(), "secret".to_string(), FeishuConnectionMode::Webhook);
        assert_eq!(adapter.app_id, "cli_test");
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
}
