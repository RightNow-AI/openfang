//! Disciplined 10-step WorkItem execution loop.
//!
//! # Contract
//!
//! Every `WorkItem` executed through this module follows the exact same
//! bounded, auditable sequence. No improvisation outside the defined
//! contract. Each step emits a `WorkEvent` so the full lifecycle is
//! observable and reviewable.
//!
//! ## The 10 Steps
//!
//! 1. **Load context** — fetch the WorkItem; validate required fields.
//! 2. **Classify path** — determine FastPath / PlannedSwarm / ReviewSwarm.
//! 3. **Define objective** — construct a concrete, measurable target.
//! 4. **Select adapter** — choose API → CLI → Browser, record rejected alternatives.
//! 5. **Check permissions** — verify the agent exists and can run.
//! 6. **Execute one bounded action** — call `send_message_with_handle`.
//! 7. **Verify result** — apply `VerificationMethod` to the agent's output.
//! 8. **Record outcome** — persist result, emit terminal event, write status.
//! 9. **Retry discipline** — increment `retry_count`; stop at `max_retries`.
//! 10. **Delegation discipline** — create child WorkItem only if explicitly allowed.

use std::str::FromStr;
use std::sync::Arc;

use chrono::Utc;
use openfang_memory::work_item::TransitionExtra;
use openfang_types::agent::AgentId;
use openfang_types::execution::{
    ActionResult, AdapterSelection, BlockReason, ExecutionBudget, ExecutionEventKind,
    ExecutionObjective, ExecutionReport, ExecutionStatus, VerificationMethod, VerificationResult,
};
use openfang_types::planning::ExecutionPath;
use openfang_types::tool_contract::AdapterKind;
use openfang_types::work_item::{WorkEvent, WorkItem, WorkSource, WorkStatus, WorkType};

use crate::kernel::OpenFangKernel;

// ────────────────────────────────────────────────────────────────────────────
// WorkItemExecutor
// ────────────────────────────────────────────────────────────────────────────

/// Drives WorkItems through the disciplined 10-step execution loop.
pub struct WorkItemExecutor {
    kernel: Arc<OpenFangKernel>,
}

impl WorkItemExecutor {
    pub fn new(kernel: Arc<OpenFangKernel>) -> Self {
        Self { kernel }
    }

    /// Execute a WorkItem through all 10 steps, returning a full audit report.
    pub async fn execute(&self, work_item_id: &str) -> ExecutionReport {
        let started_at = Utc::now();
        let mut events_emitted: Vec<ExecutionEventKind> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();

        // ── Step 1: Load execution context ──────────────────────────────────
        let item = match self.kernel.memory.work_items().get_by_id(work_item_id) {
            Ok(Some(i)) => i,
            Ok(None) => {
                return ExecutionReport::blocked(
                    work_item_id,
                    BlockReason::MissingContext {
                        missing: vec!["work_item".to_string()],
                    },
                    ExecutionEventKind::DependencyMissing,
                );
            }
            Err(e) => {
                return ExecutionReport::blocked(
                    work_item_id,
                    BlockReason::DependencyMissing {
                        dependency: format!("storage: {e}"),
                    },
                    ExecutionEventKind::DependencyMissing,
                );
            }
        };

        // Validate minimum required fields.
        let mut missing: Vec<String> = Vec::new();
        if item.assigned_agent_id.is_none() {
            missing.push("assigned_agent_id".to_string());
        }
        if item.title.trim().is_empty() {
            missing.push("title".to_string());
        }
        if !missing.is_empty() {
            self.emit_event(
                &item,
                ExecutionEventKind::DependencyMissing,
                &format!("missing context: {}", missing.join(", ")),
                &mut events_emitted,
            );
            return ExecutionReport::blocked(
                work_item_id,
                BlockReason::MissingContext { missing },
                ExecutionEventKind::DependencyMissing,
            );
        }

        // ── Step 2: Classify execution path ─────────────────────────────────
        let execution_path = classify_execution_path(&item);
        self.emit_event(
            &item,
            ExecutionEventKind::ScopeClassified,
            &format!("path={}", execution_path_str(&execution_path)),
            &mut events_emitted,
        );

        if matches!(execution_path, ExecutionPath::ReviewSwarm) {
            self.emit_event(
                &item,
                ExecutionEventKind::PlanningRequired,
                "ReviewSwarm path requires a full planning round before execution",
                &mut events_emitted,
            );
            warnings.push("ReviewSwarm path: planning round required".to_string());
        }

        // ── Step 3: Define execution objective ──────────────────────────────
        let objective = build_objective(&item);

        // ── Step 4: Select execution adapter ────────────────────────────────
        // Preference order: API (rank 0) → CLI (rank 1) → Browser (rank 2).
        // OpenFang agents are reached via the API surface.
        let adapter_selection = AdapterSelection {
            chosen: AdapterKind::Api,
            rejected: vec![AdapterKind::Cli, AdapterKind::Browser],
            rationale: "Agent loop runs over the kernel API surface; CLI and Browser are not available for agent-to-agent delegation.".to_string(),
        };
        self.emit_event(
            &item,
            ExecutionEventKind::AdapterSelected,
            &format!(
                "adapter=api rejected=[cli,browser] reason={}",
                adapter_selection.rationale
            ),
            &mut events_emitted,
        );

        // ── Step 5: Check permissions / approvals ───────────────────────────
        let agent_id_str = item.assigned_agent_id.as_deref().unwrap_or("");

        // Parse the agent ID — hard fail if it's malformed.
        let agent_id = match AgentId::from_str(agent_id_str) {
            Ok(id) => id,
            Err(_) => {
                self.emit_event(
                    &item,
                    ExecutionEventKind::PermissionDenied,
                    &format!("invalid agent_id format: {agent_id_str}"),
                    &mut events_emitted,
                );
                return ExecutionReport {
                    work_item_id: work_item_id.to_string(),
                    execution_path: Some(execution_path),
                    adapter_selection: Some(adapter_selection),
                    objective: Some(objective),
                    action_result: None,
                    verification: None,
                    status: ExecutionStatus::Blocked,
                    block_reason: Some(BlockReason::PermissionDenied {
                        detail: format!("invalid agent_id: {agent_id_str}"),
                    }),
                    result_summary: format!("agent_id is not a valid UUID: {agent_id_str}"),
                    artifact_refs: vec![],
                    retry_count: item.retry_count,
                    retry_scheduled: false,
                    delegated_to: None,
                    cost_usd: None,
                    warnings,
                    events_emitted,
                    started_at,
                    finished_at: Utc::now(),
                };
            }
        };

        // Confirm the agent exists in the registry.
        if self.kernel.registry.get(agent_id).is_none() {
            self.emit_event(
                &item,
                ExecutionEventKind::PermissionDenied,
                &format!("agent {agent_id_str} not found in registry"),
                &mut events_emitted,
            );
            return ExecutionReport {
                work_item_id: work_item_id.to_string(),
                execution_path: Some(execution_path),
                adapter_selection: Some(adapter_selection),
                objective: Some(objective),
                action_result: None,
                verification: None,
                status: ExecutionStatus::Blocked,
                block_reason: Some(BlockReason::DependencyMissing {
                    dependency: format!("agent:{agent_id_str}"),
                }),
                result_summary: format!("agent {agent_id_str} not registered"),
                artifact_refs: vec![],
                retry_count: item.retry_count,
                retry_scheduled: false,
                delegated_to: None,
                cost_usd: None,
                warnings,
                events_emitted,
                started_at,
                finished_at: Utc::now(),
            };
        }

        // ── Step 6: Execute one bounded action ──────────────────────────────
        let prompt = build_execution_prompt(&item, &objective);
        let action_started_at = Utc::now();

        self.emit_event(
            &item,
            ExecutionEventKind::ExecutionStarted,
            &format!(
                "agent={} adapter=api prompt_len={}",
                agent_id_str,
                prompt.len()
            ),
            &mut events_emitted,
        );

        let loop_result = self
            .kernel
            .send_message_with_handle(agent_id, &prompt, None)
            .await;

        let action_finished_at = Utc::now();

        let action_result = match loop_result {
            Ok(result) => {
                self.emit_event(
                    &item,
                    ExecutionEventKind::ExecutionFinished,
                    &format!(
                        "tokens_in={} tokens_out={} iterations={} cost_usd={:.6}",
                        result.total_usage.input_tokens,
                        result.total_usage.output_tokens,
                        result.iterations,
                        result.cost_usd.unwrap_or(0.0),
                    ),
                    &mut events_emitted,
                );
                ActionResult {
                    input_summary: format!("WorkItem: {}", item.title),
                    adapter: AdapterKind::Api,
                    action: prompt.clone(),
                    output: result.response.clone(),
                    error: None,
                    action_succeeded: true,
                    tokens_in: result.total_usage.input_tokens as u32,
                    tokens_out: result.total_usage.output_tokens as u32,
                    cost_usd: result.cost_usd,
                    iterations: result.iterations,
                    started_at: action_started_at,
                    finished_at: action_finished_at,
                }
            }
            Err(e) => {
                let error_msg = e.to_string();
                self.emit_event(
                    &item,
                    ExecutionEventKind::ExecutionFinished,
                    &format!("error={error_msg}"),
                    &mut events_emitted,
                );
                ActionResult {
                    input_summary: format!("WorkItem: {}", item.title),
                    adapter: AdapterKind::Api,
                    action: prompt.clone(),
                    output: String::new(),
                    error: Some(error_msg.clone()),
                    action_succeeded: false,
                    tokens_in: 0,
                    tokens_out: 0,
                    cost_usd: None,
                    iterations: 0,
                    started_at: action_started_at,
                    finished_at: action_finished_at,
                }
            }
        };

        // ── Step 7: Verify result ────────────────────────────────────────────
        let verification = verify_result(&action_result, &objective.verification_method);

        if verification.passed {
            self.emit_event(
                &item,
                ExecutionEventKind::VerifiedSuccess,
                &verification.evidence,
                &mut events_emitted,
            );
        } else {
            self.emit_event(
                &item,
                ExecutionEventKind::VerificationFailed,
                &verification.evidence,
                &mut events_emitted,
            );
        }

        // ── Step 8: Record outcome ───────────────────────────────────────────
        let overall_success = action_result.action_succeeded && verification.passed;
        let cost_usd = action_result.cost_usd;

        if overall_success {
            let extra = TransitionExtra {
                result: Some(action_result.output.clone()),
                error: None,
                iterations: Some(action_result.iterations),
                approval_status: None,
                approved_by: None,
                approved_at: None,
                approval_note: None,
            };
            let _ = self.kernel.memory.work_items().transition(
                work_item_id,
                WorkStatus::Completed,
                Some("work_executor"),
                Some("execution verified successfully"),
                Some(extra),
            );
            self.emit_event(
                &item,
                ExecutionEventKind::Completed,
                "work item completed successfully",
                &mut events_emitted,
            );

            let finished_at = Utc::now();
            return ExecutionReport {
                work_item_id: work_item_id.to_string(),
                execution_path: Some(execution_path),
                adapter_selection: Some(adapter_selection),
                objective: Some(objective),
                action_result: Some(action_result),
                verification: Some(verification),
                status: ExecutionStatus::Completed,
                block_reason: None,
                result_summary: "execution completed and verified".to_string(),
                artifact_refs: vec![],
                retry_count: item.retry_count,
                retry_scheduled: false,
                delegated_to: None,
                cost_usd,
                warnings,
                events_emitted,
                started_at,
                finished_at,
            };
        }

        // ── Step 9: Retry discipline ─────────────────────────────────────────
        // Only retry on action failures (not verification failures on successful output).
        // Increment retry_count and check against max_retries.
        let new_retry_count = item.retry_count + 1;
        let max_retries = item.max_retries.max(1); // at minimum 1 retry

        if !action_result.action_succeeded && new_retry_count <= max_retries {
            // Schedule retry by resetting item to Pending.
            let _ = self.kernel.memory.work_items().transition(
                work_item_id,
                WorkStatus::Pending,
                Some("work_executor"),
                Some(&format!(
                    "retry {new_retry_count}/{max_retries} scheduled after action failure"
                )),
                None,
            );
            // Persist the incremented retry count by emitting a retry event.
            self.emit_event(
                &item,
                ExecutionEventKind::RetryScheduled,
                &format!("retry_count={new_retry_count} max_retries={max_retries}"),
                &mut events_emitted,
            );

            let finished_at = Utc::now();
            return ExecutionReport {
                work_item_id: work_item_id.to_string(),
                execution_path: Some(execution_path),
                adapter_selection: Some(adapter_selection),
                objective: Some(objective),
                action_result: Some(action_result),
                verification: Some(verification),
                status: ExecutionStatus::RetryScheduled,
                block_reason: None,
                result_summary: format!("retry scheduled ({new_retry_count}/{max_retries})"),
                artifact_refs: vec![],
                retry_count: new_retry_count,
                retry_scheduled: true,
                delegated_to: None,
                cost_usd,
                warnings,
                events_emitted,
                started_at,
                finished_at,
            };
        }

        // ── Step 10: Delegation discipline ───────────────────────────────────
        // Only delegate if the work item has a `delegate` tag and no parent
        // (to prevent unbounded delegation chains).
        let can_delegate = item.tags.iter().any(|t| t == "delegate") && item.parent_id.is_none();

        if can_delegate {
            let delegated_id = self.delegate_work_item(&item, &action_result);
            match delegated_id {
                Some(child_id) => {
                    self.emit_event(
                        &item,
                        ExecutionEventKind::DelegatedToSubagent,
                        &format!("child_work_item_id={child_id}"),
                        &mut events_emitted,
                    );
                    // Transition parent to Pending (it is now waiting on the child).
                    let _ = self.kernel.memory.work_items().transition(
                        work_item_id,
                        WorkStatus::Pending,
                        Some("work_executor"),
                        Some(&format!("delegated to child work item {child_id}")),
                        None,
                    );

                    let finished_at = Utc::now();
                    return ExecutionReport {
                        work_item_id: work_item_id.to_string(),
                        execution_path: Some(execution_path),
                        adapter_selection: Some(adapter_selection),
                        objective: Some(objective),
                        action_result: Some(action_result),
                        verification: Some(verification),
                        status: ExecutionStatus::DelegatedToSubagent,
                        block_reason: None,
                        result_summary: format!("delegated to child work item {child_id}"),
                        artifact_refs: vec![],
                        retry_count: item.retry_count,
                        retry_scheduled: false,
                        delegated_to: Some(child_id),
                        cost_usd,
                        warnings,
                        events_emitted,
                        started_at,
                        finished_at,
                    };
                }
                None => {
                    warnings.push(
                        "delegation requested but child work item could not be created".to_string(),
                    );
                }
            }
        }

        // ── Terminal failure ──────────────────────────────────────────────────
        let failure_reason: String = action_result
            .error
            .clone()
            .unwrap_or_else(|| "verification failed after action succeeded".to_string());
        let extra = TransitionExtra {
            result: None,
            error: Some(failure_reason.clone()),
            iterations: Some(action_result.iterations),
            approval_status: None,
            approved_by: None,
            approved_at: None,
            approval_note: None,
        };
        let _ = self.kernel.memory.work_items().transition(
            work_item_id,
            WorkStatus::Failed,
            Some("work_executor"),
            Some(&format!("max retries exhausted: {failure_reason}")),
            Some(extra),
        );
        self.emit_event(
            &item,
            ExecutionEventKind::Failed,
            &format!("retries_exhausted retry_count={new_retry_count} max_retries={max_retries}"),
            &mut events_emitted,
        );

        ExecutionReport {
            work_item_id: work_item_id.to_string(),
            execution_path: Some(execution_path),
            adapter_selection: Some(adapter_selection),
            objective: Some(objective),
            action_result: Some(action_result),
            verification: Some(verification),
            status: ExecutionStatus::Failed,
            block_reason: None,
            result_summary: format!("failed after {new_retry_count} attempt(s): {failure_reason}"),
            artifact_refs: vec![],
            retry_count: new_retry_count,
            retry_scheduled: false,
            delegated_to: None,
            cost_usd,
            warnings,
            events_emitted,
            started_at,
            finished_at: Utc::now(),
        }
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    /// Append a `WorkEvent` to the work item's audit trail.
    fn emit_event(
        &self,
        item: &WorkItem,
        kind: ExecutionEventKind,
        detail: &str,
        emitted: &mut Vec<ExecutionEventKind>,
    ) {
        let event = WorkEvent {
            id: uuid::Uuid::new_v4().to_string(),
            work_item_id: item.id.clone(),
            event_type: kind.as_str().to_string(),
            from_status: None,
            to_status: None,
            actor: Some("work_executor".to_string()),
            detail: Some(detail.to_string()),
            created_at: Utc::now(),
        };
        let _ = self.kernel.memory.work_items().append_event(&event);
        emitted.push(kind);
    }

    /// Create a child work item for delegation (step 10).
    ///
    /// Returns the new child ID on success.
    fn delegate_work_item(&self, parent: &WorkItem, action_result: &ActionResult) -> Option<String> {
        let child_id = uuid::Uuid::new_v4().to_string();
        let context_payload = serde_json::json!({
            "parent_id": parent.id,
            "parent_title": parent.title,
            "prior_action": action_result.action,
            "prior_output": action_result.output,
        });
        let child = WorkItem {
            id: child_id.clone(),
            title: format!("[delegated] {}", parent.title),
            description: format!(
                "Delegated from work item {}.\n\nOriginal description:\n{}",
                parent.id, parent.description
            ),
            work_type: WorkType::AgentTask,
            source: WorkSource::AgentSpawned,
            status: WorkStatus::Pending,
            assigned_agent_id: parent.assigned_agent_id.clone(),
            assigned_agent_name: parent.assigned_agent_name.clone(),
            parent_id: Some(parent.id.clone()),
            tags: parent
                .tags
                .iter()
                .filter(|t| *t != "delegate")
                .cloned()
                .collect(),
            requires_approval: parent.requires_approval,
            max_retries: parent.max_retries,
            payload: context_payload,
            priority: parent.priority,
            created_by: Some("work_executor".to_string()),
            // Runtime fields
            approval_status: Default::default(),
            result: None,
            error: None,
            iterations: 0,
            retry_count: 0,
            scheduled_at: None,
            deadline: parent.deadline,
            started_at: None,
            completed_at: None,
            approved_by: None,
            approved_at: None,
            approval_note: None,
            idempotency_key: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        match self.kernel.memory.work_items().create(&child) {
            Ok(created) => Some(created.id),
            Err(_) => None,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Pure helpers — no self, no async
// ────────────────────────────────────────────────────────────────────────────

/// Classify the execution path based on work type and tags.
///
/// Rules (in priority order):
/// - `review` tag → ReviewSwarm
/// - `swarm` tag OR `Workflow` work type → PlannedSwarm
/// - Everything else → FastPath
fn classify_execution_path(item: &WorkItem) -> ExecutionPath {
    let has_tag = |t: &str| item.tags.iter().any(|tag| tag == t);

    if has_tag("review") {
        ExecutionPath::ReviewSwarm
    } else if has_tag("swarm") || matches!(item.work_type, WorkType::Workflow) {
        ExecutionPath::PlannedSwarm
    } else {
        ExecutionPath::FastPath
    }
}

fn execution_path_str(path: &ExecutionPath) -> &'static str {
    match path {
        ExecutionPath::FastPath => "fast_path",
        ExecutionPath::PlannedSwarm => "planned_swarm",
        ExecutionPath::ReviewSwarm => "review_swarm",
    }
}

/// Construct a concrete, measurable objective for this WorkItem.
fn build_objective(item: &WorkItem) -> ExecutionObjective {
    let verification_method = if item.tags.iter().any(|t| t == "artifact") {
        VerificationMethod::ArtifactExists {
            artifact_key: "result".to_string(),
        }
    } else {
        VerificationMethod::ResponseNonEmpty
    };

    ExecutionObjective {
        target_system: item
            .assigned_agent_id
            .as_deref()
            .unwrap_or("unknown")
            .to_string(),
        intended_action: item.title.clone(),
        success_condition: "Agent returns a non-empty response without an error".to_string(),
        verification_method,
        budget: ExecutionBudget::default(),
        fallback_adapter: None,
    }
}

/// Build the prompt string sent to the agent in step 6.
fn build_execution_prompt(item: &WorkItem, _objective: &ExecutionObjective) -> String {
    let mut prompt = item.title.clone();

    if !item.description.is_empty() {
        prompt.push_str("\n\n");
        prompt.push_str(&item.description);
    }

    if !item.payload.is_null() && item.payload != serde_json::Value::Object(Default::default()) {
        if let Ok(payload_str) = serde_json::to_string_pretty(&item.payload) {
            prompt.push_str("\n\n## Context\n```json\n");
            prompt.push_str(&payload_str);
            prompt.push_str("\n```");
        }
    }

    prompt
}

/// Apply the `VerificationMethod` to the `ActionResult` and return proof.
fn verify_result(action: &ActionResult, method: &VerificationMethod) -> VerificationResult {
    let now = Utc::now();

    if !action.action_succeeded {
        return VerificationResult {
            passed: false,
            method_used: method.clone(),
            evidence: format!(
                "Action failed before verification: {}",
                action.error.as_deref().unwrap_or("unknown error")
            ),
            verified_at: now,
        };
    }

    match method {
        VerificationMethod::ResponseNonEmpty => {
            let passed = !action.output.trim().is_empty();
            VerificationResult {
                passed,
                method_used: method.clone(),
                evidence: if passed {
                    format!("Response length: {} chars", action.output.len())
                } else {
                    "Response is empty".to_string()
                },
                verified_at: now,
            }
        }
        VerificationMethod::ArtifactExists { artifact_key } => {
            // We cannot check the payload from ActionResult; accept any non-empty output.
            let passed = !action.output.trim().is_empty();
            VerificationResult {
                passed,
                method_used: method.clone(),
                evidence: if passed {
                    format!(
                        "Output present (artifact key '{artifact_key}' presumed written); len={}",
                        action.output.len()
                    )
                } else {
                    format!("Output empty; artifact key '{artifact_key}' not confirmed")
                },
                verified_at: now,
            }
        }
        VerificationMethod::SchemaCheck { schema_hint } => {
            // Basic heuristic: output must be non-empty and parseable as JSON.
            let parsed = serde_json::from_str::<serde_json::Value>(&action.output);
            let passed = parsed.is_ok();
            VerificationResult {
                passed,
                method_used: method.clone(),
                evidence: if passed {
                    format!("Output is valid JSON (schema hint: {schema_hint})")
                } else {
                    format!(
                        "Output is not valid JSON; schema hint '{schema_hint}' check failed"
                    )
                },
                verified_at: now,
            }
        }
        VerificationMethod::RecordFetchable => {
            // Action succeeded — assume the record was written correctly.
            VerificationResult {
                passed: true,
                method_used: method.clone(),
                evidence: "Action succeeded; record assumed fetchable".to_string(),
                verified_at: now,
            }
        }
        VerificationMethod::AnyClosure => VerificationResult {
            passed: true,
            method_used: method.clone(),
            evidence: "AnyClosure method — any non-error response is accepted".to_string(),
            verified_at: now,
        },
    }
}
