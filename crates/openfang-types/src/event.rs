//! Event types for the OpenFang internal event bus.
//!
//! All inter-agent and system communication flows through events.

use crate::agent::AgentId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

/// Serde helper for `Option<Duration>` as milliseconds.
mod duration_ms {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    /// Serialize `Duration` as `u64` milliseconds.
    pub fn serialize<S: Serializer>(dur: &Option<Duration>, s: S) -> Result<S::Ok, S::Error> {
        match dur {
            Some(d) => d.as_millis().serialize(s),
            None => s.serialize_none(),
        }
    }

    /// Deserialize `u64` milliseconds into `Duration`.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Duration>, D::Error> {
        let opt: Option<u64> = Option::deserialize(d)?;
        Ok(opt.map(Duration::from_millis))
    }
}

/// Unique identifier for an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId(pub Uuid);

impl EventId {
    /// Create a new random EventId.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Where an event is directed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum EventTarget {
    /// Send to a specific agent.
    Agent(AgentId),
    /// Broadcast to all agents.
    Broadcast,
    /// Send to agents matching a pattern (e.g., tag-based).
    Pattern(String),
    /// Send to the kernel/system.
    System,
}

/// The payload of an event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum EventPayload {
    /// Direct agent-to-agent message.
    Message(AgentMessage),
    /// Tool execution result.
    ToolResult(ToolOutput),
    /// Memory changed notification.
    MemoryUpdate(MemoryDelta),
    /// Agent lifecycle event.
    Lifecycle(LifecycleEvent),
    /// Network event (remote agent activity).
    Network(NetworkEvent),
    /// System event (health, resources).
    System(SystemEvent),
    /// User-defined payload.
    Custom(Vec<u8>),
}

/// A message between agents or from user to agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    /// The text content of the message.
    pub content: String,
    /// Optional structured metadata.
    pub metadata: HashMap<String, serde_json::Value>,
    /// The role of the message sender.
    pub role: MessageRole,
}

/// Role of a message sender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    /// A human user.
    User,
    /// An AI agent.
    Agent,
    /// The system.
    System,
    /// A tool.
    Tool,
}

/// Output from a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    /// Which tool produced this output.
    pub tool_id: String,
    /// The tool_use ID this result corresponds to.
    pub tool_use_id: String,
    /// The output content.
    pub content: String,
    /// Whether the tool execution succeeded.
    pub success: bool,
    /// How long the tool took to execute.
    pub execution_time_ms: u64,
}

/// A change in the memory substrate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryDelta {
    /// What kind of memory operation.
    pub operation: MemoryOperation,
    /// The key that changed.
    pub key: String,
    /// Which agent's memory changed.
    pub agent_id: AgentId,
}

/// The type of memory operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryOperation {
    /// A new value was created.
    Created,
    /// An existing value was updated.
    Updated,
    /// A value was deleted.
    Deleted,
}

/// Agent lifecycle event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum LifecycleEvent {
    /// An agent was spawned.
    Spawned {
        /// The new agent's ID.
        agent_id: AgentId,
        /// The new agent's name.
        name: String,
    },
    /// An agent started running.
    Started {
        /// The agent's ID.
        agent_id: AgentId,
    },
    /// An agent was suspended.
    Suspended {
        /// The agent's ID.
        agent_id: AgentId,
    },
    /// An agent was resumed.
    Resumed {
        /// The agent's ID.
        agent_id: AgentId,
    },
    /// An agent was terminated.
    Terminated {
        /// The agent's ID.
        agent_id: AgentId,
        /// The reason for termination.
        reason: String,
    },
    /// An agent crashed.
    Crashed {
        /// The agent's ID.
        agent_id: AgentId,
        /// The error that caused the crash.
        error: String,
    },
}

/// Network-related event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum NetworkEvent {
    /// A peer connected.
    PeerConnected {
        /// The peer's ID.
        peer_id: String,
    },
    /// A peer disconnected.
    PeerDisconnected {
        /// The peer's ID.
        peer_id: String,
    },
    /// A message was received from a remote agent.
    MessageReceived {
        /// The peer that sent the message.
        from_peer: String,
        /// The agent that sent the message.
        from_agent: String,
    },
    /// A discovery query returned results.
    DiscoveryResult {
        /// The service that was searched for.
        service: String,
        /// The peers that provide the service.
        providers: Vec<String>,
    },
}

/// System-level event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum SystemEvent {
    /// The kernel has started.
    KernelStarted,
    /// The kernel is stopping.
    KernelStopping,
    /// An agent is approaching a resource quota.
    QuotaWarning {
        /// The agent's ID.
        agent_id: AgentId,
        /// Which resource is running low.
        resource: String,
        /// How much of the quota has been used (0-100).
        usage_percent: f32,
    },
    /// A health check was performed.
    HealthCheck {
        /// The health status.
        status: String,
    },
    /// A quota enforcement event.
    QuotaEnforced {
        /// The agent whose quota was enforced.
        agent_id: AgentId,
        /// Amount spent in the current window.
        spent: f64,
        /// The quota limit.
        limit: f64,
    },
    /// A model was auto-routed based on complexity.
    ModelRouted {
        /// The agent using the routed model.
        agent_id: AgentId,
        /// The detected complexity level.
        complexity: String,
        /// The model selected.
        model: String,
    },
    /// A user action was performed.
    UserAction {
        /// The user who performed the action.
        user_id: String,
        /// The action performed.
        action: String,
        /// The result of the action.
        result: String,
    },
    /// A heartbeat health check failed for an agent.
    HealthCheckFailed {
        /// The agent that failed the health check.
        agent_id: AgentId,
        /// How long the agent has been unresponsive.
        unresponsive_secs: u64,
    },
    /// A scheduled cron job was executed and produced a result.
    CronJobExecuted {
        /// The agent that ran the job.
        agent_id: AgentId,
        /// The job's unique ID.
        job_id: String,
        /// The job's display name.
        job_name: String,
        /// The trigger message sent to the agent.
        trigger_message: String,
        /// The agent's text response.
        response: String,
        /// Whether the result was delivered to a channel (e.g. Telegram).
        delivered_to_channel: bool,
    },
}

/// A complete event in the OpenFang event system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Unique event ID.
    pub id: EventId,
    /// Which agent (or system) produced this event.
    pub source: AgentId,
    /// Where this event is directed.
    pub target: EventTarget,
    /// The event payload.
    pub payload: EventPayload,
    /// When the event was created.
    pub timestamp: DateTime<Utc>,
    /// For request-response patterns: links response to request.
    pub correlation_id: Option<EventId>,
    /// Time-to-live: event expires after this duration.
    #[serde(with = "duration_ms")]
    pub ttl: Option<Duration>,
}

impl Event {
    /// Create a new event with the given source, target, and payload.
    pub fn new(source: AgentId, target: EventTarget, payload: EventPayload) -> Self {
        Self {
            id: EventId::new(),
            source,
            target,
            payload,
            timestamp: Utc::now(),
            correlation_id: None,
            ttl: None,
        }
    }

    /// Set the correlation ID for request-response linking.
    pub fn with_correlation(mut self, correlation_id: EventId) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    /// Set the TTL for this event.
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = Some(ttl);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_creation() {
        let agent_id = AgentId::new();
        let event = Event::new(
            agent_id,
            EventTarget::Broadcast,
            EventPayload::System(SystemEvent::KernelStarted),
        );
        assert_eq!(event.source, agent_id);
        assert!(event.correlation_id.is_none());
        assert!(event.ttl.is_none());
    }

    #[test]
    fn test_event_with_correlation() {
        let agent_id = AgentId::new();
        let corr_id = EventId::new();
        let event = Event::new(
            agent_id,
            EventTarget::System,
            EventPayload::System(SystemEvent::HealthCheck {
                status: "ok".to_string(),
            }),
        )
        .with_correlation(corr_id);
        assert_eq!(event.correlation_id, Some(corr_id));
    }

    #[test]
    fn test_event_serialization() {
        let agent_id = AgentId::new();
        let event = Event::new(
            agent_id,
            EventTarget::Agent(AgentId::new()),
            EventPayload::Message(AgentMessage {
                content: "Hello".to_string(),
                metadata: HashMap::new(),
                role: MessageRole::User,
            }),
        );
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, event.id);
    }

    #[test]
    fn test_event_with_ttl_serialization() {
        let agent_id = AgentId::new();
        let event = Event::new(
            agent_id,
            EventTarget::Broadcast,
            EventPayload::System(SystemEvent::KernelStarted),
        )
        .with_ttl(Duration::from_secs(60));
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.ttl, Some(Duration::from_millis(60_000)));
    }
}
