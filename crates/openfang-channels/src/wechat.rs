//! WeChat iLink Bot channel adapter.
//!
//! Uses the iLink Bot API with QR-code login and long polling. The bot token
//! and sync cursor are stored under `state_dir` so the daemon can resume
//! without re-scanning after the first successful login.

use crate::types::{
    ChannelAdapter, ChannelContent, ChannelMessage, ChannelStatus, ChannelType, ChannelUser,
};
use async_trait::async_trait;
use chrono::Utc;
use futures::Stream;
use qrcode::{render::unicode, QrCode};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};
use tracing::{debug, info, warn};

const DEFAULT_API_BASE_URL: &str = "https://ilinkai.weixin.qq.com";
const DEFAULT_CDN_BASE_URL: &str = "https://novac2c.cdn.weixin.qq.com/c2c";
const LONG_POLL_TIMEOUT_MS: u64 = 35_000;
const API_TIMEOUT_SECS: u64 = 15;
const QR_STATUS_TIMEOUT_SECS: u64 = 45;
const INITIAL_BACKOFF_SECS: u64 = 2;
const MAX_BACKOFF_SECS: u64 = 30;
const SESSION_EXPIRED_ERRCODE: i64 = -14;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct AccountData {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    saved_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct SyncData {
    #[serde(default)]
    get_updates_buf: String,
}

#[derive(Debug, Default)]
struct AdapterRuntimeState {
    connected: bool,
    started_at: Option<chrono::DateTime<Utc>>,
    last_message_at: Option<chrono::DateTime<Utc>>,
    messages_received: u64,
    messages_sent: u64,
    last_error: Option<String>,
}

/// WeChat iLink adapter with QR login and long polling.
pub struct WeChatAdapter {
    api_base_url: String,
    _cdn_base_url: String,
    state_dir: PathBuf,
    allowed_users: Vec<String>,
    client: reqwest::Client,
    bot_token: Arc<RwLock<Option<String>>>,
    account_id: Arc<RwLock<Option<String>>>,
    context_tokens: Arc<Mutex<HashMap<String, String>>>,
    cursor: Arc<Mutex<String>>,
    runtime: Arc<Mutex<AdapterRuntimeState>>,
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
}

impl WeChatAdapter {
    pub fn new(
        allowed_users: Vec<String>,
        api_base_url: Option<String>,
        cdn_base_url: Option<String>,
        state_dir: Option<String>,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let adapter = Self {
            api_base_url: api_base_url.unwrap_or_else(|| DEFAULT_API_BASE_URL.to_string()),
            _cdn_base_url: cdn_base_url.unwrap_or_else(|| DEFAULT_CDN_BASE_URL.to_string()),
            state_dir: state_dir
                .map(PathBuf::from)
                .unwrap_or_else(default_state_dir),
            allowed_users,
            client: reqwest::Client::new(),
            bot_token: Arc::new(RwLock::new(None)),
            account_id: Arc::new(RwLock::new(None)),
            context_tokens: Arc::new(Mutex::new(HashMap::new())),
            cursor: Arc::new(Mutex::new(String::new())),
            runtime: Arc::new(Mutex::new(AdapterRuntimeState::default())),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
        };
        adapter.load_persisted_state();
        adapter
    }

    fn clone_for_task(&self) -> Self {
        Self {
            api_base_url: self.api_base_url.clone(),
            _cdn_base_url: self._cdn_base_url.clone(),
            state_dir: self.state_dir.clone(),
            allowed_users: self.allowed_users.clone(),
            client: self.client.clone(),
            bot_token: Arc::clone(&self.bot_token),
            account_id: Arc::clone(&self.account_id),
            context_tokens: Arc::clone(&self.context_tokens),
            cursor: Arc::clone(&self.cursor),
            runtime: Arc::clone(&self.runtime),
            shutdown_tx: Arc::clone(&self.shutdown_tx),
            shutdown_rx: self.shutdown_rx.clone(),
        }
    }

    fn load_persisted_state(&self) {
        let account_path = self.state_dir.join("account.json");
        if let Ok(content) = std::fs::read_to_string(&account_path) {
            if let Ok(account) = serde_json::from_str::<AccountData>(&content) {
                if let Some(token) = account.token.filter(|value| !value.is_empty()) {
                    if let Ok(mut guard) = self.bot_token.write() {
                        *guard = Some(token);
                    }
                }
                if let Some(account_id) = account.account_id {
                    if let Ok(mut guard) = self.account_id.write() {
                        *guard = Some(account_id);
                    }
                }
            }
        }

        let sync_path = self.state_dir.join("sync.json");
        if let Ok(content) = std::fs::read_to_string(&sync_path) {
            if let Ok(sync) = serde_json::from_str::<SyncData>(&content) {
                if let Ok(mut guard) = self.cursor.lock() {
                    *guard = sync.get_updates_buf;
                }
            }
        }
    }

    fn save_account_data(&self, token: &str, account_id: &str, user_id: Option<&str>) {
        if std::fs::create_dir_all(&self.state_dir).is_err() {
            return;
        }
        let data = AccountData {
            token: Some(token.to_string()),
            account_id: Some(account_id.to_string()),
            user_id: user_id.map(str::to_string),
            saved_at: Some(Utc::now().to_rfc3339()),
        };
        if let Ok(serialized) = serde_json::to_string_pretty(&data) {
            let _ = std::fs::write(self.state_dir.join("account.json"), serialized);
        }
    }

    fn save_cursor(&self, cursor: &str) {
        if std::fs::create_dir_all(&self.state_dir).is_err() {
            return;
        }
        let data = SyncData {
            get_updates_buf: cursor.to_string(),
        };
        if let Ok(serialized) = serde_json::to_string(&data) {
            let _ = std::fs::write(self.state_dir.join("sync.json"), serialized);
        }
    }

    fn api_url(&self, endpoint: &str) -> String {
        format!(
            "{}/ilink/bot/{endpoint}",
            self.api_base_url.trim_end_matches('/')
        )
    }

    fn get_token(&self) -> Option<String> {
        self.bot_token.read().ok().and_then(|guard| guard.clone())
    }

    fn set_runtime_error(&self, message: impl Into<String>) {
        if let Ok(mut runtime) = self.runtime.lock() {
            runtime.last_error = Some(message.into());
        }
    }

    fn is_allowed(&self, user_id: &str) -> bool {
        self.allowed_users.is_empty()
            || self
                .allowed_users
                .iter()
                .any(|entry| entry == "*" || entry == user_id)
    }

    fn update_context_token(&self, user_id: &str, token: &str) {
        if let Ok(mut tokens) = self.context_tokens.lock() {
            tokens.insert(user_id.to_string(), token.to_string());
        }
    }

    fn get_context_token(&self, user_id: &str) -> Option<String> {
        self.context_tokens
            .lock()
            .ok()
            .and_then(|tokens| tokens.get(user_id).cloned())
    }

    async fn ensure_logged_in(&self) -> Result<(), String> {
        if self.get_token().is_some() {
            return Ok(());
        }

        let (token, account_id, user_id) = self.qr_login().await?;
        if let Ok(mut guard) = self.bot_token.write() {
            *guard = Some(token.clone());
        }
        if let Ok(mut guard) = self.account_id.write() {
            *guard = Some(account_id.clone());
        }
        self.save_account_data(&token, &account_id, user_id.as_deref());
        if let Ok(mut runtime) = self.runtime.lock() {
            runtime.connected = true;
            runtime.started_at.get_or_insert_with(Utc::now);
            runtime.last_error = None;
        }
        Ok(())
    }

    async fn qr_login(&self) -> Result<(String, String, Option<String>), String> {
        let response = self
            .client
            .get(format!("{}?bot_type=3", self.api_url("get_bot_qrcode")))
            .timeout(Duration::from_secs(API_TIMEOUT_SECS))
            .send()
            .await
            .map_err(|err| err.to_string())?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("WeChat QR code fetch failed {status}: {body}"));
        }

        let payload: serde_json::Value = response.json().await.map_err(|err| err.to_string())?;
        let qrcode = payload
            .get("qrcode")
            .and_then(|value| value.as_str())
            .ok_or_else(|| "WeChat QR response missing qrcode".to_string())?
            .to_string();
        let qr_image_url = payload
            .get("qrcode_img_content")
            .and_then(|value| value.as_str())
            .unwrap_or("");

        println!("\nWeChat QR login required.");
        if qr_image_url.is_empty() {
            println!("Scan payload: {qrcode}");
        } else {
            println!("Scan URL: {qr_image_url}");
            if let Some(qr) = render_terminal_qr(qr_image_url) {
                println!("\n{qr}\n");
            }
        }

        let deadline = Instant::now() + Duration::from_secs(8 * 60);
        while Instant::now() < deadline {
            let status_response = match self
                .client
                .get(self.api_url("get_qrcode_status"))
                .query(&[("qrcode", qrcode.as_str())])
                .timeout(Duration::from_secs(QR_STATUS_TIMEOUT_SECS))
                .send()
                .await
            {
                Ok(response) => response,
                Err(err) => {
                    // The QR-status endpoint behaves like a long poll: timeouts are normal
                    // while the user hasn't scanned yet, so we keep waiting instead of
                    // aborting adapter startup.
                    if err.is_timeout() {
                        continue;
                    }
                    return Err(err.to_string());
                }
            };

            if !status_response.status().is_success() {
                let status = status_response.status();
                let body = status_response.text().await.unwrap_or_default();
                return Err(format!("WeChat QR status failed {status}: {body}"));
            }

            let status_payload: serde_json::Value = status_response
                .json()
                .await
                .map_err(|err| err.to_string())?;
            match status_payload
                .get("status")
                .and_then(|value| value.as_str())
                .unwrap_or("wait")
            {
                "wait" => {}
                "scaned" => println!("WeChat QR 已扫码，等待手机确认..."),
                "expired" => return Err("WeChat QR code expired before confirmation".to_string()),
                "confirmed" => {
                    let bot_token = status_payload
                        .get("bot_token")
                        .and_then(|value| value.as_str())
                        .ok_or_else(|| "WeChat login missing bot_token".to_string())?
                        .to_string();
                    let account_id = status_payload
                        .get("ilink_bot_id")
                        .and_then(|value| value.as_str())
                        .ok_or_else(|| "WeChat login missing ilink_bot_id".to_string())?
                        .to_string();
                    let user_id = status_payload
                        .get("ilink_user_id")
                        .and_then(|value| value.as_str())
                        .map(str::to_string);
                    println!("WeChat connected.");
                    return Ok((bot_token, account_id, user_id));
                }
                other => debug!("Unhandled WeChat QR status: {other}"),
            }

            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        Err("Timed out waiting for WeChat QR confirmation".to_string())
    }

    async fn send_text(&self, user_id: &str, text: &str) -> Result<(), Box<dyn std::error::Error>> {
        let token = self
            .get_token()
            .ok_or_else(|| io_error("WeChat is not logged in"))?;
        let body = serde_json::json!({
            "msg": {
                "from_user_id": "",
                "to_user_id": user_id,
                "client_id": format!("openfang-{}", uuid::Uuid::new_v4()),
                "message_type": 2,
                "message_state": 2,
                "item_list": [{
                    "type": 1,
                    "text_item": {
                        "text": text
                    }
                }],
                "context_token": self.get_context_token(user_id).unwrap_or_default()
            },
            "base_info": build_base_info()
        });

        let response = self
            .client
            .post(self.api_url("sendmessage"))
            .headers(build_headers(Some(&token)))
            .json(&body)
            .timeout(Duration::from_secs(API_TIMEOUT_SECS))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(io_error(format!(
                "WeChat sendMessage failed {status}: {body}"
            )));
        }

        if let Ok(mut runtime) = self.runtime.lock() {
            runtime.messages_sent += 1;
        }
        Ok(())
    }

    async fn poll_once(&self, tx: &mpsc::Sender<ChannelMessage>) -> Result<(), String> {
        self.ensure_logged_in().await?;
        let token = self
            .get_token()
            .ok_or_else(|| "WeChat token missing after login".to_string())?;
        let cursor = self
            .cursor
            .lock()
            .ok()
            .map(|guard| guard.clone())
            .unwrap_or_default();

        let response = self
            .client
            .post(self.api_url("getupdates"))
            .headers(build_headers(Some(&token)))
            .json(&serde_json::json!({
                "get_updates_buf": cursor,
                "base_info": build_base_info()
            }))
            .timeout(Duration::from_millis(LONG_POLL_TIMEOUT_MS))
            .send()
            .await
            .map_err(|err| err.to_string())?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("WeChat getUpdates failed {status}: {body}"));
        }

        let payload: serde_json::Value = response.json().await.map_err(|err| err.to_string())?;
        let ret = payload
            .get("ret")
            .and_then(|value| value.as_i64())
            .unwrap_or(0);
        let errcode = payload
            .get("errcode")
            .and_then(|value| value.as_i64())
            .unwrap_or(0);
        if ret != 0 || errcode != 0 {
            let errmsg = payload
                .get("errmsg")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            return Err(format!(
                "WeChat getUpdates API error ret={ret} errcode={errcode} errmsg={errmsg}"
            ));
        }

        if let Some(next_cursor) = payload
            .get("get_updates_buf")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
        {
            if let Ok(mut guard) = self.cursor.lock() {
                *guard = next_cursor.to_string();
            }
            self.save_cursor(next_cursor);
        }

        let messages = payload
            .get("msgs")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();

        for message in messages {
            let from_user_id = match message.get("from_user_id").and_then(|value| value.as_str()) {
                Some(value) if !value.is_empty() => value,
                _ => continue,
            };

            if !self.is_allowed(from_user_id) {
                debug!("Ignoring unauthorized WeChat sender: {from_user_id}");
                continue;
            }

            if let Some(context_token) = message
                .get("context_token")
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
            {
                self.update_context_token(from_user_id, context_token);
            }

            let items = message
                .get("item_list")
                .and_then(|value| value.as_array())
                .cloned()
                .unwrap_or_default();
            let text = extract_text_from_items(&items);
            if text.is_empty() {
                continue;
            }

            let platform_message_id = message
                .get("message_id")
                .and_then(|value| value.as_u64())
                .map(|value| value.to_string())
                .unwrap_or_else(|| format!("wechat-{}", uuid::Uuid::new_v4()));
            let timestamp_ms = message
                .get("create_time_ms")
                .and_then(|value| value.as_i64())
                .unwrap_or_else(|| Utc::now().timestamp_millis());

            let channel_message = ChannelMessage {
                channel: ChannelType::Custom("wechat".to_string()),
                platform_message_id,
                sender: ChannelUser {
                    platform_id: from_user_id.to_string(),
                    display_name: from_user_id.to_string(),
                    openfang_user: None,
                },
                content: ChannelContent::Text(text),
                target_agent: None,
                timestamp: chrono::DateTime::<Utc>::from_timestamp_millis(timestamp_ms)
                    .unwrap_or_else(Utc::now),
                is_group: false,
                thread_id: None,
                metadata: HashMap::new(),
            };

            if tx.send(channel_message).await.is_err() {
                break;
            }

            if let Ok(mut runtime) = self.runtime.lock() {
                runtime.messages_received += 1;
                runtime.last_message_at = Some(Utc::now());
            }
        }

        Ok(())
    }

    async fn poll_updates(
        &self,
        tx: mpsc::Sender<ChannelMessage>,
        mut shutdown_rx: watch::Receiver<bool>,
    ) {
        let mut backoff_secs = INITIAL_BACKOFF_SECS;

        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        break;
                    }
                }
                result = self.poll_once(&tx) => {
                    match result {
                        Ok(()) => {
                            backoff_secs = INITIAL_BACKOFF_SECS;
                        }
                        Err(err) => {
                            let msg = {
                                let message = err.to_string();
                                drop(err);
                                message
                            };
                            warn!("WeChat polling error: {msg}");
                            self.set_runtime_error(msg.clone());
                            if msg.contains(&SESSION_EXPIRED_ERRCODE.to_string()) {
                                if let Ok(mut guard) = self.bot_token.write() {
                                    *guard = None;
                                }
                            }
                            tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                            backoff_secs = (backoff_secs * 2).min(MAX_BACKOFF_SECS);
                        }
                    }
                }
            }
        }
    }
}

#[async_trait]
impl ChannelAdapter for WeChatAdapter {
    fn name(&self) -> &str {
        "wechat"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("wechat".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        if let Ok(mut runtime) = self.runtime.lock() {
            runtime.connected = self.get_token().is_some();
            runtime.started_at.get_or_insert_with(Utc::now);
            runtime.last_error = None;
        }

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let adapter = self.clone_for_task();
        let shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            info!("WeChat adapter started");
            adapter.poll_updates(tx, shutdown_rx).await;
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn send(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let text = match content {
            ChannelContent::Text(text) => text,
            ChannelContent::Image { url, caption } => match caption {
                Some(caption) => format!("[image] {caption}\n{url}"),
                None => format!("[image] {url}"),
            },
            ChannelContent::File { url, filename } => format!("[file] {filename}\n{url}"),
            ChannelContent::FileData { filename, .. } => format!("[file] {filename}"),
            ChannelContent::Voice {
                url,
                duration_seconds,
            } => format!("[voice {duration_seconds}s] {url}"),
            ChannelContent::Location { lat, lon } => format!("[location] {lat}, {lon}"),
            ChannelContent::Command { name, args } => {
                let suffix = if args.is_empty() {
                    String::new()
                } else {
                    format!(" {}", args.join(" "))
                };
                format!("/{name}{suffix}")
            }
        };

        self.send_text(&user.platform_id, &text).await
    }

    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.shutdown_tx.send(true);
        if let Ok(mut runtime) = self.runtime.lock() {
            runtime.connected = false;
        }
        Ok(())
    }

    fn status(&self) -> ChannelStatus {
        if let Ok(runtime) = self.runtime.lock() {
            ChannelStatus {
                connected: runtime.connected,
                started_at: runtime.started_at,
                last_message_at: runtime.last_message_at,
                messages_received: runtime.messages_received,
                messages_sent: runtime.messages_sent,
                last_error: runtime.last_error.clone(),
            }
        } else {
            ChannelStatus::default()
        }
    }
}

fn default_state_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(".").to_path_buf())
        .join(".openfang")
        .join("wechat")
}

fn build_base_info() -> serde_json::Value {
    serde_json::json!({
        "channel_version": env!("CARGO_PKG_VERSION")
    })
}

fn render_terminal_qr(data: &str) -> Option<String> {
    let code = QrCode::new(data.as_bytes()).ok()?;
    Some(
        code.render::<unicode::Dense1x2>()
            .dark_color(unicode::Dense1x2::Dark)
            .light_color(unicode::Dense1x2::Light)
            .quiet_zone(true)
            .build(),
    )
}

fn build_headers(token: Option<&str>) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    headers.insert(
        "AuthorizationType",
        reqwest::header::HeaderValue::from_static("ilink_bot_token"),
    );
    if let Some(token) = token.filter(|value| !value.is_empty()) {
        if let Ok(value) = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}")) {
            headers.insert(reqwest::header::AUTHORIZATION, value);
        }
    }
    headers
}

fn extract_text_from_items(items: &[serde_json::Value]) -> String {
    for item in items {
        let item_type = item
            .get("type")
            .and_then(|value| value.as_u64())
            .unwrap_or_default();

        if item_type == 1 {
            if let Some(text) = item
                .get("text_item")
                .and_then(|value| value.get("text"))
                .and_then(|value| value.as_str())
            {
                return text.trim().to_string();
            }
        }

        if item_type == 3 {
            if let Some(text) = item
                .get("voice_item")
                .and_then(|value| value.get("text"))
                .and_then(|value| value.as_str())
            {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        }
    }
    String::new()
}

fn io_error(message: impl Into<String>) -> Box<dyn std::error::Error> {
    Box::new(std::io::Error::other(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wechat_extracts_text_from_text_item() {
        let items = vec![serde_json::json!({
            "type": 1,
            "text_item": { "text": "hello wechat" }
        })];
        assert_eq!(extract_text_from_items(&items), "hello wechat");
    }

    #[test]
    fn wechat_extracts_transcribed_voice_text() {
        let items = vec![serde_json::json!({
            "type": 3,
            "voice_item": { "text": "voice text" }
        })];
        assert_eq!(extract_text_from_items(&items), "voice text");
    }

    #[test]
    fn wechat_defaults_to_custom_channel_type() {
        let adapter = WeChatAdapter::new(vec![], None, None, None);
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("wechat".to_string())
        );
    }
}
