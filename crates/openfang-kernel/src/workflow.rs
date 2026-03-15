//! Workflow engine — multi-step agent pipeline execution.
//!
//! A workflow defines a sequence of steps where each step routes
//! a task to a specific agent. Steps can:
//! - Pass their output as input to the next step
//! - Run in sequence (pipeline) or in parallel (fan-out)
//! - Conditionally skip based on previous output
//! - Let review steps reject and return to planning
//! - Loop until a condition is met
//! - Store outputs in named variables for later reference
//!
//! Workflows are defined as Rust structs or loaded from JSON.

use chrono::{DateTime, Utc};
use openfang_types::agent::AgentId;
use openfang_types::approval::RiskLevel;
use openfang_types::task_state::{DurableTaskState, TaskExecutionState};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Unique identifier for a workflow definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkflowId(pub Uuid);

impl WorkflowId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for WorkflowId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for WorkflowId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a running workflow instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkflowRunId(pub Uuid);

impl WorkflowRunId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for WorkflowRunId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for WorkflowRunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A workflow definition — a named sequence of steps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    /// Unique identifier.
    pub id: WorkflowId,
    /// Human-readable name.
    pub name: String,
    /// Description of what this workflow does.
    pub description: String,
    /// The steps in execution order.
    pub steps: Vec<WorkflowStep>,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRouteRequest {
    pub user_id: String,
    pub channel: String,
    pub task_type: String,
    pub risk_level: RiskLevel,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowRiskPolicy {
    #[default]
    Any,
    Max(RiskLevel),
    AllowList(Vec<RiskLevel>),
}

impl WorkflowRiskPolicy {
    fn allows(&self, risk_level: RiskLevel) -> bool {
        match self {
            WorkflowRiskPolicy::Any => true,
            WorkflowRiskPolicy::Max(max_risk) => risk_rank(risk_level) <= risk_rank(*max_risk),
            WorkflowRiskPolicy::AllowList(levels) => levels.contains(&risk_level),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRouteRule {
    pub workflow_id: WorkflowId,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub task_type: Option<String>,
    #[serde(default)]
    pub risk_policy: WorkflowRiskPolicy,
    #[serde(default)]
    pub priority: i32,
}

impl WorkflowRouteRule {
    fn matches(&self, request: &WorkflowRouteRequest) -> bool {
        matches_field(&self.user_id, &request.user_id)
            && matches_field(&self.channel, &request.channel)
            && matches_field(&self.task_type, &request.task_type)
            && self.risk_policy.allows(request.risk_level)
    }

    fn score(&self) -> i32 {
        let mut score = self.priority.saturating_mul(100);
        if self.user_id.is_some() {
            score = score.saturating_add(8);
        }
        if self.channel.is_some() {
            score = score.saturating_add(4);
        }
        if self.task_type.is_some() {
            score = score.saturating_add(2);
        }
        if self.risk_policy != WorkflowRiskPolicy::Any {
            score = score.saturating_add(1);
        }
        score
    }
}

fn matches_field(expected: &Option<String>, actual: &str) -> bool {
    expected
        .as_ref()
        .map(|value| value.eq_ignore_ascii_case(actual))
        .unwrap_or(true)
}

fn risk_rank(level: RiskLevel) -> u8 {
    match level {
        RiskLevel::Low => 0,
        RiskLevel::Medium => 1,
        RiskLevel::High => 2,
        RiskLevel::Critical => 3,
    }
}

/// A single step in a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    /// Step name for logging/display.
    pub name: String,
    /// Which agent to route this step to.
    pub agent: StepAgent,
    /// The prompt template. Use `{{input}}` for previous output, `{{var_name}}` for variables.
    pub prompt_template: String,
    /// Execution mode for this step.
    pub mode: StepMode,
    /// Maximum time for this step in seconds (default: 120).
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Error handling mode for this step (default: Fail).
    #[serde(default)]
    pub error_mode: ErrorMode,
    /// Optional variable name to store this step's output in.
    #[serde(default)]
    pub output_var: Option<String>,
}

fn default_timeout() -> u64 {
    120
}

fn default_max_rejects() -> u32 {
    3
}

fn default_task_state() -> DurableTaskState {
    DurableTaskState::new(Utc::now())
}

fn default_trace_id() -> String {
    Uuid::new_v4().to_string()
}

fn default_rollback_window_secs() -> u64 {
    300
}

/// How to identify the agent for a step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StepAgent {
    /// Reference an agent by UUID.
    ById { id: String },
    /// Reference an agent by name (first match).
    ByName { name: String },
}

/// Execution mode for a workflow step.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepMode {
    /// Execute sequentially — this step runs after the previous completes.
    #[default]
    Sequential,
    /// Fan-out — this step runs in parallel with subsequent FanOut steps until Collect.
    FanOut,
    /// Collect results from all preceding fan-out steps.
    Collect,
    /// Review output and optionally reject by returning to an upstream step.
    Review {
        /// If review output contains this substring (case-insensitive), reject.
        reject_if_contains: String,
        /// Step name to return to when rejected.
        return_to_step: String,
        /// Maximum number of reject-and-return cycles allowed.
        #[serde(default = "default_max_rejects")]
        max_rejects: u32,
    },
    /// Conditional — skip this step if previous output doesn't contain `condition` (case-insensitive).
    Conditional { condition: String },
    /// Loop — repeat this step until output contains `until` or `max_iterations` reached.
    Loop { max_iterations: u32, until: String },
}

/// Error handling mode for a workflow step.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorMode {
    /// Abort the workflow on error (default).
    #[default]
    Fail,
    /// Skip this step on error and continue.
    Skip,
    /// Retry the step up to N times before failing.
    Retry { max_retries: u32 },
}

/// The current state of a workflow run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowRunState {
    Pending,
    Running,
    Completed,
    Failed,
    Blocked,
}

/// Audit event category emitted during workflow orchestration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowAuditEventType {
    Decision,
    Dispatch,
    Execution,
    Review,
}

/// Audit event correlated by workflow trace ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowAuditEvent {
    /// Unique event identifier.
    pub event_id: Uuid,
    /// Correlation ID shared by all events in a run.
    pub trace_id: String,
    /// Owning workflow run.
    pub run_id: WorkflowRunId,
    /// Workflow definition that produced this event.
    pub workflow_id: WorkflowId,
    /// Optional step name associated with this event.
    pub step_name: Option<String>,
    /// Event category.
    pub event_type: WorkflowAuditEventType,
    /// Human-readable event detail.
    pub detail: String,
    /// Event outcome summary.
    pub outcome: String,
    /// Event timestamp.
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowRunRecoveryState {
    pub next_step_index: usize,
    pub current_input: String,
    #[serde(default)]
    pub all_outputs: Vec<String>,
    #[serde(default)]
    pub pending_fan_out_outputs: Vec<String>,
    #[serde(default)]
    pub review_reject_counts: HashMap<usize, u32>,
    #[serde(default)]
    pub variables: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowStepQualityGate {
    pub acceptance_criteria: String,
    pub validation_command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowStepQualityGateConfig {
    pub workflow_id: WorkflowId,
    pub step_name: String,
    pub gate: WorkflowStepQualityGate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStepQualityGateLog {
    pub step_name: String,
    pub acceptance_criteria: String,
    pub validation_command: String,
    pub exit_code: Option<i32>,
    pub output: String,
    pub attempted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowShadowComparison {
    pub production_output: String,
    pub shadow_output: String,
    pub matches: bool,
    pub normalized_matches: bool,
    pub first_mismatch_index: Option<usize>,
    pub compared_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowTrafficPath {
    Production,
    Openfang,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowRollbackChecklistItem {
    pub step: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRollbackRecord {
    pub from_path: WorkflowTrafficPath,
    pub to_path: WorkflowTrafficPath,
    pub shadow_enabled_before: bool,
    pub shadow_enabled_after: bool,
    pub checklist: Vec<WorkflowRollbackChecklistItem>,
    pub initiated_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub within_window: bool,
    pub rollback_window_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRolloutState {
    pub workflow_id: WorkflowId,
    pub primary_path: WorkflowTrafficPath,
    pub stable_path: WorkflowTrafficPath,
    pub shadow_enabled: bool,
    #[serde(default = "default_rollback_window_secs")]
    pub rollback_window_secs: u64,
    #[serde(default = "WorkflowEngine::default_rollback_checklist")]
    pub rollback_checklist: Vec<WorkflowRollbackChecklistItem>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_rollback: Option<WorkflowRollbackRecord>,
}

impl WorkflowRunRecoveryState {
    fn new(input: String) -> Self {
        Self {
            next_step_index: 0,
            current_input: input,
            all_outputs: Vec::new(),
            pending_fan_out_outputs: Vec::new(),
            review_reject_counts: HashMap::new(),
            variables: HashMap::new(),
        }
    }
}

/// A running workflow instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRun {
    /// Run instance ID.
    pub id: WorkflowRunId,
    /// The workflow being run.
    pub workflow_id: WorkflowId,
    /// Workflow name (copied for quick access).
    pub workflow_name: String,
    /// Initial input to the workflow.
    pub input: String,
    /// Correlation ID used to query all run events.
    #[serde(default = "default_trace_id")]
    pub trace_id: String,
    /// Current state.
    pub state: WorkflowRunState,
    /// Durable canonical task state for long-running orchestration and recovery.
    #[serde(default = "default_task_state")]
    pub task_state: DurableTaskState,
    /// Results from each completed step.
    pub step_results: Vec<StepResult>,
    /// Final output (set when workflow completes).
    pub output: Option<String>,
    /// Error message if failed.
    pub error: Option<String>,
    /// Recovery state used to safely continue interrupted runs.
    #[serde(default)]
    pub recovery: WorkflowRunRecoveryState,
    /// Step-level audit trail for this run.
    #[serde(default)]
    pub audit_events: Vec<WorkflowAuditEvent>,
    /// Structured quality-gate execution logs for this run.
    #[serde(default)]
    pub quality_gate_logs: Vec<WorkflowStepQualityGateLog>,
    /// Optional production-vs-shadow comparison captured for shadow runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<WorkflowShadowComparison>,
    /// Started at.
    pub started_at: DateTime<Utc>,
    /// Completed at.
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRecoverySnapshot {
    pub version: u32,
    pub workflows: Vec<Workflow>,
    pub route_rules: Vec<WorkflowRouteRule>,
    #[serde(default)]
    pub gate_enforced_workflows: Vec<WorkflowId>,
    #[serde(default)]
    pub step_quality_gates: Vec<WorkflowStepQualityGateConfig>,
    #[serde(default)]
    pub rollout_states: Vec<WorkflowRolloutState>,
    pub runs: Vec<WorkflowRun>,
}

/// Aggregated workflow observability metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowObservabilityMetrics {
    /// Total number of tracked runs.
    pub runs_total: usize,
    /// Total terminal runs (completed/failed/blocked).
    pub terminal_runs_total: usize,
    /// Fraction of terminal runs that completed successfully.
    pub success_rate: f64,
    /// Fraction of terminal runs that failed or blocked.
    pub failure_rate: f64,
    /// Fraction of execution events that involved retry semantics.
    pub retry_rate: f64,
    /// Fraction of review events that were rejected.
    pub reject_rate: f64,
    /// Average resume delay in milliseconds from failed/blocked back to in-progress.
    pub resume_time_ms: f64,
}

/// Result from a single workflow step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// Step name.
    pub step_name: String,
    /// Agent that executed this step.
    pub agent_id: String,
    /// Agent name.
    pub agent_name: String,
    /// Output from this step.
    pub output: String,
    /// Token usage.
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

/// The workflow engine — manages definitions and executes pipeline runs.
pub struct WorkflowEngine {
    /// Registered workflow definitions.
    workflows: Arc<RwLock<HashMap<WorkflowId, Workflow>>>,
    /// Active and completed workflow runs.
    runs: Arc<RwLock<HashMap<WorkflowRunId, WorkflowRun>>>,
    route_rules: Arc<RwLock<Vec<WorkflowRouteRule>>>,
    gate_enforced_workflows: Arc<RwLock<HashSet<WorkflowId>>>,
    step_quality_gates: Arc<RwLock<HashMap<(WorkflowId, String), WorkflowStepQualityGate>>>,
    rollout_states: Arc<RwLock<HashMap<WorkflowId, WorkflowRolloutState>>>,
}

impl WorkflowEngine {
    /// Create a new workflow engine.
    pub fn new() -> Self {
        Self {
            workflows: Arc::new(RwLock::new(HashMap::new())),
            runs: Arc::new(RwLock::new(HashMap::new())),
            route_rules: Arc::new(RwLock::new(Vec::new())),
            gate_enforced_workflows: Arc::new(RwLock::new(HashSet::new())),
            step_quality_gates: Arc::new(RwLock::new(HashMap::new())),
            rollout_states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn default_rollback_checklist() -> Vec<WorkflowRollbackChecklistItem> {
        vec![
            WorkflowRollbackChecklistItem {
                step: "freeze_candidate".to_string(),
                description: "Freeze the candidate path and stop new promotion changes."
                    .to_string(),
            },
            WorkflowRollbackChecklistItem {
                step: "switch_primary".to_string(),
                description: "Switch the primary path back to the last stable production route."
                    .to_string(),
            },
            WorkflowRollbackChecklistItem {
                step: "disable_shadow".to_string(),
                description: "Disable shadow traffic until the incident is understood.".to_string(),
            },
            WorkflowRollbackChecklistItem {
                step: "verify_sli".to_string(),
                description:
                    "Verify success rate, failure rate, and recent trace anomalies after rollback."
                        .to_string(),
            },
            WorkflowRollbackChecklistItem {
                step: "capture_incident".to_string(),
                description:
                    "Capture the rollback timestamp, operator, and follow-up incident notes."
                        .to_string(),
            },
        ]
    }

    fn default_rollout_state(workflow_id: WorkflowId) -> WorkflowRolloutState {
        WorkflowRolloutState {
            workflow_id,
            primary_path: WorkflowTrafficPath::Production,
            stable_path: WorkflowTrafficPath::Production,
            shadow_enabled: false,
            rollback_window_secs: default_rollback_window_secs(),
            rollback_checklist: Self::default_rollback_checklist(),
            updated_at: Utc::now(),
            last_rollback: None,
        }
    }

    /// Register a new workflow definition.
    pub async fn register(&self, workflow: Workflow) -> WorkflowId {
        let id = workflow.id;
        self.workflows.write().await.insert(id, workflow);
        self.rollout_states
            .write()
            .await
            .entry(id)
            .or_insert_with(|| Self::default_rollout_state(id));
        info!(workflow_id = %id, "Workflow registered");
        id
    }

    /// List all registered workflows.
    pub async fn list_workflows(&self) -> Vec<Workflow> {
        self.workflows.read().await.values().cloned().collect()
    }

    /// Get a specific workflow by ID.
    pub async fn get_workflow(&self, id: WorkflowId) -> Option<Workflow> {
        self.workflows.read().await.get(&id).cloned()
    }

    /// Update an existing workflow definition while preserving immutable metadata.
    pub async fn update_workflow(&self, id: WorkflowId, mut workflow: Workflow) -> bool {
        let mut workflows = self.workflows.write().await;
        let Some(existing) = workflows.get(&id) else {
            return false;
        };
        workflow.id = id;
        workflow.created_at = existing.created_at;
        workflows.insert(id, workflow);
        true
    }

    /// Remove a workflow definition.
    pub async fn remove_workflow(&self, id: WorkflowId) -> bool {
        self.rollout_states.write().await.remove(&id);
        self.workflows.write().await.remove(&id).is_some()
    }

    pub async fn set_route_rules(&self, rules: Vec<WorkflowRouteRule>) {
        *self.route_rules.write().await = rules;
    }

    pub async fn add_route_rule(&self, rule: WorkflowRouteRule) {
        self.route_rules.write().await.push(rule);
    }

    pub async fn clear_route_rules(&self) {
        self.route_rules.write().await.clear();
    }

    pub async fn list_route_rules(&self) -> Vec<WorkflowRouteRule> {
        self.route_rules.read().await.clone()
    }

    pub async fn route_workflow(&self, request: &WorkflowRouteRequest) -> Option<WorkflowId> {
        let rules = self.route_rules.read().await;
        let workflows = self.workflows.read().await;

        let mut selected: Option<(WorkflowId, i32)> = None;
        for rule in rules.iter() {
            if !workflows.contains_key(&rule.workflow_id) || !rule.matches(request) {
                continue;
            }
            let score = rule.score();
            if selected
                .as_ref()
                .map(|(_, best_score)| score > *best_score)
                .unwrap_or(true)
            {
                selected = Some((rule.workflow_id, score));
            }
        }

        selected.map(|(workflow_id, _)| workflow_id)
    }

    pub async fn route_workflow_for_primary_path(
        &self,
        request: &WorkflowRouteRequest,
        required_path: WorkflowTrafficPath,
    ) -> Option<WorkflowId> {
        let workflow_id = self.route_workflow(request).await?;
        let rollout = self.get_rollout_state(workflow_id).await?;
        if rollout.primary_path == required_path {
            Some(workflow_id)
        } else {
            None
        }
    }

    fn resolve_step_name(workflow: &Workflow, step_name: &str) -> Option<String> {
        workflow
            .steps
            .iter()
            .find(|step| step.name.eq_ignore_ascii_case(step_name))
            .map(|step| step.name.clone())
    }

    pub async fn get_rollout_state(&self, workflow_id: WorkflowId) -> Option<WorkflowRolloutState> {
        if !self.workflows.read().await.contains_key(&workflow_id) {
            return None;
        }

        if let Some(state) = self.rollout_states.read().await.get(&workflow_id).cloned() {
            return Some(state);
        }

        let default = Self::default_rollout_state(workflow_id);
        self.rollout_states
            .write()
            .await
            .insert(workflow_id, default.clone());
        Some(default)
    }

    pub async fn update_rollout_state(
        &self,
        workflow_id: WorkflowId,
        primary_path: Option<WorkflowTrafficPath>,
        stable_path: Option<WorkflowTrafficPath>,
        shadow_enabled: Option<bool>,
        rollback_window_secs: Option<u64>,
    ) -> Result<WorkflowRolloutState, String> {
        if !self.workflows.read().await.contains_key(&workflow_id) {
            return Err(format!("Workflow '{}' not found", workflow_id));
        }
        if let Some(window) = rollback_window_secs {
            if window == 0 {
                return Err("rollback_window_secs must be greater than zero".to_string());
            }
        }

        let mut rollout_states = self.rollout_states.write().await;
        let state = rollout_states
            .entry(workflow_id)
            .or_insert_with(|| Self::default_rollout_state(workflow_id));

        if let Some(primary_path) = primary_path {
            state.primary_path = primary_path;
        }
        if let Some(stable_path) = stable_path {
            state.stable_path = stable_path;
        }
        if let Some(shadow_enabled) = shadow_enabled {
            state.shadow_enabled = shadow_enabled;
        }
        if let Some(rollback_window_secs) = rollback_window_secs {
            state.rollback_window_secs = rollback_window_secs;
        }
        state.updated_at = Utc::now();
        Ok(state.clone())
    }

    pub async fn rollback_to_stable_path(
        &self,
        workflow_id: WorkflowId,
    ) -> Result<WorkflowRolloutState, String> {
        if !self.workflows.read().await.contains_key(&workflow_id) {
            return Err(format!("Workflow '{}' not found", workflow_id));
        }

        let initiated_at = Utc::now();
        let started = std::time::Instant::now();
        let mut rollout_states = self.rollout_states.write().await;
        let state = rollout_states
            .entry(workflow_id)
            .or_insert_with(|| Self::default_rollout_state(workflow_id));
        let from_path = state.primary_path;
        let to_path = state.stable_path;
        let shadow_enabled_before = state.shadow_enabled;

        state.primary_path = to_path;
        state.shadow_enabled = false;
        let completed_at = Utc::now();
        let duration_ms = started.elapsed().as_millis() as u64;
        let record = WorkflowRollbackRecord {
            from_path,
            to_path,
            shadow_enabled_before,
            shadow_enabled_after: state.shadow_enabled,
            checklist: state.rollback_checklist.clone(),
            initiated_at,
            completed_at,
            duration_ms,
            within_window: duration_ms <= state.rollback_window_secs.saturating_mul(1000),
            rollback_window_secs: state.rollback_window_secs,
        };
        state.updated_at = completed_at;
        state.last_rollback = Some(record);
        Ok(state.clone())
    }

    pub async fn enable_step_quality_gates(&self, workflow_id: WorkflowId) -> Result<(), String> {
        if !self.workflows.read().await.contains_key(&workflow_id) {
            return Err(format!("Workflow '{}' not found", workflow_id));
        }

        self.gate_enforced_workflows
            .write()
            .await
            .insert(workflow_id);
        Ok(())
    }

    pub async fn disable_step_quality_gates(&self, workflow_id: WorkflowId) -> bool {
        self.gate_enforced_workflows
            .write()
            .await
            .remove(&workflow_id)
    }

    pub async fn step_quality_gates_enabled(&self, workflow_id: WorkflowId) -> bool {
        self.gate_enforced_workflows
            .read()
            .await
            .contains(&workflow_id)
    }

    pub async fn set_step_quality_gate(
        &self,
        workflow_id: WorkflowId,
        step_name: impl Into<String>,
        gate: WorkflowStepQualityGate,
    ) -> Result<(), String> {
        if gate.acceptance_criteria.trim().is_empty() || gate.validation_command.trim().is_empty() {
            return Err(
                "quality gate requires non-empty acceptance criteria and validation command"
                    .to_string(),
            );
        }

        let requested_step_name = step_name.into();
        let canonical_step_name = {
            let workflows = self.workflows.read().await;
            let workflow = workflows
                .get(&workflow_id)
                .ok_or_else(|| format!("Workflow '{}' not found", workflow_id))?;
            Self::resolve_step_name(workflow, &requested_step_name).ok_or_else(|| {
                format!(
                    "Workflow '{}' does not contain step '{}'",
                    workflow_id, requested_step_name
                )
            })?
        };

        self.step_quality_gates
            .write()
            .await
            .insert((workflow_id, canonical_step_name), gate);
        Ok(())
    }

    pub async fn get_step_quality_gate(
        &self,
        workflow_id: WorkflowId,
        step_name: &str,
    ) -> Option<WorkflowStepQualityGate> {
        let canonical_step_name = {
            let workflows = self.workflows.read().await;
            let workflow = workflows.get(&workflow_id)?;
            Self::resolve_step_name(workflow, step_name)?
        };

        self.step_quality_gates
            .read()
            .await
            .get(&(workflow_id, canonical_step_name))
            .cloned()
    }

    /// Maximum number of retained workflow runs. Oldest completed/failed
    /// runs are evicted when this limit is exceeded.
    const MAX_RETAINED_RUNS: usize = 200;
    const RECOVERY_SNAPSHOT_VERSION: u32 = 1;

    /// Start a workflow run. Returns the run ID and a handle to check progress.
    ///
    /// The actual execution is driven externally by calling `execute_run()`
    /// with the kernel handle, since the workflow engine doesn't own the kernel.
    pub async fn create_run(
        &self,
        workflow_id: WorkflowId,
        input: String,
    ) -> Option<WorkflowRunId> {
        let workflow = self.workflows.read().await.get(&workflow_id)?.clone();
        let run_id = WorkflowRunId::new();
        let started_at = Utc::now();

        let run = WorkflowRun {
            id: run_id,
            workflow_id,
            workflow_name: workflow.name,
            input: input.clone(),
            trace_id: default_trace_id(),
            state: WorkflowRunState::Pending,
            task_state: DurableTaskState::new(started_at),
            step_results: Vec::new(),
            output: None,
            error: None,
            recovery: WorkflowRunRecoveryState::new(input),
            audit_events: Vec::new(),
            quality_gate_logs: Vec::new(),
            shadow: None,
            started_at,
            completed_at: None,
        };

        let mut runs = self.runs.write().await;
        runs.insert(run_id, run);

        // Evict oldest completed/failed runs when we exceed the cap
        if runs.len() > Self::MAX_RETAINED_RUNS {
            let mut evictable: Vec<(WorkflowRunId, DateTime<Utc>)> = runs
                .iter()
                .filter(|(_, r)| {
                    matches!(
                        r.state,
                        WorkflowRunState::Completed
                            | WorkflowRunState::Failed
                            | WorkflowRunState::Blocked
                    )
                })
                .map(|(id, r)| (*id, r.started_at))
                .collect();

            // Sort oldest first
            evictable.sort_by_key(|(_, t)| *t);

            let to_remove = runs.len() - Self::MAX_RETAINED_RUNS;
            for (id, _) in evictable.into_iter().take(to_remove) {
                runs.remove(&id);
                debug!(run_id = %id, "Evicted old workflow run");
            }
        }

        Some(run_id)
    }

    /// Get the current state of a workflow run.
    pub async fn get_run(&self, run_id: WorkflowRunId) -> Option<WorkflowRun> {
        self.runs.read().await.get(&run_id).cloned()
    }

    fn normalize_recovery_state(run: &mut WorkflowRun) {
        if run.recovery.current_input.is_empty() {
            run.recovery.current_input = run
                .output
                .clone()
                .or_else(|| run.step_results.last().map(|step| step.output.clone()))
                .unwrap_or_else(|| run.input.clone());
        }
    }

    fn reconcile_recovered_run(run: &mut WorkflowRun) {
        Self::normalize_recovery_state(run);

        if matches!(run.state, WorkflowRunState::Running)
            || matches!(run.task_state.state, TaskExecutionState::InProgress)
        {
            let now = Utc::now();
            run.state = WorkflowRunState::Blocked;
            run.error =
                Some("Recovered interrupted workflow run; ready to resume safely".to_string());
            Self::transition_task_state(run, TaskExecutionState::Blocked, now);
            run.completed_at = None;
            Self::append_audit_event(
                run,
                WorkflowAuditEventType::Decision,
                None,
                "recovered interrupted workflow run".to_string(),
                "blocked_for_resume".to_string(),
                now,
            );
        }
    }

    fn update_recovery_state(
        run: &mut WorkflowRun,
        next_step_index: usize,
        current_input: &str,
        all_outputs: &[String],
        pending_fan_out_outputs: &[String],
        review_reject_counts: &HashMap<usize, u32>,
        variables: &HashMap<String, String>,
    ) {
        run.recovery.next_step_index = next_step_index;
        run.recovery.current_input = current_input.to_string();
        run.recovery.all_outputs = all_outputs.to_vec();
        run.recovery.pending_fan_out_outputs = pending_fan_out_outputs.to_vec();
        run.recovery.review_reject_counts = review_reject_counts.clone();
        run.recovery.variables = variables.clone();
    }

    // Keep call sites explicit while centralizing the mutation logic.
    #[allow(clippy::too_many_arguments)]
    async fn persist_recovery_state(
        &self,
        run_id: WorkflowRunId,
        next_step_index: usize,
        current_input: &str,
        all_outputs: &[String],
        pending_fan_out_outputs: &[String],
        review_reject_counts: &HashMap<usize, u32>,
        variables: &HashMap<String, String>,
    ) {
        if let Some(run) = self.runs.write().await.get_mut(&run_id) {
            Self::update_recovery_state(
                run,
                next_step_index,
                current_input,
                all_outputs,
                pending_fan_out_outputs,
                review_reject_counts,
                variables,
            );
        }
    }

    pub async fn recovery_snapshot(&self) -> WorkflowRecoverySnapshot {
        let mut gate_enforced_workflows: Vec<WorkflowId> = self
            .gate_enforced_workflows
            .read()
            .await
            .iter()
            .copied()
            .collect();
        gate_enforced_workflows.sort_by_key(|workflow_id| workflow_id.to_string());

        let mut step_quality_gates: Vec<WorkflowStepQualityGateConfig> = self
            .step_quality_gates
            .read()
            .await
            .iter()
            .map(
                |((workflow_id, step_name), gate)| WorkflowStepQualityGateConfig {
                    workflow_id: *workflow_id,
                    step_name: step_name.clone(),
                    gate: gate.clone(),
                },
            )
            .collect();
        step_quality_gates.sort_by(|left, right| {
            left.workflow_id
                .to_string()
                .cmp(&right.workflow_id.to_string())
                .then_with(|| left.step_name.cmp(&right.step_name))
        });

        let mut rollout_states: Vec<WorkflowRolloutState> =
            self.rollout_states.read().await.values().cloned().collect();
        rollout_states.sort_by_key(|state| state.workflow_id.to_string());

        WorkflowRecoverySnapshot {
            version: Self::RECOVERY_SNAPSHOT_VERSION,
            workflows: self.list_workflows().await,
            route_rules: self.list_route_rules().await,
            gate_enforced_workflows,
            step_quality_gates,
            rollout_states,
            runs: self.list_runs(None).await,
        }
    }

    pub async fn save_recovery_snapshot<P: AsRef<Path>>(&self, path: P) -> Result<(), String> {
        let snapshot = self.recovery_snapshot().await;
        let bytes = serde_json::to_vec_pretty(&snapshot)
            .map_err(|error| format!("failed to serialize recovery snapshot: {error}"))?;
        std::fs::write(path.as_ref(), bytes)
            .map_err(|error| format!("failed to write recovery snapshot: {error}"))
    }

    pub async fn load_recovery_snapshot<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let bytes = std::fs::read(path.as_ref())
            .map_err(|error| format!("failed to read recovery snapshot: {error}"))?;
        let snapshot: WorkflowRecoverySnapshot = serde_json::from_slice(&bytes)
            .map_err(|error| format!("failed to parse recovery snapshot: {error}"))?;

        if snapshot.version != Self::RECOVERY_SNAPSHOT_VERSION {
            return Err(format!(
                "unsupported recovery snapshot version: {}",
                snapshot.version
            ));
        }

        let WorkflowRecoverySnapshot {
            workflows,
            route_rules,
            gate_enforced_workflows,
            step_quality_gates,
            rollout_states,
            runs,
            ..
        } = snapshot;

        let engine = Self::new();
        *engine.workflows.write().await = workflows
            .into_iter()
            .map(|workflow| (workflow.id, workflow))
            .collect();
        *engine.route_rules.write().await = route_rules;
        *engine.gate_enforced_workflows.write().await =
            gate_enforced_workflows.into_iter().collect();
        *engine.step_quality_gates.write().await = step_quality_gates
            .into_iter()
            .map(|config| ((config.workflow_id, config.step_name), config.gate))
            .collect();
        *engine.rollout_states.write().await = rollout_states
            .into_iter()
            .map(|state| (state.workflow_id, state))
            .collect();
        {
            let workflow_ids: Vec<WorkflowId> =
                engine.workflows.read().await.keys().copied().collect();
            let mut rollout_states = engine.rollout_states.write().await;
            for workflow_id in workflow_ids {
                rollout_states
                    .entry(workflow_id)
                    .or_insert_with(|| Self::default_rollout_state(workflow_id));
            }
        }

        let mut runs: HashMap<WorkflowRunId, WorkflowRun> = runs
            .into_iter()
            .map(|mut run| {
                Self::reconcile_recovered_run(&mut run);
                (run.id, run)
            })
            .collect();
        *engine.runs.write().await = std::mem::take(&mut runs);

        Ok(engine)
    }

    /// Get a workflow run by trace ID.
    pub async fn get_run_by_trace_id(&self, trace_id: &str) -> Option<WorkflowRun> {
        self.runs
            .read()
            .await
            .values()
            .find(|run| run.trace_id == trace_id)
            .cloned()
    }

    /// List audit events for a specific trace ID in timestamp order.
    pub async fn list_audit_events_by_trace_id(&self, trace_id: &str) -> Vec<WorkflowAuditEvent> {
        let mut events: Vec<WorkflowAuditEvent> = self
            .runs
            .read()
            .await
            .values()
            .filter(|run| run.trace_id == trace_id)
            .flat_map(|run| run.audit_events.clone().into_iter())
            .collect();

        events.sort_by_key(|event| event.timestamp);
        events
    }

    fn compute_rate(numerator: usize, denominator: usize) -> f64 {
        if denominator == 0 {
            0.0
        } else {
            numerator as f64 / denominator as f64
        }
    }

    fn maybe_resume_duration_ms(
        timestamps: &openfang_types::task_state::TaskStateTimestamps,
    ) -> Option<f64> {
        let resumed_at = timestamps.in_progress_at?;

        let resume_origin = [timestamps.blocked_at, timestamps.failed_at]
            .into_iter()
            .flatten()
            .filter(|at| *at < resumed_at)
            .max()?;

        Some((resumed_at - resume_origin).num_milliseconds() as f64)
    }

    /// Aggregate workflow observability metrics for dashboard/API consumption.
    pub async fn observability_metrics(&self) -> WorkflowObservabilityMetrics {
        let runs = self.runs.read().await;
        let runs_total = runs.len();

        let mut terminal_runs_total = 0usize;
        let mut success_runs = 0usize;
        let mut failure_runs = 0usize;

        let mut execution_events_total = 0usize;
        let mut retry_events_total = 0usize;
        let mut review_events_total = 0usize;
        let mut rejected_events_total = 0usize;

        let mut resume_samples_ms: Vec<f64> = Vec::new();

        for run in runs.values() {
            match run.state {
                WorkflowRunState::Completed => {
                    terminal_runs_total += 1;
                    success_runs += 1;
                }
                WorkflowRunState::Failed | WorkflowRunState::Blocked => {
                    terminal_runs_total += 1;
                    failure_runs += 1;
                }
                WorkflowRunState::Pending | WorkflowRunState::Running => {}
            }

            if let Some(duration_ms) = Self::maybe_resume_duration_ms(&run.task_state.timestamps) {
                resume_samples_ms.push(duration_ms);
            }

            for event in &run.audit_events {
                match event.event_type {
                    WorkflowAuditEventType::Execution => {
                        execution_events_total += 1;
                        let detail = event.detail.to_lowercase();
                        if detail.contains("retry") || detail.contains("retries") {
                            retry_events_total += 1;
                        }
                    }
                    WorkflowAuditEventType::Review => {
                        review_events_total += 1;
                        if event.outcome.eq_ignore_ascii_case("rejected") {
                            rejected_events_total += 1;
                        }
                    }
                    WorkflowAuditEventType::Decision | WorkflowAuditEventType::Dispatch => {}
                }
            }
        }

        let resume_time_ms = if resume_samples_ms.is_empty() {
            0.0
        } else {
            resume_samples_ms.iter().sum::<f64>() / resume_samples_ms.len() as f64
        };

        WorkflowObservabilityMetrics {
            runs_total,
            terminal_runs_total,
            success_rate: Self::compute_rate(success_runs, terminal_runs_total),
            failure_rate: Self::compute_rate(failure_runs, terminal_runs_total),
            retry_rate: Self::compute_rate(retry_events_total, execution_events_total),
            reject_rate: Self::compute_rate(rejected_events_total, review_events_total),
            resume_time_ms,
        }
    }

    /// List all workflow runs (optionally filtered by state).
    pub async fn list_runs(&self, state_filter: Option<&str>) -> Vec<WorkflowRun> {
        self.runs
            .read()
            .await
            .values()
            .filter(|r| {
                state_filter
                    .map(|f| match f {
                        "pending" => matches!(r.state, WorkflowRunState::Pending),
                        "running" | "in_progress" => {
                            matches!(r.task_state.state, TaskExecutionState::InProgress)
                        }
                        "completed" | "done" => {
                            matches!(r.task_state.state, TaskExecutionState::Done)
                        }
                        "failed" => matches!(r.task_state.state, TaskExecutionState::Failed),
                        "blocked" => matches!(r.task_state.state, TaskExecutionState::Blocked),
                        "canceled" => matches!(r.task_state.state, TaskExecutionState::Canceled),
                        _ => true,
                    })
                    .unwrap_or(true)
            })
            .cloned()
            .collect()
    }

    fn transition_task_state(run: &mut WorkflowRun, next: TaskExecutionState, at: DateTime<Utc>) {
        if let Err(error) = run.task_state.transition(next, at) {
            warn!(
                from = ?run.task_state.state,
                to = ?next,
                %error,
                "Task state transition rejected"
            );
        }
    }

    fn append_audit_event(
        run: &mut WorkflowRun,
        event_type: WorkflowAuditEventType,
        step_name: Option<String>,
        detail: String,
        outcome: String,
        timestamp: DateTime<Utc>,
    ) {
        run.audit_events.push(WorkflowAuditEvent {
            event_id: Uuid::new_v4(),
            trace_id: run.trace_id.clone(),
            run_id: run.id,
            workflow_id: run.workflow_id,
            step_name,
            event_type,
            detail,
            outcome,
            timestamp,
        });
    }

    async fn record_audit_event(
        &self,
        run_id: WorkflowRunId,
        event_type: WorkflowAuditEventType,
        step_name: Option<&str>,
        detail: impl Into<String>,
        outcome: impl Into<String>,
    ) {
        if let Some(run) = self.runs.write().await.get_mut(&run_id) {
            Self::append_audit_event(
                run,
                event_type,
                step_name.map(|name| name.to_string()),
                detail.into(),
                outcome.into(),
                Utc::now(),
            );
        }
    }

    fn terminal_failure_state_for_step(
        step: &WorkflowStep,
    ) -> (WorkflowRunState, TaskExecutionState) {
        if matches!(step.error_mode, ErrorMode::Retry { .. }) {
            (WorkflowRunState::Blocked, TaskExecutionState::Blocked)
        } else {
            (WorkflowRunState::Failed, TaskExecutionState::Failed)
        }
    }

    async fn set_run_terminal_error(
        &self,
        run_id: WorkflowRunId,
        run_state: WorkflowRunState,
        task_state: TaskExecutionState,
        error: String,
    ) {
        if let Some(r) = self.runs.write().await.get_mut(&run_id) {
            r.state = run_state;
            r.error = Some(error);
            let now = Utc::now();
            Self::transition_task_state(r, task_state, now);
            r.completed_at = Some(now);
        }
    }

    fn format_gate_stream(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).trim().to_string()
    }

    fn combine_gate_output(stdout: &str, stderr: &str) -> String {
        match (stdout.is_empty(), stderr.is_empty()) {
            (true, true) => String::new(),
            (false, true) => stdout.to_string(),
            (true, false) => stderr.to_string(),
            (false, false) => format!(
                "stdout:
{}

stderr:
{}",
                stdout, stderr
            ),
        }
    }

    fn normalize_shadow_output(output: &str) -> String {
        output.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn first_mismatch_index(left: &str, right: &str) -> Option<usize> {
        let mut mismatch = None;
        for (idx, (lhs, rhs)) in left.chars().zip(right.chars()).enumerate() {
            if lhs != rhs {
                mismatch = Some(idx);
                break;
            }
        }

        mismatch.or_else(|| {
            let left_len = left.chars().count();
            let right_len = right.chars().count();
            (left_len != right_len).then_some(left_len.min(right_len))
        })
    }

    pub async fn record_shadow_comparison(
        &self,
        run_id: WorkflowRunId,
        production_output: impl Into<String>,
    ) -> Result<WorkflowShadowComparison, String> {
        let production_output = production_output.into();
        let compared_at = Utc::now();
        let mut runs = self.runs.write().await;
        let run = runs
            .get_mut(&run_id)
            .ok_or_else(|| format!("Workflow run '{}' not found", run_id))?;
        let shadow_output = run
            .output
            .clone()
            .or_else(|| run.step_results.last().map(|step| step.output.clone()))
            .unwrap_or_default();
        let comparison = WorkflowShadowComparison {
            matches: production_output == shadow_output,
            normalized_matches: Self::normalize_shadow_output(&production_output)
                == Self::normalize_shadow_output(&shadow_output),
            first_mismatch_index: Self::first_mismatch_index(&production_output, &shadow_output),
            production_output,
            shadow_output,
            compared_at,
        };
        let outcome = if comparison.matches {
            "shadow_match"
        } else if comparison.normalized_matches {
            "shadow_normalized_match"
        } else {
            "shadow_mismatch"
        };
        let detail = format!(
            "shadow compared against production output (normalized_match={}, first_mismatch_index={:?})",
            comparison.normalized_matches, comparison.first_mismatch_index
        );
        run.shadow = Some(comparison.clone());
        Self::append_audit_event(
            run,
            WorkflowAuditEventType::Decision,
            None,
            detail,
            outcome.to_string(),
            compared_at,
        );
        Ok(comparison)
    }

    async fn record_quality_gate_log(
        &self,
        run_id: WorkflowRunId,
        step_name: &str,
        gate: &WorkflowStepQualityGate,
        exit_code: Option<i32>,
        output: String,
    ) {
        if let Some(run) = self.runs.write().await.get_mut(&run_id) {
            run.quality_gate_logs.push(WorkflowStepQualityGateLog {
                step_name: step_name.to_string(),
                acceptance_criteria: gate.acceptance_criteria.clone(),
                validation_command: gate.validation_command.clone(),
                exit_code,
                output,
                attempted_at: Utc::now(),
            });
        }
    }

    async fn maybe_execute_step_quality_gate(
        &self,
        workflow_id: WorkflowId,
        run_id: WorkflowRunId,
        step_name: &str,
        output: &str,
    ) -> Result<(), String> {
        if !self
            .gate_enforced_workflows
            .read()
            .await
            .contains(&workflow_id)
        {
            return Ok(());
        }

        let gate = match self
            .step_quality_gates
            .read()
            .await
            .get(&(workflow_id, step_name.to_string()))
            .cloned()
        {
            Some(gate) => gate,
            None => {
                let error = format!(
                    "Step '{}' requires acceptance criteria and validation command before completion",
                    step_name
                );
                self.record_audit_event(
                    run_id,
                    WorkflowAuditEventType::Execution,
                    Some(step_name),
                    error.clone(),
                    "quality_gate_missing",
                )
                .await;
                return Err(error);
            }
        };

        if gate.acceptance_criteria.trim().is_empty() || gate.validation_command.trim().is_empty() {
            let error = format!(
                "Step '{}' requires non-empty acceptance criteria and validation command",
                step_name
            );
            self.record_audit_event(
                run_id,
                WorkflowAuditEventType::Execution,
                Some(step_name),
                error.clone(),
                "quality_gate_missing",
            )
            .await;
            return Err(error);
        }

        self.record_audit_event(
            run_id,
            WorkflowAuditEventType::Decision,
            Some(step_name),
            format!(
                "running quality gate acceptance_criteria={:?} command={:?}",
                gate.acceptance_criteria, gate.validation_command
            ),
            "quality_gate_started",
        )
        .await;

        let command_output = match Command::new("/bin/sh")
            .arg("-lc")
            .arg(&gate.validation_command)
            .env("OPENFANG_WORKFLOW_ID", workflow_id.to_string())
            .env("OPENFANG_WORKFLOW_RUN_ID", run_id.to_string())
            .env("OPENFANG_STEP_NAME", step_name)
            .env("OPENFANG_STEP_OUTPUT", output)
            .env("OPENFANG_ACCEPTANCE_CRITERIA", &gate.acceptance_criteria)
            .kill_on_drop(true)
            .output()
            .await
        {
            Ok(output) => output,
            Err(error) => {
                let detail = format!(
                    "quality gate acceptance_criteria={:?} command={:?} error={:?}",
                    gate.acceptance_criteria, gate.validation_command, error
                );
                self.record_quality_gate_log(run_id, step_name, &gate, None, error.to_string())
                    .await;
                self.record_audit_event(
                    run_id,
                    WorkflowAuditEventType::Execution,
                    Some(step_name),
                    detail,
                    "quality_gate_failed",
                )
                .await;
                return Err(format!(
                    "Step '{}' quality gate command failed to start: {}",
                    step_name, error
                ));
            }
        };

        let exit_code = command_output
            .status
            .code()
            .map(|code| code.to_string())
            .unwrap_or_else(|| "signal".to_string());
        let stdout = Self::format_gate_stream(&command_output.stdout);
        let stderr = Self::format_gate_stream(&command_output.stderr);
        let combined_output = Self::combine_gate_output(&stdout, &stderr);
        let summary = if stderr.is_empty() {
            stdout.clone()
        } else {
            stderr.clone()
        };
        let gate_detail = format!(
            "quality gate acceptance_criteria={:?} command={:?} exit_code={} stdout={:?} stderr={:?}",
            gate.acceptance_criteria, gate.validation_command, exit_code, stdout, stderr
        );
        self.record_quality_gate_log(
            run_id,
            step_name,
            &gate,
            command_output.status.code(),
            combined_output,
        )
        .await;

        if command_output.status.success() {
            self.record_audit_event(
                run_id,
                WorkflowAuditEventType::Execution,
                Some(step_name),
                gate_detail,
                "quality_gate_passed",
            )
            .await;
            Ok(())
        } else {
            self.record_audit_event(
                run_id,
                WorkflowAuditEventType::Execution,
                Some(step_name),
                gate_detail,
                "quality_gate_failed",
            )
            .await;
            Err(format!(
                "Step '{}' quality gate failed (exit_code={}): {}",
                step_name,
                exit_code,
                if summary.is_empty() {
                    "validation command exited non-zero".to_string()
                } else {
                    summary
                }
            ))
        }
    }

    /// Replace `{{var_name}}` references in a template with stored variable values.
    fn expand_variables(template: &str, input: &str, vars: &HashMap<String, String>) -> String {
        let mut result = template.replace("{{input}}", input);
        for (key, value) in vars {
            result = result.replace(&format!("{{{{{key}}}}}"), value);
        }
        result
    }

    /// Execute a single step with error mode handling. Returns (output, input_tokens, output_tokens).
    async fn execute_step_with_error_mode<F, Fut>(
        step: &WorkflowStep,
        agent_id: AgentId,
        prompt: String,
        send_message: &F,
    ) -> Result<Option<(String, u64, u64)>, String>
    where
        F: Fn(AgentId, String) -> Fut,
        Fut: std::future::Future<Output = Result<(String, u64, u64), String>>,
    {
        let timeout_dur = std::time::Duration::from_secs(step.timeout_secs);

        match &step.error_mode {
            ErrorMode::Fail => {
                let result = tokio::time::timeout(timeout_dur, send_message(agent_id, prompt))
                    .await
                    .map_err(|_| {
                        format!(
                            "Step '{}' timed out after {}s",
                            step.name, step.timeout_secs
                        )
                    })?
                    .map_err(|e| format!("Step '{}' failed: {}", step.name, e))?;
                Ok(Some(result))
            }
            ErrorMode::Skip => {
                match tokio::time::timeout(timeout_dur, send_message(agent_id, prompt)).await {
                    Ok(Ok(result)) => Ok(Some(result)),
                    Ok(Err(e)) => {
                        warn!("Step '{}' failed (skipping): {e}", step.name);
                        Ok(None)
                    }
                    Err(_) => {
                        warn!(
                            "Step '{}' timed out (skipping) after {}s",
                            step.name, step.timeout_secs
                        );
                        Ok(None)
                    }
                }
            }
            ErrorMode::Retry { max_retries } => {
                let mut last_err = String::new();
                for attempt in 0..=*max_retries {
                    match tokio::time::timeout(timeout_dur, send_message(agent_id, prompt.clone()))
                        .await
                    {
                        Ok(Ok(result)) => return Ok(Some(result)),
                        Ok(Err(e)) => {
                            last_err = e.to_string();
                            if attempt < *max_retries {
                                warn!(
                                    "Step '{}' attempt {} failed: {e}, retrying",
                                    step.name,
                                    attempt + 1
                                );
                            }
                        }
                        Err(_) => {
                            last_err = format!("timed out after {}s", step.timeout_secs);
                            if attempt < *max_retries {
                                warn!(
                                    "Step '{}' attempt {} timed out, retrying",
                                    step.name,
                                    attempt + 1
                                );
                            }
                        }
                    }
                }
                Err(format!(
                    "Step '{}' failed after {} retries: {last_err}",
                    step.name, max_retries
                ))
            }
        }
    }

    /// Execute a workflow run step-by-step.
    ///
    /// This method takes a closure that sends messages to agents,
    /// so the workflow engine remains decoupled from the kernel.
    pub async fn execute_run<F, Fut>(
        &self,
        run_id: WorkflowRunId,
        agent_resolver: impl Fn(&StepAgent) -> Option<(AgentId, String)>,
        send_message: F,
    ) -> Result<String, String>
    where
        F: Fn(AgentId, String) -> Fut,
        Fut: std::future::Future<Output = Result<(String, u64, u64), String>>,
    {
        // Get the run and workflow
        let (
            workflow,
            mut current_input,
            mut all_outputs,
            mut pending_fan_out_outputs,
            mut review_reject_counts,
            mut variables,
            mut i,
            resumed,
        ) = {
            let mut runs = self.runs.write().await;
            let run = runs.get_mut(&run_id).ok_or("Workflow run not found")?;
            Self::normalize_recovery_state(run);
            let resumed = run.recovery.next_step_index > 0
                || matches!(
                    run.task_state.state,
                    TaskExecutionState::Failed | TaskExecutionState::Blocked
                );
            run.state = WorkflowRunState::Running;
            Self::transition_task_state(run, TaskExecutionState::InProgress, Utc::now());

            let workflow = self
                .workflows
                .read()
                .await
                .get(&run.workflow_id)
                .ok_or("Workflow definition not found")?
                .clone();

            (
                workflow,
                run.recovery.current_input.clone(),
                run.recovery.all_outputs.clone(),
                run.recovery.pending_fan_out_outputs.clone(),
                run.recovery.review_reject_counts.clone(),
                run.recovery.variables.clone(),
                run.recovery.next_step_index,
                resumed,
            )
        };

        info!(
            run_id = %run_id,
            workflow = %workflow.name,
            steps = workflow.steps.len(),
            "Starting workflow execution"
        );
        self.record_audit_event(
            run_id,
            WorkflowAuditEventType::Decision,
            None,
            format!(
                "workflow '{}' {} with {} step(s)",
                workflow.name,
                if resumed { "resumed" } else { "started" },
                workflow.steps.len()
            ),
            if resumed { "resumed" } else { "in_progress" },
        )
        .await;

        while i < workflow.steps.len() {
            let step = &workflow.steps[i];
            if !matches!(step.mode, StepMode::FanOut | StepMode::Collect) {
                pending_fan_out_outputs.clear();
            }

            debug!(
                step = i + 1,
                name = %step.name,
                "Executing workflow step"
            );

            match &step.mode {
                StepMode::Sequential => {
                    let (agent_id, agent_name) = agent_resolver(&step.agent)
                        .ok_or_else(|| format!("Agent not found for step '{}'", step.name))?;

                    let prompt =
                        Self::expand_variables(&step.prompt_template, &current_input, &variables);
                    self.record_audit_event(
                        run_id,
                        WorkflowAuditEventType::Dispatch,
                        Some(&step.name),
                        format!("dispatch to agent '{}' ({agent_id})", agent_name),
                        "sent",
                    )
                    .await;

                    let start = std::time::Instant::now();
                    let result =
                        Self::execute_step_with_error_mode(step, agent_id, prompt, &send_message)
                            .await;
                    let duration_ms = start.elapsed().as_millis() as u64;

                    match result {
                        Ok(Some((output, input_tokens, output_tokens))) => {
                            if let Err(error) = self
                                .maybe_execute_step_quality_gate(
                                    workflow.id,
                                    run_id,
                                    &step.name,
                                    &output,
                                )
                                .await
                            {
                                self.persist_recovery_state(
                                    run_id,
                                    i,
                                    &current_input,
                                    &all_outputs,
                                    &pending_fan_out_outputs,
                                    &review_reject_counts,
                                    &variables,
                                )
                                .await;
                                let (run_state, task_state) =
                                    Self::terminal_failure_state_for_step(step);
                                self.set_run_terminal_error(
                                    run_id,
                                    run_state,
                                    task_state,
                                    error.clone(),
                                )
                                .await;
                                return Err(error);
                            }

                            let step_result = StepResult {
                                step_name: step.name.clone(),
                                agent_id: agent_id.to_string(),
                                agent_name,
                                output: output.clone(),
                                input_tokens,
                                output_tokens,
                                duration_ms,
                            };
                            if let Some(r) = self.runs.write().await.get_mut(&run_id) {
                                r.step_results.push(step_result);
                            }

                            if let Some(ref var) = step.output_var {
                                variables.insert(var.clone(), output.clone());
                            }

                            all_outputs.push(output.clone());
                            current_input = output;
                            info!(step = i + 1, name = %step.name, duration_ms, "Step completed");
                            self.record_audit_event(
                                run_id,
                                WorkflowAuditEventType::Execution,
                                Some(&step.name),
                                format!("completed in {duration_ms}ms"),
                                "ok",
                            )
                            .await;
                        }
                        Ok(None) => {
                            // Step was skipped (ErrorMode::Skip)
                            info!(step = i + 1, name = %step.name, "Step skipped");
                            self.record_audit_event(
                                run_id,
                                WorkflowAuditEventType::Execution,
                                Some(&step.name),
                                "step skipped by error mode",
                                "skipped",
                            )
                            .await;
                        }
                        Err(e) => {
                            self.record_audit_event(
                                run_id,
                                WorkflowAuditEventType::Execution,
                                Some(&step.name),
                                format!("step execution failed: {e}"),
                                "failed",
                            )
                            .await;
                            self.persist_recovery_state(
                                run_id,
                                i,
                                &current_input,
                                &all_outputs,
                                &pending_fan_out_outputs,
                                &review_reject_counts,
                                &variables,
                            )
                            .await;
                            let (run_state, task_state) =
                                Self::terminal_failure_state_for_step(step);
                            self.set_run_terminal_error(run_id, run_state, task_state, e.clone())
                                .await;
                            return Err(e);
                        }
                    }
                }

                StepMode::FanOut => {
                    // Collect consecutive FanOut steps and run them in parallel
                    let mut fan_out_steps = vec![(i, step)];
                    let mut j = i + 1;
                    while j < workflow.steps.len() {
                        if matches!(workflow.steps[j].mode, StepMode::FanOut) {
                            fan_out_steps.push((j, &workflow.steps[j]));
                            j += 1;
                        } else {
                            break;
                        }
                    }
                    pending_fan_out_outputs.clear();
                    self.record_audit_event(
                        run_id,
                        WorkflowAuditEventType::Decision,
                        Some(&step.name),
                        format!("fan_out group size={}", fan_out_steps.len()),
                        "parallel_dispatch",
                    )
                    .await;

                    // Build all futures
                    let mut futures = Vec::new();
                    let mut step_infos = Vec::new();

                    for (idx, fan_step) in &fan_out_steps {
                        let (agent_id, agent_name) =
                            agent_resolver(&fan_step.agent).ok_or_else(|| {
                                format!("Agent not found for step '{}'", fan_step.name)
                            })?;
                        let prompt = Self::expand_variables(
                            &fan_step.prompt_template,
                            &current_input,
                            &variables,
                        );
                        self.record_audit_event(
                            run_id,
                            WorkflowAuditEventType::Dispatch,
                            Some(&fan_step.name),
                            format!("dispatch to agent '{}' ({agent_id})", agent_name),
                            "sent",
                        )
                        .await;
                        step_infos.push((*idx, fan_step.name.clone(), agent_id, agent_name));
                        futures.push(Self::execute_step_with_error_mode(
                            fan_step,
                            agent_id,
                            prompt,
                            &send_message,
                        ));
                    }

                    let start = std::time::Instant::now();
                    let results = futures::future::join_all(futures).await;
                    let duration_ms = start.elapsed().as_millis() as u64;

                    for (k, result) in results.into_iter().enumerate() {
                        let (_, ref step_name, agent_id, ref agent_name) = step_infos[k];
                        let fan_step = fan_out_steps[k].1;

                        match result {
                            Ok(Some((output, input_tokens, output_tokens))) => {
                                if let Err(error) = self
                                    .maybe_execute_step_quality_gate(
                                        workflow.id,
                                        run_id,
                                        step_name,
                                        &output,
                                    )
                                    .await
                                {
                                    let (run_state, task_state) =
                                        Self::terminal_failure_state_for_step(fan_step);
                                    self.persist_recovery_state(
                                        run_id,
                                        i,
                                        &current_input,
                                        &all_outputs,
                                        &pending_fan_out_outputs,
                                        &review_reject_counts,
                                        &variables,
                                    )
                                    .await;
                                    self.set_run_terminal_error(
                                        run_id,
                                        run_state,
                                        task_state,
                                        error.clone(),
                                    )
                                    .await;
                                    return Err(error);
                                }

                                let step_result = StepResult {
                                    step_name: step_name.clone(),
                                    agent_id: agent_id.to_string(),
                                    agent_name: agent_name.clone(),
                                    output: output.clone(),
                                    input_tokens,
                                    output_tokens,
                                    duration_ms,
                                };
                                if let Some(r) = self.runs.write().await.get_mut(&run_id) {
                                    r.step_results.push(step_result);
                                }
                                if let Some(ref var) = fan_step.output_var {
                                    variables.insert(var.clone(), output.clone());
                                }
                                all_outputs.push(output.clone());
                                pending_fan_out_outputs.push(output.clone());
                                current_input = output;
                                self.record_audit_event(
                                    run_id,
                                    WorkflowAuditEventType::Execution,
                                    Some(step_name),
                                    format!("fan_out step completed in {duration_ms}ms"),
                                    "ok",
                                )
                                .await;
                            }
                            Ok(None) => {
                                info!(
                                    step_name = %step_name,
                                    "FanOut step skipped by error mode"
                                );
                                self.record_audit_event(
                                    run_id,
                                    WorkflowAuditEventType::Execution,
                                    Some(step_name),
                                    "fan_out step skipped by error mode",
                                    "skipped",
                                )
                                .await;
                            }
                            Err(e) => {
                                let error_msg =
                                    format!("FanOut step '{}' failed: {}", step_name, e);
                                warn!(%error_msg);
                                self.record_audit_event(
                                    run_id,
                                    WorkflowAuditEventType::Execution,
                                    Some(step_name),
                                    error_msg.clone(),
                                    "failed",
                                )
                                .await;
                                let (run_state, task_state) =
                                    Self::terminal_failure_state_for_step(fan_step);
                                self.persist_recovery_state(
                                    run_id,
                                    i,
                                    &current_input,
                                    &all_outputs,
                                    &pending_fan_out_outputs,
                                    &review_reject_counts,
                                    &variables,
                                )
                                .await;
                                self.set_run_terminal_error(
                                    run_id,
                                    run_state,
                                    task_state,
                                    error_msg.clone(),
                                )
                                .await;
                                return Err(error_msg);
                            }
                        }
                    }

                    info!(
                        count = fan_out_steps.len(),
                        duration_ms, "FanOut steps completed"
                    );

                    // Skip past the fan-out steps we just processed
                    i = j;
                    self.persist_recovery_state(
                        run_id,
                        i,
                        &current_input,
                        &all_outputs,
                        &pending_fan_out_outputs,
                        &review_reject_counts,
                        &variables,
                    )
                    .await;
                    continue;
                }

                StepMode::Collect => {
                    let collected_outputs = if pending_fan_out_outputs.is_empty() {
                        all_outputs.clone()
                    } else {
                        pending_fan_out_outputs.clone()
                    };
                    let collected_input = if collected_outputs.is_empty() {
                        current_input.clone()
                    } else {
                        collected_outputs.join(
                            "

---

",
                        )
                    };

                    if let Err(error) = self
                        .maybe_execute_step_quality_gate(
                            workflow.id,
                            run_id,
                            &step.name,
                            &collected_input,
                        )
                        .await
                    {
                        self.persist_recovery_state(
                            run_id,
                            i,
                            &current_input,
                            &all_outputs,
                            &pending_fan_out_outputs,
                            &review_reject_counts,
                            &variables,
                        )
                        .await;
                        let (run_state, task_state) = Self::terminal_failure_state_for_step(step);
                        self.set_run_terminal_error(run_id, run_state, task_state, error.clone())
                            .await;
                        return Err(error);
                    }

                    current_input = collected_input;
                    pending_fan_out_outputs.clear();
                    all_outputs.clear();
                    all_outputs.push(current_input.clone());
                    if let Some(ref var) = step.output_var {
                        variables.insert(var.clone(), current_input.clone());
                    }
                    self.record_audit_event(
                        run_id,
                        WorkflowAuditEventType::Decision,
                        Some(&step.name),
                        format!("collect aggregated {} output(s)", collected_outputs.len()),
                        "aggregated",
                    )
                    .await;
                }

                StepMode::Review {
                    reject_if_contains,
                    return_to_step,
                    max_rejects,
                } => {
                    let (agent_id, agent_name) = agent_resolver(&step.agent)
                        .ok_or_else(|| format!("Agent not found for step '{}'", step.name))?;

                    let prompt =
                        Self::expand_variables(&step.prompt_template, &current_input, &variables);
                    self.record_audit_event(
                        run_id,
                        WorkflowAuditEventType::Dispatch,
                        Some(&step.name),
                        format!("dispatch review to agent '{}' ({agent_id})", agent_name),
                        "sent",
                    )
                    .await;

                    let start = std::time::Instant::now();
                    let result =
                        Self::execute_step_with_error_mode(step, agent_id, prompt, &send_message)
                            .await;
                    let duration_ms = start.elapsed().as_millis() as u64;

                    match result {
                        Ok(Some((output, input_tokens, output_tokens))) => {
                            if let Err(error) = self
                                .maybe_execute_step_quality_gate(
                                    workflow.id,
                                    run_id,
                                    &step.name,
                                    &output,
                                )
                                .await
                            {
                                self.persist_recovery_state(
                                    run_id,
                                    i,
                                    &current_input,
                                    &all_outputs,
                                    &pending_fan_out_outputs,
                                    &review_reject_counts,
                                    &variables,
                                )
                                .await;
                                let (run_state, task_state) =
                                    Self::terminal_failure_state_for_step(step);
                                self.set_run_terminal_error(
                                    run_id,
                                    run_state,
                                    task_state,
                                    error.clone(),
                                )
                                .await;
                                return Err(error);
                            }

                            let step_result = StepResult {
                                step_name: step.name.clone(),
                                agent_id: agent_id.to_string(),
                                agent_name,
                                output: output.clone(),
                                input_tokens,
                                output_tokens,
                                duration_ms,
                            };
                            if let Some(r) = self.runs.write().await.get_mut(&run_id) {
                                r.step_results.push(step_result);
                            }

                            if let Some(ref var) = step.output_var {
                                variables.insert(var.clone(), output.clone());
                            }

                            all_outputs.push(output.clone());
                            current_input = output.clone();

                            if output
                                .to_lowercase()
                                .contains(&reject_if_contains.to_lowercase())
                            {
                                let target_idx = workflow
                                    .steps
                                    .iter()
                                    .position(|candidate| {
                                        candidate.name.eq_ignore_ascii_case(return_to_step)
                                    })
                                    .ok_or_else(|| {
                                        format!(
                                            "Review step '{}' return_to_step '{}' not found",
                                            step.name, return_to_step
                                        )
                                    })?;
                                self.record_audit_event(
                                    run_id,
                                    WorkflowAuditEventType::Review,
                                    Some(&step.name),
                                    format!(
                                        "review rejected output containing '{}': {}",
                                        reject_if_contains, output
                                    ),
                                    "rejected",
                                )
                                .await;

                                let reject_count = review_reject_counts.entry(i).or_insert(0);
                                *reject_count += 1;
                                if *reject_count > *max_rejects {
                                    let error_msg = format!(
                                        "Review step '{}' exceeded max_rejects={} (last feedback: {})",
                                        step.name, max_rejects, output
                                    );
                                    self.record_audit_event(
                                        run_id,
                                        WorkflowAuditEventType::Review,
                                        Some(&step.name),
                                        error_msg.clone(),
                                        "failed",
                                    )
                                    .await;
                                    if let Some(r) = self.runs.write().await.get_mut(&run_id) {
                                        r.state = WorkflowRunState::Failed;
                                        r.error = Some(error_msg.clone());
                                        let now = Utc::now();
                                        Self::transition_task_state(
                                            r,
                                            TaskExecutionState::Failed,
                                            now,
                                        );
                                        r.completed_at = Some(now);
                                    }
                                    self.persist_recovery_state(
                                        run_id,
                                        i,
                                        &current_input,
                                        &all_outputs,
                                        &pending_fan_out_outputs,
                                        &review_reject_counts,
                                        &variables,
                                    )
                                    .await;
                                    return Err(error_msg);
                                }

                                info!(
                                    review_step = %step.name,
                                    return_to_step,
                                    reject_count = *reject_count,
                                    max_rejects,
                                    "Review rejected output; returning to upstream step"
                                );
                                self.record_audit_event(
                                    run_id,
                                    WorkflowAuditEventType::Decision,
                                    Some(&step.name),
                                    format!(
                                        "review rejected; returning to step '{}' (reject_count={})",
                                        return_to_step, reject_count
                                    ),
                                    "return_to_step",
                                )
                                .await;
                                i = target_idx;
                                self.persist_recovery_state(
                                    run_id,
                                    i,
                                    &current_input,
                                    &all_outputs,
                                    &pending_fan_out_outputs,
                                    &review_reject_counts,
                                    &variables,
                                )
                                .await;
                                continue;
                            }

                            info!(step = i + 1, name = %step.name, duration_ms, "Review step approved");
                            self.record_audit_event(
                                run_id,
                                WorkflowAuditEventType::Review,
                                Some(&step.name),
                                format!("review approved in {duration_ms}ms"),
                                "approved",
                            )
                            .await;
                        }
                        Ok(None) => {
                            info!(step = i + 1, name = %step.name, "Review step skipped");
                            self.record_audit_event(
                                run_id,
                                WorkflowAuditEventType::Review,
                                Some(&step.name),
                                "review step skipped by error mode",
                                "skipped",
                            )
                            .await;
                        }
                        Err(e) => {
                            self.record_audit_event(
                                run_id,
                                WorkflowAuditEventType::Review,
                                Some(&step.name),
                                format!("review execution failed: {e}"),
                                "failed",
                            )
                            .await;
                            self.persist_recovery_state(
                                run_id,
                                i,
                                &current_input,
                                &all_outputs,
                                &pending_fan_out_outputs,
                                &review_reject_counts,
                                &variables,
                            )
                            .await;
                            let (run_state, task_state) =
                                Self::terminal_failure_state_for_step(step);
                            self.set_run_terminal_error(run_id, run_state, task_state, e.clone())
                                .await;
                            return Err(e);
                        }
                    }
                }

                StepMode::Conditional { condition } => {
                    let prev_lower = current_input.to_lowercase();
                    let cond_lower = condition.to_lowercase();

                    if !prev_lower.contains(&cond_lower) {
                        info!(
                            step = i + 1,
                            name = %step.name,
                            condition,
                            "Conditional step skipped (condition not met)"
                        );
                        self.record_audit_event(
                            run_id,
                            WorkflowAuditEventType::Decision,
                            Some(&step.name),
                            format!("condition '{}' not met", condition),
                            "skipped",
                        )
                        .await;
                        i += 1;
                        self.persist_recovery_state(
                            run_id,
                            i,
                            &current_input,
                            &all_outputs,
                            &pending_fan_out_outputs,
                            &review_reject_counts,
                            &variables,
                        )
                        .await;
                        continue;
                    }
                    self.record_audit_event(
                        run_id,
                        WorkflowAuditEventType::Decision,
                        Some(&step.name),
                        format!("condition '{}' met", condition),
                        "execute",
                    )
                    .await;

                    // Condition met — execute like sequential
                    let (agent_id, agent_name) = agent_resolver(&step.agent)
                        .ok_or_else(|| format!("Agent not found for step '{}'", step.name))?;

                    let prompt =
                        Self::expand_variables(&step.prompt_template, &current_input, &variables);
                    self.record_audit_event(
                        run_id,
                        WorkflowAuditEventType::Dispatch,
                        Some(&step.name),
                        format!("dispatch to agent '{}' ({agent_id})", agent_name),
                        "sent",
                    )
                    .await;

                    let start = std::time::Instant::now();
                    let result =
                        Self::execute_step_with_error_mode(step, agent_id, prompt, &send_message)
                            .await;
                    let duration_ms = start.elapsed().as_millis() as u64;

                    match result {
                        Ok(Some((output, input_tokens, output_tokens))) => {
                            if let Err(error) = self
                                .maybe_execute_step_quality_gate(
                                    workflow.id,
                                    run_id,
                                    &step.name,
                                    &output,
                                )
                                .await
                            {
                                self.persist_recovery_state(
                                    run_id,
                                    i,
                                    &current_input,
                                    &all_outputs,
                                    &pending_fan_out_outputs,
                                    &review_reject_counts,
                                    &variables,
                                )
                                .await;
                                let (run_state, task_state) =
                                    Self::terminal_failure_state_for_step(step);
                                self.set_run_terminal_error(
                                    run_id,
                                    run_state,
                                    task_state,
                                    error.clone(),
                                )
                                .await;
                                return Err(error);
                            }

                            let step_result = StepResult {
                                step_name: step.name.clone(),
                                agent_id: agent_id.to_string(),
                                agent_name,
                                output: output.clone(),
                                input_tokens,
                                output_tokens,
                                duration_ms,
                            };
                            if let Some(r) = self.runs.write().await.get_mut(&run_id) {
                                r.step_results.push(step_result);
                            }
                            if let Some(ref var) = step.output_var {
                                variables.insert(var.clone(), output.clone());
                            }
                            all_outputs.push(output.clone());
                            current_input = output;
                            self.record_audit_event(
                                run_id,
                                WorkflowAuditEventType::Execution,
                                Some(&step.name),
                                format!("conditional step completed in {duration_ms}ms"),
                                "ok",
                            )
                            .await;
                        }
                        Ok(None) => {
                            self.record_audit_event(
                                run_id,
                                WorkflowAuditEventType::Execution,
                                Some(&step.name),
                                "conditional step skipped by error mode",
                                "skipped",
                            )
                            .await;
                        }
                        Err(e) => {
                            self.record_audit_event(
                                run_id,
                                WorkflowAuditEventType::Execution,
                                Some(&step.name),
                                format!("conditional step failed: {e}"),
                                "failed",
                            )
                            .await;
                            self.persist_recovery_state(
                                run_id,
                                i,
                                &current_input,
                                &all_outputs,
                                &pending_fan_out_outputs,
                                &review_reject_counts,
                                &variables,
                            )
                            .await;
                            let (run_state, task_state) =
                                Self::terminal_failure_state_for_step(step);
                            self.set_run_terminal_error(run_id, run_state, task_state, e.clone())
                                .await;
                            return Err(e);
                        }
                    }
                }

                StepMode::Loop {
                    max_iterations,
                    until,
                } => {
                    let (agent_id, agent_name) = agent_resolver(&step.agent)
                        .ok_or_else(|| format!("Agent not found for step '{}'", step.name))?;

                    let until_lower = until.to_lowercase();
                    self.record_audit_event(
                        run_id,
                        WorkflowAuditEventType::Decision,
                        Some(&step.name),
                        format!(
                            "loop start (max_iterations={}, until='{}')",
                            max_iterations, until
                        ),
                        "started",
                    )
                    .await;

                    for loop_iter in 0..*max_iterations {
                        let prompt = Self::expand_variables(
                            &step.prompt_template,
                            &current_input,
                            &variables,
                        );
                        self.record_audit_event(
                            run_id,
                            WorkflowAuditEventType::Dispatch,
                            Some(&step.name),
                            format!(
                                "dispatch loop iteration {} to agent '{}' ({agent_id})",
                                loop_iter + 1,
                                agent_name
                            ),
                            "sent",
                        )
                        .await;

                        let start = std::time::Instant::now();
                        let result = Self::execute_step_with_error_mode(
                            step,
                            agent_id,
                            prompt,
                            &send_message,
                        )
                        .await;
                        let duration_ms = start.elapsed().as_millis() as u64;

                        match result {
                            Ok(Some((output, input_tokens, output_tokens))) => {
                                if let Err(error) = self
                                    .maybe_execute_step_quality_gate(
                                        workflow.id,
                                        run_id,
                                        &step.name,
                                        &output,
                                    )
                                    .await
                                {
                                    let (run_state, task_state) =
                                        Self::terminal_failure_state_for_step(step);
                                    self.persist_recovery_state(
                                        run_id,
                                        i,
                                        &current_input,
                                        &all_outputs,
                                        &pending_fan_out_outputs,
                                        &review_reject_counts,
                                        &variables,
                                    )
                                    .await;
                                    self.set_run_terminal_error(
                                        run_id,
                                        run_state,
                                        task_state,
                                        error.clone(),
                                    )
                                    .await;
                                    return Err(error);
                                }

                                let step_result = StepResult {
                                    step_name: format!("{} (iter {})", step.name, loop_iter + 1),
                                    agent_id: agent_id.to_string(),
                                    agent_name: agent_name.clone(),
                                    output: output.clone(),
                                    input_tokens,
                                    output_tokens,
                                    duration_ms,
                                };
                                if let Some(r) = self.runs.write().await.get_mut(&run_id) {
                                    r.step_results.push(step_result);
                                }

                                current_input = output.clone();
                                self.record_audit_event(
                                    run_id,
                                    WorkflowAuditEventType::Execution,
                                    Some(&step.name),
                                    format!(
                                        "loop iteration {} completed in {}ms",
                                        loop_iter + 1,
                                        duration_ms
                                    ),
                                    "ok",
                                )
                                .await;

                                if output.to_lowercase().contains(&until_lower) {
                                    info!(
                                        step = i + 1,
                                        name = %step.name,
                                        iterations = loop_iter + 1,
                                        "Loop terminated (until condition met)"
                                    );
                                    self.record_audit_event(
                                        run_id,
                                        WorkflowAuditEventType::Decision,
                                        Some(&step.name),
                                        format!(
                                            "loop terminated on iteration {} (until condition met)",
                                            loop_iter + 1
                                        ),
                                        "until_met",
                                    )
                                    .await;
                                    break;
                                }

                                if loop_iter + 1 == *max_iterations {
                                    info!(
                                        step = i + 1,
                                        name = %step.name,
                                        "Loop terminated (max iterations reached)"
                                    );
                                    self.record_audit_event(
                                        run_id,
                                        WorkflowAuditEventType::Decision,
                                        Some(&step.name),
                                        format!(
                                            "loop terminated after {} iteration(s) (max reached)",
                                            max_iterations
                                        ),
                                        "max_iterations_reached",
                                    )
                                    .await;
                                }
                            }
                            Ok(None) => {
                                self.record_audit_event(
                                    run_id,
                                    WorkflowAuditEventType::Execution,
                                    Some(&step.name),
                                    format!(
                                        "loop iteration {} skipped by error mode",
                                        loop_iter + 1
                                    ),
                                    "skipped",
                                )
                                .await;
                                break;
                            }
                            Err(e) => {
                                self.record_audit_event(
                                    run_id,
                                    WorkflowAuditEventType::Execution,
                                    Some(&step.name),
                                    format!("loop iteration {} failed: {}", loop_iter + 1, e),
                                    "failed",
                                )
                                .await;
                                let (run_state, task_state) =
                                    Self::terminal_failure_state_for_step(step);
                                self.persist_recovery_state(
                                    run_id,
                                    i,
                                    &current_input,
                                    &all_outputs,
                                    &pending_fan_out_outputs,
                                    &review_reject_counts,
                                    &variables,
                                )
                                .await;
                                self.set_run_terminal_error(
                                    run_id,
                                    run_state,
                                    task_state,
                                    e.clone(),
                                )
                                .await;
                                return Err(e);
                            }
                        }
                    }

                    if let Some(ref var) = step.output_var {
                        variables.insert(var.clone(), current_input.clone());
                    }
                    all_outputs.push(current_input.clone());
                }
            }

            self.persist_recovery_state(
                run_id,
                i + 1,
                &current_input,
                &all_outputs,
                &pending_fan_out_outputs,
                &review_reject_counts,
                &variables,
            )
            .await;
            i += 1;
        }

        // Mark workflow as completed
        let final_output = current_input.clone();
        if let Some(r) = self.runs.write().await.get_mut(&run_id) {
            r.state = WorkflowRunState::Completed;
            r.output = Some(final_output.clone());
            let now = Utc::now();
            Self::transition_task_state(r, TaskExecutionState::Done, now);
            r.completed_at = Some(now);
        }
        self.persist_recovery_state(
            run_id,
            workflow.steps.len(),
            &final_output,
            std::slice::from_ref(&final_output),
            &[],
            &review_reject_counts,
            &variables,
        )
        .await;
        self.record_audit_event(
            run_id,
            WorkflowAuditEventType::Decision,
            None,
            "workflow run completed",
            "done",
        )
        .await;

        info!(run_id = %run_id, "Workflow completed successfully");
        Ok(final_output)
    }
}

impl Default for WorkflowEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_workflow() -> Workflow {
        Workflow {
            id: WorkflowId::new(),
            name: "test-pipeline".to_string(),
            description: "A test pipeline".to_string(),
            steps: vec![
                WorkflowStep {
                    name: "analyze".to_string(),
                    agent: StepAgent::ByName {
                        name: "analyst".to_string(),
                    },
                    prompt_template: "Analyze this: {{input}}".to_string(),
                    mode: StepMode::Sequential,
                    timeout_secs: 30,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
                WorkflowStep {
                    name: "summarize".to_string(),
                    agent: StepAgent::ByName {
                        name: "writer".to_string(),
                    },
                    prompt_template: "Summarize this analysis: {{input}}".to_string(),
                    mode: StepMode::Sequential,
                    timeout_secs: 30,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
            ],
            created_at: Utc::now(),
        }
    }

    fn mock_resolver(agent: &StepAgent) -> Option<(AgentId, String)> {
        let _ = agent;
        Some((AgentId::new(), "mock-agent".to_string()))
    }

    #[tokio::test]
    async fn test_register_workflow() {
        let engine = WorkflowEngine::new();
        let wf = test_workflow();
        let id = engine.register(wf.clone()).await;
        assert_eq!(id, wf.id);

        let retrieved = engine.get_workflow(id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "test-pipeline");
    }

    #[tokio::test]
    async fn test_create_run() {
        let engine = WorkflowEngine::new();
        let wf = test_workflow();
        let wf_id = engine.register(wf).await;

        let run_id = engine.create_run(wf_id, "test input".to_string()).await;
        assert!(run_id.is_some());

        let run = engine.get_run(run_id.unwrap()).await.unwrap();
        assert_eq!(run.input, "test input");
        assert!(matches!(run.state, WorkflowRunState::Pending));
        assert_eq!(run.task_state.state, TaskExecutionState::Pending);
        assert!(run.task_state.timestamps.in_progress_at.is_none());
        assert!(run.task_state.timestamps.done_at.is_none());
    }

    #[tokio::test]
    async fn test_list_workflows() {
        let engine = WorkflowEngine::new();
        let wf = test_workflow();
        engine.register(wf).await;

        let list = engine.list_workflows().await;
        assert_eq!(list.len(), 1);
    }

    #[tokio::test]
    async fn test_remove_workflow() {
        let engine = WorkflowEngine::new();
        let wf = test_workflow();
        let id = engine.register(wf).await;

        assert!(engine.remove_workflow(id).await);
        assert!(engine.get_workflow(id).await.is_none());
    }

    #[tokio::test]
    async fn test_execute_pipeline() {
        let engine = WorkflowEngine::new();
        let wf = test_workflow();
        let wf_id = engine.register(wf).await;
        let run_id = engine
            .create_run(wf_id, "raw data".to_string())
            .await
            .unwrap();

        let sender = |_id: AgentId, msg: String| async move {
            Ok((format!("Processed: {msg}"), 100u64, 50u64))
        };

        let result = engine.execute_run(run_id, mock_resolver, sender).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.contains("Processed:"));

        let run = engine.get_run(run_id).await.unwrap();
        assert!(matches!(run.state, WorkflowRunState::Completed));
        assert_eq!(run.task_state.state, TaskExecutionState::Done);
        assert!(run.task_state.timestamps.in_progress_at.is_some());
        assert!(run.task_state.timestamps.done_at.is_some());
        assert_eq!(run.step_results.len(), 2);
        assert!(run.output.is_some());
    }

    #[tokio::test]
    async fn test_conditional_skip() {
        let engine = WorkflowEngine::new();
        let wf = Workflow {
            id: WorkflowId::new(),
            name: "conditional-test".to_string(),
            description: "".to_string(),
            steps: vec![
                WorkflowStep {
                    name: "first".to_string(),
                    agent: StepAgent::ByName {
                        name: "a".to_string(),
                    },
                    prompt_template: "{{input}}".to_string(),
                    mode: StepMode::Sequential,
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
                WorkflowStep {
                    name: "only-if-error".to_string(),
                    agent: StepAgent::ByName {
                        name: "a".to_string(),
                    },
                    prompt_template: "Fix: {{input}}".to_string(),
                    mode: StepMode::Conditional {
                        condition: "ERROR".to_string(),
                    },
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
            ],
            created_at: Utc::now(),
        };
        let wf_id = engine.register(wf).await;
        let run_id = engine
            .create_run(wf_id, "all good".to_string())
            .await
            .unwrap();

        let sender =
            |_id: AgentId, msg: String| async move { Ok((format!("OK: {msg}"), 10u64, 5u64)) };

        let result = engine.execute_run(run_id, mock_resolver, sender).await;
        assert!(result.is_ok());

        let run = engine.get_run(run_id).await.unwrap();
        // Only 1 step executed (conditional was skipped)
        assert_eq!(run.step_results.len(), 1);
    }

    #[tokio::test]
    async fn test_conditional_executes() {
        let engine = WorkflowEngine::new();
        let wf = Workflow {
            id: WorkflowId::new(),
            name: "conditional-test".to_string(),
            description: "".to_string(),
            steps: vec![
                WorkflowStep {
                    name: "first".to_string(),
                    agent: StepAgent::ByName {
                        name: "a".to_string(),
                    },
                    prompt_template: "{{input}}".to_string(),
                    mode: StepMode::Sequential,
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
                WorkflowStep {
                    name: "only-if-error".to_string(),
                    agent: StepAgent::ByName {
                        name: "a".to_string(),
                    },
                    prompt_template: "Fix: {{input}}".to_string(),
                    mode: StepMode::Conditional {
                        condition: "ERROR".to_string(),
                    },
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
            ],
            created_at: Utc::now(),
        };
        let wf_id = engine.register(wf).await;
        let run_id = engine.create_run(wf_id, "data".to_string()).await.unwrap();

        // This sender returns output containing "ERROR"
        let sender = |_id: AgentId, _msg: String| async move {
            Ok(("Found an ERROR in the data".to_string(), 10u64, 5u64))
        };

        let result = engine.execute_run(run_id, mock_resolver, sender).await;
        assert!(result.is_ok());

        let run = engine.get_run(run_id).await.unwrap();
        // Both steps executed
        assert_eq!(run.step_results.len(), 2);
    }

    #[tokio::test]
    async fn test_loop_until_condition() {
        let engine = WorkflowEngine::new();
        let wf = Workflow {
            id: WorkflowId::new(),
            name: "loop-test".to_string(),
            description: "".to_string(),
            steps: vec![WorkflowStep {
                name: "refine".to_string(),
                agent: StepAgent::ByName {
                    name: "a".to_string(),
                },
                prompt_template: "Refine: {{input}}".to_string(),
                mode: StepMode::Loop {
                    max_iterations: 5,
                    until: "DONE".to_string(),
                },
                timeout_secs: 10,
                error_mode: ErrorMode::Fail,
                output_var: None,
            }],
            created_at: Utc::now(),
        };
        let wf_id = engine.register(wf).await;
        let run_id = engine.create_run(wf_id, "draft".to_string()).await.unwrap();

        let call_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();
        let sender = move |_id: AgentId, _msg: String| {
            let cc = cc.clone();
            async move {
                let n = cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n >= 2 {
                    Ok(("Result: DONE".to_string(), 10u64, 5u64))
                } else {
                    Ok(("Still working...".to_string(), 10u64, 5u64))
                }
            }
        };

        let result = engine.execute_run(run_id, mock_resolver, sender).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("DONE"));
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_loop_max_iterations() {
        let engine = WorkflowEngine::new();
        let wf = Workflow {
            id: WorkflowId::new(),
            name: "loop-max-test".to_string(),
            description: "".to_string(),
            steps: vec![WorkflowStep {
                name: "refine".to_string(),
                agent: StepAgent::ByName {
                    name: "a".to_string(),
                },
                prompt_template: "{{input}}".to_string(),
                mode: StepMode::Loop {
                    max_iterations: 3,
                    until: "NEVER_MATCH".to_string(),
                },
                timeout_secs: 10,
                error_mode: ErrorMode::Fail,
                output_var: None,
            }],
            created_at: Utc::now(),
        };
        let wf_id = engine.register(wf).await;
        let run_id = engine.create_run(wf_id, "data".to_string()).await.unwrap();

        let sender = |_id: AgentId, _msg: String| async move {
            Ok(("iteration output".to_string(), 10u64, 5u64))
        };

        let result = engine.execute_run(run_id, mock_resolver, sender).await;
        assert!(result.is_ok());

        let run = engine.get_run(run_id).await.unwrap();
        assert_eq!(run.step_results.len(), 3); // max_iterations
    }

    #[tokio::test]
    async fn test_error_mode_skip() {
        let engine = WorkflowEngine::new();
        let wf = Workflow {
            id: WorkflowId::new(),
            name: "skip-test".to_string(),
            description: "".to_string(),
            steps: vec![
                WorkflowStep {
                    name: "will-fail".to_string(),
                    agent: StepAgent::ByName {
                        name: "a".to_string(),
                    },
                    prompt_template: "{{input}}".to_string(),
                    mode: StepMode::Sequential,
                    timeout_secs: 10,
                    error_mode: ErrorMode::Skip,
                    output_var: None,
                },
                WorkflowStep {
                    name: "succeeds".to_string(),
                    agent: StepAgent::ByName {
                        name: "a".to_string(),
                    },
                    prompt_template: "{{input}}".to_string(),
                    mode: StepMode::Sequential,
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
            ],
            created_at: Utc::now(),
        };
        let wf_id = engine.register(wf).await;
        let run_id = engine.create_run(wf_id, "data".to_string()).await.unwrap();

        let call_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();
        let sender = move |_id: AgentId, _msg: String| {
            let cc = cc.clone();
            async move {
                let n = cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n == 0 {
                    Err("simulated error".to_string())
                } else {
                    Ok(("success".to_string(), 10u64, 5u64))
                }
            }
        };

        let result = engine.execute_run(run_id, mock_resolver, sender).await;
        assert!(result.is_ok());

        let run = engine.get_run(run_id).await.unwrap();
        // Only 1 step result (the first was skipped due to error)
        assert_eq!(run.step_results.len(), 1);
        assert!(matches!(run.state, WorkflowRunState::Completed));
    }

    #[tokio::test]
    async fn test_error_mode_retry() {
        let engine = WorkflowEngine::new();
        let wf = Workflow {
            id: WorkflowId::new(),
            name: "retry-test".to_string(),
            description: "".to_string(),
            steps: vec![WorkflowStep {
                name: "flaky".to_string(),
                agent: StepAgent::ByName {
                    name: "a".to_string(),
                },
                prompt_template: "{{input}}".to_string(),
                mode: StepMode::Sequential,
                timeout_secs: 10,
                error_mode: ErrorMode::Retry { max_retries: 2 },
                output_var: None,
            }],
            created_at: Utc::now(),
        };
        let wf_id = engine.register(wf).await;
        let run_id = engine.create_run(wf_id, "data".to_string()).await.unwrap();

        let call_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();
        let sender = move |_id: AgentId, _msg: String| {
            let cc = cc.clone();
            async move {
                let n = cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n < 2 {
                    Err("transient error".to_string())
                } else {
                    Ok(("finally worked".to_string(), 10u64, 5u64))
                }
            }
        };

        let result = engine.execute_run(run_id, mock_resolver, sender).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "finally worked");
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_error_mode_retry_exhaustion_escalates_to_blocked() {
        let engine = WorkflowEngine::new();
        let wf = Workflow {
            id: WorkflowId::new(),
            name: "retry-block-test".to_string(),
            description: "".to_string(),
            steps: vec![WorkflowStep {
                name: "worker".to_string(),
                agent: StepAgent::ByName {
                    name: "worker".to_string(),
                },
                prompt_template: "{{input}}".to_string(),
                mode: StepMode::Sequential,
                timeout_secs: 5,
                error_mode: ErrorMode::Retry { max_retries: 1 },
                output_var: None,
            }],
            created_at: Utc::now(),
        };
        let wf_id = engine.register(wf).await;
        let run_id = engine
            .create_run(wf_id, "payload".to_string())
            .await
            .unwrap();

        let sender =
            |_id: AgentId, _msg: String| async move { Err("persistent error".to_string()) };

        let result = engine.execute_run(run_id, mock_resolver, sender).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("failed after 1 retries"));

        let run = engine.get_run(run_id).await.unwrap();
        assert!(matches!(run.state, WorkflowRunState::Blocked));
        assert_eq!(run.task_state.state, TaskExecutionState::Blocked);
        assert!(run
            .error
            .unwrap_or_default()
            .contains("failed after 1 retries"));
    }

    #[tokio::test]
    async fn test_output_variables() {
        let engine = WorkflowEngine::new();
        let wf = Workflow {
            id: WorkflowId::new(),
            name: "vars-test".to_string(),
            description: "".to_string(),
            steps: vec![
                WorkflowStep {
                    name: "produce".to_string(),
                    agent: StepAgent::ByName {
                        name: "a".to_string(),
                    },
                    prompt_template: "{{input}}".to_string(),
                    mode: StepMode::Sequential,
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: Some("first_result".to_string()),
                },
                WorkflowStep {
                    name: "transform".to_string(),
                    agent: StepAgent::ByName {
                        name: "a".to_string(),
                    },
                    prompt_template: "{{input}}".to_string(),
                    mode: StepMode::Sequential,
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: Some("second_result".to_string()),
                },
                WorkflowStep {
                    name: "combine".to_string(),
                    agent: StepAgent::ByName {
                        name: "a".to_string(),
                    },
                    prompt_template: "First: {{first_result}} | Second: {{second_result}}"
                        .to_string(),
                    mode: StepMode::Sequential,
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
            ],
            created_at: Utc::now(),
        };
        let wf_id = engine.register(wf).await;
        let run_id = engine.create_run(wf_id, "start".to_string()).await.unwrap();

        let call_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();
        let sender = move |_id: AgentId, msg: String| {
            let cc = cc.clone();
            async move {
                let n = cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                match n {
                    0 => Ok(("alpha".to_string(), 10u64, 5u64)),
                    1 => Ok(("beta".to_string(), 10u64, 5u64)),
                    _ => Ok((format!("Combined: {msg}"), 10u64, 5u64)),
                }
            }
        };

        let result = engine.execute_run(run_id, mock_resolver, sender).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        // The third step receives "First: alpha | Second: beta" as its prompt
        assert!(output.contains("First: alpha"));
        assert!(output.contains("Second: beta"));
    }

    #[tokio::test]
    async fn test_fan_out_parallel() {
        let engine = WorkflowEngine::new();
        let wf = Workflow {
            id: WorkflowId::new(),
            name: "fanout-test".to_string(),
            description: "".to_string(),
            steps: vec![
                WorkflowStep {
                    name: "task-a".to_string(),
                    agent: StepAgent::ByName {
                        name: "a".to_string(),
                    },
                    prompt_template: "Task A: {{input}}".to_string(),
                    mode: StepMode::FanOut,
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
                WorkflowStep {
                    name: "task-b".to_string(),
                    agent: StepAgent::ByName {
                        name: "b".to_string(),
                    },
                    prompt_template: "Task B: {{input}}".to_string(),
                    mode: StepMode::FanOut,
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
                WorkflowStep {
                    name: "collect".to_string(),
                    agent: StepAgent::ByName {
                        name: "c".to_string(),
                    },
                    prompt_template: "unused".to_string(),
                    mode: StepMode::Collect,
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
            ],
            created_at: Utc::now(),
        };
        let wf_id = engine.register(wf).await;
        let run_id = engine.create_run(wf_id, "data".to_string()).await.unwrap();

        let sender =
            |_id: AgentId, msg: String| async move { Ok((format!("Done: {msg}"), 10u64, 5u64)) };

        let result = engine.execute_run(run_id, mock_resolver, sender).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        // Collect step joins all outputs
        assert!(output.contains("Done: Task A"));
        assert!(output.contains("Done: Task B"));
        assert!(output.contains("---"));
    }

    #[tokio::test]
    async fn test_collect_aggregates_only_latest_fan_out_group() {
        let engine = WorkflowEngine::new();
        let wf = Workflow {
            id: WorkflowId::new(),
            name: "fanout-collect-scope-test".to_string(),
            description: "".to_string(),
            steps: vec![
                WorkflowStep {
                    name: "prepare".to_string(),
                    agent: StepAgent::ByName {
                        name: "prep".to_string(),
                    },
                    prompt_template: "Prepare: {{input}}".to_string(),
                    mode: StepMode::Sequential,
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
                WorkflowStep {
                    name: "task-a".to_string(),
                    agent: StepAgent::ByName {
                        name: "a".to_string(),
                    },
                    prompt_template: "Task A: {{input}}".to_string(),
                    mode: StepMode::FanOut,
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
                WorkflowStep {
                    name: "task-b".to_string(),
                    agent: StepAgent::ByName {
                        name: "b".to_string(),
                    },
                    prompt_template: "Task B: {{input}}".to_string(),
                    mode: StepMode::FanOut,
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
                WorkflowStep {
                    name: "collect".to_string(),
                    agent: StepAgent::ByName {
                        name: "collector".to_string(),
                    },
                    prompt_template: "unused".to_string(),
                    mode: StepMode::Collect,
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
            ],
            created_at: Utc::now(),
        };

        let wf_id = engine.register(wf).await;
        let run_id = engine.create_run(wf_id, "seed".to_string()).await.unwrap();

        let sender = |_id: AgentId, msg: String| async move {
            let output = if msg.starts_with("Prepare:") {
                "prep-output".to_string()
            } else if msg.starts_with("Task A:") {
                "branch-a".to_string()
            } else if msg.starts_with("Task B:") {
                "branch-b".to_string()
            } else {
                format!("unexpected: {msg}")
            };
            Ok((output, 10u64, 5u64))
        };

        let result = engine.execute_run(run_id, mock_resolver, sender).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.contains("branch-a"));
        assert!(output.contains("branch-b"));
        assert!(!output.contains("prep-output"));
    }

    #[tokio::test]
    async fn test_fan_out_retry_exhaustion_escalates_to_blocked() {
        let engine = WorkflowEngine::new();
        let wf = Workflow {
            id: WorkflowId::new(),
            name: "fanout-retry-block-test".to_string(),
            description: "".to_string(),
            steps: vec![WorkflowStep {
                name: "worker-branch".to_string(),
                agent: StepAgent::ByName {
                    name: "worker".to_string(),
                },
                prompt_template: "Work: {{input}}".to_string(),
                mode: StepMode::FanOut,
                timeout_secs: 5,
                error_mode: ErrorMode::Retry { max_retries: 1 },
                output_var: None,
            }],
            created_at: Utc::now(),
        };
        let wf_id = engine.register(wf).await;
        let run_id = engine
            .create_run(wf_id, "payload".to_string())
            .await
            .unwrap();

        let sender = |_id: AgentId, _msg: String| async move { Err("branch failure".to_string()) };

        let result = engine.execute_run(run_id, mock_resolver, sender).await;
        assert!(result.is_err());

        let run = engine.get_run(run_id).await.unwrap();
        assert!(matches!(run.state, WorkflowRunState::Blocked));
        assert_eq!(run.task_state.state, TaskExecutionState::Blocked);
        assert!(run
            .error
            .unwrap_or_default()
            .contains("failed after 1 retries"));
    }

    #[tokio::test]
    async fn test_review_reject_and_return_to_planning() {
        let engine = WorkflowEngine::new();
        let wf = Workflow {
            id: WorkflowId::new(),
            name: "review-return-test".to_string(),
            description: "".to_string(),
            steps: vec![
                WorkflowStep {
                    name: "planning".to_string(),
                    agent: StepAgent::ByName {
                        name: "planner".to_string(),
                    },
                    prompt_template: "Plan: {{input}}".to_string(),
                    mode: StepMode::Sequential,
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
                WorkflowStep {
                    name: "review".to_string(),
                    agent: StepAgent::ByName {
                        name: "reviewer".to_string(),
                    },
                    prompt_template: "Review: {{input}}".to_string(),
                    mode: StepMode::Review {
                        reject_if_contains: "reject".to_string(),
                        return_to_step: "planning".to_string(),
                        max_rejects: 2,
                    },
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
                WorkflowStep {
                    name: "dispatch".to_string(),
                    agent: StepAgent::ByName {
                        name: "dispatcher".to_string(),
                    },
                    prompt_template: "Dispatch: {{input}}".to_string(),
                    mode: StepMode::Sequential,
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
            ],
            created_at: Utc::now(),
        };
        let wf_id = engine.register(wf).await;
        let run_id = engine
            .create_run(wf_id, "initial request".to_string())
            .await
            .unwrap();

        let prompts = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let plan_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let review_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let prompts_ref = prompts.clone();
        let plan_ref = plan_count.clone();
        let review_ref = review_count.clone();

        let sender = move |_id: AgentId, msg: String| {
            let prompts_ref = prompts_ref.clone();
            let plan_ref = plan_ref.clone();
            let review_ref = review_ref.clone();
            async move {
                prompts_ref.lock().unwrap().push(msg.clone());
                if msg.starts_with("Plan:") {
                    let n = plan_ref.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if n == 0 {
                        Ok(("plan-v1".to_string(), 10u64, 5u64))
                    } else {
                        Ok(("plan-v2".to_string(), 10u64, 5u64))
                    }
                } else if msg.starts_with("Review:") {
                    let n = review_ref.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if n == 0 {
                        Ok(("REJECT: missing details".to_string(), 10u64, 5u64))
                    } else {
                        Ok(("APPROVED: looks good".to_string(), 10u64, 5u64))
                    }
                } else {
                    Ok((format!("dispatch-final: {msg}"), 10u64, 5u64))
                }
            }
        };

        let result = engine.execute_run(run_id, mock_resolver, sender).await;
        assert!(result.is_ok());
        assert!(result
            .unwrap()
            .contains("dispatch-final: Dispatch: APPROVED"));

        let run = engine.get_run(run_id).await.unwrap();
        assert!(matches!(run.state, WorkflowRunState::Completed));
        assert_eq!(run.step_results.len(), 5); // planning x2 + review x2 + dispatch x1
        assert_eq!(plan_count.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert_eq!(review_count.load(std::sync::atomic::Ordering::SeqCst), 2);

        let prompts = prompts.lock().unwrap();
        let plan_prompts: Vec<&String> =
            prompts.iter().filter(|p| p.starts_with("Plan:")).collect();
        assert_eq!(plan_prompts.len(), 2);
        assert!(plan_prompts[1].contains("REJECT: missing details"));
    }

    #[tokio::test]
    async fn test_review_reject_exceeds_max_retries() {
        let engine = WorkflowEngine::new();
        let wf = Workflow {
            id: WorkflowId::new(),
            name: "review-reject-limit-test".to_string(),
            description: "".to_string(),
            steps: vec![
                WorkflowStep {
                    name: "planning".to_string(),
                    agent: StepAgent::ByName {
                        name: "planner".to_string(),
                    },
                    prompt_template: "Plan: {{input}}".to_string(),
                    mode: StepMode::Sequential,
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
                WorkflowStep {
                    name: "review".to_string(),
                    agent: StepAgent::ByName {
                        name: "reviewer".to_string(),
                    },
                    prompt_template: "Review: {{input}}".to_string(),
                    mode: StepMode::Review {
                        reject_if_contains: "reject".to_string(),
                        return_to_step: "planning".to_string(),
                        max_rejects: 1,
                    },
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
            ],
            created_at: Utc::now(),
        };
        let wf_id = engine.register(wf).await;
        let run_id = engine
            .create_run(wf_id, "initial".to_string())
            .await
            .unwrap();

        let sender = |_id: AgentId, msg: String| async move {
            if msg.starts_with("Plan:") {
                Ok(("plan-proposal".to_string(), 10u64, 5u64))
            } else {
                Ok(("reject: not good enough".to_string(), 10u64, 5u64))
            }
        };

        let result = engine.execute_run(run_id, mock_resolver, sender).await;
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("exceeded max_rejects"));

        let run = engine.get_run(run_id).await.unwrap();
        assert!(matches!(run.state, WorkflowRunState::Failed));
        assert_eq!(run.task_state.state, TaskExecutionState::Failed);
    }

    #[tokio::test]
    async fn test_trace_id_queries_audit_events_across_decision_dispatch_execution_and_review() {
        let engine = WorkflowEngine::new();
        let wf = Workflow {
            id: WorkflowId::new(),
            name: "trace-audit-test".to_string(),
            description: "".to_string(),
            steps: vec![
                WorkflowStep {
                    name: "planning".to_string(),
                    agent: StepAgent::ByName {
                        name: "planner".to_string(),
                    },
                    prompt_template: "Plan: {{input}}".to_string(),
                    mode: StepMode::Sequential,
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
                WorkflowStep {
                    name: "review".to_string(),
                    agent: StepAgent::ByName {
                        name: "reviewer".to_string(),
                    },
                    prompt_template: "Review: {{input}}".to_string(),
                    mode: StepMode::Review {
                        reject_if_contains: "reject".to_string(),
                        return_to_step: "planning".to_string(),
                        max_rejects: 2,
                    },
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
                WorkflowStep {
                    name: "dispatch".to_string(),
                    agent: StepAgent::ByName {
                        name: "dispatcher".to_string(),
                    },
                    prompt_template: "Dispatch: {{input}}".to_string(),
                    mode: StepMode::Sequential,
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
            ],
            created_at: Utc::now(),
        };

        let wf_id = engine.register(wf).await;
        let run_id = engine
            .create_run(wf_id, "initial".to_string())
            .await
            .unwrap();

        let plan_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let review_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let plan_ref = plan_count.clone();
        let review_ref = review_count.clone();
        let sender = move |_id: AgentId, msg: String| {
            let plan_ref = plan_ref.clone();
            let review_ref = review_ref.clone();
            async move {
                if msg.starts_with("Plan:") {
                    let n = plan_ref.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if n == 0 {
                        Ok(("plan-v1".to_string(), 10u64, 5u64))
                    } else {
                        Ok(("plan-v2".to_string(), 10u64, 5u64))
                    }
                } else if msg.starts_with("Review:") {
                    let n = review_ref.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if n == 0 {
                        Ok(("REJECT: missing details".to_string(), 10u64, 5u64))
                    } else {
                        Ok(("APPROVED".to_string(), 10u64, 5u64))
                    }
                } else {
                    Ok(("dispatch-ok".to_string(), 10u64, 5u64))
                }
            }
        };

        let result = engine.execute_run(run_id, mock_resolver, sender).await;
        assert!(result.is_ok());

        let run = engine.get_run(run_id).await.unwrap();
        assert!(!run.trace_id.is_empty());

        let queried_run = engine.get_run_by_trace_id(&run.trace_id).await.unwrap();
        assert_eq!(queried_run.id, run_id);

        let events = engine.list_audit_events_by_trace_id(&run.trace_id).await;
        assert!(!events.is_empty());
        assert!(events.iter().all(|event| event.trace_id == run.trace_id));
        assert!(events
            .iter()
            .any(|event| event.event_type == WorkflowAuditEventType::Decision));
        assert!(events
            .iter()
            .any(|event| event.event_type == WorkflowAuditEventType::Dispatch));
        assert!(events
            .iter()
            .any(|event| event.event_type == WorkflowAuditEventType::Execution));
        assert!(events
            .iter()
            .any(|event| event.event_type == WorkflowAuditEventType::Review));
    }

    #[tokio::test]
    async fn test_trace_id_query_returns_empty_for_unknown_trace() {
        let engine = WorkflowEngine::new();
        assert!(engine
            .list_audit_events_by_trace_id("trace-does-not-exist")
            .await
            .is_empty());
        assert!(engine
            .get_run_by_trace_id("trace-does-not-exist")
            .await
            .is_none());
    }

    #[tokio::test]
    async fn test_observability_metrics_expose_success_failure_retry_and_reject_rates() {
        let engine = WorkflowEngine::new();

        let review_workflow = Workflow {
            id: WorkflowId::new(),
            name: "metrics-review-run".to_string(),
            description: "".to_string(),
            steps: vec![
                WorkflowStep {
                    name: "planning".to_string(),
                    agent: StepAgent::ByName {
                        name: "planner".to_string(),
                    },
                    prompt_template: "Plan: {{input}}".to_string(),
                    mode: StepMode::Sequential,
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
                WorkflowStep {
                    name: "review".to_string(),
                    agent: StepAgent::ByName {
                        name: "reviewer".to_string(),
                    },
                    prompt_template: "Review: {{input}}".to_string(),
                    mode: StepMode::Review {
                        reject_if_contains: "reject".to_string(),
                        return_to_step: "planning".to_string(),
                        max_rejects: 2,
                    },
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
                WorkflowStep {
                    name: "dispatch".to_string(),
                    agent: StepAgent::ByName {
                        name: "dispatcher".to_string(),
                    },
                    prompt_template: "Dispatch: {{input}}".to_string(),
                    mode: StepMode::Sequential,
                    timeout_secs: 10,
                    error_mode: ErrorMode::Fail,
                    output_var: None,
                },
            ],
            created_at: Utc::now(),
        };

        let retry_workflow = Workflow {
            id: WorkflowId::new(),
            name: "metrics-retry-run".to_string(),
            description: "".to_string(),
            steps: vec![WorkflowStep {
                name: "worker".to_string(),
                agent: StepAgent::ByName {
                    name: "worker".to_string(),
                },
                prompt_template: "Work: {{input}}".to_string(),
                mode: StepMode::Sequential,
                timeout_secs: 10,
                error_mode: ErrorMode::Retry { max_retries: 1 },
                output_var: None,
            }],
            created_at: Utc::now(),
        };

        let review_id = engine.register(review_workflow).await;
        let retry_id = engine.register(retry_workflow).await;

        let review_run_id = engine
            .create_run(review_id, "initial".to_string())
            .await
            .unwrap();
        let retry_run_id = engine
            .create_run(retry_id, "payload".to_string())
            .await
            .unwrap();

        let review_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let review_ref = review_count.clone();
        let review_sender = move |_id: AgentId, msg: String| {
            let review_ref = review_ref.clone();
            async move {
                if msg.starts_with("Review:") {
                    let n = review_ref.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if n == 0 {
                        Ok(("REJECT: add more detail".to_string(), 10u64, 5u64))
                    } else {
                        Ok(("APPROVED".to_string(), 10u64, 5u64))
                    }
                } else {
                    Ok(("ok".to_string(), 10u64, 5u64))
                }
            }
        };

        let retry_sender =
            |_id: AgentId, _msg: String| async move { Err("always failing".to_string()) };

        assert!(engine
            .execute_run(review_run_id, mock_resolver, review_sender)
            .await
            .is_ok());
        assert!(engine
            .execute_run(retry_run_id, mock_resolver, retry_sender)
            .await
            .is_err());

        let metrics = engine.observability_metrics().await;
        let epsilon = 1e-9;

        assert_eq!(metrics.runs_total, 2);
        assert_eq!(metrics.terminal_runs_total, 2);
        assert!((metrics.success_rate - 0.5).abs() < epsilon);
        assert!((metrics.failure_rate - 0.5).abs() < epsilon);
        assert!(metrics.retry_rate > 0.0);
        assert!((metrics.reject_rate - 0.5).abs() < epsilon);
        assert_eq!(metrics.resume_time_ms, 0.0);
    }

    #[tokio::test]
    async fn test_observability_metrics_resume_time_ms_uses_resume_transitions() {
        let engine = WorkflowEngine::new();
        let wf_id = engine.register(test_workflow()).await;
        let run_id = engine
            .create_run(wf_id, "resume-case".to_string())
            .await
            .unwrap();

        let t0 = Utc::now();
        let t1 = t0 + chrono::Duration::seconds(1);
        let t2 = t1 + chrono::Duration::seconds(2);
        let t3 = t2 + chrono::Duration::milliseconds(1500);
        let t4 = t3 + chrono::Duration::seconds(1);

        let mut task_state = DurableTaskState::new(t0);
        task_state
            .transition(TaskExecutionState::InProgress, t1)
            .unwrap();
        task_state
            .transition(TaskExecutionState::Blocked, t2)
            .unwrap();
        task_state
            .transition(TaskExecutionState::InProgress, t3)
            .unwrap();
        task_state.transition(TaskExecutionState::Done, t4).unwrap();

        {
            let mut runs = engine.runs.write().await;
            let run = runs.get_mut(&run_id).unwrap();
            run.state = WorkflowRunState::Completed;
            run.task_state = task_state;
        }

        let metrics = engine.observability_metrics().await;
        let expected_resume_ms = 1500.0;
        let epsilon = 1e-9;
        assert!((metrics.resume_time_ms - expected_resume_ms).abs() < epsilon);
    }

    #[tokio::test]
    async fn test_recovery_snapshot_restores_failed_run_and_resumes_from_saved_step() {
        let engine = WorkflowEngine::new();
        let wf_id = engine.register(test_workflow()).await;
        let run_id = engine
            .create_run(wf_id, "raw data".to_string())
            .await
            .unwrap();

        let sender = |_id: AgentId, msg: String| async move {
            if msg.starts_with("Analyze this:") {
                Ok(("analysis-ready".to_string(), 10u64, 5u64))
            } else {
                Err("transient summary failure".to_string())
            }
        };

        let result = engine.execute_run(run_id, mock_resolver, sender).await;
        assert!(result.is_err());

        let failed_run = engine.get_run(run_id).await.unwrap();
        assert!(matches!(failed_run.state, WorkflowRunState::Failed));
        assert_eq!(failed_run.step_results.len(), 1);
        assert_eq!(failed_run.recovery.next_step_index, 1);
        assert_eq!(failed_run.recovery.current_input, "analysis-ready");

        let tempdir = tempfile::tempdir().unwrap();
        let snapshot_path = tempdir.path().join("workflow-recovery.json");
        engine.save_recovery_snapshot(&snapshot_path).await.unwrap();

        let recovered_engine = WorkflowEngine::load_recovery_snapshot(&snapshot_path)
            .await
            .unwrap();
        let resumed_prompts = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let prompts_ref = resumed_prompts.clone();
        let resumed_sender = move |_id: AgentId, msg: String| {
            let prompts_ref = prompts_ref.clone();
            async move {
                prompts_ref.lock().unwrap().push(msg.clone());
                if msg.starts_with("Analyze this:") {
                    Err("first step should not rerun after recovery".to_string())
                } else {
                    Ok(("summary-ready".to_string(), 10u64, 5u64))
                }
            }
        };

        let resumed_output = recovered_engine
            .execute_run(run_id, mock_resolver, resumed_sender)
            .await
            .unwrap();
        assert_eq!(resumed_output, "summary-ready");

        let resumed_run = recovered_engine.get_run(run_id).await.unwrap();
        assert!(matches!(resumed_run.state, WorkflowRunState::Completed));
        assert_eq!(resumed_run.step_results.len(), 2);
        let resumed_prompts = resumed_prompts.lock().unwrap();
        assert_eq!(resumed_prompts.len(), 1);
        assert!(resumed_prompts[0].starts_with("Summarize this analysis:"));
    }

    #[tokio::test]
    async fn test_recovery_snapshot_blocks_interrupted_run_before_resume() {
        let engine = WorkflowEngine::new();
        let wf_id = engine.register(test_workflow()).await;
        let run_id = engine
            .create_run(wf_id, "interrupted input".to_string())
            .await
            .unwrap();

        {
            let mut runs = engine.runs.write().await;
            let run = runs.get_mut(&run_id).unwrap();
            run.state = WorkflowRunState::Running;
            run.task_state
                .transition(TaskExecutionState::InProgress, Utc::now())
                .unwrap();
            run.step_results.push(StepResult {
                step_name: "analyze".to_string(),
                agent_id: AgentId::new().to_string(),
                agent_name: "mock-agent".to_string(),
                output: "analysis-resume".to_string(),
                input_tokens: 10,
                output_tokens: 5,
                duration_ms: 1,
            });
            run.recovery = WorkflowRunRecoveryState {
                next_step_index: 1,
                current_input: "analysis-resume".to_string(),
                all_outputs: vec!["analysis-resume".to_string()],
                pending_fan_out_outputs: Vec::new(),
                review_reject_counts: HashMap::new(),
                variables: HashMap::new(),
            };
        }

        let tempdir = tempfile::tempdir().unwrap();
        let snapshot_path = tempdir.path().join("interrupted-workflow.json");
        engine.save_recovery_snapshot(&snapshot_path).await.unwrap();

        let recovered_engine = WorkflowEngine::load_recovery_snapshot(&snapshot_path)
            .await
            .unwrap();
        let recovered_run = recovered_engine.get_run(run_id).await.unwrap();
        assert!(matches!(recovered_run.state, WorkflowRunState::Blocked));
        assert_eq!(recovered_run.task_state.state, TaskExecutionState::Blocked);
        assert_eq!(recovered_run.recovery.next_step_index, 1);

        let resumed_sender = |_id: AgentId, msg: String| async move {
            if msg.starts_with("Analyze this:") {
                Err("interrupted step must not restart from zero".to_string())
            } else {
                Ok(("resumed-summary".to_string(), 10u64, 5u64))
            }
        };

        let resumed_output = recovered_engine
            .execute_run(run_id, mock_resolver, resumed_sender)
            .await
            .unwrap();
        assert_eq!(resumed_output, "resumed-summary");
    }

    #[tokio::test]
    async fn test_step_quality_gate_enforced_workflow_requires_gate_config() {
        let engine = WorkflowEngine::new();
        let wf_id = engine.register(test_workflow()).await;
        engine.enable_step_quality_gates(wf_id).await.unwrap();
        let run_id = engine
            .create_run(wf_id, "raw data".to_string())
            .await
            .unwrap();

        let sender = |_id: AgentId, msg: String| async move {
            Ok((format!("Processed: {msg}"), 10u64, 5u64))
        };

        let error = engine
            .execute_run(run_id, mock_resolver, sender)
            .await
            .unwrap_err();
        assert!(error.contains("requires acceptance criteria and validation command"));

        let run = engine.get_run(run_id).await.unwrap();
        assert!(matches!(run.state, WorkflowRunState::Failed));
        assert_eq!(run.step_results.len(), 0);
        assert!(run.audit_events.iter().any(|event| {
            event.step_name.as_deref() == Some("analyze") && event.outcome == "quality_gate_missing"
        }));
    }

    #[tokio::test]
    async fn test_step_quality_gate_failure_blocks_step_completion() {
        let engine = WorkflowEngine::new();
        let workflow = Workflow {
            id: WorkflowId::new(),
            name: "quality-gate-fail".to_string(),
            description: "single step quality gate failure".to_string(),
            steps: vec![WorkflowStep {
                name: "analyze".to_string(),
                agent: StepAgent::ByName {
                    name: "analyst".to_string(),
                },
                prompt_template: "Analyze this: {{input}}".to_string(),
                mode: StepMode::Sequential,
                timeout_secs: 30,
                error_mode: ErrorMode::Fail,
                output_var: None,
            }],
            created_at: Utc::now(),
        };
        let wf_id = workflow.id;
        engine.register(workflow).await;
        engine.enable_step_quality_gates(wf_id).await.unwrap();
        engine
            .set_step_quality_gate(
                wf_id,
                "analyze",
                WorkflowStepQualityGate {
                    acceptance_criteria: "output must equal approved".to_string(),
                    validation_command: r#"test "$OPENFANG_STEP_OUTPUT" = "approved""#.to_string(),
                },
            )
            .await
            .unwrap();

        let run_id = engine
            .create_run(wf_id, "raw data".to_string())
            .await
            .unwrap();
        let sender = |_id: AgentId, _msg: String| async move {
            Ok(("analysis output".to_string(), 10u64, 5u64))
        };

        let error = engine
            .execute_run(run_id, mock_resolver, sender)
            .await
            .unwrap_err();
        assert!(error.contains("quality gate failed"));

        let run = engine.get_run(run_id).await.unwrap();
        assert!(matches!(run.state, WorkflowRunState::Failed));
        assert_eq!(run.step_results.len(), 0);
        assert!(run.audit_events.iter().any(|event| {
            event.step_name.as_deref() == Some("analyze") && event.outcome == "quality_gate_failed"
        }));
    }

    #[tokio::test]
    async fn test_step_quality_gate_snapshot_restore_and_pass() {
        let engine = WorkflowEngine::new();
        let wf_id = engine.register(test_workflow()).await;
        engine.enable_step_quality_gates(wf_id).await.unwrap();
        engine
            .set_step_quality_gate(
                wf_id,
                "analyze",
                WorkflowStepQualityGate {
                    acceptance_criteria: "analyze step receives output and criteria".to_string(),
                    validation_command: r#"test "$OPENFANG_STEP_NAME" = "analyze" && test -n "$OPENFANG_STEP_OUTPUT" && test -n "$OPENFANG_ACCEPTANCE_CRITERIA""#.to_string(),
                },
            )
            .await
            .unwrap();
        engine
            .set_step_quality_gate(
                wf_id,
                "summarize",
                WorkflowStepQualityGate {
                    acceptance_criteria: "summarize step receives output and criteria".to_string(),
                    validation_command: r#"test "$OPENFANG_STEP_NAME" = "summarize" && test -n "$OPENFANG_STEP_OUTPUT" && test -n "$OPENFANG_ACCEPTANCE_CRITERIA""#.to_string(),
                },
            )
            .await
            .unwrap();

        let tempdir = tempfile::tempdir().unwrap();
        let snapshot_path = tempdir.path().join("workflow-gates.json");
        engine.save_recovery_snapshot(&snapshot_path).await.unwrap();

        let recovered_engine = WorkflowEngine::load_recovery_snapshot(&snapshot_path)
            .await
            .unwrap();
        assert!(recovered_engine.step_quality_gates_enabled(wf_id).await);
        let analyze_gate = recovered_engine
            .get_step_quality_gate(wf_id, "analyze")
            .await
            .unwrap();
        assert_eq!(
            analyze_gate.acceptance_criteria,
            "analyze step receives output and criteria"
        );

        let run_id = recovered_engine
            .create_run(wf_id, "raw data".to_string())
            .await
            .unwrap();
        let sender = |_id: AgentId, msg: String| async move {
            Ok((format!("Processed: {msg}"), 10u64, 5u64))
        };

        let output = recovered_engine
            .execute_run(run_id, mock_resolver, sender)
            .await
            .unwrap();
        assert!(output.contains("Processed:"));

        let run = recovered_engine.get_run(run_id).await.unwrap();
        assert!(matches!(run.state, WorkflowRunState::Completed));
        assert!(run.audit_events.iter().any(|event| {
            event.step_name.as_deref() == Some("analyze") && event.outcome == "quality_gate_passed"
        }));
        assert!(run.audit_events.iter().any(|event| {
            event.step_name.as_deref() == Some("summarize")
                && event.outcome == "quality_gate_passed"
        }));
    }

    #[tokio::test]
    async fn test_step_quality_gate_logs_capture_command_exit_output_and_timestamp() {
        let engine = WorkflowEngine::new();
        let workflow = Workflow {
            id: WorkflowId::new(),
            name: "quality-gate-log-failure".to_string(),
            description: "capture failed gate log".to_string(),
            steps: vec![WorkflowStep {
                name: "analyze".to_string(),
                agent: StepAgent::ByName {
                    name: "analyst".to_string(),
                },
                prompt_template: "Analyze this: {{input}}".to_string(),
                mode: StepMode::Sequential,
                timeout_secs: 30,
                error_mode: ErrorMode::Fail,
                output_var: None,
            }],
            created_at: Utc::now(),
        };
        let wf_id = workflow.id;
        engine.register(workflow).await;
        engine.enable_step_quality_gates(wf_id).await.unwrap();
        let command = r#"printf 'gate failed output'; exit 7"#;
        engine
            .set_step_quality_gate(
                wf_id,
                "analyze",
                WorkflowStepQualityGate {
                    acceptance_criteria: "must pass shell validation".to_string(),
                    validation_command: command.to_string(),
                },
            )
            .await
            .unwrap();

        let run_id = engine
            .create_run(wf_id, "raw data".to_string())
            .await
            .unwrap();
        let before = Utc::now();
        let sender = |_id: AgentId, _msg: String| async move {
            Ok(("analysis output".to_string(), 10u64, 5u64))
        };

        let error = engine
            .execute_run(run_id, mock_resolver, sender)
            .await
            .unwrap_err();
        let after = Utc::now();
        assert!(error.contains("quality gate failed"));

        let run = engine.get_run(run_id).await.unwrap();
        assert_eq!(run.quality_gate_logs.len(), 1);
        let log = &run.quality_gate_logs[0];
        assert_eq!(log.step_name, "analyze");
        assert_eq!(log.validation_command, command);
        assert_eq!(log.exit_code, Some(7));
        assert!(log.output.contains("gate failed output"));
        assert!(log.attempted_at >= before);
        assert!(log.attempted_at <= after);
    }

    #[tokio::test]
    async fn test_step_quality_gate_logs_capture_every_loop_attempt() {
        let engine = WorkflowEngine::new();
        let workflow = Workflow {
            id: WorkflowId::new(),
            name: "quality-gate-loop-log".to_string(),
            description: "capture every loop gate attempt".to_string(),
            steps: vec![WorkflowStep {
                name: "refine".to_string(),
                agent: StepAgent::ByName {
                    name: "refiner".to_string(),
                },
                prompt_template: "Refine: {{input}}".to_string(),
                mode: StepMode::Loop {
                    max_iterations: 2,
                    until: "done".to_string(),
                },
                timeout_secs: 30,
                error_mode: ErrorMode::Fail,
                output_var: None,
            }],
            created_at: Utc::now(),
        };
        let wf_id = workflow.id;
        engine.register(workflow).await;
        engine.enable_step_quality_gates(wf_id).await.unwrap();
        engine
            .set_step_quality_gate(
                wf_id,
                "refine",
                WorkflowStepQualityGate {
                    acceptance_criteria: "loop outputs are logged".to_string(),
                    validation_command: r#"printf '%s' "$OPENFANG_STEP_OUTPUT""#.to_string(),
                },
            )
            .await
            .unwrap();

        let run_id = engine.create_run(wf_id, "draft".to_string()).await.unwrap();
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter_ref = counter.clone();
        let sender = move |_id: AgentId, _msg: String| {
            let counter_ref = counter_ref.clone();
            async move {
                let current = counter_ref.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if current == 0 {
                    Ok(("keep going".to_string(), 10u64, 5u64))
                } else {
                    Ok(("done".to_string(), 10u64, 5u64))
                }
            }
        };

        let output = engine
            .execute_run(run_id, mock_resolver, sender)
            .await
            .unwrap();
        assert_eq!(output, "done");

        let run = engine.get_run(run_id).await.unwrap();
        assert_eq!(run.quality_gate_logs.len(), 2);
        assert_eq!(run.quality_gate_logs[0].output, "keep going");
        assert_eq!(run.quality_gate_logs[1].output, "done");
        assert_eq!(run.quality_gate_logs[0].exit_code, Some(0));
        assert_eq!(run.quality_gate_logs[1].exit_code, Some(0));
    }

    #[tokio::test]
    async fn test_step_quality_gate_logs_persist_across_snapshot_restore() {
        let engine = WorkflowEngine::new();
        let wf_id = engine.register(test_workflow()).await;
        engine.enable_step_quality_gates(wf_id).await.unwrap();
        engine
            .set_step_quality_gate(
                wf_id,
                "analyze",
                WorkflowStepQualityGate {
                    acceptance_criteria: "store analyze gate output".to_string(),
                    validation_command: r#"printf '%s' "$OPENFANG_STEP_NAME""#.to_string(),
                },
            )
            .await
            .unwrap();
        engine
            .set_step_quality_gate(
                wf_id,
                "summarize",
                WorkflowStepQualityGate {
                    acceptance_criteria: "store summarize gate output".to_string(),
                    validation_command: r#"printf '%s' "$OPENFANG_STEP_NAME""#.to_string(),
                },
            )
            .await
            .unwrap();

        let run_id = engine
            .create_run(wf_id, "raw data".to_string())
            .await
            .unwrap();
        let sender = |_id: AgentId, msg: String| async move {
            Ok((format!("Processed: {msg}"), 10u64, 5u64))
        };
        engine
            .execute_run(run_id, mock_resolver, sender)
            .await
            .unwrap();

        let tempdir = tempfile::tempdir().unwrap();
        let snapshot_path = tempdir.path().join("workflow-gate-logs.json");
        engine.save_recovery_snapshot(&snapshot_path).await.unwrap();

        let recovered_engine = WorkflowEngine::load_recovery_snapshot(&snapshot_path)
            .await
            .unwrap();
        let run = recovered_engine.get_run(run_id).await.unwrap();
        assert_eq!(run.quality_gate_logs.len(), 2);
        assert_eq!(run.quality_gate_logs[0].output, "analyze");
        assert_eq!(run.quality_gate_logs[1].output, "summarize");
        assert_eq!(run.quality_gate_logs[0].exit_code, Some(0));
        assert_eq!(run.quality_gate_logs[1].exit_code, Some(0));
        assert!(run.quality_gate_logs[0].attempted_at <= run.quality_gate_logs[1].attempted_at);
    }

    #[tokio::test]
    async fn test_shadow_run_comparison_records_exact_and_normalized_matches() {
        let engine = WorkflowEngine::new();
        let wf_id = engine.register(test_workflow()).await;
        let run_id = engine
            .create_run(wf_id, "raw data".to_string())
            .await
            .unwrap();
        let sender = |_id: AgentId, msg: String| async move {
            Ok((format!("Processed: {msg}"), 10u64, 5u64))
        };
        engine
            .execute_run(run_id, mock_resolver, sender)
            .await
            .unwrap();

        let exact = engine
            .record_shadow_comparison(
                run_id,
                "Processed: Summarize this analysis: Processed: Analyze this: raw data",
            )
            .await
            .unwrap();
        assert!(exact.matches);
        assert!(exact.normalized_matches);
        assert_eq!(exact.first_mismatch_index, None);

        let normalized_only = engine
            .record_shadow_comparison(
                run_id,
                "Processed:  Summarize this analysis:
Processed: Analyze this: raw data",
            )
            .await
            .unwrap();
        assert!(!normalized_only.matches);
        assert!(normalized_only.normalized_matches);
        assert_eq!(normalized_only.first_mismatch_index, Some(11));

        let run = engine.get_run(run_id).await.unwrap();
        let shadow = run.shadow.expect("shadow comparison should be stored");
        assert!(!shadow.matches);
        assert!(shadow.normalized_matches);
        assert_eq!(shadow.first_mismatch_index, Some(11));
        assert!(run.audit_events.iter().any(|event| {
            event.outcome == "shadow_match" || event.outcome == "shadow_normalized_match"
        }));
    }

    #[tokio::test]
    async fn test_shadow_run_comparison_persists_across_snapshot_restore() {
        let engine = WorkflowEngine::new();
        let wf_id = engine.register(test_workflow()).await;
        let run_id = engine
            .create_run(wf_id, "raw data".to_string())
            .await
            .unwrap();
        let sender = |_id: AgentId, msg: String| async move {
            Ok((format!("Processed: {msg}"), 10u64, 5u64))
        };
        engine
            .execute_run(run_id, mock_resolver, sender)
            .await
            .unwrap();
        engine
            .record_shadow_comparison(run_id, "legacy production output")
            .await
            .unwrap();

        let tempdir = tempfile::tempdir().unwrap();
        let snapshot_path = tempdir.path().join("workflow-shadow-comparison.json");
        engine.save_recovery_snapshot(&snapshot_path).await.unwrap();

        let recovered_engine = WorkflowEngine::load_recovery_snapshot(&snapshot_path)
            .await
            .unwrap();
        let run = recovered_engine.get_run(run_id).await.unwrap();
        let shadow = run.shadow.expect("shadow comparison should persist");
        assert_eq!(shadow.production_output, "legacy production output");
        assert!(shadow
            .shadow_output
            .contains("Processed: Summarize this analysis"));
        assert!(!shadow.matches);
        assert!(run
            .audit_events
            .iter()
            .any(|event| event.outcome == "shadow_mismatch"));
    }

    #[tokio::test]
    async fn test_workflow_rollout_state_defaults_and_fast_rollback() {
        let engine = WorkflowEngine::new();
        let wf_id = engine.register(test_workflow()).await;

        let default_state = engine.get_rollout_state(wf_id).await.unwrap();
        assert_eq!(default_state.primary_path, WorkflowTrafficPath::Production);
        assert_eq!(default_state.stable_path, WorkflowTrafficPath::Production);
        assert!(!default_state.shadow_enabled);
        assert_eq!(default_state.rollback_window_secs, 300);
        assert!(default_state.rollback_checklist.len() >= 4);

        let promoted = engine
            .update_rollout_state(
                wf_id,
                Some(WorkflowTrafficPath::Openfang),
                Some(WorkflowTrafficPath::Production),
                Some(true),
                Some(300),
            )
            .await
            .unwrap();
        assert_eq!(promoted.primary_path, WorkflowTrafficPath::Openfang);
        assert_eq!(promoted.stable_path, WorkflowTrafficPath::Production);
        assert!(promoted.shadow_enabled);

        let rolled_back = engine.rollback_to_stable_path(wf_id).await.unwrap();
        assert_eq!(rolled_back.primary_path, WorkflowTrafficPath::Production);
        assert_eq!(rolled_back.stable_path, WorkflowTrafficPath::Production);
        assert!(!rolled_back.shadow_enabled);
        let record = rolled_back
            .last_rollback
            .expect("rollback record should exist");
        assert_eq!(record.from_path, WorkflowTrafficPath::Openfang);
        assert_eq!(record.to_path, WorkflowTrafficPath::Production);
        assert!(record.shadow_enabled_before);
        assert!(!record.shadow_enabled_after);
        assert!(record.within_window);
        assert!(record.duration_ms <= 300_000);
        assert_eq!(record.checklist.len(), rolled_back.rollback_checklist.len());
    }

    #[tokio::test]
    async fn test_workflow_rollout_state_persists_across_snapshot_restore() {
        let engine = WorkflowEngine::new();
        let wf_id = engine.register(test_workflow()).await;
        engine
            .update_rollout_state(
                wf_id,
                Some(WorkflowTrafficPath::Openfang),
                Some(WorkflowTrafficPath::Production),
                Some(true),
                Some(300),
            )
            .await
            .unwrap();
        engine.rollback_to_stable_path(wf_id).await.unwrap();

        let tempdir = tempfile::tempdir().unwrap();
        let snapshot_path = tempdir.path().join("workflow-rollout-state.json");
        engine.save_recovery_snapshot(&snapshot_path).await.unwrap();

        let recovered_engine = WorkflowEngine::load_recovery_snapshot(&snapshot_path)
            .await
            .unwrap();
        let recovered = recovered_engine.get_rollout_state(wf_id).await.unwrap();
        assert_eq!(recovered.primary_path, WorkflowTrafficPath::Production);
        assert_eq!(recovered.stable_path, WorkflowTrafficPath::Production);
        assert!(!recovered.shadow_enabled);
        assert_eq!(recovered.rollback_window_secs, 300);
        let record = recovered
            .last_rollback
            .expect("rollback record should persist across snapshot restore");
        assert_eq!(record.from_path, WorkflowTrafficPath::Openfang);
        assert_eq!(record.to_path, WorkflowTrafficPath::Production);
        assert!(record.within_window);
        assert!(!record.checklist.is_empty());
    }

    #[tokio::test]
    async fn test_expand_variables() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "Alice".to_string());
        vars.insert("task".to_string(), "code review".to_string());

        let result = WorkflowEngine::expand_variables(
            "Hello {{name}}, please do {{task}} on {{input}}",
            "main.rs",
            &vars,
        );
        assert_eq!(result, "Hello Alice, please do code review on main.rs");
    }

    #[tokio::test]
    async fn test_error_mode_serialization() {
        let fail_json = serde_json::to_string(&ErrorMode::Fail).unwrap();
        assert_eq!(fail_json, "\"fail\"");

        let skip_json = serde_json::to_string(&ErrorMode::Skip).unwrap();
        assert_eq!(skip_json, "\"skip\"");

        let retry_json = serde_json::to_string(&ErrorMode::Retry { max_retries: 3 }).unwrap();
        let retry: ErrorMode = serde_json::from_str(&retry_json).unwrap();
        assert!(matches!(retry, ErrorMode::Retry { max_retries: 3 }));
    }

    #[tokio::test]
    async fn test_step_mode_conditional_serialization() {
        let mode = StepMode::Conditional {
            condition: "error".to_string(),
        };
        let json = serde_json::to_string(&mode).unwrap();
        let parsed: StepMode = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, StepMode::Conditional { condition } if condition == "error"));
    }

    #[tokio::test]
    async fn test_step_mode_loop_serialization() {
        let mode = StepMode::Loop {
            max_iterations: 5,
            until: "done".to_string(),
        };
        let json = serde_json::to_string(&mode).unwrap();
        let parsed: StepMode = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, StepMode::Loop { max_iterations: 5, until } if until == "done"));
    }

    #[tokio::test]
    async fn test_route_workflow_by_channel_task_type_and_risk() {
        let engine = WorkflowEngine::new();

        let general = test_workflow();
        let general_id = general.id;
        engine.register(general).await;

        let incident = Workflow {
            id: WorkflowId::new(),
            name: "incident-workflow".to_string(),
            description: "incident route".to_string(),
            steps: vec![],
            created_at: Utc::now(),
        };
        let incident_id = incident.id;
        engine.register(incident).await;

        engine
            .set_route_rules(vec![
                WorkflowRouteRule {
                    workflow_id: incident_id,
                    user_id: None,
                    channel: Some("feishu".to_string()),
                    task_type: Some("incident".to_string()),
                    risk_policy: WorkflowRiskPolicy::Max(RiskLevel::High),
                    priority: 10,
                },
                WorkflowRouteRule {
                    workflow_id: general_id,
                    user_id: None,
                    channel: None,
                    task_type: None,
                    risk_policy: WorkflowRiskPolicy::Any,
                    priority: 1,
                },
            ])
            .await;

        let request = WorkflowRouteRequest {
            user_id: "u-1".to_string(),
            channel: "feishu".to_string(),
            task_type: "incident".to_string(),
            risk_level: RiskLevel::Medium,
        };
        assert_eq!(engine.route_workflow(&request).await, Some(incident_id));

        let critical = WorkflowRouteRequest {
            risk_level: RiskLevel::Critical,
            ..request
        };
        assert_eq!(engine.route_workflow(&critical).await, Some(general_id));
    }

    #[tokio::test]
    async fn test_route_prefers_specific_user_rule() {
        let engine = WorkflowEngine::new();

        let default_workflow = test_workflow();
        let default_id = default_workflow.id;
        engine.register(default_workflow).await;

        let vip_workflow = Workflow {
            id: WorkflowId::new(),
            name: "vip".to_string(),
            description: "vip route".to_string(),
            steps: vec![],
            created_at: Utc::now(),
        };
        let vip_id = vip_workflow.id;
        engine.register(vip_workflow).await;

        engine
            .set_route_rules(vec![
                WorkflowRouteRule {
                    workflow_id: default_id,
                    user_id: None,
                    channel: Some("telegram".to_string()),
                    task_type: Some("support".to_string()),
                    risk_policy: WorkflowRiskPolicy::Any,
                    priority: 5,
                },
                WorkflowRouteRule {
                    workflow_id: vip_id,
                    user_id: Some("vip-user".to_string()),
                    channel: Some("telegram".to_string()),
                    task_type: Some("support".to_string()),
                    risk_policy: WorkflowRiskPolicy::AllowList(vec![
                        RiskLevel::Low,
                        RiskLevel::Medium,
                        RiskLevel::High,
                    ]),
                    priority: 5,
                },
            ])
            .await;

        let vip_request = WorkflowRouteRequest {
            user_id: "vip-user".to_string(),
            channel: "telegram".to_string(),
            task_type: "support".to_string(),
            risk_level: RiskLevel::High,
        };
        assert_eq!(engine.route_workflow(&vip_request).await, Some(vip_id));

        let normal_request = WorkflowRouteRequest {
            user_id: "regular-user".to_string(),
            channel: "telegram".to_string(),
            task_type: "support".to_string(),
            risk_level: RiskLevel::High,
        };
        assert_eq!(
            engine.route_workflow(&normal_request).await,
            Some(default_id)
        );
    }

    #[test]
    fn test_route_rule_score_saturates_without_overflow() {
        let rule = WorkflowRouteRule {
            workflow_id: WorkflowId::new(),
            user_id: Some("u-1".to_string()),
            channel: Some("feishu".to_string()),
            task_type: Some("incident".to_string()),
            risk_policy: WorkflowRiskPolicy::Max(RiskLevel::High),
            priority: i32::MAX,
        };
        assert_eq!(rule.score(), i32::MAX);
    }

    #[tokio::test]
    async fn test_route_workflow_for_primary_path_enforces_rollout_state() {
        let engine = WorkflowEngine::new();
        let workflow = test_workflow();
        let workflow_id = workflow.id;
        engine.register(workflow).await;

        engine
            .set_route_rules(vec![WorkflowRouteRule {
                workflow_id,
                user_id: None,
                channel: Some("feishu".to_string()),
                task_type: Some("incident".to_string()),
                risk_policy: WorkflowRiskPolicy::Any,
                priority: 1,
            }])
            .await;

        let request = WorkflowRouteRequest {
            user_id: "u-1".to_string(),
            channel: "feishu".to_string(),
            task_type: "incident".to_string(),
            risk_level: RiskLevel::Low,
        };

        // Default rollout state is production, so routed OpenFang traffic is blocked.
        assert_eq!(
            engine
                .route_workflow_for_primary_path(&request, WorkflowTrafficPath::Openfang)
                .await,
            None
        );

        engine
            .update_rollout_state(
                workflow_id,
                Some(WorkflowTrafficPath::Openfang),
                Some(WorkflowTrafficPath::Production),
                Some(true),
                None,
            )
            .await
            .unwrap();

        assert_eq!(
            engine
                .route_workflow_for_primary_path(&request, WorkflowTrafficPath::Openfang)
                .await,
            Some(workflow_id)
        );
    }
}
