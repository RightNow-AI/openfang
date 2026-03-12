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
