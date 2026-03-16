//! Disciplined execution contract types for WorkItem execution loops.
//!
//! These types describe the auditable 10-step loop:
//!   1. Load context
//!   2. Classify path
//!   3. Define objective
//!   4. Select adapter (API → CLI → Browser preference order)
//!   5. Check permissions/approvals
//!   6. Execute one bounded action
//!   7. Verify result
//!   8. Record outcome
//!   9. Retry discipline
//!  10. Delegation discipline
//!
//! All execution events are recorded as `WorkEvent` records via the
//! `event_type` string field — see `ExecutionEventKind::as_str()`.
//!
//! Reuses `AdapterKind`, `RetryPolicy`, and `VerificationRule` from
//! `crate::tool_contract` — never redefines them here.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::tool_contract::AdapterKind;

// ────────────────────────────────────────────────────────────────────────────
// ExecutionEventKind — the 14 canonical execution event types
// ────────────────────────────────────────────────────────────────────────────

/// The 14 event types that the disciplined execution loop must emit.
///
/// Each variant maps one-to-one to the `event_type` string stored in
/// `WorkEvent.event_type`. Use `as_str()` when writing `WorkEvent` records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionEventKind {
    /// Execution path (FastPath / PlannedSwarm / ReviewSwarm) was determined.
    ScopeClassified,
    /// Full planning round is required before proceeding.
    PlanningRequired,
    /// Adapter selected (which surface: API / CLI / Browser).
    AdapterSelected,
    /// Execution is blocked, awaiting human or policy approval.
    ApprovalRequired,
    /// A required dependency (agent, context, tool) could not be resolved.
    DependencyMissing,
    /// The agent or runtime was denied permission to proceed.
    PermissionDenied,
    /// The bounded action has been launched.
    ExecutionStarted,
    /// The bounded action returned (may be success or failure).
    ExecutionFinished,
    /// Post-execution verification did not pass.
    VerificationFailed,
    /// Post-execution verification passed; outcome is confirmed.
    VerifiedSuccess,
    /// Failure was transient; another attempt has been scheduled.
    RetryScheduled,
    /// Work has been delegated to a child work item / subagent.
    DelegatedToSubagent,
    /// Work item reached a terminal success state.
    Completed,
    /// Work item reached a terminal failure state.
    Failed,
}

impl ExecutionEventKind {
    /// Returns the `snake_case` string stored in `WorkEvent.event_type`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ScopeClassified => "scope_classified",
            Self::PlanningRequired => "planning_required",
            Self::AdapterSelected => "adapter_selected",
            Self::ApprovalRequired => "approval_required",
            Self::DependencyMissing => "dependency_missing",
            Self::PermissionDenied => "permission_denied",
            Self::ExecutionStarted => "execution_started",
            Self::ExecutionFinished => "execution_finished",
            Self::VerificationFailed => "verification_failed",
            Self::VerifiedSuccess => "verified_success",
            Self::RetryScheduled => "retry_scheduled",
            Self::DelegatedToSubagent => "delegated_to_subagent",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

impl std::fmt::Display for ExecutionEventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ────────────────────────────────────────────────────────────────────────────
// ExecutionBudget — resource ceiling for one execution attempt
// ────────────────────────────────────────────────────────────────────────────

/// Hard upper bound on resources consumed by a single execution attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionBudget {
    /// Wall-clock timeout for the entire execution (seconds).
    pub timeout_secs: u64,
    /// Maximum number of agent loop iterations permitted.
    pub max_iterations: u32,
    /// Maximum USD spend permitted across all LLM calls.
    pub max_cost_usd: Option<f64>,
}

impl Default for ExecutionBudget {
    fn default() -> Self {
        Self {
            timeout_secs: 120,
            max_iterations: 10,
            max_cost_usd: None,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// VerificationMethod — step-7 verification strategy for a WorkItem
// ────────────────────────────────────────────────────────────────────────────

/// How the executor confirms that the agent's action actually succeeded.
///
/// This is a WorkItem-level concept, intentionally simpler than
/// `VerificationRule` (which lives at the tool contract layer).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum VerificationMethod {
    /// The agent's response must be non-empty.
    #[default]
    ResponseNonEmpty,
    /// A named artifact must have been written to the payload.
    ArtifactExists {
        /// Key to look for in the result JSON.
        artifact_key: String,
    },
    /// A JSON schema hint must match the response body.
    SchemaCheck {
        /// Informal hint or "$schema" URI used to validate structure.
        schema_hint: String,
    },
    /// The work item itself must be fetchable (read-back check).
    RecordFetchable,
    /// No structured verification — accept any non-error response.
    AnyClosure,
}

// ────────────────────────────────────────────────────────────────────────────
// ExecutionObjective — step-3 concrete measurable target
// ────────────────────────────────────────────────────────────────────────────

/// The concrete, measurable target for one WorkItem execution attempt.
///
/// Created in step 3 ("Define objective") before any adapter is chosen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionObjective {
    /// Which system or resource is the target of the action (e.g. "GitHub PR #42").
    pub target_system: String,
    /// What the agent must do (e.g. "Summarise the PR diff in ≤150 words").
    pub intended_action: String,
    /// The measurable condition that proves success (e.g. "Response contains ≥1 sentence").
    pub success_condition: String,
    /// How the result will be verified after execution.
    pub verification_method: VerificationMethod,
    /// Resource ceiling for this attempt.
    pub budget: ExecutionBudget,
    /// Fallback adapter to try if the primary one fails (may be `None`).
    pub fallback_adapter: Option<AdapterKind>,
}

// ────────────────────────────────────────────────────────────────────────────
// AdapterSelection — step-4 adapter choice record
// ────────────────────────────────────────────────────────────────────────────

/// Records which adapter was chosen and why, plus the alternatives that were
/// considered and rejected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterSelection {
    /// The adapter that will be (or was) used.
    pub chosen: AdapterKind,
    /// Adapters evaluated but not chosen, in preference order.
    pub rejected: Vec<AdapterKind>,
    /// Human-readable rationale for the choice.
    pub rationale: String,
}

// ────────────────────────────────────────────────────────────────────────────
// ActionResult — step-6 single bounded action record
// ────────────────────────────────────────────────────────────────────────────

/// Full trace of the single bounded action executed in step 6.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    /// Brief description of the input given to the agent.
    pub input_summary: String,
    /// Which adapter was used.
    pub adapter: AdapterKind,
    /// The prompt or command string sent to the agent.
    pub action: String,
    /// Full output returned by the agent.
    pub output: String,
    /// Error message, if any.
    pub error: Option<String>,
    /// Whether the action itself succeeded (before verification).
    pub action_succeeded: bool,
    /// LLM input tokens consumed.
    pub tokens_in: u32,
    /// LLM output tokens generated.
    pub tokens_out: u32,
    /// Estimated cost in USD.
    pub cost_usd: Option<f64>,
    /// Agent loop iteration count.
    pub iterations: u32,
    /// When the action was dispatched.
    pub started_at: DateTime<Utc>,
    /// When the action returned.
    pub finished_at: DateTime<Utc>,
}

// ────────────────────────────────────────────────────────────────────────────
// VerificationResult — step-7 proof record
// ────────────────────────────────────────────────────────────────────────────

/// Proof that the action had (or lacked) the intended effect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Whether the verification check passed.
    pub passed: bool,
    /// Which method was used.
    pub method_used: VerificationMethod,
    /// Brief human-readable evidence (e.g. "Response length: 247 chars").
    pub evidence: String,
    /// Timestamp of verification.
    pub verified_at: DateTime<Utc>,
}

// ────────────────────────────────────────────────────────────────────────────
// BlockReason — why execution could not proceed
// ────────────────────────────────────────────────────────────────────────────

/// Tagged reason for why the execution loop could not proceed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "detail")]
pub enum BlockReason {
    /// Required context fields are missing from the WorkItem.
    MissingContext { missing: Vec<String> },
    /// No adapter could be selected for this work type.
    NoValidAdapter,
    /// An internal policy prevented execution.
    PolicyBlocked { reason: String },
    /// Human approval must be obtained before proceeding.
    ApprovalRequired,
    /// The agent or caller lacks permission.
    PermissionDenied { detail: String },
    /// The objective is malformed or unmeasurable.
    InvalidObjective { detail: String },
    /// A required dependency (agent, service, context) is not available.
    DependencyMissing { dependency: String },
}

// ────────────────────────────────────────────────────────────────────────────
// ExecutionStatus — terminal or intermediate outcome
// ────────────────────────────────────────────────────────────────────────────

/// The final (or current) status of one WorkItem execution attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    /// Work item finished successfully; verification passed.
    Completed,
    /// Work item failed; no more retries available.
    Failed,
    /// Execution is blocked and cannot proceed without external resolution.
    Blocked,
    /// Execution is paused waiting for human approval.
    WaitingApproval,
    /// A retry attempt has been scheduled.
    RetryScheduled,
    /// Work has been forwarded to a child agent / subagent.
    DelegatedToSubagent,
}

impl std::fmt::Display for ExecutionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
            Self::WaitingApproval => "waiting_approval",
            Self::RetryScheduled => "retry_scheduled",
            Self::DelegatedToSubagent => "delegated_to_subagent",
        };
        f.write_str(s)
    }
}

// ────────────────────────────────────────────────────────────────────────────
// ExecutionReport — full auditable output for one execution run
// ────────────────────────────────────────────────────────────────────────────

/// The authoritative, fully auditable record of one WorkItem execution attempt.
///
/// Returned by `WorkItemExecutor::execute()` and serialised as JSON in the
/// HTTP response to `POST /api/work/{id}/run`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReport {
    /// The work item that was executed.
    pub work_item_id: String,
    /// Execution path determined in step 2.
    pub execution_path: Option<crate::planning::ExecutionPath>,
    /// Adapter selected in step 4.
    pub adapter_selection: Option<AdapterSelection>,
    /// Concrete objective defined in step 3.
    pub objective: Option<ExecutionObjective>,
    /// Trace of the bounded action executed in step 6.
    pub action_result: Option<ActionResult>,
    /// Verification outcome from step 7.
    pub verification: Option<VerificationResult>,
    /// Final execution status.
    pub status: ExecutionStatus,
    /// Populated when `status == Blocked`, explains why.
    pub block_reason: Option<BlockReason>,
    /// Human-readable one-line summary of the outcome.
    pub result_summary: String,
    /// Keys of artifacts written to the work item payload.
    pub artifact_refs: Vec<String>,
    /// How many retries have been consumed (including this attempt).
    pub retry_count: u32,
    /// Whether another retry has been queued.
    pub retry_scheduled: bool,
    /// ID of the child work item created if delegation occurred.
    pub delegated_to: Option<String>,
    /// Total USD cost for this attempt.
    pub cost_usd: Option<f64>,
    /// Non-fatal warnings generated during execution.
    pub warnings: Vec<String>,
    /// Ordered list of event kinds emitted during this run.
    pub events_emitted: Vec<ExecutionEventKind>,
    /// When the execution loop started.
    pub started_at: DateTime<Utc>,
    /// When the execution loop completed (all steps done).
    pub finished_at: DateTime<Utc>,
}

impl ExecutionReport {
    /// Build a minimal blocked report for early-exit scenarios.
    pub fn blocked(
        work_item_id: impl Into<String>,
        reason: BlockReason,
        event_kind: ExecutionEventKind,
    ) -> Self {
        let now = Utc::now();
        let summary = format!("blocked: {}", event_kind.as_str());
        Self {
            work_item_id: work_item_id.into(),
            execution_path: None,
            adapter_selection: None,
            objective: None,
            action_result: None,
            verification: None,
            status: ExecutionStatus::Blocked,
            block_reason: Some(reason),
            result_summary: summary,
            artifact_refs: vec![],
            retry_count: 0,
            retry_scheduled: false,
            delegated_to: None,
            cost_usd: None,
            warnings: vec![],
            events_emitted: vec![event_kind],
            started_at: now,
            finished_at: now,
        }
    }
}
