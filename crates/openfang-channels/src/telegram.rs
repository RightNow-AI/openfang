//! Telegram Bot API adapter for the OpenFang channel bridge.
//!
//! Uses long-polling via `getUpdates` with exponential backoff on failures.
//! No external Telegram crate — just `reqwest` for full control over error handling.

use crate::telegram_media_batch::{
    MediaItemKind, MediaItemStatus, TelegramDownloadHint, TelegramMediaBatch, TelegramMediaItem,
};
use crate::types::{
    split_message, ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser,
    LifecycleReaction,
};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, watch, Mutex};
use tracing::{debug, info, warn};
use zeroize::Zeroizing;

/// Maximum backoff duration on API failures.
const MAX_BACKOFF: Duration = Duration::from_secs(60);
/// Initial backoff duration on API failures.
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
/// Telegram long-polling timeout (seconds) — sent as the `timeout` parameter to getUpdates.
const LONG_POLL_TIMEOUT: u64 = 30;

/// Default Telegram Bot API base URL.
const DEFAULT_API_URL: &str = "https://api.telegram.org";
/// Telegram bot command menu shown in supported clients.
const TELEGRAM_BOT_COMMANDS: &[(&str, &str)] = &[
    ("new", "Start a new conversation"),
    ("agent", "Select which agent to talk to"),
    ("agents", "List running agents"),
    ("help", "Show all commands"),
];
/// Safety cap for `getFile` when using Telegram Local Bot API.
///
/// Set to 2GB to match the configured max_download_size and fully utilize
/// Local Bot API's capability to handle large files. The previous 100MB limit
/// was too conservative and caused most videos to be skipped.
///
/// If Local Bot API crashes on extremely large files, this can be reduced,
/// but 2GB should be safe for typical video processing workflows.
const LOCAL_API_SAFE_GETFILE_LIMIT: u64 = 2 * 1024 * 1024 * 1024;
/// Official Bot API getFile limit (20 MB). Files larger than this cannot be resolved via getFile.
const OFFICIAL_API_GETFILE_LIMIT: u64 = 20 * 1024 * 1024;
/// Local file copy threshold above which we emit a live progress bar.
const LOCAL_COPY_PROGRESS_THRESHOLD: u64 = 5 * 1024 * 1024;
/// Copy Telegram Local Bot API files in chunks so large copies don't become a
/// single opaque operation and can surface progress updates.
const LOCAL_COPY_CHUNK_SIZE: usize = 1024 * 1024;
/// Emit progress updates at least this often, even if percentage barely moves.
const PROGRESS_REPORT_INTERVAL: Duration = Duration::from_secs(2);
/// Emit progress updates when another 5% bucket is crossed.
const PROGRESS_REPORT_PERCENT_BUCKET: u64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DownloadNotificationTarget {
    chat_id: i64,
    reply_to_message_id: i64,
    thread_id: Option<i64>,
}

fn build_get_updates_query(offset: Option<i64>) -> Vec<(&'static str, String)> {
    let mut query = vec![
        ("timeout", LONG_POLL_TIMEOUT.to_string()),
        (
            "allowed_updates",
            serde_json::json!(["message", "edited_message"]).to_string(),
        ),
    ];
    if let Some(off) = offset {
        query.push(("offset", off.to_string()));
    }
    query
}

fn file_size_mb(file_size: u64) -> u64 {
    file_size / 1024 / 1024
}

fn progress_percentage(downloaded: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (downloaded as f64 / total as f64) * 100.0
    }
}

fn progress_bucket(downloaded: u64, total: u64) -> u64 {
    if total == 0 {
        0
    } else {
        ((downloaded.saturating_mul(100)) / total) / PROGRESS_REPORT_PERCENT_BUCKET
    }
}

fn emit_progress_update(
    progress_callback: Option<&ProgressCallback>,
    file_id: &str,
    file_name: &str,
    total_bytes: u64,
    downloaded_bytes: u64,
    chat_id: i64,
    message_id: Option<i64>,
) {
    if let Some(callback) = progress_callback {
        callback(ProgressInfo {
            file_id: file_id.to_string(),
            file_name: file_name.to_string(),
            total_bytes,
            downloaded_bytes,
            percentage: progress_percentage(downloaded_bytes, total_bytes),
            chat_id,
            message_id,
        });
    }
}

fn telegram_message(update: &serde_json::Value) -> Option<&serde_json::Value> {
    update
        .get("message")
        .or_else(|| update.get("edited_message"))
}

fn telegram_sender_id(message: &serde_json::Value) -> Option<i64> {
    message
        .get("from")
        .and_then(|from| from.get("id"))
        .and_then(|id| id.as_i64())
        .or_else(|| {
            message
                .get("sender_chat")
                .and_then(|chat| chat.get("id"))
                .and_then(|id| id.as_i64())
        })
}

fn extract_download_notification_target(
    update: &serde_json::Value,
    allowed_users: &[String],
) -> Option<DownloadNotificationTarget> {
    let message = telegram_message(update)?;
    let sender_id = telegram_sender_id(message)?;
    if !allowed_users.is_empty()
        && !allowed_users
            .iter()
            .any(|user| user == &sender_id.to_string())
    {
        return None;
    }

    Some(DownloadNotificationTarget {
        chat_id: message.get("chat")?.get("id")?.as_i64()?,
        reply_to_message_id: message.get("message_id")?.as_i64()?,
        thread_id: message.get("message_thread_id").and_then(|id| id.as_i64()),
    })
}

fn message_will_attempt_download(
    message: &serde_json::Value,
    max_download_size: u64,
    use_local_api: bool,
) -> bool {
    if let Some(photos) = message.get("photo").and_then(|value| value.as_array()) {
        let file_size = photos
            .last()
            .and_then(|photo| photo.get("file_size"))
            .and_then(|size| size.as_u64())
            .unwrap_or(0);
        return file_size <= max_download_size;
    }

    if let Some(video) = message.get("video") {
        let file_size = video
            .get("file_size")
            .and_then(|size| size.as_u64())
            .unwrap_or(0);
        return file_size <= max_download_size && !should_skip_get_file(file_size, use_local_api);
    }

    if let Some(document) = message.get("document") {
        let file_size = document
            .get("file_size")
            .and_then(|size| size.as_u64())
            .unwrap_or(0);
        return file_size <= max_download_size && !should_skip_get_file(file_size, use_local_api);
    }

    false
}

fn update_will_attempt_download(
    update: &serde_json::Value,
    max_download_size: u64,
    use_local_api: bool,
) -> bool {
    telegram_message(update)
        .map(|message| message_will_attempt_download(message, max_download_size, use_local_api))
        .unwrap_or(false)
}

fn should_skip_get_file(file_size: u64, use_local_api: bool) -> bool {
    use_local_api && file_size > LOCAL_API_SAFE_GETFILE_LIMIT
}

fn get_file_safety_limit_mb() -> u64 {
    file_size_mb(LOCAL_API_SAFE_GETFILE_LIMIT)
}

fn prepend_caption(caption: Option<&str>, body: String) -> String {
    match caption.map(str::trim).filter(|caption| !caption.is_empty()) {
        Some(caption) => format!("{caption}\n\n{body}"),
        None => body,
    }
}

fn build_download_hint(
    file_id: &str,
    api_base_url: &str,
    use_local_api: bool,
    download_url: Option<String>,
    reason: Option<String>,
) -> TelegramDownloadHint {
    TelegramDownloadHint {
        strategy: "telegram_bot_api_file".to_string(),
        file_id: file_id.to_string(),
        api_base_url: api_base_url.to_string(),
        use_local_api,
        download_url,
        reason,
    }
}

fn build_single_message_batch(
    chat_id: i64,
    message_id: i64,
    caption: Option<&str>,
    item: TelegramMediaItem,
) -> TelegramMediaBatch {
    TelegramMediaBatch {
        batch_key: TelegramMediaBatch::single_message_key(chat_id, message_id),
        chat_id,
        message_id,
        media_group_id: String::new(),
        caption: caption
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string),
        items: vec![item],
    }
}

fn is_video_document(filename: &str, mime_type: &str) -> bool {
    if mime_type.starts_with("video/") {
        return true;
    }

    matches!(
        Path::new(filename)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase()),
        Some(ext) if matches!(ext.as_str(), "mp4" | "mov" | "mkv" | "avi" | "webm" | "m4v")
    )
}

/// Progress information for file downloads.
#[derive(Debug, Clone)]
pub struct ProgressInfo {
    pub file_id: String,
    pub file_name: String,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub percentage: f64,
    pub chat_id: i64,
    pub message_id: Option<i64>,
}

/// Callback type for download progress updates.
pub type ProgressCallback = Arc<dyn Fn(ProgressInfo) + Send + Sync>;

/// Telegram Bot API adapter using long-polling.
pub struct TelegramAdapter {
    /// SECURITY: Bot token is zeroized on drop to prevent memory disclosure.
    token: Zeroizing<String>,
    client: reqwest::Client,
    allowed_users: Vec<String>,
    poll_interval: Duration,
    /// Base URL for Telegram Bot API (supports proxies/mirrors).
    api_base_url: String,
    /// Bot username (without @), populated from `getMe` during `start()`.
    /// Used for @mention detection in group messages.
    bot_username: Arc<tokio::sync::RwLock<Option<String>>>,
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Whether to download files to local disk (default: false, only return URLs).
    download_enabled: bool,
    /// Directory for downloaded files.
    download_dir: PathBuf,
    /// Maximum file size to download (bytes). Files larger than this return URLs only.
    max_download_size: u64,
    /// Optional callback for download progress updates.
    progress_callback: Option<ProgressCallback>,
    /// Whether using Local Bot API Server (supports files >20MB).
    use_local_api: bool,
    /// Background media-group workers that should be cancelled on shutdown.
    background_tasks: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
}

impl TelegramAdapter {
    /// Create a new Telegram adapter.
    ///
    /// `token` is the raw bot token (read from env by the caller).
    /// `allowed_users` is the list of Telegram user IDs allowed to interact (empty = allow all).
    /// `api_url` overrides the Telegram Bot API base URL (for proxies/mirrors).
    pub fn new(
        token: String,
        allowed_users: Vec<String>,
        poll_interval: Duration,
        api_url: Option<String>,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let api_base_url = api_url
            .unwrap_or_else(|| DEFAULT_API_URL.to_string())
            .trim_end_matches('/')
            .to_string();
        Self {
            token: Zeroizing::new(token),
            client: reqwest::Client::new(),
            allowed_users,
            poll_interval,
            api_base_url,
            bot_username: Arc::new(tokio::sync::RwLock::new(None)),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            download_enabled: false,
            download_dir: std::env::temp_dir().join("openfang-telegram-downloads"),
            max_download_size: 2 * 1024 * 1024 * 1024, // 2GB default
            progress_callback: None,
            use_local_api: false,
            background_tasks: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Enable file downloads with custom configuration.
    pub fn with_download_config(
        mut self,
        enabled: bool,
        download_dir: Option<PathBuf>,
        max_size: Option<u64>,
        progress_callback: Option<ProgressCallback>,
    ) -> Self {
        self.download_enabled = enabled;
        if let Some(dir) = download_dir {
            self.download_dir = dir;
        }
        if let Some(size) = max_size {
            self.max_download_size = size;
        }
        self.progress_callback = progress_callback;
        self
    }

    /// Enable Local Bot API Server mode for large file downloads (>20MB).
    pub fn with_local_api(mut self, use_local: bool) -> Self {
        self.use_local_api = use_local;
        self
    }

    fn getme_error_hint(desc: &str) -> &'static str {
        let desc_lower = desc.to_lowercase();
        if desc_lower.contains("unauthorized") {
            " (Check that the bot token is correct. Get it from @BotFather on Telegram.)"
        } else if desc_lower.contains("not found") {
            " (The bot token format may be invalid. Expected format: 123456789:ABCdefGHI...)"
        } else {
            ""
        }
    }

    fn is_permanent_getme_error(desc: &str) -> bool {
        let desc_lower = desc.to_lowercase();
        desc_lower.contains("unauthorized") || desc_lower.contains("not found")
    }

    /// Validate the bot token by calling `getMe`.
    ///
    /// When using Local Bot API Server, retries up to 5 times with exponential backoff
    /// to allow the server time to fully initialize.
    pub async fn validate_token(&self) -> Result<String, Box<dyn std::error::Error>> {
        let url = format!("{}/bot{}/getMe", self.api_base_url, self.token.as_str());

        // Retry logic for Local Bot API Server startup delay
        let max_retries = if self.use_local_api { 5 } else { 1 };
        let mut last_error = None;

        for attempt in 0..max_retries {
            if attempt > 0 {
                let delay_ms = 1000 * (1 << (attempt - 1)); // 1s, 2s, 4s, 8s
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                debug!(
                    "Retrying Telegram getMe (attempt {}/{})",
                    attempt + 1,
                    max_retries
                );
            }

            match self.client.get(&url).send().await {
                Ok(resp) => match resp.json::<serde_json::Value>().await {
                    Ok(json) => {
                        if json["ok"].as_bool() == Some(true) {
                            let username = json["result"]["username"]
                                .as_str()
                                .ok_or("Missing username in getMe response")?
                                .to_string();
                            return Ok(username);
                        }

                        let desc = json["description"].as_str().unwrap_or("unknown error");
                        let hint = Self::getme_error_hint(desc);
                        let err_msg = format!("Telegram getMe failed: {desc}{hint}");
                        let permanent = Self::is_permanent_getme_error(desc);
                        last_error = Some(err_msg.clone());

                        // Local API startup can return transient getMe errors; retry only those.
                        if !self.use_local_api || permanent {
                            return Err(err_msg.into());
                        }
                    }
                    Err(e) => {
                        last_error = Some(format!("Failed to parse getMe response: {}", e));
                    }
                },
                Err(e) => {
                    last_error = Some(format!("Failed to connect to Telegram API: {}", e));
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| "Unknown error".to_string())
            .into())
    }

    /// Register the bot command menu shown by Telegram clients.
    async fn register_bot_commands(&self) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!(
            "{}/bot{}/setMyCommands",
            self.api_base_url,
            self.token.as_str()
        );
        let commands: Vec<serde_json::Value> = TELEGRAM_BOT_COMMANDS
            .iter()
            .map(|(command, description)| {
                serde_json::json!({
                    "command": command,
                    "description": description,
                })
            })
            .collect();

        let resp = self
            .client
            .post(&url)
            .json(&serde_json::json!({ "commands": commands }))
            .send()
            .await?;
        let status = resp.status();
        let body_text = resp.text().await?;

        if !status.is_success() {
            return Err(format!("Telegram setMyCommands failed ({status}): {body_text}").into());
        }

        let body: serde_json::Value = serde_json::from_str(&body_text)?;
        if body["ok"].as_bool() != Some(true) {
            let desc = body["description"].as_str().unwrap_or("unknown error");
            return Err(format!("Telegram setMyCommands returned ok=false: {desc}").into());
        }

        Ok(())
    }

    /// Call `sendMessage` on the Telegram API.
    ///
    /// When `thread_id` is provided, includes `message_thread_id` in the request
    /// so the message lands in the correct forum topic.
    /// When `metadata` contains `reply_to_message_id`, the message will be sent as a reply.
    /// When `metadata` contains `edit_message_id`, the message will edit an existing message.
    async fn api_send_message(
        &self,
        chat_id: i64,
        text: &str,
        thread_id: Option<i64>,
        metadata: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Check if this is an edit operation
        if let Some(meta) = metadata {
            if let Some(edit_msg_id) = meta.get("edit_message_id").and_then(|v| v.as_i64()) {
                // Use editMessageText instead of sendMessage
                let url = format!(
                    "{}/bot{}/editMessageText",
                    self.api_base_url,
                    self.token.as_str()
                );
                let sanitized = sanitize_telegram_html(text);
                let body = serde_json::json!({
                    "chat_id": chat_id,
                    "message_id": edit_msg_id,
                    "text": sanitized,
                    "parse_mode": "HTML",
                });
                let resp = self.client.post(&url).json(&body).send().await?;
                let status = resp.status();
                if !status.is_success() {
                    let body_text = resp.text().await.unwrap_or_default();
                    warn!("Telegram editMessageText failed ({status}): {body_text}");
                }
                return Ok(());
            }
        }

        let url = format!(
            "{}/bot{}/sendMessage",
            self.api_base_url,
            self.token.as_str()
        );

        // Sanitize: strip unsupported HTML tags so Telegram doesn't reject with 400.
        // Telegram only allows: b, i, u, s, tg-spoiler, a, code, pre, blockquote.
        // Any other tag (e.g. <name>, <thinking>) causes a 400 Bad Request.
        let sanitized = sanitize_telegram_html(text);

        // Telegram has a 4096 character limit per message — split if needed
        let chunks = split_message(&sanitized, 4096);
        for chunk in chunks {
            let mut body = serde_json::json!({
                "chat_id": chat_id,
                "text": chunk,
                "parse_mode": "HTML",
            });
            if let Some(tid) = thread_id {
                body["message_thread_id"] = serde_json::json!(tid);
            }
            // Add reply_to_message_id if present in metadata
            if let Some(meta) = metadata {
                if let Some(reply_id) = meta.get("reply_to_message_id").and_then(|v| v.as_i64()) {
                    body["reply_to_message_id"] = serde_json::json!(reply_id);
                }
            }

            let resp = self.client.post(&url).json(&body).send().await?;
            let status = resp.status();
            if !status.is_success() {
                let body_text = resp.text().await.unwrap_or_default();
                warn!("Telegram sendMessage failed ({status}): {body_text}");
            }
        }
        Ok(())
    }

    /// Call `sendPhoto` on the Telegram API.
    async fn api_send_photo(
        &self,
        chat_id: i64,
        photo_url: &str,
        caption: Option<&str>,
        thread_id: Option<i64>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/bot{}/sendPhoto", self.api_base_url, self.token.as_str());
        let mut body = serde_json::json!({
            "chat_id": chat_id,
            "photo": photo_url,
        });
        if let Some(cap) = caption {
            body["caption"] = serde_json::Value::String(cap.to_string());
            body["parse_mode"] = serde_json::Value::String("HTML".to_string());
        }
        if let Some(tid) = thread_id {
            body["message_thread_id"] = serde_json::json!(tid);
        }
        let resp = self.client.post(&url).json(&body).send().await?;
        if !resp.status().is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            warn!("Telegram sendPhoto failed: {body_text}");
        }
        Ok(())
    }

    /// Call `sendDocument` on the Telegram API.
    async fn api_send_document(
        &self,
        chat_id: i64,
        document_url: &str,
        filename: &str,
        thread_id: Option<i64>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!(
            "{}/bot{}/sendDocument",
            self.api_base_url,
            self.token.as_str()
        );
        let mut body = serde_json::json!({
            "chat_id": chat_id,
            "document": document_url,
            "caption": filename,
        });
        if let Some(tid) = thread_id {
            body["message_thread_id"] = serde_json::json!(tid);
        }
        let resp = self.client.post(&url).json(&body).send().await?;
        if !resp.status().is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            warn!("Telegram sendDocument failed: {body_text}");
        }
        Ok(())
    }

    /// Call `sendDocument` with multipart upload for local file data.
    ///
    /// Used by the proactive `channel_send` tool when `file_path` is provided.
    /// Uploads raw bytes as a multipart form instead of passing a URL.
    async fn api_send_document_upload(
        &self,
        chat_id: i64,
        data: Vec<u8>,
        filename: &str,
        mime_type: &str,
        thread_id: Option<i64>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!(
            "{}/bot{}/sendDocument",
            self.api_base_url,
            self.token.as_str()
        );

        let file_part = reqwest::multipart::Part::bytes(data)
            .file_name(filename.to_string())
            .mime_str(mime_type)?;

        let mut form = reqwest::multipart::Form::new()
            .text("chat_id", chat_id.to_string())
            .part("document", file_part);

        if let Some(tid) = thread_id {
            form = form.text("message_thread_id", tid.to_string());
        }

        let resp = self.client.post(&url).multipart(form).send().await?;
        if !resp.status().is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            warn!("Telegram sendDocument upload failed: {body_text}");
        }
        Ok(())
    }

    /// Call `sendVoice` on the Telegram API.
    async fn api_send_voice(
        &self,
        chat_id: i64,
        voice_url: &str,
        thread_id: Option<i64>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/bot{}/sendVoice", self.api_base_url, self.token.as_str());
        let mut body = serde_json::json!({
            "chat_id": chat_id,
            "voice": voice_url,
        });
        if let Some(tid) = thread_id {
            body["message_thread_id"] = serde_json::json!(tid);
        }
        let resp = self.client.post(&url).json(&body).send().await?;
        if !resp.status().is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            warn!("Telegram sendVoice failed: {body_text}");
        }
        Ok(())
    }

    /// Call `sendLocation` on the Telegram API.
    async fn api_send_location(
        &self,
        chat_id: i64,
        lat: f64,
        lon: f64,
        thread_id: Option<i64>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!(
            "{}/bot{}/sendLocation",
            self.api_base_url,
            self.token.as_str()
        );
        let mut body = serde_json::json!({
            "chat_id": chat_id,
            "latitude": lat,
            "longitude": lon,
        });
        if let Some(tid) = thread_id {
            body["message_thread_id"] = serde_json::json!(tid);
        }
        let resp = self.client.post(&url).json(&body).send().await?;
        if !resp.status().is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            warn!("Telegram sendLocation failed: {body_text}");
        }
        Ok(())
    }

    /// Call `sendChatAction` to show "typing..." indicator.
    ///
    /// When `thread_id` is provided, the typing indicator appears in the forum topic.
    async fn api_send_typing(
        &self,
        chat_id: i64,
        thread_id: Option<i64>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!(
            "{}/bot{}/sendChatAction",
            self.api_base_url,
            self.token.as_str()
        );
        let mut body = serde_json::json!({
            "chat_id": chat_id,
            "action": "typing",
        });
        if let Some(tid) = thread_id {
            body["message_thread_id"] = serde_json::json!(tid);
        }
        let _ = self.client.post(&url).json(&body).send().await?;
        Ok(())
    }

    /// Call `setMessageReaction` on the Telegram API (fire-and-forget).
    ///
    /// Sets or replaces the bot's emoji reaction on a message. Each new call
    /// automatically replaces the previous reaction, so there is no need to
    /// explicitly remove old ones.
    fn fire_reaction(&self, chat_id: i64, message_id: i64, emoji: &str) {
        let url = format!(
            "{}/bot{}/setMessageReaction",
            self.api_base_url,
            self.token.as_str()
        );
        let body = serde_json::json!({
            "chat_id": chat_id,
            "message_id": message_id,
            "reaction": [{"type": "emoji", "emoji": emoji}],
        });
        let client = self.client.clone();
        tokio::spawn(async move {
            match client.post(&url).json(&body).send().await {
                Ok(resp) if !resp.status().is_success() => {
                    let body_text = resp.text().await.unwrap_or_default();
                    debug!("Telegram setMessageReaction failed: {body_text}");
                }
                Err(e) => {
                    debug!("Telegram setMessageReaction error: {e}");
                }
                _ => {}
            }
        });
    }
}

impl TelegramAdapter {
    /// Internal helper: send content with optional forum-topic thread_id.
    ///
    /// Both `send()` and `send_in_thread()` delegate here. When `thread_id` is
    /// `Some(id)`, every outbound Telegram API call includes `message_thread_id`
    /// so the message lands in the correct forum topic.
    async fn send_content(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
        thread_id: Option<i64>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let chat_id: i64 = user
            .platform_id
            .parse()
            .map_err(|_| format!("Invalid Telegram chat_id: {}", user.platform_id))?;

        // Extract metadata from user
        let metadata = user.metadata.as_ref();

        match content {
            ChannelContent::Text(text) => {
                self.api_send_message(chat_id, &text, thread_id, metadata)
                    .await?;
            }
            ChannelContent::Image { url, caption } => {
                self.api_send_photo(chat_id, &url, caption.as_deref(), thread_id)
                    .await?;
            }
            ChannelContent::File { url, filename } => {
                self.api_send_document(chat_id, &url, &filename, thread_id)
                    .await?;
            }
            ChannelContent::FileData {
                data,
                filename,
                mime_type,
            } => {
                self.api_send_document_upload(chat_id, data, &filename, &mime_type, thread_id)
                    .await?;
            }
            ChannelContent::Voice { url, .. } => {
                self.api_send_voice(chat_id, &url, thread_id).await?;
            }
            ChannelContent::Location { lat, lon } => {
                self.api_send_location(chat_id, lat, lon, thread_id).await?;
            }
            ChannelContent::Command { name, args } => {
                let text = format!("/{name} {}", args.join(" "));
                self.api_send_message(chat_id, text.trim(), thread_id, metadata)
                    .await?;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl ChannelAdapter for TelegramAdapter {
    fn name(&self) -> &str {
        "telegram"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Telegram
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        // Validate token first (fail fast) and store bot username for mention detection
        let bot_name = self.validate_token().await?;
        {
            let mut username = self.bot_username.write().await;
            *username = Some(bot_name.clone());
        }
        info!("Telegram bot @{bot_name} connected");

        // Clear any existing webhook to avoid 409 Conflict during getUpdates polling.
        // This is necessary when the daemon restarts — the old polling session may
        // still be active on Telegram's side for ~30s, causing 409 errors.
        {
            let delete_url = format!(
                "{}/bot{}/deleteWebhook",
                self.api_base_url,
                self.token.as_str()
            );
            match self
                .client
                .post(&delete_url)
                .json(&serde_json::json!({"drop_pending_updates": false}))
                .send()
                .await
            {
                Ok(_) => info!("Telegram: cleared webhook, polling mode active"),
                Err(e) => tracing::warn!("Telegram: deleteWebhook failed (non-fatal): {e}"),
            }
        }

        if let Err(e) = self.register_bot_commands().await {
            warn!("Telegram: setMyCommands failed (non-fatal): {e}");
        } else {
            info!(
                "Telegram: registered {} bot commands",
                TELEGRAM_BOT_COMMANDS.len()
            );
        }

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);

        let token = self.token.clone();
        let client = self.client.clone();
        let allowed_users = self.allowed_users.clone();
        let poll_interval = self.poll_interval;
        let api_base_url = self.api_base_url.clone();
        let bot_username = self.bot_username.clone();
        let mut shutdown = self.shutdown_rx.clone();
        let download_enabled = self.download_enabled;
        let download_dir = self.download_dir.clone();
        let max_download_size = self.max_download_size;
        let progress_callback = self.progress_callback.clone();
        let use_local_api = self.use_local_api;
        let background_tasks = self.background_tasks.clone();

        tokio::spawn(async move {
            let mut offset: Option<i64> = None;
            let mut backoff = INITIAL_BACKOFF;
            // Media group deduplication: group_id -> (updates, last_seen_time)
            let mut media_groups: HashMap<String, (Vec<serde_json::Value>, tokio::time::Instant)> =
                HashMap::new();
            const MEDIA_GROUP_WAIT_MS: u64 = 500; // Wait 500ms for all media in a group

            info!("Telegram polling loop started");

            loop {
                // Check shutdown
                if *shutdown.borrow() {
                    break;
                }

                // Process pending media groups that have timed out
                let now = tokio::time::Instant::now();
                let mut completed_groups = Vec::new();
                for (group_id, (_updates, last_seen)) in &media_groups {
                    if now.duration_since(*last_seen).as_millis() >= MEDIA_GROUP_WAIT_MS as u128 {
                        completed_groups.push(group_id.clone());
                    }
                }

                for group_id in completed_groups {
                    if let Some((updates, _)) = media_groups.remove(&group_id) {
                        // Spawn download + merge in background so polling loop is not blocked
                        let token_clone = token.clone();
                        let client_clone = client.clone();
                        let api_base_url_clone = api_base_url.clone();
                        let allowed_users_clone = allowed_users.clone();
                        let bot_username_clone = bot_username.read().await.clone();
                        let download_dir_clone = download_dir.clone();
                        let progress_callback_clone = progress_callback.clone();
                        let tx_clone = tx.clone();
                        let notify_target = if download_enabled
                            && updates.iter().any(|update| {
                                update_will_attempt_download(
                                    update,
                                    max_download_size,
                                    use_local_api,
                                )
                            }) {
                            updates.first().and_then(|update| {
                                extract_download_notification_target(update, &allowed_users_clone)
                            })
                        } else {
                            None
                        };

                        let handle = tokio::spawn(async move {
                            // Merge media group into a single message (may download files)
                            if let Some(merged_msg) = merge_media_group_updates(
                                &updates,
                                &allowed_users_clone,
                                token_clone.as_str(),
                                &client_clone,
                                &api_base_url_clone,
                                bot_username_clone.as_deref(),
                                download_enabled,
                                &download_dir_clone,
                                max_download_size,
                                progress_callback_clone.as_ref(),
                                use_local_api,
                                notify_target,
                            )
                            .await
                            {
                                debug!(
                                    "Telegram media group ({} items) from {}: {:?}",
                                    updates.len(),
                                    merged_msg.sender.display_name,
                                    merged_msg.content
                                );
                                let _ = tx_clone.send(merged_msg).await;
                            }
                        });

                        let mut tasks = background_tasks.lock().await;
                        tasks.retain(|task| !task.is_finished());
                        tasks.push(handle);
                    }
                }

                // Build getUpdates request
                let url = format!("{}/bot{}/getUpdates", api_base_url, token.as_str());
                let query = build_get_updates_query(offset);

                // Make the request with a timeout slightly longer than the long-poll timeout
                let request_timeout = Duration::from_secs(LONG_POLL_TIMEOUT + 10);
                let result = tokio::select! {
                    res = async {
                        client
                            .get(&url)
                            .query(&query)
                            .timeout(request_timeout)
                            .send()
                            .await
                    } => res,
                    _ = shutdown.changed() => {
                        break;
                    }
                };

                let resp = match result {
                    Ok(resp) => resp,
                    Err(e) => {
                        warn!("Telegram getUpdates network error: {e}, retrying in {backoff:?}");
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(MAX_BACKOFF);
                        continue;
                    }
                };

                let status = resp.status();

                // Handle rate limiting
                if status.as_u16() == 429 {
                    let body: serde_json::Value = resp.json().await.unwrap_or_default();
                    let retry_after = body["parameters"]["retry_after"].as_u64().unwrap_or(5);
                    warn!("Telegram rate limited, retry after {retry_after}s");
                    tokio::time::sleep(Duration::from_secs(retry_after)).await;
                    continue;
                }

                // Handle conflict (another bot instance or stale session polling).
                // On daemon restart, the old long-poll may still be active on Telegram's
                // side for up to 30s. Retry with backoff instead of stopping permanently.
                if status.as_u16() == 409 {
                    warn!("Telegram 409 Conflict — stale polling session, retrying in {backoff:?}");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                    continue;
                }

                if !status.is_success() {
                    let body_text = resp.text().await.unwrap_or_default();
                    warn!("Telegram getUpdates failed ({status}): {body_text}, retrying in {backoff:?}");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                    continue;
                }

                // Parse response
                let body: serde_json::Value = match resp.json().await {
                    Ok(v) => v,
                    Err(e) => {
                        warn!("Telegram getUpdates parse error: {e}");
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(MAX_BACKOFF);
                        continue;
                    }
                };

                // Reset backoff on success
                backoff = INITIAL_BACKOFF;

                if body["ok"].as_bool() != Some(true) {
                    warn!("Telegram getUpdates returned ok=false");
                    tokio::time::sleep(poll_interval).await;
                    continue;
                }

                let updates = match body["result"].as_array() {
                    Some(arr) => arr,
                    None => {
                        tokio::time::sleep(poll_interval).await;
                        continue;
                    }
                };

                if !updates.is_empty() {
                    info!(
                        count = updates.len(),
                        next_offset = offset,
                        "Telegram getUpdates returned messages"
                    );
                }

                for update in updates {
                    // Track offset for dedup
                    if let Some(update_id) = update["update_id"].as_i64() {
                        offset = Some(update_id + 1);
                    }

                    // Check if this update is part of a media group
                    let message = update
                        .get("message")
                        .or_else(|| update.get("edited_message"));
                    let media_group_id = message
                        .and_then(|m| m.get("media_group_id"))
                        .and_then(|v| v.as_str())
                        .map(String::from);

                    if let Some(group_id) = media_group_id {
                        // Add to media group buffer
                        let entry = media_groups
                            .entry(group_id)
                            .or_insert_with(|| (Vec::new(), now));
                        entry.0.push(update.clone());
                        entry.1 = now; // Update last seen time
                        continue; // Don't process immediately
                    }

                    // Not a media group — process immediately
                    let bot_uname = bot_username.read().await.clone();
                    let notify_target = if download_enabled
                        && update_will_attempt_download(update, max_download_size, use_local_api)
                    {
                        extract_download_notification_target(update, &allowed_users)
                    } else {
                        None
                    };
                    let notify_msg_id = if download_enabled && progress_callback.is_some() {
                        match notify_target {
                            Some(target) => {
                                telegram_send_notification_raw(
                                    &client,
                                    &api_base_url,
                                    token.as_str(),
                                    target,
                                    "收到媒体，正在处理...",
                                )
                                .await
                            }
                            None => None,
                        }
                    } else {
                        None
                    };
                    let msg = match parse_telegram_update(
                        update,
                        &allowed_users,
                        token.as_str(),
                        &client,
                        &api_base_url,
                        bot_uname.as_deref(),
                        download_enabled,
                        &download_dir,
                        max_download_size,
                        progress_callback.as_ref(),
                        use_local_api,
                        notify_msg_id,
                    )
                    .await
                    {
                        Some(m) => m,
                        None => continue, // filtered out or unparseable
                    };

                    debug!(
                        "Telegram message from {}: {:?}",
                        msg.sender.display_name, msg.content
                    );

                    if tx.send(msg).await.is_err() {
                        // Receiver dropped — bridge is shutting down
                        warn!(
                            "Telegram bridge consumer dropped while sending message; stopping polling loop"
                        );
                        return;
                    }
                }

                // Small delay between polls even on success to avoid tight loops
                tokio::time::sleep(poll_interval).await;
            }

            info!("Telegram polling loop stopped");
        });

        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        Ok(Box::pin(stream))
    }

    async fn send(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send_content(user, content, None).await
    }

    async fn send_typing(&self, user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        let chat_id: i64 = user
            .platform_id
            .parse()
            .map_err(|_| format!("Invalid Telegram chat_id: {}", user.platform_id))?;
        self.api_send_typing(chat_id, None).await
    }

    async fn send_in_thread(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
        thread_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let tid: Option<i64> = thread_id.parse().ok();
        self.send_content(user, content, tid).await
    }

    async fn send_reaction(
        &self,
        user: &ChannelUser,
        message_id: &str,
        reaction: &LifecycleReaction,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let chat_id: i64 = user
            .platform_id
            .parse()
            .map_err(|_| format!("Invalid Telegram chat_id: {}", user.platform_id))?;
        let msg_id: i64 = message_id
            .parse()
            .map_err(|_| format!("Invalid Telegram message_id: {message_id}"))?;
        self.fire_reaction(chat_id, msg_id, &reaction.emoji);
        Ok(())
    }

    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.shutdown_tx.send(true);
        let mut tasks = self.background_tasks.lock().await;
        for handle in tasks.drain(..) {
            handle.abort();
        }
        Ok(())
    }
}

/// Merge multiple updates from the same media group into a single ChannelMessage.
/// Takes the first update's metadata (sender, chat, timestamp) and combines all media URLs.
#[allow(clippy::too_many_arguments)]
async fn merge_media_group_updates(
    updates: &[serde_json::Value],
    allowed_users: &[String],
    token: &str,
    client: &reqwest::Client,
    api_base_url: &str,
    bot_username: Option<&str>,
    download_enabled: bool,
    download_dir: &Path,
    max_download_size: u64,
    progress_callback: Option<&ProgressCallback>,
    use_local_api: bool,
    notify_target: Option<DownloadNotificationTarget>,
) -> Option<ChannelMessage> {
    if updates.is_empty() {
        return None;
    }

    let first_message = updates[0]
        .get("message")
        .or_else(|| updates[0].get("edited_message"))?;
    let chat_id = first_message.get("chat")?.get("id")?.as_i64()?;
    let message_id = first_message.get("message_id")?.as_i64()?;
    let media_group_id = first_message.get("media_group_id")?.as_str()?.to_string();

    // Use the first update as the base message
    let first_msg = parse_telegram_update(
        &updates[0],
        allowed_users,
        token,
        client,
        api_base_url,
        bot_username,
        false,
        download_dir,
        max_download_size,
        None,
        use_local_api,
        None,
    )
    .await?;

    let notify_msg_id = if download_enabled && progress_callback.is_some() {
        match notify_target {
            Some(target) => {
                telegram_send_notification_raw(
                    client,
                    api_base_url,
                    token,
                    target,
                    "收到媒体，正在处理...",
                )
                .await
            }
            None => None,
        }
    } else {
        None
    };

    // Collect all media items with structured metadata
    let mut media_items = Vec::new();
    let mut combined_caption = String::new();
    let mut messages = Vec::with_capacity(updates.len());

    for update in updates {
        let message = update
            .get("message")
            .or_else(|| update.get("edited_message"))?;

        // Debug: log message structure
        debug!(
            "Media group item keys: {:?}",
            message.as_object().map(|o| o.keys().collect::<Vec<_>>())
        );

        // Collect caption (usually only the first item has it)
        if let Some(caption) = message.get("caption").and_then(|v| v.as_str()) {
            if !caption.is_empty() && combined_caption.is_empty() {
                combined_caption = caption.to_string();
            }
        }

        messages.push(message);
    }

    for message in &messages {
        // Extract video
        if let Some(video) = message.get("video") {
            let file_id = video.get("file_id")?.as_str()?.to_string();
            let file_size = video.get("file_size").and_then(|v| v.as_u64()).unwrap_or(0);
            let duration = video.get("duration").and_then(|v| v.as_u64()).unwrap_or(0);
            let chat_id = message.get("chat")?.get("id")?.as_i64()?;
            let _ = message.get("message_id")?.as_i64()?;

            // file_size=0 means Telegram did not report the size (e.g. streaming video).
            // We cannot know the real size, so conservatively defer to project-side download.
            if file_size == 0 {
                info!(
                    "Video {} has unknown file size (0), deferring to project download",
                    file_id
                );
                media_items.push(TelegramMediaItem {
                    kind: MediaItemKind::Video,
                    file_id: file_id.clone(),
                    original_name: None,
                    file_size: 0,
                    duration_seconds: Some(duration),
                    status: MediaItemStatus::NeedsProjectDownload,
                    local_path: None,
                    download_hint: Some(build_download_hint(
                        &file_id,
                        api_base_url,
                        use_local_api,
                        None,
                        Some("file size unknown, deferred to project download".to_string()),
                    )),
                });
                continue;
            }

            if should_skip_get_file(file_size, use_local_api) {
                info!(
                    "Video {} ({} MB) exceeds safe getFile limit ({} MB), skipping download to prevent Local Bot API restarts",
                    file_id,
                    file_size_mb(file_size),
                    get_file_safety_limit_mb()
                );
                media_items.push(TelegramMediaItem {
                    kind: MediaItemKind::Video,
                    file_id: file_id.clone(),
                    original_name: None,
                    file_size,
                    duration_seconds: Some(duration),
                    status: MediaItemStatus::SkippedSafeLimit,
                    local_path: None,
                    download_hint: Some(build_download_hint(
                        &file_id,
                        api_base_url,
                        use_local_api,
                        None,
                        Some(format!(
                            "video exceeds {}MB safe getFile limit",
                            get_file_safety_limit_mb()
                        )),
                    )),
                });
                continue;
            }

            match telegram_get_file_info(
                token,
                client,
                &file_id,
                api_base_url,
                use_local_api,
                file_size,
            )
            .await
            {
                Some(file_info) => {
                    // Check if download is enabled and file size is within limit
                    if download_enabled && file_size <= max_download_size {
                        match download_file(
                            client,
                            &file_info,
                            download_dir,
                            progress_callback,
                            chat_id,
                            notify_msg_id,
                        )
                        .await
                        {
                            Ok(local_path) => {
                                media_items.push(TelegramMediaItem {
                                    kind: MediaItemKind::Video,
                                    file_id: file_id.clone(),
                                    original_name: None,
                                    file_size,
                                    duration_seconds: Some(duration),
                                    status: MediaItemStatus::Ready,
                                    local_path: Some(local_path.display().to_string()),
                                    download_hint: None,
                                });
                            }
                            Err(e) => {
                                warn!("Failed to download video {}: {}", file_id, e);
                                media_items.push(TelegramMediaItem {
                                    kind: MediaItemKind::Video,
                                    file_id: file_id.clone(),
                                    original_name: None,
                                    file_size,
                                    duration_seconds: Some(duration),
                                    status: MediaItemStatus::DownloadFailed,
                                    local_path: None,
                                    download_hint: Some(build_download_hint(
                                        &file_id,
                                        api_base_url,
                                        use_local_api,
                                        Some(file_info.download_url.clone()),
                                        Some(format!("bridge download failed: {e}")),
                                    )),
                                });
                            }
                        }
                    } else {
                        let reason = if file_size > max_download_size {
                            info!(
                                "Video {} ({} bytes) exceeds max download size, returning URL only",
                                file_id, file_size
                            );
                            format!(
                                "video exceeds bridge max_download_size ({} bytes)",
                                max_download_size
                            )
                        } else {
                            "bridge download disabled".to_string()
                        };
                        media_items.push(TelegramMediaItem {
                            kind: MediaItemKind::Video,
                            file_id: file_id.clone(),
                            original_name: None,
                            file_size,
                            duration_seconds: Some(duration),
                            status: MediaItemStatus::NeedsProjectDownload,
                            local_path: None,
                            download_hint: Some(build_download_hint(
                                &file_id,
                                api_base_url,
                                use_local_api,
                                Some(file_info.download_url),
                                Some(reason),
                            )),
                        });
                    }
                }
                None => {
                    // getFile returned None: either the file exceeds API limits or a network error occurred.
                    // For the official API, files >20MB cannot be resolved via getFile — treat as NeedsProjectDownload.
                    // For the local API, this is a genuine failure.
                    let (status, reason) =
                        if !use_local_api && file_size > OFFICIAL_API_GETFILE_LIMIT {
                            (
                                MediaItemStatus::NeedsProjectDownload,
                                format!(
                                    "video exceeds official API getFile limit ({} MB)",
                                    OFFICIAL_API_GETFILE_LIMIT / 1024 / 1024
                                ),
                            )
                        } else if !use_local_api && file_size == 0 {
                            (
                                MediaItemStatus::NeedsProjectDownload,
                                "file size unknown via official API, deferred to project download"
                                    .to_string(),
                            )
                        } else {
                            (
                                MediaItemStatus::DownloadFailed,
                                "failed to resolve file path via getFile".to_string(),
                            )
                        };
                    info!(
                        "Video {} (size: {} bytes) getFile returned None: {}",
                        file_id, file_size, reason
                    );
                    media_items.push(TelegramMediaItem {
                        kind: MediaItemKind::Video,
                        file_id: file_id.clone(),
                        original_name: None,
                        file_size,
                        duration_seconds: Some(duration),
                        status,
                        local_path: None,
                        download_hint: Some(build_download_hint(
                            &file_id,
                            api_base_url,
                            use_local_api,
                            None,
                            Some(reason),
                        )),
                    });
                }
            }

            continue;
        }

        // Extract document (videos can also be sent as documents)
        if let Some(document) = message.get("document") {
            let file_id = document.get("file_id")?.as_str()?.to_string();
            let filename = document
                .get("file_name")
                .and_then(|v| v.as_str())
                .map(String::from);
            let file_size = document
                .get("file_size")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let mime_type = document
                .get("mime_type")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let chat_id = message.get("chat")?.get("id")?.as_i64()?;
            let _ = message.get("message_id")?.as_i64()?;

            // Check if this is a video document
            let is_video = mime_type.starts_with("video/");
            let kind = if is_video {
                MediaItemKind::Video
            } else {
                MediaItemKind::Document
            };

            if should_skip_get_file(file_size, use_local_api) {
                info!(
                    "Document {} ({} MB) exceeds safe getFile limit ({} MB), skipping download to prevent Local Bot API restarts",
                    file_id,
                    file_size_mb(file_size),
                    get_file_safety_limit_mb()
                );
                media_items.push(TelegramMediaItem {
                    kind,
                    file_id: file_id.clone(),
                    original_name: filename,
                    file_size,
                    duration_seconds: None,
                    status: MediaItemStatus::SkippedSafeLimit,
                    local_path: None,
                    download_hint: Some(build_download_hint(
                        &file_id,
                        api_base_url,
                        use_local_api,
                        None,
                        Some(format!(
                            "document exceeds {}MB safe getFile limit",
                            get_file_safety_limit_mb()
                        )),
                    )),
                });
                continue;
            }

            match telegram_get_file_info(
                token,
                client,
                &file_id,
                api_base_url,
                use_local_api,
                file_size,
            )
            .await
            {
                Some(file_info) => {
                    // Check if download is enabled and file size is within limit
                    if download_enabled && file_size <= max_download_size {
                        match download_file(
                            client,
                            &file_info,
                            download_dir,
                            progress_callback,
                            chat_id,
                            notify_msg_id,
                        )
                        .await
                        {
                            Ok(local_path) => {
                                media_items.push(TelegramMediaItem {
                                    kind: kind.clone(),
                                    file_id: file_id.clone(),
                                    original_name: filename,
                                    file_size,
                                    duration_seconds: None,
                                    status: MediaItemStatus::Ready,
                                    local_path: Some(local_path.display().to_string()),
                                    download_hint: None,
                                });
                            }
                            Err(e) => {
                                warn!("Failed to download document {}: {}", file_id, e);
                                media_items.push(TelegramMediaItem {
                                    kind: kind.clone(),
                                    file_id: file_id.clone(),
                                    original_name: filename,
                                    file_size,
                                    duration_seconds: None,
                                    status: MediaItemStatus::DownloadFailed,
                                    local_path: None,
                                    download_hint: Some(build_download_hint(
                                        &file_id,
                                        api_base_url,
                                        use_local_api,
                                        Some(file_info.download_url.clone()),
                                        Some(format!("bridge download failed: {e}")),
                                    )),
                                });
                            }
                        }
                    } else {
                        let reason = if file_size > max_download_size {
                            info!(
                                "Document {} ({} bytes) exceeds max download size, returning URL only",
                                file_id, file_size
                            );
                            format!(
                                "document exceeds bridge max_download_size ({} bytes)",
                                max_download_size
                            )
                        } else {
                            "bridge download disabled".to_string()
                        };
                        media_items.push(TelegramMediaItem {
                            kind: kind.clone(),
                            file_id: file_id.clone(),
                            original_name: filename,
                            file_size,
                            duration_seconds: None,
                            status: MediaItemStatus::NeedsProjectDownload,
                            local_path: None,
                            download_hint: Some(build_download_hint(
                                &file_id,
                                api_base_url,
                                use_local_api,
                                Some(file_info.download_url),
                                Some(reason),
                            )),
                        });
                    }
                }
                None => {
                    info!(
                        "Skipped document file_id: {} (size: {} bytes, mime: {}) - likely exceeds safe limit",
                        file_id, file_size, mime_type
                    );
                    media_items.push(TelegramMediaItem {
                        kind,
                        file_id: file_id.clone(),
                        original_name: filename,
                        file_size,
                        duration_seconds: None,
                        status: MediaItemStatus::DownloadFailed,
                        local_path: None,
                        download_hint: Some(build_download_hint(
                            &file_id,
                            api_base_url,
                            use_local_api,
                            None,
                            Some("failed to resolve file path via getFile".to_string()),
                        )),
                    });
                }
            }

            continue;
        }

        // Extract photo
        if let Some(photos) = message["photo"].as_array() {
            let file_id = photos.last()?.get("file_id")?.as_str()?.to_string();
            let file_size = photos
                .last()?
                .get("file_size")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let chat_id = message.get("chat")?.get("id")?.as_i64()?;
            let _ = message.get("message_id")?.as_i64()?;
            match telegram_get_file_info(
                token,
                client,
                &file_id,
                api_base_url,
                use_local_api,
                file_size,
            )
            .await
            {
                Some(file_info) => {
                    if download_enabled && file_size <= max_download_size {
                        match download_file(
                            client,
                            &file_info,
                            download_dir,
                            progress_callback,
                            chat_id,
                            notify_msg_id,
                        )
                        .await
                        {
                            Ok(local_path) => {
                                media_items.push(TelegramMediaItem {
                                    kind: MediaItemKind::Image,
                                    file_id: file_id.clone(),
                                    original_name: None,
                                    file_size,
                                    duration_seconds: None,
                                    status: MediaItemStatus::Ready,
                                    local_path: Some(local_path.display().to_string()),
                                    download_hint: None,
                                });
                            }
                            Err(e) => {
                                warn!("Failed to download photo {}: {}", file_id, e);
                                media_items.push(TelegramMediaItem {
                                    kind: MediaItemKind::Image,
                                    file_id: file_id.clone(),
                                    original_name: None,
                                    file_size,
                                    duration_seconds: None,
                                    status: MediaItemStatus::DownloadFailed,
                                    local_path: None,
                                    download_hint: Some(build_download_hint(
                                        &file_id,
                                        api_base_url,
                                        use_local_api,
                                        Some(file_info.download_url),
                                        Some(format!("bridge download failed: {e}")),
                                    )),
                                });
                            }
                        }
                    } else {
                        media_items.push(TelegramMediaItem {
                            kind: MediaItemKind::Image,
                            file_id: file_id.clone(),
                            original_name: None,
                            file_size,
                            duration_seconds: None,
                            status: MediaItemStatus::NeedsProjectDownload,
                            local_path: None,
                            download_hint: Some(build_download_hint(
                                &file_id,
                                api_base_url,
                                use_local_api,
                                Some(file_info.download_url),
                                Some(if file_size > max_download_size {
                                    format!(
                                        "image exceeds bridge max_download_size ({} bytes)",
                                        max_download_size
                                    )
                                } else {
                                    "bridge download disabled".to_string()
                                }),
                            )),
                        });
                    }
                }
                None => {
                    warn!("Failed to get photo URL for file_id: {}", file_id);
                    media_items.push(TelegramMediaItem {
                        kind: MediaItemKind::Image,
                        file_id: file_id.clone(),
                        original_name: None,
                        file_size,
                        duration_seconds: None,
                        status: MediaItemStatus::DownloadFailed,
                        local_path: None,
                        download_hint: Some(build_download_hint(
                            &file_id,
                            api_base_url,
                            use_local_api,
                            None,
                            Some("failed to resolve file path via getFile".to_string()),
                        )),
                    });
                }
            }
        }
    }

    // Build TelegramMediaBatch structure
    let batch_key = TelegramMediaBatch::stable_batch_key(chat_id, &media_group_id);

    let batch = TelegramMediaBatch {
        batch_key: batch_key.clone(),
        chat_id,
        message_id,
        media_group_id,
        caption: if combined_caption.is_empty() {
            None
        } else {
            Some(combined_caption)
        },
        items: media_items,
    };

    if let Some(notify_msg_id) = notify_msg_id {
        let downloaded_any = batch
            .items
            .iter()
            .any(|item| item.status == MediaItemStatus::Ready);
        if !downloaded_any {
            let _ = telegram_edit_notification_raw(
                client,
                api_base_url,
                token,
                chat_id,
                notify_msg_id,
                &media_group_notification_complete_text(&batch),
            )
            .await;
        }
    }

    // Generate short summary text
    let summary_text = batch.summary();

    // Serialize batch to metadata
    let mut metadata = first_msg.metadata.clone();
    metadata.insert(
        "telegram_media_batch".to_string(),
        serde_json::to_value(&batch).ok()?,
    );

    Some(ChannelMessage {
        content: ChannelContent::Text(summary_text),
        metadata,
        ..first_msg
    })
}

fn media_group_notification_complete_text(batch: &TelegramMediaBatch) -> String {
    format!("✅ 媒体已接收\n{}", batch.summary())
}

/// Send a Telegram message directly (without TelegramAdapter) and return the sent message_id.
/// Used to send immediate notifications before long-running downloads.
async fn telegram_send_notification_raw(
    client: &reqwest::Client,
    api_base_url: &str,
    token: &str,
    target: DownloadNotificationTarget,
    text: &str,
) -> Option<i64> {
    let url = format!("{}/bot{}/sendMessage", api_base_url, token);
    let mut body = serde_json::json!({
        "chat_id": target.chat_id,
        "text": text,
    });
    body["reply_to_message_id"] = serde_json::json!(target.reply_to_message_id);
    if let Some(thread_id) = target.thread_id {
        body["message_thread_id"] = serde_json::json!(thread_id);
    }
    let resp = client.post(&url).json(&body).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let json: serde_json::Value = resp.json().await.ok()?;
    json.get("result")?.get("message_id")?.as_i64()
}

async fn telegram_edit_notification_raw(
    client: &reqwest::Client,
    api_base_url: &str,
    token: &str,
    chat_id: i64,
    message_id: i64,
    text: &str,
) -> bool {
    let url = format!("{}/bot{}/editMessageText", api_base_url, token);
    let body = serde_json::json!({
        "chat_id": chat_id,
        "message_id": message_id,
        "text": sanitize_telegram_html(text),
        "parse_mode": "HTML",
    });
    match client.post(&url).json(&body).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

async fn edit_download_notification_if_present(
    client: &reqwest::Client,
    api_base_url: &str,
    token: &str,
    chat_id: i64,
    notify_msg_id: Option<i64>,
    text: &str,
) {
    if let Some(message_id) = notify_msg_id {
        let _ =
            telegram_edit_notification_raw(client, api_base_url, token, chat_id, message_id, text)
                .await;
    }
}

/// File information returned by Telegram getFile API.
#[derive(Debug, Clone)]
struct FileInfo {
    file_id: String,
    file_size: u64,
    file_path: String,
    download_url: String,
    local_path: Option<String>,
}

fn looks_like_local_bot_api_path(file_path: &str) -> bool {
    let bytes = file_path.as_bytes();
    file_path.starts_with('/')
        || file_path.starts_with("\\\\")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/'))
}

fn telegram_entity_utf16_range_to_bytes(
    text: &str,
    offset_utf16: usize,
    length_utf16: usize,
) -> Option<std::ops::Range<usize>> {
    let end_utf16 = offset_utf16.checked_add(length_utf16)?;
    let mut utf16_index = 0usize;
    let mut start_byte = None;
    let mut end_byte = None;

    for (byte_index, ch) in text.char_indices() {
        if start_byte.is_none() && utf16_index == offset_utf16 {
            start_byte = Some(byte_index);
        }
        if end_byte.is_none() && utf16_index == end_utf16 {
            end_byte = Some(byte_index);
            break;
        }
        utf16_index += ch.len_utf16();
    }

    if start_byte.is_none() && utf16_index == offset_utf16 {
        start_byte = Some(text.len());
    }
    if end_byte.is_none() && utf16_index == end_utf16 {
        end_byte = Some(text.len());
    }

    match (start_byte, end_byte) {
        (Some(start), Some(end)) if start <= end => Some(start..end),
        _ => None,
    }
}

fn infer_download_extension(file_path: &str) -> String {
    Path::new(file_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .filter(|ext| !ext.is_empty())
        .map(|ext| match ext.to_ascii_lowercase().as_str() {
            "jpeg" => "jpg".to_string(),
            other => other.to_string(),
        })
        .unwrap_or_else(|| "dat".to_string())
}

/// Resolve a Telegram file_id to file information via the Bot API.
async fn telegram_get_file_info(
    token: &str,
    client: &reqwest::Client,
    file_id: &str,
    api_base_url: &str,
    use_local_api: bool,
    file_size: u64,
) -> Option<FileInfo> {
    // Official Bot API has 20MB limit on getFile
    const OFFICIAL_API_LIMIT: u64 = 20 * 1024 * 1024;

    if !use_local_api && file_size > OFFICIAL_API_LIMIT {
        warn!(
            "File {} ({} MB) exceeds official Bot API 20MB limit. \
             To download large files, deploy a Local Bot API Server \
             (https://github.com/tdlib/telegram-bot-api) and set \
             use_local_api=true in config.toml",
            file_id,
            file_size / 1024 / 1024
        );
        return None;
    }

    let url = format!("{api_base_url}/bot{token}/getFile");
    let attempts = if use_local_api { 3 } else { 1 };

    for attempt in 1..=attempts {
        let resp = match client
            .post(&url)
            .json(&serde_json::json!({"file_id": file_id}))
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(err) => {
                if attempt < attempts {
                    warn!(
                        "Telegram getFile transport error for {} on attempt {}/{}: {}",
                        file_id, attempt, attempts, err
                    );
                    tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
                    continue;
                }
                return None;
            }
        };

        let status = resp.status();
        let body: serde_json::Value = match resp.json().await {
            Ok(body) => body,
            Err(err) => {
                if attempt < attempts {
                    warn!(
                        "Telegram getFile parse error for {} on attempt {}/{}: {}",
                        file_id, attempt, attempts, err
                    );
                    tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
                    continue;
                }
                return None;
            }
        };

        if !status.is_success() || body["ok"].as_bool() != Some(true) {
            let description = body["description"].as_str().unwrap_or("unknown error");
            if use_local_api && attempt < attempts {
                warn!(
                    "Telegram getFile failed for {} on attempt {}/{} (status {}): {}",
                    file_id, attempt, attempts, status, description
                );
                tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
                continue;
            }
            return None;
        }

        let file_path = body["result"]["file_path"].as_str()?.to_string();
        let file_size = body["result"]["file_size"].as_u64().unwrap_or(0);

        // Local Bot API returns absolute file paths, official API returns relative paths.
        let local_path = if use_local_api && looks_like_local_bot_api_path(&file_path) {
            Some(file_path.clone())
        } else {
            None
        };
        let download_url = match &local_path {
            Some(path) => format!("file://{}", path),
            None => format!("{api_base_url}/file/bot{token}/{file_path}"),
        };

        return Some(FileInfo {
            file_id: file_id.to_string(),
            file_size,
            file_path,
            download_url,
            local_path,
        });
    }

    None
}

/// Download a file from Telegram to local disk with progress reporting.
async fn download_file(
    client: &reqwest::Client,
    file_info: &FileInfo,
    dest_dir: &Path,
    progress_callback: Option<&ProgressCallback>,
    chat_id: i64,
    message_id: Option<i64>,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    // Create download directory if it doesn't exist
    tokio::fs::create_dir_all(dest_dir).await?;

    // Generate unique filename
    let extension = infer_download_extension(&file_info.file_path);
    let filename = format!(
        "{}_{}.{}",
        file_info.file_id,
        chrono::Utc::now().timestamp_millis(),
        extension
    );
    let dest_path = dest_dir.join(&filename);

    // Check if this is a local file (Local Bot API)
    if let Some(local_path) = file_info.local_path.as_deref() {
        let source = Path::new(local_path);

        let actual_size = tokio::fs::metadata(source)
            .await
            .map(|m| m.len())
            .unwrap_or(file_info.file_size);
        let report_live_progress = actual_size > LOCAL_COPY_PROGRESS_THRESHOLD;

        if report_live_progress {
            emit_progress_update(
                progress_callback,
                &file_info.file_id,
                &filename,
                actual_size,
                0,
                chat_id,
                message_id,
            );
        }

        let mut source_file = File::open(source).await?;
        let mut dest_file = File::create(&dest_path).await?;
        let mut buffer = vec![0u8; LOCAL_COPY_CHUNK_SIZE];
        let mut copied = 0u64;
        let mut last_report = tokio::time::Instant::now();
        let mut last_bucket = 0u64;

        loop {
            let read = source_file.read(&mut buffer).await?;
            if read == 0 {
                break;
            }

            dest_file.write_all(&buffer[..read]).await?;
            copied += read as u64;

            if report_live_progress {
                let now = tokio::time::Instant::now();
                let bucket = progress_bucket(copied, actual_size);
                if now.duration_since(last_report) >= PROGRESS_REPORT_INTERVAL
                    || bucket > last_bucket
                    || copied >= actual_size
                {
                    emit_progress_update(
                        progress_callback,
                        &file_info.file_id,
                        &filename,
                        actual_size,
                        copied.min(actual_size),
                        chat_id,
                        message_id,
                    );
                    last_report = now;
                    last_bucket = bucket;
                }
            }

            tokio::task::yield_now().await;
        }

        dest_file.flush().await?;

        let total_bytes = actual_size.max(copied);

        emit_progress_update(
            progress_callback,
            &file_info.file_id,
            &filename,
            total_bytes,
            total_bytes,
            chat_id,
            message_id,
        );

        return Ok(dest_path);
    }

    // HTTP download (official Bot API)
    let response = client.get(&file_info.download_url).send().await?;
    if !response.status().is_success() {
        return Err(format!("HTTP error: {}", response.status()).into());
    }

    let mut file = File::create(&dest_path).await?;
    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;
    let total = file_info.file_size;
    let mut last_report = tokio::time::Instant::now();
    let mut last_bucket = 0u64;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;

        // Report progress (throttle to every 2 seconds or 5% bucket change)
        let now = tokio::time::Instant::now();
        let bucket = progress_bucket(downloaded, total);
        if now.duration_since(last_report) >= PROGRESS_REPORT_INTERVAL
            || bucket > last_bucket
            || downloaded == total
        {
            emit_progress_update(
                progress_callback,
                &file_info.file_id,
                &filename,
                total,
                downloaded,
                chat_id,
                message_id,
            );
            last_report = now;
            last_bucket = bucket;
        }
    }

    file.flush().await?;
    Ok(dest_path)
}

/// Resolve a Telegram file_id to a download URL via the Bot API (legacy function).
async fn telegram_get_file_url(
    token: &str,
    client: &reqwest::Client,
    file_id: &str,
    api_base_url: &str,
    use_local_api: bool,
    file_size: u64,
) -> Option<String> {
    telegram_get_file_info(
        token,
        client,
        file_id,
        api_base_url,
        use_local_api,
        file_size,
    )
    .await
    .map(|info| info.download_url)
}

#[allow(clippy::too_many_arguments)]
async fn parse_telegram_update(
    update: &serde_json::Value,
    allowed_users: &[String],
    token: &str,
    client: &reqwest::Client,
    api_base_url: &str,
    bot_username: Option<&str>,
    download_enabled: bool,
    download_dir: &Path,
    max_download_size: u64,
    progress_callback: Option<&ProgressCallback>,
    use_local_api: bool,
    notify_msg_id: Option<i64>,
) -> Option<ChannelMessage> {
    const OFFICIAL_API_LIMIT: u64 = 20 * 1024 * 1024;

    let update_id = update["update_id"].as_i64().unwrap_or(0);
    let message = match update
        .get("message")
        .or_else(|| update.get("edited_message"))
    {
        Some(m) => m,
        None => {
            debug!("Telegram: dropping update {update_id} — no message or edited_message field");
            return None;
        }
    };

    // Extract sender info: prefer `from` (user), fall back to `sender_chat` (channel/group)
    let (user_id, display_name) = if let Some(from) = message.get("from") {
        let uid = match from["id"].as_i64() {
            Some(id) => id,
            None => {
                debug!("Telegram: dropping update {update_id} — from.id is not an integer");
                return None;
            }
        };
        let first_name = from["first_name"].as_str().unwrap_or("Unknown");
        let last_name = from["last_name"].as_str().unwrap_or("");
        let name = if last_name.is_empty() {
            first_name.to_string()
        } else {
            format!("{first_name} {last_name}")
        };
        (uid, name)
    } else if let Some(sender_chat) = message.get("sender_chat") {
        // Messages sent on behalf of a channel or group have `sender_chat` instead of `from`.
        let uid = match sender_chat["id"].as_i64() {
            Some(id) => id,
            None => {
                debug!("Telegram: dropping update {update_id} — sender_chat.id is not an integer");
                return None;
            }
        };
        let title = sender_chat["title"].as_str().unwrap_or("Unknown Channel");
        (uid, title.to_string())
    } else {
        debug!("Telegram: dropping update {update_id} — no from or sender_chat field");
        return None;
    };

    // Security: check allowed_users (compare as strings for consistency)
    let user_id_str = user_id.to_string();
    if !allowed_users.is_empty() && !allowed_users.iter().any(|u| u == &user_id_str) {
        debug!("Telegram: ignoring message from unlisted user {user_id}");
        return None;
    }

    let chat_id = match message["chat"]["id"].as_i64() {
        Some(id) => id,
        None => {
            debug!("Telegram: dropping update {update_id} — chat.id is not an integer");
            return None;
        }
    };

    let chat_type = message["chat"]["type"].as_str().unwrap_or("private");
    let is_group = chat_type == "group" || chat_type == "supergroup";
    let message_id = message["message_id"].as_i64().unwrap_or(0);
    let timestamp = message["date"]
        .as_i64()
        .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
        .unwrap_or_else(chrono::Utc::now);
    let mut telegram_media_batch = None;

    // Determine content: text, photo, document, voice, or location
    let content = if let Some(text) = message["text"].as_str() {
        // Parse bot commands (Telegram sends entities for /commands)
        if let Some(entities) = message["entities"].as_array() {
            let is_bot_command = entities.iter().any(|e| {
                e["type"].as_str() == Some("bot_command") && e["offset"].as_i64() == Some(0)
            });
            if is_bot_command {
                let parts: Vec<&str> = text.splitn(2, ' ').collect();
                let cmd_name = parts[0].trim_start_matches('/');
                let cmd_name = cmd_name.split('@').next().unwrap_or(cmd_name);
                let args = if parts.len() > 1 {
                    parts[1].split_whitespace().map(String::from).collect()
                } else {
                    vec![]
                };
                ChannelContent::Command {
                    name: cmd_name.to_string(),
                    args,
                }
            } else {
                ChannelContent::Text(text.to_string())
            }
        } else {
            ChannelContent::Text(text.to_string())
        }
    } else if let Some(photos) = message["photo"].as_array() {
        // Photos come as array of sizes; pick the largest (last)
        let file_id = photos
            .last()
            .and_then(|p| p["file_id"].as_str())
            .unwrap_or("");
        let file_size = photos
            .last()
            .and_then(|p| p["file_size"].as_u64())
            .unwrap_or(0);
        let caption = message["caption"].as_str().map(String::from);
        match telegram_get_file_info(
            token,
            client,
            file_id,
            api_base_url,
            use_local_api,
            file_size,
        )
        .await
        {
            Some(file_info) => {
                if download_enabled && file_size <= max_download_size {
                    match download_file(
                        client,
                        &file_info,
                        download_dir,
                        progress_callback,
                        chat_id,
                        notify_msg_id,
                    )
                    .await
                    {
                        Ok(local_path) => ChannelContent::Image {
                            url: format!("file://{}", local_path.display()),
                            caption,
                        },
                        Err(e) => {
                            let err_text = e.to_string();
                            drop(e);
                            warn!("Failed to download photo {}: {}", file_id, err_text);
                            edit_download_notification_if_present(
                                client,
                                api_base_url,
                                token,
                                chat_id,
                                notify_msg_id,
                                "⚠️ 图片下载失败，已回退到远程地址",
                            )
                            .await;
                            ChannelContent::Image {
                                url: file_info.download_url,
                                caption,
                            }
                        }
                    }
                } else {
                    ChannelContent::Image {
                        url: file_info.download_url,
                        caption,
                    }
                }
            }
            None => {
                edit_download_notification_if_present(
                    client,
                    api_base_url,
                    token,
                    chat_id,
                    notify_msg_id,
                    "⚠️ 图片下载失败，未能获取文件地址",
                )
                .await;
                ChannelContent::Text(format!(
                    "[收到图片{}]",
                    caption
                        .as_deref()
                        .map(|c| format!(": {c}"))
                        .unwrap_or_default()
                ))
            }
        }
    } else if message.get("video").is_some() {
        let file_id = message["video"]["file_id"].as_str().unwrap_or("");
        let file_size = message["video"]["file_size"].as_u64().unwrap_or(0);
        let duration = message["video"]["duration"].as_u64().unwrap_or(0);
        let caption = message["caption"].as_str();
        let fallback_text = || {
            prepend_caption(
                caption,
                format!(
                    "[视频 ({} MB, {}s) - 文件过大，已跳过下载]",
                    file_size_mb(file_size),
                    duration
                ),
            )
        };

        if should_skip_get_file(file_size, use_local_api) {
            info!(
                "Video {} ({} MB) exceeds safe getFile limit ({} MB), skipping to prevent Local Bot API restarts",
                file_id,
                file_size_mb(file_size),
                get_file_safety_limit_mb()
            );
            telegram_media_batch = Some(build_single_message_batch(
                chat_id,
                message_id,
                caption,
                TelegramMediaItem {
                    kind: MediaItemKind::Video,
                    file_id: file_id.to_string(),
                    original_name: None,
                    file_size,
                    duration_seconds: Some(duration),
                    status: MediaItemStatus::SkippedSafeLimit,
                    local_path: None,
                    download_hint: Some(build_download_hint(
                        file_id,
                        api_base_url,
                        use_local_api,
                        None,
                        Some(format!(
                            "video exceeds {}MB safe getFile limit",
                            get_file_safety_limit_mb()
                        )),
                    )),
                },
            ));
            ChannelContent::Text(fallback_text())
        } else {
            match telegram_get_file_info(
                token,
                client,
                file_id,
                api_base_url,
                use_local_api,
                file_size,
            )
            .await
            {
                Some(file_info) => {
                    if download_enabled && file_size <= max_download_size {
                        match download_file(
                            client,
                            &file_info,
                            download_dir,
                            progress_callback,
                            chat_id,
                            notify_msg_id,
                        )
                        .await
                        {
                            Ok(local_path) => {
                                let body = format!(
                                    "[Video: file://{} ({}s, {} bytes)]",
                                    local_path.display(),
                                    duration,
                                    file_size
                                );
                                ChannelContent::Text(prepend_caption(caption, body))
                            }
                            Err(e) => {
                                let err_text = e.to_string();
                                drop(e);
                                warn!("Failed to download video {}: {}", file_id, err_text);
                                edit_download_notification_if_present(
                                    client,
                                    api_base_url,
                                    token,
                                    chat_id,
                                    notify_msg_id,
                                    "⚠️ 视频下载失败，已回退到远程地址",
                                )
                                .await;
                                let download_url = file_info.download_url;
                                telegram_media_batch = Some(build_single_message_batch(
                                    chat_id,
                                    message_id,
                                    caption,
                                    TelegramMediaItem {
                                        kind: MediaItemKind::Video,
                                        file_id: file_id.to_string(),
                                        original_name: None,
                                        file_size,
                                        duration_seconds: Some(duration),
                                        status: MediaItemStatus::DownloadFailed,
                                        local_path: None,
                                        download_hint: Some(build_download_hint(
                                            file_id,
                                            api_base_url,
                                            use_local_api,
                                            Some(download_url.clone()),
                                            Some(format!("bridge download failed: {err_text}")),
                                        )),
                                    },
                                ));
                                let body = format!(
                                    "[Video: {} ({}s, {} bytes)]",
                                    download_url, duration, file_size
                                );
                                ChannelContent::Text(prepend_caption(caption, body))
                            }
                        }
                    } else {
                        let download_url = file_info.download_url;
                        if file_size > max_download_size {
                            info!(
                                "Video {} ({} bytes) exceeds max download size ({} bytes), returning metadata only",
                                file_id, file_size, max_download_size
                            );
                        }
                        let reason = if file_size > max_download_size {
                            format!(
                                "video exceeds bridge max_download_size ({} bytes)",
                                max_download_size
                            )
                        } else {
                            "bridge download disabled".to_string()
                        };
                        telegram_media_batch = Some(build_single_message_batch(
                            chat_id,
                            message_id,
                            caption,
                            TelegramMediaItem {
                                kind: MediaItemKind::Video,
                                file_id: file_id.to_string(),
                                original_name: None,
                                file_size,
                                duration_seconds: Some(duration),
                                status: MediaItemStatus::NeedsProjectDownload,
                                local_path: None,
                                download_hint: Some(build_download_hint(
                                    file_id,
                                    api_base_url,
                                    use_local_api,
                                    Some(download_url.clone()),
                                    Some(reason),
                                )),
                            },
                        ));
                        let body = format!(
                            "[Video: {} ({}s, {} bytes)]",
                            download_url, duration, file_size
                        );
                        ChannelContent::Text(prepend_caption(caption, body))
                    }
                }
                None => {
                    edit_download_notification_if_present(
                        client,
                        api_base_url,
                        token,
                        chat_id,
                        notify_msg_id,
                        "⚠️ 视频下载失败，未能获取文件地址",
                    )
                    .await;
                    let reason = if !use_local_api && file_size > OFFICIAL_API_LIMIT {
                        "video exceeds official Bot API 20MB limit; configure Local Bot API"
                            .to_string()
                    } else {
                        "failed to resolve file path via getFile".to_string()
                    };
                    telegram_media_batch = Some(build_single_message_batch(
                        chat_id,
                        message_id,
                        caption,
                        TelegramMediaItem {
                            kind: MediaItemKind::Video,
                            file_id: file_id.to_string(),
                            original_name: None,
                            file_size,
                            duration_seconds: Some(duration),
                            status: MediaItemStatus::DownloadFailed,
                            local_path: None,
                            download_hint: Some(build_download_hint(
                                file_id,
                                api_base_url,
                                use_local_api,
                                None,
                                Some(reason),
                            )),
                        },
                    ));
                    ChannelContent::Text(prepend_caption(
                        caption,
                        format!(
                            "[收到视频，时长 {duration}s，大小 {} MB]",
                            file_size_mb(file_size)
                        ),
                    ))
                }
            }
        }
    } else if message.get("document").is_some() {
        let file_id = message["document"]["file_id"].as_str().unwrap_or("");
        let filename = message["document"]["file_name"]
            .as_str()
            .unwrap_or("document")
            .to_string();
        let file_size = message["document"]["file_size"].as_u64().unwrap_or(0);
        let mime_type = message["document"]["mime_type"].as_str().unwrap_or("");
        let caption = message["caption"].as_str();
        let is_video_document = is_video_document(&filename, mime_type);

        if should_skip_get_file(file_size, use_local_api) {
            info!(
                "Document {} ({} MB) exceeds safe getFile limit ({} MB), skipping to prevent Local Bot API restarts",
                file_id,
                file_size_mb(file_size),
                get_file_safety_limit_mb()
            );
            if is_video_document {
                telegram_media_batch = Some(build_single_message_batch(
                    chat_id,
                    message_id,
                    caption,
                    TelegramMediaItem {
                        kind: MediaItemKind::Video,
                        file_id: file_id.to_string(),
                        original_name: Some(filename.clone()),
                        file_size,
                        duration_seconds: None,
                        status: MediaItemStatus::SkippedSafeLimit,
                        local_path: None,
                        download_hint: Some(build_download_hint(
                            file_id,
                            api_base_url,
                            use_local_api,
                            None,
                            Some(format!(
                                "document exceeds {}MB safe getFile limit",
                                get_file_safety_limit_mb()
                            )),
                        )),
                    },
                ));
            }
            ChannelContent::Text(format!(
                "[文件 {} ({} MB) - 文件过大，已跳过下载]",
                filename,
                file_size_mb(file_size)
            ))
        } else {
            // Try to get file info first
            match telegram_get_file_info(
                token,
                client,
                file_id,
                api_base_url,
                use_local_api,
                file_size,
            )
            .await
            {
                Some(file_info) => {
                    // Check if download is enabled and file size is within limit
                    if download_enabled && file_size <= max_download_size {
                        // Attempt to download the file
                        match download_file(
                            client,
                            &file_info,
                            download_dir,
                            progress_callback,
                            chat_id,
                            notify_msg_id,
                        )
                        .await
                        {
                            Ok(local_path) => {
                                info!("Downloaded Telegram file to: {}", local_path.display());
                                ChannelContent::File {
                                    url: format!("file://{}", local_path.display()),
                                    filename,
                                }
                            }
                            Err(e) => {
                                let err_text = e.to_string();
                                drop(e);
                                warn!("Failed to download file {}: {}", file_id, err_text);
                                edit_download_notification_if_present(
                                    client,
                                    api_base_url,
                                    token,
                                    chat_id,
                                    notify_msg_id,
                                    "⚠️ 文件下载失败，已回退到远程地址",
                                )
                                .await;
                                if is_video_document {
                                    telegram_media_batch = Some(build_single_message_batch(
                                        chat_id,
                                        message_id,
                                        caption,
                                        TelegramMediaItem {
                                            kind: MediaItemKind::Video,
                                            file_id: file_id.to_string(),
                                            original_name: Some(filename.clone()),
                                            file_size,
                                            duration_seconds: None,
                                            status: MediaItemStatus::DownloadFailed,
                                            local_path: None,
                                            download_hint: Some(build_download_hint(
                                                file_id,
                                                api_base_url,
                                                use_local_api,
                                                Some(file_info.download_url.clone()),
                                                Some(format!("bridge download failed: {err_text}")),
                                            )),
                                        },
                                    ));
                                }
                                // Fallback to URL
                                ChannelContent::File {
                                    url: file_info.download_url,
                                    filename,
                                }
                            }
                        }
                    } else {
                        // Return URL only (download disabled or file too large)
                        if file_size > max_download_size {
                            info!(
                            "File {} ({} bytes) exceeds max download size ({} bytes), returning URL only",
                            file_id, file_size, max_download_size
                        );
                        }
                        if is_video_document {
                            let reason = if file_size > max_download_size {
                                format!(
                                    "document exceeds bridge max_download_size ({} bytes)",
                                    max_download_size
                                )
                            } else {
                                "bridge download disabled".to_string()
                            };
                            telegram_media_batch = Some(build_single_message_batch(
                                chat_id,
                                message_id,
                                caption,
                                TelegramMediaItem {
                                    kind: MediaItemKind::Video,
                                    file_id: file_id.to_string(),
                                    original_name: Some(filename.clone()),
                                    file_size,
                                    duration_seconds: None,
                                    status: MediaItemStatus::NeedsProjectDownload,
                                    local_path: None,
                                    download_hint: Some(build_download_hint(
                                        file_id,
                                        api_base_url,
                                        use_local_api,
                                        Some(file_info.download_url.clone()),
                                        Some(reason),
                                    )),
                                },
                            ));
                        }
                        ChannelContent::File {
                            url: file_info.download_url,
                            filename,
                        }
                    }
                }
                None => {
                    edit_download_notification_if_present(
                        client,
                        api_base_url,
                        token,
                        chat_id,
                        notify_msg_id,
                        "⚠️ 文件下载失败，未能获取文件地址",
                    )
                    .await;
                    if is_video_document {
                        let reason = if !use_local_api && file_size > OFFICIAL_API_LIMIT {
                            "video document exceeds official Bot API 20MB limit; configure Local Bot API"
                                .to_string()
                        } else {
                            "failed to resolve file path via getFile".to_string()
                        };
                        telegram_media_batch = Some(build_single_message_batch(
                            chat_id,
                            message_id,
                            caption,
                            TelegramMediaItem {
                                kind: MediaItemKind::Video,
                                file_id: file_id.to_string(),
                                original_name: Some(filename.clone()),
                                file_size,
                                duration_seconds: None,
                                status: MediaItemStatus::DownloadFailed,
                                local_path: None,
                                download_hint: Some(build_download_hint(
                                    file_id,
                                    api_base_url,
                                    use_local_api,
                                    None,
                                    Some(reason),
                                )),
                            },
                        ));
                    }
                    ChannelContent::Text(format!("[收到文档: {filename}]"))
                }
            }
        }
    } else if message.get("voice").is_some() {
        let file_id = message["voice"]["file_id"].as_str().unwrap_or("");
        let file_size = message["voice"]["file_size"].as_u64().unwrap_or(0);
        let duration = message["voice"]["duration"].as_u64().unwrap_or(0) as u32;
        match telegram_get_file_url(
            token,
            client,
            file_id,
            api_base_url,
            use_local_api,
            file_size,
        )
        .await
        {
            Some(url) => ChannelContent::Voice {
                url,
                duration_seconds: duration,
            },
            None => ChannelContent::Text(format!("[收到语音消息，时长 {duration}s]")),
        }
    } else if message.get("location").is_some() {
        let lat = message["location"]["latitude"].as_f64().unwrap_or(0.0);
        let lon = message["location"]["longitude"].as_f64().unwrap_or(0.0);
        ChannelContent::Location { lat, lon }
    } else {
        // Unsupported message type (stickers, polls, etc.)
        debug!("Telegram: dropping update {update_id} — unsupported message type (no text/photo/document/voice/location)");
        return None;
    };

    // Extract reply_to_message context — when the user replies to a previous message,
    // Telegram includes the original message in this field. Prepend the quoted context
    // so the agent knows what is being replied to.
    let content = if let Some(reply_msg) = message.get("reply_to_message") {
        let reply_text = reply_msg["text"]
            .as_str()
            .or_else(|| reply_msg["caption"].as_str());
        let reply_sender = reply_msg["from"]["first_name"].as_str();

        if let Some(quoted_text) = reply_text {
            let sender_label = reply_sender.unwrap_or("Unknown");
            let prefix = format!("[Replying to {sender_label}: {quoted_text}]\n\n");
            match content {
                ChannelContent::Text(t) => ChannelContent::Text(format!("{prefix}{t}")),
                ChannelContent::Command { name, args } => {
                    // Commands keep their structure — prepend context to first arg
                    // so the agent sees the reply context without breaking command parsing.
                    let mut new_args = vec![format!("{prefix}{}", args.join(" "))];
                    new_args.retain(|a| !a.trim().is_empty());
                    ChannelContent::Command {
                        name,
                        args: new_args,
                    }
                }
                other => other, // Image/File/Voice/Location — no text to prepend
            }
        } else {
            content
        }
    } else {
        content
    };

    // Extract forum topic thread_id (Telegram sends this as `message_thread_id`
    // for messages inside forum topics / reply threads).
    let thread_id = message["message_thread_id"]
        .as_i64()
        .map(|tid| tid.to_string());

    // Detect @mention of the bot in entities / caption_entities for MentionOnly group policy.
    let mut metadata = HashMap::new();

    // Store reply_to_message_id in metadata for downstream consumers.
    if let Some(reply_msg) = message.get("reply_to_message") {
        if let Some(reply_id) = reply_msg["message_id"].as_i64() {
            metadata.insert(
                "reply_to_message_id".to_string(),
                serde_json::json!(reply_id),
            );
        }
        let reply_is_bot = reply_msg["from"]["is_bot"].as_bool().unwrap_or(false)
            || bot_username
                .and_then(|name| reply_msg["from"]["username"].as_str().map(|u| (u, name)))
                .map(|(username, name)| username.eq_ignore_ascii_case(name))
                .unwrap_or(false);
        if reply_is_bot {
            metadata.insert("reply_to_bot_message".to_string(), serde_json::json!(true));
        }
    }
    if is_group {
        if let Some(bot_uname) = bot_username {
            let was_mentioned = check_mention_entities(message, bot_uname);
            if was_mentioned {
                metadata.insert("was_mentioned".to_string(), serde_json::json!(true));
            }
        }
    }
    if let Some(batch) = telegram_media_batch {
        if let Ok(value) = serde_json::to_value(batch) {
            metadata.insert("telegram_media_batch".to_string(), value);
        }
    }

    Some(ChannelMessage {
        channel: ChannelType::Telegram,
        platform_message_id: message_id.to_string(),
        sender: ChannelUser {
            platform_id: chat_id.to_string(),
            display_name,
            openfang_user: None,
            metadata: None,
        },
        content,
        target_agent: None,
        timestamp,
        is_group,
        thread_id,
        metadata,
    })
}

/// Check whether the bot was @mentioned in a Telegram message.
///
/// **IMPORTANT: Telegram Group Privacy Mode**
///
/// For this function to work in groups, you MUST disable "Group Privacy" in BotFather:
/// 1. Open @BotFather in Telegram
/// 2. Send /mybots
/// 3. Select your bot
/// 4. Bot Settings → Group Privacy → Turn off
///
/// **Why this is required:**
/// - Group Privacy ON: Telegram only sends /commands to the bot, @mentions are NOT delivered
/// - Group Privacy OFF: Telegram sends all messages, then OpenFang filters by @mention
///
/// **Two-layer filtering:**
/// 1. Telegram layer (Group Privacy): Controls what messages reach the bot
/// 2. OpenFang layer (group_policy): Controls what messages the bot responds to
///
/// With Group Privacy OFF + group_policy="mention_only", the bot will:
/// - Receive all group messages (Telegram layer)
/// - Only respond to @mentions (OpenFang layer)
///
/// Inspects both `entities` (for text messages) and `caption_entities` (for media
/// with captions) for entity type `"mention"` or `"text_mention"` whose text matches `@bot_username`.
fn check_mention_entities(message: &serde_json::Value, bot_username: &str) -> bool {
    let bot_mention = format!("@{}", bot_username.to_lowercase());

    // Check both entities (text messages) and caption_entities (photo/document captions)
    for entities_key in &["entities", "caption_entities"] {
        if let Some(entities) = message[entities_key].as_array() {
            // Get the text that the entities refer to
            let text = if *entities_key == "entities" {
                message["text"].as_str().unwrap_or("")
            } else {
                message["caption"].as_str().unwrap_or("")
            };

            for entity in entities {
                let entity_type = entity["type"].as_str();

                // Check for regular mentions (@username)
                if entity_type == Some("mention") {
                    let offset = entity["offset"].as_i64().unwrap_or(0) as usize;
                    let length = entity["length"].as_i64().unwrap_or(0) as usize;
                    if let Some(range) = telegram_entity_utf16_range_to_bytes(text, offset, length)
                    {
                        let mention_text = &text[range];
                        if mention_text.to_lowercase() == bot_mention {
                            return true;
                        }
                    }
                }

                // Check for text_mention (when user @mentions via UI picker)
                if entity_type == Some("text_mention") {
                    if let Some(user) = entity.get("user") {
                        if let Some(username) = user["username"].as_str() {
                            if username.to_lowercase() == bot_username.to_lowercase() {
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }

    // Fallback: check if text contains @botusername (case-insensitive)
    let text = message["text"]
        .as_str()
        .or_else(|| message["caption"].as_str())
        .unwrap_or("");

    if text.to_lowercase().contains(&bot_mention) {
        return true;
    }

    false
}

/// Calculate exponential backoff capped at MAX_BACKOFF.
pub fn calculate_backoff(current: Duration) -> Duration {
    (current * 2).min(MAX_BACKOFF)
}

/// Sanitize text for Telegram HTML parse mode.
///
/// Escapes angle brackets that are NOT part of Telegram-allowed HTML tags.
/// Allowed tags: b, i, u, s, tg-spoiler, a, code, pre, blockquote.
/// Everything else (e.g. `<name>`, `<thinking>`) gets escaped to `&lt;...&gt;`.
fn sanitize_telegram_html(text: &str) -> String {
    const ALLOWED: &[&str] = &[
        "b",
        "i",
        "u",
        "s",
        "em",
        "strong",
        "a",
        "code",
        "pre",
        "blockquote",
        "tg-spoiler",
        "tg-emoji",
    ];

    let mut result = String::with_capacity(text.len());
    let mut chars = text.char_indices().peekable();

    while let Some(&(i, ch)) = chars.peek() {
        if ch == '<' {
            // Try to parse an HTML tag
            if let Some(end_offset) = text[i..].find('>') {
                let tag_end = i + end_offset;
                let tag_content = &text[i + 1..tag_end]; // content between < and >
                let tag_name = tag_content
                    .trim_start_matches('/')
                    .split(|c: char| c.is_whitespace() || c == '/' || c == '>')
                    .next()
                    .unwrap_or("")
                    .to_lowercase();

                if !tag_name.is_empty() && ALLOWED.contains(&tag_name.as_str()) {
                    // Allowed tag — keep as-is
                    result.push_str(&text[i..tag_end + 1]);
                } else {
                    // Unknown tag — escape both brackets
                    result.push_str("&lt;");
                    result.push_str(tag_content);
                    result.push_str("&gt;");
                }
                // Advance past the whole tag
                while let Some(&(j, _)) = chars.peek() {
                    chars.next();
                    if j >= tag_end {
                        break;
                    }
                }
            } else {
                // No closing > — escape the lone <
                result.push_str("&lt;");
                chars.next();
            }
        } else {
            result.push(ch);
            chars.next();
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, OnceLock,
    };

    fn test_client() -> reqwest::Client {
        reqwest::Client::new()
    }

    async fn local_api_test_guard() -> tokio::sync::OwnedSemaphorePermit {
        static LOCAL_API_TEST_SEMAPHORE: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
        LOCAL_API_TEST_SEMAPHORE
            .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(1)))
            .clone()
            .acquire_owned()
            .await
            .expect("local API test semaphore closed")
    }

    // Helper function for tests - uses default download settings (disabled)
    async fn parse_telegram_update_test(
        update: &serde_json::Value,
        allowed_users: &[String],
        token: &str,
        client: &reqwest::Client,
        api_base_url: &str,
        bot_username: Option<&str>,
    ) -> Option<ChannelMessage> {
        parse_telegram_update_test_with_options(
            update,
            allowed_users,
            token,
            client,
            api_base_url,
            bot_username,
            false,
        )
        .await
    }

    async fn parse_telegram_update_test_with_options(
        update: &serde_json::Value,
        allowed_users: &[String],
        token: &str,
        client: &reqwest::Client,
        api_base_url: &str,
        bot_username: Option<&str>,
        use_local_api: bool,
    ) -> Option<ChannelMessage> {
        let temp_dir = std::env::temp_dir();
        parse_telegram_update(
            update,
            allowed_users,
            token,
            client,
            api_base_url,
            bot_username,
            false, // download_enabled
            &temp_dir,
            2 * 1024 * 1024 * 1024, // 2GB max
            None,                   // no progress callback
            use_local_api,
            None, // no notify_msg_id in tests
        )
        .await
    }

    #[tokio::test]
    async fn test_parse_telegram_update() {
        let update = serde_json::json!({
            "update_id": 123456,
            "message": {
                "message_id": 42,
                "from": {
                    "id": 111222333,
                    "first_name": "Alice",
                    "last_name": "Smith"
                },
                "chat": {
                    "id": 111222333,
                    "type": "private"
                },
                "date": 1700000000,
                "text": "Hello, agent!"
            }
        });

        let client = test_client();
        let msg =
            parse_telegram_update_test(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
                .await
                .unwrap();
        assert_eq!(msg.channel, ChannelType::Telegram);
        assert_eq!(msg.sender.display_name, "Alice Smith");
        assert_eq!(msg.sender.platform_id, "111222333");
        assert!(matches!(msg.content, ChannelContent::Text(ref t) if t == "Hello, agent!"));
    }

    #[tokio::test]
    async fn test_parse_telegram_command() {
        let update = serde_json::json!({
            "update_id": 123457,
            "message": {
                "message_id": 43,
                "from": {
                    "id": 111222333,
                    "first_name": "Alice"
                },
                "chat": {
                    "id": 111222333,
                    "type": "private"
                },
                "date": 1700000001,
                "text": "/agent hello-world",
                "entities": [{
                    "type": "bot_command",
                    "offset": 0,
                    "length": 6
                }]
            }
        });

        let client = test_client();
        let msg =
            parse_telegram_update_test(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
                .await
                .unwrap();
        match &msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "agent");
                assert_eq!(args, &["hello-world"]);
            }
            other => panic!("Expected Command, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_allowed_users_filter() {
        let update = serde_json::json!({
            "update_id": 123458,
            "message": {
                "message_id": 44,
                "from": {
                    "id": 999,
                    "first_name": "Bob"
                },
                "chat": {
                    "id": 999,
                    "type": "private"
                },
                "date": 1700000002,
                "text": "blocked"
            }
        });

        let client = test_client();

        // Empty allowed_users = allow all
        let msg =
            parse_telegram_update_test(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
                .await;
        assert!(msg.is_some());

        // Non-matching allowed_users = filter out
        let blocked: Vec<String> = vec!["111".to_string(), "222".to_string()];
        let msg = parse_telegram_update_test(
            &update,
            &blocked,
            "fake:token",
            &client,
            DEFAULT_API_URL,
            None,
        )
        .await;
        assert!(msg.is_none());

        // Matching allowed_users = allow
        let allowed: Vec<String> = vec!["999".to_string()];
        let msg = parse_telegram_update_test(
            &update,
            &allowed,
            "fake:token",
            &client,
            DEFAULT_API_URL,
            None,
        )
        .await;
        assert!(msg.is_some());
    }

    #[tokio::test]
    async fn test_parse_telegram_edited_message() {
        let update = serde_json::json!({
            "update_id": 123459,
            "edited_message": {
                "message_id": 42,
                "from": {
                    "id": 111222333,
                    "first_name": "Alice",
                    "last_name": "Smith"
                },
                "chat": {
                    "id": 111222333,
                    "type": "private"
                },
                "date": 1700000000,
                "edit_date": 1700000060,
                "text": "Edited message!"
            }
        });

        let client = test_client();
        let msg =
            parse_telegram_update_test(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
                .await
                .unwrap();
        assert_eq!(msg.channel, ChannelType::Telegram);
        assert_eq!(msg.sender.display_name, "Alice Smith");
        assert!(matches!(msg.content, ChannelContent::Text(ref t) if t == "Edited message!"));
    }

    #[test]
    fn test_backoff_calculation() {
        let b1 = calculate_backoff(Duration::from_secs(1));
        assert_eq!(b1, Duration::from_secs(2));

        let b2 = calculate_backoff(Duration::from_secs(2));
        assert_eq!(b2, Duration::from_secs(4));

        let b3 = calculate_backoff(Duration::from_secs(32));
        assert_eq!(b3, Duration::from_secs(60)); // capped

        let b4 = calculate_backoff(Duration::from_secs(60));
        assert_eq!(b4, Duration::from_secs(60)); // stays at cap
    }

    #[tokio::test]
    async fn test_parse_command_with_botname() {
        let update = serde_json::json!({
            "update_id": 100,
            "message": {
                "message_id": 1,
                "from": { "id": 123, "first_name": "X" },
                "chat": { "id": 123, "type": "private" },
                "date": 1700000000,
                "text": "/agents@myopenfangbot",
                "entities": [{ "type": "bot_command", "offset": 0, "length": 17 }]
            }
        });

        let client = test_client();
        let msg =
            parse_telegram_update_test(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
                .await
                .unwrap();
        match &msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "agents");
                assert!(args.is_empty());
            }
            other => panic!("Expected Command, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_telegram_location() {
        let update = serde_json::json!({
            "update_id": 200,
            "message": {
                "message_id": 50,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": 123, "type": "private" },
                "date": 1700000000,
                "location": { "latitude": 51.5074, "longitude": -0.1278 }
            }
        });

        let client = test_client();
        let msg =
            parse_telegram_update_test(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
                .await
                .unwrap();
        assert!(matches!(msg.content, ChannelContent::Location { .. }));
    }

    #[tokio::test]
    async fn test_parse_telegram_photo_fallback() {
        // When getFile fails (fake token), photo messages should fall back to
        // a text description rather than being silently dropped.
        let update = serde_json::json!({
            "update_id": 300,
            "message": {
                "message_id": 60,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": 123, "type": "private" },
                "date": 1700000000,
                "photo": [
                    { "file_id": "small_id", "file_unique_id": "a", "width": 90, "height": 90, "file_size": 1234 },
                    { "file_id": "large_id", "file_unique_id": "b", "width": 800, "height": 600, "file_size": 45678 }
                ],
                "caption": "Check this out"
            }
        });

        let client = test_client();
        let msg =
            parse_telegram_update_test(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
                .await
                .unwrap();
        // With a fake token, getFile will fail, so we get a text fallback
        match &msg.content {
            ChannelContent::Text(t) => {
                assert!(t.contains("收到图片"));
                assert!(t.contains("Check this out"));
            }
            ChannelContent::Image { caption, .. } => {
                // If somehow the HTTP call succeeded (unlikely with fake token),
                // verify caption was extracted
                assert_eq!(caption.as_deref(), Some("Check this out"));
            }
            other => panic!("Expected Text or Image fallback for photo, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_telegram_video_skips_oversized_local_api_lookup() {
        let _guard = local_api_test_guard().await;
        let oversized = LOCAL_API_SAFE_GETFILE_LIMIT + 1;
        let update = serde_json::json!({
            "update_id": 3001,
            "message": {
                "message_id": 6001,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": 123, "type": "private" },
                "date": 1700000000,
                "caption": "Huge upload",
                "video": {
                    "file_id": "huge_video_id",
                    "file_unique_id": "hv1",
                    "duration": 42,
                    "file_size": oversized
                }
            }
        });

        let client = test_client();
        let msg = parse_telegram_update_test_with_options(
            &update,
            &[],
            "fake:token",
            &client,
            DEFAULT_API_URL,
            None,
            true,
        )
        .await
        .unwrap();

        match &msg.content {
            ChannelContent::Text(text) => {
                assert!(text.contains("Huge upload"));
                assert!(text.contains("文件过大，已跳过下载"));
                assert!(text.contains(&format!("{} MB", file_size_mb(oversized))));
            }
            other => panic!("Expected Text fallback for oversized video, got {other:?}"),
        }
        let batch_value = msg
            .metadata
            .get("telegram_media_batch")
            .expect("single oversized video should expose batch metadata");
        let batch: crate::telegram_media_batch::TelegramMediaBatch =
            serde_json::from_value(batch_value.clone()).unwrap();
        assert_eq!(batch.batch_key, "single_123_6001");
        assert_eq!(batch.media_group_id, "");
        assert_eq!(batch.items.len(), 1);
        assert_eq!(
            batch.items[0].kind,
            crate::telegram_media_batch::MediaItemKind::Video
        );
        assert_eq!(
            batch.items[0].status,
            crate::telegram_media_batch::MediaItemStatus::SkippedSafeLimit
        );
        assert_eq!(batch.items[0].file_id, "huge_video_id");
    }

    #[tokio::test]
    async fn test_parse_telegram_document_fallback() {
        let update = serde_json::json!({
            "update_id": 301,
            "message": {
                "message_id": 61,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": 123, "type": "private" },
                "date": 1700000000,
                "document": {
                    "file_id": "doc_id",
                    "file_unique_id": "c",
                    "file_name": "report.pdf",
                    "file_size": 102400
                }
            }
        });

        let client = test_client();
        let msg =
            parse_telegram_update_test(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
                .await
                .unwrap();
        match &msg.content {
            ChannelContent::Text(t) => {
                assert!(t.contains("收到文档"));
                assert!(t.contains("report.pdf"));
            }
            ChannelContent::File { filename, .. } => {
                assert_eq!(filename, "report.pdf");
            }
            other => panic!("Expected Text or File for document, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_telegram_document_skips_oversized_local_api_lookup() {
        let _guard = local_api_test_guard().await;
        let oversized = LOCAL_API_SAFE_GETFILE_LIMIT + 1;
        let update = serde_json::json!({
            "update_id": 3011,
            "message": {
                "message_id": 6011,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": 123, "type": "private" },
                "date": 1700000000,
                "document": {
                    "file_id": "huge_doc_id",
                    "file_unique_id": "hd1",
                    "file_name": "archive.zip",
                    "file_size": oversized
                }
            }
        });

        let client = test_client();
        let msg = parse_telegram_update_test_with_options(
            &update,
            &[],
            "fake:token",
            &client,
            DEFAULT_API_URL,
            None,
            true,
        )
        .await
        .unwrap();

        match &msg.content {
            ChannelContent::Text(text) => {
                assert!(text.contains("archive.zip"));
                assert!(text.contains("文件过大，已跳过下载"));
                assert!(text.contains(&format!("{} MB", file_size_mb(oversized))));
            }
            other => panic!("Expected Text fallback for oversized document, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_telegram_single_video_with_download_url_exposes_batch_metadata() {
        let _guard = local_api_test_guard().await;
        let (api_base_url, server_handle) = start_telegram_test_server().await;
        let update = serde_json::json!({
            "update_id": 3012,
            "message": {
                "message_id": 6012,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": 123, "type": "private" },
                "date": 1700000000,
                "caption": "Direct download",
                "video": {
                    "file_id": "video_direct",
                    "file_unique_id": "vd1",
                    "duration": 11,
                    "file_size": 4096
                }
            }
        });

        let client = test_client();
        let msg =
            parse_telegram_update_test(&update, &[], "fake:token", &client, &api_base_url, None)
                .await
                .unwrap();

        server_handle.abort();

        match &msg.content {
            ChannelContent::Text(text) => {
                assert!(text.contains("Direct download"));
                assert!(text.contains("/file/botfake:token/files/video_direct.mp4"));
            }
            other => panic!("Expected Text fallback with URL, got {other:?}"),
        }
        let batch_value = msg
            .metadata
            .get("telegram_media_batch")
            .expect("single video should expose batch metadata");
        let batch: crate::telegram_media_batch::TelegramMediaBatch =
            serde_json::from_value(batch_value.clone()).unwrap();
        assert_eq!(batch.items.len(), 1);
        assert_eq!(
            batch.items[0].status,
            crate::telegram_media_batch::MediaItemStatus::NeedsProjectDownload
        );
        assert_eq!(batch.items[0].file_id, "video_direct");
        assert_eq!(
            batch.items[0]
                .download_hint
                .as_ref()
                .and_then(|hint| hint.download_url.as_deref()),
            Some(format!("{}/file/botfake:token/files/video_direct.mp4", api_base_url).as_str())
        );
    }

    #[tokio::test]
    async fn test_parse_telegram_single_video_download_success_does_not_add_batch_metadata() {
        let _guard = local_api_test_guard().await;
        let (api_base_url, server_handle) = start_telegram_test_server().await;
        let temp_dir =
            std::env::temp_dir().join(format!("openfang-telegram-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let update = serde_json::json!({
            "update_id": 3014,
            "message": {
                "message_id": 6014,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": 123, "type": "private" },
                "date": 1700000000,
                "caption": "Downloaded",
                "video": {
                    "file_id": "video_saved",
                    "file_unique_id": "vs1",
                    "duration": 9,
                    "file_size": 4096
                }
            }
        });

        let client = test_client();
        let msg = parse_telegram_update(
            &update,
            &[],
            "fake:token",
            &client,
            &api_base_url,
            None,
            true,
            &temp_dir,
            2 * 1024 * 1024 * 1024,
            None,
            false,
            None,
        )
        .await
        .unwrap();

        server_handle.abort();

        match &msg.content {
            ChannelContent::Text(text) => {
                assert!(text.contains("file://"));
                assert!(text.contains("Downloaded"));
            }
            other => panic!("Expected downloaded video text fallback, got {other:?}"),
        }
        assert!(!msg.metadata.contains_key("telegram_media_batch"));
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_parse_telegram_video_document_skips_oversized_local_api_lookup() {
        let _guard = local_api_test_guard().await;
        let oversized = LOCAL_API_SAFE_GETFILE_LIMIT + 1;
        let update = serde_json::json!({
            "update_id": 3013,
            "message": {
                "message_id": 6013,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": 123, "type": "private" },
                "date": 1700000000,
                "caption": "Video as document",
                "document": {
                    "file_id": "huge_doc_video",
                    "file_unique_id": "hdv1",
                    "file_name": "clip.mp4",
                    "mime_type": "video/mp4",
                    "file_size": oversized
                }
            }
        });

        let client = test_client();
        let msg = parse_telegram_update_test_with_options(
            &update,
            &[],
            "fake:token",
            &client,
            DEFAULT_API_URL,
            None,
            true,
        )
        .await
        .unwrap();

        match &msg.content {
            ChannelContent::Text(text) => {
                assert!(text.contains("clip.mp4"));
                assert!(text.contains("文件过大，已跳过下载"));
                assert!(text.contains(&format!("{} MB", file_size_mb(oversized))));
            }
            other => panic!("Expected Text fallback for oversized video document, got {other:?}"),
        }
        let batch_value = msg
            .metadata
            .get("telegram_media_batch")
            .expect("oversized video document should expose batch metadata");
        let batch: crate::telegram_media_batch::TelegramMediaBatch =
            serde_json::from_value(batch_value.clone()).unwrap();
        assert_eq!(batch.items.len(), 1);
        assert_eq!(batch.batch_key, "single_123_6013");
        assert_eq!(batch.items[0].original_name.as_deref(), Some("clip.mp4"));
        assert_eq!(
            batch.items[0].kind,
            crate::telegram_media_batch::MediaItemKind::Video
        );
        assert_eq!(
            batch.items[0].status,
            crate::telegram_media_batch::MediaItemStatus::SkippedSafeLimit
        );
    }

    #[tokio::test]
    async fn test_parse_telegram_video_document_download_success_does_not_add_batch_metadata() {
        let _guard = local_api_test_guard().await;
        let (api_base_url, server_handle) = start_telegram_test_server().await;
        let temp_dir =
            std::env::temp_dir().join(format!("openfang-telegram-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let update = serde_json::json!({
            "update_id": 3015,
            "message": {
                "message_id": 6015,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": 123, "type": "private" },
                "date": 1700000000,
                "caption": "Saved doc video",
                "document": {
                    "file_id": "doc_video_saved",
                    "file_unique_id": "dvs1",
                    "file_name": "clip.mp4",
                    "mime_type": "video/mp4",
                    "file_size": 4096
                }
            }
        });

        let client = test_client();
        let msg = parse_telegram_update(
            &update,
            &[],
            "fake:token",
            &client,
            &api_base_url,
            None,
            true,
            &temp_dir,
            2 * 1024 * 1024 * 1024,
            None,
            false,
            None,
        )
        .await
        .unwrap();

        server_handle.abort();

        match &msg.content {
            ChannelContent::File { url, filename } => {
                assert!(url.starts_with("file://"));
                assert_eq!(filename, "clip.mp4");
            }
            other => panic!("Expected downloaded file content, got {other:?}"),
        }
        assert!(!msg.metadata.contains_key("telegram_media_batch"));
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_parse_telegram_voice_fallback() {
        let update = serde_json::json!({
            "update_id": 302,
            "message": {
                "message_id": 62,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": 123, "type": "private" },
                "date": 1700000000,
                "voice": {
                    "file_id": "voice_id",
                    "file_unique_id": "d",
                    "duration": 15
                }
            }
        });

        let client = test_client();
        let msg =
            parse_telegram_update_test(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
                .await
                .unwrap();
        match &msg.content {
            ChannelContent::Text(t) => {
                assert!(t.contains("收到语音消息"));
                assert!(t.contains("15s"));
            }
            ChannelContent::Voice {
                duration_seconds, ..
            } => {
                assert_eq!(*duration_seconds, 15);
            }
            other => panic!("Expected Text or Voice for voice message, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_telegram_forum_topic_thread_id() {
        // Messages inside a Telegram forum topic include `message_thread_id`.
        let update = serde_json::json!({
            "update_id": 400,
            "message": {
                "message_id": 70,
                "message_thread_id": 42,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": -1001234567890_i64, "type": "supergroup" },
                "date": 1700000000,
                "text": "Hello from a forum topic"
            }
        });

        let client = test_client();
        let msg =
            parse_telegram_update_test(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
                .await
                .unwrap();
        assert_eq!(msg.thread_id, Some("42".to_string()));
        assert!(msg.is_group);
    }

    #[tokio::test]
    async fn test_parse_telegram_no_thread_id_in_private_chat() {
        // Private chats should have thread_id = None.
        let update = serde_json::json!({
            "update_id": 401,
            "message": {
                "message_id": 71,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": 123, "type": "private" },
                "date": 1700000000,
                "text": "Hello from DM"
            }
        });

        let client = test_client();
        let msg =
            parse_telegram_update_test(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
                .await
                .unwrap();
        assert_eq!(msg.thread_id, None);
        assert!(!msg.is_group);
    }

    #[tokio::test]
    async fn test_parse_telegram_edited_message_in_forum() {
        // Edited messages in forum topics should also preserve thread_id.
        let update = serde_json::json!({
            "update_id": 402,
            "edited_message": {
                "message_id": 72,
                "message_thread_id": 99,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": -1001234567890_i64, "type": "supergroup" },
                "date": 1700000000,
                "edit_date": 1700000060,
                "text": "Edited in forum"
            }
        });

        let client = test_client();
        let msg =
            parse_telegram_update_test(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
                .await
                .unwrap();
        assert_eq!(msg.thread_id, Some("99".to_string()));
    }

    #[tokio::test]
    async fn test_parse_sender_chat_fallback() {
        // Messages sent on behalf of a channel have `sender_chat` instead of `from`.
        let update = serde_json::json!({
            "update_id": 500,
            "message": {
                "message_id": 80,
                "sender_chat": {
                    "id": -1001999888777_i64,
                    "title": "My Channel",
                    "type": "channel"
                },
                "chat": { "id": -1001234567890_i64, "type": "supergroup" },
                "date": 1700000000,
                "text": "Forwarded from channel"
            }
        });

        let client = test_client();
        let msg =
            parse_telegram_update_test(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
                .await
                .unwrap();
        assert_eq!(msg.sender.display_name, "My Channel");
        assert_eq!(msg.sender.platform_id, "-1001234567890");
        assert!(
            matches!(msg.content, ChannelContent::Text(ref t) if t == "Forwarded from channel")
        );
    }

    #[tokio::test]
    async fn test_parse_no_from_no_sender_chat_drops() {
        // Updates with neither `from` nor `sender_chat` should be dropped with debug logging.
        let update = serde_json::json!({
            "update_id": 501,
            "message": {
                "message_id": 81,
                "chat": { "id": 123, "type": "private" },
                "date": 1700000000,
                "text": "orphan"
            }
        });

        let client = test_client();
        let msg =
            parse_telegram_update_test(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
                .await;
        assert!(msg.is_none());
    }

    #[tokio::test]
    async fn test_was_mentioned_in_group() {
        // Bot @mentioned in a group message should set metadata["was_mentioned"].
        let update = serde_json::json!({
            "update_id": 600,
            "message": {
                "message_id": 90,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": -1001234567890_i64, "type": "supergroup" },
                "date": 1700000000,
                "text": "Hey @testbot what do you think?",
                "entities": [{
                    "type": "mention",
                    "offset": 4,
                    "length": 8
                }]
            }
        });

        let client = test_client();
        let msg = parse_telegram_update_test(
            &update,
            &[],
            "fake:token",
            &client,
            DEFAULT_API_URL,
            Some("testbot"),
        )
        .await
        .unwrap();
        assert!(msg.is_group);
        assert_eq!(
            msg.metadata.get("was_mentioned").and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[tokio::test]
    async fn test_not_mentioned_in_group() {
        // Group message without a mention should NOT have was_mentioned.
        let update = serde_json::json!({
            "update_id": 601,
            "message": {
                "message_id": 91,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": -1001234567890_i64, "type": "supergroup" },
                "date": 1700000000,
                "text": "Just chatting"
            }
        });

        let client = test_client();
        let msg = parse_telegram_update_test(
            &update,
            &[],
            "fake:token",
            &client,
            DEFAULT_API_URL,
            Some("testbot"),
        )
        .await
        .unwrap();
        assert!(msg.is_group);
        assert!(!msg.metadata.contains_key("was_mentioned"));
    }

    #[tokio::test]
    async fn test_mentioned_different_bot_not_set() {
        // @mention of a different bot should NOT set was_mentioned.
        let update = serde_json::json!({
            "update_id": 602,
            "message": {
                "message_id": 92,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": -1001234567890_i64, "type": "supergroup" },
                "date": 1700000000,
                "text": "Hey @otherbot what do you think?",
                "entities": [{
                    "type": "mention",
                    "offset": 4,
                    "length": 9
                }]
            }
        });

        let client = test_client();
        let msg = parse_telegram_update_test(
            &update,
            &[],
            "fake:token",
            &client,
            DEFAULT_API_URL,
            Some("testbot"),
        )
        .await
        .unwrap();
        assert!(msg.is_group);
        assert!(!msg.metadata.contains_key("was_mentioned"));
    }

    #[tokio::test]
    async fn test_mention_in_caption_entities() {
        // Bot mentioned in a photo caption should set was_mentioned.
        let update = serde_json::json!({
            "update_id": 603,
            "message": {
                "message_id": 93,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": -1001234567890_i64, "type": "supergroup" },
                "date": 1700000000,
                "photo": [
                    { "file_id": "photo_id", "file_unique_id": "x", "width": 800, "height": 600 }
                ],
                "caption": "Look @testbot",
                "caption_entities": [{
                    "type": "mention",
                    "offset": 5,
                    "length": 8
                }]
            }
        });

        let client = test_client();
        let msg = parse_telegram_update_test(
            &update,
            &[],
            "fake:token",
            &client,
            DEFAULT_API_URL,
            Some("testbot"),
        )
        .await
        .unwrap();
        assert!(msg.is_group);
        assert_eq!(
            msg.metadata.get("was_mentioned").and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[tokio::test]
    async fn test_mention_case_insensitive() {
        // Mention detection should be case-insensitive.
        let update = serde_json::json!({
            "update_id": 604,
            "message": {
                "message_id": 94,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": -1001234567890_i64, "type": "supergroup" },
                "date": 1700000000,
                "text": "Hey @TestBot help",
                "entities": [{
                    "type": "mention",
                    "offset": 4,
                    "length": 8
                }]
            }
        });

        let client = test_client();
        let msg = parse_telegram_update_test(
            &update,
            &[],
            "fake:token",
            &client,
            DEFAULT_API_URL,
            Some("testbot"),
        )
        .await
        .unwrap();
        assert_eq!(
            msg.metadata.get("was_mentioned").and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[tokio::test]
    async fn test_private_chat_no_mention_check() {
        // Private chats should NOT populate was_mentioned even with entities.
        let update = serde_json::json!({
            "update_id": 605,
            "message": {
                "message_id": 95,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": 123, "type": "private" },
                "date": 1700000000,
                "text": "Hey @testbot",
                "entities": [{
                    "type": "mention",
                    "offset": 4,
                    "length": 8
                }]
            }
        });

        let client = test_client();
        let msg = parse_telegram_update_test(
            &update,
            &[],
            "fake:token",
            &client,
            DEFAULT_API_URL,
            Some("testbot"),
        )
        .await
        .unwrap();
        assert!(!msg.is_group);
        // In private chats, mention detection is skipped — no metadata set
        assert!(!msg.metadata.contains_key("was_mentioned"));
    }

    #[test]
    fn test_check_mention_entities_direct() {
        let message = serde_json::json!({
            "text": "Hello @mybot world",
            "entities": [{
                "type": "mention",
                "offset": 6,
                "length": 6
            }]
        });
        assert!(check_mention_entities(&message, "mybot"));
        assert!(!check_mention_entities(&message, "otherbot"));
    }

    #[test]
    fn test_sanitize_telegram_html_basic() {
        // Allowed tags preserved, unknown tags escaped
        let input = "<b>bold</b> <thinking>hmm</thinking>";
        let output = sanitize_telegram_html(input);
        assert!(output.contains("<b>bold</b>"));
        assert!(output.contains("&lt;thinking&gt;"));
    }

    #[tokio::test]
    async fn test_reply_to_message_text_prepended() {
        // When a user replies to a message, the quoted context should be prepended.
        let update = serde_json::json!({
            "update_id": 700,
            "message": {
                "message_id": 100,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": 123, "type": "private" },
                "date": 1700000000,
                "text": "I agree with that",
                "reply_to_message": {
                    "message_id": 99,
                    "from": { "id": 456, "first_name": "Bob" },
                    "chat": { "id": 123, "type": "private" },
                    "date": 1699999990,
                    "text": "We should use Rust"
                }
            }
        });

        let client = test_client();
        let msg =
            parse_telegram_update_test(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
                .await
                .unwrap();
        match &msg.content {
            ChannelContent::Text(t) => {
                assert!(t.starts_with("[Replying to Bob: We should use Rust]\n\n"));
                assert!(t.ends_with("I agree with that"));
            }
            other => panic!("Expected Text, got {other:?}"),
        }
        // reply_to_message_id should be stored in metadata
        assert_eq!(
            msg.metadata
                .get("reply_to_message_id")
                .and_then(|v| v.as_i64()),
            Some(99)
        );
        assert!(!msg.metadata.contains_key("reply_to_bot_message"));
    }

    #[tokio::test]
    async fn test_reply_to_message_with_caption() {
        // reply_to_message that has a caption (e.g. photo) instead of text.
        let update = serde_json::json!({
            "update_id": 701,
            "message": {
                "message_id": 101,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": 123, "type": "private" },
                "date": 1700000000,
                "text": "Nice photo!",
                "reply_to_message": {
                    "message_id": 98,
                    "from": { "id": 456, "first_name": "Carol" },
                    "chat": { "id": 123, "type": "private" },
                    "date": 1699999980,
                    "photo": [{ "file_id": "x", "file_unique_id": "y", "width": 100, "height": 100 }],
                    "caption": "Sunset view"
                }
            }
        });

        let client = test_client();
        let msg =
            parse_telegram_update_test(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
                .await
                .unwrap();
        match &msg.content {
            ChannelContent::Text(t) => {
                assert!(t.starts_with("[Replying to Carol: Sunset view]\n\n"));
                assert!(t.ends_with("Nice photo!"));
            }
            other => panic!("Expected Text, got {other:?}"),
        }
        assert_eq!(
            msg.metadata
                .get("reply_to_message_id")
                .and_then(|v| v.as_i64()),
            Some(98)
        );
    }

    #[tokio::test]
    async fn test_reply_to_message_no_text_no_prepend() {
        // reply_to_message with no text or caption (e.g. sticker) — no prepend, but
        // reply_to_message_id is still stored in metadata.
        let update = serde_json::json!({
            "update_id": 702,
            "message": {
                "message_id": 102,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": 123, "type": "private" },
                "date": 1700000000,
                "text": "What was that?",
                "reply_to_message": {
                    "message_id": 97,
                    "from": { "id": 456, "first_name": "Dave" },
                    "chat": { "id": 123, "type": "private" },
                    "date": 1699999970,
                    "sticker": { "file_id": "stk", "file_unique_id": "z" }
                }
            }
        });

        let client = test_client();
        let msg =
            parse_telegram_update_test(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
                .await
                .unwrap();
        match &msg.content {
            ChannelContent::Text(t) => {
                assert_eq!(t, "What was that?");
            }
            other => panic!("Expected Text, got {other:?}"),
        }
        assert_eq!(
            msg.metadata
                .get("reply_to_message_id")
                .and_then(|v| v.as_i64()),
            Some(97)
        );
    }

    #[tokio::test]
    async fn test_reply_to_message_unknown_sender() {
        // reply_to_message without a `from` field — sender should default to "Unknown".
        let update = serde_json::json!({
            "update_id": 703,
            "message": {
                "message_id": 103,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": 123, "type": "private" },
                "date": 1700000000,
                "text": "Interesting",
                "reply_to_message": {
                    "message_id": 96,
                    "chat": { "id": 123, "type": "private" },
                    "date": 1699999960,
                    "text": "Anonymous message"
                }
            }
        });

        let client = test_client();
        let msg =
            parse_telegram_update_test(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
                .await
                .unwrap();
        match &msg.content {
            ChannelContent::Text(t) => {
                assert!(t.starts_with("[Replying to Unknown: Anonymous message]\n\n"));
                assert!(t.ends_with("Interesting"));
            }
            other => panic!("Expected Text, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_reply_to_bot_message_sets_reply_to_bot_metadata() {
        let update = serde_json::json!({
            "update_id": 705,
            "message": {
                "message_id": 105,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": -200, "type": "group" },
                "date": 1700000000,
                "text": "Please continue",
                "reply_to_message": {
                    "message_id": 104,
                    "from": { "id": 999, "first_name": "OpenFang", "username": "testbot", "is_bot": true },
                    "chat": { "id": -200, "type": "group" },
                    "date": 1699999950,
                    "text": "Initial answer"
                }
            }
        });

        let client = test_client();
        let msg = parse_telegram_update_test(
            &update,
            &[],
            "fake:token",
            &client,
            DEFAULT_API_URL,
            Some("testbot"),
        )
        .await
        .unwrap();

        assert_eq!(
            msg.metadata
                .get("reply_to_message_id")
                .and_then(|v| v.as_i64()),
            Some(104)
        );
        assert_eq!(
            msg.metadata
                .get("reply_to_bot_message")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[tokio::test]
    async fn test_no_reply_to_message_unchanged() {
        // Messages without reply_to_message should be unaffected.
        let update = serde_json::json!({
            "update_id": 704,
            "message": {
                "message_id": 104,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": 123, "type": "private" },
                "date": 1700000000,
                "text": "Just a normal message"
            }
        });

        let client = test_client();
        let msg =
            parse_telegram_update_test(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
                .await
                .unwrap();
        match &msg.content {
            ChannelContent::Text(t) => {
                assert_eq!(t, "Just a normal message");
            }
            other => panic!("Expected Text, got {other:?}"),
        }
        assert!(!msg.metadata.contains_key("reply_to_message_id"));
    }

    async fn start_telegram_test_server() -> (String, tokio::task::JoinHandle<()>) {
        use axum::extract::Json;
        use axum::routing::{get, post};
        use axum::Router;

        let app = Router::new()
            .route(
                "/botfake:token/getFile",
                post(|Json(payload): Json<serde_json::Value>| async move {
                    let file_id = payload["file_id"].as_str().unwrap_or("unknown");
                    let extension = if file_id.contains("photo") {
                        "jpg"
                    } else if file_id.contains("video") {
                        "mp4"
                    } else {
                        "bin"
                    };
                    axum::Json(serde_json::json!({
                        "ok": true,
                        "result": {
                            "file_id": file_id,
                            "file_path": format!("files/{file_id}.{extension}"),
                            "file_size": 1024
                        }
                    }))
                }),
            )
            .route(
                "/file/botfake:token/{*path}",
                get(|| async { axum::body::Bytes::from_static(b"test-media") }),
            );

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        (format!("http://{addr}"), handle)
    }

    async fn start_telegram_getfile_counter_server(
    ) -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
        use axum::extract::Json;
        use axum::routing::post;
        use axum::Router;

        let get_file_calls = Arc::new(AtomicUsize::new(0));
        let app_calls = get_file_calls.clone();

        let app = Router::new().route(
            "/botfake:token/getFile",
            post(move |Json(payload): Json<serde_json::Value>| {
                let calls = app_calls.clone();
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    let file_id = payload["file_id"].as_str().unwrap_or("unknown");
                    axum::Json(serde_json::json!({
                        "ok": true,
                        "result": {
                            "file_id": file_id,
                            "file_path": format!("files/{file_id}.mp4"),
                            "file_size": 1024
                        }
                    }))
                }
            }),
        );

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        (format!("http://{addr}"), get_file_calls, handle)
    }

    async fn start_telegram_file_download_counter_server(
    ) -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
        use axum::extract::Json;
        use axum::routing::{get, post};
        use axum::Router;

        let file_downloads = Arc::new(AtomicUsize::new(0));
        let app_downloads = file_downloads.clone();

        let app = Router::new()
            .route(
                "/botfake:token/getFile",
                post(|Json(payload): Json<serde_json::Value>| async move {
                    let file_id = payload["file_id"].as_str().unwrap_or("unknown");
                    axum::Json(serde_json::json!({
                        "ok": true,
                        "result": {
                            "file_id": file_id,
                            "file_path": format!("files/{file_id}.mp4"),
                            "file_size": 1024
                        }
                    }))
                }),
            )
            .route(
                "/file/botfake:token/{*path}",
                get(move || {
                    let downloads = app_downloads.clone();
                    async move {
                        downloads.fetch_add(1, Ordering::SeqCst);
                        axum::body::Bytes::from_static(b"test-media")
                    }
                }),
            );

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        (format!("http://{addr}"), file_downloads, handle)
    }

    async fn start_telegram_send_message_capture_server() -> (
        String,
        Arc<tokio::sync::Mutex<Vec<serde_json::Value>>>,
        tokio::task::JoinHandle<()>,
    ) {
        use axum::extract::Json;
        use axum::routing::post;
        use axum::Router;

        let payloads = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let app_payloads = payloads.clone();

        let app = Router::new().route(
            "/botfake:token/sendMessage",
            post(move |Json(payload): Json<serde_json::Value>| {
                let payloads = app_payloads.clone();
                async move {
                    payloads.lock().await.push(payload);
                    axum::Json(serde_json::json!({
                        "ok": true,
                        "result": {
                            "message_id": 999
                        }
                    }))
                }
            }),
        );

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        (format!("http://{addr}"), payloads, handle)
    }

    async fn start_telegram_notification_counter_server(
    ) -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
        use axum::extract::Json;
        use axum::routing::post;
        use axum::Router;

        let notifications = Arc::new(AtomicUsize::new(0));
        let app_notifications = notifications.clone();

        let app = Router::new().route(
            "/botfake:token/sendMessage",
            post(move |Json(_payload): Json<serde_json::Value>| {
                let notifications = app_notifications.clone();
                async move {
                    notifications.fetch_add(1, Ordering::SeqCst);
                    axum::Json(serde_json::json!({
                        "ok": true,
                        "result": {
                            "message_id": 999
                        }
                    }))
                }
            }),
        );

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        (format!("http://{addr}"), notifications, handle)
    }

    async fn start_get_me_test_server<F>(
        handler: F,
    ) -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>)
    where
        F: Fn(usize) -> (axum::http::StatusCode, String) + Send + Sync + 'static,
    {
        use axum::response::IntoResponse;
        use axum::routing::get;
        use axum::Router;

        let attempts = Arc::new(AtomicUsize::new(0));
        let handler = Arc::new(handler);
        let app_attempts = attempts.clone();
        let app_handler = handler.clone();

        let app = Router::new().route(
            "/botfake:token/getMe",
            get(move || {
                let attempts = app_attempts.clone();
                let handler = app_handler.clone();
                async move {
                    let attempt = attempts.fetch_add(1, Ordering::SeqCst) + 1;
                    let (status, body) = handler(attempt);
                    (status, body).into_response()
                }
            }),
        );

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        (format!("http://{addr}"), attempts, handle)
    }

    async fn start_command_registration_test_server() -> (
        String,
        Arc<tokio::sync::Mutex<Vec<serde_json::Value>>>,
        tokio::task::JoinHandle<()>,
    ) {
        use axum::extract::Json;
        use axum::routing::{get, post};
        use axum::Router;

        let command_payloads = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let app_payloads = command_payloads.clone();

        let app = Router::new()
            .route(
                "/botfake:token/getMe",
                get(|| async {
                    axum::Json(serde_json::json!({
                        "ok": true,
                        "result": {
                            "username": "testbot"
                        }
                    }))
                }),
            )
            .route(
                "/botfake:token/deleteWebhook",
                post(|| async { axum::Json(serde_json::json!({"ok": true, "result": true})) }),
            )
            .route(
                "/botfake:token/setMyCommands",
                post(move |Json(payload): Json<serde_json::Value>| {
                    let payloads = app_payloads.clone();
                    async move {
                        payloads.lock().await.push(payload);
                        axum::Json(serde_json::json!({"ok": true, "result": true}))
                    }
                }),
            )
            .route(
                "/botfake:token/getUpdates",
                get(|| async { axum::Json(serde_json::json!({"ok": true, "result": []})) }),
            );

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        (format!("http://{addr}"), command_payloads, handle)
    }

    #[tokio::test]
    async fn test_validate_token_retries_transient_local_api_failure() {
        let _guard = local_api_test_guard().await;
        let (api_base_url, attempts, server_handle) = start_get_me_test_server(|attempt| {
            if attempt == 1 {
                (
                    axum::http::StatusCode::OK,
                    serde_json::json!({
                        "ok": false,
                        "description": "Bad Gateway"
                    })
                    .to_string(),
                )
            } else {
                (
                    axum::http::StatusCode::OK,
                    serde_json::json!({
                        "ok": true,
                        "result": {
                            "username": "local_bot"
                        }
                    })
                    .to_string(),
                )
            }
        })
        .await;

        let adapter = TelegramAdapter::new(
            "fake:token".to_string(),
            vec![],
            Duration::from_secs(1),
            Some(api_base_url),
        )
        .with_local_api(true);

        let username = adapter.validate_token().await.unwrap();

        server_handle.abort();

        assert_eq!(username, "local_bot");
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_validate_token_does_not_retry_without_local_api() {
        let (api_base_url, attempts, server_handle) = start_get_me_test_server(|attempt| {
            if attempt == 1 {
                (
                    axum::http::StatusCode::OK,
                    serde_json::json!({
                        "ok": false,
                        "description": "Bad Gateway"
                    })
                    .to_string(),
                )
            } else {
                (
                    axum::http::StatusCode::OK,
                    serde_json::json!({
                        "ok": true,
                        "result": {
                            "username": "should_not_be_reached"
                        }
                    })
                    .to_string(),
                )
            }
        })
        .await;

        let adapter = TelegramAdapter::new(
            "fake:token".to_string(),
            vec![],
            Duration::from_secs(1),
            Some(api_base_url),
        );

        let err = adapter.validate_token().await.unwrap_err().to_string();

        server_handle.abort();

        assert!(err.contains("Bad Gateway"));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_validate_token_does_not_retry_invalid_token_with_local_api() {
        let _guard = local_api_test_guard().await;
        let (api_base_url, attempts, server_handle) = start_get_me_test_server(|_| {
            (
                axum::http::StatusCode::UNAUTHORIZED,
                serde_json::json!({
                    "ok": false,
                    "description": "Unauthorized"
                })
                .to_string(),
            )
        })
        .await;

        let adapter = TelegramAdapter::new(
            "fake:token".to_string(),
            vec![],
            Duration::from_secs(1),
            Some(api_base_url),
        )
        .with_local_api(true);

        let err = adapter.validate_token().await.unwrap_err().to_string();

        server_handle.abort();

        assert!(err.contains("Unauthorized"));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_start_registers_bot_commands_with_new_session_entry() {
        let (api_base_url, command_payloads, server_handle) =
            start_command_registration_test_server().await;

        let adapter = TelegramAdapter::new(
            "fake:token".to_string(),
            vec![],
            Duration::from_millis(10),
            Some(api_base_url),
        );

        let _stream = adapter.start().await.unwrap();

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if !command_payloads.lock().await.is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("timed out waiting for setMyCommands");

        adapter.stop().await.unwrap();
        server_handle.abort();

        let payloads = command_payloads.lock().await.clone();
        assert_eq!(payloads.len(), 1);

        let commands = payloads[0]["commands"]
            .as_array()
            .expect("commands array missing");
        let new_command = commands
            .iter()
            .find(|cmd| cmd["command"].as_str() == Some("new"))
            .expect("/new command missing from Telegram menu");

        assert_eq!(
            new_command["description"].as_str(),
            Some("Start a new conversation")
        );
    }

    #[test]
    fn test_build_get_updates_query_uses_query_params() {
        let no_offset = build_get_updates_query(None);
        assert!(no_offset.iter().any(|(k, v)| *k == "timeout" && v == "30"));
        assert!(no_offset
            .iter()
            .any(|(k, v)| { *k == "allowed_updates" && v == "[\"message\",\"edited_message\"]" }));
        assert!(!no_offset.iter().any(|(k, _)| *k == "offset"));

        let with_offset = build_get_updates_query(Some(42));
        assert!(with_offset.iter().any(|(k, v)| *k == "offset" && v == "42"));
    }

    #[tokio::test]
    async fn test_large_file_requires_local_api() {
        let client = test_client();
        let info = telegram_get_file_info(
            "fake:token",
            &client,
            "large-video",
            DEFAULT_API_URL,
            false,
            21 * 1024 * 1024,
        )
        .await;

        assert!(info.is_none());
    }

    #[tokio::test]
    async fn test_large_file_works_with_local_api() {
        let _guard = local_api_test_guard().await;
        let (api_base_url, server_handle) = start_telegram_test_server().await;
        let client = test_client();

        let info = telegram_get_file_info(
            "fake:token",
            &client,
            "large-video",
            &api_base_url,
            true,
            565 * 1024 * 1024,
        )
        .await
        .unwrap();

        server_handle.abort();

        assert_eq!(info.file_id, "large-video");
        assert!(info
            .download_url
            .contains("/file/botfake:token/files/large-video.mp4"));
        assert!(info.local_path.is_none());
    }

    #[tokio::test]
    async fn test_download_file_local_path_reports_incremental_progress() {
        let temp_root =
            std::env::temp_dir().join(format!("openfang-telegram-test-{}", uuid::Uuid::new_v4()));
        let source_path = temp_root.join("source.mp4");
        let dest_dir = temp_root.join("downloads");
        std::fs::create_dir_all(&temp_root).unwrap();

        let file_size = LOCAL_COPY_PROGRESS_THRESHOLD as usize + (LOCAL_COPY_CHUNK_SIZE * 2);
        std::fs::write(&source_path, vec![7u8; file_size]).unwrap();

        let updates = Arc::new(std::sync::Mutex::new(Vec::<ProgressInfo>::new()));
        let updates_clone = updates.clone();
        let callback: ProgressCallback = Arc::new(move |info| {
            updates_clone.lock().unwrap().push(info);
        });

        let file_info = FileInfo {
            file_id: "local-progress".to_string(),
            file_size: file_size as u64,
            file_path: source_path.to_string_lossy().to_string(),
            download_url: format!("file://{}", source_path.display()),
            local_path: Some(source_path.to_string_lossy().to_string()),
        };

        let client = test_client();
        let downloaded = download_file(
            &client,
            &file_info,
            &dest_dir,
            Some(&callback),
            123,
            Some(456),
        )
        .await
        .unwrap();

        assert!(downloaded.exists());

        let progress = updates.lock().unwrap();
        assert!(
            progress.len() >= 3,
            "expected 0%, mid-copy, and 100% updates"
        );
        assert_eq!(progress.first().unwrap().downloaded_bytes, 0);
        assert_eq!(progress.last().unwrap().downloaded_bytes, file_size as u64);
        assert_eq!(progress.last().unwrap().total_bytes, file_size as u64);
        assert!(progress
            .iter()
            .any(|info| info.downloaded_bytes > 0 && info.downloaded_bytes < file_size as u64));

        let _ = std::fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn test_extract_download_notification_target_filters_users_and_keeps_thread() {
        let update = serde_json::json!({
            "update_id": 900,
            "message": {
                "message_id": 55,
                "message_thread_id": 42,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": -200, "type": "supergroup" },
                "date": 1700000200,
                "video": {
                    "file_id": "video_threaded",
                    "file_size": 1024
                }
            }
        });

        let target = extract_download_notification_target(&update, &[]).unwrap();
        assert_eq!(
            target,
            DownloadNotificationTarget {
                chat_id: -200,
                reply_to_message_id: 55,
                thread_id: Some(42),
            }
        );
        assert!(extract_download_notification_target(&update, &["999".to_string()]).is_none());
    }

    #[test]
    fn test_update_will_attempt_download_respects_limits() {
        let update = serde_json::json!({
            "update_id": 901,
            "message": {
                "message_id": 56,
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": 123, "type": "private" },
                "date": 1700000201,
                "video": {
                    "file_id": "video_large",
                    "file_size": LOCAL_API_SAFE_GETFILE_LIMIT + 1
                }
            }
        });

        assert!(!update_will_attempt_download(
            &update,
            3 * 1024 * 1024 * 1024,
            true,
        ));
        assert!(!update_will_attempt_download(&update, 1024, false));
    }

    #[tokio::test]
    async fn test_telegram_send_notification_raw_includes_reply_and_thread() {
        let (api_base_url, payloads, server_handle) =
            start_telegram_send_message_capture_server().await;

        let sent_message_id = telegram_send_notification_raw(
            &test_client(),
            &api_base_url,
            "fake:token",
            DownloadNotificationTarget {
                chat_id: -200,
                reply_to_message_id: 77,
                thread_id: Some(42),
            },
            "收到媒体，正在处理...",
        )
        .await;

        server_handle.abort();

        assert_eq!(sent_message_id, Some(999));
        let payloads = payloads.lock().await;
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0]["chat_id"].as_i64(), Some(-200));
        assert_eq!(payloads[0]["reply_to_message_id"].as_i64(), Some(77));
        assert_eq!(payloads[0]["message_thread_id"].as_i64(), Some(42));
        assert_eq!(payloads[0]["text"].as_str(), Some("收到媒体，正在处理..."));
    }

    #[test]
    fn test_check_mention_entities_handles_utf16_offsets() {
        let message = serde_json::json!({
            "text": "🙂 @mybot world",
            "entities": [{
                "type": "mention",
                "offset": 3,
                "length": 6
            }]
        });

        assert!(check_mention_entities(&message, "mybot"));
    }

    #[test]
    fn test_looks_like_local_bot_api_path_supports_windows_and_unix() {
        assert!(looks_like_local_bot_api_path("/var/lib/telegram/file.bin"));
        assert!(looks_like_local_bot_api_path("C:\\telegram\\file.bin"));
        assert!(looks_like_local_bot_api_path("\\\\server\\share\\file.bin"));
        assert!(!looks_like_local_bot_api_path("files/file.bin"));
    }

    #[tokio::test]
    async fn test_merge_media_group_preserves_original_item_order() {
        let _guard = local_api_test_guard().await;
        let (api_base_url, server_handle) = start_telegram_test_server().await;

        let updates = vec![
            serde_json::json!({
                "update_id": 800,
                "message": {
                    "message_id": 201,
                    "media_group_id": "group-1",
                    "from": { "id": 123, "first_name": "Alice" },
                    "chat": { "id": 123, "type": "private" },
                    "date": 1700000000,
                    "photo": [
                        { "file_id": "photo_1_small", "file_unique_id": "a", "width": 90, "height": 90 },
                        { "file_id": "photo_1", "file_unique_id": "b", "width": 800, "height": 600 }
                    ],
                    "caption": "Trip recap"
                }
            }),
            serde_json::json!({
                "update_id": 801,
                "message": {
                    "message_id": 202,
                    "media_group_id": "group-1",
                    "from": { "id": 123, "first_name": "Alice" },
                    "chat": { "id": 123, "type": "private" },
                    "date": 1700000001,
                    "video": {
                        "file_id": "video_1",
                        "file_unique_id": "c",
                        "duration": 8,
                        "file_size": 2048
                    }
                }
            }),
            serde_json::json!({
                "update_id": 802,
                "message": {
                    "message_id": 203,
                    "media_group_id": "group-1",
                    "from": { "id": 123, "first_name": "Alice" },
                    "chat": { "id": 123, "type": "private" },
                    "date": 1700000002,
                    "photo": [
                        { "file_id": "photo_2_small", "file_unique_id": "d", "width": 90, "height": 90 },
                        { "file_id": "photo_2", "file_unique_id": "e", "width": 800, "height": 600 }
                    ]
                }
            }),
        ];

        let client = test_client();
        let temp_dir = std::env::temp_dir();
        let merged = merge_media_group_updates(
            &updates,
            &[],
            "fake:token",
            &client,
            &api_base_url,
            None,
            false,
            &temp_dir,
            2 * 1024 * 1024 * 1024,
            None,
            false,
            None,
        )
        .await
        .unwrap();

        server_handle.abort();

        // Verify the content is now a short summary
        let ChannelContent::Text(content) = merged.content else {
            panic!("Expected merged text content");
        };

        // The new format should be a short summary
        assert!(content.contains("收到 Telegram 媒体批次"));
        assert!(content.contains("1 个视频"));
        assert!(content.contains("2 张图片"));

        // Verify telegram_media_batch exists in metadata
        let batch_value = merged
            .metadata
            .get("telegram_media_batch")
            .expect("telegram_media_batch should exist in metadata");

        let batch: crate::telegram_media_batch::TelegramMediaBatch =
            serde_json::from_value(batch_value.clone()).expect("Should deserialize batch");

        // Verify batch structure
        assert_eq!(batch.items.len(), 3);
        assert_eq!(batch.caption, Some("Trip recap".to_string()));
        assert_eq!(batch.batch_key, "group_123_group_1_117d5863");

        // Verify order: photo, video, photo
        assert_eq!(
            batch.items[0].kind,
            crate::telegram_media_batch::MediaItemKind::Image
        );
        assert_eq!(
            batch.items[1].kind,
            crate::telegram_media_batch::MediaItemKind::Video
        );
        assert_eq!(
            batch.items[2].kind,
            crate::telegram_media_batch::MediaItemKind::Image
        );
    }

    #[tokio::test]
    async fn test_merge_media_group_downloads_single_item_only_once() {
        let (api_base_url, file_downloads, server_handle) =
            start_telegram_file_download_counter_server().await;
        let temp_dir =
            std::env::temp_dir().join(format!("openfang-telegram-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let update = serde_json::json!({
            "update_id": 850,
            "message": {
                "message_id": 205,
                "media_group_id": "single-video-group",
                "from": { "id": 123, "first_name": "Alice" },
                "chat": { "id": 123, "type": "private" },
                "date": 1700000100,
                "video": {
                    "file_id": "video_single",
                    "file_unique_id": "vs",
                    "duration": 8,
                    "file_size": 1024
                }
            }
        });

        let client = test_client();
        let merged = merge_media_group_updates(
            &[update],
            &[],
            "fake:token",
            &client,
            &api_base_url,
            None,
            true,
            &temp_dir,
            2 * 1024 * 1024 * 1024,
            None,
            false,
            None,
        )
        .await
        .unwrap();

        server_handle.abort();

        assert_eq!(file_downloads.load(Ordering::SeqCst), 1);
        let batch_value = merged.metadata.get("telegram_media_batch").unwrap();
        let batch: crate::telegram_media_batch::TelegramMediaBatch =
            serde_json::from_value(batch_value.clone()).unwrap();
        assert_eq!(batch.items.len(), 1);
        assert_eq!(
            batch.items[0].status,
            crate::telegram_media_batch::MediaItemStatus::Ready
        );
        assert!(batch.items[0].local_path.is_some());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_merge_media_group_does_not_notify_unlisted_user() {
        let (api_base_url, notifications, server_handle) =
            start_telegram_notification_counter_server().await;

        let update = serde_json::json!({
            "update_id": 860,
            "message": {
                "message_id": 206,
                "media_group_id": "filtered-group",
                "from": { "id": 999, "first_name": "Mallory" },
                "chat": { "id": 123, "type": "private" },
                "date": 1700000200,
                "photo": [
                    { "file_id": "photo_filtered_small", "file_unique_id": "pf0", "width": 90, "height": 90 },
                    { "file_id": "photo_filtered", "file_unique_id": "pf1", "width": 800, "height": 600 }
                ]
            }
        });

        let client = test_client();
        let temp_dir = std::env::temp_dir();
        let merged = merge_media_group_updates(
            &[update],
            &[String::from("123")],
            "fake:token",
            &client,
            &api_base_url,
            None,
            true,
            &temp_dir,
            2 * 1024 * 1024 * 1024,
            None,
            false,
            None,
        )
        .await;

        server_handle.abort();

        assert!(merged.is_none());
        assert_eq!(notifications.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_merge_media_group_one_video_nine_images_summary_and_order() {
        let _guard = local_api_test_guard().await;
        let (api_base_url, server_handle) = start_telegram_test_server().await;
        let mut updates = Vec::new();
        for idx in 0..10 {
            let message = if idx == 0 {
                serde_json::json!({
                    "message_id": 300 + idx,
                    "media_group_id": "album-10",
                    "from": { "id": 456, "first_name": "Bob" },
                    "chat": { "id": 456, "type": "private" },
                    "date": 1700001000 + idx,
                    "video": { "file_id": "video_main", "file_unique_id": "v1", "duration": 12, "file_size": 4096 },
                    "caption": "batch caption"
                })
            } else {
                serde_json::json!({
                    "message_id": 300 + idx,
                    "media_group_id": "album-10",
                    "from": { "id": 456, "first_name": "Bob" },
                    "chat": { "id": 456, "type": "private" },
                    "date": 1700001000 + idx,
                    "photo": [
                        { "file_id": format!("photo_{idx}_small"), "file_unique_id": format!("s{idx}"), "width": 90, "height": 90 },
                        { "file_id": format!("photo_{idx}"), "file_unique_id": format!("b{idx}"), "width": 800, "height": 600 }
                    ]
                })
            };
            updates.push(serde_json::json!({
                "update_id": 900 + idx,
                "message": message
            }));
        }

        let client = test_client();
        let merged = merge_media_group_updates(
            &updates,
            &[],
            "fake:token",
            &client,
            &api_base_url,
            None,
            false,
            &std::env::temp_dir(),
            2 * 1024 * 1024 * 1024,
            None,
            false,
            None,
        )
        .await
        .unwrap();

        server_handle.abort();

        let ChannelContent::Text(content) = merged.content else {
            panic!("Expected merged text content");
        };
        assert!(content.contains("收到 Telegram 媒体批次"));
        assert!(content.contains("1 个视频"));
        assert!(content.contains("9 张图片"));

        let batch_value = merged.metadata.get("telegram_media_batch").unwrap();
        let batch: crate::telegram_media_batch::TelegramMediaBatch =
            serde_json::from_value(batch_value.clone()).unwrap();
        assert_eq!(batch.items.len(), 10);
        assert_eq!(
            batch.items[0].kind,
            crate::telegram_media_batch::MediaItemKind::Video
        );
        for item in batch.items.iter().skip(1) {
            assert_eq!(item.kind, crate::telegram_media_batch::MediaItemKind::Image);
        }
    }

    #[tokio::test]
    async fn test_large_video_safe_limit_skips_getfile_with_structured_hint() {
        let _guard = local_api_test_guard().await;
        let oversized = LOCAL_API_SAFE_GETFILE_LIMIT + 1;
        let (api_base_url, calls, server_handle) = start_telegram_getfile_counter_server().await;
        let update = serde_json::json!({
            "update_id": 1200,
            "message": {
                "message_id": 510,
                "media_group_id": "large-video-group",
                "from": { "id": 999, "first_name": "Carol" },
                "chat": { "id": 999, "type": "private" },
                "date": 1700010000,
                "video": {
                    "file_id": "huge_video",
                    "file_unique_id": "hv",
                    "duration": 30,
                    "file_size": oversized
                }
            }
        });

        let client = test_client();
        let merged = merge_media_group_updates(
            &[update],
            &[],
            "fake:token",
            &client,
            &api_base_url,
            None,
            false,
            &std::env::temp_dir(),
            2 * 1024 * 1024 * 1024,
            None,
            true,
            None,
        )
        .await
        .unwrap();

        server_handle.abort();
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        let batch_value = merged.metadata.get("telegram_media_batch").unwrap();
        let batch: crate::telegram_media_batch::TelegramMediaBatch =
            serde_json::from_value(batch_value.clone()).unwrap();
        assert_eq!(batch.items.len(), 1);
        let item = &batch.items[0];
        assert_eq!(
            item.status,
            crate::telegram_media_batch::MediaItemStatus::SkippedSafeLimit
        );
        let hint = item.download_hint.as_ref().unwrap();
        assert_eq!(hint.strategy, "telegram_bot_api_file");
        assert_eq!(hint.file_id, "huge_video");
        assert_eq!(hint.api_base_url, api_base_url);
        assert!(hint.use_local_api);
        assert!(hint.download_url.is_none());
        assert!(hint
            .reason
            .as_deref()
            .unwrap_or_default()
            .contains("safe getFile limit"));
    }
}
