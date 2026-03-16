//! Telegram Bot API adapter for the OpenFang channel bridge.
//!
//! Uses long-polling via `getUpdates` with exponential backoff on failures.
//! No external Telegram crate — just `reqwest` for full control over error handling.

use crate::types::{
    split_message, ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser,
    LifecycleReaction,
};
use async_trait::async_trait;
use futures::Stream;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
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
        }
    }

    /// Validate the bot token by calling `getMe`.
    pub async fn validate_token(&self) -> Result<String, Box<dyn std::error::Error>> {
        let url = format!("{}/bot{}/getMe", self.api_base_url, self.token.as_str());
        let resp: serde_json::Value = self.client.get(&url).send().await?.json().await?;

        if resp["ok"].as_bool() != Some(true) {
            let desc = resp["description"].as_str().unwrap_or("unknown error");
            let hint = if desc.to_lowercase().contains("unauthorized") {
                " (Check that the bot token is correct. Get it from @BotFather on Telegram.)"
            } else if desc.to_lowercase().contains("not found") {
                " (The bot token format may be invalid. Expected format: 123456789:ABCdefGHI...)"
            } else {
                ""
            };
            return Err(format!("Telegram getMe failed: {desc}{hint}").into());
        }

        let bot_name = resp["result"]["username"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        Ok(bot_name)
    }

    /// Call `sendMessage` on the Telegram API.
    ///
    /// When `thread_id` is provided, includes `message_thread_id` in the request
    /// so the message lands in the correct forum topic.
    async fn api_send_message(
        &self,
        chat_id: i64,
        text: &str,
        thread_id: Option<i64>,
    ) -> Result<(), Box<dyn std::error::Error>> {
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

        match content {
            ChannelContent::Text(text) => {
                self.api_send_message(chat_id, &text, thread_id).await?;
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
                self.api_send_message(chat_id, text.trim(), thread_id)
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
                .json(&serde_json::json!({"drop_pending_updates": true}))
                .send()
                .await
            {
                Ok(_) => info!("Telegram: cleared webhook, polling mode active"),
                Err(e) => tracing::warn!("Telegram: deleteWebhook failed (non-fatal): {e}"),
            }
        }

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);

        let token = self.token.clone();
        let client = self.client.clone();
        let allowed_users = self.allowed_users.clone();
        let poll_interval = self.poll_interval;
        let api_base_url = self.api_base_url.clone();
        let bot_username = self.bot_username.clone();
        let mut shutdown = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let mut offset: Option<i64> = None;
            let mut backoff = INITIAL_BACKOFF;

            loop {
                // Check shutdown
                if *shutdown.borrow() {
                    break;
                }

                // Build getUpdates request
                let url = format!("{}/bot{}/getUpdates", api_base_url, token.as_str());
                let mut params = serde_json::json!({
                    "timeout": LONG_POLL_TIMEOUT,
                    "allowed_updates": ["message", "edited_message"],
                });
                if let Some(off) = offset {
                    params["offset"] = serde_json::json!(off);
                }

                // Make the request with a timeout slightly longer than the long-poll timeout
                let request_timeout = Duration::from_secs(LONG_POLL_TIMEOUT + 10);
                let result = tokio::select! {
                    res = async {
                        client
                            .get(&url)
                            .json(&params)
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

                for update in updates {
                    // Track offset for dedup
                    if let Some(update_id) = update["update_id"].as_i64() {
                        offset = Some(update_id + 1);
                    }

                    // Parse the message
                    let bot_uname = bot_username.read().await.clone();
                    let msg = match parse_telegram_update(
                        update,
                        &allowed_users,
                        token.as_str(),
                        &client,
                        &api_base_url,
                        bot_uname.as_deref(),
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
        Ok(())
    }
}

/// Parse a Telegram update JSON into a `ChannelMessage`, or `None` if filtered/unparseable.
/// Handles both `message` and `edited_message` update types.
/// Resolve a Telegram file_id to a download URL via the Bot API.
async fn telegram_get_file_url(
    token: &str,
    client: &reqwest::Client,
    file_id: &str,
    api_base_url: &str,
) -> Option<String> {
    let url = format!("{api_base_url}/bot{token}/getFile");
    let resp = client
        .post(&url)
        .json(&serde_json::json!({"file_id": file_id}))
        .send()
        .await
        .ok()?;
    let body: serde_json::Value = resp.json().await.ok()?;
    if body["ok"].as_bool() != Some(true) {
        return None;
    }
    let file_path = body["result"]["file_path"].as_str()?;
    Some(format!("{api_base_url}/file/bot{token}/{file_path}"))
}

async fn parse_telegram_update(
    update: &serde_json::Value,
    allowed_users: &[String],
    token: &str,
    client: &reqwest::Client,
    api_base_url: &str,
    bot_username: Option<&str>,
) -> Option<ChannelMessage> {
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
        let caption = message["caption"].as_str().map(String::from);
        match telegram_get_file_url(token, client, file_id, api_base_url).await {
            Some(url) => ChannelContent::Image { url, caption },
            None => ChannelContent::Text(format!(
                "[Photo received{}]",
                caption
                    .as_deref()
                    .map(|c| format!(": {c}"))
                    .unwrap_or_default()
            )),
        }
    } else if message.get("document").is_some() {
        let file_id = message["document"]["file_id"].as_str().unwrap_or("");
        let filename = message["document"]["file_name"]
            .as_str()
            .unwrap_or("document")
            .to_string();
        match telegram_get_file_url(token, client, file_id, api_base_url).await {
            Some(url) => ChannelContent::File { url, filename },
            None => ChannelContent::Text(format!("[Document received: {filename}]")),
        }
    } else if message.get("voice").is_some() {
        let file_id = message["voice"]["file_id"].as_str().unwrap_or("");
        let duration = message["voice"]["duration"].as_u64().unwrap_or(0) as u32;
        match telegram_get_file_url(token, client, file_id, api_base_url).await {
            Some(url) => ChannelContent::Voice {
                url,
                duration_seconds: duration,
            },
            None => ChannelContent::Text(format!("[Voice message, {duration}s]")),
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
    }
    if is_group {
        if let Some(bot_uname) = bot_username {
            let was_mentioned = check_mention_entities(message, bot_uname);
            if was_mentioned {
                metadata.insert("was_mentioned".to_string(), serde_json::json!(true));
            }
        }
    }

    Some(ChannelMessage {
        channel: ChannelType::Telegram,
        platform_message_id: message_id.to_string(),
        sender: ChannelUser {
            platform_id: chat_id.to_string(),
            display_name,
            openfang_user: None,
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
/// Inspects both `entities` (for text messages) and `caption_entities` (for media
/// with captions) for entity type `"mention"` whose text matches `@bot_username`.
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
                if entity["type"].as_str() != Some("mention") {
                    continue;
                }
                let offset = entity["offset"].as_i64().unwrap_or(0) as usize;
                let length = entity["length"].as_i64().unwrap_or(0) as usize;
                if offset + length <= text.len() {
                    let mention_text = &text[offset..offset + length];
                    if mention_text.to_lowercase() == bot_mention {
                        return true;
                    }
                }
            }
        }
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

    fn test_client() -> reqwest::Client {
        reqwest::Client::new()
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
        let msg = parse_telegram_update(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
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
        let msg = parse_telegram_update(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
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
            parse_telegram_update(&update, &[], "fake:token", &client, DEFAULT_API_URL, None).await;
        assert!(msg.is_some());

        // Non-matching allowed_users = filter out
        let blocked: Vec<String> = vec!["111".to_string(), "222".to_string()];
        let msg = parse_telegram_update(
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
        let msg = parse_telegram_update(
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
        let msg = parse_telegram_update(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
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
        let msg = parse_telegram_update(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
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
        let msg = parse_telegram_update(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
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
        let msg = parse_telegram_update(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
            .await
            .unwrap();
        // With a fake token, getFile will fail, so we get a text fallback
        match &msg.content {
            ChannelContent::Text(t) => {
                assert!(t.contains("Photo received"));
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
        let msg = parse_telegram_update(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
            .await
            .unwrap();
        match &msg.content {
            ChannelContent::Text(t) => {
                assert!(t.contains("Document received"));
                assert!(t.contains("report.pdf"));
            }
            ChannelContent::File { filename, .. } => {
                assert_eq!(filename, "report.pdf");
            }
            other => panic!("Expected Text or File for document, got {other:?}"),
        }
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
        let msg = parse_telegram_update(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
            .await
            .unwrap();
        match &msg.content {
            ChannelContent::Text(t) => {
                assert!(t.contains("Voice message"));
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
        let msg = parse_telegram_update(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
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
        let msg = parse_telegram_update(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
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
        let msg = parse_telegram_update(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
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
        let msg = parse_telegram_update(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
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
            parse_telegram_update(&update, &[], "fake:token", &client, DEFAULT_API_URL, None).await;
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
        let msg = parse_telegram_update(
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
        let msg = parse_telegram_update(
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
        let msg = parse_telegram_update(
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
        let msg = parse_telegram_update(
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
        let msg = parse_telegram_update(
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
        let msg = parse_telegram_update(
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
        let msg = parse_telegram_update(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
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
        let msg = parse_telegram_update(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
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
        let msg = parse_telegram_update(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
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
        let msg = parse_telegram_update(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
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
        let msg = parse_telegram_update(&update, &[], "fake:token", &client, DEFAULT_API_URL, None)
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
}
