//! Voice channel adapter.
//!
//! Provides a WebSocket server that accepts voice clients (mobile apps, Meet
//! bots, web browsers).  Supports two modes determined by the first frame:
//!
//! ## Text mode (default — backward compatible)
//!
//! Clients handle STT and TTS themselves and exchange JSON text frames.
//!
//! ```text
//! Client → Server (JSON text frames):
//!   { "type": "utterance", "text": "...", "speaker": "Alice", "final": true }
//!   { "type": "cancel" }
//!   { "type": "end" }
//!
//! Server → Client (JSON text frames):
//!   { "type": "response", "text": "...", "emotion": "neutral", "sentence_end": true }
//!   { "type": "backchannel", "text": "I see." }
//!   { "type": "status", "state": "thinking" | "listening" }
//!   { "type": "error", "message": "..." }
//! ```
//!
//! ## PCM mode (requires `stt` + `tts` config)
//!
//! Activated when the first WebSocket frame is binary.  The server handles the
//! full audio pipeline: Smart Turn end-of-utterance detection → STT → agent →
//! TTS → PCM back to client.  JSON control frames are sent alongside binary PCM.
//!
//! ```text
//! Client → Server:
//!   [binary]  Raw Int16 LE PCM at 16 kHz mono, any chunk size
//!
//! Server → Client:
//!   [binary]  Raw Int16 LE PCM at 16 kHz mono (TTS output)
//!   [text]    { "type": "transcribed", "text": "..." }
//!   [text]    { "type": "status", "state": "thinking" | "listening" }
//!   [text]    { "type": "error", "message": "..." }
//! ```
//!
//! ## Emotion Tags
//!
//! The agent may prefix sentences with emotion tags like `[amused]`, which
//! the adapter parses, validates against a known set, and sends as a structured
//! `emotion` field. Unknown tags default to `neutral`.

use crate::smart_turn::SmartTurnDetector;
use crate::stt;
use crate::tts::{self, i16_to_bytes};
use crate::types::{
    ChannelAdapter, ChannelContent, ChannelMessage, ChannelStatus, ChannelType, ChannelUser,
};
use async_trait::async_trait;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use chrono::Utc;
use futures::{SinkExt, Stream, StreamExt};
use openfang_types::config::{VoiceSttConfig, VoiceTtsConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{mpsc, watch, RwLock};
use tracing::{debug, error, info, warn};

// ── Constants ────────────────────────────────────────────────────────────────

/// Known emotion tags the agent may emit.
const KNOWN_EMOTIONS: &[&str] = &[
    "neutral",
    "amused",
    "concerned",
    "formal",
    "warm",
    "apologetic",
    "excited",
];

const DEFAULT_EMOTION: &str = "neutral";

// ── Protocol Types ───────────────────────────────────────────────────────────

/// Message from client to server.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMessage {
    /// Transcribed speech from the client's STT.
    Utterance {
        text: String,
        #[serde(default)]
        speaker: Option<String>,
        #[serde(default = "default_true")]
        r#final: bool,
    },
    /// Identity announcement — sent once on session open before PCM streams.
    /// Sets the primary speaker name for the session. In text mode, overrides
    /// the per-utterance `speaker` field as the session-wide default.
    Hello { speaker: String },
    /// Cancel current agent response (barge-in).
    Cancel,
    /// End the voice session.
    End,
}

fn default_true() -> bool {
    true
}

/// Message from server to client.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerMessage {
    /// A sentence of the agent's response with emotion.
    Response {
        text: String,
        emotion: String,
        sentence_end: bool,
    },
    /// A backchannel acknowledgment (no LLM involved).
    #[allow(dead_code)]
    Backchannel { text: String },
    /// Status update.
    Status { state: String },
    /// Error message.
    Error { message: String },
    /// Barge-in acknowledged — TTS stopped, server is listening again.
    BargeInAck,
    /// Session configuration sent once after the Hello handshake.
    #[allow(dead_code)]
    Config {
        barge_in_threshold: u32,
        barge_in_speaking_threshold: u32,
    },
}

// ── Voice Session ────────────────────────────────────────────────────────────

/// A connected voice client session.
#[allow(dead_code)]
struct VoiceSession {
    session_id: String,
    sender: ChannelUser,
    /// Send responses back to the WebSocket client.
    response_tx: mpsc::Sender<ServerMessage>,
}

// ── Adapter ──────────────────────────────────────────────────────────────────

/// Voice channel adapter — WebSocket server for voice clients.
pub struct VoiceAdapter {
    /// Address to bind the WebSocket server (for backwards-compatible direct access).
    listen_addr: String,
    /// Default agent to route voice messages to.
    #[allow(dead_code)]
    default_agent: Option<String>,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Active text-mode sessions: session_id → ServerMessage sender.
    sessions: Arc<RwLock<HashMap<String, mpsc::Sender<ServerMessage>>>>,
    /// Active PCM-mode sessions: session_id → agent text sender (→ TTS → PCM out).
    pcm_sessions: Arc<RwLock<HashMap<String, mpsc::Sender<String>>>>,
    /// Status tracking.
    status: Arc<RwLock<ChannelStatus>>,
    /// Receiver for async delegation results (set by channel bridge at startup).
    #[allow(clippy::type_complexity)]
    async_result_rx: Arc<tokio::sync::Mutex<Option<mpsc::Receiver<(String, String)>>>>,
    /// PCM pipeline config (STT, TTS, Smart Turn).
    pipeline: Option<Arc<VoicePipeline>>,
    /// Bridge sender — pre-created so make_router() can share it with start().
    bridge_tx: mpsc::Sender<ChannelMessage>,
    /// Bridge receiver — taken exactly once by start() to return the event stream.
    bridge_rx: Arc<tokio::sync::Mutex<Option<mpsc::Receiver<ChannelMessage>>>>,
}

/// Loaded voice pipeline — STT, TTS, and optional Smart Turn.
pub struct VoicePipeline {
    pub stt: VoiceSttConfig,
    pub tts: VoiceTtsConfig,
    pub smart_turn: Option<SmartTurnDetector>,
    pub barge_in_threshold: u32,
    pub barge_in_speaking_threshold: u32,
}

impl VoiceAdapter {
    /// Create a new voice adapter.
    pub fn new(listen_addr: String, default_agent: Option<String>) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (bridge_tx, bridge_rx) = mpsc::channel::<ChannelMessage>(256);
        Self {
            listen_addr,
            default_agent,
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            pcm_sessions: Arc::new(RwLock::new(HashMap::new())),
            status: Arc::new(RwLock::new(ChannelStatus::default())),
            async_result_rx: Arc::new(tokio::sync::Mutex::new(None)),
            pipeline: None,
            bridge_tx,
            bridge_rx: Arc::new(tokio::sync::Mutex::new(Some(bridge_rx))),
        }
    }

    /// Returns an Axum router containing the `/voice` WebSocket handler.
    ///
    /// Merge this into the main API server router so voice is accessible
    /// through the same port as the REST API — no separate port needs to be
    /// exposed, and a single reverse-proxy rule covers everything.
    ///
    /// The handler shares session state with the optional standalone server
    /// started by `start()`, so both access paths see the same sessions.
    pub fn make_router(&self) -> Router<()> {
        let state = AppState {
            bridge_tx: self.bridge_tx.clone(),
            sessions: self.sessions.clone(),
            pcm_sessions: self.pcm_sessions.clone(),
            status: self.status.clone(),
            pipeline: self.pipeline.clone(),
        };
        Router::new()
            .route("/voice", get(ws_handler))
            .with_state(state)
    }

    /// Attach a voice pipeline for PCM mode.  Call before `start()`.
    ///
    /// `smart_turn` is loaded separately (via `spawn_blocking`) so this method
    /// accepts an already-resolved `Option<SmartTurnDetector>`.
    pub fn with_pipeline(
        mut self,
        stt: VoiceSttConfig,
        tts: VoiceTtsConfig,
        smart_turn: Option<SmartTurnDetector>,
        barge_in_threshold: u32,
        barge_in_speaking_threshold: u32,
    ) -> Self {
        self.pipeline = Some(Arc::new(VoicePipeline {
            stt,
            tts,
            smart_turn,
            barge_in_threshold,
            barge_in_speaking_threshold,
        }));
        self
    }

    /// Set the async result receiver for delegation callbacks.
    /// Called by channel_bridge after wiring the runtime's tool_runner channel.
    pub fn set_async_result_receiver(&self, rx: mpsc::Receiver<(String, String)>) {
        // Can't block here, but this is called before start(), so no contention
        if let Ok(mut guard) = self.async_result_rx.try_lock() {
            *guard = Some(rx);
        }
    }
}

/// Shared state for the axum WebSocket handler.
#[derive(Clone)]
struct AppState {
    /// Channel to send ChannelMessages to the bridge.
    bridge_tx: mpsc::Sender<ChannelMessage>,
    /// Active text-mode sessions.
    sessions: Arc<RwLock<HashMap<String, mpsc::Sender<ServerMessage>>>>,
    /// Active PCM-mode sessions: session_id → agent text sender.
    pcm_sessions: Arc<RwLock<HashMap<String, mpsc::Sender<String>>>>,
    /// Status tracking.
    status: Arc<RwLock<ChannelStatus>>,
    /// Optional PCM pipeline (STT + TTS + Smart Turn).
    pipeline: Option<Arc<VoicePipeline>>,
}

#[async_trait]
impl ChannelAdapter for VoiceAdapter {
    fn name(&self) -> &str {
        "voice"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("voice".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        info!("Starting voice adapter on {}", self.listen_addr);

        // Take bridge_rx — start() can only succeed once.
        let bridge_rx = self
            .bridge_rx
            .lock()
            .await
            .take()
            .ok_or("Voice adapter already started")?;

        // Build Axum app for the standalone server (backwards-compatible direct
        // access on listen_addr).  The /voice handler is also mounted on the
        // main API port via make_router() so a single reverse-proxy suffices.
        let state = AppState {
            bridge_tx: self.bridge_tx.clone(),
            sessions: self.sessions.clone(),
            pcm_sessions: self.pcm_sessions.clone(),
            status: self.status.clone(),
            pipeline: self.pipeline.clone(),
        };

        let app = Router::new()
            .route("/voice", get(ws_handler))
            .route("/health", get(|| async { "ok" }))
            .with_state(state);

        let addr = self.listen_addr.clone();
        let shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let listener = match tokio::net::TcpListener::bind(&addr).await {
                Ok(l) => {
                    info!("Voice WebSocket server listening on {addr}");
                    l
                }
                Err(e) => {
                    error!("Failed to bind voice server on {addr}: {e}");
                    return;
                }
            };

            let mut shutdown = shutdown_rx;
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = shutdown.changed().await;
                })
                .await
                .unwrap_or_else(|e| error!("Voice server error: {e}"));

            info!("Voice WebSocket server stopped");
        });

        {
            let mut s = self.status.write().await;
            s.connected = true;
            s.started_at = Some(Utc::now());
        }

        // Listen for async delegation results (formatted responses)
        if let Some(async_rx) = self.async_result_rx.lock().await.take() {
            let sessions_for_async = self.sessions.clone();
            tokio::spawn(async move {
                let mut async_rx = async_rx;
                while let Some((_caller_id, result_text)) = async_rx.recv().await {
                    debug!(
                        "Async result received for delivery: {}",
                        &result_text[..result_text.len().min(100)]
                    );
                    let sessions = sessions_for_async.read().await;
                    if let Some(tx) = sessions.values().next() {
                        for sentence in split_into_sentences(&result_text) {
                            let (emotion, clean_text) = parse_emotion_tag(&sentence);
                            if clean_text.is_empty() {
                                continue;
                            }
                            let _ = tx
                                .send(ServerMessage::Response {
                                    text: clean_text,
                                    emotion: emotion.to_string(),
                                    sentence_end: true,
                                })
                                .await;
                        }
                        let _ = tx
                            .send(ServerMessage::Status {
                                state: "listening".to_string(),
                            })
                            .await;
                        info!("Delivered async delegation result to voice session");
                    } else {
                        warn!("Async result received but no active voice session");
                    }
                }
            });
        }

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(
            bridge_rx,
        )))
    }

    async fn send(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let text = match &content {
            ChannelContent::Text(t) => t.clone(),
            _ => return Ok(()),
        };

        // Check PCM sessions first — route text directly to TTS pipeline
        {
            let pcm = self.pcm_sessions.read().await;
            let pcm_tx = if let Some(tx) = pcm.get(&user.platform_id) {
                Some(tx.clone())
            } else if pcm.len() == 1 {
                pcm.values().next().cloned()
            } else {
                None
            };

            if let Some(tx) = pcm_tx {
                if text.trim() == "[SILENT]" {
                    return Ok(());
                }
                let _ = tx.send(text).await;
                let mut s = self.status.write().await;
                s.messages_sent += 1;
                return Ok(());
            }
        }

        // Text-mode session routing
        let sessions = self.sessions.read().await;
        let tx = if let Some(tx) = sessions.get(&user.platform_id) {
            tx.clone()
        } else if sessions.len() == 1 {
            sessions.values().next().unwrap().clone()
        } else if !sessions.is_empty() {
            warn!(
                "No exact voice session for {}, routing to first active session",
                user.platform_id
            );
            sessions.values().next().unwrap().clone()
        } else {
            debug!("No voice sessions active, dropping response");
            return Ok(());
        };
        drop(sessions);

        // Filter [SILENT] — valid in group chats but not on voice
        if text.trim() == "[SILENT]" {
            debug!("Filtering [SILENT] on voice channel");
            let _ = tx
                .send(ServerMessage::Status {
                    state: "listening".to_string(),
                })
                .await;
            return Ok(());
        }

        // Split response into sentences, parse emotion tags, send each
        for sentence in split_into_sentences(&text) {
            let (emotion, clean_text) = parse_emotion_tag(&sentence);
            if clean_text.is_empty() {
                continue;
            }

            let msg = ServerMessage::Response {
                text: clean_text,
                emotion: emotion.to_string(),
                sentence_end: true,
            };

            if tx.send(msg).await.is_err() {
                warn!("Voice session closed for {}", user.platform_id);
                break;
            }
        }

        // Signal that we're done responding
        let _ = tx
            .send(ServerMessage::Status {
                state: "listening".to_string(),
            })
            .await;

        {
            let mut s = self.status.write().await;
            s.messages_sent += 1;
        }

        Ok(())
    }

    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Stopping voice adapter");
        let _ = self.shutdown_tx.send(true);
        let mut s = self.status.write().await;
        s.connected = false;
        Ok(())
    }

    fn status(&self) -> ChannelStatus {
        self.status
            .try_read()
            .map(|s| s.clone())
            .unwrap_or_default()
    }
}

// ── WebSocket Handler ────────────────────────────────────────────────────────

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_voice_session(socket, state))
}

async fn handle_voice_session(socket: WebSocket, state: AppState) {
    let session_id = uuid::Uuid::new_v4().to_string();
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Create response channel for this session
    let (response_tx, mut response_rx) = mpsc::channel::<ServerMessage>(64);

    // Register session
    {
        let mut sessions = state.sessions.write().await;
        sessions.insert(session_id.clone(), response_tx.clone());
    }

    info!("Voice session started: {session_id}");

    // Peek at the first frame to decide mode.
    // Binary frame → PCM mode (if pipeline configured).
    // Hello text frame → may still become PCM mode: read the next frame to decide,
    //   passing the Hello through so PCM mode can process it for speaker identity.
    // Other text frame → text mode (existing protocol).
    let first_frame = match ws_rx.next().await {
        Some(Ok(f)) => f,
        _ => {
            let mut sessions = state.sessions.write().await;
            sessions.remove(&session_id);
            return;
        }
    };

    // If the first frame is a Hello text frame and we have a pipeline, read one
    // more frame to determine whether the client intends PCM mode (binary) or
    // text mode. The speaker name from Hello is extracted here and passed
    // directly to handle_pcm_session so it isn't lost.
    let mut pre_hello_speaker: Option<String> = None;
    let (first_frame, is_pcm) = if let Message::Text(ref t) = first_frame {
        if let Ok(ClientMessage::Hello { ref speaker }) = serde_json::from_str::<ClientMessage>(t) {
            pre_hello_speaker = Some(speaker.clone());
            if state.pipeline.is_some() {
                // Read the follow-up frame to see if it's binary PCM
                match ws_rx.next().await {
                    Some(Ok(second)) if matches!(second, Message::Binary(_)) => (second, true),
                    Some(Ok(second)) => {
                        // Text mode: Hello then a text frame — stay in text mode,
                        // handle Hello in the text loop below
                        (second, false)
                    }
                    _ => {
                        let mut sessions = state.sessions.write().await;
                        sessions.remove(&session_id);
                        return;
                    }
                }
            } else {
                (first_frame, false)
            }
        } else {
            (first_frame, false)
        }
    } else {
        let is_pcm = matches!(first_frame, Message::Binary(_));
        (first_frame, is_pcm)
    };

    if is_pcm {
        if let Some(pipeline) = state.pipeline.clone() {
            info!("Voice session {session_id}: PCM mode");
            handle_pcm_session(
                first_frame,
                ws_tx,
                ws_rx,
                state,
                session_id,
                pipeline,
                pre_hello_speaker,
            )
            .await;
        } else {
            warn!(
                "Voice session {session_id}: received binary frame but no pipeline configured — \
                 STT/TTS providers must be set to use PCM mode"
            );
            let _ = ws_tx
                .send(Message::Text(
                    r#"{"type":"error","message":"PCM mode requires stt and tts configuration"}"#
                        .into(),
                ))
                .await;
        }
        return;
    }

    // ── Text mode ────────────────────────────────────────────────────────────

    // Send status
    let _ = send_json(
        &mut ws_tx,
        &ServerMessage::Status {
            state: "listening".to_string(),
        },
    )
    .await;

    // Spawn response forwarder (server → client)
    let sid_clone = session_id.clone();
    let forward_task = tokio::spawn(async move {
        while let Some(msg) = response_rx.recv().await {
            if let Ok(json) = serde_json::to_string(&msg) {
                if ws_tx.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
        }
        debug!("Response forwarder ended for {sid_clone}");
    });

    // Default sender (updated when Hello or first utterance arrives)
    let mut sender = ChannelUser {
        platform_id: session_id.clone(),
        display_name: "Caller".to_string(),
        openfang_user: None,
    };
    let mut primary_speaker: Option<String> = None;

    // Process first frame through the normal text handler
    process_text_frame(
        &first_frame,
        &mut sender,
        &mut primary_speaker,
        &session_id,
        &state,
        &response_tx,
    )
    .await;

    // Process incoming messages (client → server)
    while let Some(Ok(msg)) = ws_rx.next().await {
        if matches!(msg, Message::Close(_)) {
            info!("Voice session closed: {session_id}");
            break;
        }
        let should_break = process_text_frame(
            &msg,
            &mut sender,
            &mut primary_speaker,
            &session_id,
            &state,
            &response_tx,
        )
        .await;
        if should_break {
            break;
        }
    }

    // Cleanup
    forward_task.abort();
    {
        let mut sessions = state.sessions.write().await;
        sessions.remove(&session_id);
    }

    info!("Voice session cleaned up: {session_id}");
}

/// Process a single text-mode WebSocket frame.  Returns true if the session
/// should terminate (End message or bridge closed).
async fn process_text_frame(
    msg: &Message,
    sender: &mut ChannelUser,
    primary_speaker: &mut Option<String>,
    session_id: &str,
    state: &AppState,
    response_tx: &mpsc::Sender<ServerMessage>,
) -> bool {
    let text = match msg {
        Message::Text(t) => t.clone(),
        _ => return false,
    };

    let parsed: Result<ClientMessage, _> = serde_json::from_str(&text);
    match parsed {
        Ok(ClientMessage::Hello { speaker }) => {
            info!("Voice session {session_id}: speaker identified as {speaker:?}");
            sender.display_name = speaker.clone();
            *primary_speaker = Some(speaker);
        }
        Ok(ClientMessage::Utterance {
            text,
            speaker,
            r#final,
        }) => {
            // Per-utterance speaker overrides the session default
            if let Some(ref name) = speaker {
                sender.display_name = name.clone();
                *primary_speaker = Some(name.clone());
            }

            if !r#final {
                debug!("Interim utterance: {text}");
                return false;
            }

            if text.trim().is_empty() {
                return false;
            }

            let _ = response_tx
                .send(ServerMessage::Backchannel {
                    text: "Of course, sir.".to_string(),
                })
                .await;
            let _ = response_tx
                .send(ServerMessage::Status {
                    state: "thinking".to_string(),
                })
                .await;

            let prefixed_text = match primary_speaker {
                Some(name) => format!("[From: {name}] {text}"),
                None => text,
            };

            let mut metadata = HashMap::new();
            metadata.insert(
                "voice_session".to_string(),
                serde_json::Value::String(session_id.to_string()),
            );

            let channel_msg = ChannelMessage {
                channel: ChannelType::Custom("voice".to_string()),
                platform_message_id: format!(
                    "voice-{}-{}",
                    session_id,
                    Utc::now().timestamp_millis()
                ),
                sender: sender.clone(),
                content: ChannelContent::Text(prefixed_text),
                target_agent: None,
                timestamp: Utc::now(),
                is_group: false,
                thread_id: None,
                metadata,
            };

            if state.bridge_tx.send(channel_msg).await.is_err() {
                warn!("Bridge channel closed");
                return true;
            }

            {
                let mut s = state.status.write().await;
                s.messages_received += 1;
                s.last_message_at = Some(Utc::now());
            }
        }
        Ok(ClientMessage::Cancel) => {
            info!("Barge-in from text session {session_id}");
            let _ = response_tx.send(ServerMessage::BargeInAck).await;
            let _ = response_tx
                .send(ServerMessage::Status {
                    state: "listening".to_string(),
                })
                .await;
        }
        Ok(ClientMessage::End) => {
            info!("Voice session ended by client: {session_id}");
            return true;
        }
        Err(e) => {
            debug!("Invalid voice message: {e}");
            let _ = response_tx
                .send(ServerMessage::Error {
                    message: format!("Invalid message: {e}"),
                })
                .await;
        }
    }

    false
}

async fn send_json(
    tx: &mut futures::stream::SplitSink<WebSocket, Message>,
    msg: &ServerMessage,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string(msg)?;
    tx.send(Message::Text(json.into())).await?;
    Ok(())
}

// ── PCM Mode ─────────────────────────────────────────────────────────────────

/// How often (ms) to run Smart Turn inference while buffering audio.
const SMART_TURN_INTERVAL_MS: u64 = 300;
/// Silence-based fallback: how long (ms) of quiet after last speech triggers dispatch.
const SILENCE_TIMEOUT_MS: u64 = 1200;
/// Minimum audio buffer (ms) before Smart Turn or silence dispatch runs.
const MIN_AUDIO_MS: u64 = 500;
/// Sample rate.
const PCM_SAMPLE_RATE: usize = 16_000;
/// RMS threshold (Int16 units) for counting a chunk as "speech" in the silence fallback.
/// ~1% of max amplitude — low enough to catch normal speech, high enough to ignore mic noise.
const SILENCE_SPEECH_THRESHOLD: f64 = 300.0;

fn rms_i16(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum / samples.len() as f64).sqrt()
}

/// Handle a PCM-mode voice session.
///
/// `first_frame` is the binary frame that triggered PCM mode detection.
async fn handle_pcm_session(
    first_frame: Message,
    mut ws_tx: futures::stream::SplitSink<WebSocket, Message>,
    mut ws_rx: futures::stream::SplitStream<WebSocket>,
    state: AppState,
    session_id: String,
    pipeline: Arc<VoicePipeline>,
    pre_hello_speaker: Option<String>,
) {
    // Send listening status
    let _ = ws_tx
        .send(Message::Text(
            r#"{"type":"status","state":"listening"}"#.into(),
        ))
        .await;

    let initial_name = pre_hello_speaker
        .clone()
        .unwrap_or_else(|| "Caller".to_string());
    let mut sender = ChannelUser {
        platform_id: session_id.clone(),
        display_name: initial_name.clone(),
        openfang_user: None,
    };
    let mut primary_speaker: Option<String> = pre_hello_speaker.clone();
    if let Some(ref speaker) = pre_hello_speaker {
        info!("Voice session {session_id}: speaker identified as {speaker:?}");
        // Send Config frame now that we know the speaker
        let config_msg = serde_json::json!({
            "type": "config",
            "barge_in_threshold": pipeline.barge_in_threshold,
            "barge_in_speaking_threshold": pipeline.barge_in_speaking_threshold,
        })
        .to_string();
        let _ = ws_tx.send(Message::Text(config_msg.into())).await;
    }

    // Channel for agent text responses back to this PCM session.
    // Registered in pcm_sessions so VoiceAdapter::send() routes here → TTS.
    let (agent_tx, mut agent_rx) = mpsc::channel::<String>(16);
    {
        let mut pcm = state.pcm_sessions.write().await;
        pcm.insert(session_id.clone(), agent_tx.clone());
    }

    // Barge-in cancel signal: main loop sends true to skip current TTS.
    let (cancel_tx, cancel_rx_tts) = watch::channel(false);
    // Track sentences the agent has already spoken (capped at 3 for context).
    let spoken_sentences: Arc<tokio::sync::Mutex<Vec<String>>> =
        Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let spoken_sentences_tts = spoken_sentences.clone();
    // Context injected into the next utterance after a barge-in.
    let mut pending_barge_in_context: Option<String> = None;

    // Multi-utterance interrupt buffering:
    // If the user speaks again before the agent has responded, we buffer the new input
    // and discard the stale response when it arrives, then restart with all pending inputs.
    let mut waiting_for_response = false;
    let mut pending_inputs: Vec<String> = Vec::new(); // final_text for buffered utterances

    // Spawn agent response handler: collects text from bridge, synthesizes TTS.
    // Checks cancel_rx before and after synthesis to skip output during barge-in.
    let pipeline_for_tts = pipeline.clone();
    let sid_for_tts = session_id.clone();
    let (pcm_out_tx, mut pcm_out_rx) = mpsc::channel::<(Vec<i16>, String)>(8);
    tokio::spawn(async move {
        while let Some(text) = agent_rx.recv().await {
            if text.trim() == "[SILENT]" {
                continue;
            }
            // Skip synthesis if barge-in is active
            if *cancel_rx_tts.borrow() {
                continue;
            }
            debug!(
                "PCM session {sid_for_tts}: synthesizing TTS for: {}",
                &text[..text.len().min(80)]
            );
            match tts::synthesize(&text, &pipeline_for_tts.tts).await {
                Ok(pcm) => {
                    // Discard output if barge-in arrived during synthesis
                    if *cancel_rx_tts.borrow() {
                        continue;
                    }
                    {
                        let mut spoken = spoken_sentences_tts.lock().await;
                        spoken.push(text.clone());
                        if spoken.len() > 3 {
                            spoken.remove(0);
                        }
                    }
                    if pcm_out_tx.send((pcm, text.clone())).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    warn!("TTS error for PCM session {sid_for_tts}: {e}");
                }
            }
        }
    });

    // Main PCM loop
    let mut audio_buf: Vec<i16> = Vec::new();
    let mut last_smart_turn = tokio::time::Instant::now();
    let min_samples = (MIN_AUDIO_MS as usize * PCM_SAMPLE_RATE) / 1000;
    let smart_turn_interval = tokio::time::Duration::from_millis(SMART_TURN_INTERVAL_MS);
    let silence_timeout = tokio::time::Duration::from_millis(SILENCE_TIMEOUT_MS);
    // Silence fallback: absolute deadline, pushed forward on each speech chunk.
    // Initialized far in the future so it only fires after actual speech is detected —
    // prevents dispatching silence/noise during the opening seconds of a session.
    let far_future = tokio::time::Instant::now() + tokio::time::Duration::from_secs(3600);
    let mut silence_fire_at = far_future;

    // Process the first binary frame immediately
    if let Message::Binary(bytes) = first_frame {
        let samples: Vec<i16> = bytes
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        if rms_i16(&samples) > SILENCE_SPEECH_THRESHOLD {
            silence_fire_at = tokio::time::Instant::now() + silence_timeout;
        }
        audio_buf.extend_from_slice(&samples);
    }

    loop {
        tokio::select! {
            // Incoming audio from client
            frame = ws_rx.next() => {
                match frame {
                    Some(Ok(Message::Text(t))) => {
                        // Handle control frames (Hello, Cancel, End) in PCM mode
                        let parsed: Result<ClientMessage, _> = serde_json::from_str(&t);
                        match parsed {
                            Ok(ClientMessage::Hello { speaker }) => {
                                info!("PCM session {session_id}: speaker identified as {speaker:?}");
                                sender.display_name = speaker.clone();
                                primary_speaker = Some(speaker);
                                // Send session config (barge-in thresholds) to client
                                let config_msg = serde_json::json!({
                                    "type": "config",
                                    "barge_in_threshold": pipeline.barge_in_threshold,
                                    "barge_in_speaking_threshold": pipeline.barge_in_speaking_threshold,
                                })
                                .to_string();
                                let _ = ws_tx.send(Message::Text(config_msg.into())).await;
                            }
                            Ok(ClientMessage::Cancel) => {
                                info!("Barge-in from PCM session {session_id}");
                                // Signal TTS task to skip pending synthesis
                                cancel_tx.send(true).ok();
                                // Drain any already-queued PCM frames
                                while pcm_out_rx.try_recv().is_ok() {}
                                // Discard buffered inputs — user is starting fresh
                                pending_inputs.clear();
                                waiting_for_response = false;
                                // Build context from what was spoken
                                let spoken_ctx = {
                                    let s = spoken_sentences.lock().await;
                                    s.join(" ")
                                };
                                if !spoken_ctx.is_empty() {
                                    pending_barge_in_context =
                                        Some(format!("[Agent had said: \"{spoken_ctx}\"]"));
                                }
                                spoken_sentences.lock().await.clear();
                                // Discard stale pre-barge-in audio
                                audio_buf.clear();
                                let _ = ws_tx
                                    .send(Message::Text(r#"{"type":"barge_in_ack"}"#.into()))
                                    .await;
                                let _ = ws_tx
                                    .send(Message::Text(
                                        r#"{"type":"status","state":"listening"}"#.into(),
                                    ))
                                    .await;
                            }
                            Ok(ClientMessage::End) => {
                                info!("PCM session ended by client: {session_id}");
                                break;
                            }
                            _ => {}
                        }
                    }
                    Some(Ok(Message::Binary(bytes))) => {
                        let samples: Vec<i16> = bytes
                            .chunks_exact(2)
                            .map(|c| i16::from_le_bytes([c[0], c[1]]))
                            .collect();
                        // Silence fallback: push deadline forward when speech is detected
                        if pipeline.smart_turn.is_none()
                            && rms_i16(&samples) > SILENCE_SPEECH_THRESHOLD
                        {
                            silence_fire_at = tokio::time::Instant::now() + silence_timeout;
                        }
                        audio_buf.extend_from_slice(&samples);

                        // Run Smart Turn at interval
                        if last_smart_turn.elapsed() >= smart_turn_interval
                            && audio_buf.len() >= min_samples
                        {
                            last_smart_turn = tokio::time::Instant::now();
                            let complete = if let Some(ref detector) = pipeline.smart_turn {
                                let (complete, prob) = detector.predict(&audio_buf);
                                debug!("Smart Turn: complete={complete} prob={prob:.3}");
                                complete
                            } else {
                                // No model — use silence timeout as fallback
                                false
                            };

                            if complete {
                                if let Some((_disp, final_text)) = transcribe_utterance(
                                    &audio_buf,
                                    &pipeline,
                                    &primary_speaker,
                                    &mut pending_barge_in_context,
                                    &session_id,
                                    &mut ws_tx,
                                )
                                .await
                                {
                                    if waiting_for_response {
                                        pending_inputs.push(final_text);
                                    } else {
                                        send_utterance_to_agent(final_text, &state, &sender, &cancel_tx, &session_id).await;
                                        waiting_for_response = true;
                                    }
                                }
                                audio_buf.clear();
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        info!("PCM session closed: {session_id}");
                        break;
                    }
                    _ => {}
                }
            }

            // Silence timeout — flush buffer when no speech heard for SILENCE_TIMEOUT_MS.
            // Uses sleep_until(absolute deadline) so the timer survives across loop
            // iterations without resetting — unlike sleep(duration) which would reset
            // on every incoming audio frame and never fire.
            _ = tokio::time::sleep_until(silence_fire_at), if pipeline.smart_turn.is_none() && !audio_buf.is_empty() && audio_buf.len() >= min_samples => {
                if let Some((_disp, final_text)) = transcribe_utterance(
                    &audio_buf,
                    &pipeline,
                    &primary_speaker,
                    &mut pending_barge_in_context,
                    &session_id,
                    &mut ws_tx,
                )
                .await
                {
                    if waiting_for_response {
                        pending_inputs.push(final_text);
                    } else {
                        send_utterance_to_agent(final_text, &state, &sender, &cancel_tx, &session_id).await;
                        waiting_for_response = true;
                    }
                }
                audio_buf.clear();
                // Reset to far future — wait for new speech before next dispatch
                silence_fire_at = far_future;
            }

            // Outgoing TTS audio to client
            Some((pcm, response_text)) = pcm_out_rx.recv() => {
                if !pending_inputs.is_empty() {
                    // User spoke while agent was thinking — discard this stale response.
                    // Inject the discarded response as context so the model knows it answered
                    // but the user never heard it (see RightNow-AI/openfang#974 for proper fix).
                    cancel_tx.send(true).ok();
                    while pcm_out_rx.try_recv().is_ok() {}
                    let n = pending_inputs.len();
                    let stale_ctx = if response_text.trim().is_empty() {
                        String::new()
                    } else {
                        format!("[You had responded: \"{}\" but the user spoke again before hearing it]\n\n", response_text.trim())
                    };
                    let combined = format!("{stale_ctx}{}", std::mem::take(&mut pending_inputs).join("\n"));
                    info!("PCM session {session_id}: discarding stale response, replaying {n} buffered input(s)");
                    send_utterance_to_agent(combined, &state, &sender, &cancel_tx, &session_id).await;
                    // waiting_for_response stays true
                } else {
                    // Normal path: send response text and PCM audio to client
                    waiting_for_response = false;
                    let resp_msg = serde_json::json!({
                        "type": "response",
                        "text": response_text,
                        "sentence_end": true,
                    }).to_string();
                    let _ = ws_tx.send(Message::Text(resp_msg.into())).await;
                    let bytes = i16_to_bytes(&pcm);
                    if ws_tx.send(Message::Binary(bytes.into())).await.is_err() {
                        break;
                    }
                    let _ = ws_tx
                        .send(Message::Text(r#"{"type":"status","state":"listening"}"#.into()))
                        .await;
                }
            }
        }
    }

    // Cleanup
    {
        let mut pcm = state.pcm_sessions.write().await;
        pcm.remove(&session_id);
    }
    info!("PCM session cleaned up: {session_id}");
}

/// Transcribe PCM audio and send transcript to the client.
/// Returns `Some((display_text, final_text))` on success, `None` if empty or error.
/// `display_text` is the clean user-visible text; `final_text` has speaker prefix + barge-in context.
async fn transcribe_utterance(
    audio_buf: &[i16],
    pipeline: &VoicePipeline,
    primary_speaker: &Option<String>,
    pending_barge_in_context: &mut Option<String>,
    session_id: &str,
    ws_tx: &mut futures::stream::SplitSink<WebSocket, Message>,
) -> Option<(String, String)> {
    // Send "thinking" status immediately so the user knows we heard them
    let _ = ws_tx
        .send(Message::Text(
            r#"{"type":"status","state":"thinking"}"#.into(),
        ))
        .await;

    // display_text: clean STT output for showing in the chat UI (no agent context)
    // final_text: full text sent to the agent (with speaker prefix, barge-in context)
    let (display_text, final_text) = match stt::transcribe(audio_buf, &pipeline.stt).await {
        Ok(stt::TranscriptResult::Plain(t)) if !t.trim().is_empty() => {
            let clean = t.trim().to_string();
            let tagged = match primary_speaker {
                Some(name) => format!("[From: {name}] {clean}"),
                None => clean.clone(),
            };
            (clean, tagged)
        }
        Ok(stt::TranscriptResult::Diarized(segments)) if !segments.is_empty() => {
            let tagged = segments
                .iter()
                .map(|seg| {
                    let name = if seg.speaker_index == 0 {
                        primary_speaker
                            .clone()
                            .unwrap_or_else(|| "Speaker 0".to_string())
                    } else {
                        format!("Speaker {}", seg.speaker_index)
                    };
                    format!("[From: {name}] {}", seg.text)
                })
                .collect::<Vec<_>>()
                .join("\n");
            let clean = segments
                .iter()
                .map(|seg| seg.text.trim().to_string())
                .collect::<Vec<_>>()
                .join(" ");
            (clean, tagged)
        }
        Ok(_) => {
            // Empty transcript — resume listening
            let _ = ws_tx
                .send(Message::Text(
                    r#"{"type":"status","state":"listening"}"#.into(),
                ))
                .await;
            return None;
        }
        Err(e) => {
            warn!("STT error for PCM session {session_id}: {e}");
            let msg = serde_json::json!({"type":"error","message": e.to_string()}).to_string();
            let _ = ws_tx.send(Message::Text(msg.into())).await;
            let _ = ws_tx
                .send(Message::Text(
                    r#"{"type":"status","state":"listening"}"#.into(),
                ))
                .await;
            return None;
        }
    };

    // Prepend barge-in context if the user interrupted the previous response
    let final_text = if let Some(ctx) = pending_barge_in_context.take() {
        format!("{ctx}\n\nUser said: {final_text}")
    } else {
        final_text
    };

    // Send transcript to client: display_text for the chat UI, text for the agent
    let transcript_msg =
        serde_json::json!({"type": "transcribed", "text": final_text, "display_text": display_text})
            .to_string();
    let _ = ws_tx.send(Message::Text(transcript_msg.into())).await;

    Some((display_text, final_text))
}

/// Send a transcribed utterance to the agent bridge.
async fn send_utterance_to_agent(
    final_text: String,
    state: &AppState,
    sender: &ChannelUser,
    cancel_tx: &watch::Sender<bool>,
    session_id: &str,
) {
    // Reset barge-in cancel so TTS processes the upcoming response normally
    cancel_tx.send(false).ok();

    let mut metadata = HashMap::new();
    metadata.insert(
        "voice_session".to_string(),
        serde_json::Value::String(session_id.to_string()),
    );
    metadata.insert("pcm_mode".to_string(), serde_json::Value::Bool(true));

    let channel_msg = ChannelMessage {
        channel: ChannelType::Custom("voice".to_string()),
        platform_message_id: format!("voice-{}-{}", session_id, Utc::now().timestamp_millis()),
        sender: sender.clone(),
        content: ChannelContent::Text(final_text),
        target_agent: None,
        timestamp: Utc::now(),
        is_group: false,
        thread_id: None,
        metadata,
    };

    if state.bridge_tx.send(channel_msg).await.is_err() {
        warn!("Bridge closed for PCM session {session_id}");
        return;
    }

    {
        let mut s = state.status.write().await;
        s.messages_received += 1;
        s.last_message_at = Some(Utc::now());
    }
}

// ── Emotion Parsing ──────────────────────────────────────────────────────────

/// Parse an emotion tag from the start of a sentence.
/// Returns (emotion, clean_text). Unknown tags (e.g., `[SENTIMENT: warm]`)
/// are stripped and default to neutral.
fn parse_emotion_tag(text: &str) -> (&str, String) {
    let trimmed = text.trim();
    if trimmed.starts_with('[') {
        if let Some(end) = trimmed.find(']') {
            let tag = &trimmed[1..end];
            let rest = trimmed[end + 1..].trim().to_string();
            // Check for known emotion (exact match)
            let tag_lower = tag.to_lowercase();
            if let Some(emotion) = KNOWN_EMOTIONS.iter().find(|&&e| e == tag_lower) {
                return (emotion, rest);
            }
            // Unknown tag — strip it, use default emotion
            return (DEFAULT_EMOTION, rest);
        }
    }
    (DEFAULT_EMOTION, trimmed.to_string())
}

/// Split text into sentences for sentence-by-sentence delivery.
/// A sentence boundary is `.` `!` or `?` followed by a space and an uppercase
/// letter (or `[` for emotion tags), or at end of text. This avoids splitting
/// on abbreviations like "p.m.", "Dr.", "e.g.".
fn split_into_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let bytes = text.as_bytes();
    let mut start = 0;

    let mut i = 0;
    while i < bytes.len() {
        if matches!(bytes[i], b'.' | b'!' | b'?') {
            // End of text?
            if i + 1 >= bytes.len() {
                let s = text[start..=i].trim().to_string();
                if !s.is_empty() {
                    sentences.push(s);
                }
                start = i + 1;
            }
            // Followed by space + uppercase or bracket?
            else if i + 2 < bytes.len()
                && bytes[i + 1] == b' '
                && (bytes[i + 2].is_ascii_uppercase()
                    || bytes[i + 2] == b'['
                    || bytes[i + 2] == b'"')
            {
                let s = text[start..=i].trim().to_string();
                if !s.is_empty() {
                    sentences.push(s);
                }
                start = i + 2; // skip the space
            }
        }
        i += 1;
    }

    // Remaining text
    let remaining = text[start..].trim().to_string();
    if !remaining.is_empty() {
        sentences.push(remaining);
    }

    sentences
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_emotion_known() {
        let (emotion, text) = parse_emotion_tag("[amused] Very good, sir.");
        assert_eq!(emotion, "amused");
        assert_eq!(text, "Very good, sir.");
    }

    #[test]
    fn test_parse_emotion_unknown_stripped() {
        let (emotion, text) = parse_emotion_tag("[slightly_perturbed] I shall investigate.");
        assert_eq!(emotion, "neutral");
        assert_eq!(text, "I shall investigate.");
    }

    #[test]
    fn test_parse_emotion_no_tag() {
        let (emotion, text) = parse_emotion_tag("Very good, sir.");
        assert_eq!(emotion, "neutral");
        assert_eq!(text, "Very good, sir.");
    }

    #[test]
    fn test_parse_emotion_case_insensitive() {
        let (emotion, text) = parse_emotion_tag("[CONCERNED] I see a problem.");
        assert_eq!(emotion, "concerned");
        assert_eq!(text, "I see a problem.");
    }

    #[test]
    fn test_split_into_sentences() {
        let sentences =
            split_into_sentences("Very good, sir. I shall check your calendar. One moment!");
        assert_eq!(sentences.len(), 3);
        assert_eq!(sentences[0], "Very good, sir.");
        assert_eq!(sentences[1], "I shall check your calendar.");
        assert_eq!(sentences[2], "One moment!");
    }

    #[test]
    fn test_split_into_sentences_no_punctuation() {
        let sentences = split_into_sentences("Just a fragment");
        assert_eq!(sentences.len(), 1);
        assert_eq!(sentences[0], "Just a fragment");
    }

    #[test]
    fn test_split_into_sentences_abbreviations() {
        let sentences = split_into_sentences("It is 3 p.m., sir. Shall I proceed?");
        assert_eq!(sentences.len(), 2);
        assert_eq!(sentences[0], "It is 3 p.m., sir.");
        assert_eq!(sentences[1], "Shall I proceed?");
    }

    #[test]
    fn test_split_into_sentences_with_emotion_tags() {
        let sentences = split_into_sentences(
            "[amused] Very good, sir. [concerned] However, there is an issue.",
        );
        assert_eq!(sentences.len(), 2);
        assert!(sentences[0].contains("Very good"));
        assert!(sentences[1].contains("issue"));
    }

    #[test]
    fn test_all_known_emotions() {
        for emotion in KNOWN_EMOTIONS {
            let input = format!("[{emotion}] Test text.");
            let (parsed, text) = parse_emotion_tag(&input);
            assert_eq!(parsed, *emotion, "Failed for emotion: {emotion}");
            assert_eq!(text, "Test text.");
        }
    }

    #[test]
    fn test_adapter_creation() {
        let adapter = VoiceAdapter::new("0.0.0.0:4201".to_string(), Some("assistant".to_string()));
        assert_eq!(adapter.name(), "voice");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("voice".to_string())
        );
    }
}
