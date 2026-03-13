//! Worker agent — Layer 2 of the team model.
//!
//! A worker owns a specific domain (UI, SDK, backend-contract, QA).
//! Workers pull tasks from the blackboard, work independently, and post
//! status messages back.  They never communicate with each other directly
//! — all coordination flows through the blackboard.
//!
//! Subagents ([`SubagentHandle`]) are Layer 3: short-lived, bounded units
//! spawned by workers for a single delegated piece of work.  Subagents
//! report only to their parent worker.

use crate::team::{
    blackboard::Blackboard,
    messages::{TeamMessage, TeamMessageKind},
    registry::TaskState,
};
use std::sync::Arc;
use tracing::{debug, info};

/// A domain-scoped specialist agent that owns a well-defined area of the
/// codebase and receives task assignments from the coordinator.
#[derive(Debug, Clone)]
pub struct WorkerAgent {
    /// Stable slug that matches `owner_agent_id` in task packets (e.g. `"team/sdk"`).
    pub agent_id: String,
    /// Logical domain label (e.g. `"ui"`, `"sdk"`, `"backend-contract"`, `"qa"`).
    pub domain: String,
    /// Files this worker is allowed to edit without coordinator approval.
    /// Used by the coordinator to detect concurrent ownership conflicts.
    pub owned_files: Vec<String>,
    pub blackboard: Arc<Blackboard>,
}

impl WorkerAgent {
    pub fn new(
        agent_id: impl Into<String>,
        domain: impl Into<String>,
        owned_files: Vec<String>,
        blackboard: Arc<Blackboard>,
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            domain: domain.into(),
            owned_files,
            blackboard,
        }
    }

    /// Report that work has started on a task.
    pub async fn report_started(&self, task_id: &str, coordinator_id: &str) {
        info!(agent = %self.agent_id, task = %task_id, "worker: started");
        self.blackboard
            .registry
            .set_state(task_id, TaskState::InProgress);
        let msg = TeamMessage::new(
            &self.agent_id,
            coordinator_id,
            task_id,
            TeamMessageKind::Progress,
            "started",
        );
        self.blackboard.post(msg).await;
    }

    /// Report that the task is blocked and needs coordinator attention.
    pub async fn report_blocked(
        &self,
        task_id: &str,
        coordinator_id: &str,
        reason: impl Into<String>,
    ) {
        let reason = reason.into();
        debug!(agent = %self.agent_id, task = %task_id, reason = %reason, "worker: blocked");
        self.blackboard
            .registry
            .set_blocked(task_id, reason.clone());
        let msg = TeamMessage::new(
            &self.agent_id,
            coordinator_id,
            task_id,
            TeamMessageKind::Blocked,
            reason,
        );
        self.blackboard.post(msg).await;
    }

    /// Report that the task is fully done and the output contract is satisfied.
    pub async fn report_complete(&self, task_id: &str, coordinator_id: &str) {
        info!(agent = %self.agent_id, task = %task_id, "worker: complete");
        self.blackboard
            .registry
            .set_state(task_id, TaskState::Done);
        let msg = TeamMessage::new(
            &self.agent_id,
            coordinator_id,
            task_id,
            TeamMessageKind::Complete,
            "complete",
        );
        self.blackboard.post(msg).await;
    }

    /// Request that another agent take over incomplete work.
    pub async fn request_handoff(
        &self,
        task_id: &str,
        to_agent: &str,
        summary: impl Into<String>,
    ) {
        let msg = TeamMessage::new(
            &self.agent_id,
            to_agent,
            task_id,
            TeamMessageKind::Handoff,
            summary,
        );
        self.blackboard.post(msg).await;
    }

    /// Poll the mailbox for messages addressed to this worker.
    pub async fn tick(&self) -> Vec<TeamMessage> {
        self.blackboard.drain_for(&self.agent_id).await
    }
}

/// Handle for a subagent spawned by a worker to handle bounded, delegated work.
///
/// Subagents are not registered in the global task registry.  They are
/// lightweight and report only to their parent worker — never to the coordinator.
#[derive(Debug, Clone)]
pub struct SubagentHandle {
    /// The worker that spawned this subagent.
    pub parent_agent_id: String,
    /// The parent task this subagent is contributing to.
    pub task_id: String,
    /// One-line description of the bounded work delegated.
    pub description: String,
}

impl SubagentHandle {
    pub fn new(
        parent_agent_id: impl Into<String>,
        task_id: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            parent_agent_id: parent_agent_id.into(),
            task_id: task_id.into(),
            description: description.into(),
        }
    }
}
