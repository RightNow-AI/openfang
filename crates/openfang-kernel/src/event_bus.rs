//! Event bus — pub/sub with pattern matching and history ring buffer.

use dashmap::DashMap;
use openfang_types::agent::AgentId;
use openfang_types::event::{Event, EventTarget};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::debug;

/// Maximum events retained in the history ring buffer.
const HISTORY_SIZE: usize = 1000;

/// The central event bus for inter-agent and system communication.
pub struct EventBus {
    /// Broadcast channel for all events.
    sender: broadcast::Sender<Event>,
    /// Per-agent event channels.
    agent_channels: DashMap<AgentId, broadcast::Sender<Event>>,
    /// Event history ring buffer.
    history: Arc<RwLock<VecDeque<Event>>>,
}

impl EventBus {
    fn push_history_sync(&self, event: &Event) {
        if let Ok(mut history) = self.history.try_write() {
            if history.len() >= HISTORY_SIZE {
                history.pop_front();
            }
            history.push_back(event.clone());
        }
    }

    /// Create a new event bus.
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1024);
        Self {
            sender,
            agent_channels: DashMap::new(),
            history: Arc::new(RwLock::new(VecDeque::with_capacity(HISTORY_SIZE))),
        }
    }

    /// Publish an event to the bus.
    pub async fn publish(&self, event: Event) {
        debug!(
            event_id = %event.id,
            source = %event.source,
            "Publishing event"
        );

        // Store in history
        {
            let mut history = self.history.write().await;
            if history.len() >= HISTORY_SIZE {
                history.pop_front();
            }
            history.push_back(event.clone());
        }

        // Route to target
        match &event.target {
            EventTarget::Agent(agent_id) => {
                if let Some(sender) = self.agent_channels.get(agent_id) {
                    let _ = sender.send(event.clone());
                }
            }
            EventTarget::Broadcast => {
                let _ = self.sender.send(event.clone());
                for entry in self.agent_channels.iter() {
                    let _ = entry.value().send(event.clone());
                }
            }
            EventTarget::Pattern(_pattern) => {
                // Phase 1: broadcast to all for pattern matching
                let _ = self.sender.send(event.clone());
            }
            EventTarget::System => {
                let _ = self.sender.send(event.clone());
            }
        }
    }

    /// Publish an event from synchronous code paths.
    ///
    /// History recording is best-effort here because callers cannot await the
    /// async lock used by the ring buffer.
    pub fn publish_immediate(&self, event: Event) {
        debug!(
            event_id = %event.id,
            source = %event.source,
            "Publishing event immediately"
        );

        self.push_history_sync(&event);

        match &event.target {
            EventTarget::Agent(agent_id) => {
                if let Some(sender) = self.agent_channels.get(agent_id) {
                    let _ = sender.send(event.clone());
                }
            }
            EventTarget::Broadcast => {
                let _ = self.sender.send(event.clone());
                for entry in self.agent_channels.iter() {
                    let _ = entry.value().send(event.clone());
                }
            }
            EventTarget::Pattern(_) | EventTarget::System => {
                let _ = self.sender.send(event.clone());
            }
        }
    }

    /// Subscribe to events for a specific agent.
    pub fn subscribe_agent(&self, agent_id: AgentId) -> broadcast::Receiver<Event> {
        let entry = self.agent_channels.entry(agent_id).or_insert_with(|| {
            let (tx, _) = broadcast::channel(256);
            tx
        });
        entry.subscribe()
    }

    /// Subscribe to all broadcast/system events.
    pub fn subscribe_all(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    /// Get recent event history.
    pub async fn history(&self, limit: usize) -> Vec<Event> {
        let history = self.history.read().await;
        history.iter().rev().take(limit).cloned().collect()
    }

    /// Remove an agent's channel when it's terminated.
    pub fn unsubscribe_agent(&self, agent_id: AgentId) {
        self.agent_channels.remove(&agent_id);
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_types::event::{EventPayload, SystemEvent};

    #[tokio::test]
    async fn test_publish_and_history() {
        let bus = EventBus::new();
        let agent_id = AgentId::new();
        let event = Event::new(
            agent_id,
            EventTarget::System,
            EventPayload::System(SystemEvent::KernelStarted),
        );
        bus.publish(event).await;
        let history = bus.history(10).await;
        assert_eq!(history.len(), 1);
    }

    #[tokio::test]
    async fn test_agent_subscribe() {
        let bus = EventBus::new();
        let agent_id = AgentId::new();
        let mut rx = bus.subscribe_agent(agent_id);

        let event = Event::new(
            AgentId::new(),
            EventTarget::Agent(agent_id),
            EventPayload::System(SystemEvent::HealthCheck {
                status: "ok".to_string(),
            }),
        );
        bus.publish(event).await;

        let received = rx.recv().await.unwrap();
        match received.payload {
            EventPayload::System(SystemEvent::HealthCheck { status }) => {
                assert_eq!(status, "ok");
            }
            _ => panic!("Wrong payload"),
        }
    }

    #[tokio::test]
    async fn test_publish_immediate_reaches_broadcast_subscribers() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe_all();
        let event = Event::new(
            AgentId::new(),
            EventTarget::Broadcast,
            EventPayload::System(SystemEvent::KernelStarted),
        );

        bus.publish_immediate(event.clone());

        let received = rx.recv().await.unwrap();
        assert_eq!(received.id, event.id);
    }
}
