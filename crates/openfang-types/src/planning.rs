//! BMAD-inspired Structured Planning types for the OpenFang AOS.
//!
//! Implements three core BMAD adaptations:
//!
//! **Rule 1 — WorkItemContextBundle**: Every WorkItem accumulates a progressive
//! context bundle as it moves through planning and execution phases. Nothing is
//! ever deleted from context; each phase appends entries.
//!
//! **Rule 2 — ScopeClassification**: Every task is routed through a scope
//! classifier before execution. The classifier decides whether the task takes the
//! fast path (single agent, no planning round) or the full swarm path (multi-agent
//! with a mandatory `StructuredPlanningRound`).
//!
//! **Rule 6 — StructuredPlanningRound**: Every planned swarm action runs a
//! deterministic five-role deliberation round before transitioning the WorkItem to
//! `Running`. Turn order is `Planner → Reviewer → RiskChecker → PolicyGate →
//! Executor`. Out-of-order turns, sequence mismatches, and unauthorised vetoes are
//! hard errors. A single veto from `RiskChecker` or `PolicyGate` immediately halts
//! the round.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Scope Classification — Rule 2: quick path vs full swarm routing
// ---------------------------------------------------------------------------

/// The resolved execution path for a WorkItem.
///
/// Determined by `ScopeClassification` after evaluating the work item's
/// risk, complexity, and context signals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPath {
    /// Single agent, single call. No planning round required.
    /// Suitable for trivial, low-risk, internal-only tasks.
    #[default]
    FastPath,
    /// Multi-agent swarm with a mandatory `StructuredPlanningRound`.
    /// Required when risk ≥ Medium, external services are involved, or the
    /// work type is Research / Workflow.
    PlannedSwarm,
    /// Multi-agent swarm with pre- AND post-execution human review.
    /// Required for High-risk tasks, PII handling, or external communications.
    ReviewSwarm,
}

impl ExecutionPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FastPath => "fast_path",
            Self::PlannedSwarm => "planned_swarm",
            Self::ReviewSwarm => "review_swarm",
        }
    }

    /// Returns `true` if a `StructuredPlanningRound` is required.
    pub fn requires_planning_round(&self) -> bool {
        matches!(self, Self::PlannedSwarm | Self::ReviewSwarm)
    }
}

impl std::fmt::Display for ExecutionPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A single signal that influenced the scope classification decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeSignal {
    /// Short machine-readable code, e.g. `"high_risk"`, `"external_network"`.
    pub code: String,
    /// Human-readable description of what this signal represents.
    pub description: String,
    /// Weight of this signal in the classification decision (0.0–1.0).
    pub weight: f32,
}

/// Output of the scope classifier — determines execution path for a WorkItem.
///
/// Produced once per WorkItem, stored in its `WorkItemContextBundle`.
/// Immutable after creation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeClassification {
    /// The resolved execution path.
    pub path: ExecutionPath,
    /// Signals that drove this decision.
    pub signals: Vec<ScopeSignal>,
    /// Human-readable justification for the decision.
    pub rationale: String,
    /// Whether a full `StructuredPlanningRound` is required before execution.
    pub requires_planning_round: bool,
    /// Whether human sign-off is required before execution starts.
    pub requires_pre_approval: bool,
    /// Whether human sign-off is required after agent output is produced.
    pub requires_post_approval: bool,
    /// When this classification was produced.
    pub classified_at: DateTime<Utc>,
}

impl ScopeClassification {
    /// Construct a fast-path classification with a single reason.
    pub fn fast_path(reason: impl Into<String>) -> Self {
        Self {
            path: ExecutionPath::FastPath,
            signals: vec![],
            rationale: reason.into(),
            requires_planning_round: false,
            requires_pre_approval: false,
            requires_post_approval: false,
            classified_at: Utc::now(),
        }
    }
}

// ---------------------------------------------------------------------------
// PlanningRole — turn order definition
// ---------------------------------------------------------------------------

/// A participant role in a `StructuredPlanningRound`.
///
/// The turn order is DETERMINISTIC and ENFORCED:
///
/// ```text
/// Planner (0) → Reviewer (1) → RiskChecker (2) → PolicyGate (3) → Executor (4)
/// ```
///
/// Turns submitted out of sequence are rejected with [`PlanningError::TurnOrderViolation`].
/// Only `RiskChecker` and `PolicyGate` may cast a `Vetoed` verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanningRole {
    /// Decomposes the WorkItem into a concrete, actionable execution plan.
    Planner,
    /// Reviews the plan for completeness, correctness, and coherence.
    Reviewer,
    /// Assesses risk exposure of the proposed execution.
    RiskChecker,
    /// Verifies the plan conforms to project policy and constitutional rules.
    PolicyGate,
    /// Confirms readiness and takes execution ownership.
    Executor,
}

impl PlanningRole {
    /// Returns the required sequence index for this role (0-based).
    ///
    /// A `PlanningTurn` is only valid when `turn.sequence == turn.role.sequence_index()`.
    pub fn sequence_index(&self) -> u8 {
        match self {
            Self::Planner => 0,
            Self::Reviewer => 1,
            Self::RiskChecker => 2,
            Self::PolicyGate => 3,
            Self::Executor => 4,
        }
    }

    /// Returns the role that must have submitted a non-vetoed turn before this
    /// role may submit its own.
    pub fn predecessor(&self) -> Option<PlanningRole> {
        match self {
            Self::Planner => None,
            Self::Reviewer => Some(Self::Planner),
            Self::RiskChecker => Some(Self::Reviewer),
            Self::PolicyGate => Some(Self::RiskChecker),
            Self::Executor => Some(Self::PolicyGate),
        }
    }

    /// Returns `true` if this role may cast a `Vetoed` verdict, halting the round.
    ///
    /// Only `RiskChecker` and `PolicyGate` have veto power.
    pub fn has_veto_power(&self) -> bool {
        matches!(self, Self::RiskChecker | Self::PolicyGate)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Planner => "planner",
            Self::Reviewer => "reviewer",
            Self::RiskChecker => "risk_checker",
            Self::PolicyGate => "policy_gate",
            Self::Executor => "executor",
        }
    }
}

impl std::fmt::Display for PlanningRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ---------------------------------------------------------------------------
// PlanningTurn — a single structured participant response
// ---------------------------------------------------------------------------

/// The verdict a planning participant returns for their turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanningVerdict {
    /// Approved — round may advance to the next role.
    Approved,
    /// Approved, but the next role must address the listed conditions.
    ApprovedWithConditions,
    /// Deferred — this role needs additional information before deciding.
    Deferred,
    /// Vetoed — round is halted immediately.
    ///
    /// Only [`PlanningRole::RiskChecker`] and [`PlanningRole::PolicyGate`] may
    /// issue this verdict. Any other role attempting a veto receives
    /// [`PlanningError::UnauthorizedVeto`].
    Vetoed,
}

/// A structured annotation attached to a planning turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanningAnnotation {
    /// Category: `"risk"`, `"cost"`, `"compliance"`, `"dependency"`, `"quality"`.
    pub category: String,
    /// Short annotation text.
    pub text: String,
    /// Severity: `"info"`, `"warning"`, `"blocker"`.
    pub severity: String,
}

/// A single structured turn in a `StructuredPlanningRound`.
///
/// One agent (identified by `agent_id`) speaks for one `PlanningRole`.
/// Turns MUST arrive in `role` sequence order (0–4). Out-of-order turns
/// are rejected with [`PlanningError::TurnOrderViolation`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanningTurn {
    /// Unique turn identifier.
    pub id: String,
    /// The round this turn belongs to.
    pub round_id: String,
    /// The role this turn represents.
    pub role: PlanningRole,
    /// Sequence index — must equal `role.sequence_index()` or the turn is rejected.
    pub sequence: u8,
    /// The agent that produced this turn.
    pub agent_id: String,
    /// The agent's display name.
    pub agent_name: String,
    /// The structured content from this participant (plan, review, risk assessment, etc.).
    pub content: String,
    /// Structured annotations (risks, compliance flags, cost notes).
    #[serde(default)]
    pub annotations: Vec<PlanningAnnotation>,
    /// The verdict for this turn.
    pub verdict: PlanningVerdict,
    /// Conditions that must be addressed if verdict is `ApprovedWithConditions`.
    #[serde(default)]
    pub conditions: Vec<String>,
    /// When this turn was submitted.
    pub submitted_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// StructuredPlanningRound
// ---------------------------------------------------------------------------

/// Status of a `StructuredPlanningRound`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlanningRoundStatus {
    /// Round is open; turns are still being submitted.
    #[default]
    Open,
    /// All five roles submitted turns with `Approved` or `ApprovedWithConditions`.
    /// The WorkItem is now cleared for execution.
    Approved,
    /// A vetoing role halted the round. The WorkItem must NOT transition to `Running`.
    Vetoed,
    /// The round expired without completing.
    TimedOut,
    /// Round was aborted (e.g., WorkItem was cancelled).
    Aborted,
}

/// A complete pre-execution deliberation round for a WorkItem.
///
/// A WorkItem in `Ready` state transitions to `Running` ONLY when the attached
/// `StructuredPlanningRound` reaches `PlanningRoundStatus::Approved`.
///
/// ## Turn Order Enforcement Rules
///
/// 1. Turns must arrive in ascending `role.sequence_index()` order (0 → 4).
/// 2. A turn is rejected if `turn.sequence != turn.role.sequence_index()`.
/// 3. A turn is rejected if `turn.role != self.next_expected_role`.
/// 4. A `Vetoed` verdict from any role without `has_veto_power() == true` is rejected.
/// 5. A `Vetoed` verdict from `RiskChecker` or `PolicyGate` sets the round status
///    to `Vetoed` immediately; no further turns are accepted.
/// 6. When all five roles have submitted non-vetoed turns, the round status becomes
///    `Approved` and `approved_plan` is derived from the `Executor`'s content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredPlanningRound {
    /// Unique round identifier.
    pub id: String,
    /// The WorkItem this round gates.
    pub work_item_id: String,
    /// The `SwarmPlan` this round is validating.
    pub swarm_plan_id: String,
    /// Current round status.
    pub status: PlanningRoundStatus,
    /// Ordered turns submitted so far.
    pub turns: Vec<PlanningTurn>,
    /// The next role expected to submit a turn (enforced by `submit_turn`).
    pub next_expected_role: PlanningRole,
    /// Maximum number of amendment iterations before the round is timed out.
    #[serde(default = "default_max_rounds")]
    pub max_rounds: u8,
    /// Current iteration count.
    #[serde(default)]
    pub current_round: u8,
    /// Hard deadline — if the round is not `Approved` by this time it becomes `TimedOut`.
    #[serde(default)]
    pub deadline: Option<DateTime<Utc>>,
    /// The consolidated execution plan (set when status becomes `Approved`).
    #[serde(default)]
    pub approved_plan: Option<String>,
    /// The actor (agent ID or `"system"`) who locked the approved plan.
    #[serde(default)]
    pub locked_by: Option<String>,
    /// When the plan was locked.
    #[serde(default)]
    pub locked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl StructuredPlanningRound {
    /// Create a new, open planning round.
    pub fn new(
        id: impl Into<String>,
        work_item_id: impl Into<String>,
        swarm_plan_id: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            work_item_id: work_item_id.into(),
            swarm_plan_id: swarm_plan_id.into(),
            status: PlanningRoundStatus::Open,
            turns: Vec::new(),
            next_expected_role: PlanningRole::Planner,
            max_rounds: default_max_rounds(),
            current_round: 0,
            deadline: None,
            approved_plan: None,
            locked_by: None,
            locked_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Validate and submit a new turn.
    ///
    /// # Errors
    ///
    /// Returns [`PlanningError`] if:
    /// - The turn's sequence index doesn't match `turn.role.sequence_index()`
    /// - The turn's role is not the next expected role
    /// - The round is no longer `Open`
    /// - A veto is attempted by a role without `has_veto_power() == true`
    pub fn submit_turn(&mut self, turn: PlanningTurn) -> Result<(), PlanningError> {
        // Rule: round must be open.
        if self.status != PlanningRoundStatus::Open {
            return Err(PlanningError::RoundNotOpen {
                status: format!("{:?}", self.status),
            });
        }

        // Rule: sequence index must match role's expected index.
        let expected_seq = turn.role.sequence_index();
        if turn.sequence != expected_seq {
            return Err(PlanningError::SequenceIndexMismatch {
                role: turn.role.as_str(),
                expected: expected_seq,
                got: turn.sequence,
            });
        }

        // Rule: role must be the next expected role.
        if turn.role != self.next_expected_role {
            return Err(PlanningError::TurnOrderViolation {
                expected: self.next_expected_role.as_str(),
                got: turn.role.as_str(),
            });
        }

        // Rule: only RiskChecker and PolicyGate may veto.
        if turn.verdict == PlanningVerdict::Vetoed && !turn.role.has_veto_power() {
            return Err(PlanningError::UnauthorizedVeto {
                role: turn.role.as_str(),
            });
        }

        // Apply veto: close the round immediately.
        if turn.verdict == PlanningVerdict::Vetoed {
            self.status = PlanningRoundStatus::Vetoed;
            self.turns.push(turn);
            self.updated_at = Utc::now();
            return Ok(());
        }

        // Final turn (Executor) approves the round.
        if turn.role == PlanningRole::Executor {
            let plan_content = turn.content.clone();
            self.turns.push(turn);
            self.status = PlanningRoundStatus::Approved;
            self.approved_plan = Some(plan_content);
            self.locked_at = Some(Utc::now());
            self.updated_at = Utc::now();
            return Ok(());
        }

        // Advance to the next expected role.
        self.next_expected_role = match turn.role {
            PlanningRole::Planner => PlanningRole::Reviewer,
            PlanningRole::Reviewer => PlanningRole::RiskChecker,
            PlanningRole::RiskChecker => PlanningRole::PolicyGate,
            PlanningRole::PolicyGate => PlanningRole::Executor,
            PlanningRole::Executor => unreachable!("handled above"),
        };
        self.turns.push(turn);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Returns `true` if this round has reached a terminal state.
    pub fn is_terminal(&self) -> bool {
        !matches!(self.status, PlanningRoundStatus::Open)
    }

    /// Returns the turn submitted for a given role, if any.
    pub fn turn_for_role(&self, role: PlanningRole) -> Option<&PlanningTurn> {
        self.turns.iter().find(|t| t.role == role)
    }
}

fn default_max_rounds() -> u8 {
    3
}

// ---------------------------------------------------------------------------
// WorkItemContextBundle — Rule 1: every WorkItem gets a context bundle
// ---------------------------------------------------------------------------

/// A progressive context bundle attached to a WorkItem.
///
/// Context accumulates as the WorkItem moves through classification, planning,
/// and execution phases. Entries are append-only — nothing is ever deleted.
///
/// The bundle is stored separately from the `WorkItem` and indexed by
/// `work_item_id`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkItemContextBundle {
    /// The WorkItem this bundle belongs to.
    pub work_item_id: String,
    /// Scope classification (set after the Rule 2 classifier runs).
    #[serde(default)]
    pub scope: Option<ScopeClassification>,
    /// Reference to the planning round (set when path requires one).
    #[serde(default)]
    pub planning_round_id: Option<String>,
    /// Key facts about the project derived from the constitutional context file.
    #[serde(default)]
    pub project_context_entries: Vec<ContextEntry>,
    /// Research artifacts gathered by Research-role agents during planning.
    #[serde(default)]
    pub research_artifacts: Vec<ContextEntry>,
    /// Tool results collected during planning or execution.
    #[serde(default)]
    pub tool_results: Vec<ContextEntry>,
    /// Validation and risk notes from RiskChecker and PolicyGate.
    #[serde(default)]
    pub validation_notes: Vec<ContextEntry>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WorkItemContextBundle {
    /// Create an empty bundle for a WorkItem.
    pub fn new(work_item_id: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            work_item_id: work_item_id.into(),
            created_at: now,
            updated_at: now,
            ..Default::default()
        }
    }

    /// Append a context entry to the appropriate bucket.
    pub fn append(&mut self, entry: ContextEntry) {
        match entry.source.as_str() {
            "project_context" => self.project_context_entries.push(entry),
            "research" => self.research_artifacts.push(entry),
            "tool" => self.tool_results.push(entry),
            "validation" => self.validation_notes.push(entry),
            _ => self.tool_results.push(entry),
        }
        self.updated_at = Utc::now();
    }

    /// Total number of context entries across all buckets.
    pub fn entry_count(&self) -> usize {
        self.project_context_entries.len()
            + self.research_artifacts.len()
            + self.tool_results.len()
            + self.validation_notes.len()
    }
}

/// A single append-only entry in a `WorkItemContextBundle`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEntry {
    /// Unique entry identifier.
    pub id: String,
    /// What produced this entry: `"project_context"`, `"research"`, `"tool"`, `"validation"`.
    pub source: String,
    /// The agent ID or `"system"` that created this entry.
    pub author: String,
    /// Text content of this entry.
    pub content: String,
    /// Optional structured JSON data.
    #[serde(default)]
    pub data: serde_json::Value,
    /// When this entry was appended (append-only; never updated).
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// PlanningError
// ---------------------------------------------------------------------------

/// Errors that can occur during a `StructuredPlanningRound`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanningError {
    /// A turn was submitted for a role that is not `next_expected_role`.
    TurnOrderViolation {
        expected: &'static str,
        got: &'static str,
    },
    /// The `sequence` field on the turn doesn't match `role.sequence_index()`.
    SequenceIndexMismatch {
        role: &'static str,
        expected: u8,
        got: u8,
    },
    /// A `Vetoed` verdict was attempted by a role without `has_veto_power()`.
    UnauthorizedVeto { role: &'static str },
    /// The round is not in `Open` status (already terminal).
    RoundNotOpen { status: String },
}

impl std::fmt::Display for PlanningError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TurnOrderViolation { expected, got } => {
                write!(f, "turn order violation: expected '{expected}', got '{got}'")
            }
            Self::SequenceIndexMismatch { role, expected, got } => {
                write!(
                    f,
                    "sequence mismatch for role '{role}': expected {expected}, got {got}"
                )
            }
            Self::UnauthorizedVeto { role } => {
                write!(f, "role '{role}' does not have veto power")
            }
            Self::RoundNotOpen { status } => {
                write!(f, "planning round is not open (status: {status})")
            }
        }
    }
}

impl std::error::Error for PlanningError {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_turn(round_id: &str, role: PlanningRole, verdict: PlanningVerdict) -> PlanningTurn {
        PlanningTurn {
            id: format!("turn-{}", role.as_str()),
            round_id: round_id.to_string(),
            sequence: role.sequence_index(),
            role,
            agent_id: "agent-test".to_string(),
            agent_name: "Test Agent".to_string(),
            content: format!("{} content", role.as_str()),
            annotations: vec![],
            verdict,
            conditions: vec![],
            submitted_at: Utc::now(),
        }
    }

    fn open_round() -> StructuredPlanningRound {
        StructuredPlanningRound::new("round-1", "wi-1", "plan-1")
    }

    // --- Happy path ----------------------------------------------------------

    #[test]
    fn full_round_reaches_approved() {
        let mut round = open_round();
        round
            .submit_turn(make_turn("round-1", PlanningRole::Planner, PlanningVerdict::Approved))
            .unwrap();
        round
            .submit_turn(make_turn("round-1", PlanningRole::Reviewer, PlanningVerdict::Approved))
            .unwrap();
        round
            .submit_turn(make_turn(
                "round-1",
                PlanningRole::RiskChecker,
                PlanningVerdict::Approved,
            ))
            .unwrap();
        round
            .submit_turn(make_turn(
                "round-1",
                PlanningRole::PolicyGate,
                PlanningVerdict::Approved,
            ))
            .unwrap();
        round
            .submit_turn(make_turn("round-1", PlanningRole::Executor, PlanningVerdict::Approved))
            .unwrap();

        assert_eq!(round.status, PlanningRoundStatus::Approved);
        assert!(round.approved_plan.is_some());
        assert!(round.locked_at.is_some());
        assert_eq!(round.turns.len(), 5);
    }

    #[test]
    fn approved_round_is_terminal() {
        let mut round = open_round();
        for role in [
            PlanningRole::Planner,
            PlanningRole::Reviewer,
            PlanningRole::RiskChecker,
            PlanningRole::PolicyGate,
            PlanningRole::Executor,
        ] {
            round
                .submit_turn(make_turn("round-1", role, PlanningVerdict::Approved))
                .unwrap();
        }
        assert!(round.is_terminal());
    }

    // --- Turn order enforcement ---------------------------------------------

    #[test]
    fn out_of_order_turn_is_rejected() {
        let mut round = open_round();
        // Submit Reviewer before Planner.
        let err = round
            .submit_turn(make_turn("round-1", PlanningRole::Reviewer, PlanningVerdict::Approved))
            .unwrap_err();
        assert!(
            matches!(err, PlanningError::TurnOrderViolation { .. }),
            "expected TurnOrderViolation, got: {err}"
        );
    }

    #[test]
    fn wrong_sequence_index_is_rejected() {
        let mut round = open_round();
        let mut bad_turn = make_turn("round-1", PlanningRole::Planner, PlanningVerdict::Approved);
        bad_turn.sequence = 99; // Deliberate mismatch.
        let err = round.submit_turn(bad_turn).unwrap_err();
        assert!(matches!(err, PlanningError::SequenceIndexMismatch { .. }));
    }

    #[test]
    fn submit_after_terminal_is_rejected() {
        let mut round = open_round();
        round
            .submit_turn(make_turn("round-1", PlanningRole::Planner, PlanningVerdict::Approved))
            .unwrap();
        round
            .submit_turn(make_turn("round-1", PlanningRole::Reviewer, PlanningVerdict::Approved))
            .unwrap();
        // Veto to close the round.
        round
            .submit_turn(make_turn(
                "round-1",
                PlanningRole::RiskChecker,
                PlanningVerdict::Vetoed,
            ))
            .unwrap();
        // Attempt to submit after veto.
        let err = round
            .submit_turn(make_turn(
                "round-1",
                PlanningRole::PolicyGate,
                PlanningVerdict::Approved,
            ))
            .unwrap_err();
        assert!(matches!(err, PlanningError::RoundNotOpen { .. }));
    }

    // --- Veto rules ---------------------------------------------------------

    #[test]
    fn risk_checker_veto_halts_round() {
        let mut round = open_round();
        round
            .submit_turn(make_turn("round-1", PlanningRole::Planner, PlanningVerdict::Approved))
            .unwrap();
        round
            .submit_turn(make_turn("round-1", PlanningRole::Reviewer, PlanningVerdict::Approved))
            .unwrap();
        round
            .submit_turn(make_turn(
                "round-1",
                PlanningRole::RiskChecker,
                PlanningVerdict::Vetoed,
            ))
            .unwrap();
        assert_eq!(round.status, PlanningRoundStatus::Vetoed);
        assert!(round.is_terminal());
    }

    #[test]
    fn policy_gate_veto_halts_round() {
        let mut round = open_round();
        round
            .submit_turn(make_turn("round-1", PlanningRole::Planner, PlanningVerdict::Approved))
            .unwrap();
        round
            .submit_turn(make_turn("round-1", PlanningRole::Reviewer, PlanningVerdict::Approved))
            .unwrap();
        round
            .submit_turn(make_turn(
                "round-1",
                PlanningRole::RiskChecker,
                PlanningVerdict::Approved,
            ))
            .unwrap();
        round
            .submit_turn(make_turn(
                "round-1",
                PlanningRole::PolicyGate,
                PlanningVerdict::Vetoed,
            ))
            .unwrap();
        assert_eq!(round.status, PlanningRoundStatus::Vetoed);
    }

    #[test]
    fn planner_cannot_veto() {
        let mut round = open_round();
        let err = round
            .submit_turn(make_turn("round-1", PlanningRole::Planner, PlanningVerdict::Vetoed))
            .unwrap_err();
        assert!(matches!(err, PlanningError::UnauthorizedVeto { .. }));
    }

    #[test]
    fn reviewer_cannot_veto() {
        let mut round = open_round();
        round
            .submit_turn(make_turn("round-1", PlanningRole::Planner, PlanningVerdict::Approved))
            .unwrap();
        let err = round
            .submit_turn(make_turn("round-1", PlanningRole::Reviewer, PlanningVerdict::Vetoed))
            .unwrap_err();
        assert!(matches!(err, PlanningError::UnauthorizedVeto { .. }));
    }

    #[test]
    fn executor_cannot_veto() {
        let mut round = open_round();
        for role in [
            PlanningRole::Planner,
            PlanningRole::Reviewer,
            PlanningRole::RiskChecker,
            PlanningRole::PolicyGate,
        ] {
            round
                .submit_turn(make_turn("round-1", role, PlanningVerdict::Approved))
                .unwrap();
        }
        let err = round
            .submit_turn(make_turn("round-1", PlanningRole::Executor, PlanningVerdict::Vetoed))
            .unwrap_err();
        assert!(matches!(err, PlanningError::UnauthorizedVeto { .. }));
    }

    // --- PlanningRole helpers -----------------------------------------------

    #[test]
    fn predecessor_chain_is_correct() {
        assert!(PlanningRole::Planner.predecessor().is_none());
        assert_eq!(
            PlanningRole::Reviewer.predecessor(),
            Some(PlanningRole::Planner)
        );
        assert_eq!(
            PlanningRole::RiskChecker.predecessor(),
            Some(PlanningRole::Reviewer)
        );
        assert_eq!(
            PlanningRole::PolicyGate.predecessor(),
            Some(PlanningRole::RiskChecker)
        );
        assert_eq!(
            PlanningRole::Executor.predecessor(),
            Some(PlanningRole::PolicyGate)
        );
    }

    #[test]
    fn only_risk_and_policy_have_veto_power() {
        assert!(!PlanningRole::Planner.has_veto_power());
        assert!(!PlanningRole::Reviewer.has_veto_power());
        assert!(PlanningRole::RiskChecker.has_veto_power());
        assert!(PlanningRole::PolicyGate.has_veto_power());
        assert!(!PlanningRole::Executor.has_veto_power());
    }

    #[test]
    fn sequence_indices_are_strictly_ordered() {
        let roles = [
            PlanningRole::Planner,
            PlanningRole::Reviewer,
            PlanningRole::RiskChecker,
            PlanningRole::PolicyGate,
            PlanningRole::Executor,
        ];
        for (i, role) in roles.iter().enumerate() {
            assert_eq!(role.sequence_index(), i as u8);
        }
    }

    #[test]
    fn execution_path_planning_round_requirement() {
        assert!(!ExecutionPath::FastPath.requires_planning_round());
        assert!(ExecutionPath::PlannedSwarm.requires_planning_round());
        assert!(ExecutionPath::ReviewSwarm.requires_planning_round());
    }

    // --- WorkItemContextBundle -----------------------------------------------

    #[test]
    fn context_bundle_appends_to_correct_buckets() {
        let mut bundle = WorkItemContextBundle::new("wi-1");
        let now = Utc::now();

        bundle.append(ContextEntry {
            id: "e1".to_string(),
            source: "project_context".to_string(),
            author: "system".to_string(),
            content: "Project uses Rust.".to_string(),
            data: serde_json::Value::Null,
            created_at: now,
        });
        bundle.append(ContextEntry {
            id: "e2".to_string(),
            source: "validation".to_string(),
            author: "risk_checker".to_string(),
            content: "No high-risk dependencies found.".to_string(),
            data: serde_json::Value::Null,
            created_at: now,
        });

        assert_eq!(bundle.project_context_entries.len(), 1);
        assert_eq!(bundle.validation_notes.len(), 1);
        assert_eq!(bundle.entry_count(), 2);
    }

    #[test]
    fn scope_classification_fast_path_helper() {
        let sc = ScopeClassification::fast_path("trivial greeting");
        assert_eq!(sc.path, ExecutionPath::FastPath);
        assert!(!sc.requires_planning_round);
        assert!(!sc.requires_pre_approval);
        assert!(!sc.requires_post_approval);
    }

    // --- turn_for_role lookup -----------------------------------------------

    #[test]
    fn turn_for_role_returns_correct_turn() {
        let mut round = open_round();
        round
            .submit_turn(make_turn("round-1", PlanningRole::Planner, PlanningVerdict::Approved))
            .unwrap();
        let turn = round.turn_for_role(PlanningRole::Planner);
        assert!(turn.is_some());
        assert_eq!(turn.unwrap().role, PlanningRole::Planner);
        assert!(round.turn_for_role(PlanningRole::Reviewer).is_none());
    }
}
