//! Swarm manifest specification, swarm planning, and delegation types.
//!
//! This module defines all the structured types needed to turn agents and hands
//! into validated, executable system assets — and to make swarm composition
//! deterministic and fully auditable via WorkEvents.
//!
//! Design principles:
//! - No agent may execute without a manifest-backed identity.
//! - Every swarm selection decision is persisted to WorkEvent history.
//! - Parent-child delegation creates explicit child WorkItems.
//! - Approval gates integrate with WorkItem status transitions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Enums — shared across manifest and swarm types
// ---------------------------------------------------------------------------

/// The operational division an agent belongs to.
/// Used to make swarm selection legible and deterministic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SwarmDivision {
    /// Agents that write code, call tools, produce artifacts.
    #[default]
    Execution,
    /// Agents that retrieve, synthesize, and summarise information.
    Research,
    /// Agents that review outputs for correctness, safety, or style.
    Quality,
    /// Agents that sequence steps and coordinate multi-agent work.
    Coordination,
    /// Agents that verify claims, validate identity, or audit decisions.
    Trust,
    /// Agents that manage infra, scheduling, and operational health.
    Operations,
    /// Escape hatch for custom, non-standard divisions.
    Custom(String),
}

impl SwarmDivision {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Execution => "execution",
            Self::Research => "research",
            Self::Quality => "quality",
            Self::Coordination => "coordination",
            Self::Trust => "trust",
            Self::Operations => "operations",
            Self::Custom(s) => s.as_str(),
        }
    }
}

impl std::str::FromStr for SwarmDivision {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "execution" => Ok(Self::Execution),
            "research" => Ok(Self::Research),
            "quality" => Ok(Self::Quality),
            "coordination" => Ok(Self::Coordination),
            "trust" => Ok(Self::Trust),
            "operations" => Ok(Self::Operations),
            other => Ok(Self::Custom(other.to_string())),
        }
    }
}

/// Risk rating for an agent's actions. Affects approval policy and swarm gating.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ManifestRiskLevel {
    /// Agent produces non-sensitive outputs with no external side-effects.
    #[default]
    Low,
    /// Agent may call external services or modify data.
    Medium,
    /// Agent executes commands, sends external communications, or handles PII.
    High,
}

impl ManifestRiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

impl std::str::FromStr for ManifestRiskLevel {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            _ => Err(()),
        }
    }
}

/// Approval gate policy — when human sign-off is required for this agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalGatePolicy {
    /// No approval gate. Agent may run freely.
    #[default]
    None,
    /// Block execution until an operator manually approves.
    PreExecute,
    /// Let the agent produce a draft, then gate finalization.
    PostDraft,
    /// Gate based on a runtime risk/cost assessment.
    Conditional,
}

impl ApprovalGatePolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::PreExecute => "pre_execute",
            Self::PostDraft => "post_draft",
            Self::Conditional => "conditional",
        }
    }
}

/// Expected duration class for a task executed by this agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeClass {
    /// Completes in seconds. Single LLM call. No tool loops.
    #[default]
    Short,
    /// Completes in under a minute. May use tools.
    Medium,
    /// Multi-step, may iterate, may spawn subtasks. Can take minutes.
    Long,
}

impl RuntimeClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Short => "short",
            Self::Medium => "medium",
            Self::Long => "long",
        }
    }
}

/// Verbosity of per-execution observability output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TraceLevel {
    /// Only lifecycle start/end events.
    Minimal,
    /// Standard tool calls, transitions, and outcomes.
    #[default]
    Standard,
    /// Full message traces including intermediate prompts.
    Verbose,
}

/// Cost sensitivity label — affects swarm selection under budget constraints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CostSensitivity {
    /// Prefer cheap models, avoid large context windows.
    Low,
    #[default]
    /// Standard cost trade-offs.
    Medium,
    /// May use expensive frontier models, large contexts, many tool calls.
    High,
}

// ---------------------------------------------------------------------------
// Structured capability tag
// ---------------------------------------------------------------------------

/// A structured capability tag — not a loose string, but a namespaced type.
///
/// Examples:
/// - `CapabilityTag { namespace: "code", name: "rust" }`
/// - `CapabilityTag { namespace: "research", name: "web_search" }`
/// - `CapabilityTag { namespace: "output", name: "json_schema" }`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CapabilityTag {
    /// Broad domain: code, research, output, comms, data, ops, trust, etc.
    pub namespace: String,
    /// Specific specialization within the namespace.
    pub name: String,
}

impl CapabilityTag {
    pub fn new(namespace: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            name: name.into(),
        }
    }

    /// Parse `"namespace:name"` format.
    pub fn parse(s: &str) -> Option<Self> {
        let (ns, n) = s.split_once(':')?;
        if ns.is_empty() || n.is_empty() {
            return None;
        }
        Some(Self::new(ns, n))
    }

    pub fn as_dotted(&self) -> String {
        format!("{}:{}", self.namespace, self.name)
    }
}

impl std::fmt::Display for CapabilityTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.namespace, self.name)
    }
}

// ---------------------------------------------------------------------------
// Schema reference for I/O typing
// ---------------------------------------------------------------------------

/// A reference to the schema of an input or output type.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchemaRef {
    /// Short human name, e.g. "MarkdownReport" or "CodePatch".
    pub type_name: String,
    /// Optional inline JSON Schema describing the structure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
    /// Freeform description of the type when a formal schema is unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// ---------------------------------------------------------------------------
// Service dependency
// ---------------------------------------------------------------------------

/// A runtime service this agent or hand depends on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDependency {
    /// Unique service identifier, e.g. "anthropic", "github", "postgres".
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Whether the service is strictly required or merely enhances the agent.
    #[serde(default)]
    pub optional: bool,
    /// Environment variable expected to carry the API key or connection string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_key: Option<String>,
    /// URL to obtain the key or set up the service.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup_url: Option<String>,
    /// Health-check endpoint to probe at startup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_endpoint: Option<String>,
}

// ---------------------------------------------------------------------------
// Agent Swarm Manifest
// ---------------------------------------------------------------------------

/// The full structured manifest for a swarm-capable agent.
///
/// Every agent that participates in swarm selection MUST have a validated
/// `AgentSwarmManifest`. Agents without a manifest cannot be selected.
///
/// Stored in: `agents/{name}/SWARM.toml` or inline in `agent.toml` under `[swarm]`.
/// Validated: at kernel startup and on agent spawn.
/// Queried: by the swarm planner when composing a roster for a WorkItem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSwarmManifest {
    // ---- Identity -----------------------------------------------------------
    /// Stable internal identifier (matches agent directory name).
    pub id: String,
    /// Human-readable display name.
    pub name: String,
    /// Operational division — used for swarm role assignment.
    #[serde(default)]
    pub division: SwarmDivision,
    /// Short description of what this agent does.
    pub description: String,
    /// Semver version string.
    #[serde(default = "default_version")]
    pub version: String,
    /// Author or team responsible for this manifest.
    #[serde(default)]
    pub author: String,
    /// Risk rating for this agent's actions.
    #[serde(default)]
    pub risk_level: ManifestRiskLevel,

    // ---- Capabilities -------------------------------------------------------
    /// Structured capability tags matched against WorkItem requirements.
    #[serde(default)]
    pub capability_tags: Vec<CapabilityTag>,
    /// Input types this agent accepts.
    #[serde(default)]
    pub input_types: Vec<SchemaRef>,
    /// Output types this agent can produce.
    #[serde(default)]
    pub output_types: Vec<SchemaRef>,
    /// Tool IDs that must be available for this agent to function.
    #[serde(default)]
    pub required_tools: Vec<String>,
    /// Service IDs that must be configured and healthy.
    #[serde(default)]
    pub required_services: Vec<ServiceDependency>,
    /// Service IDs that enhance capability but aren't strictly required.
    #[serde(default)]
    pub optional_services: Vec<ServiceDependency>,
    /// Maximum number of concurrent task executions.
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: u32,
    /// Whether the agent can be delegated subtasks (child WorkItems).
    #[serde(default)]
    pub supports_subtasks: bool,

    // ---- Execution Contract -------------------------------------------------
    /// Expected execution duration class.
    #[serde(default)]
    pub expected_runtime_class: RuntimeClass,
    /// Whether the agent's successful run produces a file/structured artifact.
    #[serde(default)]
    pub produces_artifact: bool,
    /// Schema for the artifact, when `produces_artifact` is true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_schema: Option<SchemaRef>,
    /// Whether this agent requires human approval.
    #[serde(default)]
    pub requires_approval: bool,
    /// Approval gate policy for this agent.
    #[serde(default)]
    pub approval_policy: ApprovalGatePolicy,

    // ---- Constraints --------------------------------------------------------
    /// Cost sensitivity label for swarm selection under budget pressure.
    #[serde(default)]
    pub cost_sensitivity: CostSensitivity,
    /// Whether the agent makes outbound calls to external networks.
    #[serde(default)]
    pub requires_external_network: bool,
    /// Whether the agent can execute in automated pipelines without human review.
    #[serde(default = "default_true")]
    pub safe_for_auto_run: bool,

    // ---- Observability ------------------------------------------------------
    /// Whether the agent emits structured WorkEvents during execution.
    #[serde(default = "default_true")]
    pub emits_events: bool,
    /// Whether the agent persists artifacts to the work item audit trail.
    #[serde(default)]
    pub logs_artifacts: bool,
    /// How much observability data to produce per execution.
    #[serde(default)]
    pub trace_level: TraceLevel,

    // ---- Compatibility -------------------------------------------------------
    /// Agent IDs that can be composed with this agent in the same swarm.
    #[serde(default)]
    pub compatible_with: Vec<String>,
    /// Agent IDs that MUST NOT appear in the same swarm as this agent.
    #[serde(default)]
    pub incompatible_with: Vec<String>,
}

fn default_version() -> String {
    "0.1.0".to_string()
}

fn default_max_concurrency() -> u32 {
    1
}

fn default_true() -> bool {
    true
}

// ---------------------------------------------------------------------------
// Hand / Tool Manifest
// ---------------------------------------------------------------------------

/// A callable function exposed by a Hand (tool bundle).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandToolFunction {
    /// Function name, e.g. "send_email", "search_web".
    pub name: String,
    /// Short description.
    pub description: String,
    /// JSON Schema for the input parameters.
    #[serde(default)]
    pub input_schema: serde_json::Value,
    /// JSON Schema for the output.
    #[serde(default)]
    pub output_schema: serde_json::Value,
}

/// Rate limiting profile for a Hand.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RateLimitProfile {
    /// Maximum calls per minute.
    #[serde(default)]
    pub calls_per_minute: Option<u32>,
    /// Maximum calls per day.
    #[serde(default)]
    pub calls_per_day: Option<u32>,
    /// Burst allowance above the per-minute rate.
    #[serde(default)]
    pub burst_allowance: Option<u32>,
}

/// Cost profile for a Hand's usage.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CostProfile {
    /// Label for the cost tier.
    #[serde(default)]
    pub tier: CostSensitivity,
    /// Approximate cost per call in USD (0.0 = free).
    #[serde(default)]
    pub approx_cost_per_call_usd: f64,
    /// Monthly cap in USD (0.0 = no cap).
    #[serde(default)]
    pub monthly_cap_usd: f64,
}

/// Structured manifest for a Hand (tool bundle / capability adapter).
///
/// Every Hand must declare this manifest to participate in swarm tool selection.
/// Validated at startup. Stored in `HAND.toml` alongside the hand implementation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandSwarmManifest {
    // ---- Identity -----------------------------------------------------------
    pub id: String,
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    pub description: String,

    // ---- Interface ----------------------------------------------------------
    /// Callable functions exposed by this hand.
    #[serde(default)]
    pub tool_functions: Vec<HandToolFunction>,
    /// Combined input schema for the hand.
    #[serde(default)]
    pub input_schema: serde_json::Value,
    /// Combined output schema for the hand.
    #[serde(default)]
    pub output_schema: serde_json::Value,

    // ---- Dependencies -------------------------------------------------------
    /// Services this hand requires.
    #[serde(default)]
    pub required_services: Vec<ServiceDependency>,
    /// Environment variables that must be set.
    #[serde(default)]
    pub required_env_vars: Vec<String>,

    // ---- Operational --------------------------------------------------------
    /// Health-check endpoint to verify the hand is operational.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_check_endpoint: Option<String>,
    #[serde(default)]
    pub cost_profile: CostProfile,
    #[serde(default)]
    pub rate_limit_profile: RateLimitProfile,
}

// ---------------------------------------------------------------------------
// Swarm Planner — inputs and outputs
// ---------------------------------------------------------------------------

/// The required capabilities the swarm planner derives from a WorkItem.
/// Produced by Step 1 of the swarm selection algorithm.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkItemRequirements {
    /// Tags that participating agents must collectively satisfy.
    pub required_capability_tags: Vec<CapabilityTag>,
    /// Required output schema description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_output_schema: Option<SchemaRef>,
    /// Minimum acceptable risk tolerance for the operation.
    #[serde(default)]
    pub risk_level: ManifestRiskLevel,
    /// Expected execution duration.
    #[serde(default)]
    pub runtime_class: RuntimeClass,
    /// Whether approval is required for any agent in the swarm.
    #[serde(default)]
    pub requires_approval: bool,
    /// Whether all selected agents must be safe-for-auto-run.
    #[serde(default)]
    pub auto_run_required: bool,
}

/// Why a candidate agent was rejected during swarm selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectionReason {
    /// The agent that was evaluated.
    pub agent_id: String,
    pub agent_name: String,
    /// Human-readable rejection explanation.
    pub reason: String,
    /// Which rule caused the rejection.
    pub rule: RejectionRule,
}

/// The specific rule that caused an agent to be rejected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectionRule {
    MissingCapabilityTag,
    MissingRequiredTool,
    MissingRequiredService,
    MaxConcurrencyExceeded,
    IncompatibleWithSelected,
    NotSafeForAutoRun,
    RiskLevelMismatch,
    ApprovalPolicyConflict,
    NoManifest,
}

/// A selected agent entry in the composed swarm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmMember {
    /// Agent entry ID.
    pub agent_id: String,
    pub agent_name: String,
    /// The role this agent plays in the swarm.
    pub role: SwarmRole,
    /// Division the agent belongs to.
    pub division: SwarmDivision,
    /// Capability tags that matched the WorkItem's requirements.
    pub matched_tags: Vec<CapabilityTag>,
    /// Whether the agent requires approval before/after execution.
    pub requires_approval: bool,
    pub approval_policy: ApprovalGatePolicy,
    /// Current operational health.
    pub health_status: AgentHealthStatus,
}

/// The role a swarm member plays in the composition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwarmRole {
    /// The primary agent to execute the work.
    PrimaryExecution,
    /// A supporting research agent gathering information.
    Research,
    /// A validation/quality agent reviewing output.
    QualityCheck,
    /// A coordination agent managing multi-step flow.
    Coordination,
    /// An approval-gate agent reviewing risky actions.
    ApprovalGate,
    /// A trust/identity verification agent.
    Trust,
}

impl SwarmRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PrimaryExecution => "primary_execution",
            Self::Research => "research",
            Self::QualityCheck => "quality_check",
            Self::Coordination => "coordination",
            Self::ApprovalGate => "approval_gate",
            Self::Trust => "trust",
        }
    }
}

/// Operational health of a swarm member at selection time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentHealthStatus {
    /// Agent is running and all required services are available.
    Healthy,
    /// Agent is running but some optional services are missing.
    Degraded,
    /// Agent is not running or a required service is missing.
    Unavailable,
    /// Health has not been checked.
    Unknown,
}

/// The full result of a swarm planning operation.
/// Persisted to WorkEvent history. Never discarded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmPlan {
    /// Unique ID for this planning event.
    pub id: String,
    /// The WorkItem this plan was composed for.
    pub work_item_id: String,
    /// When this plan was produced.
    pub planned_at: DateTime<Utc>,
    /// The derived requirements that drove selection.
    pub requirements: WorkItemRequirements,
    /// The selected swarm members (ordered: primary first).
    pub members: Vec<SwarmMember>,
    /// Agents that were considered but rejected.
    pub rejected_candidates: Vec<RejectionReason>,
    /// Human-readable explanation for the selection.
    pub selection_rationale: String,
    /// Risk assessment narrative.
    pub risk_assessment: String,
    /// Whether any member requires an approval gate.
    pub approval_gating_required: bool,
    /// The approval gate policy to apply if gating is required.
    #[serde(default)]
    pub approval_gate_policy: ApprovalGatePolicy,
    /// Whether this plan was successfully composed (false = no valid swarm found).
    pub success: bool,
    /// Error if success is false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// Manifest validation
// ---------------------------------------------------------------------------

/// A single validation finding on a manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationFinding {
    /// Severity of the finding.
    pub level: ValidationLevel,
    /// Short machine-readable code, e.g. "missing_required_service".
    pub code: String,
    /// Human-readable description.
    pub message: String,
    /// The manifest field(s) that caused this finding.
    #[serde(default)]
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationLevel {
    /// Fatal — the manifest is invalid and the agent cannot be registered.
    Error,
    /// Non-fatal — the agent can run but may behave unexpectedly.
    Warning,
}

/// The result of validating an agent or hand manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestValidationResult {
    pub agent_id: String,
    pub valid: bool,
    pub findings: Vec<ValidationFinding>,
    pub validated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Delegation record
// ---------------------------------------------------------------------------

/// A delegation event: when a parent agent spawns a child WorkItem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationRecord {
    pub id: String,
    /// Parent WorkItem ID.
    pub parent_work_item_id: String,
    /// The new child WorkItem ID.
    pub child_work_item_id: String,
    /// Reason for delegating.
    pub reason: String,
    /// Which agent requested the delegation.
    pub delegating_agent_id: String,
    /// Which agent was assigned to the child item.
    pub assigned_child_agent_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Work swarm observation (GET /work/{id}/swarm)
// ---------------------------------------------------------------------------

/// The full observable swarm state for a single WorkItem.
/// Returned by `GET /work/{id}/swarm`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkSwarmState {
    pub work_item_id: String,
    /// Selected swarm plan (if any).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swarm_plan: Option<SwarmPlan>,
    /// Delegation records — parent/child chain.
    #[serde(default)]
    pub delegations: Vec<DelegationRecord>,
    /// IDs of child work items spawned from this item.
    #[serde(default)]
    pub child_work_item_ids: Vec<String>,
    /// Whether any member requires approval gating.
    pub approval_gating_required: bool,
    /// Current status of approval gate.
    pub approval_gate_status: String,
}

// ---------------------------------------------------------------------------
// Swarm manifest registry entry
// ---------------------------------------------------------------------------

/// An entry in the agent swarm manifest registry.
/// Populated at kernel startup via `SwarmRegistry::load()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmRegistryEntry {
    pub agent_id: String,
    pub manifest: AgentSwarmManifest,
    pub validation: ManifestValidationResult,
    pub registered_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_tag_parse_roundtrip() {
        let tag = CapabilityTag::parse("code:rust").unwrap();
        assert_eq!(tag.namespace, "code");
        assert_eq!(tag.name, "rust");
        assert_eq!(tag.as_dotted(), "code:rust");
    }

    #[test]
    fn capability_tag_parse_rejects_empty() {
        assert!(CapabilityTag::parse(":rust").is_none());
        assert!(CapabilityTag::parse("code:").is_none());
        assert!(CapabilityTag::parse("nocodon").is_none());
    }

    #[test]
    fn swarm_division_roundtrip() {
        assert_eq!(SwarmDivision::Execution.as_str(), "execution");
        let d: SwarmDivision = "research".parse().unwrap();
        assert_eq!(d, SwarmDivision::Research);
        let custom: SwarmDivision = "finance".parse().unwrap();
        assert_eq!(custom.as_str(), "finance");
    }

    #[test]
    fn manifest_risk_ordering() {
        assert!(ManifestRiskLevel::Low < ManifestRiskLevel::Medium);
        assert!(ManifestRiskLevel::Medium < ManifestRiskLevel::High);
    }

    #[test]
    fn approval_gate_policy_as_str() {
        assert_eq!(ApprovalGatePolicy::None.as_str(), "none");
        assert_eq!(ApprovalGatePolicy::PreExecute.as_str(), "pre_execute");
        assert_eq!(ApprovalGatePolicy::PostDraft.as_str(), "post_draft");
        assert_eq!(ApprovalGatePolicy::Conditional.as_str(), "conditional");
    }

    #[test]
    fn default_agent_swarm_manifest_fields() {
        let m = AgentSwarmManifest {
            id: "test".into(),
            name: "test".into(),
            division: SwarmDivision::Execution,
            description: "test agent".into(),
            version: "0.1.0".into(),
            author: "test".into(),
            risk_level: ManifestRiskLevel::Low,
            capability_tags: vec![CapabilityTag::new("code", "rust")],
            input_types: vec![],
            output_types: vec![SchemaRef {
                type_name: "MarkdownReport".into(),
                schema: None,
                description: None,
            }],
            required_tools: vec!["file_read".into()],
            required_services: vec![],
            optional_services: vec![],
            max_concurrency: 1,
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
        };
        assert!(m.safe_for_auto_run);
        assert_eq!(m.capability_tags[0].as_dotted(), "code:rust");
    }
}
