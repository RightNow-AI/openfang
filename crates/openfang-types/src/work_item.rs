//! WorkItem — canonical unit of work in the OpenFang AOS.
//!
//! A `WorkItem` represents any discrete task: agent work, workflow steps,
//! scheduled jobs, or human-triggered actions. Every state transition emits
//! a `WorkEvent` for a full audit trail.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Lifecycle states for a WorkItem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkStatus {
    /// Created but not yet ready for execution.
    #[default]
    Pending,
    /// Assigned and ready to run.
    Ready,
    /// Actively executing.
    Running,
    /// Execution completed — awaiting human approval before finalising.
    WaitingApproval,
    /// Approved by an authorised actor.
    Approved,
    /// Rejected by an authorised actor.
    Rejected,
    /// Execution finished successfully.
    Completed,
    /// Execution failed (see `error` field).
    Failed,
    /// Cancelled before completion.
    Cancelled,
}

impl WorkStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::WaitingApproval => "waiting_approval",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

}

impl std::str::FromStr for WorkStatus {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "ready" => Ok(Self::Ready),
            "running" => Ok(Self::Running),
            "waiting_approval" => Ok(Self::WaitingApproval),
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(()),
        }
    }
}

impl WorkStatus {
    /// Returns `true` if the transition to `next` is valid.
    pub fn can_transition_to(&self, next: &WorkStatus) -> bool {
        use WorkStatus::*;
        matches!(
            (self, next),
            (Pending, Ready)
                | (Pending, Running)
                | (Pending, Cancelled)
                | (Ready, Running)
                | (Ready, Cancelled)
                | (Running, WaitingApproval)
                | (Running, Completed)
                | (Running, Failed)
                | (Running, Cancelled)
                | (WaitingApproval, Approved)
                | (WaitingApproval, Rejected)
                | (WaitingApproval, Cancelled)
                | (Approved, Completed)
                | (Approved, Failed)
                | (Failed, Pending) // retry
        )
    }
}

/// Whether human sign-off is required before a work item is finalised.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    /// No approval gate is configured for this item.
    #[default]
    NotRequired,
    /// Approval requested but not yet given.
    Pending,
    /// Approved by an authorised actor.
    Approved,
    /// Rejected by an authorised actor.
    Rejected,
}

impl ApprovalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotRequired => "not_required",
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
        }
    }

}

impl std::str::FromStr for ApprovalStatus {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "not_required" => Ok(Self::NotRequired),
            "pending" => Ok(Self::Pending),
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            _ => Err(()),
        }
    }
}

/// How the work item was created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkSource {
    /// Created via the REST API.
    #[default]
    Api,
    /// Created by the scheduler / cron system.
    Schedule,
    /// Created by an event trigger.
    Trigger,
    /// Created by another agent.
    AgentSpawned,
    /// Manually created by a human operator.
    Human,
}

impl WorkSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Schedule => "schedule",
            Self::Trigger => "trigger",
            Self::AgentSpawned => "agent_spawned",
            Self::Human => "human",
        }
    }

}

impl std::str::FromStr for WorkSource {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "api" => Ok(Self::Api),
            "schedule" => Ok(Self::Schedule),
            "trigger" => Ok(Self::Trigger),
            "agent_spawned" => Ok(Self::AgentSpawned),
            "human" => Ok(Self::Human),
            _ => Err(()),
        }
    }
}

/// The category of work being performed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkType {
    /// A task delegated to an agent.
    #[default]
    AgentTask,
    /// A multi-step orchestrated workflow.
    Workflow,
    /// A data transformation or processing job.
    Transformation,
    /// Research or information retrieval.
    Research,
    /// Content generation.
    Generation,
    /// User-defined custom work type.
    Custom,
}

impl WorkType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AgentTask => "agent_task",
            Self::Workflow => "workflow",
            Self::Transformation => "transformation",
            Self::Research => "research",
            Self::Generation => "generation",
            Self::Custom => "custom",
        }
    }

}

impl std::str::FromStr for WorkType {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "agent_task" => Ok(Self::AgentTask),
            "workflow" => Ok(Self::Workflow),
            "transformation" => Ok(Self::Transformation),
            "research" => Ok(Self::Research),
            "generation" => Ok(Self::Generation),
            "custom" => Ok(Self::Custom),
            _ => Err(()),
        }
    }
}

// ---------------------------------------------------------------------------
// Core structs
// ---------------------------------------------------------------------------

/// A single unit of work tracked through its full lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItem {
    // --- Identity ---
    /// Unique identifier (UUID v4).
    pub id: String,
    /// Human-readable name for this item.
    pub title: String,
    /// Optional longer description or prompt.
    #[serde(default)]
    pub description: String,
    /// Category of work.
    #[serde(default)]
    pub work_type: WorkType,
    /// How this item was created.
    #[serde(default)]
    pub source: WorkSource,

    // --- Status ---
    /// Current lifecycle state.
    #[serde(default)]
    pub status: WorkStatus,
    /// Current approval gate state.
    #[serde(default)]
    pub approval_status: ApprovalStatus,

    // --- Execution ---
    /// Agent UUID assigned to execute this item, if any.
    #[serde(default)]
    pub assigned_agent_id: Option<String>,
    /// Human-readable agent name (denormalized for display).
    #[serde(default)]
    pub assigned_agent_name: Option<String>,
    /// Output produced by the agent on success.
    #[serde(default)]
    pub result: Option<String>,
    /// Error message if execution failed.
    #[serde(default)]
    pub error: Option<String>,
    /// Number of agent loop iterations consumed.
    #[serde(default)]
    pub iterations: u32,

    // --- Scheduling ---
    /// Execution priority (0-255, default 128; higher = runs sooner).
    #[serde(default = "default_priority")]
    pub priority: u8,
    /// Earliest time this item may run (null = run ASAP).
    #[serde(default)]
    pub scheduled_at: Option<DateTime<Utc>>,
    /// Wall-clock time execution began.
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    /// Wall-clock time execution ended (success or failure).
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
    /// Hard deadline — item will be cancelled if not completed by this time.
    #[serde(default)]
    pub deadline: Option<DateTime<Utc>>,

    // --- Approval ---
    /// When `true`, transitions `Running → WaitingApproval` instead of
    /// `Running → Completed`.
    #[serde(default)]
    pub requires_approval: bool,
    /// ID or display name of the person who approved or rejected.
    #[serde(default)]
    pub approved_by: Option<String>,
    /// Timestamp of the approval/rejection decision.
    #[serde(default)]
    pub approved_at: Option<DateTime<Utc>>,
    /// Free-form note attached to the approval decision.
    #[serde(default)]
    pub approval_note: Option<String>,

    // --- Payload / context ---
    /// Arbitrary JSON payload passed to the executing agent.
    #[serde(default)]
    pub payload: serde_json::Value,
    /// Searchable string tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Who requested this work (user ID, agent ID, or display name).
    #[serde(default)]
    pub created_by: Option<String>,
    /// Optional deduplication key — re-creating with an existing key is a no-op.
    #[serde(default)]
    pub idempotency_key: Option<String>,

    // --- Metadata ---
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// How many times this item has been automatically retried.
    #[serde(default)]
    pub retry_count: u32,
    /// Maximum number of automatic retries before the item stays `Failed`.
    #[serde(default)]
    pub max_retries: u32,
    /// Parent work item ID for subagent delegation chains.
    #[serde(default)]
    pub parent_id: Option<String>,
    /// Stable run identifier used for durable run-scoped artifacts.
    #[serde(default)]
    pub run_id: Option<String>,
    /// Stable workspace identifier used for durable workspace-scoped artifacts.
    #[serde(default)]
    pub workspace_id: Option<String>,
}

fn default_priority() -> u8 {
    128
}

/// A single event in the audit trail of a WorkItem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkEvent {
    /// Unique event identifier.
    pub id: String,
    /// The work item this event belongs to.
    pub work_item_id: String,
    /// Short identifier for the event kind (e.g. "status_change", "approved").
    pub event_type: String,
    /// Status before this transition (if applicable).
    #[serde(default)]
    pub from_status: Option<String>,
    /// Status after this transition (if applicable).
    #[serde(default)]
    pub to_status: Option<String>,
    /// The actor that caused this event (user, agent ID, or "system").
    #[serde(default)]
    pub actor: Option<String>,
    /// Free-form detail text.
    #[serde(default)]
    pub detail: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// An approval or rejection decision record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRecord {
    pub id: String,
    pub work_item_id: String,
    /// `"approved"` or `"rejected"`.
    pub decision: String,
    pub actor: String,
    #[serde(default)]
    pub note: Option<String>,
    pub decided_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Request / filter types
// ---------------------------------------------------------------------------

/// Filters for the `GET /api/work` list endpoint.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct WorkItemFilter {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub work_type: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub assigned_agent_id: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub approval_status: Option<String>,
    #[serde(default)]
    pub scheduled: Option<bool>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
}

/// Body for `POST /api/work`.
#[derive(Debug, Deserialize)]
pub struct CreateWorkItemRequest {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub work_type: WorkType,
    #[serde(default)]
    pub source: WorkSource,
    #[serde(default)]
    pub assigned_agent_id: Option<String>,
    #[serde(default)]
    pub priority: Option<u8>,
    #[serde(default)]
    pub scheduled_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub deadline: Option<DateTime<Utc>>,
    #[serde(default)]
    pub requires_approval: bool,
    #[serde(default)]
    pub payload: serde_json::Value,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub idempotency_key: Option<String>,
    #[serde(default)]
    pub max_retries: u32,
    /// Parent work item ID to chain this as a subagent child.
    #[serde(default)]
    pub parent_id: Option<String>,
    /// Stable run identifier used for durable run-scoped artifacts.
    #[serde(default)]
    pub run_id: Option<String>,
    /// Stable workspace identifier used for durable workspace-scoped artifacts.
    #[serde(default)]
    pub workspace_id: Option<String>,
}

/// Body for `POST /api/work/{id}/run`.
#[derive(Debug, Default, Deserialize)]
pub struct RunWorkItemRequest {
    /// Optionally override the assigned agent.
    #[serde(default)]
    pub agent_id: Option<String>,
}

/// Body for `POST /api/work/{id}/approve` and `.../reject`.
#[derive(Debug, Default, Deserialize)]
pub struct ApprovalDecisionRequest {
    pub actor: String,
    #[serde(default)]
    pub note: Option<String>,
}

/// Body for `POST /api/work/{id}/cancel`.
#[derive(Debug, Default, Deserialize)]
pub struct CancelWorkItemRequest {
    #[serde(default)]
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Operator / Orchestrator types
// ---------------------------------------------------------------------------

/// Summary counts for all WorkItem statuses — returned by `GET /api/work/summary`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkSummary {
    pub pending: u64,
    pub ready: u64,
    pub running: u64,
    pub waiting_approval: u64,
    pub approved: u64,
    pub rejected: u64,
    pub completed: u64,
    pub failed: u64,
    pub cancelled: u64,
    pub total: u64,
    /// Number of items with `scheduled_at IS NOT NULL`.
    pub scheduled: u64,
    pub generated_at: DateTime<Utc>,
}

/// Snapshot of the heartbeat orchestrator state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorStatus {
    pub running: bool,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub queued_count: u64,
    pub running_count: u64,
    pub pending_approval_count: u64,
    pub scheduler_version: String,
    pub uptime_secs: u64,
}

/// Record of a single heartbeat / orchestrator run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorRun {
    pub id: String,
    pub triggered_at: DateTime<Utc>,
    pub triggered_by: String,
    pub items_claimed: u32,
    pub items_scheduled_started: u32,
    pub items_delegated: u32,
    pub duration_ms: u64,
    #[serde(default)]
    pub note: Option<String>,
}

/// Body for `POST /api/work/{id}/delegate`.
#[derive(Debug, Default, Deserialize)]
pub struct DelegateWorkItemRequest {
    /// Agent to assign the child item to (defaults to parent's agent).
    #[serde(default)]
    pub agent_id: Option<String>,
    /// Override title for the child item (defaults to parent title with "[sub]" prefix).
    #[serde(default)]
    pub title: Option<String>,
    /// Human-readable reason for the delegation (recorded in the DelegationRecord audit event).
    #[serde(default)]
    pub reason: Option<String>,
    /// Payload for the child item.
    #[serde(default)]
    pub payload: serde_json::Value,
}
