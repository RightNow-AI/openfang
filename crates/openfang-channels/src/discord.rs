//! Discord Gateway adapter for the OpenFang channel bridge.
//!
//! Uses Discord Gateway WebSocket (v10) for receiving messages and the REST API
//! for sending responses. No external Discord crate — just `tokio-tungstenite` + `reqwest`.

use crate::types::{
    split_message, ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser,
};
use async_trait::async_trait;
use futures::{SinkExt, Stream, StreamExt};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch, Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};
use zeroize::Zeroizing;

const DISCORD_API_BASE: &str = "https://discord.com/api/v10";
const MAX_BACKOFF: Duration = Duration::from_secs(60);
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const DISCORD_MSG_LIMIT: usize = 2000;
/// Multipart field name Discord requires for the first attachment payload.
/// Centralized so a `file[0]` typo at the call site can't slip past review,
/// and so the unit test below pins the exact wire format.
const ATTACHMENT_FIELD_NAME: &str = "files[0]";
/// Floor on the rate-limit retry delay. Discord occasionally returns
/// `retry_after: 0` (or a missing header), which would busy-loop the retry.
const RETRY_AFTER_FLOOR_SECS: f64 = 0.05;
/// Cap on the rate-limit retry delay so a misbehaving response can't park us
/// for a long time on a one-shot retry.
const RETRY_AFTER_CEIL_SECS: f64 = 30.0;
/// Per-request timeout for outbound URL fetches (File/Image arms). Matches the
/// adapter's existing 15s budget for other REST calls so a slow remote can't
/// stall the send pipeline.
const URL_FETCH_TIMEOUT: Duration = Duration::from_secs(15);
/// Hard cap on the size of a fetched URL body, both via Content-Length pre-flight
/// and via streamed accumulation. Discord itself caps non-Nitro uploads at
/// 25 MiB; matching that here means we reject before paying for bytes Discord
/// would refuse anyway.
const URL_FETCH_MAX_BYTES: usize = 25 * 1024 * 1024;

/// Discord Gateway opcodes.
mod opcode {
    pub const DISPATCH: u64 = 0;
    pub const HEARTBEAT: u64 = 1;
    pub const IDENTIFY: u64 = 2;
    pub const RESUME: u64 = 6;
    pub const RECONNECT: u64 = 7;
    pub const INVALID_SESSION: u64 = 9;
    pub const HELLO: u64 = 10;
    pub const HEARTBEAT_ACK: u64 = 11;
}

/// Build a Discord gateway heartbeat (opcode 1) payload.
///
/// Per the Discord gateway spec, the payload `d` field is the last received
/// dispatch sequence number, or `null` if no dispatch has been received yet.
/// See: <https://discord.com/developers/docs/topics/gateway#sending-heartbeats>
fn build_heartbeat_payload(last_sequence: Option<u64>) -> serde_json::Value {
    serde_json::json!({
        "op": opcode::HEARTBEAT,
        "d": last_sequence,
    })
}

/// Discord Gateway adapter using WebSocket.
pub struct DiscordAdapter {
    /// SECURITY: Bot token is zeroized on drop to prevent memory disclosure.
    token: Zeroizing<String>,
    client: reqwest::Client,
    allowed_guilds: Vec<String>,
    allowed_users: Vec<String>,
    ignore_bots: bool,
    intents: u64,
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Bot's own user ID (populated after READY event).
    bot_user_id: Arc<RwLock<Option<String>>>,
    /// Session ID for resume (populated after READY event).
    session_id: Arc<RwLock<Option<String>>>,
    /// Resume gateway URL.
    resume_gateway_url: Arc<RwLock<Option<String>>>,
}

impl DiscordAdapter {
    pub fn new(
        token: String,
        allowed_guilds: Vec<String>,
        allowed_users: Vec<String>,
        ignore_bots: bool,
        intents: u64,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            token: Zeroizing::new(token),
            client: reqwest::Client::new(),
            allowed_guilds,
            allowed_users,
            ignore_bots,
            intents,
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            bot_user_id: Arc::new(RwLock::new(None)),
            session_id: Arc::new(RwLock::new(None)),
            resume_gateway_url: Arc::new(RwLock::new(None)),
        }
    }

    /// Get the WebSocket gateway URL from the Discord API.
    async fn get_gateway_url(&self) -> Result<String, Box<dyn std::error::Error>> {
        let url = format!("{DISCORD_API_BASE}/gateway/bot");
        let resp: serde_json::Value = self
            .client
            .get(&url)
            .header("Authorization", format!("Bot {}", self.token.as_str()))
            .send()
            .await?
            .json()
            .await?;

        let ws_url = resp["url"]
            .as_str()
            .ok_or("Missing 'url' in gateway response")?;

        Ok(format!("{ws_url}/?v=10&encoding=json"))
    }

    /// Send a message to a Discord channel via REST API.
    async fn api_send_message(
        &self,
        channel_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{DISCORD_API_BASE}/channels/{channel_id}/messages");
        let chunks = split_message(text, DISCORD_MSG_LIMIT);

        for chunk in chunks {
            let body = serde_json::json!({ "content": chunk });
            let resp = self
                .client
                .post(&url)
                .header("Authorization", format!("Bot {}", self.token.as_str()))
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let body_text = resp.text().await.unwrap_or_default();
                warn!("Discord sendMessage failed: {body_text}");
            }
        }
        Ok(())
    }

    /// Send a file attachment to a Discord channel via REST multipart upload.
    ///
    /// Mirrors the proven `api_send_document_upload` pattern from the Telegram
    /// adapter. Uses `multipart/form-data` with two fields: `payload_json` (the
    /// usual message body) and `files[0]` (raw bytes). `reqwest` sets the
    /// multipart `Content-Type` boundary; we only set `Authorization`.
    ///
    /// On HTTP 429 we honor `Retry-After` once before giving up. Higher-tier
    /// rate-limit handling can land later if needed.
    async fn api_send_attachment(
        &self,
        channel_id: &str,
        data: impl Into<bytes::Bytes>,
        filename: &str,
        mime_type: &str,
        caption: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{DISCORD_API_BASE}/channels/{channel_id}/messages");

        // Discord caps message content at DISCORD_MSG_LIMIT chars; truncate
        // explicitly so a long caption doesn't silently 400.
        let payload_json = build_attachment_payload_json(caption);

        // Accept anything that becomes `Bytes` cheaply. `Bytes::clone` is a
        // refcount bump, so retry-path form builds don't reallocate the
        // payload. Callers passing `Vec<u8>` move ownership; callers passing
        // a `Bytes` from `download_url_to_bytes` reuse the same allocation.
        let bytes: bytes::Bytes = data.into();
        let body_len = bytes.len() as u64;
        let build_form = || -> Result<reqwest::multipart::Form, Box<dyn std::error::Error>> {
            let body = reqwest::Body::from(bytes.clone());
            let file_part = reqwest::multipart::Part::stream_with_length(body, body_len)
                .file_name(filename.to_string())
                .mime_str(mime_type)?;
            Ok(reqwest::multipart::Form::new()
                .text("payload_json", payload_json.clone())
                .part(ATTACHMENT_FIELD_NAME, file_part))
        };

        let mut attempts = 0u8;
        loop {
            attempts += 1;
            let form = build_form()?;
            let resp = self
                .client
                .post(&url)
                .header("Authorization", format!("Bot {}", self.token.as_str()))
                .multipart(form)
                .send()
                .await?;

            let status = resp.status();
            if status.is_success() {
                return Ok(());
            }

            // Honor Retry-After once on 429. Discord puts the canonical
            // `retry_after` in the JSON body; the HTTP header is a fallback
            // (and is sometimes absent on per-route limits).
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS && attempts == 1 {
                let header_secs = resp
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<f64>().ok());
                let body_text = resp.text().await.unwrap_or_default();
                let body_secs = serde_json::from_str::<serde_json::Value>(&body_text)
                    .ok()
                    .and_then(|v| v.get("retry_after").and_then(|r| r.as_f64()));
                let retry_after_secs = body_secs
                    .or(header_secs)
                    .unwrap_or(1.0)
                    .clamp(RETRY_AFTER_FLOOR_SECS, RETRY_AFTER_CEIL_SECS);
                warn!(
                    "Discord sendAttachment rate-limited; retrying after {retry_after_secs:.2}s"
                );
                tokio::time::sleep(Duration::from_millis(
                    (retry_after_secs * 1000.0) as u64,
                ))
                .await;
                continue;
            }

            let body_text = resp.text().await.unwrap_or_default();
            warn!("Discord sendAttachment failed ({status}): {body_text}");
            return Err(
                format!("Discord sendAttachment failed ({status}): {body_text}").into(),
            );
        }
    }

    /// Fetch a URL into memory with a 15s timeout and a 25 MiB cap.
    ///
    /// Two-stage size enforcement:
    ///   1. If the response carries `Content-Length` and it exceeds the cap,
    ///      reject before reading the body.
    ///   2. Stream `chunk()`-by-`chunk()` and abort once accumulated bytes
    ///      exceed the cap (covers servers that omit/lie about Content-Length).
    ///
    /// Returns the body as `Bytes` (so the multipart helper can refcount-clone
    /// instead of memcpy on retry) plus the response's `Content-Type` with any
    /// parameters (e.g. `; charset=utf-8`) stripped.
    async fn download_url_to_bytes(
        &self,
        url: &str,
    ) -> Result<(bytes::Bytes, Option<String>), Box<dyn std::error::Error>> {
        let resp = self
            .client
            .get(url)
            .timeout(URL_FETCH_TIMEOUT)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            // Read up to 512B of the body for diagnostics; ignore errors.
            let snippet: String = resp
                .text()
                .await
                .unwrap_or_default()
                .chars()
                .take(512)
                .collect();
            return Err(format!(
                "Discord download_url_to_bytes failed ({status}) for {url}: {snippet}"
            )
            .into());
        }

        // Pre-flight: trust Content-Length when present so we can fail fast
        // without buffering 26 MiB before erroring.
        if let Some(len) = resp.content_length() {
            if len as usize > URL_FETCH_MAX_BYTES {
                return Err(format!(
                    "Discord download_url_to_bytes: Content-Length {len} exceeds cap {URL_FETCH_MAX_BYTES} for {url}"
                )
                .into());
            }
        }

        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(strip_mime_params)
            .filter(|s| !s.is_empty());

        // Stream chunks so we can abort if the server lied about/omitted
        // Content-Length and tries to push past the cap. `chunk()` returns
        // `Bytes`; we accumulate into a single `BytesMut` and freeze at the end.
        let mut buf = bytes::BytesMut::new();
        let mut resp = resp;
        while let Some(chunk) = resp.chunk().await? {
            if buf.len() + chunk.len() > URL_FETCH_MAX_BYTES {
                return Err(format!(
                    "Discord download_url_to_bytes: streamed body exceeds cap {URL_FETCH_MAX_BYTES} for {url}"
                )
                .into());
            }
            buf.extend_from_slice(&chunk);
        }

        Ok((buf.freeze(), content_type))
    }

    /// Send typing indicator to a Discord channel.
    async fn api_send_typing(&self, channel_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{DISCORD_API_BASE}/channels/{channel_id}/typing");
        let _ = self
            .client
            .post(&url)
            .header("Authorization", format!("Bot {}", self.token.as_str()))
            .send()
            .await?;
        Ok(())
    }
}

#[async_trait]
impl ChannelAdapter for DiscordAdapter {
    fn name(&self) -> &str {
        "discord"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Discord
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        let gateway_url = self.get_gateway_url().await?;
        info!("Discord gateway URL obtained");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);

        let token = self.token.clone();
        let intents = self.intents;
        let allowed_guilds = self.allowed_guilds.clone();
        let allowed_users = self.allowed_users.clone();
        let ignore_bots = self.ignore_bots;
        let bot_user_id = self.bot_user_id.clone();
        let session_id_store = self.session_id.clone();
        let resume_url_store = self.resume_gateway_url.clone();
        let mut shutdown = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let mut backoff = INITIAL_BACKOFF;
            let mut connect_url = gateway_url;
            // Sequence persists across reconnections for RESUME
            let sequence: Arc<RwLock<Option<u64>>> = Arc::new(RwLock::new(None));

            loop {
                if *shutdown.borrow() {
                    break;
                }

                info!("Connecting to Discord gateway...");

                let ws_result = tokio_tungstenite::connect_async(&connect_url).await;
                let ws_stream = match ws_result {
                    Ok((stream, _)) => stream,
                    Err(e) => {
                        warn!("Discord gateway connection failed: {e}, retrying in {backoff:?}");
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(MAX_BACKOFF);
                        continue;
                    }
                };

                backoff = INITIAL_BACKOFF;
                info!("Discord gateway connected");

                let (ws_tx_raw, mut ws_rx) = ws_stream.split();
                // Wrap the sink so the periodic heartbeat task and the inner
                // loop can both write to it.
                let ws_tx = Arc::new(Mutex::new(ws_tx_raw));
                let mut heartbeat_handle: Option<JoinHandle<()>> = None;
                // Tracks whether the most recent heartbeat we sent has been
                // ACKed (opcode 11). Initialized to `true` so the first
                // heartbeat is always allowed to fire.
                let heartbeat_acked = Arc::new(AtomicBool::new(true));

                // Inner message loop — returns true if we should reconnect
                let should_reconnect = 'inner: loop {
                    let msg = tokio::select! {
                        msg = ws_rx.next() => msg,
                        _ = shutdown.changed() => {
                            if *shutdown.borrow() {
                                info!("Discord shutdown requested");
                                if let Some(h) = heartbeat_handle.take() {
                                    h.abort();
                                }
                                let _ = ws_tx.lock().await.close().await;
                                return;
                            }
                            continue;
                        }
                    };

                    let msg = match msg {
                        Some(Ok(m)) => m,
                        Some(Err(e)) => {
                            warn!("Discord WebSocket error: {e}");
                            break 'inner true;
                        }
                        None => {
                            info!("Discord WebSocket closed");
                            break 'inner true;
                        }
                    };

                    let text = match msg {
                        tokio_tungstenite::tungstenite::Message::Text(t) => t,
                        tokio_tungstenite::tungstenite::Message::Close(_) => {
                            info!("Discord gateway closed by server");
                            break 'inner true;
                        }
                        _ => continue,
                    };

                    let payload: serde_json::Value = match serde_json::from_str(&text) {
                        Ok(v) => v,
                        Err(e) => {
                            warn!("Discord: failed to parse gateway message: {e}");
                            continue;
                        }
                    };

                    let op = payload["op"].as_u64().unwrap_or(999);

                    // Update sequence number from any payload that carries one
                    // (typically dispatch events, opcode 0).
                    if let Some(s) = payload["s"].as_u64() {
                        *sequence.write().await = Some(s);
                    }

                    match op {
                        opcode::HELLO => {
                            let interval =
                                payload["d"]["heartbeat_interval"].as_u64().unwrap_or(45000);
                            debug!("Discord HELLO: heartbeat_interval={interval}ms");

                            // Spawn the periodic heartbeat task BEFORE we send
                            // IDENTIFY/RESUME, per the Discord gateway flow.
                            // Abort any stale handle from a previous attempt
                            // first (defensive — should normally be None here).
                            if let Some(h) = heartbeat_handle.take() {
                                h.abort();
                            }
                            heartbeat_acked.store(true, Ordering::Relaxed);
                            let hb_sink = ws_tx.clone();
                            let hb_seq = sequence.clone();
                            let hb_acked = heartbeat_acked.clone();
                            let mut hb_shutdown = shutdown.clone();
                            heartbeat_handle = Some(tokio::spawn(async move {
                                let mut ticker =
                                    tokio::time::interval(Duration::from_millis(interval));
                                // Skip the immediate first tick — we want to
                                // wait one full interval before the first beat.
                                ticker.tick().await;
                                loop {
                                    tokio::select! {
                                        _ = ticker.tick() => {}
                                        _ = hb_shutdown.changed() => {
                                            if *hb_shutdown.borrow() {
                                                return;
                                            }
                                            continue;
                                        }
                                    }

                                    // If the previous heartbeat was never
                                    // ACKed, the connection is zombied — close
                                    // the sink so the read loop sees EOF and
                                    // triggers a reconnect (Discord spec).
                                    if !hb_acked.swap(false, Ordering::Relaxed) {
                                        warn!(
                                            "Discord: previous heartbeat not ACKed, \
                                             forcing reconnect"
                                        );
                                        let _ = hb_sink.lock().await.close().await;
                                        return;
                                    }

                                    let seq = *hb_seq.read().await;
                                    let payload = build_heartbeat_payload(seq);
                                    let text = match serde_json::to_string(&payload) {
                                        Ok(s) => s,
                                        Err(e) => {
                                            error!("Discord: failed to serialize heartbeat: {e}");
                                            return;
                                        }
                                    };
                                    let send_res = hb_sink
                                        .lock()
                                        .await
                                        .send(tokio_tungstenite::tungstenite::Message::Text(text))
                                        .await;
                                    if let Err(e) = send_res {
                                        warn!("Discord: failed to send heartbeat: {e}");
                                        return;
                                    }
                                    debug!("Discord heartbeat sent (seq={:?})", seq);
                                }
                            }));

                            // Try RESUME if we have a session, otherwise IDENTIFY
                            let has_session = session_id_store.read().await.is_some();
                            let has_seq = sequence.read().await.is_some();

                            let gateway_msg = if has_session && has_seq {
                                let sid = session_id_store.read().await.clone().unwrap();
                                let seq = *sequence.read().await;
                                info!("Discord: sending RESUME (session={sid})");
                                serde_json::json!({
                                    "op": opcode::RESUME,
                                    "d": {
                                        "token": token.as_str(),
                                        "session_id": sid,
                                        "seq": seq
                                    }
                                })
                            } else {
                                info!("Discord: sending IDENTIFY");
                                serde_json::json!({
                                    "op": opcode::IDENTIFY,
                                    "d": {
                                        "token": token.as_str(),
                                        "intents": intents,
                                        "properties": {
                                            "os": "linux",
                                            "browser": "openfang",
                                            "device": "openfang"
                                        }
                                    }
                                })
                            };

                            if let Err(e) = ws_tx
                                .lock()
                                .await
                                .send(tokio_tungstenite::tungstenite::Message::Text(
                                    serde_json::to_string(&gateway_msg).unwrap(),
                                ))
                                .await
                            {
                                error!("Discord: failed to send IDENTIFY/RESUME: {e}");
                                break 'inner true;
                            }
                        }

                        opcode::DISPATCH => {
                            let event_name = payload["t"].as_str().unwrap_or("");
                            let d = &payload["d"];

                            match event_name {
                                "READY" => {
                                    let user_id =
                                        d["user"]["id"].as_str().unwrap_or("").to_string();
                                    let username =
                                        d["user"]["username"].as_str().unwrap_or("unknown");
                                    let sid = d["session_id"].as_str().unwrap_or("").to_string();
                                    let resume_url =
                                        d["resume_gateway_url"].as_str().unwrap_or("").to_string();

                                    *bot_user_id.write().await = Some(user_id.clone());
                                    *session_id_store.write().await = Some(sid);
                                    if !resume_url.is_empty() {
                                        *resume_url_store.write().await = Some(resume_url);
                                    }

                                    info!("Discord bot ready: {username} ({user_id})");
                                }

                                "MESSAGE_CREATE" | "MESSAGE_UPDATE" => {
                                    if let Some(msg) = parse_discord_message(
                                        d,
                                        &bot_user_id,
                                        &allowed_guilds,
                                        &allowed_users,
                                        ignore_bots,
                                    )
                                    .await
                                    {
                                        debug!(
                                            "Discord {event_name} from {}: {:?}",
                                            msg.sender.display_name, msg.content
                                        );
                                        if tx.send(msg).await.is_err() {
                                            return;
                                        }
                                    }
                                }

                                "RESUMED" => {
                                    info!("Discord session resumed successfully");
                                }

                                _ => {
                                    debug!("Discord event: {event_name}");
                                }
                            }
                        }

                        opcode::HEARTBEAT => {
                            // Server requests immediate heartbeat
                            let seq = *sequence.read().await;
                            let hb = build_heartbeat_payload(seq);
                            let _ = ws_tx
                                .lock()
                                .await
                                .send(tokio_tungstenite::tungstenite::Message::Text(
                                    serde_json::to_string(&hb).unwrap(),
                                ))
                                .await;
                            // The server-requested heartbeat counts as a fresh
                            // beat — reset the ACK gate so the periodic task
                            // doesn't see a stale "unacked" flag.
                            heartbeat_acked.store(false, Ordering::Relaxed);
                        }

                        opcode::HEARTBEAT_ACK => {
                            debug!("Discord heartbeat ACK received");
                            heartbeat_acked.store(true, Ordering::Relaxed);
                        }

                        opcode::RECONNECT => {
                            info!("Discord: server requested reconnect");
                            break 'inner true;
                        }

                        opcode::INVALID_SESSION => {
                            let resumable = payload["d"].as_bool().unwrap_or(false);
                            if resumable {
                                info!("Discord: invalid session (resumable)");
                            } else {
                                info!("Discord: invalid session (not resumable), clearing session");
                                *session_id_store.write().await = None;
                                *sequence.write().await = None;
                            }
                            break 'inner true;
                        }

                        _ => {
                            debug!("Discord: unknown opcode {op}");
                        }
                    }
                };

                // Tear down the heartbeat task before we either exit or
                // reconnect, so it doesn't outlive its WebSocket sink.
                if let Some(h) = heartbeat_handle.take() {
                    h.abort();
                }

                if !should_reconnect || *shutdown.borrow() {
                    break;
                }

                // Try resume URL if available
                if let Some(ref url) = *resume_url_store.read().await {
                    connect_url = format!("{url}/?v=10&encoding=json");
                }

                warn!("Discord: reconnecting in {backoff:?}");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }

            info!("Discord gateway loop stopped");
        });

        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        Ok(Box::pin(stream))
    }

    async fn send(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // platform_id is the channel_id for Discord
        let channel_id = &user.platform_id;
        match content {
            ChannelContent::Text(text) => {
                self.api_send_message(channel_id, &text).await?;
            }
            ChannelContent::FileData {
                data,
                filename,
                mime_type,
            } => {
                self.api_send_attachment(channel_id, data, &filename, &mime_type, None)
                    .await?;
            }
            ChannelContent::File {
                url,
                filename,
                mime,
                size: _,
            } => {
                // Fetch then route through the existing multipart helper.
                // `download_url_to_bytes` enforces 15s timeout and 25 MiB cap.
                let (bytes, response_ct) = self.download_url_to_bytes(&url).await?;
                let resolved_filename =
                    resolve_file_filename(Some(filename.as_str()), &url);
                let resolved_mime = resolve_file_mime(
                    mime.as_deref(),
                    response_ct.as_deref(),
                    &resolved_filename,
                );
                // `File` carries no caption field; the upstream router emits a
                // Multipart([Text, File]) when there's a caption, which is
                // handled by the parent recursion path (not this arm).
                self.api_send_attachment(
                    channel_id,
                    bytes,
                    &resolved_filename,
                    &resolved_mime,
                    None,
                )
                .await?;
            }
            ChannelContent::Image { url, caption } => {
                let (bytes, response_ct) = self.download_url_to_bytes(&url).await?;
                let resolved_mime = resolve_image_mime(response_ct.as_deref(), &url);
                let resolved_filename = resolve_image_filename(&url, &resolved_mime);
                let caption_ref = caption.as_deref().filter(|s| !s.is_empty());
                self.api_send_attachment(
                    channel_id,
                    bytes,
                    &resolved_filename,
                    &resolved_mime,
                    caption_ref,
                )
                .await?;
            }
            _ => {
                self.api_send_message(channel_id, "(Unsupported content type)")
                    .await?;
            }
        }
        Ok(())
    }

    async fn send_typing(&self, user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        self.api_send_typing(&user.platform_id).await
    }

    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.shutdown_tx.send(true);
        Ok(())
    }
}

/// Maximum byte size for an attachment to be classified as a vision-eligible
/// image. Anthropic's image content blocks are capped at 5 MB; oversize images
/// fall through to `File` so the bridge passes the URL as text instead of
/// attempting an inline image block.
const VISION_IMAGE_MAX_BYTES: u64 = 5 * 1024 * 1024;

/// Build the `payload_json` body for an outbound attachment request.
///
/// Discord's `POST /channels/{id}/messages` multipart endpoint expects a
/// `payload_json` part containing the same JSON the JSON-only variant would
/// take. Captions longer than `DISCORD_MSG_LIMIT` chars must be truncated
/// explicitly; otherwise the API responds 400 and silently drops the upload.
fn build_attachment_payload_json(caption: Option<&str>) -> String {
    match caption {
        Some(c) if !c.is_empty() => {
            let truncated: String = c.chars().take(DISCORD_MSG_LIMIT).collect();
            serde_json::json!({ "content": truncated }).to_string()
        }
        _ => serde_json::json!({}).to_string(),
    }
}

/// Strip MIME parameters (e.g. `; charset=utf-8`) so downstream comparisons
/// against canonical types like `image/png` work. Lower-cases and trims so
/// `IMAGE/PNG ; charset=utf-8` and `image/png` both normalize to `image/png`.
fn strip_mime_params(raw: &str) -> String {
    raw.split(';').next().unwrap_or("").trim().to_ascii_lowercase()
}

/// Derive a filename from a URL path: take the segment after the last `/`,
/// drop any query/fragment, percent-decode best-effort. Returns None if the
/// URL has no useful path segment (e.g. `https://host/`).
fn derive_filename_from_url(url: &str) -> Option<String> {
    // Strip scheme://host. We only care about the path-ish suffix; doing
    // this without a real URL parser keeps the helper dep-free and total
    // (a malformed URL still gets a best-effort answer).
    let after_scheme = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let path = after_scheme.split_once('/').map(|(_, r)| r).unwrap_or("");
    // Drop query and fragment.
    let path = path.split(['?', '#']).next().unwrap_or("");
    let last = path.rsplit('/').next().unwrap_or("");
    if last.is_empty() {
        return None;
    }
    // Percent-decode best-effort; fall back to the raw segment on failure.
    let decoded = percent_decode_lossy(last);
    if decoded.is_empty() {
        None
    } else {
        Some(decoded)
    }
}

/// Tiny percent-decoder. We don't pull in `percent-encoding` for this — the
/// adapter already avoids new deps and we only need it to prettify Discord
/// CDN paths like `photo%20final.png` → `photo final.png`.
fn percent_decode_lossy(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Pick a filename for an outbound `File` arm. Preference order: explicit
/// `filename` field → URL path tail → `"file"`.
fn resolve_file_filename(field: Option<&str>, url: &str) -> String {
    field
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| derive_filename_from_url(url))
        .unwrap_or_else(|| "file".to_string())
}

/// Pick a MIME for an outbound `File` arm. Preference order: explicit `mime`
/// field → response Content-Type → extension lookup from filename →
/// `application/octet-stream`.
fn resolve_file_mime(
    field: Option<&str>,
    response_ct: Option<&str>,
    filename: &str,
) -> String {
    field
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| response_ct.map(str::to_string))
        .or_else(|| mime_from_extension(filename).map(str::to_string))
        .unwrap_or_else(|| "application/octet-stream".to_string())
}

/// Pick a filename for an outbound `Image` arm. Preference order: URL path
/// tail → `"image" + extension` inferred from the resolved MIME (default
/// `.png`).
fn resolve_image_filename(url: &str, resolved_mime: &str) -> String {
    if let Some(name) = derive_filename_from_url(url) {
        return name;
    }
    let ext = match resolved_mime {
        "image/jpeg" => ".jpg",
        "image/gif" => ".gif",
        "image/webp" => ".webp",
        "image/heic" => ".heic",
        "image/heif" => ".heif",
        _ => ".png",
    };
    format!("image{ext}")
}

/// Pick a MIME for an outbound `Image` arm. Preference order: response
/// Content-Type → extension lookup from URL tail → `image/png`.
fn resolve_image_mime(response_ct: Option<&str>, url: &str) -> String {
    if let Some(ct) = response_ct.filter(|s| !s.is_empty()) {
        return ct.to_string();
    }
    if let Some(tail) = derive_filename_from_url(url) {
        if let Some(ext) = mime_from_extension(&tail) {
            return ext.to_string();
        }
    }
    "image/png".to_string()
}

/// Best-effort MIME inference from a filename extension. Used as a fallback
/// when Discord's `content_type` field is missing or empty (we've observed
/// this on some bot-relayed attachments).
fn mime_from_extension(filename: &str) -> Option<&'static str> {
    let ext = filename.rsplit('.').next()?.to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "heic" => Some("image/heic"),
        "heif" => Some("image/heif"),
        "pdf" => Some("application/pdf"),
        "txt" => Some("text/plain"),
        "md" => Some("text/markdown"),
        "json" => Some("application/json"),
        "mp4" => Some("video/mp4"),
        "mov" => Some("video/quicktime"),
        "mp3" => Some("audio/mpeg"),
        "wav" => Some("audio/wav"),
        "ogg" => Some("audio/ogg"),
        _ => None,
    }
}

/// Classify a single Discord attachment JSON object into a `ChannelContent`
/// block. Vision-eligible image MIME types (jpeg/png/gif/webp) under
/// `VISION_IMAGE_MAX_BYTES` become `Image`; everything else becomes `File`
/// (URL-pass-through; the bridge will surface it as a text descriptor in v1).
///
/// MIME resolution chain: `attachments[].content_type` (if non-empty) →
/// extension lookup → `application/octet-stream`.
fn classify_discord_attachment(att: &serde_json::Value) -> ChannelContent {
    let url = att["url"].as_str().unwrap_or("").to_string();
    let filename = att["filename"].as_str().unwrap_or("file").to_string();
    let size = att["size"].as_u64();

    let resolved_mime: String = att["content_type"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| mime_from_extension(&filename).map(str::to_string))
        .unwrap_or_else(|| "application/octet-stream".to_string());

    let is_vision_mime = matches!(
        resolved_mime.as_str(),
        "image/jpeg" | "image/png" | "image/gif" | "image/webp"
    );
    // If size is unknown, optimistically allow the image — the bridge will
    // surface a 4xx if Anthropic rejects it, which is better than silently
    // demoting to a text URL.
    let within_vision_limit = size.map(|s| s <= VISION_IMAGE_MAX_BYTES).unwrap_or(true);

    if is_vision_mime && within_vision_limit {
        ChannelContent::Image { url, caption: None }
    } else {
        ChannelContent::File {
            url,
            filename,
            mime: Some(resolved_mime),
            size,
        }
    }
}

/// Parse a Discord MESSAGE_CREATE or MESSAGE_UPDATE payload into a `ChannelMessage`.
async fn parse_discord_message(
    d: &serde_json::Value,
    bot_user_id: &Arc<RwLock<Option<String>>>,
    allowed_guilds: &[String],
    allowed_users: &[String],
    ignore_bots: bool,
) -> Option<ChannelMessage> {
    // Diagnostic: dump the raw Discord payload so we can ground attachment
    // parsing in real JSON. Gated by RUST_LOG; silent at default `info` level.
    // Enable with: RUST_LOG=openfang_channels::discord=debug
    debug!(target: "openfang_channels::discord", payload = %d, "discord raw message payload");

    let author = d.get("author")?;
    let author_id = author["id"].as_str()?;

    // Filter out bot's own messages
    if let Some(ref bid) = *bot_user_id.read().await {
        if author_id == bid {
            return None;
        }
    }

    // Filter out other bots (configurable via ignore_bots)
    if ignore_bots && author["bot"].as_bool() == Some(true) {
        return None;
    }

    // Filter by allowed users
    if !allowed_users.is_empty() && !allowed_users.iter().any(|u| u == author_id) {
        debug!("Discord: ignoring message from unlisted user {author_id}");
        return None;
    }

    // Filter by allowed guilds
    if !allowed_guilds.is_empty() {
        if let Some(guild_id) = d["guild_id"].as_str() {
            if !allowed_guilds.iter().any(|g| g == guild_id) {
                return None;
            }
        }
    }

    let content_text = d["content"].as_str().unwrap_or("");
    let channel_id = d["channel_id"].as_str()?;
    let message_id = d["id"].as_str().unwrap_or("0");
    let username = author["username"].as_str().unwrap_or("Unknown");
    let discriminator = author["discriminator"].as_str().unwrap_or("0000");
    let display_name = if discriminator == "0" {
        username.to_string()
    } else {
        format!("{username}#{discriminator}")
    };

    let timestamp = d["timestamp"]
        .as_str()
        .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(chrono::Utc::now);

    // Parse commands (messages starting with /). Commands do not carry
    // attachments in v1; attachment processing only runs in the non-command path.
    let content = if content_text.starts_with('/') {
        let parts: Vec<&str> = content_text.splitn(2, ' ').collect();
        let cmd_name = &parts[0][1..];
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
        let attachment_blocks: Vec<ChannelContent> = d["attachments"]
            .as_array()
            .map(|arr| arr.iter().map(classify_discord_attachment).collect())
            .unwrap_or_default();

        match (content_text.is_empty(), attachment_blocks.len()) {
            // No text, no attachments → nothing to ingest.
            (true, 0) => return None,
            // Text only.
            (false, 0) => ChannelContent::Text(content_text.to_string()),
            // Single attachment, no caption.
            (true, 1) => attachment_blocks.into_iter().next().unwrap(),
            // Single attachment + caption: emit Multipart with the caption as
            // a sibling Text block. This keeps the caption visible to providers
            // that flatten content to text only (e.g. claude-code/*, which
            // currently drops Image blocks) — the user gets a coherent
            // text-only response instead of a hallucination. Vision-capable
            // providers see the same blocks and dispatch multimodally.
            (false, 1) => {
                let block = attachment_blocks.into_iter().next().unwrap();
                let normalized = match block {
                    // Drop any caption that classify_discord_attachment may have
                    // attached; the sibling Text block is now the caption.
                    ChannelContent::Image { url, caption: _ } => {
                        ChannelContent::Image { url, caption: None }
                    }
                    other => other,
                };
                ChannelContent::Multipart(vec![
                    ChannelContent::Text(content_text.to_string()),
                    normalized,
                ])
            }
            // Multiple attachments, no caption.
            (true, _) => ChannelContent::Multipart(attachment_blocks),
            // Multiple attachments + caption: text first, then attachments
            // (matches Discord's visual ordering: text above attachments).
            (false, _) => {
                let mut blocks = Vec::with_capacity(attachment_blocks.len() + 1);
                blocks.push(ChannelContent::Text(content_text.to_string()));
                blocks.extend(attachment_blocks);
                ChannelContent::Multipart(blocks)
            }
        }
    };

    // Determine if this is a group message (guild_id present = server channel)
    let is_group = d["guild_id"].as_str().is_some();

    // Check if bot was @mentioned (for MentionOnly policy enforcement)
    let was_mentioned = if let Some(ref bid) = *bot_user_id.read().await {
        // Check Discord mentions array
        let mentioned_in_array = d["mentions"]
            .as_array()
            .map(|arr| arr.iter().any(|m| m["id"].as_str() == Some(bid.as_str())))
            .unwrap_or(false);
        // Also check content for <@bot_id> or <@!bot_id> patterns
        let mentioned_in_content = content_text.contains(&format!("<@{bid}>"))
            || content_text.contains(&format!("<@!{bid}>"));
        mentioned_in_array || mentioned_in_content
    } else {
        false
    };

    let mut metadata = HashMap::new();
    if was_mentioned {
        metadata.insert("was_mentioned".to_string(), serde_json::json!(true));
    }
    // Stash the Discord author ID so the router can key bindings on user, not channel.
    // (`sender.platform_id` below is the channel ID, used for the send path.)
    metadata.insert("sender_user_id".to_string(), serde_json::json!(author_id));

    Some(ChannelMessage {
        channel: ChannelType::Discord,
        platform_message_id: message_id.to_string(),
        sender: ChannelUser {
            platform_id: channel_id.to_string(),
            display_name,
            openfang_user: None,
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

    #[tokio::test]
    async fn test_parse_discord_message_basic() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "content": "Hello agent!",
            "author": {
                "id": "user456",
                "username": "alice",
                "discriminator": "0",
                "bot": false
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        assert_eq!(msg.channel, ChannelType::Discord);
        assert_eq!(msg.sender.display_name, "alice");
        assert_eq!(msg.sender.platform_id, "ch1");
        assert!(matches!(msg.content, ChannelContent::Text(ref t) if t == "Hello agent!"));
    }

    #[tokio::test]
    async fn test_parse_discord_message_filters_bot() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "content": "My own message",
            "author": {
                "id": "bot123",
                "username": "openfang",
                "discriminator": "0"
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        let msg = parse_discord_message(&d, &bot_id, &[], &[], true).await;
        assert!(msg.is_none());
    }

    #[tokio::test]
    async fn test_parse_discord_message_filters_other_bots() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "content": "Bot message",
            "author": {
                "id": "other_bot",
                "username": "somebot",
                "discriminator": "0",
                "bot": true
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        let msg = parse_discord_message(&d, &bot_id, &[], &[], true).await;
        assert!(msg.is_none());
    }

    #[tokio::test]
    async fn test_parse_discord_ignore_bots_false_allows_other_bots() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "content": "Bot message",
            "author": {
                "id": "other_bot",
                "username": "somebot",
                "discriminator": "0",
                "bot": true
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        // With ignore_bots=false, other bots' messages should be allowed
        let msg = parse_discord_message(&d, &bot_id, &[], &[], false).await;
        assert!(msg.is_some());
        let msg = msg.unwrap();
        assert_eq!(msg.sender.display_name, "somebot");
        assert!(matches!(msg.content, ChannelContent::Text(ref t) if t == "Bot message"));
    }

    #[tokio::test]
    async fn test_parse_discord_ignore_bots_false_still_filters_self() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "content": "My own message",
            "author": {
                "id": "bot123",
                "username": "openfang",
                "discriminator": "0",
                "bot": true
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        // Even with ignore_bots=false, the bot's own messages must still be filtered
        let msg = parse_discord_message(&d, &bot_id, &[], &[], false).await;
        assert!(msg.is_none());
    }

    #[tokio::test]
    async fn test_parse_discord_message_guild_filter() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "guild_id": "999",
            "content": "Hello",
            "author": {
                "id": "user1",
                "username": "bob",
                "discriminator": "0"
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        // Not in allowed guilds
        let msg =
            parse_discord_message(&d, &bot_id, &["111".into(), "222".into()], &[], true).await;
        assert!(msg.is_none());

        // In allowed guilds
        let msg = parse_discord_message(&d, &bot_id, &["999".into()], &[], true).await;
        assert!(msg.is_some());
    }

    #[tokio::test]
    async fn test_parse_discord_command() {
        let bot_id = Arc::new(RwLock::new(None));
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "content": "/agent hello-world",
            "author": {
                "id": "user1",
                "username": "alice",
                "discriminator": "0"
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
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
    async fn test_parse_discord_empty_content() {
        let bot_id = Arc::new(RwLock::new(None));
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "content": "",
            "author": {
                "id": "user1",
                "username": "alice",
                "discriminator": "0"
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        let msg = parse_discord_message(&d, &bot_id, &[], &[], true).await;
        assert!(msg.is_none());
    }

    #[tokio::test]
    async fn test_parse_discord_discriminator() {
        let bot_id = Arc::new(RwLock::new(None));
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "content": "Hi",
            "author": {
                "id": "user1",
                "username": "alice",
                "discriminator": "1234"
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        assert_eq!(msg.sender.display_name, "alice#1234");
    }

    #[tokio::test]
    async fn test_parse_discord_message_update() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "content": "Edited message content",
            "author": {
                "id": "user456",
                "username": "alice",
                "discriminator": "0",
                "bot": false
            },
            "timestamp": "2024-01-01T00:00:00+00:00",
            "edited_timestamp": "2024-01-01T00:01:00+00:00"
        });

        // MESSAGE_UPDATE uses the same parse function as MESSAGE_CREATE
        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        assert_eq!(msg.channel, ChannelType::Discord);
        assert!(
            matches!(msg.content, ChannelContent::Text(ref t) if t == "Edited message content")
        );
    }

    #[tokio::test]
    async fn test_parse_discord_allowed_users_filter() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "content": "Hello",
            "author": {
                "id": "user999",
                "username": "bob",
                "discriminator": "0"
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        // Not in allowed users
        let msg = parse_discord_message(
            &d,
            &bot_id,
            &[],
            &["user111".into(), "user222".into()],
            true,
        )
        .await;
        assert!(msg.is_none());

        // In allowed users
        let msg = parse_discord_message(&d, &bot_id, &[], &["user999".into()], true).await;
        assert!(msg.is_some());

        // Empty allowed_users = allow all
        let msg = parse_discord_message(&d, &bot_id, &[], &[], true).await;
        assert!(msg.is_some());
    }

    #[tokio::test]
    async fn test_parse_discord_mention_detection() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));

        // Message with bot mentioned in mentions array
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "guild_id": "guild1",
            "content": "Hey <@bot123> help me",
            "mentions": [{"id": "bot123", "username": "openfang"}],
            "author": {
                "id": "user1",
                "username": "alice",
                "discriminator": "0"
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        assert!(msg.is_group);
        assert_eq!(
            msg.metadata.get("was_mentioned").and_then(|v| v.as_bool()),
            Some(true)
        );

        // Message without mention in group
        let d2 = serde_json::json!({
            "id": "msg2",
            "channel_id": "ch1",
            "guild_id": "guild1",
            "content": "Just chatting",
            "author": {
                "id": "user1",
                "username": "alice",
                "discriminator": "0"
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        let msg2 = parse_discord_message(&d2, &bot_id, &[], &[], true)
            .await
            .unwrap();
        assert!(msg2.is_group);
        assert!(!msg2.metadata.contains_key("was_mentioned"));
    }

    #[tokio::test]
    async fn test_parse_discord_dm_not_group() {
        let bot_id = Arc::new(RwLock::new(None));
        let d = serde_json::json!({
            "id": "msg1",
            "channel_id": "dm-ch1",
            "content": "Hello",
            "author": {
                "id": "user1",
                "username": "alice",
                "discriminator": "0"
            },
            "timestamp": "2024-01-01T00:00:00+00:00"
        });

        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        assert!(!msg.is_group);
    }

    #[test]
    fn test_build_heartbeat_payload_with_sequence() {
        let payload = build_heartbeat_payload(Some(42));
        assert_eq!(payload["op"], 1);
        assert_eq!(payload["d"], 42);
        // Round-trip through serde_json::to_string and re-parse to assert
        // valid JSON matching {"op":1,"d":42} regardless of key ordering.
        let s = serde_json::to_string(&payload).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed, serde_json::json!({"op": 1, "d": 42}));
    }

    #[test]
    fn test_build_heartbeat_payload_without_sequence() {
        let payload = build_heartbeat_payload(None);
        assert_eq!(payload["op"], 1);
        assert!(payload["d"].is_null());
        let s = serde_json::to_string(&payload).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(
            parsed,
            serde_json::json!({"op": 1, "d": serde_json::Value::Null})
        );
    }

    #[test]
    fn test_discord_adapter_creation() {
        let adapter = DiscordAdapter::new(
            "test-token".to_string(),
            vec!["123".to_string(), "456".to_string()],
            vec![],
            true,
            37376,
        );
        assert_eq!(adapter.name(), "discord");
        assert_eq!(adapter.channel_type(), ChannelType::Discord);
    }

    // -- Multipart / attachment parsing tests (commit 4) ----------------------

    fn att(filename: &str, content_type: Option<&str>, size: u64) -> serde_json::Value {
        let mut obj = serde_json::json!({
            "url": format!("https://cdn.discordapp.com/attachments/1/2/{filename}"),
            "filename": filename,
            "size": size,
        });
        if let Some(ct) = content_type {
            obj["content_type"] = serde_json::Value::String(ct.to_string());
        }
        obj
    }

    fn payload_with(content: &str, attachments: Vec<serde_json::Value>) -> serde_json::Value {
        serde_json::json!({
            "id": "msg1",
            "channel_id": "ch1",
            "content": content,
            "author": {
                "id": "user456",
                "username": "alice",
                "discriminator": "0",
                "bot": false
            },
            "timestamp": "2024-01-01T00:00:00+00:00",
            "attachments": attachments,
        })
    }

    #[tokio::test]
    async fn test_parse_image_only_no_caption() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = payload_with("", vec![att("photo.png", Some("image/png"), 100_000)]);
        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        match msg.content {
            ChannelContent::Image { caption, url } => {
                assert!(caption.is_none());
                assert!(url.contains("photo.png"));
            }
            other => panic!("expected Image, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_image_with_caption() {
        // Single image + caption is emitted as Multipart([Text, Image]) so the
        // caption survives providers that flatten content blocks to text only
        // (e.g. claude-code/*). The Image carries no caption of its own; the
        // sibling Text block IS the caption.
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = payload_with(
            "look at this",
            vec![att("photo.jpg", Some("image/jpeg"), 50_000)],
        );
        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        match msg.content {
            ChannelContent::Multipart(parts) => {
                assert_eq!(parts.len(), 2);
                assert!(matches!(&parts[0], ChannelContent::Text(t) if t == "look at this"));
                match &parts[1] {
                    ChannelContent::Image { caption, url } => {
                        assert!(
                            caption.is_none(),
                            "image caption should be None; the sibling Text block is the caption"
                        );
                        assert!(url.contains("photo.jpg"));
                    }
                    other => panic!("expected Image as second part, got {other:?}"),
                }
            }
            other => panic!("expected Multipart, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_multi_image_no_caption() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = payload_with(
            "",
            vec![
                att("a.png", Some("image/png"), 10_000),
                att("b.png", Some("image/png"), 20_000),
            ],
        );
        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        match msg.content {
            ChannelContent::Multipart(parts) => {
                assert_eq!(parts.len(), 2);
                assert!(parts
                    .iter()
                    .all(|p| matches!(p, ChannelContent::Image { .. })));
            }
            other => panic!("expected Multipart, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_multi_image_with_caption() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = payload_with(
            "two pics",
            vec![
                att("a.png", Some("image/png"), 10_000),
                att("b.png", Some("image/png"), 20_000),
            ],
        );
        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        match msg.content {
            ChannelContent::Multipart(parts) => {
                assert_eq!(parts.len(), 3);
                // Text first, then images.
                assert!(matches!(&parts[0], ChannelContent::Text(t) if t == "two pics"));
                assert!(matches!(&parts[1], ChannelContent::Image { .. }));
                assert!(matches!(&parts[2], ChannelContent::Image { .. }));
            }
            other => panic!("expected Multipart, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_heic_falls_to_file() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = payload_with("", vec![att("photo.heic", Some("image/heic"), 100_000)]);
        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        match msg.content {
            ChannelContent::File { mime, filename, .. } => {
                assert_eq!(filename, "photo.heic");
                assert_eq!(mime.as_deref(), Some("image/heic"));
            }
            other => panic!("expected File, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_oversize_image_falls_to_file() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        // 6 MB exceeds VISION_IMAGE_MAX_BYTES (5 MB).
        let d = payload_with(
            "",
            vec![att("huge.png", Some("image/png"), 6 * 1024 * 1024)],
        );
        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        match msg.content {
            ChannelContent::File {
                filename,
                mime,
                size,
                ..
            } => {
                assert_eq!(filename, "huge.png");
                assert_eq!(mime.as_deref(), Some("image/png"));
                assert_eq!(size, Some(6 * 1024 * 1024));
            }
            other => panic!("expected File, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_file_with_caption_yields_multipart() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = payload_with(
            "see attached",
            vec![att("doc.pdf", Some("application/pdf"), 200_000)],
        );
        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        match msg.content {
            ChannelContent::Multipart(parts) => {
                assert_eq!(parts.len(), 2);
                assert!(matches!(&parts[0], ChannelContent::Text(t) if t == "see attached"));
                assert!(matches!(&parts[1], ChannelContent::File { .. }));
            }
            other => panic!("expected Multipart, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_extension_fallback_when_content_type_missing() {
        // Discord occasionally omits content_type on bot-relayed attachments;
        // we should fall back to the filename extension.
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = payload_with("", vec![att("pic.png", None, 50_000)]);
        let msg = parse_discord_message(&d, &bot_id, &[], &[], true)
            .await
            .unwrap();
        assert!(matches!(msg.content, ChannelContent::Image { .. }));
    }

    // -- Pure helper tests for File/Image arm fallback chains --------------

    #[test]
    fn test_strip_mime_params_basic() {
        assert_eq!(strip_mime_params("image/png"), "image/png");
        assert_eq!(strip_mime_params("image/png; charset=utf-8"), "image/png");
        assert_eq!(
            strip_mime_params("  IMAGE/PNG ; charset=utf-8 "),
            "image/png"
        );
        assert_eq!(strip_mime_params(""), "");
    }

    #[test]
    fn test_derive_filename_from_url() {
        assert_eq!(
            derive_filename_from_url("https://cdn.example.com/a/b/photo.png"),
            Some("photo.png".to_string())
        );
        assert_eq!(
            derive_filename_from_url("https://cdn.example.com/a/b/photo.png?ex=1&hm=2"),
            Some("photo.png".to_string())
        );
        assert_eq!(
            derive_filename_from_url("https://cdn.example.com/a/photo%20final.png"),
            Some("photo final.png".to_string())
        );
        // Trailing slash → no filename derivable.
        assert_eq!(derive_filename_from_url("https://cdn.example.com/"), None);
        assert_eq!(derive_filename_from_url("https://cdn.example.com"), None);
    }

    #[test]
    fn test_resolve_file_filename_chain() {
        // Field wins over URL.
        assert_eq!(
            resolve_file_filename(Some("explicit.bin"), "https://x/y/url.dat"),
            "explicit.bin"
        );
        // Empty field → URL fallback.
        assert_eq!(
            resolve_file_filename(Some(""), "https://x/y/url.dat"),
            "url.dat"
        );
        // None → URL fallback.
        assert_eq!(
            resolve_file_filename(None, "https://x/y/url.dat"),
            "url.dat"
        );
        // No URL tail → "file".
        assert_eq!(resolve_file_filename(None, "https://x/"), "file");
    }

    #[test]
    fn test_resolve_file_mime_chain() {
        // Field wins.
        assert_eq!(
            resolve_file_mime(Some("application/pdf"), Some("text/plain"), "f.txt"),
            "application/pdf"
        );
        // No field → response Content-Type.
        assert_eq!(
            resolve_file_mime(None, Some("text/plain"), "f.txt"),
            "text/plain"
        );
        // No field, no CT → extension lookup.
        assert_eq!(resolve_file_mime(None, None, "f.pdf"), "application/pdf");
        // Nothing → default.
        assert_eq!(
            resolve_file_mime(None, None, "no-ext"),
            "application/octet-stream"
        );
    }

    #[test]
    fn test_resolve_image_filename_chain() {
        // URL tail wins.
        assert_eq!(
            resolve_image_filename("https://x/y/picture.jpg", "image/jpeg"),
            "picture.jpg"
        );
        // No URL tail → image + ext from MIME.
        assert_eq!(resolve_image_filename("https://x/", "image/jpeg"), "image.jpg");
        assert_eq!(resolve_image_filename("https://x/", "image/png"), "image.png");
        assert_eq!(resolve_image_filename("https://x/", "image/webp"), "image.webp");
        // Unknown MIME → .png default.
        assert_eq!(
            resolve_image_filename("https://x/", "application/octet-stream"),
            "image.png"
        );
    }

    #[test]
    fn test_resolve_image_mime_chain() {
        // Response CT wins.
        assert_eq!(
            resolve_image_mime(Some("image/jpeg"), "https://x/y/foo.png"),
            "image/jpeg"
        );
        // No CT → URL extension.
        assert_eq!(
            resolve_image_mime(None, "https://x/y/foo.png"),
            "image/png"
        );
        // No CT, no extension → default.
        assert_eq!(resolve_image_mime(None, "https://x/y/blob"), "image/png");
    }

    // -- download_url_to_bytes size-cap tests ------------------------------

    /// Spawn a hand-rolled HTTP server that replies with a fixed status,
    /// optional Content-Length header (lying or omitted), and a body produced
    /// by the supplied closure. This intentionally does NOT use axum's
    /// `Body::from(Vec<u8>)` (which always sends the real Content-Length); we
    /// need control over the header to exercise both pre-flight and streaming
    /// rejection paths.
    async fn spawn_raw_http_server(
        status_line: &'static str,
        content_length: Option<&'static str>,
        body: bytes::Bytes,
    ) -> String {
        use tokio::io::AsyncWriteExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            // Accept exactly one connection — sufficient for a single
            // download_url_to_bytes test invocation.
            let (mut sock, _) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => return,
            };
            // Drain the request line + headers (best-effort; we don't parse).
            let mut buf = [0u8; 4096];
            let _ = tokio::io::AsyncReadExt::read(&mut sock, &mut buf).await;
            let mut header = format!("HTTP/1.1 {status_line}\r\n");
            header.push_str("Content-Type: application/octet-stream\r\n");
            if let Some(cl) = content_length {
                header.push_str(&format!("Content-Length: {cl}\r\n"));
            }
            header.push_str("Connection: close\r\n\r\n");
            let _ = sock.write_all(header.as_bytes()).await;
            let _ = sock.write_all(&body).await;
            let _ = sock.shutdown().await;
        });
        format!("http://{addr}/file")
    }

    fn test_adapter() -> DiscordAdapter {
        DiscordAdapter::new("test-token".into(), vec![], vec![], true, 0)
    }

    #[tokio::test]
    async fn test_download_size_cap_via_content_length() {
        // Server advertises an oversized Content-Length and sends a tiny body.
        // The adapter must reject before reading anything significant.
        let oversized = (URL_FETCH_MAX_BYTES + 1).to_string();
        // Leak the string so we can hand &'static str to the spawn helper.
        let cl: &'static str = Box::leak(oversized.into_boxed_str());
        let url =
            spawn_raw_http_server("200 OK", Some(cl), bytes::Bytes::from_static(b"x")).await;

        let adapter = test_adapter();
        let res = adapter.download_url_to_bytes(&url).await;
        assert!(res.is_err(), "expected Err on oversized Content-Length");
        let err = res.unwrap_err().to_string();
        assert!(err.contains("Content-Length"), "err should mention CL: {err}");
    }

    #[tokio::test]
    async fn test_download_size_cap_via_streaming() {
        // Server omits Content-Length and streams >25 MiB. Adapter must abort
        // mid-stream rather than buffering the full payload.
        let big = bytes::Bytes::from(vec![0u8; URL_FETCH_MAX_BYTES + 1024]);
        let url = spawn_raw_http_server("200 OK", None, big).await;

        let adapter = test_adapter();
        let res = adapter.download_url_to_bytes(&url).await;
        assert!(res.is_err(), "expected Err on oversized streamed body");
        let err = res.unwrap_err().to_string();
        assert!(
            err.contains("streamed body exceeds cap")
                || err.contains("Content-Length"),
            "err should mention size cap: {err}"
        );
    }

    #[tokio::test]
    async fn test_download_under_cap_succeeds_and_returns_bytes_and_ct() {
        // Sanity check: a normal small payload returns the bytes and the
        // stripped Content-Type. Uses axum so Content-Length is set correctly.
        use axum::{routing::get, Router};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new().route(
            "/f",
            get(|| async {
                (
                    [(axum::http::header::CONTENT_TYPE, "image/png; charset=utf-8")],
                    bytes::Bytes::from_static(b"PNGDATA"),
                )
            }),
        );
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let url = format!("http://{addr}/f");

        let adapter = test_adapter();
        let (bytes, ct) = adapter.download_url_to_bytes(&url).await.unwrap();
        assert_eq!(&bytes[..], b"PNGDATA");
        // Content-Type parameters must be stripped.
        assert_eq!(ct.as_deref(), Some("image/png"));
    }

    #[tokio::test]
    async fn test_parse_empty_message_with_no_attachments_returns_none() {
        let bot_id = Arc::new(RwLock::new(Some("bot123".to_string())));
        let d = payload_with("", vec![]);
        let msg = parse_discord_message(&d, &bot_id, &[], &[], true).await;
        assert!(msg.is_none());
    }
}
