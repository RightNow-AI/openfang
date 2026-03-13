//! Coordinator — Layer 1 of the team model.
//!
//! The coordinator owns:
//! - Task registration and assignment
//! - Contract ledger (via the blackboard)
//! - Blocker resolution and escalation
//! - Final merge order
//!
//! It communicates with workers exclusively through the [`Blackboard`].

use crate::team::{
    blackboard::Blackboard,
    messages::{TeamMessage, TeamMessageKind},
    registry::{TaskRecord, TaskState},
    task_packet::TaskPacket,
};
use std::{collections::HashSet, sync::Arc};
use tracing::{info, warn};

/// The coordinator is the single authoritative agent for the task registry.
///
/// All workers report to the coordinator; the coordinator never reports
/// to a worker.
#[derive(Debug, Clone)]
pub struct Coordinator {
    pub agent_id: String,
    pub blackboard: Arc<Blackboard>,
}

impl Coordinator {
    pub fn new(agent_id: impl Into<String>, blackboard: Arc<Blackboard>) -> Self {
        Self {
            agent_id: agent_id.into(),
            blackboard,
        }
    }

    /// Register a task packet and post an [`TeamMessageKind::Assign`] to the owner.
    pub async fn assign(&self, packet: TaskPacket) {
        let task_id = packet.task_id.clone();
        let owner = packet.owner_agent_id.clone();
        info!(task_id = %task_id, owner = %owner, "coordinator: assigning task");
        self.blackboard.registry.insert(TaskRecord::new(packet));
        let msg = TeamMessage::new(
            &self.agent_id,
            &owner,
            &task_id,
            TeamMessageKind::Assign,
            format!("task {task_id} assigned"),
        );
        self.blackboard.post(msg).await;
    }

    /// Process one round of messages addressed to the coordinator.
    ///
    /// Call this in a loop (e.g. every scheduler tick) to keep the registry
    /// consistent with what workers are reporting.
    pub async fn tick(&self) {
        let messages = self.blackboard.drain_for(&self.agent_id).await;
        for msg in messages {
            match msg.kind {
                TeamMessageKind::Blocked => {
                    warn!(
                        task_id = %msg.task_id,
                        from = %msg.from_agent,
                        summary = %msg.summary,
                        "coordinator: task blocked"
                    );
                    self.blackboard
                        .registry
                        .set_blocked(&msg.task_id, msg.summary.clone());
                }
                TeamMessageKind::Complete => {
                    info!(
                        task_id = %msg.task_id,
                        from = %msg.from_agent,
                        "coordinator: task complete"
                    );
                    self.blackboard
                        .registry
                        .set_state(&msg.task_id, TaskState::Done);
                }
                TeamMessageKind::NeedInput => {
                    warn!(
                        task_id = %msg.task_id,
                        from = %msg.from_agent,
                        summary = %msg.summary,
                        "coordinator: agent needs input — escalate to human"
                    );
                }
                TeamMessageKind::Handoff => {
                    info!(
                        task_id = %msg.task_id,
                        from = %msg.from_agent,
                        to = %msg.to_agent,
                        "coordinator: handoff in progress"
                    );
                }
                _ => {}
            }
        }
    }

    /// Promote all `Pending` tasks whose dependencies are fully `Done` to `Ready`.
    pub async fn promote_ready(&self) {
        let all = self.blackboard.registry.all_records();
        let done_ids: HashSet<String> = all
            .iter()
            .filter(|r| r.state == TaskState::Done)
            .map(|r| r.packet.task_id.clone())
            .collect();

        for record in &all {
            if record.state == TaskState::Pending
                && record
                    .packet
                    .dependencies
                    .iter()
                    .all(|dep| done_ids.contains(dep))
            {
                self.blackboard
                    .registry
                    .set_state(&record.packet.task_id, TaskState::Ready);
            }
        }
    }

    /// Tasks that are `Ready` and can be started by their owners immediately.
    pub fn ready_tasks(&self) -> Vec<TaskRecord> {
        self.blackboard
            .registry
            .all_records()
            .into_iter()
            .filter(|r| r.state == TaskState::Ready)
            .collect()
    }

    /// All blocked tasks — used for human-readable status reporting.
    pub fn blocked_tasks(&self) -> Vec<TaskRecord> {
        self.blackboard
            .registry
            .all_records()
            .into_iter()
            .filter(|r| r.state == TaskState::Blocked)
            .collect()
    }
}
