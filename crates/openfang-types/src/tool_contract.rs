//! Typed tool contract — the strict interface every attached app adapter must satisfy.
//!
//! ## Why this exists
//!
//! Without a typed contract every new integration becomes custom glue: different
//! error shapes, undeclared side-effects, no risk gating, no approval path.
//! This module defines the single authoritative interface that makes tools
//! composable, auditable, and safe to delegate to agents.
//!
//! ## Layers
//!
//! ```text
//! ToolContract          ← static declaration (what a tool CAN do)
//!     └─ AdapterKind    ← how to reach it (API / CLI / Browser)
//!     └─ RiskTier       ← what side-effects it may have
//!     └─ RetryPolicy    ← how to recover from transient failures
//!     └─ VerificationRule ← how to know it actually worked
//!
//! ToolEventRecord       ← dynamic trace (what a tool DID do)
//!
//! ServiceHealth         ← liveness snapshot (is the service reachable)
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ────────────────────────────────────────────────────────────────────────────
// RiskTier — side-effect classification
// ────────────────────────────────────────────────────────────────────────────

/// Classification of the side-effects a tool may produce.
///
/// Determines the default approval policy and whether auto-run is permitted.
///
/// | Tier              | Side-effect           | Auto-run? | Needs approval? |
/// |-------------------|-----------------------|-----------|-----------------|
/// | `ReadOnly`        | None                  | Always    | Never           |
/// | `WriteInternal`   | Local / in-process    | Trusted   | Configurable    |
/// | `WriteExternal`   | External API / send   | Cautious  | Often           |
/// | `Destructive`     | Delete / overwrite    | Never     | Always          |
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskTier {
    /// Pure reads — no state changed anywhere.
    ReadOnly,
    /// Writes to an internal, reversible store (memory, local files, session).
    WriteInternal,
    /// Writes to an external service (API calls, messages, notifications).
    WriteExternal,
    /// Destructive — delete, overwrite, execute irreversible operations.
    Destructive,
}

impl RiskTier {
    /// Returns true if the tier may always be auto-run without approval.
    pub fn always_auto_run(&self) -> bool {
        matches!(self, Self::ReadOnly)
    }

    /// Returns true if the tier always requires human approval.
    pub fn always_needs_approval(&self) -> bool {
        matches!(self, Self::WriteExternal | Self::Destructive)
    }

    /// Human-readable description of the tier's implications.
    pub fn description(&self) -> &'static str {
        match self {
            Self::ReadOnly => "No state changed. Safe to run automatically.",
            Self::WriteInternal => "Modifies local/internal state. May auto-run for trusted agents.",
            Self::WriteExternal => "Calls an external service or sends a message. Review recommended.",
            Self::Destructive => "Deletes or irreversibly modifies data. Always requires approval.",
        }
    }
}

impl std::fmt::Display for RiskTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadOnly => write!(f, "read_only"),
            Self::WriteInternal => write!(f, "write_internal"),
            Self::WriteExternal => write!(f, "write_external"),
            Self::Destructive => write!(f, "destructive"),
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// AdapterKind — execution surface
// ────────────────────────────────────────────────────────────────────────────

/// How the adapter reaches the underlying capability.
///
/// Preference order: `Api` → `Cli` → `Browser`.
/// Agents and the registry should always prefer lower-index adapters when
/// multiple are available for the same command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterKind {
    /// Official REST/GraphQL/gRPC API — lowest latency, most reliable.
    Api,
    /// Application CLI / SDK — good fallback when no API exists.
    Cli,
    /// Browser automation (Playwright / Puppeteer) — last resort only.
    Browser,
}

impl AdapterKind {
    /// Preference rank: lower = more preferred.
    pub fn preference_rank(&self) -> u8 {
        match self {
            Self::Api => 0,
            Self::Cli => 1,
            Self::Browser => 2,
        }
    }
}

impl std::fmt::Display for AdapterKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Api => write!(f, "api"),
            Self::Cli => write!(f, "cli"),
            Self::Browser => write!(f, "browser"),
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// RetryPolicy
// ────────────────────────────────────────────────────────────────────────────

/// Controls how execution retries transient failures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts (0 = no retries).
    pub max_attempts: u32,
    /// Base delay in milliseconds before the first retry.
    pub initial_delay_ms: u64,
    /// Multiplier applied to the delay after each retry (exponential backoff).
    ///
    /// Use `1.0` for constant delay, `2.0` for doubling, etc.
    pub backoff_multiplier: f32,
    /// Maximum delay cap in milliseconds regardless of multiplier.
    pub max_delay_ms: u64,
    /// If true, retry only on network/timeout errors; surface auth/schema errors immediately.
    pub retry_on_transient_only: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_ms: 500,
            backoff_multiplier: 2.0,
            max_delay_ms: 10_000,
            retry_on_transient_only: true,
        }
    }
}

impl RetryPolicy {
    /// A policy with no retries.
    pub fn no_retry() -> Self {
        Self {
            max_attempts: 0,
            ..Default::default()
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// ExecutionMode — sync vs async
// ────────────────────────────────────────────────────────────────────────────

/// Whether the tool executes synchronously or posts a job and polls for the result.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// Call and wait for a result before returning.
    #[default]
    Synchronous,
    /// Post a job, receive a job ID, poll until done.
    Asynchronous,
}

// ────────────────────────────────────────────────────────────────────────────
// VerificationRule — post-execution proof
// ────────────────────────────────────────────────────────────────────────────

/// Describes how the system proves that a tool call actually had its intended effect.
///
/// No tool action is complete until its `VerificationRule` passes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VerificationRule {
    /// Check that the HTTP/API response status code is in the success range.
    ApiStatusCode {
        /// Expected HTTP status code range start (inclusive).
        success_min: u16,
        /// Expected HTTP status code range end (inclusive).
        success_max: u16,
    },
    /// Fetch the artifact back and confirm it exists / has expected content.
    ArtifactExists {
        /// URL or storage path template. Use `{id}` to interpolate the created ID.
        fetch_url_template: String,
    },
    /// Confirm using a separate read-only tool that the state changed.
    StateChange {
        /// Name of the read tool to call.
        read_tool: String,
        /// JSONPath expression that must evaluate to a truthy value.
        expected_jsonpath: String,
    },
    /// A structured JSON diff between pre- and post-execution state.
    JsonDiff {
        /// JSONPath to extract the relevant field for comparison.
        path: String,
    },
    /// No verification performed — only use for truly idempotent reads.
    None,
}

// ────────────────────────────────────────────────────────────────────────────
// ToolContract — the central declaration
// ────────────────────────────────────────────────────────────────────────────

/// The single typed contract every attached app tool must satisfy.
///
/// This is the static declaration: what the tool *can* do, under what
/// conditions, how to retry it, and how to verify it worked.
///
/// At runtime the corresponding `AppAdapter` implementation handles execution,
/// and a `ToolEventRecord` captures the trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolContract {
    // ── Identity ─────────────────────────────────────────────────────────
    /// Fully-qualified tool name. Format: `{app}.{group}.{action}`.
    ///
    /// Examples: `github.issue.create`, `slack.message.send`, `notion.page.create`
    pub name: String,

    /// Human-friendly display name.
    pub display_name: String,

    /// Single-sentence explanation of what the tool does.
    pub description: String,

    /// Semantic version of this contract definition. Bumped on breaking changes.
    pub version: String,

    // ── Capability ───────────────────────────────────────────────────────
    /// Which application provides this capability.
    pub app_id: String,

    /// Logical command group within the app. Matches the middle segment of `name`.
    pub command_group: String,

    /// How the adapter reaches the underlying capability.
    pub adapter_kind: AdapterKind,

    // ── Schema ───────────────────────────────────────────────────────────
    /// JSON Schema object for the tool's input parameters.
    pub input_schema: serde_json::Value,

    /// JSON Schema object describing the structured output the tool returns.
    pub output_schema: serde_json::Value,

    // ── Risk & Execution ─────────────────────────────────────────────────
    /// Side-effect classification driving approval policy.
    pub risk_tier: RiskTier,

    /// Whether human approval must be obtained before execution.
    pub requires_approval: bool,

    /// Execution mode (sync vs async poll).
    pub execution_mode: ExecutionMode,

    /// Wall-clock execution timeout in milliseconds.
    pub timeout_ms: u64,

    /// How to retry on failure.
    pub retry_policy: RetryPolicy,

    // ── Verification ─────────────────────────────────────────────────────
    /// Rule applied after execution to confirm actual effect.
    pub verification_rule: VerificationRule,

    // ── Documentation ────────────────────────────────────────────────────
    /// Whether this tool is read-only (no writes).
    pub is_read_only: bool,

    /// Whether this tool is deterministic given the same inputs.
    pub is_deterministic: bool,

    /// Whether the tool action is idempotent (safe to retry).
    pub is_idempotent: bool,

    /// Link to official docs for this API endpoint / CLI command.
    pub docs_url: Option<String>,
}

impl ToolContract {
    /// Build a minimal `ToolContract` for rapid testing or scaffolding.
    pub fn minimal(name: impl Into<String>, app_id: impl Into<String>, risk_tier: RiskTier) -> Self {
        let name = name.into();
        let parts: Vec<&str> = name.splitn(3, '.').collect();
        let group = parts.get(1).copied().unwrap_or("misc").to_string();
        Self {
            name: name.clone(),
            display_name: name.clone(),
            description: String::new(),
            version: "1.0.0".to_string(),
            app_id: app_id.into(),
            command_group: group,
            adapter_kind: AdapterKind::Api,
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
            output_schema: serde_json::json!({"type": "object"}),
            risk_tier,
            requires_approval: risk_tier.always_needs_approval(),
            execution_mode: ExecutionMode::Synchronous,
            timeout_ms: 30_000,
            retry_policy: RetryPolicy::default(),
            verification_rule: VerificationRule::None,
            is_read_only: matches!(risk_tier, RiskTier::ReadOnly),
            is_deterministic: false,
            is_idempotent: matches!(risk_tier, RiskTier::ReadOnly),
            docs_url: None,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// ServiceHealth — liveness of a configured service
// ────────────────────────────────────────────────────────────────────────────

/// The operational status of a service integration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceStatus {
    /// Service is configured and reachable. Tools are usable.
    Healthy,
    /// Service has configuration but the endpoint is unreachable.
    Unreachable,
    /// Configuration is present but credentials are invalid.
    AuthFailed,
    /// Configuration is missing entirely (API key, token, etc.).
    Unconfigured,
    /// Service is reachable but returns errors (rate limited, partially broken).
    Degraded,
    /// Status has not been checked yet.
    Unknown,
}

impl ServiceStatus {
    /// Returns true when tools for this service may be executed.
    pub fn is_usable(&self) -> bool {
        matches!(self, Self::Healthy)
    }

    /// Returns true when we should surface a visible warning in the UI.
    pub fn needs_attention(&self) -> bool {
        !matches!(self, Self::Healthy | Self::Unknown)
    }
}

impl std::fmt::Display for ServiceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Healthy => write!(f, "healthy"),
            Self::Unreachable => write!(f, "unreachable"),
            Self::AuthFailed => write!(f, "auth_failed"),
            Self::Unconfigured => write!(f, "unconfigured"),
            Self::Degraded => write!(f, "degraded"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// A health snapshot for a single service integration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceHealth {
    /// The service identifier (e.g. `"github"`, `"slack"`, `"email"`).
    pub service_id: String,

    /// Human-readable service name.
    pub display_name: String,

    /// Current operational status.
    pub status: ServiceStatus,

    /// Optional detail string (error message, HTTP status, etc.).
    pub detail: Option<String>,

    /// When this snapshot was last refreshed.
    pub checked_at: DateTime<Utc>,

    /// Whether credentials are present (not necessarily valid).
    pub credentials_present: bool,

    /// Names of environment variables or config keys that are missing.
    pub missing_config: Vec<String>,
}

impl ServiceHealth {
    /// Create a placeholder health record for a service not yet checked.
    pub fn unknown(service_id: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            service_id: service_id.into(),
            display_name: display_name.into(),
            status: ServiceStatus::Unknown,
            detail: None,
            checked_at: Utc::now(),
            credentials_present: false,
            missing_config: vec![],
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// ToolPermissions — per-persona permission declaration
// ────────────────────────────────────────────────────────────────────────────

/// The permissions a persona has over the tool registry.
///
/// Every persona must carry either an `allow_all` flag or explicit allowlists.
/// `forbidden_tools` always wins over `allowed_tools`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPermissions {
    /// If true, the persona may use any tool up to `max_risk_tier` without
    /// an explicit allowlist. Explicit `forbidden_tools` still apply.
    pub allow_all: bool,

    /// Specific tool names this persona may use (exact match or `prefix.*` glob).
    /// Ignored when `allow_all = true`.
    pub allowed_tools: Vec<String>,

    /// Tools this persona is never permitted to call, regardless of `allow_all`.
    pub forbidden_tools: Vec<String>,

    /// The highest `RiskTier` this persona may invoke without human approval.
    pub max_risk_tier: RiskTier,

    /// Whether this persona may invoke write-external tools without approval.
    pub write_external_needs_approval: bool,

    /// Whether this persona must obtain approval before delegating to a sub-agent.
    pub delegation_needs_approval: bool,
}

impl Default for ToolPermissions {
    /// Conservative default: read-only, nothing allowed without explicit list.
    fn default() -> Self {
        Self {
            allow_all: false,
            allowed_tools: vec![],
            forbidden_tools: vec![],
            max_risk_tier: RiskTier::ReadOnly,
            write_external_needs_approval: true,
            delegation_needs_approval: true,
        }
    }
}

impl ToolPermissions {
    /// A permissive baseline for trusted system agents.
    pub fn trusted_system() -> Self {
        Self {
            allow_all: true,
            allowed_tools: vec![],
            forbidden_tools: vec![],
            max_risk_tier: RiskTier::WriteInternal,
            write_external_needs_approval: true,
            delegation_needs_approval: false,
        }
    }

    /// A restrictive baseline for untrusted or customer-facing agents.
    pub fn restricted() -> Self {
        Self {
            allow_all: false,
            allowed_tools: vec![],
            forbidden_tools: vec![],
            max_risk_tier: RiskTier::ReadOnly,
            write_external_needs_approval: true,
            delegation_needs_approval: true,
        }
    }

    /// Check whether a tool is permitted under this policy.
    ///
    /// Returns `Ok(())` if the tool may execute, `Err(reason)` if blocked.
    pub fn check(&self, tool_name: &str, risk_tier: RiskTier) -> Result<(), String> {
        // Hard block: forbidden list always wins
        for forbidden in &self.forbidden_tools {
            if Self::glob_match(forbidden, tool_name) {
                return Err(format!(
                    "Tool '{tool_name}' is in the forbidden list for this persona."
                ));
            }
        }

        // Risk ceiling
        if risk_tier > self.max_risk_tier {
            return Err(format!(
                "Tool '{tool_name}' has risk tier '{risk_tier}' which exceeds this persona's \
                 maximum permitted tier '{}'.",
                self.max_risk_tier
            ));
        }

        // Allow-all path
        if self.allow_all {
            return Ok(());
        }

        // Explicit allowlist
        let allowed = self
            .allowed_tools
            .iter()
            .any(|pattern| Self::glob_match(pattern, tool_name));
        if allowed {
            return Ok(());
        }

        Err(format!(
            "Tool '{tool_name}' is not in the allowed list for this persona."
        ))
    }

    /// Minimal glob matching: `prefix.*` matches `prefix.anything`, exact otherwise.
    fn glob_match(pattern: &str, name: &str) -> bool {
        if let Some(prefix) = pattern.strip_suffix(".*") {
            // Match `prefix.anything` — the name must start with `prefix.`
            name.starts_with(&format!("{prefix}."))
        } else {
            pattern == name
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// ToolEventRecord — execution lineage
// ────────────────────────────────────────────────────────────────────────────

/// A first-class event record capturing the full lineage of a single tool call.
///
/// Every tool execution (success, failure, blocked, approved) produces one of
/// these. They feed the dashboard, audit log, and cost analytics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolEventRecord {
    /// Unique event ID.
    pub event_id: String,

    /// The tool contract name that was invoked.
    pub tool_name: String,

    /// The app this tool belongs to.
    pub app_id: String,

    /// Which persona requested the execution.
    pub persona_id: Option<String>,

    /// The agent display name.
    pub agent_name: String,

    /// The agent ID.
    pub agent_id: String,

    /// Truncated input summary (first 512 chars of JSON-serialised input).
    pub input_summary: String,

    /// Truncated output summary (first 512 chars of result content).
    pub output_summary: String,

    /// The risk tier of the tool that was called.
    pub risk_tier: RiskTier,

    /// The adapter used to execute.
    pub adapter_kind: AdapterKind,

    /// Whether the call succeeded.
    pub success: bool,

    /// Whether the call produced an error result.
    pub is_error: bool,

    /// HTTP status code or OS exit code, if applicable.
    pub status_code: Option<i32>,

    /// The outcome of the post-execution verification step.
    pub verification_outcome: VerificationOutcome,

    /// Whether the call required and obtained human approval.
    pub approval_required: bool,

    /// Current approval state.
    pub approval_state: ApprovalState,

    /// How many times this call was retried.
    pub retry_count: u32,

    /// Wall-clock execution time in milliseconds.
    pub duration_ms: u64,

    /// Any artifact IDs produced by this call.
    pub artifact_ids: Vec<String>,

    /// The work item this call contributed to, if any.
    pub work_item_id: Option<String>,

    /// When the event was recorded.
    pub recorded_at: DateTime<Utc>,
}

/// Outcome of the post-execution verification step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationOutcome {
    /// Verification passed — side-effect confirmed.
    Passed,
    /// Verification failed — expected state change did not occur.
    Failed {
        /// Human-readable explanation of what was expected vs what was found.
        reason: String,
    },
    /// Verification was skipped (rule is `None` or opted out).
    Skipped,
    /// Verification could not be run (network error, timeout, etc.).
    Error {
        reason: String,
    },
}

/// Approval state for a tool call that required gating.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalState {
    /// Approval was not required.
    NotRequired,
    /// Awaiting a human decision.
    Pending,
    /// A human approved the action.
    Approved,
    /// A human rejected the action (tool was not executed).
    Rejected,
    /// Approval timed out; action was not executed.
    TimedOut,
}

impl ToolEventRecord {
    /// Create a complete event record for a tool execution.
    ///
    /// # Parameters
    /// - `tool_name` / `app_id` — contract identity
    /// - `persona_id` — empty string if not set
    /// - `agent_name` / `agent_id` — executing agent
    /// - `input_summary` / `output_summary` — already-truncated summaries
    /// - `risk_tier` / `adapter_kind` — from the contract
    /// - `success` — whether execution succeeded
    /// - `approval_required` — whether the contract requires approval
    /// - `duration_ms` — wall-clock execution time
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tool_name: impl Into<String>,
        app_id: impl Into<String>,
        persona_id: impl Into<String>,
        agent_name: impl Into<String>,
        agent_id: impl Into<String>,
        input_summary: impl Into<String>,
        output_summary: impl Into<String>,
        risk_tier: RiskTier,
        adapter_kind: AdapterKind,
        success: bool,
        approval_required: bool,
        duration_ms: u64,
    ) -> Self {
        let persona = persona_id.into();
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            tool_name: tool_name.into(),
            app_id: app_id.into(),
            persona_id: if persona.is_empty() { None } else { Some(persona) },
            agent_name: agent_name.into(),
            agent_id: agent_id.into(),
            input_summary: input_summary.into(),
            output_summary: output_summary.into(),
            risk_tier,
            adapter_kind,
            success,
            is_error: !success,
            status_code: None,
            verification_outcome: VerificationOutcome::Skipped,
            approval_required,
            approval_state: if approval_required {
                ApprovalState::Pending
            } else {
                ApprovalState::NotRequired
            },
            retry_count: 0,
            duration_ms,
            artifact_ids: vec![],
            work_item_id: None,
            recorded_at: Utc::now(),
        }
    }

    /// Truncate a string to at most 512 bytes for the summary fields.
    /// Never splits a UTF-8 character boundary.
    pub fn summarise(s: &str) -> String {
        if s.len() <= 512 {
            s.to_string()
        } else {
            // Find safe truncation point
            let safe = crate::truncate_str(s, 509);
            format!("{safe}\u{2026}")
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// PreflightResult — pre-execution readiness check
// ────────────────────────────────────────────────────────────────────────────

/// The result of a preflight check run before a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightResult {
    /// The tool contract that was checked.
    pub tool_name: String,

    /// Whether all checks passed and execution may proceed.
    pub ok: bool,

    /// Reasons the preflight failed (empty when `ok = true`).
    pub failures: Vec<PreflightFailure>,
}

/// An individual preflight failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PreflightFailure {
    /// Service is not configured (missing credentials).
    ServiceUnconfigured { service_id: String },

    /// Service credentials are present but authentication failed.
    AuthFailed { service_id: String, detail: String },

    /// The service endpoint could not be reached.
    ServiceUnreachable { service_id: String },

    /// Rate limit or quota exhausted.
    RateLimited { service_id: String, retry_after_secs: Option<u64> },

    /// The tool version is incompatible with the running service API.
    VersionMismatch { expected: String, actual: String },

    /// The persona does not have permission to call this tool.
    PermissionDenied { reason: String },
}

impl PreflightResult {
    pub fn ok(tool_name: impl Into<String>) -> Self {
        Self { tool_name: tool_name.into(), ok: true, failures: vec![] }
    }

    pub fn failed(tool_name: impl Into<String>, failures: Vec<PreflightFailure>) -> Self {
        Self { tool_name: tool_name.into(), ok: false, failures }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_tier_ordering() {
        assert!(RiskTier::ReadOnly < RiskTier::WriteInternal);
        assert!(RiskTier::WriteInternal < RiskTier::WriteExternal);
        assert!(RiskTier::WriteExternal < RiskTier::Destructive);
    }

    #[test]
    fn test_risk_tier_auto_run_rules() {
        assert!(RiskTier::ReadOnly.always_auto_run());
        assert!(!RiskTier::WriteInternal.always_auto_run());
        assert!(RiskTier::Destructive.always_needs_approval());
        assert!(!RiskTier::ReadOnly.always_needs_approval());
    }

    #[test]
    fn test_adapter_kind_preference() {
        assert!(AdapterKind::Api.preference_rank() < AdapterKind::Cli.preference_rank());
        assert!(AdapterKind::Cli.preference_rank() < AdapterKind::Browser.preference_rank());
    }

    #[test]
    fn test_tool_contract_minimal() {
        let contract = ToolContract::minimal("github.issue.create", "github", RiskTier::WriteExternal);
        assert_eq!(contract.app_id, "github");
        assert_eq!(contract.command_group, "issue");
        assert!(contract.requires_approval);
        assert!(!contract.is_read_only);
    }

    #[test]
    fn test_tool_permissions_check_forbidden() {
        let perms = ToolPermissions {
            allow_all: true,
            forbidden_tools: vec!["github.repo.delete".to_string()],
            max_risk_tier: RiskTier::WriteExternal,
            ..Default::default()
        };
        assert!(perms.check("github.issue.create", RiskTier::WriteExternal).is_ok());
        assert!(perms.check("github.repo.delete", RiskTier::Destructive).is_err());
    }

    #[test]
    fn test_tool_permissions_check_risk_ceiling() {
        let perms = ToolPermissions {
            allow_all: true,
            max_risk_tier: RiskTier::ReadOnly,
            ..Default::default()
        };
        assert!(perms.check("github.issue.list", RiskTier::ReadOnly).is_ok());
        assert!(perms.check("slack.message.send", RiskTier::WriteExternal).is_err());
    }

    #[test]
    fn test_tool_permissions_glob_match() {
        let perms = ToolPermissions {
            allow_all: false,
            allowed_tools: vec!["github.*".to_string()],
            max_risk_tier: RiskTier::WriteExternal,
            ..Default::default()
        };
        assert!(perms.check("github.issue.create", RiskTier::WriteExternal).is_ok());
        assert!(perms.check("slack.message.send", RiskTier::WriteExternal).is_err());
    }

    #[test]
    fn test_service_health_is_usable() {
        assert!(ServiceStatus::Healthy.is_usable());
        assert!(!ServiceStatus::Unconfigured.is_usable());
        assert!(!ServiceStatus::AuthFailed.is_usable());
        assert!(!ServiceStatus::Degraded.is_usable());
    }

    #[test]
    fn test_tool_event_record_new() {
        let record = ToolEventRecord::new(
            "github.issue.create",
            "github",
            "persona-coder",
            "coder",
            "agent-123",
            "input summary",
            "output summary",
            RiskTier::WriteExternal,
            AdapterKind::Api,
            true,
            false,
            250,
        );
        assert_eq!(record.tool_name, "github.issue.create");
        assert!(record.success);
        assert!(!record.is_error);
        assert_eq!(record.duration_ms, 250);
        assert_eq!(record.approval_state, ApprovalState::NotRequired);
        assert_eq!(record.persona_id, Some("persona-coder".to_string()));
    }

    #[test]
    fn test_preflight_result_ok() {
        let result = PreflightResult::ok("github.issue.create");
        assert!(result.ok);
        assert!(result.failures.is_empty());
    }

    #[test]
    fn test_retry_policy_no_retry() {
        let p = RetryPolicy::no_retry();
        assert_eq!(p.max_attempts, 0);
    }

    #[test]
    fn test_tool_contract_serialization() {
        let contract = ToolContract::minimal("slack.message.send", "slack", RiskTier::WriteExternal);
        let json = serde_json::to_string(&contract).expect("serialize");
        let back: ToolContract = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.name, "slack.message.send");
        assert_eq!(back.risk_tier, RiskTier::WriteExternal);
    }
}
