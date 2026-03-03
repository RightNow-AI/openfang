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
