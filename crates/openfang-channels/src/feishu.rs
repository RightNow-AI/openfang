//! Feishu/Lark Open Platform channel adapter.
//!
//! Supports both regions via the `region` parameter:
//! - **CN** (Feishu domestic): `open.feishu.cn`
//! - **International** (Lark): `open.larksuite.com`
//!
//! Features:
//! - Region-based API domain switching
//! - Message deduplication (event_id + message_id)
//! - Group chat filtering (require @mention or question mark)
//! - Rich text (post) message parsing
//! - Event encryption/decryption support (AES-256-CBC)
//! - Tenant access token caching with auto-refresh

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

// ─── Region-based API endpoints ─────────────────────────────────────────────

/// API base domains per region.
const FEISHU_DOMAIN: &str = "https://open.feishu.cn";
const LARK_DOMAIN: &str = "https://open.larksuite.com";

/// Maximum message text length (characters).
const MAX_MESSAGE_LEN: usize = 4000;

/// Token refresh buffer — refresh 5 minutes before actual expiry.
const TOKEN_REFRESH_BUFFER_SECS: u64 = 300;

/// Maximum cached message/event IDs for deduplication.
const DEDUP_CACHE_SIZE: usize = 1000;

// ─── Region ─────────────────────────────────────────────────────────────────

/// Feishu/Lark region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeishuRegion {
    /// China domestic (open.feishu.cn).
    Cn,
    /// International / Lark (open.larksuite.com).
    Intl,
}

impl FeishuRegion {
    pub fn parse_region(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "intl" | "international" | "lark" => Self::Intl,
            _ => Self::Cn,
        }
    }

    fn domain(&self) -> &'static str {
        match self {
            Self::Cn => FEISHU_DOMAIN,
            Self::Intl => LARK_DOMAIN,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Cn => "Feishu",
            Self::Intl => "Lark",
        }
    }

    fn channel_name(&self) -> &'static str {
        match self {
            Self::Cn => "feishu",
            Self::Intl => "lark",
        }
    }
}

// ─── Deduplication ──────────────────────────────────────────────────────────

/// Simple ring-buffer deduplication cache.
struct DedupCache {
    ids: std::sync::Mutex<Vec<String>>,
    max_size: usize,
}

impl DedupCache {
    fn new(max_size: usize) -> Self {
        Self {
            ids: std::sync::Mutex::new(Vec::with_capacity(max_size)),
            max_size,
        }
    }

    /// Returns `true` if the ID was already seen (duplicate).
    fn check_and_insert(&self, id: &str) -> bool {
        let mut ids = self.ids.lock().unwrap();
        if ids.iter().any(|s| s == id) {
            return true;
        }
        if ids.len() >= self.max_size {
            let drain_count = self.max_size / 2;
            ids.drain(..drain_count);
        }
        ids.push(id.to_string());
        false
    }
}

// ─── Adapter ────────────────────────────────────────────────────────────────

/// Feishu/Lark Open Platform adapter.
///
/// Inbound messages arrive via a webhook HTTP server that receives event
/// callbacks from the platform. Outbound messages are sent via the IM API
/// with a tenant access token for authentication.
pub struct FeishuAdapter {
    /// Feishu/Lark app ID.
    app_id: String,
    /// SECURITY: App secret, zeroized on drop.
    app_secret: Zeroizing<String>,
    /// Port on which the inbound webhook HTTP server listens.
    webhook_port: u16,
    /// Region (CN or International).
    region: FeishuRegion,
    /// Webhook path (default: `/feishu/webhook`).
    webhook_path: String,
    /// Optional verification token for webhook event validation.
    verification_token: Option<String>,
    /// Optional encrypt key for webhook event decryption.
    encrypt_key: Option<String>,
    /// Bot name aliases for group-chat mention detection.
    bot_names: Vec<String>,
    /// HTTP client for API calls.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Cached tenant access token and its expiry instant.
    cached_token: Arc<RwLock<Option<(String, Instant)>>>,
    /// Message deduplication cache.
    message_dedup: Arc<DedupCache>,
    /// Event deduplication cache.
    event_dedup: Arc<DedupCache>,
}

impl FeishuAdapter {
    /// Create a new Feishu adapter with minimal config.
    pub fn new(app_id: String, app_secret: String, webhook_port: u16) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            app_id,
            app_secret: Zeroizing::new(app_secret),
            webhook_port,
            region: FeishuRegion::Cn,
            webhook_path: "/feishu/webhook".to_string(),
            verification_token: None,
            encrypt_key: None,
            bot_names: Vec::new(),
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            cached_token: Arc::new(RwLock::new(None)),
            message_dedup: Arc::new(DedupCache::new(DEDUP_CACHE_SIZE)),
            event_dedup: Arc::new(DedupCache::new(DEDUP_CACHE_SIZE)),
        }
    }

    /// Create a new adapter with full configuration.
    #[allow(clippy::too_many_arguments)]
    pub fn with_config(
        app_id: String,
        app_secret: String,
        webhook_port: u16,
        region: FeishuRegion,
        webhook_path: Option<String>,
        verification_token: Option<String>,
        encrypt_key: Option<String>,
        bot_names: Vec<String>,
    ) -> Self {
        let mut adapter = Self::new(app_id, app_secret, webhook_port);
        adapter.region = region;
        if let Some(path) = webhook_path {
            adapter.webhook_path = path;
        }
        adapter.verification_token = verification_token;
        adapter.encrypt_key = encrypt_key;
        adapter.bot_names = bot_names;
        adapter
    }

    /// API URL for a given path suffix.
    fn api_url(&self, path: &str) -> String {
        format!("{}{}", self.region.domain(), path)
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

        let url = self.api_url("/open-apis/auth/v3/tenant_access_token/internal");
        let resp = self.client.post(&url).json(&body).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let resp_body = resp.text().await.unwrap_or_default();
            return Err(format!(
                "{} token request failed {status}: {resp_body}",
                self.region.label()
            )
            .into());
        }

        let resp_body: serde_json::Value = resp.json().await?;
        let code = resp_body["code"].as_i64().unwrap_or(-1);
        if code != 0 {
            let msg = resp_body["msg"].as_str().unwrap_or("unknown error");
            return Err(format!("{} token error: {msg}", self.region.label()).into());
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
        let url = self.api_url("/open-apis/bot/v3/info");

        let resp = self.client.get(&url).bearer_auth(&token).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!(
                "{} authentication failed {status}: {body}",
                self.region.label()
            )
            .into());
        }

        let body: serde_json::Value = resp.json().await?;
        let code = body["code"].as_i64().unwrap_or(-1);
        if code != 0 {
            let msg = body["msg"].as_str().unwrap_or("unknown error");
            return Err(format!("{} bot info error: {msg}", self.region.label()).into());
        }

        let bot_name = body["bot"]["app_name"]
            .as_str()
            .unwrap_or("Bot")
            .to_string();
        Ok(bot_name)
    }

    /// Send a text message to a chat.
    async fn api_send_message(
        &self,
        receive_id: &str,
        receive_id_type: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let token = self.get_token().await?;
        let url = format!(
            "{}?receive_id_type={}",
            self.api_url("/open-apis/im/v1/messages"),
            receive_id_type
        );

        let chunks = split_message(text, MAX_MESSAGE_LEN);

        for chunk in chunks {
            let content = serde_json::json!({ "text": chunk });
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
                return Err(format!(
                    "{} send message error {status}: {resp_body}",
                    self.region.label()
                )
                .into());
            }

            let resp_body: serde_json::Value = resp.json().await?;
            let code = resp_body["code"].as_i64().unwrap_or(-1);
            if code != 0 {
                let msg = resp_body["msg"].as_str().unwrap_or("unknown error");
                warn!("{} send message API error: {msg}", self.region.label());
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
        let url = self.api_url(&format!("/open-apis/im/v1/messages/{}/reply", message_id));

        let content = serde_json::json!({ "text": text });
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
            return Err(format!(
                "{} reply message error {status}: {resp_body}",
                self.region.label()
            )
            .into());
        }

        Ok(())
    }
}

// ─── Event parsing helpers ──────────────────────────────────────────────────

/// Extract plain text from a "post" (rich text) content structure.
fn extract_text_from_post(content: &serde_json::Value) -> Option<String> {
    let locales = ["en_us", "zh_cn", "ja_jp", "zh_hk", "zh_tw"];

    let mut post_content = None;
    for locale in &locales {
        if let Some(locale_data) = content.get(locale) {
            if let Some(paragraphs) = locale_data.get("content") {
                post_content = Some(paragraphs);
                break;
            }
        }
    }

    if post_content.is_none() {
        post_content = content.get("content");
    }

    let paragraphs = post_content?.as_array()?;
    let mut text_parts = Vec::new();

    for paragraph in paragraphs {
        let elements = paragraph.as_array()?;
        for element in elements {
            let tag = element["tag"].as_str().unwrap_or("");
            match tag {
                "text" => {
                    if let Some(text) = element["text"].as_str() {
                        text_parts.push(text.to_string());
                    }
                }
                "a" => {
                    if let Some(text) = element["text"].as_str() {
                        text_parts.push(text.to_string());
                    }
                    if let Some(href) = element["href"].as_str() {
                        text_parts.push(format!("({href})"));
                    }
                }
                "at" => {
                    if let Some(name) = element["user_name"].as_str() {
                        text_parts.push(format!("@{name}"));
                    }
                }
                _ => {}
            }
        }
        text_parts.push("\n".to_string());
    }

    let result = text_parts.join("").trim().to_string();
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// Check whether the bot should respond to a group message.
fn should_respond_in_group(text: &str, mentions: &serde_json::Value, bot_names: &[String]) -> bool {
    if let Some(arr) = mentions.as_array() {
        if !arr.is_empty() {
            return true;
        }
    }

    if text.contains('?') || text.contains('\u{FF1F}') {
        return true;
    }

    let lower = text.to_lowercase();
    for name in bot_names {
        if lower.contains(&name.to_lowercase()) {
            return true;
        }
    }

    false
}

/// Strip @mention placeholders from text (`@_user_N` format).
fn strip_mention_placeholders(text: &str) -> String {
    let re = regex_lite::Regex::new(r"@_user_\d+\s*").unwrap();
    re.replace_all(text, "").trim().to_string()
}

/// Decrypt an AES-256-CBC encrypted event payload.
fn decrypt_event(
    encrypted: &str,
    encrypt_key: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    use base64::Engine;
    use sha2::Digest;

    let cipher_bytes = base64::engine::general_purpose::STANDARD.decode(encrypted)?;
    if cipher_bytes.len() < 16 {
        return Err("Encrypted data too short".into());
    }

    let key = sha2::Sha256::digest(encrypt_key.as_bytes());
    let iv = &cipher_bytes[..16];
    let ciphertext = &cipher_bytes[16..];

    use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
    type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;

    let decryptor = Aes256CbcDec::new(key.as_slice().into(), iv.into());
    let mut buf = ciphertext.to_vec();
    let plaintext = decryptor
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|e| format!("Decryption failed: {e}"))?;

    let json_str = std::str::from_utf8(plaintext)?;
    let value: serde_json::Value = serde_json::from_str(json_str)?;
    Ok(value)
}

/// Parse a webhook event (V2 schema) into a `ChannelMessage`.
fn parse_event(
    event: &serde_json::Value,
    bot_names: &[String],
    channel_name: &str,
) -> Option<ChannelMessage> {
    let header = event.get("header")?;
    let event_type = header["event_type"].as_str().unwrap_or("");

    if event_type != "im.message.receive_v1" {
        return None;
    }

    let event_data = event.get("event")?;
    let message = event_data.get("message")?;
    let sender = event_data.get("sender")?;

    let sender_type = sender["sender_type"].as_str().unwrap_or("user");
    if sender_type == "bot" {
        return None;
    }

    let msg_type = message["message_type"].as_str().unwrap_or("");
    let content_str = message["content"].as_str().unwrap_or("{}");
    let content_json: serde_json::Value = serde_json::from_str(content_str).unwrap_or_default();

    let text = match msg_type {
        "text" => {
            let t = content_json["text"]
                .as_str()
                .unwrap_or("")
                .trim()
                .to_string();
            if t.is_empty() {
                return None;
            }
            t
        }
        "post" => extract_text_from_post(&content_json)?,
        _ => return None,
    };

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

    let is_group = chat_type == "group";
    let mentions = message
        .get("mentions")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let text = if is_group {
        let stripped = strip_mention_placeholders(&text);
        if stripped.is_empty() || !should_respond_in_group(&stripped, &mentions, bot_names) {
            return None;
        }
        stripped
    } else {
        text
    };

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
        ChannelContent::Text(text)
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
    if !mentions.is_null() {
        metadata.insert("mentions".to_string(), mentions);
    }

    Some(ChannelMessage {
        channel: ChannelType::Custom(channel_name.to_string()),
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

// ─── ChannelAdapter impl ────────────────────────────────────────────────────

#[async_trait]
impl ChannelAdapter for FeishuAdapter {
    fn name(&self) -> &str {
        self.region.channel_name()
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom(self.region.channel_name().to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        let bot_name = self.validate().await?;
        let label = self.region.label();
        info!("{label} adapter authenticated as {bot_name}");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let port = self.webhook_port;
        let webhook_path = self.webhook_path.clone();
        let verification_token = self.verification_token.clone();
        let encrypt_key = self.encrypt_key.clone();
        let bot_names = self.bot_names.clone();
        let channel_name = self.region.channel_name().to_string();
        let region_label = self.region.label().to_string();
        let message_dedup = Arc::clone(&self.message_dedup);
        let event_dedup = Arc::clone(&self.event_dedup);
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let verification_token = Arc::new(verification_token);
            let encrypt_key = Arc::new(encrypt_key);
            let tx = Arc::new(tx);
            let bot_names = Arc::new(bot_names);
            let channel_name = Arc::new(channel_name);
            let region_label = Arc::new(region_label);

            let app = axum::Router::new().route(
                &webhook_path,
                axum::routing::post({
                    let vt = Arc::clone(&verification_token);
                    let ek = Arc::clone(&encrypt_key);
                    let tx = Arc::clone(&tx);
                    let bot_names = Arc::clone(&bot_names);
                    let channel_name = Arc::clone(&channel_name);
                    let region_label = Arc::clone(&region_label);
                    let message_dedup = Arc::clone(&message_dedup);
                    let event_dedup = Arc::clone(&event_dedup);
                    move |body: axum::extract::Json<serde_json::Value>| {
                        let vt = Arc::clone(&vt);
                        let ek = Arc::clone(&ek);
                        let tx = Arc::clone(&tx);
                        let bot_names = Arc::clone(&bot_names);
                        let channel_name = Arc::clone(&channel_name);
                        let region_label = Arc::clone(&region_label);
                        let message_dedup = Arc::clone(&message_dedup);
                        let event_dedup = Arc::clone(&event_dedup);
                        async move {
                            let mut event_data = body.0.clone();

                            // Step 1: Decrypt if encrypted
                            if let Some(encrypted) = body.0.get("encrypt").and_then(|v| v.as_str())
                            {
                                if let Some(ref key) = *ek {
                                    match decrypt_event(encrypted, key) {
                                        Ok(decrypted) => {
                                            event_data = decrypted;
                                        }
                                        Err(e) => {
                                            warn!("{region_label}: decrypt failed: {e}");
                                            return (
                                                axum::http::StatusCode::BAD_REQUEST,
                                                axum::Json(
                                                    serde_json::json!({"error": "decrypt failed"}),
                                                ),
                                            );
                                        }
                                    }
                                }
                            }

                            // Step 2: URL verification challenge
                            if event_data.get("type").and_then(|v| v.as_str())
                                == Some("url_verification")
                            {
                                if let Some(ref expected_token) = *vt {
                                    let token = event_data["token"].as_str().unwrap_or("");
                                    if token != expected_token {
                                        warn!("{region_label}: invalid verification token");
                                        return (
                                            axum::http::StatusCode::FORBIDDEN,
                                            axum::Json(serde_json::json!({})),
                                        );
                                    }
                                }
                                // Also handle v2 challenge format
                                if let Some(challenge) = body.0.get("challenge") {
                                    return (
                                        axum::http::StatusCode::OK,
                                        axum::Json(serde_json::json!({
                                            "challenge": challenge,
                                        })),
                                    );
                                }
                                let challenge = event_data
                                    .get("challenge")
                                    .cloned()
                                    .unwrap_or(serde_json::Value::String(String::new()));
                                return (
                                    axum::http::StatusCode::OK,
                                    axum::Json(serde_json::json!({
                                        "challenge": challenge,
                                    })),
                                );
                            }

                            // Step 3: Event deduplication
                            if let Some(event_id) = event_data
                                .get("header")
                                .and_then(|h| h.get("event_id"))
                                .and_then(|v| v.as_str())
                            {
                                if event_dedup.check_and_insert(event_id) {
                                    return (
                                        axum::http::StatusCode::OK,
                                        axum::Json(serde_json::json!({"code": 0})),
                                    );
                                }
                            }

                            // Step 4: Parse V2 event
                            let schema = event_data.get("schema").and_then(|v| v.as_str());
                            if schema == Some("2.0") {
                                if let Some(msg) =
                                    parse_event(&event_data, &bot_names, &channel_name)
                                {
                                    if !message_dedup.check_and_insert(&msg.platform_message_id) {
                                        let _ = tx.send(msg).await;
                                    }
                                }
                            } else {
                                // V1 legacy event format
                                let event_type = event_data["event"]["type"].as_str().unwrap_or("");
                                if event_type == "message" {
                                    let event = &event_data["event"];
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

                                        if !message_dedup.check_and_insert(&msg_id) {
                                            let content = if text.starts_with('/') {
                                                let parts: Vec<&str> =
                                                    text.splitn(2, ' ').collect();
                                                let cmd = parts[0].trim_start_matches('/');
                                                let args: Vec<String> = parts
                                                    .get(1)
                                                    .map(|a| {
                                                        a.split_whitespace()
                                                            .map(String::from)
                                                            .collect()
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
                                                channel: ChannelType::Custom(
                                                    channel_name.to_string(),
                                                ),
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
                            }

                            (
                                axum::http::StatusCode::OK,
                                axum::Json(serde_json::json!({"code": 0})),
                            )
                        }
                    }
                }),
            );

            let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
            info!("{} webhook server listening on {addr}", *region_label);

            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    warn!("{} webhook bind failed: {e}", *region_label);
                    return;
                }
            };

            let server = axum::serve(listener, app);

            tokio::select! {
                result = server => {
                    if let Err(e) = result {
                        warn!("{} webhook server error: {e}", *region_label);
                    }
                }
                _ = shutdown_rx.changed() => {
                    info!("{} adapter shutting down", *region_label);
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
        assert_eq!(adapter.region, FeishuRegion::Cn);
    }

    #[test]
    fn test_lark_region_adapter() {
        let adapter = FeishuAdapter::with_config(
            "cli_abc123".to_string(),
            "secret".to_string(),
            9100,
            FeishuRegion::Intl,
            Some("/lark/webhook".to_string()),
            Some("verify-token".to_string()),
            Some("encrypt-key".to_string()),
            vec!["MyBot".to_string()],
        );
        assert_eq!(adapter.name(), "lark");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("lark".to_string())
        );
        assert_eq!(adapter.webhook_path, "/lark/webhook");
        assert_eq!(adapter.region, FeishuRegion::Intl);
    }

    #[test]
    fn test_region_from_str() {
        assert_eq!(FeishuRegion::parse_region("cn"), FeishuRegion::Cn);
        assert_eq!(FeishuRegion::parse_region("intl"), FeishuRegion::Intl);
        assert_eq!(FeishuRegion::parse_region("lark"), FeishuRegion::Intl);
        assert_eq!(
            FeishuRegion::parse_region("international"),
            FeishuRegion::Intl
        );
        assert_eq!(FeishuRegion::parse_region("anything"), FeishuRegion::Cn);
    }

    #[test]
    fn test_region_domains() {
        assert_eq!(FeishuRegion::Cn.domain(), "https://open.feishu.cn");
        assert_eq!(FeishuRegion::Intl.domain(), "https://open.larksuite.com");
    }

    #[test]
    fn test_with_verification() {
        let adapter = FeishuAdapter::with_config(
            "cli_abc123".to_string(),
            "secret".to_string(),
            9000,
            FeishuRegion::Cn,
            None,
            Some("verify-token".to_string()),
            Some("encrypt-key".to_string()),
            vec![],
        );
        assert_eq!(adapter.verification_token, Some("verify-token".to_string()));
        assert_eq!(adapter.encrypt_key, Some("encrypt-key".to_string()));
        assert_eq!(adapter.webhook_path, "/feishu/webhook"); // default
    }

    // ─── Dedup tests ────────────────────────────────────────────────────

    #[test]
    fn test_dedup_cache_basic() {
        let cache = DedupCache::new(10);
        assert!(!cache.check_and_insert("msg1"));
        assert!(cache.check_and_insert("msg1"));
        assert!(!cache.check_and_insert("msg2"));
    }

    #[test]
    fn test_dedup_cache_eviction() {
        let cache = DedupCache::new(4);
        assert!(!cache.check_and_insert("a"));
        assert!(!cache.check_and_insert("b"));
        assert!(!cache.check_and_insert("c"));
        assert!(!cache.check_and_insert("d"));
        assert!(!cache.check_and_insert("e"));
        assert!(!cache.check_and_insert("a")); // evicted
        assert!(cache.check_and_insert("c")); // still present
        assert!(cache.check_and_insert("e")); // still present
    }

    // ─── Group chat filter tests ────────────────────────────────────────

    #[test]
    fn test_should_respond_when_mentioned() {
        let mentions = serde_json::json!([{"key": "@_user_1", "id": {"open_id": "ou_123"}}]);
        assert!(should_respond_in_group("hello", &mentions, &[]));
    }

    #[test]
    fn test_should_respond_with_question_mark() {
        let mentions = serde_json::Value::Null;
        assert!(should_respond_in_group("how are you?", &mentions, &[]));
    }

    #[test]
    fn test_should_respond_with_fullwidth_question() {
        let mentions = serde_json::Value::Null;
        assert!(should_respond_in_group(
            "how are you\u{FF1F}",
            &mentions,
            &[]
        ));
    }

    #[test]
    fn test_should_respond_with_bot_name() {
        let mentions = serde_json::Value::Null;
        let bot_names = vec!["MyBot".to_string()];
        assert!(should_respond_in_group(
            "hey mybot help",
            &mentions,
            &bot_names
        ));
    }

    #[test]
    fn test_should_not_respond_plain_group_msg() {
        let mentions = serde_json::Value::Null;
        assert!(!should_respond_in_group("random chat", &mentions, &[]));
    }

    // ─── Rich text parsing tests ────────────────────────────────────────

    #[test]
    fn test_extract_text_from_post_en() {
        let content = serde_json::json!({
            "en_us": {
                "content": [
                    [
                        {"tag": "text", "text": "Hello "},
                        {"tag": "text", "text": "world"}
                    ]
                ]
            }
        });
        let result = extract_text_from_post(&content).unwrap();
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_extract_text_from_post_with_link() {
        let content = serde_json::json!({
            "en_us": {
                "content": [
                    [
                        {"tag": "text", "text": "Visit "},
                        {"tag": "a", "text": "Google", "href": "https://google.com"}
                    ]
                ]
            }
        });
        let result = extract_text_from_post(&content).unwrap();
        assert!(result.contains("Google"));
        assert!(result.contains("(https://google.com)"));
    }

    #[test]
    fn test_extract_text_from_post_empty() {
        let content = serde_json::json!({});
        assert!(extract_text_from_post(&content).is_none());
    }

    // ─── Mention stripping tests ────────────────────────────────────────

    #[test]
    fn test_strip_mention_placeholders() {
        assert_eq!(
            strip_mention_placeholders("@_user_1 hello world"),
            "hello world"
        );
        assert_eq!(strip_mention_placeholders("@_user_1 @_user_2 hi"), "hi");
        assert_eq!(strip_mention_placeholders("no mentions"), "no mentions");
    }

    // ─── Event parsing tests ────────────────────────────────────────────

    #[test]
    fn test_parse_event_v2_text() {
        let event = serde_json::json!({
            "schema": "2.0",
            "header": {
                "event_id": "evt-001",
                "event_type": "im.message.receive_v1",
            },
            "event": {
                "sender": {
                    "sender_id": { "open_id": "ou_abc123" },
                    "sender_type": "user"
                },
                "message": {
                    "message_id": "om_abc123",
                    "root_id": null,
                    "chat_id": "oc_chat123",
                    "chat_type": "p2p",
                    "message_type": "text",
                    "content": "{\"text\":\"Hello!\"}"
                }
            }
        });

        let msg = parse_event(&event, &[], "feishu").unwrap();
        assert_eq!(msg.channel, ChannelType::Custom("feishu".to_string()));
        assert_eq!(msg.platform_message_id, "om_abc123");
        assert!(!msg.is_group);
        assert!(matches!(msg.content, ChannelContent::Text(ref t) if t == "Hello!"));

        // Same event but as "lark" channel
        let msg = parse_event(&event, &[], "lark").unwrap();
        assert_eq!(msg.channel, ChannelType::Custom("lark".to_string()));
    }

    #[test]
    fn test_parse_event_group_filters() {
        let event = serde_json::json!({
            "schema": "2.0",
            "header": {
                "event_id": "evt-002",
                "event_type": "im.message.receive_v1"
            },
            "event": {
                "sender": {
                    "sender_id": { "open_id": "ou_abc123" },
                    "sender_type": "user"
                },
                "message": {
                    "message_id": "om_grp1",
                    "chat_id": "oc_grp123",
                    "chat_type": "group",
                    "message_type": "text",
                    "content": "{\"text\":\"random group chat\"}"
                }
            }
        });

        // No mention, no question mark — filtered
        assert!(parse_event(&event, &[], "feishu").is_none());
    }

    #[test]
    fn test_parse_event_group_with_question() {
        let event = serde_json::json!({
            "schema": "2.0",
            "header": {
                "event_id": "evt-003",
                "event_type": "im.message.receive_v1"
            },
            "event": {
                "sender": {
                    "sender_id": { "open_id": "ou_abc123" },
                    "sender_type": "user"
                },
                "message": {
                    "message_id": "om_grp2",
                    "chat_id": "oc_grp123",
                    "chat_type": "group",
                    "message_type": "text",
                    "content": "{\"text\":\"what is the status?\"}"
                }
            }
        });

        let msg = parse_event(&event, &[], "feishu").unwrap();
        assert!(msg.is_group);
    }

    #[test]
    fn test_parse_event_command() {
        let event = serde_json::json!({
            "schema": "2.0",
            "header": {
                "event_id": "evt-004",
                "event_type": "im.message.receive_v1"
            },
            "event": {
                "sender": {
                    "sender_id": { "open_id": "ou_abc123" },
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

        let msg = parse_event(&event, &[], "feishu").unwrap();
        match &msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "help");
                assert_eq!(args, &["all"]);
            }
            other => panic!("Expected Command, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_event_skips_bot() {
        let event = serde_json::json!({
            "schema": "2.0",
            "header": {
                "event_id": "evt-005",
                "event_type": "im.message.receive_v1"
            },
            "event": {
                "sender": {
                    "sender_id": { "open_id": "ou_bot" },
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

        assert!(parse_event(&event, &[], "feishu").is_none());
    }

    #[test]
    fn test_parse_event_post_message() {
        let post_content = serde_json::json!({
            "en_us": {
                "content": [[
                    {"tag": "text", "text": "Check order "},
                    {"tag": "text", "text": "#1234"}
                ]]
            }
        });

        let event = serde_json::json!({
            "schema": "2.0",
            "header": {
                "event_id": "evt-006",
                "event_type": "im.message.receive_v1"
            },
            "event": {
                "sender": {
                    "sender_id": { "open_id": "ou_user1" },
                    "sender_type": "user"
                },
                "message": {
                    "message_id": "om_post1",
                    "chat_id": "oc_chat1",
                    "chat_type": "p2p",
                    "message_type": "post",
                    "content": post_content.to_string()
                }
            }
        });

        let msg = parse_event(&event, &[], "feishu").unwrap();
        match &msg.content {
            ChannelContent::Text(t) => assert!(t.contains("Check order")),
            other => panic!("Expected Text, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_event_thread_id() {
        let event = serde_json::json!({
            "schema": "2.0",
            "header": {
                "event_id": "evt-007",
                "event_type": "im.message.receive_v1"
            },
            "event": {
                "sender": {
                    "sender_id": { "open_id": "ou_user1" },
                    "sender_type": "user"
                },
                "message": {
                    "message_id": "om_thread1",
                    "root_id": "om_root1",
                    "chat_id": "oc_chat1",
                    "chat_type": "p2p",
                    "message_type": "text",
                    "content": "{\"text\":\"Thread reply\"}"
                }
            }
        });

        let msg = parse_event(&event, &[], "feishu").unwrap();
        assert_eq!(msg.thread_id, Some("om_root1".to_string()));
    }
}
