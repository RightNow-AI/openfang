//! Telegram Bot API adapter for the OpenFang channel bridge.
//!
//! Uses long-polling via `getUpdates` with exponential backoff on failures.
//! No external Telegram crate — just `reqwest` for full control over error handling.

use crate::formatter;
use crate::types::{
    split_message, ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser,
    StreamSink,
};
use openfang_types::config::{TelegramStreamConfig, TelegramStreamMode};
use async_trait::async_trait;
use futures::Stream;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tracing::{debug, error, info, warn};
use zeroize::Zeroizing;

/// Maximum backoff duration on API failures.
const MAX_BACKOFF: Duration = Duration::from_secs(60);
/// Initial backoff duration on API failures.
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
/// Telegram long-polling timeout (seconds) — sent as the `timeout` parameter to getUpdates.
const LONG_POLL_TIMEOUT: u64 = 30;

/// Telegram Bot API adapter using long-polling.
pub struct TelegramAdapter {
    /// SECURITY: Bot token is zeroized on drop to prevent memory disclosure.
    token: Zeroizing<String>,
    client: reqwest::Client,
    allowed_users: Vec<i64>,
    poll_interval: Duration,
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Streaming mode configuration.
    stream_mode: TelegramStreamMode,
    /// Streaming behavior configuration.
    stream_config: TelegramStreamConfig,
}

impl TelegramAdapter {
    /// Create a new Telegram adapter.
    ///
    /// `token` is the raw bot token (read from env by the caller).
    /// `allowed_users` is the list of Telegram user IDs allowed to interact (empty = allow all).
    pub fn new(token: String, allowed_users: Vec<i64>, poll_interval: Duration) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            token: Zeroizing::new(token),
            client: reqwest::Client::new(),
            allowed_users,
            poll_interval,
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            stream_mode: TelegramStreamMode::default(),
            stream_config: TelegramStreamConfig::default(),
        }
    }

    /// Create a new Telegram adapter with streaming configuration.
    pub fn with_streaming(
        token: String,
        allowed_users: Vec<i64>,
        poll_interval: Duration,
        stream_mode: TelegramStreamMode,
        stream_config: TelegramStreamConfig,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            token: Zeroizing::new(token),
            client: reqwest::Client::new(),
            allowed_users,
            poll_interval,
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            stream_mode,
            stream_config,
        }
    }

    /// Validate the bot token by calling `getMe`.
    pub async fn validate_token(&self) -> Result<String, Box<dyn std::error::Error>> {
        let url = format!("https://api.telegram.org/bot{}/getMe", self.token.as_str());
        let resp: serde_json::Value = self.client.get(&url).send().await?.json().await?;

        if resp["ok"].as_bool() != Some(true) {
            let desc = resp["description"].as_str().unwrap_or("unknown error");
            return Err(format!("Telegram getMe failed: {desc}").into());
        }

        let bot_name = resp["result"]["username"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        Ok(bot_name)
    }

    /// Call `sendMessage` on the Telegram API.
    async fn api_send_message(
        &self,
        chat_id: i64,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.api_send_message_with_id(chat_id, text).await?;
        Ok(())
    }

    /// Call `sendMessage` on the Telegram API, returning the message_id of the last chunk.
    async fn api_send_message_with_id(
        &self,
        chat_id: i64,
        text: &str,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.token.as_str()
        );

        // Sanitize: strip unsupported HTML tags so Telegram doesn't reject with 400.
        // Telegram only allows: b, i, u, s, tg-spoiler, a, code, pre, blockquote.
        // Any other tag (e.g. <name>, <thinking>) causes a 400 Bad Request.
        let sanitized = sanitize_telegram_html(text);

        // Telegram has a 4096 character limit per message — split if needed
        let chunks = split_message(&sanitized, 4096);
        let mut last_message_id: i64 = 0;
        for chunk in chunks {
            let body = serde_json::json!({
                "chat_id": chat_id,
                "text": chunk,
                "parse_mode": "HTML",
            });

            let resp = self.client.post(&url).json(&body).send().await?;
            let status = resp.status();
            if !status.is_success() {
                let body_text = resp.text().await.unwrap_or_default();
                warn!("Telegram sendMessage failed ({status}): {body_text}");
            } else {
                let resp_body: serde_json::Value = resp.json().await.unwrap_or_default();
                last_message_id = resp_body["result"]["message_id"].as_i64().unwrap_or(0);
            }
        }
        Ok(last_message_id)
    }

    /// Call `sendPhoto` on the Telegram API.
    async fn api_send_photo(
        &self,
        chat_id: i64,
        photo_url: &str,
        caption: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendPhoto",
            self.token.as_str()
        );
        let mut body = serde_json::json!({
            "chat_id": chat_id,
            "photo": photo_url,
        });
        if let Some(cap) = caption {
            body["caption"] = serde_json::Value::String(cap.to_string());
            body["parse_mode"] = serde_json::Value::String("HTML".to_string());
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
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendDocument",
            self.token.as_str()
        );
        let body = serde_json::json!({
            "chat_id": chat_id,
            "document": document_url,
            "caption": filename,
        });
        let resp = self.client.post(&url).json(&body).send().await?;
        if !resp.status().is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            warn!("Telegram sendDocument failed: {body_text}");
        }
        Ok(())
    }

    /// Call `sendVoice` on the Telegram API.
    async fn api_send_voice(
        &self,
        chat_id: i64,
        voice_url: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendVoice",
            self.token.as_str()
        );
        let body = serde_json::json!({
            "chat_id": chat_id,
            "voice": voice_url,
        });
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
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendLocation",
            self.token.as_str()
        );
        let body = serde_json::json!({
            "chat_id": chat_id,
            "latitude": lat,
            "longitude": lon,
        });
        let resp = self.client.post(&url).json(&body).send().await?;
        if !resp.status().is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            warn!("Telegram sendLocation failed: {body_text}");
        }
        Ok(())
    }

    /// Call `sendChatAction` to show "typing..." indicator.
    async fn api_send_typing(&self, chat_id: i64) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendChatAction",
            self.token.as_str()
        );
        let body = serde_json::json!({
            "chat_id": chat_id,
            "action": "typing",
        });
        let _ = self.client.post(&url).json(&body).send().await?;
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
        // Validate token first (fail fast)
        let bot_name = self.validate_token().await?;
        info!("Telegram bot @{bot_name} connected");

        // Clear any existing webhook to avoid 409 Conflict during getUpdates polling.
        // This is necessary when the daemon restarts — the old polling session may
        // still be active on Telegram's side for ~30s, causing 409 errors.
        {
            let delete_url = format!(
                "https://api.telegram.org/bot{}/deleteWebhook",
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
                let url = format!("https://api.telegram.org/bot{}/getUpdates", token.as_str());
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

                // Handle conflict (another bot instance polling)
                if status.as_u16() == 409 {
                    error!("Telegram 409 Conflict — another bot instance is running. Stopping.");
                    break;
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
                    let msg = match parse_telegram_update(update, &allowed_users) {
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
        let chat_id: i64 = user
            .platform_id
            .parse()
            .map_err(|_| format!("Invalid Telegram chat_id: {}", user.platform_id))?;

        match content {
            ChannelContent::Text(text) => {
                self.api_send_message(chat_id, &text).await?;
            }
            ChannelContent::Image { url, caption } => {
                self.api_send_photo(chat_id, &url, caption.as_deref())
                    .await?;
            }
            ChannelContent::File { url, filename } => {
                self.api_send_document(chat_id, &url, &filename).await?;
            }
            ChannelContent::Voice { url, .. } => {
                self.api_send_voice(chat_id, &url).await?;
            }
            ChannelContent::Location { lat, lon } => {
                self.api_send_location(chat_id, lat, lon).await?;
            }
            ChannelContent::Command { name, args } => {
                let text = format!("/{name} {}", args.join(" "));
                self.api_send_message(chat_id, text.trim()).await?;
            }
        }
        Ok(())
    }

    async fn send_typing(&self, user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        let chat_id: i64 = user
            .platform_id
            .parse()
            .map_err(|_| format!("Invalid Telegram chat_id: {}", user.platform_id))?;
        self.api_send_typing(chat_id).await
    }

    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.shutdown_tx.send(true);
        Ok(())
    }

    fn supports_streaming(&self) -> bool {
        self.stream_mode != TelegramStreamMode::Off
    }

    async fn begin_stream(
        &self,
        user: &ChannelUser,
        is_group: bool,
    ) -> Result<Box<dyn StreamSink>, Box<dyn std::error::Error>> {
        let chat_id: i64 = user
            .platform_id
            .parse()
            .map_err(|_| format!("Invalid Telegram chat_id: {}", user.platform_id))?;

        // Resolve effective mode: Auto → Edit (Draft requires Bot API 9.5 which isn't widely available yet)
        let effective_mode = match self.stream_mode {
            TelegramStreamMode::Off => {
                return Err("Streaming is disabled".into());
            }
            TelegramStreamMode::Edit => StreamMode::Edit,
            TelegramStreamMode::Draft => {
                if is_group {
                    // Draft doesn't work in groups — fall back to Edit
                    StreamMode::Edit
                } else {
                    StreamMode::Draft
                }
            }
            TelegramStreamMode::Auto => {
                // Auto: always use Edit for now (Draft support is experimental)
                StreamMode::Edit
            }
        };

        Ok(Box::new(TelegramStreamSink::new(
            self.token.as_str().to_string(),
            self.client.clone(),
            chat_id,
            effective_mode,
            self.stream_config.clone(),
        )))
    }
}

/// Internal mode enum for the active streaming strategy.
#[derive(Debug, Clone, Copy, PartialEq)]
enum StreamMode {
    Edit,
    Draft,
}

/// Streaming output sink for Telegram — buffers text and flushes via edits.
struct TelegramStreamSink {
    token: String,
    client: reqwest::Client,
    chat_id: i64,
    mode: StreamMode,
    config: TelegramStreamConfig,

    /// Accumulated full text buffer.
    buffer: String,
    /// Message ID of the message being edited (Edit mode).
    message_id: Option<i64>,
    /// Whether the first message has been sent.
    first_sent: bool,
    /// Last flush instant for rate limiting.
    last_flush: tokio::time::Instant,
    /// Characters in buffer at last flush.
    chars_at_last_flush: usize,
    /// Number of consecutive 429 errors for adaptive backoff.
    consecutive_rate_limits: u32,
    /// Current extra delay from rate limiting.
    rate_limit_delay: Duration,
    /// Messages already sent (for multi-message splitting).
    sent_messages: Vec<i64>,
    /// Whether finalize has been called.
    finalized: bool,
}

impl TelegramStreamSink {
    fn new(
        token: String,
        client: reqwest::Client,
        chat_id: i64,
        mode: StreamMode,
        config: TelegramStreamConfig,
    ) -> Self {
        Self {
            token,
            client,
            chat_id,
            mode,
            config,
            buffer: String::new(),
            message_id: None,
            first_sent: false,
            last_flush: tokio::time::Instant::now(),
            chars_at_last_flush: 0,
            consecutive_rate_limits: 0,
            rate_limit_delay: Duration::ZERO,
            sent_messages: Vec::new(),
            finalized: false,
        }
    }

    /// Whether enough time and chars have accumulated for a flush.
    fn should_flush(&self) -> bool {
        let elapsed = self.last_flush.elapsed();
        let interval = Duration::from_millis(self.config.flush_interval_ms) + self.rate_limit_delay;
        let chars_added = self.buffer.len().saturating_sub(self.chars_at_last_flush);

        elapsed >= interval && chars_added >= self.config.min_chars_per_flush
    }

    /// Flush the current buffer to Telegram.
    async fn flush(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        // Check if we need to split the message
        if self.buffer.len() > self.config.max_message_chars && self.message_id.is_some() {
            return self.split_and_continue().await;
        }

        // Prepare display text: escape HTML entities + cursor indicator
        let display_text =
            formatter::escape_html_entities(&self.buffer) + &self.config.cursor_indicator;

        match self.mode {
            StreamMode::Edit => {
                if !self.first_sent {
                    // First flush: send a new message
                    match self.send_message_raw(&display_text, None).await {
                        Ok(msg_id) => {
                            self.message_id = Some(msg_id);
                            self.first_sent = true;
                        }
                        Err(e) => {
                            warn!("Telegram stream: sendMessage failed: {e}");
                            return Err(e);
                        }
                    }
                } else if let Some(msg_id) = self.message_id {
                    // Subsequent flushes: edit the existing message
                    let edit_err = match self.edit_message_raw(msg_id, &display_text, None).await {
                        Ok(()) => None,
                        Err(e) => Some(e.to_string()),
                    };
                    if let Some(err_str) = edit_err {
                        if err_str.contains("429") {
                            self.handle_rate_limit(&err_str).await;
                        } else {
                            warn!("Telegram stream: editMessageText failed: {err_str}");
                        }
                    }
                }
            }
            StreamMode::Draft => {
                // Draft mode: use sendMessageDraft (experimental)
                // Falls back to Edit if it fails
                let draft_failed = match self.send_draft(&display_text).await {
                    Ok(()) => false,
                    Err(e) => {
                        warn!("Telegram stream: Draft failed, falling back to Edit: {e}");
                        true
                    }
                };
                if draft_failed {
                    self.mode = StreamMode::Edit;
                    // Inline the Edit path instead of recursive flush()
                    match self.send_message_raw(&display_text, None).await {
                        Ok(msg_id) => {
                            self.message_id = Some(msg_id);
                            self.first_sent = true;
                        }
                        Err(e) => {
                            warn!("Telegram stream: fallback sendMessage failed: {e}");
                            return Err(e);
                        }
                    }
                }
            }
        }

        self.last_flush = tokio::time::Instant::now();
        self.chars_at_last_flush = self.buffer.len();
        self.consecutive_rate_limits = 0;

        Ok(())
    }

    /// Split a long message: finalize the current message and start a new one.
    async fn split_and_continue(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let split_at =
            formatter::find_split_point(&self.buffer, self.config.max_message_chars);
        let completed_text = self.buffer[..split_at].to_string();
        let remaining = self.buffer[split_at..].trim_start().to_string();

        // Finalize the current message with the completed chunk
        if let Some(msg_id) = self.message_id {
            let formatted = formatter::escape_html_entities(&completed_text);
            self.edit_message_raw(msg_id, &formatted, None).await.ok();
            self.sent_messages.push(msg_id);
        }

        // Start a new message with the remaining text + cursor
        self.buffer = remaining;
        let display =
            formatter::escape_html_entities(&self.buffer) + &self.config.cursor_indicator;
        match self.send_message_raw(&display, None).await {
            Ok(msg_id) => {
                self.message_id = Some(msg_id);
                self.chars_at_last_flush = self.buffer.len();
            }
            Err(e) => {
                warn!("Telegram stream: failed to start continuation message: {e}");
            }
        }

        Ok(())
    }

    /// Handle a 429 rate limit response with adaptive backoff.
    async fn handle_rate_limit(&mut self, err_text: &str) {
        self.consecutive_rate_limits += 1;

        // Parse retry_after from error if available
        let base_delay = if let Some(pos) = err_text.find("retry_after") {
            err_text[pos..]
                .split(|c: char| c.is_ascii_digit())
                .nth(0)
                .and_then(|_| {
                    err_text[pos..]
                        .chars()
                        .filter(|c| c.is_ascii_digit())
                        .collect::<String>()
                        .parse::<u64>()
                        .ok()
                })
                .map(Duration::from_secs)
                .unwrap_or(Duration::from_secs(1))
        } else {
            Duration::from_secs(1)
        };

        // Adaptive: double interval after 3 consecutive rate limits
        if self.consecutive_rate_limits >= 3 {
            self.rate_limit_delay = (self.rate_limit_delay + base_delay) * 2;
        } else {
            self.rate_limit_delay = base_delay;
        }

        let delay = self.rate_limit_delay.min(Duration::from_secs(10));
        debug!(
            "Telegram stream: rate limited, sleeping {delay:?} (consecutive: {})",
            self.consecutive_rate_limits
        );
        tokio::time::sleep(delay).await;
    }

    /// Raw sendMessage call that returns the message_id.
    async fn send_message_raw(
        &self,
        text: &str,
        parse_mode: Option<&str>,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.token
        );

        let mut body = serde_json::json!({
            "chat_id": self.chat_id,
            "text": text,
        });
        if let Some(pm) = parse_mode {
            body["parse_mode"] = serde_json::Value::String(pm.to_string());
        }

        let resp = self.client.post(&url).json(&body).send().await?;
        let status = resp.status();
        let resp_body: serde_json::Value = resp.json().await.unwrap_or_default();

        if !status.is_success() {
            let desc = resp_body["description"].as_str().unwrap_or("unknown error");
            return Err(format!("sendMessage failed ({status}): {desc}").into());
        }

        let msg_id = resp_body["result"]["message_id"]
            .as_i64()
            .unwrap_or(0);
        Ok(msg_id)
    }

    /// Raw editMessageText call.
    async fn edit_message_raw(
        &self,
        message_id: i64,
        text: &str,
        parse_mode: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!(
            "https://api.telegram.org/bot{}/editMessageText",
            self.token
        );

        let mut body = serde_json::json!({
            "chat_id": self.chat_id,
            "message_id": message_id,
            "text": text,
        });
        if let Some(pm) = parse_mode {
            body["parse_mode"] = serde_json::Value::String(pm.to_string());
        }

        let resp = self.client.post(&url).json(&body).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            if status.as_u16() == 400 && body_text.contains("message is not modified") {
                return Ok(());
            }
            return Err(format!("{status}: {body_text}").into());
        }
        Ok(())
    }

    /// Send a draft message (Bot API 9.5 experimental).
    async fn send_draft(&self, text: &str) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessageDraft",
            self.token
        );

        let body = serde_json::json!({
            "chat_id": self.chat_id,
            "text": text,
        });

        let resp = self.client.post(&url).json(&body).send().await?;
        if !resp.status().is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(format!("sendMessageDraft failed: {body_text}").into());
        }
        Ok(())
    }
}

#[async_trait]
impl StreamSink for TelegramStreamSink {
    async fn push_text(&mut self, text: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.buffer.push_str(text);

        if self.should_flush() {
            self.flush().await?;
        }

        Ok(())
    }

    async fn push_tool_start(&mut self, tool_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Append a tool indicator to the buffer
        if !self.buffer.is_empty() && !self.buffer.ends_with('\n') {
            self.buffer.push('\n');
        }
        self.buffer
            .push_str(&format!("\n[Running: {tool_name}...]\n"));

        // Force flush to show tool activity
        self.flush().await?;
        Ok(())
    }

    async fn push_tool_result(
        &mut self,
        _tool_name: &str,
        _preview: &str,
        _is_error: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Tool results are typically not shown inline during streaming —
        // the LLM response after tool use will contain the relevant info.
        Ok(())
    }

    async fn finalize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.finalized {
            return Ok(());
        }
        self.finalized = true;

        if self.buffer.is_empty() {
            return Ok(());
        }

        match self.mode {
            StreamMode::Edit => {
                if let Some(msg_id) = self.message_id {
                    // Final edit with full Markdown→HTML formatting (no cursor)
                    let formatted =
                        formatter::format_for_channel(&self.buffer, openfang_types::config::OutputFormat::TelegramHtml);
                    let sanitized = sanitize_telegram_html(&formatted);

                    // If the final text is too long, we need to split
                    if sanitized.len() > 4096 {
                        let chunks = split_message(&sanitized, 4096);
                        // Edit existing message with first chunk
                        if let Some(first) = chunks.first() {
                            self.edit_message_raw(msg_id, first, Some("HTML"))
                                .await
                                .ok();
                        }
                        // Send remaining chunks as new messages
                        for chunk in &chunks[1..] {
                            self.send_message_raw(chunk, Some("HTML")).await.ok();
                        }
                    } else {
                        self.edit_message_raw(msg_id, &sanitized, Some("HTML"))
                            .await
                            .ok();
                    }
                } else {
                    // Never sent a first message — send the complete text
                    let formatted =
                        formatter::format_for_channel(&self.buffer, openfang_types::config::OutputFormat::TelegramHtml);
                    let sanitized = sanitize_telegram_html(&formatted);
                    self.send_message_raw(&sanitized, Some("HTML")).await.ok();
                }
            }
            StreamMode::Draft => {
                // Draft messages disappear — send a final real message
                let formatted =
                    formatter::format_for_channel(&self.buffer, openfang_types::config::OutputFormat::TelegramHtml);
                let sanitized = sanitize_telegram_html(&formatted);
                self.send_message_raw(&sanitized, Some("HTML")).await.ok();
            }
        }

        Ok(())
    }

    async fn abort(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.finalized {
            return Ok(());
        }
        // Best-effort: finalize whatever we have
        self.finalize().await
    }
}

/// Parse a Telegram update JSON into a `ChannelMessage`, or `None` if filtered/unparseable.
/// Handles both `message` and `edited_message` update types.
fn parse_telegram_update(
    update: &serde_json::Value,
    allowed_users: &[i64],
) -> Option<ChannelMessage> {
    let message = update
        .get("message")
        .or_else(|| update.get("edited_message"))?;
    let from = message.get("from")?;
    let user_id = from["id"].as_i64()?;

    // Security: check allowed_users
    if !allowed_users.is_empty() && !allowed_users.contains(&user_id) {
        debug!("Telegram: ignoring message from unlisted user {user_id}");
        return None;
    }

    let chat_id = message["chat"]["id"].as_i64()?;
    let first_name = from["first_name"].as_str().unwrap_or("Unknown");
    let last_name = from["last_name"].as_str().unwrap_or("");
    let display_name = if last_name.is_empty() {
        first_name.to_string()
    } else {
        format!("{first_name} {last_name}")
    };

    let chat_type = message["chat"]["type"].as_str().unwrap_or("private");
    let is_group = chat_type == "group" || chat_type == "supergroup";

    let text = message["text"].as_str()?;
    let message_id = message["message_id"].as_i64().unwrap_or(0);
    let timestamp = message["date"]
        .as_i64()
        .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
        .unwrap_or_else(chrono::Utc::now);

    // Parse bot commands (Telegram sends entities for /commands)
    let content = if let Some(entities) = message["entities"].as_array() {
        let is_bot_command = entities
            .iter()
            .any(|e| e["type"].as_str() == Some("bot_command") && e["offset"].as_i64() == Some(0));
        if is_bot_command {
            let parts: Vec<&str> = text.splitn(2, ' ').collect();
            let cmd_name = parts[0].trim_start_matches('/');
            // Strip @botname from command (e.g. /agents@mybot -> agents)
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
    };

    // Use chat_id as the platform_id (so responses go to the right chat)
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
        thread_id: None,
        metadata: HashMap::new(),
    })
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
        "b", "i", "u", "s", "em", "strong", "a", "code", "pre", "blockquote", "tg-spoiler",
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

    #[test]
    fn test_parse_telegram_update() {
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

        let msg = parse_telegram_update(&update, &[]).unwrap();
        assert_eq!(msg.channel, ChannelType::Telegram);
        assert_eq!(msg.sender.display_name, "Alice Smith");
        assert_eq!(msg.sender.platform_id, "111222333");
        assert!(matches!(msg.content, ChannelContent::Text(ref t) if t == "Hello, agent!"));
    }

    #[test]
    fn test_parse_telegram_command() {
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

        let msg = parse_telegram_update(&update, &[]).unwrap();
        match &msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "agent");
                assert_eq!(args, &["hello-world"]);
            }
            other => panic!("Expected Command, got {other:?}"),
        }
    }

    #[test]
    fn test_allowed_users_filter() {
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

        // Empty allowed_users = allow all
        let msg = parse_telegram_update(&update, &[]);
        assert!(msg.is_some());

        // Non-matching allowed_users = filter out
        let msg = parse_telegram_update(&update, &[111, 222]);
        assert!(msg.is_none());

        // Matching allowed_users = allow
        let msg = parse_telegram_update(&update, &[999]);
        assert!(msg.is_some());
    }

    #[test]
    fn test_parse_telegram_edited_message() {
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

        let msg = parse_telegram_update(&update, &[]).unwrap();
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

    #[test]
    fn test_parse_command_with_botname() {
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

        let msg = parse_telegram_update(&update, &[]).unwrap();
        match &msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "agents");
                assert!(args.is_empty());
            }
            other => panic!("Expected Command, got {other:?}"),
        }
    }
}
