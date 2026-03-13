//! Task registry — the coordinator's authoritative record of all task packets and their states.

use crate::team::task_packet::TaskPacket;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Lifecycle state of a task packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    /// Created but dependencies are not yet resolved.
    #[default]
    Pending,
    /// All dependencies satisfied; ready to be claimed by the owner.
    Ready,
    /// Owner has started work.
    InProgress,
    /// Owner cannot proceed without external resolution.
    Blocked,
    /// Output contract satisfied; coordinator has accepted the output.
    Done,
    /// Task terminated with unrecoverable error.
    Failed,
}

/// One row in the registry: the packet plus its live state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub packet: TaskPacket,
    pub state: TaskState,
    /// Set when state is `Blocked` or `Failed`.
    pub blocker_reason: Option<String>,
}

impl TaskRecord {
    pub fn new(packet: TaskPacket) -> Self {
        Self {
            packet,
            state: TaskState::Pending,
            blocker_reason: None,
        }
    }
}

/// Concurrent, in-memory task registry keyed by `task_id`.
///
/// Uses a `DashMap` so coordinator and workers can read/update concurrently
/// without a global lock.
#[derive(Debug, Clone, Default)]
pub struct TaskRegistry {
    tasks: Arc<DashMap<String, TaskRecord>>,
}

impl TaskRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace a record.
    pub fn insert(&self, record: TaskRecord) {
        self.tasks.insert(record.packet.task_id.clone(), record);
    }

    /// Get a cloned snapshot of a record.
    pub fn get(&self, task_id: &str) -> Option<TaskRecord> {
        self.tasks.get(task_id).map(|r| r.clone())
    }

    /// Transition a task to a new state.  Returns `true` if the task was found.
    pub fn set_state(&self, task_id: &str, state: TaskState) -> bool {
        if let Some(mut entry) = self.tasks.get_mut(task_id) {
            entry.state = state;
            entry.blocker_reason = None;
            true
        } else {
            false
        }
    }

    /// Mark a task blocked and record the reason.
    pub fn set_blocked(&self, task_id: &str, reason: String) -> bool {
        if let Some(mut entry) = self.tasks.get_mut(task_id) {
            entry.state = TaskState::Blocked;
            entry.blocker_reason = Some(reason);
            true
        } else {
            false
        }
    }

    /// Snapshot all records (allocation per call — use for reporting, not hot paths).
    pub fn all_records(&self) -> Vec<TaskRecord> {
        self.tasks.iter().map(|e| e.value().clone()).collect()
    }

    /// All pending or ready tasks belonging to a given agent.
    pub fn pending_for_agent(&self, agent_id: &str) -> Vec<TaskRecord> {
        self.tasks
            .iter()
            .filter(|e| {
                e.value().packet.owner_agent_id == agent_id
                    && matches!(e.value().state, TaskState::Pending | TaskState::Ready)
            })
            .map(|e| e.value().clone())
            .collect()
    }
}
