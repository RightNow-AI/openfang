//! Core channel bridge types.

use chrono::{DateTime, Utc};
use openfang_types::agent::AgentId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;

/// The type of messaging channel.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChannelType {
    Telegram,
    WhatsApp,
    Slack,
    Discord,
    Signal,
    Matrix,
    Email,
    Teams,
    Mattermost,
    WebChat,
    CLI,
    /// MQTT pub/sub messaging.
    Mqtt,
    Custom(String),
}

/// A user on a messaging platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelUser {
    /// Platform-specific user ID.
    pub platform_id: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Optional mapping to an OpenFang user identity.
    pub openfang_user: Option<String>,
}

/// Content types that can be received from a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChannelContent {
    Text(String),
    Image {
        url: String,
        caption: Option<String>,
    },
    File {
        url: String,
        filename: String,
        /// Best-effort MIME type from the source platform (e.g. Discord's
        /// `attachments[].content_type`). `None` if the platform did not
        /// provide one; downstream consumers may sniff bytes or fall back
        /// to extension-based detection.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mime: Option<String>,
        /// Size in bytes, when known. Useful for capacity gating before
        /// the bridge attempts to materialize or transmit the file.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        size: Option<u64>,
    },
    /// Local file data (bytes read from disk). Used by the proactive `channel_send`
    /// tool when `file_path` is provided instead of `file_url`.
    FileData {
        data: Vec<u8>,
        filename: String,
        mime_type: String,
    },
    Voice {
        url: String,
        duration_seconds: u32,
    },
    Location {
        lat: f64,
        lon: f64,
    },
    Command {
        name: String,
        args: Vec<String>,
    },
    /// A composite message carrying multiple content blocks (e.g. a Discord
    /// message with several attachments, or an image with a separate file
    /// sibling). Blocks are flat-mapped by the bridge into multiple LLM
    /// content blocks. Implementations should not produce nested `Multipart`
    /// values; consumers may `debug_assert!` against nesting.
    Multipart(Vec<ChannelContent>),
    /// An interactive message: a text body plus one or more action buttons.
    /// Discord renders this as a message with an action row; adapters that do
    /// not override [`ChannelAdapter::supports_interactive`] degrade to the
    /// `text` body alone (callers MUST keep `text` self-sufficient, e.g. it
    /// still contains `/approve <id>` for the #2a text path). `buttons` carry
    /// an opaque `custom_id` only — never authorization (ANAI-82).
    Interactive {
        text: String,
        buttons: Vec<InteractiveButton>,
    },
}

/// A unified message from any channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelMessage {
    /// Which channel this came from.
    pub channel: ChannelType,
    /// Platform-specific message identifier.
    pub platform_message_id: String,
    /// Who sent this message.
    pub sender: ChannelUser,
    /// The message content.
    pub content: ChannelContent,
    /// Optional target agent (if routed directly).
    pub target_agent: Option<AgentId>,
    /// When the message was sent.
    pub timestamp: DateTime<Utc>,
    /// Whether this message is from a group chat (vs DM).
    #[serde(default)]
    pub is_group: bool,
    /// Thread ID for threaded conversations (platform-specific).
    #[serde(default)]
    pub thread_id: Option<String>,
    /// Arbitrary platform metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

// Re-export the adapter allowlist from openfang-types so config validation
// and routing share a single source of truth (no drift between the two).
pub use openfang_types::config::CHANNELS_WITH_PLATFORM_ID_AS_CHANNEL;

impl ChannelMessage {
    /// Return the platform-native channel/conversation ID for this message,
    /// suitable for matching against an `AgentBinding`'s `channel_id` field.
    ///
    /// Resolution order:
    /// 1. For adapters in [`CHANNELS_WITH_PLATFORM_ID_AS_CHANNEL`],
    ///    `sender.platform_id` already *is* the channel ID (these adapters
    ///    overload the field because it doubles as the send target).
    /// 2. Otherwise, fall back to `metadata["channel_id"]` if present (any
    ///    adapter can opt in by populating that key).
    /// 3. Otherwise, `None`.
    ///
    /// This is the central routing-time accessor — config validation and the
    /// router both consult it (directly or via the same allowlist) so the two
    /// cannot drift.
    pub fn channel_id(&self) -> Option<String> {
        // For builtin variants the string is already lowercase by construction.
        // For `Custom(s)`, adapters _should_ register lowercase names but we
        // case-fold here so a stray `Custom("Twitch")` cannot silently slip
        // past the allowlist (and out of step with the validation path, which
        // already lowercases user input). Allocates only on the Custom arm.
        let channel_str: std::borrow::Cow<'_, str> = match &self.channel {
            ChannelType::Telegram => "telegram".into(),
            ChannelType::Discord => "discord".into(),
            ChannelType::Slack => "slack".into(),
            ChannelType::WhatsApp => "whatsapp".into(),
            ChannelType::Signal => "signal".into(),
            ChannelType::Matrix => "matrix".into(),
            ChannelType::Email => "email".into(),
            ChannelType::Teams => "teams".into(),
            ChannelType::Mattermost => "mattermost".into(),
            ChannelType::WebChat => "webchat".into(),
            ChannelType::CLI => "cli".into(),
            ChannelType::Mqtt => "mqtt".into(),
            ChannelType::Custom(s) => s.to_lowercase().into(),
        };
        if CHANNELS_WITH_PLATFORM_ID_AS_CHANNEL
            .iter()
            .any(|c| *c == channel_str.as_ref())
        {
            Some(self.sender.platform_id.clone())
        } else {
            self.metadata
                .get("channel_id")
                .and_then(|v| v.as_str())
                .map(String::from)
        }
    }
}

/// Agent lifecycle phase for UX indicators.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AgentPhase {
    /// Message is queued, waiting for agent.
    Queued,
    /// Agent is calling the LLM.
    Thinking,
    /// Agent is executing a tool.
    ToolUse {
        /// Tool being executed (max 64 chars, sanitized).
        tool_name: String,
    },
    /// Agent is streaming tokens.
    Streaming,
    /// Agent finished successfully.
    Done,
    /// Agent encountered an error.
    Error,
}

impl AgentPhase {
    /// Sanitize a tool name for display (truncate to 64 chars, strip control chars).
    pub fn tool_use(name: &str) -> Self {
        let sanitized: String = name.chars().filter(|c| !c.is_control()).take(64).collect();
        Self::ToolUse {
            tool_name: sanitized,
        }
    }
}

/// Reaction to show in a channel (emoji-based).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleReaction {
    /// The agent phase this reaction represents.
    pub phase: AgentPhase,
    /// Channel-appropriate emoji.
    pub emoji: String,
    /// Whether to remove the previous phase reaction.
    pub remove_previous: bool,
}

/// Hardcoded emoji allowlist for lifecycle reactions.
pub const ALLOWED_REACTION_EMOJI: &[&str] = &[
    "\u{1F914}",        // 🤔 thinking
    "\u{2699}\u{FE0F}", // ⚙️ tool_use
    "\u{270D}\u{FE0F}", // ✍️ streaming
    "\u{2705}",         // ✅ done
    "\u{274C}",         // ❌ error
    "\u{23F3}",         // ⏳ queued
    "\u{1F504}",        // 🔄 processing
    "\u{1F440}",        // 👀 looking
];

/// Get the default emoji for a given agent phase.
pub fn default_phase_emoji(phase: &AgentPhase) -> &'static str {
    match phase {
        AgentPhase::Queued => "\u{23F3}",                 // ⏳
        AgentPhase::Thinking => "\u{1F914}",              // 🤔
        AgentPhase::ToolUse { .. } => "\u{2699}\u{FE0F}", // ⚙️
        AgentPhase::Streaming => "\u{270D}\u{FE0F}",      // ✍️
        AgentPhase::Done => "\u{2705}",                   // ✅
        AgentPhase::Error => "\u{274C}",                  // ❌
    }
}

/// Delivery status for outbound messages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    /// Message was sent to the channel API.
    Sent,
    /// Message was confirmed delivered to recipient.
    Delivered,
    /// Message delivery failed.
    Failed,
    /// Best-effort delivery (no confirmation available).
    BestEffort,
}

/// Receipt tracking outbound message delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryReceipt {
    /// Platform message ID (if available).
    pub message_id: String,
    /// Channel type this was sent through.
    pub channel: String,
    /// Sanitized recipient identifier (no PII).
    pub recipient: String,
    /// Delivery status.
    pub status: DeliveryStatus,
    /// When the delivery attempt occurred.
    pub timestamp: DateTime<Utc>,
    /// Error message (if failed — sanitized, no credentials).
    pub error: Option<String>,
}

/// Health status for a channel adapter.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelStatus {
    /// Whether the adapter is currently connected/running.
    pub connected: bool,
    /// When the adapter was started (ISO 8601).
    pub started_at: Option<DateTime<Utc>>,
    /// When the last message was received.
    pub last_message_at: Option<DateTime<Utc>>,
    /// Total messages received since start.
    pub messages_received: u64,
    /// Total messages sent since start.
    pub messages_sent: u64,
    /// Last error message (if any).
    pub last_error: Option<String>,
}

// Re-export policy/format types from openfang-types for convenience.
pub use openfang_types::config::{DmPolicy, GroupPolicy, OutputFormat};

/// Structured error returned by [`ChannelAdapter::resolve_recipient`] when
/// a recipient string cannot be turned into a platform-native identifier.
///
/// These variants are surfaced verbatim through `ToolError::RecipientUnresolved`
/// so the calling agent can distinguish "you typed it wrong" from "we know
/// who you mean but can't reach them" from "this is ambiguous, please qualify".
#[derive(Debug, Clone)]
pub enum ResolutionError {
    /// No channel or known user matches the recipient string.
    ///
    /// For Discord this typically means we have never seen an inbound
    /// MESSAGE_CREATE from the named user, or GUILD_CREATE has not yet
    /// landed channel metadata for the named channel.
    UnknownRecipient { recipient: String },

    /// A bare channel name (e.g. `"general"` or `"#general"`) matches more
    /// than one guild's channel list. The agent must qualify with the
    /// channel mention `<#…>` or upstream tooling.
    AmbiguousChannel { name: String, guilds: Vec<String> },

    /// A bare username (no leading `@` or `<@id>` form) was passed as a DM
    /// target. Refused by design to eliminate the username-collision class
    /// where a legitimate Discord user shares a username with someone the
    /// agent intends to message (see ANAI-55 security review, finding F1).
    BareNameDmRefused { name: String },

    /// The platform refused to open a DM channel with the resolved user
    /// (e.g. Discord returned 403 because DMs are closed or the bot is
    /// blocked). Fail-closed; no auto-retry.
    DmOpenFailed { user_id: String, status: u16 },
}

impl std::fmt::Display for ResolutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolutionError::UnknownRecipient { recipient } => write!(
                f,
                "No channel or known user matches `{recipient}`. The user must \
                 have messaged us at least once before they can be addressed by name."
            ),
            ResolutionError::AmbiguousChannel { name, guilds } => write!(
                f,
                "Channel `#{name}` exists in {} guilds ({}). Use the channel \
                 mention `<#…>` to disambiguate.",
                guilds.len(),
                guilds.join(", ")
            ),
            ResolutionError::BareNameDmRefused { name } => write!(
                f,
                "DM recipient must be qualified as `@{name}` or `<@user_id>`. \
                 Bare names are not resolved for DM safety."
            ),
            ResolutionError::DmOpenFailed { user_id, status } => write!(
                f,
                "Platform refused to open a DM channel with user {user_id} \
                 (status {status}). They may have DMs disabled or have blocked \
                 the bot."
            ),
        }
    }
}

impl std::error::Error for ResolutionError {}

/// Trait that every channel adapter must implement.
///
/// A channel adapter bridges a messaging platform to the OpenFang kernel by converting
/// platform-specific messages into `ChannelMessage` events and sending responses back.
#[async_trait]
pub trait ChannelAdapter: Send + Sync {
    /// Human-readable name of this adapter.
    fn name(&self) -> &str;

    /// The channel type this adapter handles.
    fn channel_type(&self) -> ChannelType;

    /// Start receiving messages. Returns a stream of incoming messages.
    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>;

    /// Send a response back to a user on this channel.
    async fn send(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>>;

    /// Whether this adapter can render [`ChannelContent::Interactive`]
    /// (action buttons). Default `false`: the dispatch path degrades
    /// interactive content to its text body before calling `send`. Discord
    /// overrides this to `true` (ANAI-82). Keeping the default `false` means
    /// the ~50 other adapters need no change and never receive a variant they
    /// cannot render.
    fn supports_interactive(&self) -> bool {
        false
    }

    /// Send a typing indicator (optional — default no-op).
    async fn send_typing(&self, _user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    /// Send a lifecycle reaction to a message (optional — default no-op).
    async fn send_reaction(
        &self,
        _user: &ChannelUser,
        _message_id: &str,
        _reaction: &LifecycleReaction,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    /// Stop the adapter and clean up resources.
    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>>;

    /// Get the current health status of this adapter (optional — default returns disconnected).
    fn status(&self) -> ChannelStatus {
        ChannelStatus::default()
    }

    /// Send a response as a thread reply (optional — default falls back to `send()`).
    async fn send_in_thread(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
        _thread_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send(user, content).await
    }

    /// Determine whether to auto-create a thread for an incoming message.
    /// Returns Some(thread_name) to create a thread, or None to reply directly.
    /// Default implementation returns None (no auto-threading).
    async fn should_auto_thread(&self, _message: &ChannelMessage) -> Option<String> {
        None
    }

    /// Create a new thread (typically triggered after should_auto_thread returns Some).
    /// Returns the new thread ID on success.
    async fn create_thread(
        &self,
        _user: &ChannelUser,
        _message_id: &str,
        _thread_name: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        Err("Thread creation not supported for this adapter".into())
    }

    /// Whether this adapter should suppress sending internal agent errors back to the user.
    ///
    /// Returns `true` for public broadcast channels (e.g. Mastodon) where posting
    /// an error message would create a public status update. Errors are always
    /// logged regardless of this setting.
    fn suppress_error_responses(&self) -> bool {
        false
    }

    /// Resolve a free-form recipient string into a platform-native
    /// [`ChannelUser`] before dispatch.
    ///
    /// The default implementation is **passthrough** — it wraps the input
    /// string as both `platform_id` and `display_name`. This preserves the
    /// pre-ANAI-55 behavior for every adapter that does not override the
    /// method (i.e. every adapter except Discord, until others opt in).
    ///
    /// Adapters that override this method should:
    /// - Accept platform-native IDs (e.g. Discord snowflakes) verbatim.
    /// - Accept the platform's mention/handle forms where they exist.
    /// - Fail closed with a structured [`ResolutionError`] on miss,
    ///   ambiguity, or any unsafe shorthand.
    /// - Never blast a message at a fallback recipient on resolution failure.
    ///
    /// See ANAI-55 and the channel-send-attachments proposal for the
    /// Discord-specific resolution matrix.
    async fn resolve_recipient(&self, recipient: &str) -> Result<ChannelUser, ResolutionError> {
        Ok(ChannelUser {
            platform_id: recipient.to_string(),
            display_name: recipient.to_string(),
            openfang_user: None,
        })
    }
}

/// Split a message into chunks of at most `max_len` characters,
/// preferring to split at newline boundaries.
///
/// Shared utility used by Telegram, Discord, and Slack adapters.
pub fn split_message(text: &str, max_len: usize) -> Vec<&str> {
    if text.len() <= max_len {
        return vec![text];
    }
    let mut chunks = Vec::new();
    let mut remaining = text;
    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining);
            break;
        }
        // Try to split at a newline near the boundary (UTF-8 safe)
        let safe_end = openfang_types::truncate_str(remaining, max_len).len();
        let split_at = remaining[..safe_end].rfind('\n').unwrap_or(safe_end);
        let (chunk, rest) = remaining.split_at(split_at);
        chunks.push(chunk);
        // Skip the newline (and optional \r) we split on
        remaining = rest
            .strip_prefix("\r\n")
            .or_else(|| rest.strip_prefix('\n'))
            .unwrap_or(rest);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_message_serialization() {
        let msg = ChannelMessage {
            channel: ChannelType::Telegram,
            platform_message_id: "123".to_string(),
            sender: ChannelUser {
                platform_id: "user1".to_string(),
                display_name: "Alice".to_string(),
                openfang_user: None,
            },
            content: ChannelContent::Text("Hello!".to_string()),
            target_agent: None,
            timestamp: Utc::now(),
            is_group: false,
            thread_id: None,
            metadata: HashMap::new(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: ChannelMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.channel, ChannelType::Telegram);
    }

    #[test]
    fn test_split_message_short() {
        assert_eq!(split_message("hello", 100), vec!["hello"]);
    }

    #[test]
    fn test_split_message_at_newlines() {
        let text = "line1\nline2\nline3";
        let chunks = split_message(text, 10);
        assert_eq!(chunks, vec!["line1", "line2", "line3"]);
    }

    #[test]
    fn test_channel_type_matrix_serde() {
        let ct = ChannelType::Matrix;
        let json = serde_json::to_string(&ct).unwrap();
        let back: ChannelType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ChannelType::Matrix);
    }

    #[test]
    fn test_channel_type_email_serde() {
        let ct = ChannelType::Email;
        let json = serde_json::to_string(&ct).unwrap();
        let back: ChannelType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ChannelType::Email);
    }

    #[test]
    fn test_channel_id_custom_arm_is_case_insensitive() {
        // A stray capitalized Custom variant must still resolve through the
        // allowlist. The validation path lowercases user input; the routing
        // path needs the same case-fold to stay in sync.
        let make = |name: &str| ChannelMessage {
            channel: ChannelType::Custom(name.to_string()),
            platform_message_id: "m".to_string(),
            sender: ChannelUser {
                platform_id: "C123".to_string(),
                display_name: "x".to_string(),
                openfang_user: None,
            },
            content: ChannelContent::Text("hi".to_string()),
            target_agent: None,
            timestamp: Utc::now(),
            is_group: false,
            thread_id: None,
            metadata: HashMap::new(),
        };
        assert_eq!(make("twitch").channel_id().as_deref(), Some("C123"));
        assert_eq!(make("Twitch").channel_id().as_deref(), Some("C123"));
        assert_eq!(make("TWITCH").channel_id().as_deref(), Some("C123"));
        // Lark spelling (Feishu Intl) must also match.
        assert_eq!(make("lark").channel_id().as_deref(), Some("C123"));
        assert_eq!(make("Lark").channel_id().as_deref(), Some("C123"));
    }

    #[test]
    fn test_channel_content_variants() {
        let text = ChannelContent::Text("hello".to_string());
        let cmd = ChannelContent::Command {
            name: "status".to_string(),
            args: vec![],
        };
        let loc = ChannelContent::Location {
            lat: 40.7128,
            lon: -74.0060,
        };

        // Just verify they serialize without panic
        serde_json::to_string(&text).unwrap();
        serde_json::to_string(&cmd).unwrap();
        serde_json::to_string(&loc).unwrap();
    }

    // ----- AgentPhase tests -----

    #[test]
    fn test_agent_phase_serde_roundtrip() {
        let phases = vec![
            AgentPhase::Queued,
            AgentPhase::Thinking,
            AgentPhase::tool_use("web_fetch"),
            AgentPhase::Streaming,
            AgentPhase::Done,
            AgentPhase::Error,
        ];
        for phase in &phases {
            let json = serde_json::to_string(phase).unwrap();
            let back: AgentPhase = serde_json::from_str(&json).unwrap();
            assert_eq!(*phase, back);
        }
    }

    #[test]
    fn test_agent_phase_tool_use_sanitizes() {
        let phase = AgentPhase::tool_use("hello\x00world\x01test");
        if let AgentPhase::ToolUse { tool_name } = phase {
            assert!(!tool_name.contains('\x00'));
            assert!(!tool_name.contains('\x01'));
            assert!(tool_name.contains("hello"));
        } else {
            panic!("Expected ToolUse variant");
        }
    }

    #[test]
    fn test_agent_phase_tool_use_truncates_long_name() {
        let long_name = "a".repeat(200);
        let phase = AgentPhase::tool_use(&long_name);
        if let AgentPhase::ToolUse { tool_name } = phase {
            assert!(tool_name.len() <= 64);
        }
    }

    #[test]
    fn test_default_phase_emoji() {
        assert_eq!(default_phase_emoji(&AgentPhase::Thinking), "\u{1F914}");
        assert_eq!(default_phase_emoji(&AgentPhase::Done), "\u{2705}");
        assert_eq!(default_phase_emoji(&AgentPhase::Error), "\u{274C}");
    }

    // ----- DeliveryReceipt tests -----

    #[test]
    fn test_delivery_status_serde() {
        let statuses = vec![
            DeliveryStatus::Sent,
            DeliveryStatus::Delivered,
            DeliveryStatus::Failed,
            DeliveryStatus::BestEffort,
        ];
        for status in &statuses {
            let json = serde_json::to_string(status).unwrap();
            let back: DeliveryStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(*status, back);
        }
    }

    #[test]
    fn test_delivery_receipt_serde() {
        let receipt = DeliveryReceipt {
            message_id: "msg-123".to_string(),
            channel: "telegram".to_string(),
            recipient: "user-456".to_string(),
            status: DeliveryStatus::Sent,
            timestamp: Utc::now(),
            error: None,
        };
        let json = serde_json::to_string(&receipt).unwrap();
        let back: DeliveryReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(back.message_id, "msg-123");
        assert_eq!(back.status, DeliveryStatus::Sent);
    }

    #[test]
    fn test_delivery_receipt_with_error() {
        let receipt = DeliveryReceipt {
            message_id: "msg-789".to_string(),
            channel: "slack".to_string(),
            recipient: "channel-abc".to_string(),
            status: DeliveryStatus::Failed,
            timestamp: Utc::now(),
            error: Some("Connection refused".to_string()),
        };
        let json = serde_json::to_string(&receipt).unwrap();
        assert!(json.contains("Connection refused"));
    }
}


/// A single button in a [`ChannelContent::Interactive`] action row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InteractiveButton {
    /// Opaque identifier echoed back verbatim on click (Discord `custom_id`).
    /// Encodes the approval request id + nonce; MUST NOT carry authorization
    /// (capability-in-URL antipattern — authz is the clicking user, checked
    /// server-side at resolve time). Discord caps this at 100 chars.
    pub custom_id: String,
    /// Visible button label.
    pub label: String,
    /// Visual style hint; mapped to a native style by the adapter.
    pub style: ButtonStyle,
}

/// Visual style for an [`InteractiveButton`]. Names are platform-neutral;
/// adapters map them to native styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ButtonStyle {
    Primary,
    Secondary,
    Success,
    Danger,
}

impl ButtonStyle {
    /// Discord Component button `style` integer (API v10):
    /// Primary=1, Secondary=2, Success=3, Danger=4.
    pub fn discord_style(self) -> u8 {
        match self {
            ButtonStyle::Primary => 1,
            ButtonStyle::Secondary => 2,
            ButtonStyle::Success => 3,
            ButtonStyle::Danger => 4,
        }
    }
}

impl ChannelContent {
    /// Collapse a richer variant to a plain-text equivalent for adapters that
    /// cannot render it. Used by the dispatch path to gracefully degrade an
    /// [`ChannelContent::Interactive`] to text on adapters whose
    /// [`ChannelAdapter::supports_interactive`] is `false`. All other variants
    /// pass through unchanged.
    pub fn degrade_to_text(self) -> ChannelContent {
        match self {
            ChannelContent::Interactive { text, .. } => ChannelContent::Text(text),
            other => other,
        }
    }
}
