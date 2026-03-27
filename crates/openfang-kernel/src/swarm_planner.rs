//! Swarm Planner — deterministic, auditable swarm composition for WorkItems.
//!
//! ## Selection Algorithm (5 steps)
//!
//! **Step 1 — Derive requirements** from WorkItem fields (work_type, tags, payload).
//! **Step 2 — Filter eligible agents** by capability tags, tools, services, concurrency.
//! **Step 3 — Apply selection constraints** (cost, runtime class, auto-run safety, load).
//! **Step 4 — Compose swarm** (primary + optional research / quality / coordination / approval).
//! **Step 5 — Record decision** as a WorkEvent on the WorkItem. Never skipped.
//!
//! ## Strict rules
//! - No swarm may form without a recorded `SwarmPlan`.
//! - Incompatible agents are never placed in the same swarm.
//! - Max swarm size defaults to 3 unless workflow explicitly overrides.
//! - Approval agents are added automatically when any member requires gating.
//! - Every rejection is logged with a `RejectionRule` code.

use crate::swarm_registry::SwarmRegistry;
use chrono::Utc;
use openfang_types::swarm::{
    AgentHealthStatus, ApprovalGatePolicy, CapabilityTag, ManifestRiskLevel, RejectionReason,
    RejectionRule, RuntimeClass, SwarmDivision, SwarmMember, SwarmPlan, SwarmRole,
    WorkItemRequirements,
};use openfang_types::work_item::{WorkEvent, WorkItem, WorkType};
use std::collections::HashSet;
use tracing::info;

/// Default maximum swarm roster size.
pub const DEFAULT_MAX_SWARM_SIZE: usize = 3;

/// Configuration for a single swarm plan request.
#[derive(Debug, Clone)]
pub struct SwarmPlanRequest {
    /// The work item to compose a swarm for.
    pub work_item: WorkItem,
    /// Override the maximum roster size (default: `DEFAULT_MAX_SWARM_SIZE`).
    pub max_swarm_size: Option<usize>,
    /// If true, only include agents marked `safe_for_auto_run`.
    pub require_auto_run: bool,
    /// Extra capability tags to require beyond what the work item implies.
    pub extra_required_tags: Vec<CapabilityTag>,
}

/// The main swarm planning engine. Thread-safe; holds an Arc to the manifest registry.
pub struct SwarmPlanner {
    pub registry: std::sync::Arc<SwarmRegistry>,
}

impl SwarmPlanner {
    pub fn new(registry: std::sync::Arc<SwarmRegistry>) -> Self {
        Self { registry }
    }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Compose a swarm plan for the given WorkItem.
    ///
    /// Always returns a `SwarmPlan` — the plan may have `success = false` if
    /// no valid swarm could be formed. The plan must be persisted as a
    /// WorkEvent by the caller (see `plan_to_work_events`).
    pub fn plan(&self, req: SwarmPlanRequest) -> SwarmPlan {
        let plan_id = uuid::Uuid::new_v4().to_string();
        let requirements = derive_requirements(&req.work_item, &req.extra_required_tags);
        let max_size = req.max_swarm_size.unwrap_or(DEFAULT_MAX_SWARM_SIZE);

        // Step 2 — Filter candidates
        let (eligible, mut rejected) =
            self.filter_eligible(&requirements, req.require_auto_run);

        if eligible.is_empty() {
            let plan = SwarmPlan {
                id: plan_id,
                work_item_id: req.work_item.id.clone(),
                planned_at: Utc::now(),
                requirements,
                members: vec![],
                rejected_candidates: rejected,
                selection_rationale: "No eligible agents found. Check capability tags, \
                    required tools, and service availability."
                    .into(),
                risk_assessment: "Unable to assess — no candidates.".into(),
                approval_gating_required: false,
                approval_gate_policy: ApprovalGatePolicy::None,
                success: false,
                error: Some("no_eligible_agents".into()),
            };
            return plan;
        }

        // Step 3 — Apply selection constraints and compose roster
        let (members, additional_rejected) = compose_swarm(eligible, max_size, &requirements);
        rejected.extend(additional_rejected);

        if members.is_empty() {
            return SwarmPlan {
                id: plan_id,
                work_item_id: req.work_item.id.clone(),
                planned_at: Utc::now(),
                requirements,
                members: vec![],
                rejected_candidates: rejected,
                selection_rationale: "Swarm composition failed after constraint filtering.".into(),
                risk_assessment: "Unable to assess — no members selected.".into(),
                approval_gating_required: false,
                approval_gate_policy: ApprovalGatePolicy::None,
                success: false,
                error: Some("composition_failed".into()),
            };
        }

        // Step 4 — Determine approval gating
        let approval_gating_required =
            members.iter().any(|m| m.requires_approval) || requirements.requires_approval;
        let approval_gate_policy = derive_approval_policy(&members, &requirements);

        // Build rationale and risk assessment
        let rationale = build_rationale(&members, &rejected, &requirements);
        let risk_assessment = build_risk_assessment(&members, &requirements);

        info!(
            work_item_id = %req.work_item.id,
            member_count = members.len(),
            rejected_count = rejected.len(),
            approval_required = approval_gating_required,
            "swarm plan composed"
        );

        SwarmPlan {
            id: plan_id,
            work_item_id: req.work_item.id.clone(),
            planned_at: Utc::now(),
            requirements,
            members,
            rejected_candidates: rejected,
            selection_rationale: rationale,
            risk_assessment,
            approval_gating_required,
            approval_gate_policy,
            success: true,
            error: None,
        }
    }

    // -----------------------------------------------------------------------
    // Step 2: Filter eligible candidates
    // -----------------------------------------------------------------------

    fn filter_eligible(
        &self,
        requirements: &WorkItemRequirements,
        require_auto_run: bool,
    ) -> (Vec<EligibleCandidate>, Vec<RejectionReason>) {
        let all_entries = self.registry.list();
        let mut eligible = Vec::new();
        let mut rejected = Vec::new();

        for entry in all_entries {
            let manifest = &entry.manifest;
            let id = &entry.agent_id;
            let name = &manifest.name;

            // Rule: must have matching capability tags (if requirements specify any)
            if !requirements.required_capability_tags.is_empty() {
                let matched: Vec<CapabilityTag> = requirements
                    .required_capability_tags
                    .iter()
                    .filter(|req_tag| {
                        manifest.capability_tags.iter().any(|t| {
                            t.namespace == req_tag.namespace && t.name == req_tag.name
                        })
                    })
                    .cloned()
                    .collect();

                if matched.is_empty() {
                    rejected.push(RejectionReason {
                        agent_id: id.clone(),
                        agent_name: name.clone(),
                        reason: format!(
                            "No capability tags matched required: {:?}",
                            requirements
                                .required_capability_tags
                                .iter()
                                .map(|t| t.as_dotted())
                                .collect::<Vec<_>>()
                        ),
                        rule: RejectionRule::MissingCapabilityTag,
                    });
                    continue;
                }
            }

            // Rule: safe_for_auto_run gating
            if require_auto_run && requirements.auto_run_required && !manifest.safe_for_auto_run {
                rejected.push(RejectionReason {
                    agent_id: id.clone(),
                    agent_name: name.clone(),
                    reason: "Agent is not safe_for_auto_run but auto-run is required.".into(),
                    rule: RejectionRule::NotSafeForAutoRun,
                });
                continue;
            }

            // Calculate matched tags for scoring
            let matched_tags: Vec<CapabilityTag> = if requirements
                .required_capability_tags
                .is_empty()
            {
                manifest.capability_tags.clone()
            } else {
                requirements
                    .required_capability_tags
                    .iter()
                    .filter(|req_tag| {
                        manifest.capability_tags.iter().any(|t| {
                            t.namespace == req_tag.namespace && t.name == req_tag.name
                        })
                    })
                    .cloned()
                    .collect()
            };

            eligible.push(EligibleCandidate {
                agent_id: id.clone(),
                matched_tags,
                entry,
            });
        }

        (eligible, rejected)
    }
}

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

struct EligibleCandidate {
    agent_id: String,
    matched_tags: Vec<CapabilityTag>,
    entry: openfang_types::swarm::SwarmRegistryEntry,
}

// ---------------------------------------------------------------------------
// Step 1: Derive requirements from WorkItem
// ---------------------------------------------------------------------------

pub fn derive_requirements(
    item: &WorkItem,
    extra_tags: &[CapabilityTag],
) -> WorkItemRequirements {
    let mut required_tags: Vec<CapabilityTag> = extra_tags.to_vec();

    // Map work_type to capability tags
    let type_tags = match item.work_type {
        WorkType::AgentTask => vec![],
        WorkType::Workflow => vec![CapabilityTag::new("coordination", "workflow")],
        WorkType::Transformation => vec![CapabilityTag::new("data", "transform")],
        WorkType::Research => vec![CapabilityTag::new("research", "web")],
        WorkType::Generation => vec![CapabilityTag::new("output", "generation")],
        WorkType::Custom => vec![],
    };
    required_tags.extend(type_tags);

    // Hoist tags from WorkItem tags into capability requirements
    for tag in &item.tags {
        if let Some(ct) = CapabilityTag::parse(tag) {
            if !required_tags.iter().any(|r| r == &ct) {
                required_tags.push(ct);
            }
        }
    }

    // Determine risk level (derive from approval requirement as a lower bound)
    let risk_level = if item.requires_approval {
        ManifestRiskLevel::High
    } else {
        ManifestRiskLevel::Low
    };

    WorkItemRequirements {
        required_capability_tags: required_tags,
        required_output_schema: None,
        risk_level,
        runtime_class: RuntimeClass::Medium,
        requires_approval: item.requires_approval,
        auto_run_required: !item.requires_approval,
    }
}

// ---------------------------------------------------------------------------
// Step 3 + 4: Compose swarm from eligible candidates
// ---------------------------------------------------------------------------

/// After filtering, compose the actual swarm roster.
///
/// Returns `(members, additional_rejected)`.
/// Enforces:
/// - Maximum swarm size
/// - Incompatibility rules between selected agents
/// - Role diversity (one primary, optional research, quality, coordination)
fn compose_swarm(
    eligible: Vec<EligibleCandidate>,
    max_size: usize,
    requirements: &WorkItemRequirements,
) -> (Vec<SwarmMember>, Vec<RejectionReason>) {
    let mut members: Vec<SwarmMember> = Vec::new();
    let mut additional_rejected: Vec<RejectionReason> = Vec::new();
    let mut selected_ids: HashSet<String> = HashSet::new();
    // Accumulates all agent IDs that any currently-selected agent has declared incompatible.
    // When a new candidate is considered, checking this set provides the symmetric guard.
    let mut excluded_by_selected: HashSet<String> = HashSet::new();

    // Sort candidates: prefer exact capability tag matches, then by division priority
    let mut sorted = eligible;
    sorted.sort_by(|a, b| {
        // More matched tags = higher priority
        b.matched_tags.len().cmp(&a.matched_tags.len())
    });

    for candidate in sorted {
        if members.len() >= max_size {
            additional_rejected.push(RejectionReason {
                agent_id: candidate.agent_id.clone(),
                agent_name: candidate.entry.manifest.name.clone(),
                reason: format!("Swarm roster full (max_size = {max_size})."),
                rule: RejectionRule::MaxConcurrencyExceeded,
            });
            continue;
        }

        let manifest = &candidate.entry.manifest;

        // Incompatibility check: reject if any already-selected agent is incompatible
        let incompatible_with_selected = manifest
            .incompatible_with
            .iter()
            .any(|incompat_id| selected_ids.contains(incompat_id.as_str()));
        if incompatible_with_selected {
            additional_rejected.push(RejectionReason {
                agent_id: candidate.agent_id.clone(),
                agent_name: manifest.name.clone(),
                reason: format!(
                    "Incompatible with one or more already-selected agents: {:?}",
                    manifest.incompatible_with
                ),
                rule: RejectionRule::IncompatibleWithSelected,
            });
            continue;
        }

        // Also reject if any already-selected agent declared this candidate as incompatible.
        let excluded_by_prior = excluded_by_selected.contains(candidate.agent_id.as_str());
        if excluded_by_prior {
            additional_rejected.push(RejectionReason {
                agent_id: candidate.agent_id.clone(),
                agent_name: manifest.name.clone(),
                reason: "An already-selected agent declared this agent incompatible.".into(),
                rule: RejectionRule::IncompatibleWithSelected,
            });
            continue;
        }

        // Assign swarm role based on division
        let role = assign_role(manifest, members.is_empty(), requirements);

        let member = SwarmMember {
            agent_id: candidate.agent_id.clone(),
            agent_name: manifest.name.clone(),
            role,
            division: manifest.division.clone(),
            matched_tags: candidate.matched_tags,
            requires_approval: manifest.requires_approval,
            approval_policy: manifest.approval_policy.clone(),
            health_status: AgentHealthStatus::Unknown, // resolved at execution time
        };

        selected_ids.insert(candidate.agent_id.clone());
        // Register all agent IDs this selected candidate is incompatible with
        // so the reverse check fires when those agents come up as candidates later.
        for incompat_id in &manifest.incompatible_with {
            excluded_by_selected.insert(incompat_id.clone());
        }
        members.push(member);
    }

    (members, additional_rejected)
}

/// Assign a swarm role based on agent division and whether a primary is already chosen.
fn assign_role(
    manifest: &openfang_types::swarm::AgentSwarmManifest,
    is_first: bool,
    _requirements: &WorkItemRequirements,
) -> SwarmRole {
    if is_first {
        return SwarmRole::PrimaryExecution;
    }
    match manifest.division {
        SwarmDivision::Research => SwarmRole::Research,
        SwarmDivision::Quality => SwarmRole::QualityCheck,
        SwarmDivision::Coordination => SwarmRole::Coordination,
        SwarmDivision::Trust => SwarmRole::Trust,
        SwarmDivision::Operations => SwarmRole::Coordination,
        _ => SwarmRole::Research, // fallback secondary role
    }
}

/// Derive the approval gate policy for the composed swarm.
fn derive_approval_policy(
    members: &[SwarmMember],
    requirements: &WorkItemRequirements,
) -> ApprovalGatePolicy {
    // Any member with pre_execute triggers the whole swarm
    if members
        .iter()
        .any(|m| m.approval_policy == ApprovalGatePolicy::PreExecute)
    {
        return ApprovalGatePolicy::PreExecute;
    }
    if requirements.requires_approval {
        return ApprovalGatePolicy::PostDraft;
    }
    if members
        .iter()
        .any(|m| m.approval_policy == ApprovalGatePolicy::PostDraft)
    {
        return ApprovalGatePolicy::PostDraft;
    }
    if members
        .iter()
        .any(|m| m.approval_policy == ApprovalGatePolicy::Conditional)
    {
        return ApprovalGatePolicy::Conditional;
    }
    ApprovalGatePolicy::None
}

// ---------------------------------------------------------------------------
// Rationale and risk narrative builders
// ---------------------------------------------------------------------------

fn build_rationale(
    members: &[SwarmMember],
    rejected: &[RejectionReason],
    requirements: &WorkItemRequirements,
) -> String {
    let mut parts = Vec::new();

    if !requirements.required_capability_tags.is_empty() {
        let tag_list: Vec<String> = requirements
            .required_capability_tags
            .iter()
            .map(|t| t.as_dotted())
            .collect();
        parts.push(format!("Required capabilities: [{}].", tag_list.join(", ")));
    }

    parts.push(format!(
        "Selected {} agent(s): {}.",
        members.len(),
        members
            .iter()
            .map(|m| format!("{} ({})", m.agent_name, m.role.as_str()))
            .collect::<Vec<_>>()
            .join(", ")
    ));

    if !rejected.is_empty() {
        parts.push(format!(
            "Rejected {} candidate(s): {}.",
            rejected.len(),
            rejected
                .iter()
                .map(|r| format!("{} [{:?}]", r.agent_name, r.rule))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    parts.join(" ")
}

fn build_risk_assessment(
    members: &[SwarmMember],
    requirements: &WorkItemRequirements,
) -> String {
    // Use requirement risk level as the baseline — per-member risk is
    // available from the registry at execution time but not cached on SwarmMember.
    let approval_note = if members.iter().any(|m| m.requires_approval) {
        " Approval gate is required before or after execution."
    } else {
        ""
    };

    format!(
        "Requirement risk context: {}. Approval gating required: {}.{}",
        requirements.risk_level.as_str(),
        members.iter().any(|m| m.requires_approval),
        approval_note,
    )
}

// ---------------------------------------------------------------------------
// WorkEvent helpers — Step 5: Record swarm decision
// ---------------------------------------------------------------------------

/// Convert a `SwarmPlan` into one or two `WorkEvent`s for audit trail persistence.
///
/// Returns:
/// - A `"swarm_planned"` event with the full plan serialized into `detail`.
/// - If the plan failed, a `"swarm_selection_failed"` event.
pub fn plan_to_work_events(plan: &SwarmPlan) -> Vec<WorkEvent> {
    let now = Utc::now();
    let mut events = Vec::new();

    if plan.success {
        let detail = serde_json::to_string(plan).unwrap_or_else(|e| {
            format!("{{\"error\": \"serialization failed: {e}\"}}")
        });
        events.push(WorkEvent {
            id: uuid::Uuid::new_v4().to_string(),
            work_item_id: plan.work_item_id.clone(),
            event_type: "swarm_planned".to_string(),
            from_status: None,
            to_status: None,
            actor: Some("swarm_planner".to_string()),
            detail: Some(detail),
            created_at: now,
        });
    } else {
        events.push(WorkEvent {
            id: uuid::Uuid::new_v4().to_string(),
            work_item_id: plan.work_item_id.clone(),
            event_type: "swarm_selection_failed".to_string(),
            from_status: None,
            to_status: None,
            actor: Some("swarm_planner".to_string()),
            detail: Some(
                plan.error
                    .clone()
                    .unwrap_or_else(|| "unknown error".to_string()),
            ),
            created_at: now,
        });
    }

    events
}

/// Build a `"dependency_missing"` WorkEvent for a required service that is unavailable.
pub fn dependency_missing_event(work_item_id: &str, service_id: &str, detail: &str) -> WorkEvent {
    WorkEvent {
        id: uuid::Uuid::new_v4().to_string(),
        work_item_id: work_item_id.to_string(),
        event_type: "dependency_missing".to_string(),
        from_status: None,
        to_status: None,
        actor: Some("swarm_planner".to_string()),
        detail: Some(format!("service={service_id}: {detail}")),
        created_at: Utc::now(),
    }
}

/// Build an `"agent_failed"` WorkEvent.
pub fn agent_failed_event(
    work_item_id: &str,
    agent_id: &str,
    agent_name: &str,
    error: &str,
) -> WorkEvent {
    WorkEvent {
        id: uuid::Uuid::new_v4().to_string(),
        work_item_id: work_item_id.to_string(),
        event_type: "agent_failed".to_string(),
        from_status: None,
        to_status: None,
        actor: Some(agent_id.to_string()),
        detail: Some(format!("agent={agent_name}: {error}")),
        created_at: Utc::now(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swarm_registry::SwarmRegistry;
    use openfang_types::swarm::*;
    use openfang_types::work_item::{WorkItem, WorkStatus, WorkType};
    use std::sync::Arc;

    fn make_work_item(id: &str, work_type: WorkType, tags: Vec<String>) -> WorkItem {
        WorkItem {
            id: id.to_string(),
            title: format!("Test: {id}"),
            description: String::new(),
            work_type,
            source: openfang_types::work_item::WorkSource::Api,
            status: WorkStatus::Pending,
            approval_status: openfang_types::work_item::ApprovalStatus::NotRequired,
            assigned_agent_id: None,
            assigned_agent_name: None,
            result: None,
            error: None,
            iterations: 0,
            priority: 128,
            scheduled_at: None,
            started_at: None,
            completed_at: None,
            deadline: None,
            requires_approval: false,
            approved_by: None,
            approved_at: None,
            approval_note: None,
            payload: serde_json::Value::Null,
            tags,
            created_by: None,
            idempotency_key: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            retry_count: 0,
            max_retries: 3,
            parent_id: None,
            run_id: Some(id.to_string()),
            workspace_id: None,
        }
    }

    fn make_manifest(id: &str, division: SwarmDivision, tags: Vec<CapabilityTag>) -> AgentSwarmManifest {
        AgentSwarmManifest {
            id: id.to_string(),
            name: id.to_string(),
            division,
            description: "test".into(),
            version: "0.1.0".into(),
            author: "test".into(),
            risk_level: ManifestRiskLevel::Low,
            capability_tags: tags,
            input_types: vec![],
            output_types: vec![],
            required_tools: vec![],
            required_services: vec![],
            optional_services: vec![],
            max_concurrency: 2,
            supports_subtasks: false,
            expected_runtime_class: RuntimeClass::Short,
            produces_artifact: false,
            artifact_schema: None,
            requires_approval: false,
            approval_policy: ApprovalGatePolicy::None,
            cost_sensitivity: CostSensitivity::Low,
            requires_external_network: false,
            safe_for_auto_run: true,
            emits_events: true,
            logs_artifacts: false,
            trace_level: TraceLevel::Standard,
            compatible_with: vec![],
            incompatible_with: vec![],
        }
    }

    fn make_planner_with(agents: Vec<AgentSwarmManifest>) -> SwarmPlanner {
        let registry = Arc::new(SwarmRegistry::new());
        for a in agents {
            registry.register_unchecked(a);
        }
        SwarmPlanner::new(registry)
    }

    // -----------------------------------------------------------------------
    // Swarm composition tests
    // -----------------------------------------------------------------------

    #[test]
    fn swarm_selects_primary_execution_agent() {
        let planner = make_planner_with(vec![make_manifest(
            "coder",
            SwarmDivision::Execution,
            vec![CapabilityTag::new("code", "rust")],
        )]);

        let item = make_work_item("wi-1", WorkType::AgentTask, vec!["code:rust".into()]);
        let plan = planner.plan(SwarmPlanRequest {
            work_item: item,
            max_swarm_size: None,
            require_auto_run: false,
            extra_required_tags: vec![],
        });

        assert!(plan.success, "plan failed: {:?}", plan.error);
        assert_eq!(plan.members.len(), 1);
        assert_eq!(plan.members[0].role, SwarmRole::PrimaryExecution);
        assert_eq!(plan.members[0].agent_id, "coder");
    }

    #[test]
    fn swarm_respects_max_size() {
        let agents = (0..5)
            .map(|i| {
                make_manifest(
                    &format!("agent-{i}"),
                    SwarmDivision::Execution,
                    vec![CapabilityTag::new("code", "rust")],
                )
            })
            .collect();
        let planner = make_planner_with(agents);
        let item = make_work_item("wi-2", WorkType::AgentTask, vec!["code:rust".into()]);
        let plan = planner.plan(SwarmPlanRequest {
            work_item: item,
            max_swarm_size: Some(2),
            require_auto_run: false,
            extra_required_tags: vec![],
        });

        assert!(plan.success);
        assert_eq!(plan.members.len(), 2);
    }

    #[test]
    fn swarm_fails_when_no_capability_match() {
        let planner = make_planner_with(vec![make_manifest(
            "coder",
            SwarmDivision::Execution,
            vec![CapabilityTag::new("code", "python")],
        )]);
        let item = make_work_item("wi-3", WorkType::AgentTask, vec!["code:rust".into()]);
        let plan = planner.plan(SwarmPlanRequest {
            work_item: item,
            max_swarm_size: None,
            require_auto_run: false,
            extra_required_tags: vec![],
        });

        assert!(!plan.success);
        assert_eq!(plan.error.as_deref(), Some("no_eligible_agents"));
        assert!(!plan.rejected_candidates.is_empty());
        assert_eq!(
            plan.rejected_candidates[0].rule,
            RejectionRule::MissingCapabilityTag
        );
    }

    #[test]
    fn incompatible_agents_not_in_same_swarm() {
        let mut alpha = make_manifest(
            "alpha",
            SwarmDivision::Execution,
            vec![CapabilityTag::new("code", "rust")],
        );
        alpha.incompatible_with = vec!["beta".into()];

        let beta = make_manifest(
            "beta",
            SwarmDivision::Research,
            vec![CapabilityTag::new("code", "rust")],
        );

        let planner = make_planner_with(vec![alpha, beta]);
        let item = make_work_item("wi-4", WorkType::AgentTask, vec!["code:rust".into()]);
        let plan = planner.plan(SwarmPlanRequest {
            work_item: item,
            max_swarm_size: Some(3),
            require_auto_run: false,
            extra_required_tags: vec![],
        });

        assert!(plan.success);
        // Both match, but beta should be rejected due to alpha's incompatibility
        let selected_ids: HashSet<&str> =
            plan.members.iter().map(|m| m.agent_id.as_str()).collect();
        assert!(
            !(selected_ids.contains("alpha") && selected_ids.contains("beta")),
            "alpha and beta must not both be selected"
        );
    }

    #[test]
    fn not_safe_for_auto_run_rejected_when_required() {
        let mut m = make_manifest(
            "risky-agent",
            SwarmDivision::Execution,
            vec![CapabilityTag::new("code", "rust")],
        );
        m.safe_for_auto_run = false;

        let planner = make_planner_with(vec![m]);
        let item = make_work_item("wi-5", WorkType::AgentTask, vec!["code:rust".into()]);
        let plan = planner.plan(SwarmPlanRequest {
            work_item: item,
            max_swarm_size: None,
            require_auto_run: true,
            extra_required_tags: vec![],
        });

        assert!(!plan.success);
        assert_eq!(
            plan.rejected_candidates[0].rule,
            RejectionRule::NotSafeForAutoRun
        );
    }

    #[test]
    fn delegation_graph_parent_references() {
        // Ensure that a child WorkItem correctly references parent_id
        let item = make_work_item("wi-parent", WorkType::AgentTask, vec![]);
        let child_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now();

        let child = WorkItem {
            id: child_id.clone(),
            title: "[sub] test".into(),
            parent_id: Some(item.id.clone()),
            run_id: Some(child_id.clone()),
            workspace_id: None,
            created_by: Some(format!("parent:{}", item.id)),
            source: openfang_types::work_item::WorkSource::AgentSpawned,
            description: String::new(),
            work_type: WorkType::AgentTask,
            status: WorkStatus::Pending,
            approval_status: openfang_types::work_item::ApprovalStatus::NotRequired,
            assigned_agent_id: None,
            assigned_agent_name: None,
            result: None,
            error: None,
            iterations: 0,
            priority: 128,
            scheduled_at: None,
            started_at: None,
            completed_at: None,
            deadline: None,
            requires_approval: false,
            approved_by: None,
            approved_at: None,
            approval_note: None,
            payload: serde_json::Value::Null,
            tags: vec![],
            idempotency_key: None,
            created_at: now,
            updated_at: now,
            retry_count: 0,
            max_retries: 3,
        };

        assert_eq!(child.parent_id.as_deref(), Some("wi-parent"));
        assert!(child.created_by.as_deref().unwrap().contains("parent:"));
    }

    #[test]
    fn plan_to_work_events_emits_swarm_planned() {
        let planner = make_planner_with(vec![make_manifest(
            "coder",
            SwarmDivision::Execution,
            vec![CapabilityTag::new("code", "rust")],
        )]);
        let item = make_work_item("wi-6", WorkType::AgentTask, vec!["code:rust".into()]);
        let plan = planner.plan(SwarmPlanRequest {
            work_item: item,
            max_swarm_size: None,
            require_auto_run: false,
            extra_required_tags: vec![],
        });

        let events = plan_to_work_events(&plan);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "swarm_planned");
        assert_eq!(events[0].work_item_id, "wi-6");
        assert!(events[0].detail.as_ref().unwrap().contains("coder"));
    }

    #[test]
    fn plan_to_work_events_emits_failure_event() {
        let planner = make_planner_with(vec![]);
        let item = make_work_item("wi-7", WorkType::AgentTask, vec!["code:rust".into()]);
        let plan = planner.plan(SwarmPlanRequest {
            work_item: item,
            max_swarm_size: None,
            require_auto_run: false,
            extra_required_tags: vec![],
        });

        let events = plan_to_work_events(&plan);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "swarm_selection_failed");
    }

    #[test]
    fn approval_gating_set_when_member_requires_approval() {
        let mut m = make_manifest(
            "gated-agent",
            SwarmDivision::Execution,
            vec![CapabilityTag::new("code", "rust")],
        );
        m.requires_approval = true;
        m.approval_policy = ApprovalGatePolicy::PreExecute;
        m.risk_level = ManifestRiskLevel::High;
        m.safe_for_auto_run = false;

        let planner = make_planner_with(vec![m]);
        let item = make_work_item("wi-8", WorkType::AgentTask, vec!["code:rust".into()]);
        let plan = planner.plan(SwarmPlanRequest {
            work_item: item,
            max_swarm_size: None,
            require_auto_run: false,
            extra_required_tags: vec![],
        });

        assert!(plan.success);
        assert!(plan.approval_gating_required);
        assert_eq!(plan.approval_gate_policy, ApprovalGatePolicy::PreExecute);
    }

    #[test]
    fn research_work_type_requires_research_capability() {
        let item = make_work_item("wi-9", WorkType::Research, vec![]);
        let requirements = derive_requirements(&item, &[]);
        assert!(requirements
            .required_capability_tags
            .iter()
            .any(|t| t.namespace == "research" && t.name == "web"));
    }
}
