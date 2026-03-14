//! Static persona registry for all built-in OpenFang subagent personas.
//!
//! This module defines every persona as a concrete Rust value, registers them
//! in a process-wide `LazyLock`, and exposes typed query helpers used by the
//! swarm planner, approval system, and dashboard.
//!
//! # Structure
//! - `PERSONA_REGISTRY` — the live registry (all 18 personas)
//! - `all_personas()` — slice accessor
//! - Query helpers — filter by ID, division, capability, work type, etc.
//! - `validate_registry()` — startup self-check
//! - `#[cfg(test)]` — validation and query tests

use openfang_types::persona::{
    AgentConstraints, AgentPersona, ApprovalRules, ExecutionContract, HandoffRules,
    ObservabilityPolicy, WorkOutputType,
};
use openfang_types::swarm::{
    ApprovalGatePolicy, CapabilityTag, CostSensitivity, ManifestRiskLevel, RuntimeClass,
    SwarmDivision, TraceLevel,
};
use openfang_types::work_item::WorkType;
use std::sync::LazyLock;

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Shorthand for building a [`CapabilityTag`] from a `"namespace:name"` string.
/// Panics at startup if the literal is malformed — this is intentional: bad
/// literals are a programming error, not a runtime error.
#[inline]
fn t(s: &str) -> CapabilityTag {
    CapabilityTag::parse(s)
        .unwrap_or_else(|| panic!("invalid capability tag literal in persona registry: {s:?}"))
}

// ---------------------------------------------------------------------------
// Coordination agents
// ---------------------------------------------------------------------------

fn orchestrator_delegate() -> AgentPersona {
    AgentPersona {
        id: "orchestrator_delegate",
        name: "Orchestrator Delegate",
        version: "1.0.0",
        division: SwarmDivision::Coordination,
        description: "Top-level coordinator that decomposes goals into routable work items.",
        core_focus: "Goal decomposition, swarm assembly, and cross-agent coordination.",
        capability_tags: vec![
            t("coordination:routing"),
            t("coordination:delegation"),
            t("coordination:orchestration"),
            t("coordination:goal_decomposition"),
        ],
        accepted_work_types: vec![WorkType::AgentTask, WorkType::Workflow],
        output_types: vec![WorkOutputType::RoutedWorkItem, WorkOutputType::ScheduledItem],
        required_tools: vec!["work_item_store", "registry_lookup", "event_bus"],
        required_services: vec![],
        optional_services: vec!["llm_api"],
        execution_contract: ExecutionContract {
            expected_runtime_class: RuntimeClass::Short,
            requires_approval: false,
            approval_policy: ApprovalGatePolicy::None,
            produces_artifact: false,
            artifact_schema_ref: None,
            supports_subtasks: true,
        },
        constraints: AgentConstraints {
            risk_level: ManifestRiskLevel::Low,
            cost_sensitivity: CostSensitivity::Low,
            safe_for_auto_run: true,
            max_concurrency: 8,
            compatible_with: vec![
                "task_router",
                "dependency_checker",
                "scheduler_operator",
                "audit_trail_agent",
            ],
            incompatible_with: vec![],
        },
        observability: ObservabilityPolicy {
            emits_events: true,
            trace_level: TraceLevel::Standard,
            logs_artifacts: false,
            required_event_types: vec!["swarm_planned", "delegated_to_subagent"],
        },
        handoff_rules: HandoffRules {
            can_receive_from: vec![
                "dependency_checker",
                "task_router",
                "qa_reviewer",
                "output_validator",
                "risk_checker",
                "scheduler_operator",
                "inbox_triage_agent",
                "policy_gate_agent",
                "audit_trail_agent",
            ],
            can_handoff_to: vec![
                "task_router",
                "dependency_checker",
                "workflow_executor",
                "content_builder",
                "inbox_triage_agent",
                "scheduler_operator",
                "approval_packet_builder",
            ],
            delegation_allowed: true,
        },
        approval_rules: ApprovalRules {
            must_preapprove: false,
            must_postapprove: false,
            approval_triggers: vec![],
        },
        implementation_notes: "Orchestrator Delegate is always the first agent in a multi-agent \
            swarm. It must emit a swarm_planned WorkEvent before delegating any child tasks. \
            It must not generate end-user-visible content directly.",
    }
}

fn task_router() -> AgentPersona {
    AgentPersona {
        id: "task_router",
        name: "Task Router",
        version: "1.0.0",
        division: SwarmDivision::Coordination,
        description: "Routes individual work items to the best-matched agent based on capability tags.",
        core_focus: "Capability-based work item routing and agent selection.",
        capability_tags: vec![
            t("coordination:routing"),
            t("coordination:capability_matching"),
            t("coordination:load_balancing"),
        ],
        accepted_work_types: vec![WorkType::AgentTask, WorkType::Workflow],
        output_types: vec![WorkOutputType::RoutedWorkItem],
        required_tools: vec!["capability_matcher", "registry_lookup", "work_item_store"],
        required_services: vec![],
        optional_services: vec![],
        execution_contract: ExecutionContract {
            expected_runtime_class: RuntimeClass::Short,
            requires_approval: false,
            approval_policy: ApprovalGatePolicy::None,
            produces_artifact: false,
            artifact_schema_ref: None,
            supports_subtasks: true,
        },
        constraints: AgentConstraints {
            risk_level: ManifestRiskLevel::Low,
            cost_sensitivity: CostSensitivity::Low,
            safe_for_auto_run: true,
            max_concurrency: 16,
            compatible_with: vec!["orchestrator_delegate", "dependency_checker"],
            incompatible_with: vec![],
        },
        observability: ObservabilityPolicy {
            emits_events: true,
            trace_level: TraceLevel::Standard,
            logs_artifacts: false,
            required_event_types: vec!["delegated_to_subagent", "status_changed"],
        },
        handoff_rules: HandoffRules {
            can_receive_from: vec!["orchestrator_delegate", "dependency_checker"],
            can_handoff_to: vec![
                "orchestrator_delegate",
                "research_analyst",
                "source_verifier",
                "brief_synthesizer",
                "workflow_executor",
                "api_integrator",
                "content_builder",
                "qa_reviewer",
                "output_validator",
                "risk_checker",
                "scheduler_operator",
                "inbox_triage_agent",
                "approval_packet_builder",
                "audit_trail_agent",
                "identity_lineage_agent",
                "policy_gate_agent",
            ],
            delegation_allowed: true,
        },
        approval_rules: ApprovalRules {
            must_preapprove: false,
            must_postapprove: false,
            approval_triggers: vec![],
        },
        implementation_notes: "Task Router selects the single best-fit agent using swarm registry \
            capability matching. It must never execute domain work itself. When no eligible agent \
            is found it must emit a swarm_selection_failed event and return the item to Pending.",
    }
}

fn dependency_checker() -> AgentPersona {
    AgentPersona {
        id: "dependency_checker",
        name: "Dependency Checker",
        version: "1.0.0",
        division: SwarmDivision::Coordination,
        description: "Validates that all WorkItem dependencies are satisfied before execution proceeds.",
        core_focus: "Dependency graph traversal and execution readiness verification.",
        capability_tags: vec![
            t("coordination:dependency_graph"),
            t("validation:readiness"),
            t("coordination:blocking"),
        ],
        accepted_work_types: vec![WorkType::AgentTask, WorkType::Workflow],
        output_types: vec![WorkOutputType::ValidationResult],
        required_tools: vec!["work_item_store", "graph_traversal"],
        required_services: vec![],
        optional_services: vec![],
        execution_contract: ExecutionContract {
            expected_runtime_class: RuntimeClass::Short,
            requires_approval: false,
            approval_policy: ApprovalGatePolicy::None,
            produces_artifact: false,
            artifact_schema_ref: None,
            supports_subtasks: false,
        },
        constraints: AgentConstraints {
            risk_level: ManifestRiskLevel::Low,
            cost_sensitivity: CostSensitivity::Low,
            safe_for_auto_run: true,
            max_concurrency: 8,
            compatible_with: vec!["orchestrator_delegate", "task_router"],
            incompatible_with: vec![],
        },
        observability: ObservabilityPolicy {
            emits_events: true,
            trace_level: TraceLevel::Minimal,
            logs_artifacts: false,
            required_event_types: vec!["status_changed"],
        },
        handoff_rules: HandoffRules {
            can_receive_from: vec!["orchestrator_delegate", "task_router"],
            can_handoff_to: vec!["orchestrator_delegate", "task_router"],
            delegation_allowed: false,
        },
        approval_rules: ApprovalRules {
            must_preapprove: false,
            must_postapprove: false,
            approval_triggers: vec![],
        },
        implementation_notes: "Dependency Checker must block downstream execution when any \
            required parent WorkItem is not in Completed or Approved status. It emits a \
            status_changed event to Pending if blocked.",
    }
}

// ---------------------------------------------------------------------------
// Research agents
// ---------------------------------------------------------------------------

fn research_analyst() -> AgentPersona {
    AgentPersona {
        id: "research_analyst",
        name: "Research Analyst",
        version: "1.0.0",
        division: SwarmDivision::Research,
        description: "Discovers and organises research insights required for a WorkItem.",
        core_focus: "Web research, knowledge retrieval, and structured insight generation.",
        capability_tags: vec![
            t("research:web"),
            t("research:analysis"),
            t("output:structured_insights"),
            t("research:knowledge_retrieval"),
        ],
        accepted_work_types: vec![WorkType::Research, WorkType::AgentTask],
        output_types: vec![WorkOutputType::StructuredInsights, WorkOutputType::Report],
        required_tools: vec!["web_search", "document_reader"],
        required_services: vec!["llm_api"],
        optional_services: vec!["search_provider", "knowledge_base"],
        execution_contract: ExecutionContract {
            expected_runtime_class: RuntimeClass::Medium,
            requires_approval: false,
            approval_policy: ApprovalGatePolicy::None,
            produces_artifact: true,
            artifact_schema_ref: Some("structured_insights_v1".to_string()),
            supports_subtasks: false,
        },
        constraints: AgentConstraints {
            risk_level: ManifestRiskLevel::Low,
            cost_sensitivity: CostSensitivity::Medium,
            safe_for_auto_run: true,
            max_concurrency: 4,
            compatible_with: vec!["source_verifier", "brief_synthesizer", "qa_reviewer"],
            incompatible_with: vec!["api_integrator"],
        },
        observability: ObservabilityPolicy {
            emits_events: true,
            trace_level: TraceLevel::Standard,
            logs_artifacts: true,
            required_event_types: vec!["started", "completed"],
        },
        handoff_rules: HandoffRules {
            can_receive_from: vec!["task_router", "source_verifier"],
            can_handoff_to: vec!["source_verifier", "brief_synthesizer", "qa_reviewer"],
            delegation_allowed: false,
        },
        approval_rules: ApprovalRules {
            must_preapprove: false,
            must_postapprove: false,
            approval_triggers: vec![],
        },
        implementation_notes: "Research Analyst must not make write-back API calls or modify \
            external state. All research outputs must be saved as WorkEvent detail payloads \
            before handing off to Source Verifier.",
    }
}

fn source_verifier() -> AgentPersona {
    AgentPersona {
        id: "source_verifier",
        name: "Source Verifier",
        version: "1.0.0",
        division: SwarmDivision::Research,
        description: "Validates source credibility, checks citations, and flags unverified claims.",
        core_focus: "Source validation, citation accuracy, and claim verification.",
        capability_tags: vec![
            t("research:verification"),
            t("validation:sources"),
            t("research:citations"),
            t("research:fact_checking"),
        ],
        accepted_work_types: vec![WorkType::Research, WorkType::AgentTask],
        output_types: vec![WorkOutputType::ValidationResult, WorkOutputType::StructuredInsights],
        required_tools: vec!["link_checker", "citation_validator", "web_search"],
        required_services: vec!["llm_api"],
        optional_services: vec!["search_provider"],
        execution_contract: ExecutionContract {
            expected_runtime_class: RuntimeClass::Medium,
            requires_approval: false,
            approval_policy: ApprovalGatePolicy::None,
            produces_artifact: false,
            artifact_schema_ref: None,
            supports_subtasks: false,
        },
        constraints: AgentConstraints {
            risk_level: ManifestRiskLevel::Low,
            cost_sensitivity: CostSensitivity::Low,
            safe_for_auto_run: true,
            max_concurrency: 4,
            compatible_with: vec!["research_analyst", "brief_synthesizer"],
            incompatible_with: vec![],
        },
        observability: ObservabilityPolicy {
            emits_events: true,
            trace_level: TraceLevel::Standard,
            logs_artifacts: false,
            required_event_types: vec!["started", "completed"],
        },
        handoff_rules: HandoffRules {
            can_receive_from: vec!["task_router", "research_analyst"],
            can_handoff_to: vec!["research_analyst", "brief_synthesizer"],
            delegation_allowed: false,
        },
        approval_rules: ApprovalRules {
            must_preapprove: false,
            must_postapprove: false,
            approval_triggers: vec![],
        },
        implementation_notes: "Source Verifier must tag each source with a confidence score \
            (high/medium/low/unverified) in its output. Unverified claims must block brief \
            synthesis unless explicitly overridden by an approved WorkItem flag.",
    }
}

fn brief_synthesizer() -> AgentPersona {
    AgentPersona {
        id: "brief_synthesizer",
        name: "Brief Synthesizer",
        version: "1.0.0",
        division: SwarmDivision::Research,
        description: "Synthesises research findings into coherent, structured briefs.",
        core_focus: "Narrative synthesis, executive summarisation, and brief generation.",
        capability_tags: vec![
            t("research:synthesis"),
            t("output:brief"),
            t("output:summary"),
            t("research:summarisation"),
        ],
        accepted_work_types: vec![WorkType::Research, WorkType::AgentTask],
        output_types: vec![WorkOutputType::RefinedBrief, WorkOutputType::Report],
        required_tools: vec!["llm_runner", "document_writer"],
        required_services: vec!["llm_api"],
        optional_services: vec![],
        execution_contract: ExecutionContract {
            expected_runtime_class: RuntimeClass::Medium,
            requires_approval: true,
            approval_policy: ApprovalGatePolicy::PostDraft,
            produces_artifact: true,
            artifact_schema_ref: Some("refined_brief_v1".to_string()),
            supports_subtasks: false,
        },
        constraints: AgentConstraints {
            risk_level: ManifestRiskLevel::Low,
            cost_sensitivity: CostSensitivity::Medium,
            safe_for_auto_run: false,
            max_concurrency: 4,
            compatible_with: vec!["research_analyst", "source_verifier", "qa_reviewer"],
            incompatible_with: vec![],
        },
        observability: ObservabilityPolicy {
            emits_events: true,
            trace_level: TraceLevel::Standard,
            logs_artifacts: true,
            required_event_types: vec!["started", "approval_requested", "completed"],
        },
        handoff_rules: HandoffRules {
            can_receive_from: vec!["task_router", "research_analyst", "source_verifier", "content_builder"],
            can_handoff_to: vec!["qa_reviewer", "output_validator", "approval_packet_builder"],
            delegation_allowed: false,
        },
        approval_rules: ApprovalRules {
            must_preapprove: false,
            must_postapprove: true,
            approval_triggers: vec!["draft_complete"],
        },
        implementation_notes: "Brief Synthesizer transitions the WorkItem to WaitingApproval \
            after producing its draft. A downstream QA Reviewer or human actor must approve \
            before the brief is finalised.",
    }
}

// ---------------------------------------------------------------------------
// Execution agents
// ---------------------------------------------------------------------------

fn workflow_executor() -> AgentPersona {
    AgentPersona {
        id: "workflow_executor",
        name: "Workflow Executor",
        version: "1.0.0",
        division: SwarmDivision::Execution,
        description: "Orchestrates and runs complex multi-step workflows on behalf of a work item.",
        core_focus: "Sequential and parallel workflow execution with state tracking.",
        capability_tags: vec![
            t("execution:workflow"),
            t("execution:orchestration"),
            t("execution:multi_step"),
            t("output:artifact"),
        ],
        accepted_work_types: vec![WorkType::Workflow, WorkType::AgentTask],
        output_types: vec![WorkOutputType::Artifact, WorkOutputType::Report],
        required_tools: vec!["workflow_engine", "task_runner", "event_bus", "work_item_store"],
        required_services: vec!["llm_api"],
        optional_services: vec![],
        execution_contract: ExecutionContract {
            expected_runtime_class: RuntimeClass::Long,
            requires_approval: true,
            approval_policy: ApprovalGatePolicy::PreExecute,
            produces_artifact: true,
            artifact_schema_ref: None,
            supports_subtasks: true,
        },
        constraints: AgentConstraints {
            risk_level: ManifestRiskLevel::Medium,
            cost_sensitivity: CostSensitivity::Medium,
            safe_for_auto_run: false,
            max_concurrency: 4,
            compatible_with: vec![
                "qa_reviewer",
                "output_validator",
                "risk_checker",
                "audit_trail_agent",
                "policy_gate_agent",
            ],
            incompatible_with: vec![],
        },
        observability: ObservabilityPolicy {
            emits_events: true,
            trace_level: TraceLevel::Verbose,
            logs_artifacts: true,
            required_event_types: vec!["started", "approval_requested", "completed", "failed"],
        },
        handoff_rules: HandoffRules {
            can_receive_from: vec!["orchestrator_delegate", "task_router"],
            can_handoff_to: vec![
                "qa_reviewer",
                "output_validator",
                "risk_checker",
                "audit_trail_agent",
                "policy_gate_agent",
            ],
            delegation_allowed: true,
        },
        approval_rules: ApprovalRules {
            must_preapprove: true,
            must_postapprove: false,
            approval_triggers: vec!["execution_planned"],
        },
        implementation_notes: "Workflow Executor must gate on pre-execute approval before \
            touching any external service or persisted state. Every workflow step must emit \
            a WorkEvent so the audit trail is complete. On failure, the item must transition \
            to Failed before any retry.",
    }
}

fn api_integrator() -> AgentPersona {
    AgentPersona {
        id: "api_integrator",
        name: "API Integrator",
        version: "1.0.0",
        division: SwarmDivision::Execution,
        description: "Connects to external APIs and services to fulfil a work item.",
        core_focus: "External API calls, webhook dispatch, and service integration.",
        capability_tags: vec![
            t("execution:api_call"),
            t("integration:external"),
            t("integration:webhook"),
            t("output:artifact"),
        ],
        accepted_work_types: vec![WorkType::AgentTask, WorkType::Transformation],
        output_types: vec![WorkOutputType::Artifact, WorkOutputType::Notification],
        required_tools: vec!["http_client", "webhook_dispatcher", "api_caller"],
        required_services: vec![],
        optional_services: vec!["external_api", "llm_api"],
        execution_contract: ExecutionContract {
            expected_runtime_class: RuntimeClass::Short,
            requires_approval: true,
            approval_policy: ApprovalGatePolicy::PreExecute,
            produces_artifact: true,
            artifact_schema_ref: None,
            supports_subtasks: false,
        },
        constraints: AgentConstraints {
            risk_level: ManifestRiskLevel::High,
            cost_sensitivity: CostSensitivity::High,
            safe_for_auto_run: false,
            max_concurrency: 2,
            compatible_with: vec![
                "output_validator",
                "risk_checker",
                "audit_trail_agent",
                "policy_gate_agent",
            ],
            incompatible_with: vec!["research_analyst"],
        },
        observability: ObservabilityPolicy {
            emits_events: true,
            trace_level: TraceLevel::Verbose,
            logs_artifacts: true,
            required_event_types: vec!["started", "approval_requested", "completed", "failed"],
        },
        handoff_rules: HandoffRules {
            can_receive_from: vec!["orchestrator_delegate", "task_router"],
            can_handoff_to: vec![
                "output_validator",
                "risk_checker",
                "audit_trail_agent",
                "policy_gate_agent",
            ],
            delegation_allowed: false,
        },
        approval_rules: ApprovalRules {
            must_preapprove: true,
            must_postapprove: false,
            approval_triggers: vec!["external_call_planned"],
        },
        implementation_notes: "API Integrator must always be paired with audit_trail_agent \
            or policy_gate_agent in any production swarm. No external call may proceed \
            without a pre-execute approval WorkEvent on the parent WorkItem. Credentials \
            must be sourced from the secrets vault, never from the WorkItem payload.",
    }
}

fn content_builder() -> AgentPersona {
    AgentPersona {
        id: "content_builder",
        name: "Content Builder",
        version: "1.0.0",
        division: SwarmDivision::Execution,
        description: "Generates structured content, documents, and formatted outputs.",
        core_focus: "LLM-driven content generation and document formatting.",
        capability_tags: vec![
            t("execution:generation"),
            t("output:document"),
            t("output:artifact"),
            t("execution:llm_generation"),
        ],
        accepted_work_types: vec![WorkType::Generation, WorkType::AgentTask],
        output_types: vec![
            WorkOutputType::Artifact,
            WorkOutputType::Report,
            WorkOutputType::RefinedBrief,
        ],
        required_tools: vec!["llm_runner", "document_writer"],
        required_services: vec!["llm_api"],
        optional_services: vec![],
        execution_contract: ExecutionContract {
            expected_runtime_class: RuntimeClass::Medium,
            requires_approval: true,
            approval_policy: ApprovalGatePolicy::PostDraft,
            produces_artifact: true,
            artifact_schema_ref: None,
            supports_subtasks: false,
        },
        constraints: AgentConstraints {
            risk_level: ManifestRiskLevel::Low,
            cost_sensitivity: CostSensitivity::Medium,
            safe_for_auto_run: false,
            max_concurrency: 6,
            compatible_with: vec!["qa_reviewer", "output_validator", "brief_synthesizer"],
            incompatible_with: vec![],
        },
        observability: ObservabilityPolicy {
            emits_events: true,
            trace_level: TraceLevel::Standard,
            logs_artifacts: true,
            required_event_types: vec!["started", "approval_requested", "completed"],
        },
        handoff_rules: HandoffRules {
            can_receive_from: vec!["orchestrator_delegate", "task_router"],
            can_handoff_to: vec![
                "qa_reviewer",
                "output_validator",
                "brief_synthesizer",
                "approval_packet_builder",
            ],
            delegation_allowed: false,
        },
        approval_rules: ApprovalRules {
            must_preapprove: false,
            must_postapprove: true,
            approval_triggers: vec!["draft_complete"],
        },
        implementation_notes: "Content Builder transitions to WaitingApproval after producing \
            its draft. Downstream QA Reviewer or Output Validator must sign off before the \
            content is written back to the work item result.",
    }
}

// ---------------------------------------------------------------------------
// Quality agents
// ---------------------------------------------------------------------------

fn qa_reviewer() -> AgentPersona {
    AgentPersona {
        id: "qa_reviewer",
        name: "QA Reviewer",
        version: "1.0.0",
        division: SwarmDivision::Quality,
        description: "Performs holistic quality review of work outputs before finalisation.",
        core_focus: "Output quality scoring, issue detection, and review recommendations.",
        capability_tags: vec![
            t("quality:review"),
            t("validation:holistic"),
            t("output:review_report"),
            t("quality:scoring"),
        ],
        accepted_work_types: vec![WorkType::AgentTask],
        output_types: vec![WorkOutputType::ValidationResult, WorkOutputType::Report],
        required_tools: vec!["diff_checker", "quality_scorer", "llm_runner"],
        required_services: vec!["llm_api"],
        optional_services: vec![],
        execution_contract: ExecutionContract {
            expected_runtime_class: RuntimeClass::Short,
            requires_approval: false,
            approval_policy: ApprovalGatePolicy::None,
            produces_artifact: false,
            artifact_schema_ref: None,
            supports_subtasks: false,
        },
        constraints: AgentConstraints {
            risk_level: ManifestRiskLevel::Low,
            cost_sensitivity: CostSensitivity::Low,
            safe_for_auto_run: true,
            max_concurrency: 8,
            compatible_with: vec![
                "output_validator",
                "brief_synthesizer",
                "content_builder",
                "workflow_executor",
            ],
            incompatible_with: vec![],
        },
        observability: ObservabilityPolicy {
            emits_events: true,
            trace_level: TraceLevel::Standard,
            logs_artifacts: false,
            required_event_types: vec!["started", "completed"],
        },
        handoff_rules: HandoffRules {
            can_receive_from: vec![
                "task_router",
                "research_analyst",
                "brief_synthesizer",
                "workflow_executor",
                "content_builder",
                "output_validator",
            ],
            can_handoff_to: vec!["orchestrator_delegate", "task_router", "approval_packet_builder"],
            delegation_allowed: false,
        },
        approval_rules: ApprovalRules {
            must_preapprove: false,
            must_postapprove: false,
            approval_triggers: vec![],
        },
        implementation_notes: "QA Reviewer is the canonical quality gate. A failed review \
            must produce a ValidationResult containing specific findings, not a generic \
            failure message. The reviewer must not rewrite content — only flag and score.",
    }
}

fn output_validator() -> AgentPersona {
    AgentPersona {
        id: "output_validator",
        name: "Output Validator",
        version: "1.0.0",
        division: SwarmDivision::Quality,
        description: "Validates work outputs against expected schemas and structural contracts.",
        core_focus: "Schema validation, type checking, and structural contract enforcement.",
        capability_tags: vec![
            t("quality:schema_validation"),
            t("validation:contract"),
            t("validation:type_checking"),
            t("output:validation_result"),
        ],
        accepted_work_types: vec![WorkType::AgentTask, WorkType::Transformation],
        output_types: vec![WorkOutputType::ValidationResult],
        required_tools: vec!["schema_validator", "type_checker", "json_schema_engine"],
        required_services: vec![],
        optional_services: vec!["llm_api"],
        execution_contract: ExecutionContract {
            expected_runtime_class: RuntimeClass::Short,
            requires_approval: false,
            approval_policy: ApprovalGatePolicy::None,
            produces_artifact: false,
            artifact_schema_ref: None,
            supports_subtasks: false,
        },
        constraints: AgentConstraints {
            risk_level: ManifestRiskLevel::Low,
            cost_sensitivity: CostSensitivity::Low,
            safe_for_auto_run: true,
            max_concurrency: 16,
            compatible_with: vec!["qa_reviewer", "output_validator"],
            incompatible_with: vec![],
        },
        observability: ObservabilityPolicy {
            emits_events: true,
            trace_level: TraceLevel::Minimal,
            logs_artifacts: false,
            required_event_types: vec!["completed"],
        },
        handoff_rules: HandoffRules {
            can_receive_from: vec![
                "task_router",
                "research_analyst",
                "workflow_executor",
                "api_integrator",
                "content_builder",
            ],
            can_handoff_to: vec!["orchestrator_delegate", "task_router", "qa_reviewer"],
            delegation_allowed: false,
        },
        approval_rules: ApprovalRules {
            must_preapprove: false,
            must_postapprove: false,
            approval_triggers: vec![],
        },
        implementation_notes: "Output Validator is stateless and deterministic. It must \
            not use LLM calls for structural validation — LLM use is limited to semantic \
            contract checking when schema alone is insufficient.",
    }
}

fn risk_checker() -> AgentPersona {
    AgentPersona {
        id: "risk_checker",
        name: "Risk Checker",
        version: "1.0.0",
        division: SwarmDivision::Quality,
        description: "Assesses the risk of executing a work item and flags concerns for escalation.",
        core_focus: "Risk scoring, taint analysis, and escalation triggering.",
        capability_tags: vec![
            t("quality:risk_assessment"),
            t("compliance:policy"),
            t("validation:taint"),
            t("output:risk_report"),
        ],
        accepted_work_types: vec![WorkType::AgentTask, WorkType::Workflow],
        output_types: vec![WorkOutputType::ValidationResult, WorkOutputType::PolicyDecision],
        required_tools: vec!["risk_scorer", "taint_analyzer"],
        required_services: vec![],
        optional_services: vec!["policy_engine", "llm_api"],
        execution_contract: ExecutionContract {
            expected_runtime_class: RuntimeClass::Short,
            requires_approval: false,
            approval_policy: ApprovalGatePolicy::Conditional,
            produces_artifact: false,
            artifact_schema_ref: None,
            supports_subtasks: false,
        },
        constraints: AgentConstraints {
            risk_level: ManifestRiskLevel::Medium,
            cost_sensitivity: CostSensitivity::Low,
            safe_for_auto_run: true,
            max_concurrency: 8,
            compatible_with: vec![
                "policy_gate_agent",
                "audit_trail_agent",
                "workflow_executor",
                "api_integrator",
            ],
            incompatible_with: vec![],
        },
        observability: ObservabilityPolicy {
            emits_events: true,
            trace_level: TraceLevel::Standard,
            logs_artifacts: false,
            required_event_types: vec!["started", "completed"],
        },
        handoff_rules: HandoffRules {
            can_receive_from: vec!["task_router", "workflow_executor", "api_integrator"],
            can_handoff_to: vec!["policy_gate_agent", "orchestrator_delegate", "approval_packet_builder"],
            delegation_allowed: false,
        },
        approval_rules: ApprovalRules {
            must_preapprove: false,
            must_postapprove: false,
            approval_triggers: vec!["risk_level_high"],
        },
        implementation_notes: "When risk_level_high is detected, Risk Checker must transition \
            the WorkItem to WaitingApproval before returning control and must route to \
            policy_gate_agent or approval_packet_builder for human review.",
    }
}

// ---------------------------------------------------------------------------
// Operations agents
// ---------------------------------------------------------------------------

fn scheduler_operator() -> AgentPersona {
    AgentPersona {
        id: "scheduler_operator",
        name: "Scheduler Operator",
        version: "1.0.0",
        division: SwarmDivision::Operations,
        description: "Manages work item scheduling, cron triggers, and deferred execution.",
        core_focus: "Time-based scheduling, cron management, and deferred execution control.",
        capability_tags: vec![
            t("operations:scheduling"),
            t("operations:cron"),
            t("output:schedule"),
            t("operations:deferred_execution"),
        ],
        accepted_work_types: vec![WorkType::AgentTask, WorkType::Custom],
        output_types: vec![WorkOutputType::ScheduledItem, WorkOutputType::Notification],
        required_tools: vec!["cron_engine", "scheduler", "work_item_store"],
        required_services: vec![],
        optional_services: vec!["notification_gateway"],
        execution_contract: ExecutionContract {
            expected_runtime_class: RuntimeClass::Short,
            requires_approval: false,
            approval_policy: ApprovalGatePolicy::None,
            produces_artifact: false,
            artifact_schema_ref: None,
            supports_subtasks: false,
        },
        constraints: AgentConstraints {
            risk_level: ManifestRiskLevel::Low,
            cost_sensitivity: CostSensitivity::Low,
            safe_for_auto_run: true,
            max_concurrency: 4,
            compatible_with: vec!["orchestrator_delegate", "inbox_triage_agent"],
            incompatible_with: vec![],
        },
        observability: ObservabilityPolicy {
            emits_events: true,
            trace_level: TraceLevel::Minimal,
            logs_artifacts: false,
            required_event_types: vec!["status_changed"],
        },
        handoff_rules: HandoffRules {
            can_receive_from: vec!["orchestrator_delegate", "task_router"],
            can_handoff_to: vec!["orchestrator_delegate", "task_router"],
            delegation_allowed: false,
        },
        approval_rules: ApprovalRules {
            must_preapprove: false,
            must_postapprove: false,
            approval_triggers: vec![],
        },
        implementation_notes: "Scheduler Operator must persist every cron entry to the \
            scheduler store before confirming the work item as Completed. Cancellation \
            of a scheduled item must also cancel the cron entry atomically.",
    }
}

fn inbox_triage_agent() -> AgentPersona {
    AgentPersona {
        id: "inbox_triage_agent",
        name: "Inbox Triage Agent",
        version: "1.0.0",
        division: SwarmDivision::Operations,
        description: "Triages incoming messages and converts them to prioritised work items.",
        core_focus: "Inbox monitoring, priority classification, and work item creation.",
        capability_tags: vec![
            t("operations:triage"),
            t("operations:inbox"),
            t("output:routed_item"),
            t("operations:priority_classification"),
        ],
        accepted_work_types: vec![WorkType::AgentTask],
        output_types: vec![WorkOutputType::RoutedWorkItem, WorkOutputType::Notification],
        required_tools: vec!["inbox_reader", "priority_classifier", "work_item_store"],
        required_services: vec![],
        optional_services: vec!["email_provider", "webhook_gateway"],
        execution_contract: ExecutionContract {
            expected_runtime_class: RuntimeClass::Short,
            requires_approval: false,
            approval_policy: ApprovalGatePolicy::None,
            produces_artifact: false,
            artifact_schema_ref: None,
            supports_subtasks: false,
        },
        constraints: AgentConstraints {
            risk_level: ManifestRiskLevel::Low,
            cost_sensitivity: CostSensitivity::Low,
            safe_for_auto_run: true,
            max_concurrency: 8,
            compatible_with: vec!["orchestrator_delegate", "scheduler_operator", "approval_packet_builder"],
            incompatible_with: vec![],
        },
        observability: ObservabilityPolicy {
            emits_events: true,
            trace_level: TraceLevel::Minimal,
            logs_artifacts: false,
            required_event_types: vec!["status_changed"],
        },
        handoff_rules: HandoffRules {
            can_receive_from: vec!["orchestrator_delegate", "task_router"],
            can_handoff_to: vec!["orchestrator_delegate", "task_router", "approval_packet_builder"],
            delegation_allowed: false,
        },
        approval_rules: ApprovalRules {
            must_preapprove: false,
            must_postapprove: false,
            approval_triggers: vec![],
        },
        implementation_notes: "Inbox Triage Agent must classify urgency and domain from the \
            raw message body before routing. Sensitive messages (legal, financial, security) \
            must be flagged for approval_packet_builder rather than routed directly.",
    }
}

fn approval_packet_builder() -> AgentPersona {
    AgentPersona {
        id: "approval_packet_builder",
        name: "Approval Packet Builder",
        version: "1.0.0",
        division: SwarmDivision::Operations,
        description: "Assembles all context needed for a human approval review into a single packet.",
        core_focus: "Approval context packaging, evidence collection, and review preparation.",
        capability_tags: vec![
            t("operations:approval_packaging"),
            t("output:approval_packet"),
            t("compliance:review_ready"),
            t("operations:context_assembly"),
        ],
        accepted_work_types: vec![WorkType::AgentTask, WorkType::Workflow],
        output_types: vec![WorkOutputType::ApprovalPacket],
        required_tools: vec!["document_writer", "diff_checker", "work_item_store"],
        required_services: vec![],
        optional_services: vec!["llm_api"],
        execution_contract: ExecutionContract {
            expected_runtime_class: RuntimeClass::Short,
            requires_approval: false,
            approval_policy: ApprovalGatePolicy::None,
            produces_artifact: true,
            artifact_schema_ref: Some("approval_packet_v1".to_string()),
            supports_subtasks: false,
        },
        constraints: AgentConstraints {
            risk_level: ManifestRiskLevel::Low,
            cost_sensitivity: CostSensitivity::Low,
            safe_for_auto_run: true,
            max_concurrency: 4,
            compatible_with: vec![
                "policy_gate_agent",
                "audit_trail_agent",
                "qa_reviewer",
                "risk_checker",
            ],
            incompatible_with: vec![],
        },
        observability: ObservabilityPolicy {
            emits_events: true,
            trace_level: TraceLevel::Standard,
            logs_artifacts: true,
            required_event_types: vec!["started", "completed"],
        },
        handoff_rules: HandoffRules {
            can_receive_from: vec![
                "orchestrator_delegate",
                "task_router",
                "brief_synthesizer",
                "content_builder",
                "qa_reviewer",
                "risk_checker",
                "inbox_triage_agent",
            ],
            can_handoff_to: vec!["policy_gate_agent", "audit_trail_agent"],
            delegation_allowed: false,
        },
        approval_rules: ApprovalRules {
            must_preapprove: false,
            must_postapprove: false,
            approval_triggers: vec![],
        },
        implementation_notes: "Approval Packet Builder collects the work item, all related \
            events, diffs, risk assessments, and agent outputs into a single reviewable packet. \
            It must never make a policy decision itself — it only packages context for human \
            or policy_gate_agent review.",
    }
}

// ---------------------------------------------------------------------------
// Trust agents
// ---------------------------------------------------------------------------

fn audit_trail_agent() -> AgentPersona {
    AgentPersona {
        id: "audit_trail_agent",
        name: "Audit Trail Agent",
        version: "1.0.0",
        division: SwarmDivision::Trust,
        description: "Records and serves tamper-evident audit logs for all swarm actions.",
        core_focus: "Audit log creation, event persistence, and compliance trace generation.",
        capability_tags: vec![
            t("trust:audit"),
            t("compliance:audit_trail"),
            t("output:audit_log"),
            t("trust:tamper_evidence"),
        ],
        accepted_work_types: vec![WorkType::AgentTask],
        output_types: vec![WorkOutputType::AuditTrail, WorkOutputType::Report],
        required_tools: vec!["event_store", "work_item_store"],
        required_services: vec![],
        optional_services: vec!["manifest_signer"],
        execution_contract: ExecutionContract {
            expected_runtime_class: RuntimeClass::Short,
            requires_approval: false,
            approval_policy: ApprovalGatePolicy::None,
            produces_artifact: true,
            artifact_schema_ref: Some("audit_trail_v1".to_string()),
            supports_subtasks: false,
        },
        constraints: AgentConstraints {
            risk_level: ManifestRiskLevel::Low,
            cost_sensitivity: CostSensitivity::Low,
            safe_for_auto_run: true,
            max_concurrency: 16,
            compatible_with: vec![
                "identity_lineage_agent",
                "policy_gate_agent",
                "approval_packet_builder",
            ],
            incompatible_with: vec![],
        },
        observability: ObservabilityPolicy {
            emits_events: true,
            trace_level: TraceLevel::Verbose,
            logs_artifacts: true,
            required_event_types: vec!["completed"],
        },
        handoff_rules: HandoffRules {
            can_receive_from: vec![
                "task_router",
                "workflow_executor",
                "api_integrator",
                "approval_packet_builder",
                "identity_lineage_agent",
                "policy_gate_agent",
            ],
            can_handoff_to: vec!["orchestrator_delegate"],
            delegation_allowed: false,
        },
        approval_rules: ApprovalRules {
            must_preapprove: false,
            must_postapprove: false,
            approval_triggers: vec![],
        },
        implementation_notes: "Audit Trail Agent is purely observational. It must not modify \
            any existing WorkItem or event. Every entry it creates must include a content hash \
            of the referenced event for tamper detection. It should be included in every swarm \
            that involves an api_integrator or workflow_executor.",
    }
}

fn identity_lineage_agent() -> AgentPersona {
    AgentPersona {
        id: "identity_lineage_agent",
        name: "Identity & Lineage Agent",
        version: "1.0.0",
        division: SwarmDivision::Trust,
        description: "Traces and validates agent identity claims and work item provenance.",
        core_focus: "Agent identity verification, work item lineage, and provenance graph construction.",
        capability_tags: vec![
            t("trust:identity"),
            t("trust:lineage"),
            t("compliance:provenance"),
            t("trust:verification"),
        ],
        accepted_work_types: vec![WorkType::AgentTask],
        output_types: vec![WorkOutputType::AuditTrail, WorkOutputType::ValidationResult],
        required_tools: vec!["identity_store", "lineage_graph", "work_item_store"],
        required_services: vec![],
        optional_services: vec!["manifest_signer"],
        execution_contract: ExecutionContract {
            expected_runtime_class: RuntimeClass::Short,
            requires_approval: false,
            approval_policy: ApprovalGatePolicy::None,
            produces_artifact: false,
            artifact_schema_ref: None,
            supports_subtasks: false,
        },
        constraints: AgentConstraints {
            risk_level: ManifestRiskLevel::Medium,
            cost_sensitivity: CostSensitivity::Low,
            safe_for_auto_run: true,
            max_concurrency: 8,
            compatible_with: vec!["audit_trail_agent", "policy_gate_agent"],
            incompatible_with: vec![],
        },
        observability: ObservabilityPolicy {
            emits_events: true,
            trace_level: TraceLevel::Verbose,
            logs_artifacts: false,
            required_event_types: vec!["started", "completed"],
        },
        handoff_rules: HandoffRules {
            can_receive_from: vec!["task_router"],
            can_handoff_to: vec!["audit_trail_agent", "policy_gate_agent"],
            delegation_allowed: false,
        },
        approval_rules: ApprovalRules {
            must_preapprove: false,
            must_postapprove: false,
            approval_triggers: vec![],
        },
        implementation_notes: "Identity Lineage Agent must validate that every agent in a \
            swarm has a registered identity before the swarm runs. A failed lineage check \
            must block the swarm and route to policy_gate_agent for escalation.",
    }
}

fn policy_gate_agent() -> AgentPersona {
    AgentPersona {
        id: "policy_gate_agent",
        name: "Policy Gate Agent",
        version: "1.0.0",
        division: SwarmDivision::Trust,
        description: "Enforces policy rules and acts as the final authorisation gate before risky operations.",
        core_focus: "Policy enforcement, taint analysis, and pre-execution authorisation.",
        capability_tags: vec![
            t("trust:policy_enforcement"),
            t("compliance:policy_gate"),
            t("output:policy_decision"),
            t("trust:authorisation"),
        ],
        accepted_work_types: vec![WorkType::AgentTask, WorkType::Workflow],
        output_types: vec![WorkOutputType::PolicyDecision, WorkOutputType::ValidationResult],
        required_tools: vec!["policy_engine", "taint_analyzer", "work_item_store"],
        required_services: vec![],
        optional_services: vec!["llm_api", "notification_gateway"],
        execution_contract: ExecutionContract {
            expected_runtime_class: RuntimeClass::Short,
            requires_approval: true,
            approval_policy: ApprovalGatePolicy::PreExecute,
            produces_artifact: false,
            artifact_schema_ref: None,
            supports_subtasks: false,
        },
        constraints: AgentConstraints {
            risk_level: ManifestRiskLevel::High,
            cost_sensitivity: CostSensitivity::Low,
            safe_for_auto_run: false,
            max_concurrency: 4,
            compatible_with: vec![
                "audit_trail_agent",
                "identity_lineage_agent",
                "risk_checker",
                "approval_packet_builder",
            ],
            incompatible_with: vec![],
        },
        observability: ObservabilityPolicy {
            emits_events: true,
            trace_level: TraceLevel::Verbose,
            logs_artifacts: false,
            required_event_types: vec!["started", "approval_requested", "completed", "failed"],
        },
        handoff_rules: HandoffRules {
            can_receive_from: vec![
                "task_router",
                "workflow_executor",
                "api_integrator",
                "risk_checker",
                "approval_packet_builder",
                "identity_lineage_agent",
            ],
            can_handoff_to: vec!["orchestrator_delegate", "audit_trail_agent"],
            delegation_allowed: false,
        },
        approval_rules: ApprovalRules {
            must_preapprove: true,
            must_postapprove: false,
            approval_triggers: vec!["high_risk_detected", "policy_violation_detected"],
        },
        implementation_notes: "Policy Gate Agent is the final authority before any high-risk \
            operation proceeds. A deny decision must transition the work item to Rejected and \
            emit both a policy_decision WorkEvent and an audit_trail_agent notification. \
            It must never be bypassed — even by Orchestrator Delegate.",
    }
}

// ---------------------------------------------------------------------------
// Static persona registry
// ---------------------------------------------------------------------------

/// Process-wide registry of all built-in OpenFang subagent personas.
///
/// Initialised once on first access via [`std::sync::LazyLock`].
/// All 18 core personas are present at startup.
pub static PERSONA_REGISTRY: LazyLock<Vec<AgentPersona>> = LazyLock::new(|| {
    vec![
        // Coordination
        orchestrator_delegate(),
        task_router(),
        dependency_checker(),
        // Research
        research_analyst(),
        source_verifier(),
        brief_synthesizer(),
        // Execution
        workflow_executor(),
        api_integrator(),
        content_builder(),
        // Quality
        qa_reviewer(),
        output_validator(),
        risk_checker(),
        // Operations
        scheduler_operator(),
        inbox_triage_agent(),
        approval_packet_builder(),
        // Trust
        audit_trail_agent(),
        identity_lineage_agent(),
        policy_gate_agent(),
    ]
});

/// Returns a slice over all registered personas.
pub fn all_personas() -> &'static [AgentPersona] {
    &PERSONA_REGISTRY
}

// ---------------------------------------------------------------------------
// Query helpers — used by the swarm planner and API routes
// ---------------------------------------------------------------------------

/// Find a persona by its unique string ID.
///
/// Returns `None` if no persona with `id` is registered.
pub fn persona_by_id(id: &str) -> Option<&'static AgentPersona> {
    all_personas().iter().find(|p| p.id == id)
}

/// Return all personas in a given [`SwarmDivision`].
pub fn personas_by_division(division: &SwarmDivision) -> Vec<&'static AgentPersona> {
    all_personas()
        .iter()
        .filter(|p| &p.division == division)
        .collect()
}

/// Return all personas that carry a specific capability namespace and name.
pub fn personas_by_capability(namespace: &str, name: &str) -> Vec<&'static AgentPersona> {
    all_personas()
        .iter()
        .filter(|p| {
            p.capability_tags
                .iter()
                .any(|t| t.namespace == namespace && t.name == name)
        })
        .collect()
}

/// Return all personas that accept a given [`WorkType`].
pub fn personas_by_work_type(work_type: &WorkType) -> Vec<&'static AgentPersona> {
    all_personas()
        .iter()
        .filter(|p| p.accepted_work_types.contains(work_type))
        .collect()
}

/// Return all personas with a specific [`ApprovalGatePolicy`].
pub fn personas_by_approval_policy(
    policy: &ApprovalGatePolicy,
) -> Vec<&'static AgentPersona> {
    all_personas()
        .iter()
        .filter(|p| &p.execution_contract.approval_policy == policy)
        .collect()
}

/// Return all personas marked `safe_for_auto_run`.
pub fn auto_run_safe_personas() -> Vec<&'static AgentPersona> {
    all_personas()
        .iter()
        .filter(|p| p.constraints.safe_for_auto_run)
        .collect()
}

/// Return all personas at or below a given [`ManifestRiskLevel`].
///
/// `ManifestRiskLevel::Low` returns only Low; `High` returns all.
pub fn personas_within_risk(max_risk: &ManifestRiskLevel) -> Vec<&'static AgentPersona> {
    all_personas()
        .iter()
        .filter(|p| &p.constraints.risk_level <= max_risk)
        .collect()
}

/// Given a candidate set of persona IDs, remove any that are incompatible with
/// `anchor_id`.
///
/// Returns the filtered set (not including `anchor_id` itself).
pub fn exclude_incompatible<'a>(
    anchor_id: &str,
    candidates: impl Iterator<Item = &'a AgentPersona>,
) -> Vec<&'a AgentPersona> {
    let anchor = match persona_by_id(anchor_id) {
        Some(a) => a,
        None => return candidates.collect(),
    };
    candidates
        .filter(|c| {
            c.id != anchor_id
                && !anchor.constraints.incompatible_with.contains(&c.id)
                && !c.constraints.incompatible_with.contains(&anchor_id)
        })
        .collect()
}

/// Select all eligible personas for a given [`WorkType`], optionally
/// restricting to `safe_for_auto_run` and within a maximum risk level.
pub fn eligible_personas_for_work(
    work_type: &WorkType,
    require_auto_run_safe: bool,
    max_risk: Option<&ManifestRiskLevel>,
) -> Vec<&'static AgentPersona> {
    all_personas()
        .iter()
        .filter(|p| {
            p.accepted_work_types.contains(work_type)
                && (!require_auto_run_safe || p.constraints.safe_for_auto_run)
                && max_risk
                    .map(|max| &p.constraints.risk_level <= max)
                    .unwrap_or(true)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// A single validation finding from [`validate_registry`].
#[derive(Debug, Clone)]
pub struct PersonaValidationError {
    /// The persona ID where the problem was found.
    pub persona_id: &'static str,
    /// Human-readable description of the problem.
    pub message: String,
}

/// Validate the entire persona registry for internal consistency.
///
/// Returns a list of [`PersonaValidationError`]s. An empty list means the
/// registry is valid and safe to use.
///
/// # Checks performed
/// - Unique IDs across all personas
/// - `compatible_with` / `incompatible_with` IDs exist in the registry
/// - No ID appears in both lists for the same persona
/// - `requires_approval: true` ↔ `approval_policy != None` alignment
/// - `emits_events: true` → `required_event_types` non-empty
/// - `delegation_allowed: false` when `supports_subtasks: false`
/// - All handoff IDs (`can_receive_from`, `can_handoff_to`) resolve
pub fn validate_registry() -> Vec<PersonaValidationError> {
    let personas = all_personas();
    let known_ids: std::collections::HashSet<&str> = personas.iter().map(|p| p.id).collect();
    let mut errors: Vec<PersonaValidationError> = Vec::new();

    // 1. Unique IDs
    let mut seen_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for p in personas {
        if !seen_ids.insert(p.id) {
            errors.push(PersonaValidationError {
                persona_id: p.id,
                message: format!("duplicate persona id: {:?}", p.id),
            });
        }
    }

    for p in personas {
        let id = p.id;

        // 2. compatible_with IDs exist
        for cid in &p.constraints.compatible_with {
            if !known_ids.contains(cid) {
                errors.push(PersonaValidationError {
                    persona_id: id,
                    message: format!("compatible_with references unknown id: {cid:?}"),
                });
            }
        }

        // 3. incompatible_with IDs exist
        for cid in &p.constraints.incompatible_with {
            if !known_ids.contains(cid) {
                errors.push(PersonaValidationError {
                    persona_id: id,
                    message: format!("incompatible_with references unknown id: {cid:?}"),
                });
            }
        }

        // 4. No overlap between compatible_with and incompatible_with
        for cid in &p.constraints.compatible_with {
            if p.constraints.incompatible_with.contains(cid) {
                errors.push(PersonaValidationError {
                    persona_id: id,
                    message: format!(
                        "agent {cid:?} appears in both compatible_with and incompatible_with"
                    ),
                });
            }
        }

        // 5. Approval alignment
        if p.execution_contract.requires_approval
            && p.execution_contract.approval_policy == ApprovalGatePolicy::None
        {
            errors.push(PersonaValidationError {
                persona_id: id,
                message: "requires_approval is true but approval_policy is None".to_string(),
            });
        }

        // 6. emits_events → required_event_types non-empty
        if p.observability.emits_events && p.observability.required_event_types.is_empty() {
            errors.push(PersonaValidationError {
                persona_id: id,
                message: "emits_events is true but required_event_types is empty".to_string(),
            });
        }

        // 7. delegation_allowed requires supports_subtasks
        if p.handoff_rules.delegation_allowed && !p.execution_contract.supports_subtasks {
            errors.push(PersonaValidationError {
                persona_id: id,
                message:
                    "delegation_allowed is true but supports_subtasks is false".to_string(),
            });
        }

        // 8. can_receive_from IDs exist
        for rid in &p.handoff_rules.can_receive_from {
            if !known_ids.contains(rid) {
                errors.push(PersonaValidationError {
                    persona_id: id,
                    message: format!("can_receive_from references unknown id: {rid:?}"),
                });
            }
        }

        // 9. can_handoff_to IDs exist
        for hid in &p.handoff_rules.can_handoff_to {
            if !known_ids.contains(hid) {
                errors.push(PersonaValidationError {
                    persona_id: id,
                    message: format!("can_handoff_to references unknown id: {hid:?}"),
                });
            }
        }

        // 10. must_preapprove alignment with approval_rules
        if p.approval_rules.must_preapprove
            && p.execution_contract.approval_policy == ApprovalGatePolicy::None
        {
            errors.push(PersonaValidationError {
                persona_id: id,
                message: "must_preapprove is true but approval_policy is None".to_string(),
            });
        }
    }

    errors
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_valid() {
        let errors = validate_registry();
        if !errors.is_empty() {
            for e in &errors {
                eprintln!("PERSONA VALIDATION: [{}] {}", e.persona_id, e.message);
            }
        }
        assert!(
            errors.is_empty(),
            "{} validation error(s) in persona registry",
            errors.len()
        );
    }

    #[test]
    fn all_18_personas_registered() {
        assert_eq!(all_personas().len(), 18, "expected exactly 18 personas");
    }

    #[test]
    fn all_persona_ids_unique() {
        let ids: Vec<&str> = all_personas().iter().map(|p| p.id).collect();
        let unique: std::collections::HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len(), "duplicate persona ids detected");
    }

    #[test]
    fn persona_by_id_resolves_all() {
        for persona in all_personas() {
            let found = persona_by_id(persona.id);
            assert!(
                found.is_some(),
                "persona_by_id failed for {:?}",
                persona.id
            );
        }
    }

    #[test]
    fn persona_by_id_returns_none_for_unknown() {
        assert!(persona_by_id("nonexistent_agent_abc").is_none());
    }

    #[test]
    fn division_queries_return_correct_agents() {
        let coord = personas_by_division(&SwarmDivision::Coordination);
        assert_eq!(coord.len(), 3, "expected 3 coordination agents");

        let research = personas_by_division(&SwarmDivision::Research);
        assert_eq!(research.len(), 3, "expected 3 research agents");

        let exec = personas_by_division(&SwarmDivision::Execution);
        assert_eq!(exec.len(), 3, "expected 3 execution agents");

        let quality = personas_by_division(&SwarmDivision::Quality);
        assert_eq!(quality.len(), 3, "expected 3 quality agents");

        let ops = personas_by_division(&SwarmDivision::Operations);
        assert_eq!(ops.len(), 3, "expected 3 operations agents");

        let trust = personas_by_division(&SwarmDivision::Trust);
        assert_eq!(trust.len(), 3, "expected 3 trust agents");
    }

    #[test]
    fn auto_run_safe_personas_are_safe() {
        for p in auto_run_safe_personas() {
            assert!(
                p.constraints.safe_for_auto_run,
                "{} returned by auto_run_safe_personas but is not safe",
                p.id
            );
        }
    }

    #[test]
    fn high_risk_agents_require_approval() {
        let high_risk: Vec<_> = all_personas()
            .iter()
            .filter(|p| p.constraints.risk_level == ManifestRiskLevel::High)
            .collect();
        for p in &high_risk {
            assert!(
                p.execution_contract.requires_approval,
                "{} is high risk but requires_approval is false",
                p.id
            );
        }
    }

    #[test]
    fn auto_run_agents_cannot_be_high_risk() {
        for p in all_personas() {
            if p.constraints.safe_for_auto_run {
                assert_ne!(
                    p.constraints.risk_level,
                    ManifestRiskLevel::High,
                    "{} is safe_for_auto_run but also ManifestRiskLevel::High",
                    p.id
                );
            }
        }
    }

    #[test]
    fn workflow_work_type_returns_multiple_agents() {
        let agents = personas_by_work_type(&WorkType::Workflow);
        assert!(
            agents.len() >= 3,
            "expected at least 3 agents accepting Workflow type"
        );
    }

    #[test]
    fn capability_tag_query_works() {
        let routing = personas_by_capability("coordination", "routing");
        assert!(
            routing.len() >= 2,
            "expected at least 2 agents with coordination:routing"
        );
    }

    #[test]
    fn exclude_incompatible_removes_correct_agents() {
        // research_analyst is incompatible_with api_integrator (declared on api_integrator)
        // The exclude_incompatible function checks both directions.
        let api_int = persona_by_id("api_integrator").expect("api_integrator must exist");
        let all = all_personas();
        let filtered = exclude_incompatible("api_integrator", all.iter());
        assert!(
            !filtered.iter().any(|p| p.id == "research_analyst"),
            "research_analyst should be excluded from api_integrator's swarm"
        );
        let _ = api_int; // silence unused binding warning
    }

    #[test]
    fn eligible_personas_for_research_work_type() {
        let eligible = eligible_personas_for_work(&WorkType::Research, false, None);
        assert!(
            eligible.iter().any(|p| p.id == "research_analyst"),
            "research_analyst must be eligible for Research work"
        );
        assert!(
            eligible.iter().any(|p| p.id == "source_verifier"),
            "source_verifier must be eligible for Research work"
        );
    }

    #[test]
    fn pre_execute_agents_are_not_auto_run_safe() {
        let pre_execute = personas_by_approval_policy(&ApprovalGatePolicy::PreExecute);
        for p in pre_execute {
            assert!(
                !p.constraints.safe_for_auto_run,
                "{} requires PreExecute approval but is marked safe_for_auto_run",
                p.id
            );
        }
    }

    #[test]
    fn handoff_graph_has_no_dead_end_personas() {
        // Every non-leaf persona must either handoff to some other persona or receive from one.
        // Leaf personas (pure terminals) are permitted only in the trust division.
        for p in all_personas() {
            let has_outbound = !p.handoff_rules.can_handoff_to.is_empty();
            let has_inbound = !p.handoff_rules.can_receive_from.is_empty();
            let is_trust_observer = p.division == SwarmDivision::Trust
                && p.execution_contract.approval_policy == ApprovalGatePolicy::None;

            assert!(
                has_outbound || has_inbound || is_trust_observer,
                "{} is completely isolated in the handoff graph",
                p.id
            );
        }
    }

    #[test]
    fn api_integrator_is_not_auto_run_safe_and_is_high_risk() {
        let p = persona_by_id("api_integrator").expect("api_integrator must exist");
        assert!(!p.constraints.safe_for_auto_run);
        assert_eq!(p.constraints.risk_level, ManifestRiskLevel::High);
    }

    #[test]
    fn orchestrator_delegate_can_delegate() {
        let p = persona_by_id("orchestrator_delegate").expect("must exist");
        assert!(p.handoff_rules.delegation_allowed);
        assert!(p.execution_contract.supports_subtasks);
    }

    #[test]
    fn audit_trail_agent_cannot_delegate() {
        let p = persona_by_id("audit_trail_agent").expect("must exist");
        assert!(!p.handoff_rules.delegation_allowed);
    }
}
