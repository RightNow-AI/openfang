//! Team message types for inter-agent communication via the blackboard mailbox.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

fn default_now() -> DateTime<Utc> {
    Utc::now()
}

/// The intent of a message posted to the team mailbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamMessageKind {
    /// Worker proposes taking on a task.
    Propose,
    /// Worker claims a task from the ready queue.
    Claim,
    /// Coordinator assigns a specific task to a specific worker.
    Assign,
    /// Worker accepts an assigned task.
    Accept,
    /// Worker rejects an assignment (must include reason in summary).
    Reject,
    /// Worker reports incremental progress.
    Progress,
    /// Worker is blocked and cannot continue without coordinator help.
    Blocked,
    /// Worker needs a decision or input before continuing.
    NeedInput,
    /// Worker is passing incomplete work to another agent.
    Handoff,
    /// Worker reports the task is fully done and output contract is satisfied.
    Complete,
    /// Coordinator or worker cancels a task.
    Cancel,
}

/// A message exchanged between agents through the shared blackboard mailbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMessage {
    /// Unique message identifier.
    pub msg_id: String,
    /// Sending agent ID.
    pub from_agent: String,
    /// Receiving agent ID.  Use `"*"` to broadcast to all agents.
    pub to_agent: String,
    /// Task this message relates to.
    pub task_id: String,
    /// Semantic intent of the message.
    pub kind: TeamMessageKind,
    /// Human-readable one-line summary.
    pub summary: String,
    /// Optional structured payload (schema depends on `kind`).
    pub payload_json: serde_json::Value,
    /// Wall-clock time the message was posted.
    #[serde(default = "default_now")]
    pub timestamp: DateTime<Utc>,
}

impl TeamMessage {
    /// Create a new message with a generated UUID.
    pub fn new(
        from_agent: impl Into<String>,
        to_agent: impl Into<String>,
        task_id: impl Into<String>,
        kind: TeamMessageKind,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            msg_id: uuid::Uuid::new_v4().to_string(),
            from_agent: from_agent.into(),
            to_agent: to_agent.into(),
            task_id: task_id.into(),
            kind,
            summary: summary.into(),
            payload_json: serde_json::Value::Null,
            timestamp: Utc::now(),
        }
    }

    /// Attach a structured JSON payload to this message.
    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload_json = payload;
        self
    }
}
