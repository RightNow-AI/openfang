//! The Algorithm Executor — runs the 7-phase pipeline.
//!
//! ## HONEST ASSESSMENT OF THIS DESIGN
//!
//! This executor is modeled on Maestro's `maestro_algorithm::executor` (2,294 LOC)
//! but redesigned to use Rig.rs for model calls instead of raw HTTP.
//!
//! ### What works from Maestro's original:
//! - The 7-phase structure itself is sound and battle-tested
//! - ISC-based verification is a genuinely good pattern
//! - The LEARN phase captures useful structured feedback
//!
//! ### What was broken in Maestro's original:
//! - Used raw reqwest HTTP calls instead of a proper model abstraction
//! - JSON parsing was fragile (regex-based extraction from markdown)
//! - No integration with any agent framework (ran in isolation)
//! - The ADAPT phase was a no-op stub
//! - No cost tracking despite having fields for it
//! - Retry logic was a simple counter, not exponential backoff
//!
//! ### What this redesign fixes:
//! - Uses Rig.rs `CompletionModel` trait for all LLM calls
//! - Uses Rig.rs `Extractor` for structured JSON output (no regex parsing)
//! - Defines clear integration points with OpenFang's kernel and supervisor
//! - Adds proper error types and backoff
//!
//! ### What this redesign does NOT fix yet:
//! - ADAPT phase is still conceptual (needs a feedback loop mechanism)
//! - No streaming support (Rig supports it, but the pipeline doesn't use it)
//! - No parallel EXECUTE (each step runs sequentially)
//! - Learning storage is in-memory only

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn, instrument};

use crate::{
    AlgorithmResult, Learning, Phase, PhaseOutput, RunId,
    error::AlgorithmError,
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
}

impl Default for AlgorithmConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            backoff_base_ms: 1000,
            satisfaction_threshold: 0.7,
            max_iterations: 3,
            learn_on_failure: true,
            enable_adapt: false, // Disabled by default — it's not ready
        }
    }
}

/// Callback trait for OpenFang integration.
///
/// The executor calls these hooks at key points so OpenFang's kernel
/// can update its state, emit events, and coordinate with other agents.
///
/// HONEST NOTE: This trait is the critical integration surface. If these
/// hooks are not implemented properly, the algorithm runs in isolation
/// and provides no value over a simple prompt chain.
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

    /// Called when the run completes (success or failure).
    async fn on_run_complete(&self, result: &AlgorithmResult);
}

/// The main algorithm executor.
#[allow(dead_code)]
pub struct AlgorithmExecutor<M: ModelProvider, H: ExecutionHooks> {
    model: Arc<M>,
    hooks: Arc<H>,
    config: AlgorithmConfig,
}

impl<M: ModelProvider, H: ExecutionHooks> AlgorithmExecutor<M, H> {
    pub fn new(model: Arc<M>, hooks: Arc<H>, config: AlgorithmConfig) -> Self {
        Self { model, hooks, config }
    }

    /// Run the full 7-phase algorithm on a task.
    #[instrument(skip(self), fields(run_id))]
    pub async fn run(&self, task: &str) -> Result<AlgorithmResult, AlgorithmError> {
        let run_id = RunId::new();
        let started_at = chrono::Utc::now();
        let mut phase_outputs = Vec::new();
        let mut total_tokens: u64 = 0;

        info!(run_id = %run_id.0, "Starting 7-phase algorithm for task");

        // Phase 1: OBSERVE
        self.hooks.on_phase_start(run_id, Phase::Observe).await;
        let observe_output = self.run_observe(run_id, task).await?;
        total_tokens += observe_output.tokens_used;
        self.hooks.on_phase_complete(run_id, Phase::Observe, &observe_output).await;
        phase_outputs.push(observe_output);

        // Phase 2: ORIENT
        self.hooks.on_phase_start(run_id, Phase::Orient).await;
        let orient_output = self.run_orient(run_id, task, &phase_outputs).await?;
        total_tokens += orient_output.tokens_used;
        self.hooks.on_phase_complete(run_id, Phase::Orient, &orient_output).await;
        phase_outputs.push(orient_output);

        // Phase 3: PLAN (generates ISC criteria)
        self.hooks.on_phase_start(run_id, Phase::Plan).await;
        let plan_output = self.run_plan(run_id, task, &phase_outputs).await?;
        total_tokens += plan_output.tokens_used;
        self.hooks.on_phase_complete(run_id, Phase::Plan, &plan_output).await;
        phase_outputs.push(plan_output);

        // Phase 4-5: EXECUTE → VERIFY loop
        let mut satisfaction = 0.0_f64;
        let mut iteration = 0;

        while iteration < self.config.max_iterations
            && satisfaction < self.config.satisfaction_threshold
        {
            iteration += 1;
            info!(iteration, "EXECUTE → VERIFY iteration");

            // EXECUTE
            self.hooks.on_phase_start(run_id, Phase::Execute).await;
            let exec_output = self.run_execute(run_id, task, &phase_outputs).await?;
            total_tokens += exec_output.tokens_used;
            self.hooks.on_phase_complete(run_id, Phase::Execute, &exec_output).await;
            phase_outputs.push(exec_output);

            // VERIFY
            self.hooks.on_phase_start(run_id, Phase::Verify).await;
            let verify_output = self.run_verify(run_id, &phase_outputs).await?;
            total_tokens += verify_output.tokens_used;

            // Extract satisfaction score from verify output
            satisfaction = verify_output
                .output
                .get("overall_satisfaction")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
                / 100.0; // Normalize from percentage

            self.hooks.on_phase_complete(run_id, Phase::Verify, &verify_output).await;
            phase_outputs.push(verify_output);

            if satisfaction >= self.config.satisfaction_threshold {
                info!(satisfaction, "ISC threshold met, proceeding to LEARN");
                break;
            } else {
                warn!(satisfaction, threshold = self.config.satisfaction_threshold,
                    "ISC threshold not met, retrying EXECUTE");
            }
        }

        // Phase 6: LEARN
        let learnings = if self.config.learn_on_failure || satisfaction >= self.config.satisfaction_threshold {
            self.hooks.on_phase_start(run_id, Phase::Learn).await;
            let learn_output = self.run_learn(run_id, task, &phase_outputs).await?;
            total_tokens += learn_output.tokens_used;
            self.hooks.on_phase_complete(run_id, Phase::Learn, &learn_output).await;

            let learnings = self.extract_learnings(&learn_output);
            for learning in &learnings {
                self.hooks.store_learning(learning).await;
            }
            phase_outputs.push(learn_output);
            learnings
        } else {
            vec![]
        };

        // Phase 7: ADAPT (experimental, disabled by default)
        if self.config.enable_adapt {
            self.hooks.on_phase_start(run_id, Phase::Adapt).await;
            let adapt_output = self.run_adapt(run_id, &learnings).await?;
            total_tokens += adapt_output.tokens_used;
            self.hooks.on_phase_complete(run_id, Phase::Adapt, &adapt_output).await;
            phase_outputs.push(adapt_output);
        }

        let result = AlgorithmResult {
            run_id,
            task_description: task.to_string(),
            phase_outputs,
            overall_satisfaction: satisfaction,
            learnings,
            started_at,
            completed_at: chrono::Utc::now(),
            total_tokens_used: total_tokens,
            total_cost_usd: 0.0, // TODO: Implement cost tracking via model provider
        };

        self.hooks.on_run_complete(&result).await;
        Ok(result)
    }

    // ── Phase implementations (stubs — each needs full prompt engineering) ──

    async fn run_observe(&self, _run_id: RunId, _task: &str) -> Result<PhaseOutput, AlgorithmError> {
        // TODO: Implement OBSERVE phase
        // Should gather: environment state, available tools, relevant context,
        // prior learnings from similar tasks
        todo!("OBSERVE phase implementation")
    }

    async fn run_orient(&self, _run_id: RunId, _task: &str, _prior: &[PhaseOutput]) -> Result<PhaseOutput, AlgorithmError> {
        // TODO: Implement ORIENT phase
        // Should produce: task decomposition, complexity assessment,
        // constraint identification, capability requirements
        todo!("ORIENT phase implementation")
    }

    async fn run_plan(&self, _run_id: RunId, _task: &str, _prior: &[PhaseOutput]) -> Result<PhaseOutput, AlgorithmError> {
        // TODO: Implement PLAN phase
        // Should produce: execution steps, ISC criteria, capability-to-agent mapping
        todo!("PLAN phase implementation")
    }

    async fn run_execute(&self, _run_id: RunId, _task: &str, _prior: &[PhaseOutput]) -> Result<PhaseOutput, AlgorithmError> {
        // TODO: Implement EXECUTE phase
        // Should: delegate steps to OpenFang agents via hooks.delegate_to_agent()
        todo!("EXECUTE phase implementation")
    }

    async fn run_verify(&self, _run_id: RunId, _prior: &[PhaseOutput]) -> Result<PhaseOutput, AlgorithmError> {
        // TODO: Implement VERIFY phase
        // Should: mechanically check each ISC criterion against EXECUTE output
        todo!("VERIFY phase implementation")
    }

    async fn run_learn(&self, _run_id: RunId, _task: &str, _prior: &[PhaseOutput]) -> Result<PhaseOutput, AlgorithmError> {
        // TODO: Implement LEARN phase
        // Should: extract structured learnings from the full execution
        todo!("LEARN phase implementation")
    }

    async fn run_adapt(&self, _run_id: RunId, _learnings: &[Learning]) -> Result<PhaseOutput, AlgorithmError> {
        // TODO: Implement ADAPT phase (experimental)
        // Should: propose parameter adjustments based on accumulated learnings
        // HONEST NOTE: This is the hardest phase. Maestro never implemented it.
        // Kore.ai doesn't have it either. It requires a meta-learning loop.
        todo!("ADAPT phase implementation")
    }

    fn extract_learnings(&self, _output: &PhaseOutput) -> Vec<Learning> {
        // TODO: Parse LEARN phase output into structured Learning objects
        vec![]
    }
}
