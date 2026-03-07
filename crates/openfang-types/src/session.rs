//! Conversation session with message history.

use crate::agent::{AgentId, SessionId};
use crate::message::Message;
use serde::{Deserialize, Serialize};

/// A conversation session with message history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Session ID.
    pub id: SessionId,
    /// Owning agent ID.
    pub agent_id: AgentId,
    /// Conversation messages.
    pub messages: Vec<Message>,
    /// Estimated token count for the context window.
    pub context_window_tokens: u64,
    /// Optional human-readable session label.
    pub label: Option<String>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            id: SessionId::new(),
            agent_id: AgentId::default(),
            messages: Vec::new(),
            context_window_tokens: 0,
            label: None,
        }
    }
}

/// Trait for types that can persist sessions.
pub trait SessionPersistence {
    /// Save a session to the backing store.
    fn save_session(&self, session: &Session) -> crate::error::OpenFangResult<()>;
}

impl Session {
    /// Create a new empty session for an agent.
    pub fn new(agent_id: AgentId) -> Self {
        Self {
            id: SessionId::new(),
            agent_id,
            messages: Vec::new(),
            context_window_tokens: 0,
            label: None,
        }
    }

    /// Create a new session with a label.
    pub fn with_label(agent_id: AgentId, label: impl Into<String>) -> Self {
        Self {
            id: SessionId::new(),
            agent_id,
            messages: Vec::new(),
            context_window_tokens: 0,
            label: Some(label.into()),
        }
    }
}
