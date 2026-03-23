//! Voice channel adapter.
//!
//! Provides a WebSocket server that accepts voice clients (mobile apps, Meet
//! bots, web clients). Clients handle STT and TTS directly with providers
//! (Deepgram, Cartesia); this adapter only exchanges text.
//!
//! # Protocol
//!
//! Text WebSocket frames, JSON messages:
//!
//! ```text
//! Client → Server:
//!   { "type": "utterance", "text": "...", "speaker": "Philippe", "final": true }
//!   { "type": "cancel" }
//!   { "type": "end" }
//!
//! Server → Client:
//!   { "type": "response", "text": "...", "emotion": "neutral", "sentence_end": true }
//!   { "type": "backchannel", "text": "I see, sir." }
//!   { "type": "status", "state": "thinking" | "listening" }
//!   { "type": "error", "message": "..." }
//! ```
//!
//! # Emotion Tags
//!
//! The agent may prefix sentences with emotion tags like `[amused]`, which
//! the adapter parses, validates against a known set, and sends as a structured
//! `emotion` field. Unknown tags default to `neutral`.

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
    /// Address to bind the WebSocket server.
    listen_addr: String,
    /// Default agent to route voice messages to.
    #[allow(dead_code)]
    default_agent: Option<String>,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Active sessions: session_id → response sender.
    sessions: Arc<RwLock<HashMap<String, mpsc::Sender<ServerMessage>>>>,
    /// Status tracking.
    status: Arc<RwLock<ChannelStatus>>,
    /// Receiver for async delegation results (set by channel bridge at startup).
    #[allow(clippy::type_complexity)]
    async_result_rx: Arc<tokio::sync::Mutex<Option<mpsc::Receiver<(String, String)>>>>,
}

impl VoiceAdapter {
    /// Create a new voice adapter.
    pub fn new(listen_addr: String, default_agent: Option<String>) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            listen_addr,
            default_agent,
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            status: Arc::new(RwLock::new(ChannelStatus::default())),
            async_result_rx: Arc::new(tokio::sync::Mutex::new(None)),
        }
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
    /// Active sessions.
    sessions: Arc<RwLock<HashMap<String, mpsc::Sender<ServerMessage>>>>,
    /// Status tracking.
    status: Arc<RwLock<ChannelStatus>>,
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

        let (bridge_tx, bridge_rx) = mpsc::channel::<ChannelMessage>(256);

        let state = AppState {
            bridge_tx,
            sessions: self.sessions.clone(),
            status: self.status.clone(),
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

        // Find the session — try exact platform_id match first,
        // then fall back to any active session (for async callbacks
        // that arrive without channel context).
        let sessions = self.sessions.read().await;
        let tx = if let Some(tx) = sessions.get(&user.platform_id) {
            tx.clone()
        } else if sessions.len() == 1 {
            // Only one session — route to it (async callback case)
            sessions.values().next().unwrap().clone()
        } else if !sessions.is_empty() {
            // Multiple sessions — pick the first (best effort)
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

    // Default sender (updated when first utterance arrives with speaker info)
    let mut sender = ChannelUser {
        platform_id: session_id.clone(),
        display_name: "Caller".to_string(),
        openfang_user: None,
    };

    // Process incoming messages (client → server)
    while let Some(Ok(msg)) = ws_rx.next().await {
        match msg {
            Message::Text(text) => {
                let parsed: Result<ClientMessage, _> = serde_json::from_str(&text);
                match parsed {
                    Ok(ClientMessage::Utterance {
                        text,
                        speaker,
                        r#final,
                    }) => {
                        // Update sender info if speaker provided
                        if let Some(ref name) = speaker {
                            sender.display_name = name.clone();
                        }

                        if !r#final {
                            // Interim utterance — could trigger backchannel in future
                            debug!("Interim utterance: {text}");
                            continue;
                        }

                        if text.trim().is_empty() {
                            continue;
                        }

                        // Send immediate acknowledgment so the client can
                        // start TTS while the agent/delegation processes.
                        // This is spoken as "thinking..." feedback.
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

                        // Build ChannelMessage and send to bridge
                        let mut metadata = HashMap::new();
                        if let Some(ref name) = speaker {
                            metadata.insert(
                                "sender_email".to_string(),
                                serde_json::Value::String(name.clone()),
                            );
                        }
                        metadata.insert(
                            "voice_session".to_string(),
                            serde_json::Value::String(session_id.clone()),
                        );

                        let channel_msg = ChannelMessage {
                            channel: ChannelType::Custom("voice".to_string()),
                            platform_message_id: format!(
                                "voice-{}-{}",
                                session_id,
                                Utc::now().timestamp_millis()
                            ),
                            sender: sender.clone(),
                            content: ChannelContent::Text(text),
                            target_agent: None,
                            timestamp: Utc::now(),
                            is_group: false,
                            thread_id: None,
                            metadata,
                        };

                        if state.bridge_tx.send(channel_msg).await.is_err() {
                            warn!("Bridge channel closed");
                            break;
                        }

                        {
                            let mut s = state.status.write().await;
                            s.messages_received += 1;
                            s.last_message_at = Some(Utc::now());
                        }
                    }
                    Ok(ClientMessage::Cancel) => {
                        info!("Barge-in cancel from {session_id}");
                        // TODO: call kernel stop_run for the agent
                    }
                    Ok(ClientMessage::End) => {
                        info!("Voice session ended by client: {session_id}");
                        break;
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
            }
            Message::Close(_) => {
                info!("Voice session closed: {session_id}");
                break;
            }
            _ => {}
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

async fn send_json(
    tx: &mut futures::stream::SplitSink<WebSocket, Message>,
    msg: &ServerMessage,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string(msg)?;
    tx.send(Message::Text(json.into())).await?;
    Ok(())
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
