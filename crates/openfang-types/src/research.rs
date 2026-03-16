//! Autoresearch types — OpenFang's disciplined experiment loop.
//!
//! Implements the three core concepts from the autoresearch model:
//!
//! **ControlPlaneConfig** — typed equivalent of a `program.md` control file.
//! Specifies what the system is allowed to mutate, which adapters are preferred,
//! approval and retry policies, scoring rules, and forbidden actions.
//!
//! **ResearchExperiment** — a single tracked research run, from hypothesis to
//! acceptance or rejection.  Every run records which validated patterns were
//! reused, a `SelectionTrace` for each role decision, and the final score.
//!
//! **Role separation** — `Planner` forms the hypothesis and selects tools,
//! `Executor` runs the action, `Reviewer` scores the result and decides
//! promotion.  Each role is an `AgentId`; they may be the same agent or
//! different agents depending on configuration.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Control plane config  (program.md equivalent)
// ---------------------------------------------------------------------------

/// The top-level decision contract for the autoresearch system.
///
/// One `ControlPlaneConfig` is active per research session.  It is loaded
/// from the agent's `program.md` equivalent at session start and can be
/// updated via the `/api/research/control-plane` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlPlaneConfig {
    /// Human-readable label for this config version.
    pub label: String,

    /// Surfaces that may be mutated during this session.
    ///
    /// Examples: `["database:users", "filesystem:/tmp", "api:sendgrid"]`
    pub mutation_surfaces: Vec<String>,

    /// Adapter priority list — chosen adapters come from this list in order.
    pub adapter_priority: Vec<String>,

    /// When additional approval is required.
    pub approval_policy: ApprovalPolicy,

    /// How failed experiments should be retried.
    pub retry_policy: RetryPolicy,

    /// Constraints on sub-agent delegation.
    pub delegation_policy: DelegationPolicy,

    /// Rules used by the Reviewer role to score experiments.
    pub scoring_rules: ScoringRules,

    /// Strings that must not appear in any generated action or tool call.
    /// Checked by the PolicyGate before any executor call.
    pub forbidden_actions: Vec<String>,
}

impl Default for ControlPlaneConfig {
    fn default() -> Self {
        Self {
            label: "default".into(),
            mutation_surfaces: vec![],
            adapter_priority: vec!["api".into(), "cli".into()],
            approval_policy: ApprovalPolicy::default(),
            retry_policy: RetryPolicy::default(),
            delegation_policy: DelegationPolicy::default(),
            scoring_rules: ScoringRules::default(),
            forbidden_actions: vec![],
        }
    }
}

/// When does a research action require explicit approval?
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalPolicy {
    /// No approval required — act immediately.
    #[default]
    Automatic,
    /// Always require a human or PolicyGate approval before executing.
    AlwaysRequired,
    /// Require approval only when the computed risk score exceeds the threshold.
    RiskBased {
        /// Risk score threshold (0.0–1.0).  Default 0.7.
        threshold: f32,
    },
}

/// How the executor retries failed experiments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts.
    pub max_attempts: u32,
    /// Seconds to wait before each retry (linear backoff).
    pub backoff_secs: u32,
    /// Whether to retry automatically when verification fails.
    pub retry_on_verification_fail: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self { max_attempts: 3, backoff_secs: 5, retry_on_verification_fail: true }
    }
}

/// Constraints on when and how the system may spawn sub-agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationPolicy {
    /// Maximum delegation depth (0 = no delegation allowed).
    pub max_depth: u32,
    /// Agent role tags allowed to be delegated to.
    /// Empty list means any role is permitted.
    pub allowed_agent_roles: Vec<String>,
}

impl Default for DelegationPolicy {
    fn default() -> Self {
        Self { max_depth: 2, allowed_agent_roles: vec![] }
    }
}

/// Rules that the Reviewer uses to compute an `ExperimentScore`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringRules {
    /// Minimum composite score required for promotion (0.0–1.0).
    pub min_score_to_promote: f32,
    /// How many reviewers must score the experiment before it can be promoted.
    pub reviewer_count_required: u32,
    /// Weight for each scoring dimension.
    pub weights: ScoreWeights,
}

impl Default for ScoringRules {
    fn default() -> Self {
        Self {
            min_score_to_promote: 0.7,
            reviewer_count_required: 1,
            weights: ScoreWeights::default(),
        }
    }
}

/// Per-dimension weights used to compute the composite score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreWeights {
    pub relevance:   f32,
    pub correctness: f32,
    pub efficiency:  f32,
    pub safety:      f32,
}

impl Default for ScoreWeights {
    fn default() -> Self {
        Self { relevance: 0.25, correctness: 0.40, efficiency: 0.20, safety: 0.15 }
    }
}

// ---------------------------------------------------------------------------
// Role separation
// ---------------------------------------------------------------------------

/// The three roles in the autoresearch loop.
///
/// Each role must be filled by an agent ID at plan time.  A single agent may
/// hold multiple roles (e.g., a solo researcher holds all three), but the
/// `StructuredPlanningRound` enforces that Planner decisions are reviewed
/// before Executor calls proceed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchRole {
    /// Forms the hypothesis, decomposes the research question, selects
    /// adapters, and writes the experiment specification.
    Planner,
    /// Runs the experiment exactly as specified — no ad-hoc mutations.
    Executor,
    /// Scores the raw results, decides whether the output passes verification,
    /// and gates promotion to validated patterns.
    Reviewer,
}

impl ResearchRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Planner  => "planner",
            Self::Executor => "executor",
            Self::Reviewer => "reviewer",
        }
    }
}

// ---------------------------------------------------------------------------
// Experiment
// ---------------------------------------------------------------------------

/// Current lifecycle status of a `ResearchExperiment`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentStatus {
    /// Created but not yet assigned to an executor.
    #[default]
    Planned,
    /// Currently being executed.
    Running,
    /// Execution done; awaiting reviewer score.
    AwaitingReview,
    /// Reviewer has scored; outcome is in `promotion_status`.
    Reviewed,
    /// Terminated due to a hard error or forbidden action gate.
    Aborted,
}

/// A single tracked research run from hypothesis through promotion decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchExperiment {
    /// Unique experiment identifier.
    pub id: String,
    /// Optional link to the `WorkItem` that triggered this experiment.
    pub work_item_id: Option<String>,
    /// The hypothesis being tested.
    pub hypothesis: String,
    /// Agent ID filling the Planner role.
    pub planner_id: String,
    /// Agent ID filling the Executor role (assigned at run time).
    pub executor_id: Option<String>,
    /// Agent ID filling the Reviewer role (assigned at run time).
    pub reviewer_id: Option<String>,
    /// Current lifecycle position.
    pub status: ExperimentStatus,
    /// Reviewer's scorecard (set after status transitions to `Reviewed`).
    pub score: Option<ExperimentScore>,
    /// Final promotion decision.
    pub promotion_status: Option<PromotionStatus>,
    /// IDs of `ValidatedPattern`s that were reused in this experiment.
    pub validated_patterns_applied: Vec<String>,
    /// Free-form summary written by the Reviewer.
    pub result_summary: Option<String>,
    /// Ordered trace of decisions made by each role.
    pub selection_trace: Vec<SelectionTrace>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl ResearchExperiment {
    pub fn new(id: String, hypothesis: String, planner_id: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            work_item_id: None,
            hypothesis,
            planner_id,
            executor_id: None,
            reviewer_id: None,
            status: ExperimentStatus::Planned,
            score: None,
            promotion_status: None,
            validated_patterns_applied: vec![],
            result_summary: None,
            selection_trace: vec![],
            started_at: now,
            finished_at: None,
            created_at: now,
        }
    }
}

// ---------------------------------------------------------------------------
// Scorecard
// ---------------------------------------------------------------------------

/// Per-dimension scores assigned by the Reviewer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentScore {
    /// How relevant the output was to the original hypothesis (0.0–1.0).
    pub relevance: f32,
    /// How factually correct or operationally accurate the result is (0.0–1.0).
    pub correctness: f32,
    /// Token / time / cost efficiency of the execution (0.0–1.0).
    pub efficiency: f32,
    /// Absence of unsafe mutations or policy violations (0.0–1.0).
    pub safety: f32,
    /// Weighted composite computed as `Σ(dimension * weight)`.
    pub composite: f32,
    /// Free-form reviewer comment.
    pub reviewer_notes: Option<String>,
    pub scored_at: DateTime<Utc>,
}

impl ExperimentScore {
    /// Compute the weighted composite from individual dimensions + weights.
    pub fn compute(
        relevance: f32,
        correctness: f32,
        efficiency: f32,
        safety: f32,
        weights: &ScoreWeights,
        notes: Option<String>,
    ) -> Self {
        let composite = relevance   * weights.relevance
                      + correctness * weights.correctness
                      + efficiency  * weights.efficiency
                      + safety      * weights.safety;
        Self {
            relevance,
            correctness,
            efficiency,
            safety,
            composite: composite.clamp(0.0, 1.0),
            reviewer_notes: notes,
            scored_at: Utc::now(),
        }
    }
}

// ---------------------------------------------------------------------------
// Promotion gate
// ---------------------------------------------------------------------------

/// Outcome of the promotion decision gate.
///
/// The Reviewer produces a `PromotionStatus` after scoring.  Only `Promoted`
/// entries are persisted as `ValidatedPattern`s.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromotionStatus {
    /// Passes the score threshold — ready to be persisted as a ValidatedPattern.
    Promoted,
    /// Acceptable quality but does not meet the promotion bar.
    Accepted,
    /// Does not meet minimum quality bar; result should not be reused.
    Rejected,
    /// Score is borderline — route to human or PolicyGate for final decision.
    NeedsReview,
}

impl PromotionStatus {
    /// Decide promotion status based on composite score and threshold.
    pub fn from_score(composite: f32, threshold: f32) -> Self {
        if composite >= threshold {
            Self::Promoted
        } else if composite >= threshold * 0.8 {
            Self::Accepted
        } else if composite >= threshold * 0.6 {
            Self::NeedsReview
        } else {
            Self::Rejected
        }
    }
}

// ---------------------------------------------------------------------------
// Validated pattern store
// ---------------------------------------------------------------------------

/// A reusable pattern extracted from a successful `ResearchExperiment`.
///
/// When a `PromotionStatus::Promoted` result is stored, the Reviewer writes
/// a concise description that can be injected into future Planner prompts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatedPattern {
    /// Unique pattern identifier.
    pub id: String,
    /// Short human-readable description of what this pattern does.
    pub description: String,
    /// Coarse category tag (e.g. "adapter_selection", "verification_strategy").
    pub pattern_type: String,
    /// Examples of work item IDs where this pattern was applied successfully.
    pub example_work_item_ids: Vec<String>,
    /// Number of times this pattern has been reused.
    pub times_applied: u32,
    /// Fraction of reuses where it led to a `Promoted`/`Accepted` outcome.
    pub success_rate: f32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ValidatedPattern {
    pub fn new(id: String, description: String, pattern_type: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            description,
            pattern_type,
            example_work_item_ids: vec![],
            times_applied: 0,
            success_rate: 0.0,
            created_at: now,
            updated_at: now,
        }
    }
}

// ---------------------------------------------------------------------------
// Selection trace
// ---------------------------------------------------------------------------

/// A single role decision recorded during an experiment run.
///
/// The `selection_trace` on a `ResearchExperiment` contains one entry per
/// role handoff.  Together they form a full audit trail of why the system
/// made each choice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionTrace {
    /// Which role made this decision.
    pub role: ResearchRole,
    /// Agent ID that held the role at this point.
    pub agent_id: String,
    /// Free-form reasoning for the decision made at this step.
    pub reasoning: String,
    pub selected_at: DateTime<Utc>,
}

impl SelectionTrace {
    pub fn new(role: ResearchRole, agent_id: String, reasoning: String) -> Self {
        Self { role, agent_id, reasoning, selected_at: Utc::now() }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_control_plane_has_sensible_retry_policy() {
        let cfg = ControlPlaneConfig::default();
        assert_eq!(cfg.retry_policy.max_attempts, 3);
        assert!(cfg.retry_policy.retry_on_verification_fail);
    }

    #[test]
    fn default_control_plane_has_empty_forbidden_actions() {
        let cfg = ControlPlaneConfig::default();
        assert!(cfg.forbidden_actions.is_empty());
    }

    #[test]
    fn control_plane_config_roundtrips_json() {
        let cfg = ControlPlaneConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let back: ControlPlaneConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg.label, back.label);
        assert_eq!(cfg.retry_policy.max_attempts, back.retry_policy.max_attempts);
    }

    #[test]
    fn approval_policy_defaults_to_automatic() {
        let cfg = ControlPlaneConfig::default();
        assert!(matches!(cfg.approval_policy, ApprovalPolicy::Automatic));
    }

    #[test]
    fn scoring_rules_default_weight_sum_is_one() {
        let w = ScoringRules::default().weights;
        let sum = w.relevance + w.correctness + w.efficiency + w.safety;
        assert!((sum - 1.0).abs() < 1e-5, "weights must sum to 1.0, got {sum}");
    }

    #[test]
    fn experiment_score_compute_clamps_to_range() {
        let w = ScoreWeights::default();
        let score = ExperimentScore::compute(1.0, 1.0, 1.0, 1.0, &w, None);
        assert!(score.composite <= 1.0);
        assert!(score.composite >= 0.0);
    }

    #[test]
    fn promotion_status_promoted_above_threshold() {
        assert_eq!(PromotionStatus::from_score(0.9, 0.7), PromotionStatus::Promoted);
    }

    #[test]
    fn promotion_status_accepted_just_below_threshold() {
        // 0.7 * 0.8 = 0.56  →  composite=0.65 is above 0.56 but below 0.70
        assert_eq!(PromotionStatus::from_score(0.65, 0.7), PromotionStatus::Accepted);
    }

    #[test]
    fn promotion_status_needs_review_borderline() {
        // 0.7 * 0.6 = 0.42  →  composite=0.50 is above 0.42 but below 0.56
        assert_eq!(PromotionStatus::from_score(0.50, 0.7), PromotionStatus::NeedsReview);
    }

    #[test]
    fn promotion_status_rejected_below_minimum() {
        assert_eq!(PromotionStatus::from_score(0.2, 0.7), PromotionStatus::Rejected);
    }

    #[test]
    fn research_role_as_str() {
        assert_eq!(ResearchRole::Planner.as_str(),  "planner");
        assert_eq!(ResearchRole::Executor.as_str(), "executor");
        assert_eq!(ResearchRole::Reviewer.as_str(), "reviewer");
    }

    #[test]
    fn experiment_new_starts_in_planned_state() {
        let exp = ResearchExperiment::new("e1".into(), "test".into(), "agent-1".into());
        assert_eq!(exp.status, ExperimentStatus::Planned);
        assert!(exp.score.is_none());
        assert!(exp.promotion_status.is_none());
    }

    #[test]
    fn validated_pattern_starts_with_zero_applications() {
        let p = ValidatedPattern::new("p1".into(), "desc".into(), "adapter_selection".into());
        assert_eq!(p.times_applied, 0);
    }

    #[test]
    fn selection_trace_records_role_and_agent() {
        let t = SelectionTrace::new(ResearchRole::Planner, "agent-planner".into(), "chose api adapter".into());
        assert_eq!(t.role, ResearchRole::Planner);
        assert_eq!(t.agent_id, "agent-planner");
    }

    #[test]
    fn experiment_roundtrips_json() {
        let exp = ResearchExperiment::new("e2".into(), "hypothesis".into(), "agent-2".into());
        let json = serde_json::to_string(&exp).unwrap();
        let back: ResearchExperiment = serde_json::from_str(&json).unwrap();
        assert_eq!(exp.id, back.id);
        assert_eq!(exp.hypothesis, back.hypothesis);
    }

    #[test]
    fn validated_pattern_roundtrips_json() {
        let p = ValidatedPattern::new("p2".into(), "use api adapter for CRUD".into(), "adapter_selection".into());
        let json = serde_json::to_string(&p).unwrap();
        let back: ValidatedPattern = serde_json::from_str(&json).unwrap();
        assert_eq!(p.id, back.id);
        assert_eq!(p.pattern_type, back.pattern_type);
    }
}
