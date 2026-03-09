//! Structured output types for each phase of the MAESTRO algorithm.
//!
//! Every type derives `JsonSchema` so Rig.rs can generate a JSON schema
//! and pass it to the LLM for structured extraction. This eliminates the
//! fragile regex-based parsing from Maestro's original implementation.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ── OBSERVE Phase Output ────────────────────────────────────────────────────

/// Output from the OBSERVE phase: raw information gathering about the task,
/// environment, and available resources.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ObserveOutput {
    /// Restated task in the observer's own words (ensures comprehension).
    pub task_restatement: String,

    /// Key entities, concepts, or subjects identified in the task.
    pub entities: Vec<String>,

    /// Constraints explicitly stated or implied by the task.
    pub constraints: Vec<String>,

    /// What information is available vs. what needs to be gathered.
    pub information_gaps: Vec<String>,

    /// Relevant context from prior learnings (if any).
    pub prior_learnings: Vec<String>,

    /// Available tools and capabilities that may be useful.
    pub available_capabilities: Vec<String>,

    /// Raw observations that don't fit other categories.
    pub notes: Vec<String>,
}

// ── ORIENT Phase Output ─────────────────────────────────────────────────────

/// Output from the ORIENT phase: analysis, decomposition, and strategic
/// assessment of the task.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OrientOutput {
    /// Complexity score from 1 (trivial) to 10 (extremely complex).
    /// Drives the dynamic scaling decision:
    /// - 1-3: Single agent, no orchestration
    /// - 4-6: Sequential orchestration
    /// - 7-10: Parallel orchestration with specialist agents
    pub complexity: u8,

    /// Task decomposition into logical sub-tasks.
    pub sub_tasks: Vec<SubTask>,

    /// Identified risks or failure modes.
    pub risks: Vec<Risk>,

    /// Recommended number of agents for parallel execution.
    pub recommended_agent_count: u8,

    /// Whether the task requires external data (web search, API calls, etc.).
    pub requires_external_data: bool,

    /// Whether the task produces artifacts (files, code, documents).
    pub produces_artifacts: bool,

    /// Strategic approach summary (1-2 sentences).
    pub strategy_summary: String,
}

/// A logical sub-task identified during orientation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SubTask {
    /// Short identifier (e.g., "research", "draft", "review").
    pub id: String,

    /// Human-readable description of what this sub-task accomplishes.
    pub description: String,

    /// Required capabilities (e.g., "web_search", "code_generation", "analysis").
    pub capabilities: Vec<String>,

    /// IDs of sub-tasks that must complete before this one can start.
    pub depends_on: Vec<String>,

    /// Estimated relative effort (1-5 scale).
    pub effort: u8,
}

/// A risk or failure mode identified during orientation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Risk {
    /// What could go wrong.
    pub description: String,

    /// Likelihood: "low", "medium", "high".
    pub likelihood: String,

    /// Impact: "low", "medium", "high".
    pub impact: String,

    /// Mitigation strategy.
    pub mitigation: String,
}

// ── PLAN Phase Output ───────────────────────────────────────────────────────

/// Output from the PLAN phase: concrete execution plan with ISC criteria
/// and agent assignments.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlanOutput {
    /// Ordered execution steps.
    pub steps: Vec<ExecutionStep>,

    /// Ideal State Criteria — measurable success conditions.
    pub criteria: Vec<Criterion>,

    /// Agent assignments: which agent template handles which steps.
    pub agent_assignments: Vec<AgentAssignment>,

    /// Estimated total token budget for the execution.
    pub estimated_token_budget: u64,

    /// Plan summary (1-2 sentences).
    pub plan_summary: String,
}

/// A concrete execution step in the plan.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecutionStep {
    /// Step number (1-indexed).
    pub step_number: u32,

    /// What the agent should do (detailed instruction).
    pub instruction: String,

    /// Expected output format or description.
    pub expected_output: String,

    /// Which sub-task this step belongs to.
    pub sub_task_id: String,

    /// Whether this step can run in parallel with other steps.
    pub parallelizable: bool,

    /// Maximum time allowed for this step (seconds).
    pub timeout_seconds: u64,
}

/// An Ideal State Criterion — a measurable, verifiable success condition.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Criterion {
    /// Unique criterion ID (e.g., "C1", "C2").
    pub id: String,

    /// Human-readable description of what must be true.
    pub description: String,

    /// Category of the criterion.
    pub category: CriterionCategory,

    /// How to verify this criterion mechanically.
    /// Must be concrete: "check that output contains X",
    /// "verify word count >= N", "confirm all N items present".
    pub verification_method: String,

    /// Weight for scoring (0.0 - 1.0). All weights should sum to 1.0.
    pub weight: f64,
}

/// Category of an ISC criterion.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum CriterionCategory {
    /// Output must perform a specific function correctly.
    Functional,
    /// Output must meet a quality threshold.
    Quality,
    /// Output must include all required elements.
    Completeness,
    /// Output must not violate a constraint.
    Constraint,
}

/// Maps an agent template to the steps it should execute.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentAssignment {
    /// Agent template name or role (e.g., "researcher", "coder", "writer").
    pub agent_role: String,

    /// Required capabilities for this agent.
    pub capabilities: Vec<String>,

    /// Step numbers this agent is responsible for.
    pub step_numbers: Vec<u32>,

    /// Preferred model tier: "fast" (cheap, quick), "balanced", "best" (expensive, high quality).
    pub model_tier: String,
}

// ── EXECUTE Phase Output ────────────────────────────────────────────────────

/// Output from the EXECUTE phase: results from delegated agent work.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecuteOutput {
    /// Results from each execution step.
    pub step_results: Vec<StepResult>,

    /// Overall execution summary.
    pub summary: String,

    /// Whether all steps completed successfully.
    pub all_steps_completed: bool,

    /// Total tokens consumed during execution.
    pub tokens_used: u64,
}

/// Result from a single execution step.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StepResult {
    /// Step number that was executed.
    pub step_number: u32,

    /// The agent's output text.
    pub output: String,

    /// Whether this step succeeded.
    pub success: bool,

    /// Error message if the step failed.
    pub error: Option<String>,

    /// Time taken in milliseconds.
    pub duration_ms: u64,

    /// Tokens consumed by this step.
    pub tokens_used: u64,
}

// ── VERIFY Phase Output ─────────────────────────────────────────────────────

/// Output from the VERIFY phase: mechanical verification of ISC criteria
/// against the execution output.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct VerifyOutput {
    /// Verification result for each criterion.
    pub criterion_results: Vec<CriterionResult>,

    /// Overall satisfaction score (0-100).
    pub overall_satisfaction: f64,

    /// Whether the threshold is met.
    pub threshold_met: bool,

    /// Specific feedback for improvement if threshold not met.
    pub improvement_suggestions: Vec<String>,
}

/// Verification result for a single ISC criterion.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CriterionResult {
    /// The criterion ID being verified.
    pub criterion_id: String,

    /// Verification status.
    pub status: VerificationStatus,

    /// Evidence supporting the verdict.
    pub evidence: String,

    /// Confidence in the verdict (0.0 - 1.0).
    pub confidence: f64,

    /// Score contribution (weight * status_score).
    pub score: f64,
}

/// Status of a criterion verification.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum VerificationStatus {
    /// Criterion is fully satisfied.
    Satisfied,
    /// Criterion is partially satisfied.
    Partial,
    /// Criterion is not satisfied.
    Failed,
}

// ── LEARN Phase Output ──────────────────────────────────────────────────────

/// Output from the LEARN phase: structured learnings extracted from the
/// full execution run.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LearnOutput {
    /// Structured learnings from this run.
    pub learnings: Vec<LearningEntry>,

    /// What went well.
    pub successes: Vec<String>,

    /// What went wrong or could be improved.
    pub failures: Vec<String>,

    /// Recommendations for future similar tasks.
    pub recommendations: Vec<String>,
}

/// A single structured learning entry.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LearningEntry {
    /// Category of the learning.
    pub category: LearningCategory,

    /// The insight itself.
    pub insight: String,

    /// Context in which this learning was observed.
    pub context: String,

    /// Whether this learning suggests a concrete action.
    pub actionable: bool,

    /// Suggested action if actionable.
    pub suggested_action: Option<String>,
}

/// Category of a learning entry.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum LearningCategory {
    /// About the system's capabilities or limitations.
    System,
    /// About the algorithm's performance or parameters.
    Algorithm,
    /// About a failure mode or error pattern.
    Failure,
    /// Synthesized insight combining multiple observations.
    Synthesis,
    /// Meta-reflection about the learning process itself.
    Reflection,
}

// ── ADAPT Phase Output ──────────────────────────────────────────────────────

/// Output from the ADAPT phase: proposed parameter adjustments based on
/// accumulated learnings.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AdaptOutput {
    /// Proposed parameter adjustments.
    pub adjustments: Vec<ParameterAdjustment>,

    /// Rationale for the proposed changes.
    pub rationale: String,

    /// Confidence in the proposed adjustments (0.0 - 1.0).
    pub confidence: f64,
}

/// A proposed adjustment to an algorithm parameter.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ParameterAdjustment {
    /// Parameter name (e.g., "satisfaction_threshold", "max_iterations").
    pub parameter: String,

    /// Current value (as string for flexibility).
    pub current_value: String,

    /// Proposed new value.
    pub proposed_value: String,

    /// Why this change is recommended.
    pub reason: String,
}
