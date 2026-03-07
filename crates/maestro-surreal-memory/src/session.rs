//! Session types for the SurrealDB memory substrate.

use openfang_types::agent::{AgentId, SessionId};
use openfang_types::message::Message;

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Session {
    pub id: SessionId,
    pub agent_id: AgentId,
    pub messages: Vec<Message>,
    pub context_window_tokens: u64,
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

impl Session {
    pub fn new(agent_id: AgentId) -> Self {
        Self {
            id: SessionId::new(),
            agent_id,
            messages: Vec::new(),
            context_window_tokens: 0,
            label: None,
        }
    }

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