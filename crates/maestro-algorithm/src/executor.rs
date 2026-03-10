//! The Algorithm Executor — runs the 7-phase pipeline.
//!
//! ## Design
//!
//! This executor orchestrates the full OBSERVE → ORIENT → PLAN → EXECUTE →
//! VERIFY → LEARN → ADAPT pipeline. Each phase is implemented in the `phases`
//! module; this module handles sequencing, the EXECUTE → VERIFY retry loop,
//! and integration with OpenFang via `ExecutionHooks`.
//!
//! ## Integration with OpenFang
//!
//! - `ModelProvider` abstracts over Rig.rs `CompletionModel` / `Agent`
//! - `ExecutionHooks` bridges to the kernel's agent infrastructure
//! - The executor is generic over both traits for testability

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn, instrument};

use crate::{
    AlgorithmResult, Learning, LearningCategory, Phase, PhaseOutput, RunId,
    error::AlgorithmError,
    phases,
    types::{LearnOutput, AdaptOutput},
};

/// Trait for the model provider — abstracts over Rig.rs CompletionModel.
///
/// This exists so the executor can be tested with mock models.
/// In production, this wraps a `rig::agent::Agent`.
#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// Send a prompt and get a text response.
    async fn complete(&self, prompt: &str, system: &str) -> Result<String, AlgorithmError>;

    /// Send a prompt and extract structured JSON output.
    async fn extract<T: for<'de> Deserialize<'de> + Send>(
        &self,
        prompt: &str,
        system: &str,
    ) -> Result<T, AlgorithmError>;

    /// Get the model identifier being used.
    fn model_id(&self) -> &str;
}

/// Configuration for the algorithm executor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlgorithmConfig {
    /// Maximum retry attempts per phase.
    pub max_retries: u32,
    /// Base delay for exponential backoff (milliseconds).
    pub backoff_base_ms: u64,
    /// ISC satisfaction threshold to consider the task complete (0.0 - 1.0).
    pub satisfaction_threshold: f64,
    /// Maximum total iterations (EXECUTE → VERIFY → retry loop).
    pub max_iterations: u32,
    /// Whether to run the LEARN phase even on failure.
    pub learn_on_failure: bool,
    /// Whether to run the ADAPT phase (experimental).
    pub enable_adapt: bool,
    /// Default timeout for execution steps (seconds).
    pub default_timeout_seconds: u64,
    /// Complexity threshold below which orchestration is skipped (single agent).
    pub complexity_threshold_sequential: u8,
    /// Complexity threshold above which parallel orchestration is used.
    pub complexity_threshold_parallel: u8,
}

impl Default for AlgorithmConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            backoff_base_ms: 1000,
            satisfaction_threshold: 0.7,
            max_iterations: 3,
            learn_on_failure: true,
            enable_adapt: false,
            default_timeout_seconds: 120,
            complexity_threshold_sequential: 3,
            complexity_threshold_parallel: 7,
        }
    }
}

/// Callback trait for OpenFang integration.
///
/// The executor calls these hooks at key points so OpenFang's kernel
/// can update its state, emit events, and coordinate with other agents.
#[async_trait]
pub trait ExecutionHooks: Send + Sync {
    /// Called when a phase begins. OpenFang should update the task board.
    async fn on_phase_start(&self, run_id: RunId, phase: Phase);

    /// Called when a phase completes. OpenFang should store the output.
    async fn on_phase_complete(&self, run_id: RunId, phase: Phase, output: &PhaseOutput);

    /// Called when the EXECUTE phase needs to delegate work.
    /// OpenFang should dispatch this to an agent via the supervisor.
    ///
    /// Returns the agent's output as a string.
    async fn delegate_to_agent(
        &self,
        step_description: &str,
        capabilities: &[String],
    ) -> Result<String, AlgorithmError>;

    /// Called when a learning is captured. OpenFang should persist this
    /// to its memory/knowledge system.
    async fn store_learning(&self, learning: &Learning);

    /// Retrieve past learnings relevant to a task description.
    /// OpenFang should query its memory system for similar tasks.
    async fn retrieve_learnings(&self, task: &str) -> Vec<String>;

    /// Called when the run completes (success or failure).
    async fn on_run_complete(&self, result: &AlgorithmResult);
}

/// The main algorithm executor.
pub struct AlgorithmExecutor<M: ModelProvider, H: ExecutionHooks> {
    model: Arc<M>,
    hooks: Arc<H>,
    config: AlgorithmConfig,
}

impl<M: ModelProvider, H: ExecutionHooks> AlgorithmExecutor<M, H> {
    pub fn new(model: Arc<M>, hooks: Arc<H>, config: AlgorithmConfig) -> Self {
        Self { model, hooks, config }
    }

    /// Get the current configuration (for ADAPT phase to serialize).
    pub fn config(&self) -> &AlgorithmConfig {
        &self.config
    }

    /// Apply parameter adjustments from the ADAPT phase.
    pub fn apply_adaptations(&mut self, adapt_output: &serde_json::Value) {
        if let Ok(adapt) = serde_json::from_value::<AdaptOutput>(adapt_output.clone()) {
            for adj in &adapt.adjustments {
                match adj.parameter.as_str() {
                    "satisfaction_threshold" => {
                        if let Ok(v) = adj.proposed_value.parse::<f64>() {
                            if (0.1..=1.0).contains(&v) {
                                info!(
                                    old = self.config.satisfaction_threshold,
                                    new = v,
                                    "ADAPT: Updating satisfaction_threshold"
                                );
                                self.config.satisfaction_threshold = v;
                            }
                        }
                    }
                    "max_iterations" => {
                        if let Ok(v) = adj.proposed_value.parse::<u32>() {
                            if (1..=10).contains(&v) {
                                info!(old = self.config.max_iterations, new = v, "ADAPT: Updating max_iterations");
                                self.config.max_iterations = v;
                            }
                        }
                    }
                    "max_retries" => {
                        if let Ok(v) = adj.proposed_value.parse::<u32>() {
                            if (1..=10).contains(&v) {
                                info!(old = self.config.max_retries, new = v, "ADAPT: Updating max_retries");
                                self.config.max_retries = v;
                            }
                        }
                    }
                    "backoff_base_ms" => {
                        if let Ok(v) = adj.proposed_value.parse::<u64>() {
                            if (100..=30000).contains(&v) {
                                info!(old = self.config.backoff_base_ms, new = v, "ADAPT: Updating backoff_base_ms");
                                self.config.backoff_base_ms = v;
                            }
                        }
                    }
                    "default_timeout_seconds" => {
                        if let Ok(v) = adj.proposed_value.parse::<u64>() {
                            if (10..=600).contains(&v) {
                                info!(old = self.config.default_timeout_seconds, new = v, "ADAPT: Updating default_timeout_seconds");
                                self.config.default_timeout_seconds = v;
                            }
                        }
                    }
                    other => {
                        warn!(parameter = other, "ADAPT: Unknown parameter, skipping");
                    }
                }
            }
        }
    }

    /// Run the full 7-phase algorithm on a task.
    #[instrument(skip(self), fields(run_id))]
    pub async fn run(&self, task: &str) -> Result<AlgorithmResult, AlgorithmError> {
        self.run_with_capabilities(task, &[]).await
    }

    /// Run the full 7-phase algorithm with explicit capability list.
    #[instrument(skip(self, capabilities), fields(run_id))]
    pub async fn run_with_capabilities(
        &self,
        task: &str,
        capabilities: &[String],
    ) -> Result<AlgorithmResult, AlgorithmError> {
        let run_id = RunId::new();
        let started_at = Utc::now();
        let mut phase_outputs = Vec::new();
        let mut total_tokens: u64 = 0;

        info!(run_id = %run_id.0, "Starting 7-phase algorithm for task");

        // Retrieve past learnings for this task
        let prior_learnings = self.hooks.retrieve_learnings(task).await;

        // ── Phase 1: OBSERVE ────────────────────────────────────────────
        self.hooks.on_phase_start(run_id, Phase::Observe).await;
        let observe_output = phases::run_observe(
            self.model.as_ref(),
            run_id,
            task,
            capabilities,
            &prior_learnings,
        )
        .await?;
        total_tokens += observe_output.tokens_used;
        self.hooks
            .on_phase_complete(run_id, Phase::Observe, &observe_output)
            .await;
        phase_outputs.push(observe_output);

        // ── Phase 2: ORIENT ─────────────────────────────────────────────
        self.hooks.on_phase_start(run_id, Phase::Orient).await;
        let observe_data = phase_outputs[0].output.clone();
        let orient_output =
            phases::run_orient(self.model.as_ref(), run_id, task, &observe_data).await?;
        total_tokens += orient_output.tokens_used;
        self.hooks
            .on_phase_complete(run_id, Phase::Orient, &orient_output)
            .await;

        // Check complexity for dynamic scaling decision
        let complexity = orient_output
            .output
            .get("complexity")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as u8;

        info!(
            complexity,
            threshold_seq = self.config.complexity_threshold_sequential,
            threshold_par = self.config.complexity_threshold_parallel,
            "Complexity assessment complete"
        );

        let orient_data = orient_output.output.clone();
        phase_outputs.push(orient_output);

        // ── Phase 3: PLAN ───────────────────────────────────────────────
        self.hooks.on_phase_start(run_id, Phase::Plan).await;
        let plan_output = phases::run_plan(
            self.model.as_ref(),
            run_id,
            task,
            &observe_data,
            &orient_data,
        )
        .await?;
        total_tokens += plan_output.tokens_used;
        self.hooks
            .on_phase_complete(run_id, Phase::Plan, &plan_output)
            .await;
        phase_outputs.push(plan_output);

        // ── Phase 4-5: EXECUTE → VERIFY loop ────────────────────────────
        let mut satisfaction = 0.0_f64;
        let mut iteration = 0;
        let plan_data = &phase_outputs[2].output.clone();

        while iteration < self.config.max_iterations
            && satisfaction < self.config.satisfaction_threshold
        {
            iteration += 1;
            info!(iteration, max = self.config.max_iterations, "EXECUTE → VERIFY iteration");

            // EXECUTE
            self.hooks.on_phase_start(run_id, Phase::Execute).await;
            let exec_output = phases::run_execute(
                self.model.as_ref(),
                self.hooks.as_ref(),
                run_id,
                task,
                plan_data,
            )
            .await?;
            total_tokens += exec_output.tokens_used;
            self.hooks
                .on_phase_complete(run_id, Phase::Execute, &exec_output)
                .await;
            phase_outputs.push(exec_output);

            // VERIFY
            self.hooks.on_phase_start(run_id, Phase::Verify).await;
            let exec_data = &phase_outputs.last().unwrap().output;
            let verify_output = phases::run_verify(
                self.model.as_ref(),
                run_id,
                plan_data,
                exec_data,
                self.config.satisfaction_threshold,
            )
            .await?;
            total_tokens += verify_output.tokens_used;

            // Extract satisfaction score from verify output
            satisfaction = verify_output
                .output
                .get("overall_satisfaction")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
                / 100.0; // Normalize from percentage to 0.0-1.0

            self.hooks
                .on_phase_complete(run_id, Phase::Verify, &verify_output)
                .await;
            phase_outputs.push(verify_output);

            if satisfaction >= self.config.satisfaction_threshold {
                info!(satisfaction = format!("{:.1}%", satisfaction * 100.0), "ISC threshold met");
                break;
            } else {
                warn!(
                    satisfaction = format!("{:.1}%", satisfaction * 100.0),
                    threshold = format!("{:.1}%", self.config.satisfaction_threshold * 100.0),
                    "ISC threshold not met, retrying EXECUTE"
                );

                // Exponential backoff before retry
                if iteration < self.config.max_iterations {
                    let delay = self.config.backoff_base_ms * 2u64.pow(iteration - 1);
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                }
            }
        }

        // ── Phase 6: LEARN ──────────────────────────────────────────────
        let learnings = if self.config.learn_on_failure
            || satisfaction >= self.config.satisfaction_threshold
        {
            self.hooks.on_phase_start(run_id, Phase::Learn).await;
            let learn_output = phases::run_learn(
                self.model.as_ref(),
                run_id,
                task,
                satisfaction,
                &phase_outputs,
            )
            .await?;
            total_tokens += learn_output.tokens_used;
            self.hooks
                .on_phase_complete(run_id, Phase::Learn, &learn_output)
                .await;

            let learnings = extract_learnings(&learn_output);
            for learning in &learnings {
                self.hooks.store_learning(learning).await;
            }
            phase_outputs.push(learn_output);
            learnings
        } else {
            vec![]
        };

        // ── Phase 7: ADAPT (experimental) ───────────────────────────────
        if self.config.enable_adapt {
            self.hooks.on_phase_start(run_id, Phase::Adapt).await;

            let learn_data = phase_outputs
                .iter()
                .rev()
                .find(|p| p.phase == Phase::Learn)
                .map(|p| &p.output)
                .unwrap_or(&serde_json::Value::Null);

            let config_json = serde_json::to_value(&self.config).unwrap_or_default();
            let past_learning_strings: Vec<String> = prior_learnings;

            let adapt_output = phases::run_adapt(
                self.model.as_ref(),
                run_id,
                learn_data,
                &config_json,
                &past_learning_strings,
            )
            .await?;
            total_tokens += adapt_output.tokens_used;
            self.hooks
                .on_phase_complete(run_id, Phase::Adapt, &adapt_output)
                .await;
            phase_outputs.push(adapt_output);

            // Note: apply_adaptations requires &mut self, so the caller
            // should call it after run() returns if they want to apply changes.
        }

        let result = AlgorithmResult {
            run_id,
            task_description: task.to_string(),
            phase_outputs,
            overall_satisfaction: satisfaction,
            learnings,
            started_at,
            completed_at: Utc::now(),
            total_tokens_used: total_tokens,
            total_cost_usd: estimate_cost(self.model.model_id(), total_tokens),
        };

        self.hooks.on_run_complete(&result).await;
        Ok(result)
    }
}

/// Estimate the USD cost for a completed algorithm run.
///
/// Uses the same pricing table as `openfang-kernel::metering::estimate_cost_rates`
/// but is self-contained so that `maestro-algorithm` does not need to depend on
/// the kernel crates. The formula is:
///
/// ```text
/// cost = (tokens / 1_000_000) * blended_rate
/// ```
///
/// Because `PhaseOutput::tokens_used` is a combined total (not split into input
/// vs. output), we use a conservative blended rate that weights input tokens
/// at 70% and output tokens at 30% of the model's published per-million rates.
/// This is a reasonable approximation for multi-turn reasoning workloads where
/// prompts are typically longer than completions.
fn estimate_cost(model_id: &str, total_tokens: u64) -> f64 {
    let model = model_id.to_lowercase();
    // Returns (input_per_m, output_per_m) for the model.
    // Ordered from most-specific to least-specific to avoid prefix collisions.
    let (input_per_m, output_per_m) = if model.contains("haiku") {
        (0.25, 1.25)
    } else if model.contains("opus") {
        (15.0, 75.0)
    } else if model.contains("sonnet") {
        (3.0, 15.0)
    } else if model.contains("gpt-4o-mini") {
        (0.15, 0.60)
    } else if model.contains("gpt-4o") {
        (2.50, 10.0)
    } else if model.contains("gpt-4.1-nano") {
        (0.10, 0.40)
    } else if model.contains("gpt-4.1-mini") {
        (0.40, 1.60)
    } else if model.contains("gpt-4.1") {
        (2.00, 8.00)
    } else if model.contains("o4-mini") || model.contains("o3-mini") {
        (1.10, 4.40)
    } else if model.contains("o3") {
        (2.00, 8.00)
    } else if model.contains("gpt-4") {
        (2.50, 10.0)
    } else if model.contains("gemini-2.5-pro") {
        (1.25, 10.0)
    } else if model.contains("gemini-2.5-flash") {
        (0.15, 0.60)
    } else if model.contains("gemini-2.0-flash") || model.contains("gemini-flash") {
        (0.10, 0.40)
    } else if model.contains("gemini") {
        (0.15, 0.60)
    } else if model.contains("deepseek-r1") || model.contains("deepseek-reasoner") {
        (0.55, 2.19)
    } else if model.contains("deepseek") {
        (0.27, 1.10)
    } else if model.contains("llama-4-maverick") {
        (0.50, 0.77)
    } else if model.contains("llama-4-scout") {
        (0.11, 0.34)
    } else if model.contains("llama") || model.contains("mixtral") {
        (0.05, 0.10)
    } else if model.contains("mistral-large") {
        (2.00, 6.00)
    } else if model.contains("mistral") {
        (0.10, 0.30)
    } else if model.contains("grok-3-mini") || model.contains("grok-mini") {
        (0.30, 0.50)
    } else if model.contains("grok-3") || model.contains("grok-4") {
        (3.0, 15.0)
    } else if model.contains("grok") {
        (2.0, 10.0)
    } else {
        // Conservative default: $1/$3 per million tokens
        (1.0, 3.0)
    };
    // Blended rate: 70% input weight, 30% output weight
    let blended_per_m = input_per_m * 0.70 + output_per_m * 0.30;
    (total_tokens as f64 / 1_000_000.0) * blended_per_m
}

/// Test shim: exposes `estimate_cost` for unit tests in `tests.rs`.
#[cfg(test)]
pub(crate) fn estimate_cost_for_test(model_id: &str, total_tokens: u64) -> f64 {
    estimate_cost(model_id, total_tokens)
}

/// Extract structured `Learning` objects from a LEARN phase output.
fn extract_learnings(learn_phase: &PhaseOutput) -> Vec<Learning> {
    let Ok(learn_output) = serde_json::from_value::<LearnOutput>(learn_phase.output.clone())
    else {
        warn!("Failed to parse LEARN phase output into LearnOutput");
        return vec![];
    };

    learn_output
        .learnings
        .into_iter()
        .map(|entry| Learning {
            category: match entry.category {
                crate::types::LearningCategory::System => LearningCategory::System,
                crate::types::LearningCategory::Algorithm => LearningCategory::Algorithm,
                crate::types::LearningCategory::Failure => LearningCategory::Failure,
                crate::types::LearningCategory::Synthesis => LearningCategory::Synthesis,
                crate::types::LearningCategory::Reflection => LearningCategory::Reflection,
            },
            insight: entry.insight,
            context: entry.context,
            actionable: entry.actionable,
            timestamp: Utc::now(),
        })
        .collect()
}
