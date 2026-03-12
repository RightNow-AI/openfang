//! SupervisorEngine — orchestrates multi-agent task execution using the
//! MAESTRO 7-phase algorithm.
//!
//! ## Architecture
//!
//! The SupervisorEngine sits between the kernel's agent infrastructure and the
//! MAESTRO algorithm executor. It implements `ExecutionHooks` so the algorithm
//! can delegate work to real agents, and provides a high-level `orchestrate()`
//! method that the kernel or API can call.
//!
//! ## Dynamic Scaling
//!
//! - Complexity ≤ threshold_sequential → Single agent, no orchestration overhead
//! - Complexity > threshold_sequential → Full MAESTRO pipeline with agent delegation
//! - Complexity > threshold_parallel → Future: parallel agent execution
//!
//! ## Integration Points
//!
//! - Uses `KernelHandle` for agent spawn/send/kill
//! - Uses `EventBus` for phase lifecycle events
//! - Uses `MemorySubstrate` (via kernel handle) for learning persistence
//! - Exposes status via `SupervisorStatus` for the API dashboard

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use maestro_algorithm::{
    AlgorithmResult, Learning, Phase, PhaseOutput, RunId,
    error::AlgorithmError,
    executor::{AlgorithmConfig, ExecutionHooks},
};
use openfang_runtime::kernel_handle::KernelHandle;
use openfang_types::agent::AgentId;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

// ── Types ───────────────────────────────────────────────────────────────────

/// Unique identifier for a supervisor orchestration run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrchestrationId(pub Uuid);

impl OrchestrationId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for OrchestrationId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for OrchestrationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The result of a supervisor orchestration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationResult {
    /// Unique run ID.
    pub id: OrchestrationId,
    /// The original task description.
    pub task: String,
    /// Whether orchestration was used (false = single-agent passthrough).
    pub orchestrated: bool,
    /// Complexity score from ORIENT phase (1-10).
    pub complexity: u8,
    /// Number of agents spawned during execution.
    pub agents_spawned: u32,
    /// Final output text.
    pub output: String,
    /// ISC satisfaction score (0.0-1.0).
    pub satisfaction: f64,
    /// Total tokens consumed across all phases.
    pub total_tokens: u64,
    /// Total wall-clock duration in milliseconds.
    pub duration_ms: u64,
    /// Per-phase timing breakdown.
    pub phase_timings: Vec<PhaseTiming>,
    /// Learnings captured during execution.
    pub learnings: Vec<String>,
    /// When the orchestration started.
    pub started_at: DateTime<Utc>,
    /// When the orchestration completed.
    pub completed_at: DateTime<Utc>,
}

/// Timing information for a single phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseTiming {
    pub phase: String,
    pub duration_ms: u64,
    pub tokens_used: u64,
}

/// Current status of the supervisor engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisorStatus {
    /// Whether the supervisor is currently running an orchestration.
    pub active: bool,
    /// Current active orchestration ID (if any).
    pub current_run: Option<OrchestrationId>,
    /// Current phase being executed (if active).
    pub current_phase: Option<String>,
    /// Total orchestrations completed.
    pub total_runs: u64,
    /// Total orchestrations that met ISC threshold.
    pub successful_runs: u64,
    /// Average satisfaction score across all runs.
    pub avg_satisfaction: f64,
    /// Total learnings captured.
    pub total_learnings: u64,
    /// Recent orchestration results (last 10).
    pub recent_runs: Vec<OrchestrationSummary>,
}

/// Summary of a past orchestration for the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationSummary {
    pub id: OrchestrationId,
    pub task: String,
    pub complexity: u8,
    pub satisfaction: f64,
    pub duration_ms: u64,
    pub agents_spawned: u32,
    pub completed_at: DateTime<Utc>,
}

/// Types of tasks for classification.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskType {
    /// Software Engineering Task
    SWE,
    /// Regular MAESTRO orchestration task
    General,
}

// ── SupervisorEngine ────────────────────────────────────────────────────────

/// The main supervisor engine that orchestrates multi-agent tasks.
pub struct SupervisorEngine {
    /// Algorithm configuration.
    config: RwLock<AlgorithmConfig>,
    /// Kernel handle for agent operations.
    kernel: Arc<dyn KernelHandle>,
    /// Active orchestration tracking.
    active_run: RwLock<Option<ActiveRun>>,
    /// Completed orchestration history.
    history: RwLock<Vec<OrchestrationResult>>,
    /// Agents spawned during the current run (for cleanup).
    spawned_agents: DashMap<OrchestrationId, Vec<AgentId>>,
    /// Accumulated learnings across all runs.
    learnings: RwLock<Vec<Learning>>,
    /// Statistics counters.
    stats: RwLock<SupervisorStats>,
}

/// Internal tracking for an active orchestration.
#[allow(dead_code)]
struct ActiveRun {
    id: OrchestrationId,
    task: String,
    current_phase: Phase,
    started_at: DateTime<Utc>,
}

/// Internal statistics.
#[derive(Default)]
struct SupervisorStats {
    total_runs: u64,
    successful_runs: u64,
    total_satisfaction: f64,
    total_learnings: u64,
}

impl SupervisorEngine {
    /// Create a new supervisor engine.
    pub fn new(kernel: Arc<dyn KernelHandle>, config: AlgorithmConfig) -> Self {
        Self {
            config: RwLock::new(config),
            kernel,
            active_run: RwLock::new(None),
            history: RwLock::new(Vec::new()),
            spawned_agents: DashMap::new(),
            learnings: RwLock::new(Vec::new()),
            stats: RwLock::new(SupervisorStats::default()),
        }
    }

    /// Get the current status of the supervisor.
    pub async fn status(&self) -> SupervisorStatus {
        let active = self.active_run.read().await;
        let stats = self.stats.read().await;
        let history = self.history.read().await;

        let recent_runs: Vec<OrchestrationSummary> = history
            .iter()
            .rev()
            .take(10)
            .map(|r| OrchestrationSummary {
                id: r.id,
                task: r.task.chars().take(100).collect(),
                complexity: r.complexity,
                satisfaction: r.satisfaction,
                duration_ms: r.duration_ms,
                agents_spawned: r.agents_spawned,
                completed_at: r.completed_at,
            })
            .collect();

        SupervisorStatus {
            active: active.is_some(),
            current_run: active.as_ref().map(|r| r.id),
            current_phase: active.as_ref().map(|r| r.current_phase.to_string()),
            total_runs: stats.total_runs,
            successful_runs: stats.successful_runs,
            avg_satisfaction: if stats.total_runs > 0 {
                stats.total_satisfaction / stats.total_runs as f64
            } else {
                0.0
            },
            total_learnings: stats.total_learnings,
            recent_runs,
        }
    }

    /// Get the algorithm configuration (for API exposure).
    pub async fn algorithm_config(&self) -> AlgorithmConfig {
        self.config.read().await.clone()
    }

    /// Update the algorithm configuration.
    pub async fn update_config(&self, config: AlgorithmConfig) {
        *self.config.write().await = config;
    }

    /// Orchestrate a task. This is the main entry point.
    ///
    /// The supervisor decides whether to use full MAESTRO orchestration
    /// or pass through to a single agent based on complexity assessment.
    pub async fn orchestrate(
        &self,
        task: &str,
        capabilities: &[String],
    ) -> Result<OrchestrationResult, AlgorithmError> {
        let orch_id = OrchestrationId::new();
        let started_at = Utc::now();
        let start = std::time::Instant::now();

        info!(
            orchestration_id = %orch_id,
            task_preview = %task.chars().take(80).collect::<String>(),
            "Starting supervisor orchestration"
        );

        // Set active run
        {
            let mut active = self.active_run.write().await;
            *active = Some(ActiveRun {
                id: orch_id,
                task: task.to_string(),
                current_phase: Phase::Observe,
                started_at,
            });
        }

        // Initialize spawned agents tracking
        self.spawned_agents.insert(orch_id, Vec::new());

        // Check if this is a SWE (Software Engineering) task - if so, delegate immediately
        if let TaskType::SWE = self.classify_task(task, capabilities).await {
            info!(orchestration_id = %orch_id, "Task classified as SWE - delegating to SWE agent");
            // Cleanup - clear the temporary active run for SWE task
            {
                let mut active = self.active_run.write().await;
                *active = None;
            }
            // Clean up spawn tracking
            self.spawned_agents.remove(&orch_id);
            
            return self.delegate_swe_task(task, task.to_string()).await;
        }

        // Create the hooks bridge
        let hooks = SupervisorHooks {
            kernel: Arc::clone(&self.kernel),
            orchestration_id: orch_id,
            active_run: &self.active_run,
            spawned_agents: &self.spawned_agents,
        };

        // Run the full algorithm pipeline
        // Note: We use the hooks directly rather than constructing an AlgorithmExecutor
        // because the executor requires ownership of the model provider, which is
        // managed by the kernel. Instead, we delegate model calls through the kernel.
        let result = self.run_algorithm(task, capabilities, &hooks).await;

        // Clear active run
        {
            let mut active = self.active_run.write().await;
            *active = None;
        }

        // Build the orchestration result
        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(algo_result) => {
                let orch_result = self
                    .build_orchestration_result(orch_id, task, algo_result, duration_ms, started_at)
                    .await;

                // Update stats
                {
                    let mut stats = self.stats.write().await;
                    stats.total_runs += 1;
                    stats.total_satisfaction += orch_result.satisfaction;
                    if orch_result.satisfaction >= self.config.read().await.satisfaction_threshold {
                        stats.successful_runs += 1;
                    }
                }

                // Store in history (keep last 100)
                {
                    let mut history = self.history.write().await;
                    history.push(orch_result.clone());
                    if history.len() > 100 {
                        history.remove(0);
                    }
                }

                // Accumulate learnings into in-memory store and persist to kernel memory
                self.persist_learnings(&orch_result).await;

                // Run feedback loop: adjust config based on historical performance
                self.feedback_loop().await;

                // Cleanup spawned agents (optional — they may be reusable)
                self.cleanup_spawned_agents(orch_id).await;

                info!(
                    orchestration_id = %orch_id,
                    satisfaction = format!("{:.1}%", orch_result.satisfaction * 100.0),
                    duration_ms,
                    agents_spawned = orch_result.agents_spawned,
                    "Orchestration complete"
                );

                Ok(orch_result)
            }
            Err(e) => {
                warn!(
                    orchestration_id = %orch_id,
                    error = %e,
                    "Orchestration failed"
                );

                // Cleanup on failure
                self.cleanup_spawned_agents(orch_id).await;

                // Update stats
                {
                    let mut stats = self.stats.write().await;
                    stats.total_runs += 1;
                }

                Err(e)
            }
        }
    }

    /// Run the MAESTRO algorithm using the kernel for model calls.
    ///
    /// This method implements the algorithm pipeline inline rather than
    /// using `AlgorithmExecutor` because the model provider is the kernel
    /// itself (which routes to the appropriate LLM based on agent config).
    async fn run_algorithm(
        &self,
        task: &str,
        capabilities: &[String],
        hooks: &SupervisorHooks<'_>,
    ) -> Result<AlgorithmResult, AlgorithmError> {
        let run_id = RunId::new();
        let started_at = Utc::now();
        let config = self.config.read().await.clone();

        // Retrieve prior learnings
        let prior_learnings = hooks.retrieve_learnings(task).await;

        // Quick complexity check: send a simple prompt to assess if we need
        // full orchestration or can just delegate to a single agent
        let complexity = self
            .assess_complexity(task, capabilities, &prior_learnings)
            .await;

        if complexity <= config.complexity_threshold_sequential {
            info!(
                complexity,
                threshold = config.complexity_threshold_sequential,
                "Low complexity — using single-agent passthrough"
            );
            return self.single_agent_passthrough(run_id, task, capabilities).await;
        }

        info!(
            complexity,
            "Complexity above threshold — running full MAESTRO pipeline"
        );

        // For the full pipeline, we delegate to the kernel's send_to_agent
        // with a specially-crafted supervisor agent that runs each phase.
        // The phases module handles the actual LLM calls.

        // Build a simplified result from the delegation
        let output = hooks
            .delegate_to_agent(task, capabilities)
            .await
            .map_err(|e| AlgorithmError::DelegationError(e.to_string()))?;

        Ok(AlgorithmResult {
            run_id,
            task_description: task.to_string(),
            phase_outputs: vec![PhaseOutput {
                phase: Phase::Execute,
                output: serde_json::json!({ "result": output }),
                tokens_used: 0,
                duration_ms: 0,
                model_used: "kernel-delegated".to_string(),
            }],
            overall_satisfaction: 0.8, // Default for delegated execution
            learnings: vec![],
            started_at,
            completed_at: Utc::now(),
            total_tokens_used: 0,
            total_cost_usd: 0.0,
        })
    }

    /// Assess task complexity without running the full OBSERVE/ORIENT phases.
    /// Returns a score from 1-10.
    async fn assess_complexity(
        &self,
        task: &str,
        capabilities: &[String],
        _prior_learnings: &[String],
    ) -> u8 {
        // Heuristic-based complexity assessment (no LLM call needed):
        // - Word count
        // - Number of capabilities requested
        // - Presence of multi-step keywords
        let word_count = task.split_whitespace().count();
        let cap_count = capabilities.len();

        let multi_step_keywords = [
            "then", "after", "next", "finally", "first", "second",
            "step", "phase", "pipeline", "workflow", "coordinate",
            "multiple", "several", "each", "all", "every",
            "analyze", "compare", "synthesize", "research",
        ];
        let keyword_hits = multi_step_keywords
            .iter()
            .filter(|kw| task.to_lowercase().contains(**kw))
            .count();

        let mut score: u8 = 1;

        // Word count contribution (0-3 points)
        score += match word_count {
            0..=20 => 0,
            21..=50 => 1,
            51..=100 => 2,
            _ => 3,
        };

        // Capability count contribution (0-2 points)
        score += match cap_count {
            0..=1 => 0,
            2..=3 => 1,
            _ => 2,
        };

        // Keyword hits contribution (0-3 points)
        score += match keyword_hits {
            0 => 0,
            1..=2 => 1,
            3..=4 => 2,
            _ => 3,
        };

        // Cap at 10
        score.min(10)
    }

    /// For low-complexity tasks, skip orchestration and delegate directly.
    async fn single_agent_passthrough(
        &self,
        run_id: RunId,
        task: &str,
        capabilities: &[String],
    ) -> Result<AlgorithmResult, AlgorithmError> {
        let started_at = Utc::now();

        // Find an existing agent that matches the capabilities, or use default
        let agents = self.kernel.list_agents();
        let matching_agent = agents.iter().find(|a| {
            a.state == "Running"
                && capabilities.iter().all(|cap| {
                    a.tools.iter().any(|t| t.contains(cap))
                        || a.tags.iter().any(|t| t.contains(cap))
                })
        });

        let output = if let Some(agent) = matching_agent {
            info!(
                agent_name = %agent.name,
                "Passthrough: delegating to existing agent"
            );
            self.kernel
                .send_to_agent(&agent.id, task)
                .await
                .map_err(AlgorithmError::DelegationError)?
        } else {
            // Spawn a temporary agent for this task
            info!("Passthrough: spawning temporary agent");
            let manifest = build_worker_manifest("supervisor-worker", capabilities);
            let (agent_id, _name) = self
                .kernel
                .spawn_agent(&manifest, None)
                .await
                .map_err(AlgorithmError::DelegationError)?;

            let result = self
                .kernel
                .send_to_agent(&agent_id, task)
                .await
                .map_err(AlgorithmError::DelegationError)?;

            // Kill the temporary agent
            let _ = self.kernel.kill_agent(&agent_id).await;

            result
        };

        Ok(AlgorithmResult {
            run_id,
            task_description: task.to_string(),
            phase_outputs: vec![PhaseOutput {
                phase: Phase::Execute,
                output: serde_json::json!({ "result": output }),
                tokens_used: 0,
                duration_ms: 0,
                model_used: "passthrough".to_string(),
            }],
            overall_satisfaction: 1.0, // Passthrough assumes success
            learnings: vec![],
            started_at,
            completed_at: Utc::now(),
            total_tokens_used: 0,
            total_cost_usd: 0.0,
        })
    }

    /// Build an OrchestrationResult from an AlgorithmResult.
    async fn build_orchestration_result(
        &self,
        orch_id: OrchestrationId,
        task: &str,
        algo_result: AlgorithmResult,
        duration_ms: u64,
        started_at: DateTime<Utc>,
    ) -> OrchestrationResult {
        let phase_timings: Vec<PhaseTiming> = algo_result
            .phase_outputs
            .iter()
            .map(|p| PhaseTiming {
                phase: p.phase.to_string(),
                duration_ms: p.duration_ms,
                tokens_used: p.tokens_used,
            })
            .collect();

        let agents_spawned = self
            .spawned_agents
            .get(&orch_id)
            .map(|v| v.len() as u32)
            .unwrap_or(0);

        // Extract complexity from ORIENT phase output
        let complexity = algo_result
            .phase_outputs
            .iter()
            .find(|p| p.phase == Phase::Orient)
            .and_then(|p| p.output.get("complexity"))
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as u8;

        // Extract final output from the last EXECUTE phase
        let output = algo_result
            .phase_outputs
            .iter()
            .rev()
            .find(|p| p.phase == Phase::Execute)
            .and_then(|p| p.output.get("result"))
            .and_then(|v| v.as_str())
            .unwrap_or("Orchestration completed")
            .to_string();

        let learnings: Vec<String> = algo_result
            .learnings
            .iter()
            .map(|l| l.insight.clone())
            .collect();

        OrchestrationResult {
            id: orch_id,
            task: task.to_string(),
            orchestrated: complexity > self.config.read().await.complexity_threshold_sequential,
            complexity,
            agents_spawned,
            output,
            satisfaction: algo_result.overall_satisfaction,
            total_tokens: algo_result.total_tokens_used,
            duration_ms,
            phase_timings,
            learnings,
            started_at,
            completed_at: Utc::now(),
        }
    }

    /// Persist learnings from an orchestration result into both in-memory
    /// storage and the kernel's memory system for cross-session retrieval.
    async fn persist_learnings(&self, result: &OrchestrationResult) {
        if result.learnings.is_empty() {
            return;
        }

        // Update stats counter
        {
            let mut stats = self.stats.write().await;
            stats.total_learnings += result.learnings.len() as u64;
        }
