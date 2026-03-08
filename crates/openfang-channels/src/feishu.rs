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
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch, RwLock};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{debug, error, info, warn};
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

    /// Start webhook server (Webhook mode).
    async fn start_webhook(&self, tx: mpsc::Sender<ChannelMessage>) -> Result<(), Box<dyn std::error::Error>> {
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

                            if let Some(schema) = body.0["schema"].as_str() {
                                if schema == "2.0" {
                                    if let Some(msg) = parse_feishu_event(&body.0) {
                                        let _ = tx.send(msg).await;
                                    }
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

        Ok(())
    }

    /// Start WebSocket connection loop (WebSocket mode).
    async fn start_websocket_loop(&self, tx: mpsc::Sender<ChannelMessage>) -> Result<(), Box<dyn std::error::Error>> {
        let self_arc = Arc::new(self.clone_adapter());
        
        tokio::spawn(async move {
            info!("Starting Feishu WebSocket mode");
            let mut backoff = Duration::from_secs(1);
            let max_backoff = Duration::from_secs(60);
            
            loop {
                match Self::run_websocket_inner(self_arc.clone(), tx.clone()).await {
                    Ok(_) => {
                        info!("Feishu WebSocket connection closed, reconnecting...");
                    }
                    Err(e) => {
                        error!("Feishu WebSocket error: {e}, reconnecting in {backoff:?}");
                    }
                }
                
                tokio::time::sleep(backoff).await;
                backoff = std::cmp::min(backoff * 2, max_backoff);
            }
        });

        Ok(())
    }

    /// Clone adapter for use in async tasks.
    fn clone_adapter(&self) -> FeishuAdapterClone {
        FeishuAdapterClone {
            app_id: self.app_id.clone(),
            app_secret: self.app_secret.clone(),
            client: self.client.clone(),
            cached_token: self.cached_token.clone(),
            shutdown_rx: self.shutdown_rx.clone(),
        }
    }

    /// Run WebSocket connection loop (inner implementation).
    async fn run_websocket_inner(
        adapter: Arc<FeishuAdapterClone>,
        tx: mpsc::Sender<ChannelMessage>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let ws_url = adapter.get_websocket_endpoint().await?;
        info!("Connecting to Feishu WebSocket endpoint: {ws_url}");

        let (ws_stream, _) = connect_async(ws_url).await?;
        info!("Feishu WebSocket connected successfully");

        let (mut write, mut read) = ws_stream.split();
        let mut shutdown_rx = adapter.shutdown_rx.clone();

        loop {
            tokio::select! {
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            debug!("Received Feishu WebSocket message: {text}");
                            if let Ok(event) = serde_json::from_str::<serde_json::Value>(&text) {
                                if let Some(msg) = parse_feishu_event(&event) {
                                    let _ = tx.send(msg).await;
                                }
                            }
                        }
                        Some(Ok(Message::Binary(data))) => {
                            debug!("Received Feishu WebSocket binary message: {} bytes", data.len());
                            if let Ok(text) = String::from_utf8(data) {
                                if let Ok(event) = serde_json::from_str::<serde_json::Value>(&text) {
                                    if let Some(msg) = parse_feishu_event(&event) {
                                        let _ = tx.send(msg).await;
                                    }
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            info!("Feishu WebSocket connection closed by server");
                            break;
                        }
                        Some(Ok(Message::Ping(_))) => {
                            debug!("Received Feishu WebSocket ping, sending pong");
                            let _ = write.send(Message::Pong(Vec::new())).await;
                        }
                        Some(Ok(Message::Pong(_))) => {
                            debug!("Received Feishu WebSocket pong");
                        }
                        Some(Ok(_)) => {
                            debug!("Received unhandled Feishu WebSocket message type");
                        }
                        Some(Err(e)) => {
                            error!("Feishu WebSocket error: {e}");
                            break;
                        }
                        None => {
                            info!("Feishu WebSocket stream ended");
                            break;
                        }
                    }
                }
                _ = shutdown_rx.changed() => {
                    info!("Feishu WebSocket shutting down");
                    let _ = write.close().await;
                    break;
                }
            }
        }

        Ok(())
    }
}

/// Cloneable Feishu adapter parts for use in async tasks.
struct FeishuAdapterClone {
    app_id: String,
    app_secret: Zeroizing<String>,
    client: reqwest::Client,
    cached_token: Arc<RwLock<Option<(String, Instant)>>>,
    shutdown_rx: watch::Receiver<bool>,
}

impl FeishuAdapterClone {
    /// Get a valid tenant access token, refreshing if expired or missing.
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

    /// Get WebSocket endpoint from Feishu API.
    async fn get_websocket_endpoint(&self) -> Result<String, Box<dyn std::error::Error>> {
        let token = self.get_token().await?;
        let url = "https://open.feishu.cn/open-apis/ws/v1/endpoint";
        
        let resp = self
            .client
            .get(url)
            .bearer_auth(&token)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let resp_body = resp.text().await.unwrap_or_default();
            return Err(format!("Feishu WebSocket endpoint request failed {status}: {resp_body}").into());
        }

        let resp_body: serde_json::Value = resp.json().await?;
        let code = resp_body["code"].as_i64().unwrap_or(-1);
        if code != 0 {
            let msg = resp_body["msg"].as_str().unwrap_or("unknown error");
            return Err(format!("Feishu WebSocket endpoint error: {msg}").into());
        }

        let ws_url = resp_body["data"]["url"]
            .as_str()
            .ok_or("Missing WebSocket URL in response")?
            .to_string();

        Ok(ws_url)
    }
}

/// Parse a Feishu webhook event into a `ChannelMessage`.
///
/// Handles `im.message.receive_v1` events with text message type.
fn parse_feishu_event(event: &serde_json::Value) -> Option<ChannelMessage> {
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
            FeishuConnectionMode::Webhook => {
                self.start_webhook(tx).await?;
            }
            FeishuConnectionMode::WebSocket => {
                self.start_websocket_loop(tx).await?;
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
        let adapter = FeishuAdapter::new_websocket(
            "cli_abc123".to_string(),
            "app-secret-456".to_string(),
        );
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
