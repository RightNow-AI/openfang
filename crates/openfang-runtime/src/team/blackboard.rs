//! Blackboard — the shared coordination surface for a multi-agent team.
//!
//! All inter-agent communication flows through the blackboard:
//! - The [`TaskRegistry`] is the source of truth for task states.
//! - The `mailbox` is an async-safe message queue; workers post and drain from it.
//! - `contracts` is a key-value store for locked interface contracts
//!   (e.g. base URL, chat transport schema, history shape).

use crate::team::{
    messages::TeamMessage,
    registry::TaskRegistry,
};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

/// Shared state visible to all members of a team.
///
/// Clone-cheap: all fields are `Arc`-wrapped.
#[derive(Debug, Clone)]
pub struct Blackboard {
    /// Canonical view of every task packet and its current state.
    pub registry: Arc<TaskRegistry>,
    /// Async message queue.  Workers post; coordinator and workers drain by recipient.
    pub mailbox: Arc<RwLock<Vec<TeamMessage>>>,
    /// Locked interface contracts keyed by short name (e.g. `"base_url"`, `"chat_transport"`).
    pub contracts: Arc<RwLock<Value>>,
}

impl Default for Blackboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Blackboard {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(TaskRegistry::new()),
            mailbox: Arc::new(RwLock::new(Vec::new())),
            contracts: Arc::new(RwLock::new(Value::Object(Default::default()))),
        }
    }

    /// Post a message to the shared mailbox.
    pub async fn post(&self, msg: TeamMessage) {
        debug!(
            msg_id = %msg.msg_id,
            kind = ?msg.kind,
            from = %msg.from_agent,
            to = %msg.to_agent,
            "blackboard: post"
        );
        self.mailbox.write().await.push(msg);
    }

    /// Drain all messages addressed to `agent_id` or broadcast (`"*"`).
    ///
    /// Non-matching messages are left in the mailbox.
    pub async fn drain_for(&self, agent_id: &str) -> Vec<TeamMessage> {
        let mut mailbox = self.mailbox.write().await;
        let mut mine = Vec::new();
        let mut rest = Vec::new();
        for msg in mailbox.drain(..) {
            if msg.to_agent == agent_id || msg.to_agent == "*" {
                mine.push(msg);
            } else {
                rest.push(msg);
            }
        }
        *mailbox = rest;
        mine
    }

    /// Set or update a contract by key.
    ///
    /// Panics only if the root `contracts` value has been replaced with a non-object,
    /// which should never happen in normal usage.
    pub async fn set_contract(&self, key: &str, value: Value) {
        let mut contracts = self.contracts.write().await;
        if let Value::Object(ref mut map) = *contracts {
            map.insert(key.to_string(), value);
        }
    }

    /// Read a contract value by key.
    pub async fn get_contract(&self, key: &str) -> Option<Value> {
        let contracts = self.contracts.read().await;
        if let Value::Object(ref map) = *contracts {
            map.get(key).cloned()
        } else {
            None
        }
    }

    /// Snapshot all locked contracts as a JSON object.
    pub async fn contracts_snapshot(&self) -> Value {
        self.contracts.read().await.clone()
    }
}
