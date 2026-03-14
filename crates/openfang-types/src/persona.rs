//! Typed persona definitions for the OpenFang subagent system.
//!
//! This module defines the strongly-typed structs and enums representing each
//! agent persona in the OpenFang registry. Personas are first-class runtime
//! objects that drive swarm selection, approval gating, observability, and
//! handoff rule enforcement.
//!
//! Types here deliberately reuse enums from [`crate::swarm`] and
//! [`crate::work_item`] to keep the type system unified and the swarm planner
//! directly queryable.

use crate::swarm::{
    ApprovalGatePolicy, CapabilityTag, CostSensitivity, ManifestRiskLevel, RuntimeClass,
    SwarmDivision, TraceLevel,
};
use crate::work_item::WorkType;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Output type taxonomy
// ---------------------------------------------------------------------------

/// The kinds of deliverables an agent persona can produce.
///
/// Used by the swarm planner to match required outputs against available agents
/// and by the dashboard to display what a given swarm has produced.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkOutputType {
    /// A normalized, machine-readable set of research findings.
    StructuredInsights,
    /// A binary or structured artifact (file, JSON payload, rendered document).
    Artifact,
    /// A human-readable summary or report document.
    Report,
    /// A work item that has been routed to a new agent or queue.
    RoutedWorkItem,
    /// The outcome of a validation pass: pass/fail with findings.
    ValidationResult,
    /// A compiled approval package ready for human review.
    ApprovalPacket,
    /// A tamper-evident audit log entry.
    AuditTrail,
    /// A policy enforcement decision (allow / deny / escalate).
    PolicyDecision,
    /// A scheduled work item with a future execution time.
    ScheduledItem,
    /// A synthesized brief ready for downstream processing or review.
    RefinedBrief,
    /// A lightweight notification message (email, webhook, event).
    Notification,
    /// Escape hatch for domain-specific output types.
    Custom(String),
}

// ---------------------------------------------------------------------------
// Execution contract
// ---------------------------------------------------------------------------

/// Defines what an agent promises to do and under what conditions.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionContract {
    /// Expected duration class for this agent's typical work.
    pub expected_runtime_class: RuntimeClass,
    /// Whether any approval interaction is required before or after execution.
    pub requires_approval: bool,
    /// The approval policy that governs this agent's execution lifecycle.
    pub approval_policy: ApprovalGatePolicy,
    /// Whether this agent writes a durable artifact to object storage or memory.
    pub produces_artifact: bool,
    /// Optional JSON Schema `$ref` or schema ID describing the artifact shape.
    pub artifact_schema_ref: Option<String>,
    /// Whether this agent may spawn child WorkItems (sub-tasks) during execution.
    pub supports_subtasks: bool,
}

// ---------------------------------------------------------------------------
// Constraints
// ---------------------------------------------------------------------------

/// Operational boundaries for the persona: risk, cost, concurrency, and
/// which other agents it can or cannot share a swarm with.
#[derive(Debug, Clone, Serialize)]
pub struct AgentConstraints {
    /// The risk level of this agent's typical work output.
    pub risk_level: ManifestRiskLevel,
    /// How sensitive this agent is to per-token or per-call LLM costs.
    pub cost_sensitivity: CostSensitivity,
    /// Whether this agent may be invoked without prior human approval.
    pub safe_for_auto_run: bool,
    /// Maximum number of concurrent executions permitted.
    pub max_concurrency: usize,
    /// Agent IDs this persona works well alongside in the same swarm.
    pub compatible_with: Vec<&'static str>,
    /// Agent IDs that must never co-occur in the same swarm as this persona.
    pub incompatible_with: Vec<&'static str>,
}

// ---------------------------------------------------------------------------
// Observability
// ---------------------------------------------------------------------------

/// Controls how much telemetry this persona emits.
#[derive(Debug, Clone, Serialize)]
pub struct ObservabilityPolicy {
    /// Whether this agent emits `WorkEvent` records during execution.
    pub emits_events: bool,
    /// Verbosity of OpenTelemetry / tracing spans for this agent.
    pub trace_level: TraceLevel,
    /// Whether produced artifacts are written to the artifact log.
    pub logs_artifacts: bool,
    /// Event type strings that MUST be emitted (validated at registration time).
    pub required_event_types: Vec<&'static str>,
}

// ---------------------------------------------------------------------------
// Handoff rules
// ---------------------------------------------------------------------------

/// Directed routing constraints: which agents may delegate to this one, and
/// which agents this one may hand work off to.
#[derive(Debug, Clone, Serialize)]
pub struct HandoffRules {
    /// Agent IDs that are permitted to route work to this persona.
    pub can_receive_from: Vec<&'static str>,
    /// Agent IDs that this persona may forward or delegate work items to.
    pub can_handoff_to: Vec<&'static str>,
    /// Whether this persona is allowed to create child WorkItems.
    pub delegation_allowed: bool,
}

// ---------------------------------------------------------------------------
// Approval rules
// ---------------------------------------------------------------------------

/// Fine-grained per-agent approval requirements beyond the execution contract.
#[derive(Debug, Clone, Serialize)]
pub struct ApprovalRules {
    /// Require explicit approval before any write-back or execution begins.
    pub must_preapprove: bool,
    /// Require explicit approval after output is produced but before finalising.
    pub must_postapprove: bool,
    /// Conditions or event types that trigger the approval flow.
    pub approval_triggers: Vec<&'static str>,
}

// ---------------------------------------------------------------------------
// AgentPersona — the top-level persona definition
// ---------------------------------------------------------------------------

/// A fully typed, registry-valid definition of an OpenFang subagent persona.
///
/// This is the single source of truth used by the swarm planner, approval
/// system, observability layer, and dashboard for everything relating to a
/// named agent persona.
#[derive(Debug, Clone, Serialize)]
pub struct AgentPersona {
    // --- Identity ---
    /// Unique snake_case identifier used in all routing and handoff references.
    pub id: &'static str,
    /// Human-friendly display name.
    pub name: &'static str,
    /// SemVer string for the persona definition itself.
    pub version: &'static str,
    /// The functional division this persona belongs to.
    pub division: SwarmDivision,
    /// A single-sentence description of this persona's role.
    pub description: &'static str,
    /// The precise mission area this persona is accountable for.
    pub core_focus: &'static str,

    // --- Capabilities ---
    /// Semantic capability tags used for swarm matching.
    pub capability_tags: Vec<CapabilityTag>,
    /// WorkItem types this persona is authorised to accept.
    pub accepted_work_types: Vec<WorkType>,
    /// The output types this persona produces.
    pub output_types: Vec<WorkOutputType>,
    /// Tool identifiers required for this persona to function.
    pub required_tools: Vec<&'static str>,
    /// Service identifiers required for this persona to function.
    pub required_services: Vec<&'static str>,
    /// Service identifiers that enhance this persona but are not required.
    pub optional_services: Vec<&'static str>,

    // --- Contracts, constraints, observability, handoff, approval ---
    pub execution_contract: ExecutionContract,
    pub constraints: AgentConstraints,
    pub observability: ObservabilityPolicy,
    pub handoff_rules: HandoffRules,
    pub approval_rules: ApprovalRules,

    /// Free-form implementation notes for developers wiring this persona.
    pub implementation_notes: &'static str,
}
