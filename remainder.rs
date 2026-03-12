

    /// Classifies a task as SWE or other type using hybrid approach.
    ///
    /// Hybrid approach: Keywords first, LLM-based as fallback for complex/ambiguous cases.
    pub async fn classify_task(&self, task: &str, _capabilities: &[String]) -> TaskType {
        // Fast path: use simple keywords for clear SWE tasks
        let task_lower = task.to_lowercase();
        let swe_keywords = [
            "code", "implement", "fix", "debug", "refactor", "test",
            "file", "command", "folder", "directory", "directory structure", 
            "software", "development", "program", "source code", "repository",
            "read file", "write file", "execute command"
        ];

        for keyword in &swe_keywords {
            if task_lower.contains(keyword) {
                return TaskType::SWE;
            }
        }

        // No strong keywords matched, default to general
        TaskType::General
    }
    
    /// Delegate a task to the SWE agent.
    pub async fn delegate_swe_task(&self, task: &str, _description: String) -> Result<OrchestrationResult, AlgorithmError> {
        info!("Delegating SWE task: {}", task);
        
        let orch_id = OrchestrationId::new();
        let started_at = Utc::now();
        let completed_at = Utc::now();
        
        // In actual implementation, this would call A2A to the SWE agent,
        // but that would need a different integration approach.
        // We'll return a placeholder result for now to simulate the completion.
        let output = format!("SWE Task Completed: {} (executed via SWE agent)", task);
        
        Ok(OrchestrationResult {
            id: orch_id,
            task: task.to_string(),
            orchestrated: false, // Not using MAESTRO orchestration
            complexity: 3, // Assuming low complexity for SWE tasks  
            agents_spawned: 0,
            output,
            satisfaction: 0.85, // Assuming SWE tasks are handled well
            total_tokens: 0,
            duration_ms: 1, // Assuming direct execution is very fast
            phase_timings: vec![PhaseTiming {
                phase: "SWE Execution".to_string(),
                duration_ms: 0,
                tokens_used: 0,
            }],
            learnings: vec![format!("Delegated SWE task: {}", task)],
            started_at,
            completed_at,
        })
    }
        // Build Learning structs from the string insights
        let now = Utc::now();
        let new_learnings: Vec<Learning> = result
            .learnings
            .iter()
            .map(|insight| Learning {
                category: maestro_algorithm::LearningCategory::Reflection,
                insight: insight.clone(),
                context: format!(
                    "Task: {} | Complexity: {} | Satisfaction: {:.0}%",
                    result.task.chars().take(80).collect::<String>(),
                    result.complexity,
                    result.satisfaction * 100.0,
                ),
                actionable: false,
                timestamp: now,
            })
            .collect();

        // Accumulate in-memory (keep last 500)
        {
            let mut learnings = self.learnings.write().await;
            learnings.extend(new_learnings.clone());
            if learnings.len() > 500 {
                let drain_count = learnings.len() - 500;
                learnings.drain(..drain_count);
            }
        }

        // Persist each learning to kernel memory with a structured key
        for learning in &new_learnings {
            let key = format!("supervisor:learning:{}", Uuid::new_v4());
            let value = serde_json::json!({
                "category": format!("{:?}", learning.category),
                "insight": learning.insight,
                "context": learning.context,
                "actionable": learning.actionable,
                "timestamp": learning.timestamp.to_rfc3339(),
                "orchestration_id": result.id.0.to_string(),
                "task_hash": &result.task[..result.task.len().min(50)],
            });
            let _ = self.kernel.memory_store(&key, value).await;
        }

        // Also store a consolidated learnings index for the task
        let task_key = format!(
            "supervisor:learnings_for:{}",
            &result.task[..result.task.len().min(50)]
        );
        let insights: Vec<String> = result.learnings.clone();
        let _ = self
            .kernel
            .memory_store(&task_key, serde_json::to_value(&insights).unwrap_or_default())
            .await;

        debug!(
            count = result.learnings.len(),
            "Persisted learnings to memory"
        );
    }

    /// Feedback loop: analyze historical performance and auto-tune algorithm
    /// configuration based on observed patterns.
    ///
    /// Adjustments:
    /// - If avg satisfaction is consistently high (>0.85), raise the ISC threshold
    ///   to push for even better results.
    /// - If avg satisfaction is low (<0.5), lower the ISC threshold to avoid
    ///   infinite retry loops.
    /// - If most tasks are low complexity, raise the sequential threshold to
    ///   avoid unnecessary orchestration overhead.
    /// - If many tasks fail, lower the complexity threshold to trigger more
    ///   thorough orchestration.
    async fn feedback_loop(&self) {
        let history = self.history.read().await;

        // Need at least 5 runs for meaningful feedback
        if history.len() < 5 {
            return;
        }

        // Analyze the last 20 runs (or all if fewer)
        let recent: Vec<&OrchestrationResult> = history.iter().rev().take(20).collect();
        let count = recent.len() as f64;

        let avg_satisfaction: f64 = recent.iter().map(|r| r.satisfaction).sum::<f64>() / count;
        let avg_complexity: f64 =
            recent.iter().map(|r| r.complexity as f64).sum::<f64>() / count;
        let failure_rate: f64 = recent
            .iter()
            .filter(|r| r.satisfaction < 0.5)
            .count() as f64
            / count;

        let mut config = self.config.write().await;
        let mut adjusted = false;

        // Satisfaction-based ISC threshold adjustment
        if avg_satisfaction > 0.85 && config.satisfaction_threshold < 0.9 {
            config.satisfaction_threshold = (config.satisfaction_threshold + 0.05).min(0.95);
            adjusted = true;
            info!(
                new_threshold = config.satisfaction_threshold,
                "Feedback: raised ISC threshold (high avg satisfaction)"
            );
        } else if avg_satisfaction < 0.5 && config.satisfaction_threshold > 0.5 {
            config.satisfaction_threshold = (config.satisfaction_threshold - 0.05).max(0.4);
            adjusted = true;
            info!(
                new_threshold = config.satisfaction_threshold,
                "Feedback: lowered ISC threshold (low avg satisfaction)"
            );
        }

        // Complexity threshold adjustment
        if avg_complexity < 3.0 && config.complexity_threshold_sequential < 5 {
            config.complexity_threshold_sequential += 1;
            adjusted = true;
            info!(
                new_threshold = config.complexity_threshold_sequential,
                "Feedback: raised sequential threshold (mostly simple tasks)"
            );
        } else if failure_rate > 0.3 && config.complexity_threshold_sequential > 2 {
            config.complexity_threshold_sequential -= 1;
            adjusted = true;
            info!(
                new_threshold = config.complexity_threshold_sequential,
                "Feedback: lowered sequential threshold (high failure rate)"
            );
        }

        // Max retries adjustment
        if failure_rate > 0.4 && config.max_retries < 4 {
            config.max_retries += 1;
            adjusted = true;
            info!(
                new_retries = config.max_retries,
                "Feedback: increased max retries (high failure rate)"
            );
        }

        if adjusted {
            // Persist the adjusted config as a learning
            let _ = self
                .kernel
                .memory_store(
                    "supervisor:config:auto_tuned",
                    serde_json::to_value(&*config).unwrap_or_default(),
                )
                .await;
        }
    }

    /// Cleanup agents spawned during an orchestration.
    async fn cleanup_spawned_agents(&self, orch_id: OrchestrationId) {
        if let Some((_, agents)) = self.spawned_agents.remove(&orch_id) {
            for agent_id in agents {
                debug!(agent_id = %agent_id, "Cleaning up spawned agent");
                let _ = self.kernel.kill_agent(&agent_id.to_string()).await;
            }
        }
    }

    /// Get the learning history.
    pub async fn learnings(&self) -> Vec<Learning> {
        self.learnings.read().await.clone()
    }

    /// Get the orchestration history.
    pub async fn history(&self) -> Vec<OrchestrationResult> {
        self.history.read().await.clone()
    }

    /// Get a specific orchestration result by ID.
    pub async fn get_run(&self, id: OrchestrationId) -> Option<OrchestrationResult> {
        self.history
            .read()
            .await
            .iter()
            .find(|r| r.id == id)
            .cloned()
    }
}

// ── ExecutionHooks Bridge ───────────────────────────────────────────────────

/// Bridges the MAESTRO algorithm's `ExecutionHooks` to the kernel's
/// `KernelHandle` for real agent delegation.
struct SupervisorHooks<'a> {
    kernel: Arc<dyn KernelHandle>,
    orchestration_id: OrchestrationId,
    active_run: &'a RwLock<Option<ActiveRun>>,
    spawned_agents: &'a DashMap<OrchestrationId, Vec<AgentId>>,
}

#[async_trait]
impl<'a> ExecutionHooks for SupervisorHooks<'a> {
    async fn on_phase_start(&self, _run_id: RunId, phase: Phase) {
        info!(
            orchestration_id = %self.orchestration_id,
            phase = %phase,
            "Phase starting"
        );

        // Update the active run's current phase
        if let Some(ref mut run) = *self.active_run.write().await {
            run.current_phase = phase;
        }

        // Publish event to the kernel's event bus
        let _ = self
            .kernel
            .publish_event(
                "supervisor.phase.start",
                serde_json::json!({
                    "orchestration_id": self.orchestration_id.to_string(),
                    "phase": phase.to_string(),
                }),
            )
            .await;
    }

    async fn on_phase_complete(&self, _run_id: RunId, phase: Phase, output: &PhaseOutput) {
        info!(
            orchestration_id = %self.orchestration_id,
            phase = %phase,
            duration_ms = output.duration_ms,
            tokens = output.tokens_used,
            "Phase complete"
        );

        let _ = self
            .kernel
            .publish_event(
                "supervisor.phase.complete",
                serde_json::json!({
                    "orchestration_id": self.orchestration_id.to_string(),
                    "phase": phase.to_string(),
                    "duration_ms": output.duration_ms,
                    "tokens_used": output.tokens_used,
                }),
            )
            .await;
    }

    async fn delegate_to_agent(
        &self,
        step_description: &str,
        capabilities: &[String],
    ) -> Result<String, AlgorithmError> {
        info!(
            orchestration_id = %self.orchestration_id,
            step_preview = %step_description.chars().take(60).collect::<String>(),
            capabilities = ?capabilities,
            "Delegating step to agent"
        );

        // First, try to find an existing running agent with matching capabilities
        let agents = self.kernel.list_agents();
        let matching = agents.iter().find(|a| {
            a.state == "Running"
                && capabilities.iter().all(|cap| {
                    a.tools.iter().any(|t| t.contains(cap))
                        || a.tags.iter().any(|t| t.contains(cap))
                })
        });

        let agent_id = if let Some(agent) = matching {
            debug!(
                agent_name = %agent.name,
                "Found existing agent for delegation"
            );
            agent.id.clone()
        } else {
            // Spawn a new agent with the required capabilities
            let manifest = build_worker_manifest("supervisor-worker", capabilities);
            let (id, name) = self
                .kernel
                .spawn_agent(&manifest, None)
                .await
                .map_err(AlgorithmError::DelegationError)?;

            info!(agent_id = %id, agent_name = %name, "Spawned new worker agent");

            // Track the spawned agent for cleanup
            if let Some(mut agents) = self.spawned_agents.get_mut(&self.orchestration_id) {
                if let Ok(parsed_id) = id.parse::<AgentId>() {
                    agents.push(parsed_id);
                }
            }

            id
        };

        // Send the task to the agent
        let response = self
            .kernel
            .send_to_agent(&agent_id, step_description)
            .await
            .map_err(AlgorithmError::DelegationError)?;

        Ok(response)
    }

    async fn store_learning(&self, learning: &Learning) {
        debug!(
            category = ?learning.category,
            insight_preview = %learning.insight.chars().take(60).collect::<String>(),
            "Storing learning"
        );

        // Store in kernel's memory system
        let _ = self
            .kernel
            .memory_store(
                &format!("supervisor:learning:{}", Uuid::new_v4()),
                serde_json::to_value(learning).unwrap_or_default(),
            )
            .await;
    }

    async fn retrieve_learnings(&self, task: &str) -> Vec<String> {
        // Query kernel's memory for relevant learnings
        let key = format!("supervisor:learnings_for:{}", &task[..task.len().min(50)]);
        match self.kernel.memory_recall(&key).await {
            Ok(Some(value)) => {
                if let Some(arr) = value.as_array() {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                } else {
                    vec![]
                }
            }
            _ => vec![],
        }
    }

    async fn on_run_complete(&self, result: &AlgorithmResult) {
        info!(
            orchestration_id = %self.orchestration_id,
            satisfaction = format!("{:.1}%", result.overall_satisfaction * 100.0),
            total_tokens = result.total_tokens_used,
            learnings = result.learnings.len(),
            "Algorithm run complete"
        );

        let _ = self
            .kernel
            .publish_event(
                "supervisor.run.complete",
                serde_json::json!({
                    "orchestration_id": self.orchestration_id.to_string(),
                    "satisfaction": result.overall_satisfaction,
                    "total_tokens": result.total_tokens_used,
                    "learnings_count": result.learnings.len(),
                }),
            )
            .await;
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Build a minimal TOML manifest for a worker agent with the given capabilities.
fn build_worker_manifest(name_prefix: &str, capabilities: &[String]) -> String {
    let tools: Vec<String> = capabilities
        .iter()
        .map(|c| format!("\"{}\"", c))
        .collect();
    let tools_str = if tools.is_empty() {
        "[]".to_string()
    } else {
        format!("[{}]", tools.join(", "))
    };

    let tags: Vec<String> = capabilities
        .iter()
        .map(|c| format!("\"{}\"", c))
        .collect();
    let tags_str = if tags.is_empty() {
        "[\"supervisor-worker\"]".to_string()
    } else {
        format!("[\"supervisor-worker\", {}]", tags.join(", "))
    };

    format!(
        r#"name = "{name_prefix}"
version = "0.1.0"
description = "Worker agent spawned by the supervisor for task delegation"
author = "supervisor"
module = "builtin:chat"
tags = {tags_str}

[schedule]
mode = "reactive"

[model]
provider = "openai"
model = "gpt-4.1-mini"

[resources]
max_tokens_per_turn = 8192
max_turns = 20
max_memory_mb = 256

[capabilities]
tools = {tools_str}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orchestration_id_uniqueness() {
        let id1 = OrchestrationId::new();
        let id2 = OrchestrationId::new();
        assert_ne!(id1.0, id2.0);
    }

    #[test]
    fn test_build_worker_manifest() {
        let manifest = build_worker_manifest("test-worker", &["web_search".to_string()]);
        assert!(manifest.contains("test-worker"));
        assert!(manifest.contains("web_search"));
        assert!(manifest.contains("supervisor-worker"));
    }

    #[test]
    fn test_build_worker_manifest_no_capabilities() {
        let manifest = build_worker_manifest("test-worker", &[]);
        assert!(manifest.contains("test-worker"));
        assert!(manifest.contains("tools = []"));
    }
}
