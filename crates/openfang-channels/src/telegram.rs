//! Telegram Bot API adapter for the OpenFang channel bridge.
//!
//! Uses long-polling via `getUpdates` with exponential backoff on failures.
//! No external Telegram crate — just `reqwest` for full control over error handling.

use crate::supervisor::{self, SupervisorConfig};
use crate::types::{
    split_message, ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser,
};
use async_trait::async_trait;
use futures::Stream;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tracing::{debug, error, info, warn};
use zeroize::Zeroizing;

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
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.token.as_str()
        );

        // Telegram has a 4096 character limit per message — split if needed
        let chunks = split_message(text, 4096);
        for chunk in chunks {
            let body = serde_json::json!({
                "chat_id": chat_id,
                "text": chunk,
            });

            let resp = self.client.post(&url).json(&body).send().await?;
            let status = resp.status();
            if !status.is_success() {
                let body_text = resp.text().await.unwrap_or_default();
                warn!("Telegram sendMessage failed ({status}): {body_text}");
            }
        }
        Ok(())
    }

    /// Send a file via `sendDocument` on the Telegram API.
    async fn api_send_document(
        &self,
        chat_id: i64,
        file_url: &str,
        filename: &str,
        caption: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendDocument",
            self.token.as_str()
        );

        // Download the file first, then upload via multipart
        let file_bytes = crate::media_utils::download_url(&self.client, file_url)
            .await
            .ok_or("Failed to download file for sending")?;

        let part = reqwest::multipart::Part::bytes(file_bytes.to_vec())
            .file_name(filename.to_string())
            .mime_str("application/octet-stream")?;

        let mut form = reqwest::multipart::Form::new()
            .text("chat_id", chat_id.to_string())
            .part("document", part);

        if let Some(cap) = caption {
            form = form.text("caption", cap.to_string());
        }

        let resp = self.client.post(&url).multipart(form).send().await?;
        if !resp.status().is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            warn!("Telegram sendDocument failed: {body_text}");
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

        let (tx, rx) = mpsc::channel::<ChannelMessage>(supervisor::DEFAULT_CHANNEL_BUFFER);

        let token = self.token.clone();
        let client = self.client.clone();
        let allowed_users = self.allowed_users.clone();
        let poll_interval = self.poll_interval;
        let shutdown = self.shutdown_rx.clone();
        let offset = Arc::new(tokio::sync::RwLock::new(None::<i64>));

        tokio::spawn(async move {
            let config = SupervisorConfig::new("Telegram");

            supervisor::run_supervised_loop_reset_on_connect(
                config,
                shutdown.clone(),
                || {
                    let token = token.clone();
                    let client = client.clone();
                    let allowed_users = allowed_users.clone();
                    let tx = tx.clone();
                    let shutdown = shutdown.clone();
                    let offset = offset.clone();
                    async move {
                        poll_telegram_updates(
                            &token, &client, &allowed_users, poll_interval,
                            &offset, &tx, &mut shutdown.clone(),
                        ).await
                    }
                },
            ).await;
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
            ChannelContent::File { url, filename } => {
                self.api_send_document(chat_id, &url, &filename, None)
                    .await?;
            }
            ChannelContent::Image { url, caption } => {
                self.api_send_document(chat_id, &url, "image.jpg", caption.as_deref())
                    .await?;
            }
            _ => {
                self.api_send_message(chat_id, "(Unsupported content type)")
                    .await?;
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
}

/// Run the Telegram long-polling loop until an error or shutdown occurs.
///
/// Returns `Ok(false)` on permanent stop (e.g. 409 Conflict), `Ok(true)` on
/// recoverable disconnect, or `Err` on failure.
async fn poll_telegram_updates(
    token: &Zeroizing<String>,
    client: &reqwest::Client,
    allowed_users: &[i64],
    poll_interval: Duration,
    offset: &Arc<tokio::sync::RwLock<Option<i64>>>,
    tx: &mpsc::Sender<ChannelMessage>,
    shutdown: &mut watch::Receiver<bool>,
) -> Result<bool, String> {
    loop {
        if *shutdown.borrow() {
            return Ok(false);
        }

        let url = format!("https://api.telegram.org/bot{}/getUpdates", token.as_str());
        let mut params = serde_json::json!({
            "timeout": LONG_POLL_TIMEOUT,
            "allowed_updates": ["message", "edited_message"],
        });
        if let Some(off) = *offset.read().await {
            params["offset"] = serde_json::json!(off);
        }

        let request_timeout = Duration::from_secs(LONG_POLL_TIMEOUT + 10);
        let result = tokio::select! {
            res = async {
                client.get(&url).json(&params).timeout(request_timeout).send().await
            } => res,
            _ = shutdown.changed() => {
                return Ok(false);
            }
        };

        let resp = match result {
            Ok(resp) => resp,
            Err(e) => {
                return Err(format!("Telegram getUpdates network error: {e}"));
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

        // Handle conflict (another bot instance polling) - permanent stop
        if status.as_u16() == 409 {
            error!("Telegram 409 Conflict — another bot instance is running. Stopping.");
            return Ok(false);
        }

        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(format!("Telegram getUpdates failed ({status}): {body_text}"));
        }

        let body: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                return Err(format!("Telegram getUpdates parse error: {e}"));
            }
        };

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
            if let Some(update_id) = update["update_id"].as_i64() {
                *offset.write().await = Some(update_id + 1);
            }

            let msg = match parse_telegram_update(update, allowed_users) {
                Some(m) => m,
                None => {
                    if let Some(m) = try_handle_voice(update, allowed_users, client, token).await {
                        m
                    } else if let Some(m) = try_handle_photo(update, allowed_users, client, token).await {
                        m
                    } else if let Some(m) = try_handle_document(update, allowed_users, client, token).await {
                        m
                    } else {
                        continue;
                    }
                }
            };

            debug!(
                "Telegram message from {}: {:?}",
                msg.sender.display_name, msg.content
            );

            // Send ack reaction (non-blocking)
            {
                let ack_url = format!(
                    "https://api.telegram.org/bot{}/setMessageReaction",
                    token.as_str()
                );
                let ack_body = serde_json::json!({
                    "chat_id": msg.sender.platform_id,
                    "message_id": msg.platform_message_id.parse::<i64>().unwrap_or(0),
                    "reaction": [{"type": "emoji", "emoji": "\u{1F440}"}]
                });
                let ack_client = client.clone();
                tokio::spawn(async move {
                    match ack_client.post(&ack_url).json(&ack_body).send().await {
                        Ok(resp) if !resp.status().is_success() => {
                            debug!("Telegram ack reaction failed: {}", resp.status());
                        }
                        Err(e) => debug!("Telegram ack reaction error: {e}"),
                        _ => {}
                    }
                });
            }

            if tx.send(msg).await.is_err() {
                return Ok(false);
            }
        }

        tokio::time::sleep(poll_interval).await;
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

/// Handle voice messages from Telegram by downloading and transcribing with Groq Whisper.
///
/// Returns a `ChannelMessage` with the transcription as text, or `None` if not a voice message
/// or transcription fails.
async fn try_handle_voice(
    update: &serde_json::Value,
    allowed_users: &[i64],
    client: &reqwest::Client,
    token: &str,
) -> Option<ChannelMessage> {
    let message = update.get("message")?;
    let voice = message.get("voice")?;
    let from = message.get("from")?;
    let user_id = from["id"].as_i64()?;

    if !allowed_users.is_empty() && !allowed_users.contains(&user_id) {
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
    let message_id = message["message_id"].as_i64().unwrap_or(0);
    let timestamp = message["date"]
        .as_i64()
        .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
        .unwrap_or_else(chrono::Utc::now);

    let file_id = voice["file_id"].as_str()?;
    info!("Telegram: voice message from {display_name}, downloading...");

    // Step 1: Get file path via Telegram getFile API
    let get_file_url = format!("https://api.telegram.org/bot{token}/getFile?file_id={file_id}");
    let file_resp: serde_json::Value = client.get(&get_file_url).send().await.ok()?.json().await.ok()?;
    let file_path = file_resp["result"]["file_path"].as_str()?;

    // Step 2: Download the audio file
    let download_url = format!("https://api.telegram.org/file/bot{token}/{file_path}");
    let audio_bytes = client.get(&download_url).send().await.ok()?.bytes().await.ok()?;

    // Step 3: Transcribe with Groq Whisper (or OpenAI Whisper as fallback)
    let transcription = transcribe_audio(client, &audio_bytes, file_path).await;
    let text = match transcription {
        Some(t) if !t.is_empty() => t,
        _ => {
            warn!("Telegram: voice transcription failed for {display_name}");
            "[Voice message received, but transcription failed]".to_string()
        }
    };

    info!("Telegram: voice transcribed for {display_name}: {}", openfang_types::truncate_str(&text, 100));

    Some(ChannelMessage {
        channel: ChannelType::Telegram,
        platform_message_id: message_id.to_string(),
        sender: ChannelUser {
            platform_id: chat_id.to_string(),
            display_name,
            openfang_user: None,
        },
        content: ChannelContent::Text(text),
        target_agent: None,
        timestamp,
        is_group,
        thread_id: None,
        metadata: HashMap::new(),
    })
}

/// Transcribe audio bytes using Groq Whisper or OpenAI Whisper.
async fn transcribe_audio(
    client: &reqwest::Client,
    audio_bytes: &[u8],
    filename: &str,
) -> Option<String> {
    // Determine MIME type and normalize filename for API compatibility.
    // Telegram uses .oga (Ogg Opus) which Groq doesn't recognize — rename to .ogg.
    let (mime, upload_filename) = if filename.ends_with(".oga") || filename.ends_with(".ogg") {
        ("audio/ogg", filename.replace(".oga", ".ogg"))
    } else if filename.ends_with(".mp3") {
        ("audio/mpeg", filename.to_string())
    } else if filename.ends_with(".wav") {
        ("audio/wav", filename.to_string())
    } else if filename.ends_with(".m4a") {
        ("audio/mp4", filename.to_string())
    } else {
        ("audio/ogg", format!("{filename}.ogg"))
    };

    // Try Groq Whisper first (fast, free tier)
    if let Ok(groq_key) = std::env::var("GROQ_API_KEY") {
        let form = reqwest::multipart::Form::new()
            .part(
                "file",
                reqwest::multipart::Part::bytes(audio_bytes.to_vec())
                    .file_name(upload_filename.clone())
                    .mime_str(mime)
                    .ok()?,
            )
            .text("model", "whisper-large-v3-turbo")
            .text("response_format", "json");

        match client
            .post("https://api.groq.com/openai/v1/audio/transcriptions")
            .bearer_auth(&groq_key)
            .multipart(form)
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status();
                match resp.json::<serde_json::Value>().await {
                    Ok(result) => {
                        if let Some(text) = result["text"].as_str() {
                            return Some(text.to_string());
                        }
                        warn!("Groq Whisper: no 'text' in response (status={status}): {result}");
                    }
                    Err(e) => {
                        warn!("Groq Whisper: failed to parse response (status={status}): {e}");
                    }
                }
            }
            Err(e) => {
                warn!("Groq Whisper: request failed: {e}");
            }
        }
    } else {
        warn!("GROQ_API_KEY not set, skipping Groq Whisper");
    }

    // Fallback: OpenAI Whisper
    if let Ok(openai_key) = std::env::var("OPENAI_API_KEY") {
        let form = reqwest::multipart::Form::new()
            .part(
                "file",
                reqwest::multipart::Part::bytes(audio_bytes.to_vec())
                    .file_name(upload_filename.clone())
                    .mime_str(mime)
                    .ok()?,
            )
            .text("model", "whisper-1")
            .text("response_format", "json");

        match client
            .post("https://api.openai.com/v1/audio/transcriptions")
            .bearer_auth(&openai_key)
            .multipart(form)
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status();
                match resp.json::<serde_json::Value>().await {
                    Ok(result) => {
                        if let Some(text) = result["text"].as_str() {
                            return Some(text.to_string());
                        }
                        warn!("OpenAI Whisper: no 'text' in response (status={status}): {result}");
                    }
                    Err(e) => {
                        warn!("OpenAI Whisper: failed to parse response (status={status}): {e}");
                    }
                }
            }
            Err(e) => {
                warn!("OpenAI Whisper: request failed: {e}");
            }
        }
    }

    warn!("Voice transcription: all providers failed for file '{filename}' ({} bytes, mime={mime})", audio_bytes.len());
    None
}

/// Handle photo messages from Telegram by downloading and recognizing with Gemini Vision.
///
/// Picks the largest available photo size. Captions are used as the recognition prompt.
/// Returns a `ChannelMessage` with the recognition result as text.
async fn try_handle_photo(
    update: &serde_json::Value,
    allowed_users: &[i64],
    client: &reqwest::Client,
    token: &str,
) -> Option<ChannelMessage> {
    let message = update.get("message")?;
    let photos = message.get("photo")?.as_array()?;
    if photos.is_empty() {
        return None;
    }
    let from = message.get("from")?;
    let user_id = from["id"].as_i64()?;

    if !allowed_users.is_empty() && !allowed_users.contains(&user_id) {
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
    let message_id = message["message_id"].as_i64().unwrap_or(0);
    let timestamp = message["date"]
        .as_i64()
        .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
        .unwrap_or_else(chrono::Utc::now);

    // Pick the largest photo (last in the array — Telegram sorts by size ascending)
    let photo = photos.last()?;
    let file_id = photo["file_id"].as_str()?;
    let caption = message["caption"].as_str().unwrap_or("");

    info!("Telegram: photo from {display_name}, downloading...");

    // Download via Telegram getFile API
    let get_file_url = format!("https://api.telegram.org/bot{token}/getFile?file_id={file_id}");
    let file_resp: serde_json::Value =
        client.get(&get_file_url).send().await.ok()?.json().await.ok()?;
    let file_path = file_resp["result"]["file_path"].as_str()?;
    let download_url = format!("https://api.telegram.org/file/bot{token}/{file_path}");
    let image_bytes = client.get(&download_url).send().await.ok()?.bytes().await.ok()?;

    // Recognize with Gemini Vision
    let recognition = crate::media_utils::recognize_image_gemini(client, &image_bytes, caption).await;

    let text = if caption.is_empty() {
        format!("[用户发送了一张图片]\n图片内容: {recognition}")
    } else {
        format!("[用户发送了一张图片并说: {caption}]\n图片内容: {recognition}")
    };

    info!(
        "Telegram: photo recognized for {display_name}: {}",
        openfang_types::truncate_str(&text, 100)
    );

    Some(ChannelMessage {
        channel: ChannelType::Telegram,
        platform_message_id: message_id.to_string(),
        sender: ChannelUser {
            platform_id: chat_id.to_string(),
            display_name,
            openfang_user: None,
        },
        content: ChannelContent::Text(text),
        target_agent: None,
        timestamp,
        is_group,
        thread_id: None,
        metadata: HashMap::new(),
    })
}

/// Handle document/file messages from Telegram.
///
/// Downloads the file and:
/// - If image: recognizes with Gemini Vision
/// - If text file (<50KB): extracts content
/// - Otherwise: reports filename, size, and MIME type
async fn try_handle_document(
    update: &serde_json::Value,
    allowed_users: &[i64],
    client: &reqwest::Client,
    token: &str,
) -> Option<ChannelMessage> {
    let message = update.get("message")?;
    let document = message.get("document")?;
    let from = message.get("from")?;
    let user_id = from["id"].as_i64()?;

    if !allowed_users.is_empty() && !allowed_users.contains(&user_id) {
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
    let message_id = message["message_id"].as_i64().unwrap_or(0);
    let timestamp = message["date"]
        .as_i64()
        .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
        .unwrap_or_else(chrono::Utc::now);

    let file_name = document["file_name"].as_str().unwrap_or("unknown_file");
    let file_size = document["file_size"].as_u64().unwrap_or(0);
    let mime_type = document["mime_type"]
        .as_str()
        .unwrap_or("application/octet-stream");
    let file_id = document["file_id"].as_str()?;
    let caption = message["caption"].as_str().unwrap_or("");

    info!("Telegram: document '{file_name}' ({file_size} bytes) from {display_name}");

    // Download via Telegram getFile API (bots can download files up to 20MB)
    let get_file_url = format!("https://api.telegram.org/bot{token}/getFile?file_id={file_id}");
    let file_resp: serde_json::Value =
        client.get(&get_file_url).send().await.ok()?.json().await.ok()?;
    let file_path = file_resp["result"]["file_path"].as_str()?;
    let download_url = format!("https://api.telegram.org/file/bot{token}/{file_path}");
    let file_bytes = client.get(&download_url).send().await.ok()?.bytes().await.ok()?;

    let mut text =
        format!("[用户发送了文件: {file_name} (大小: {file_size} 字节, 类型: {mime_type})]");

    // If it's an image sent as document (uncompressed), use Gemini Vision
    if mime_type.starts_with("image/") {
        let recognition = crate::media_utils::recognize_image_gemini(client, &file_bytes, caption).await;
        text.push_str(&format!("\n图片内容: {recognition}"));
    }
    // If it's a text-like file and small enough, extract content
    else if crate::media_utils::is_text_file(file_name, mime_type) && file_bytes.len() < 50_000 {
        match std::str::from_utf8(&file_bytes) {
            Ok(content) => {
                let truncated = openfang_types::truncate_str(content, 3000);
                text.push_str(&format!("\n文件内容:\n```\n{truncated}\n```"));
                if content.len() > 3000 {
                    text.push_str(&format!("\n... (共 {} 字符，已截断)", content.len()));
                }
            }
            Err(_) => {
                text.push_str("\n[二进制文件，无法显示文本内容]");
            }
        }
    }

    if !caption.is_empty() {
        text.push_str(&format!("\n用户附言: {caption}"));
    }

    info!(
        "Telegram: document processed for {display_name}: {}",
        openfang_types::truncate_str(&text, 100)
    );

    Some(ChannelMessage {
        channel: ChannelType::Telegram,
        platform_message_id: message_id.to_string(),
        sender: ChannelUser {
            platform_id: chat_id.to_string(),
            display_name,
            openfang_user: None,
        },
        content: ChannelContent::Text(text),
        target_agent: None,
        timestamp,
        is_group,
        thread_id: None,
        metadata: HashMap::new(),
    })
}

/// Calculate exponential backoff capped at the default maximum.
pub fn calculate_backoff(current: Duration) -> Duration {
    supervisor::next_backoff(current, supervisor::DEFAULT_MAX_BACKOFF)
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
