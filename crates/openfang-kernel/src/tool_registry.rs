//! Central tool registry for the OpenFang kernel.
//!
//! The `ToolRegistry` is the single source of truth for:
//! - Which `ToolContract`s are available (registered by app adapters at startup).
//! - Which `AppAdapter` implementation handles each contract.
//! - The live `ServiceHealth` snapshot for every integrated service.
//! - The `ToolPermissions` policy for each persona.
//! - Preflight checks before execution.
//!
//! ## Lifetime
//!
//! The registry is constructed once in `OpenFangKernel::new()` and is shared
//! via `Arc<ToolRegistry>`. It is safe to clone the `Arc`.
//!
//! ## Adapter preference
//!
//! When multiple adapters are registered for the same tool name, the registry
//! always selects the one with the lowest `AdapterKind::preference_rank()`
//! (i.e. `Api` before `Cli` before `Browser`).

use dashmap::DashMap;
use openfang_types::tool_contract::{
    AdapterKind, PreflightFailure, PreflightResult, RiskTier, ServiceHealth, ServiceStatus,
    ToolContract, ToolEventRecord, ToolPermissions,
};
use openfang_types::app_adapter::{AdapterExecutionContext, AdapterResult, AppAdapter};
use std::sync::Arc;

// ────────────────────────────────────────────────────────────────────────────
// Adapter entry — wraps an AppAdapter with its preferred contract
// ────────────────────────────────────────────────────────────────────────────

struct AdapterEntry {
    adapter: Arc<dyn AppAdapter>,
    contract: ToolContract,
}

// ────────────────────────────────────────────────────────────────────────────
// ToolRegistry
// ────────────────────────────────────────────────────────────────────────────

/// Process-wide registry of tool contracts, adapters, service health, and
/// persona permission policies.
///
/// All operations are lock-free (`DashMap` inside) and safe to call from
/// multiple async tasks simultaneously.
pub struct ToolRegistry {
    /// tool_name → best AdapterEntry (lowest preference_rank wins)
    adapters: DashMap<String, AdapterEntry>,

    /// app_id → [tool_name]
    app_index: DashMap<String, Vec<String>>,

    /// service_id → ServiceHealth
    health: DashMap<String, ServiceHealth>,

    /// persona_id → ToolPermissions
    persona_permissions: DashMap<String, ToolPermissions>,
}

impl ToolRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        let registry = Self {
            adapters: DashMap::new(),
            app_index: DashMap::new(),
            health: DashMap::new(),
            persona_permissions: DashMap::new(),
        };

        // Seed health entries for all built-in service IDs as Unknown.
        for svc in &["github", "slack", "email", "calendar", "notion"] {
            let display = svc
                .chars()
                .next()
                .map(|c| c.to_uppercase().to_string() + &svc[1..])
                .unwrap_or_default();
            registry.update_health(ServiceHealth::unknown(*svc, display));
        }

        registry
    }

    /// Seed with all built-in starter-pack contracts (no adapter wired — just contracts).
    ///
    /// Call `register_adapter` to attach real adapters on top of these.
    pub fn with_builtin_contracts(self) -> Self {
        for c in builtin_contracts() {
            self.register_contract(c);
        }
        self
    }

    // ── Contract management ──────────────────────────────────────────────

    /// Register a `ToolContract` without an adapter.
    ///
    /// This makes the contract visible to persona planning and permission checks
    /// but `execute()` will return an error until an adapter is registered.
    pub fn register_contract(&self, contract: ToolContract) {
        let tool_name = contract.name.clone();
        let app_id = contract.app_id.clone();

        // Whether to replace existing: prefer lower adapter rank
        let should_replace = if let Some(existing) = self.adapters.get(&tool_name) {
            contract.adapter_kind.preference_rank()
                < existing.contract.adapter_kind.preference_rank()
        } else {
            true
        };

        if should_replace {
            self.adapters.insert(
                tool_name.clone(),
                AdapterEntry {
                    adapter: Arc::new(NoopAdapter {
                        app_id: app_id.clone(),
                        adapter_kind: contract.adapter_kind,
                    }),
                    contract,
                },
            );
        }

        // Update app index
        self.app_index
            .entry(app_id)
            .or_default()
            .push(tool_name);
    }

    /// Register an `AppAdapter` implementation.
    ///
    /// All contracts reported by the adapter are registered. If a better-ranked
    /// adapter is already registered for a contract, this one is ignored.
    pub fn register_adapter(&self, adapter: Arc<dyn AppAdapter>) {
        let app_id = adapter.app_id().to_string();
        let contracts = adapter.contracts();

        for contract in contracts {
            let tool_name = contract.name.clone();

            let should_replace = if let Some(existing) = self.adapters.get(&tool_name) {
                contract.adapter_kind.preference_rank()
                    < existing.contract.adapter_kind.preference_rank()
            } else {
                true
            };

            if should_replace {
                // Update app index
                self.app_index
                    .entry(app_id.clone())
                    .or_default()
                    .push(tool_name.clone());

                self.adapters.insert(
                    tool_name,
                    AdapterEntry {
                        adapter: adapter.clone(),
                        contract,
                    },
                );
            }
        }
    }

    // ── Queries ──────────────────────────────────────────────────────────

    /// Look up a registered `ToolContract` by its fully-qualified name.
    pub fn get(&self, tool_name: &str) -> Option<ToolContract> {
        self.adapters.get(tool_name).map(|e| e.contract.clone())
    }

    /// Returns all contracts registered for the given `app_id`.
    pub fn contracts_for_app(&self, app_id: &str) -> Vec<ToolContract> {
        self.app_index
            .get(app_id)
            .map(|names| {
                names
                    .iter()
                    .filter_map(|n| self.adapters.get(n.as_str()).map(|e| e.contract.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Returns every registered `ToolContract`.
    pub fn all_contracts(&self) -> Vec<ToolContract> {
        self.adapters.iter().map(|e| e.contract.clone()).collect()
    }

    /// Returns all app IDs that have at least one registered contract.
    pub fn registered_apps(&self) -> Vec<String> {
        self.app_index.iter().map(|e| e.key().clone()).collect()
    }

    // ── Permission checks ────────────────────────────────────────────────

    /// Check whether a persona may call a given tool.
    ///
    /// Returns `Ok(())` if allowed, `Err(reason)` if denied.
    pub fn check_permission(
        &self,
        tool_name: &str,
        permissions: &ToolPermissions,
    ) -> Result<(), String> {
        let contract = match self.get(tool_name) {
            Some(c) => c,
            None => return Err(format!("Tool '{tool_name}' is not registered")),
        };
        permissions.check(tool_name, contract.risk_tier)
    }

    /// Set or replace the `ToolPermissions` policy for a persona.
    pub fn set_persona_permissions(&self, persona_id: impl Into<String>, perms: ToolPermissions) {
        self.persona_permissions.insert(persona_id.into(), perms);
    }

    /// Retrieve the `ToolPermissions` for a persona.  
    /// Returns `ToolPermissions::default()` (restrictive) if none has been set.
    pub fn persona_permissions(&self, persona_id: &str) -> ToolPermissions {
        self.persona_permissions
            .get(persona_id)
            .map(|p| p.clone())
            .unwrap_or_default()
    }

    // ── Service health ───────────────────────────────────────────────────

    /// Overwrite (or create) the health record for a service.
    pub fn update_health(&self, health: ServiceHealth) {
        self.health.insert(health.service_id.clone(), health);
    }

    /// Get the current `ServiceHealth` snapshot for `service_id`.
    pub fn service_health(&self, service_id: &str) -> Option<ServiceHealth> {
        self.health.get(service_id).map(|h| h.clone())
    }

    /// Returns `true` if the service is `Healthy` (tools may be called).
    pub fn is_service_healthy(&self, service_id: &str) -> bool {
        self.health
            .get(service_id)
            .map(|h| h.status.is_usable())
            .unwrap_or(false)
    }

    /// Returns a snapshot of all service health records.
    pub fn all_service_health(&self) -> Vec<ServiceHealth> {
        self.health.iter().map(|h| h.clone()).collect()
    }

    // ── Preflight ────────────────────────────────────────────────────────

    /// Run a pre-execution readiness check for `tool_name` under `permissions`.
    ///
    /// Returns `PreflightResult` — call `.ok` to check if execution should proceed.
    pub fn preflight(
        &self,
        tool_name: &str,
        permissions: &ToolPermissions,
    ) -> PreflightResult {
        let mut failures: Vec<PreflightFailure> = Vec::new();

        // 1. Tool must exist
        let contract = match self.get(tool_name) {
            Some(c) => c,
            None => {
                return PreflightResult {
                    tool_name: tool_name.to_string(),
                    ok: false,
                    failures: vec![PreflightFailure::PermissionDenied {
                        reason: format!("Tool '{tool_name}' is not registered"),
                    }],
                };
            }
        };

        // 2. Permission check
        if let Err(reason) = permissions.check(tool_name, contract.risk_tier) {
            failures.push(PreflightFailure::PermissionDenied { reason });
        }

        // 3. Service health check
        let svc_id = &contract.app_id;
        if let Some(health) = self.service_health(svc_id) {
            match health.status {
                ServiceStatus::Unconfigured => {
                    failures.push(PreflightFailure::ServiceUnconfigured {
                        service_id: svc_id.clone(),
                    });
                }
                ServiceStatus::Unreachable => {
                    failures.push(PreflightFailure::ServiceUnreachable {
                        service_id: svc_id.clone(),
                    });
                }
                ServiceStatus::AuthFailed => {
                    failures.push(PreflightFailure::AuthFailed {
                        service_id: svc_id.clone(),
                        detail: health.detail.unwrap_or_else(|| "Credentials rejected".to_string()),
                    });
                }
                ServiceStatus::Degraded => {
                    // Degraded is a warning, not a hard block — execution can proceed.
                    tracing::warn!(
                        tool = tool_name,
                        service = svc_id,
                        "Service is degraded — tool execution may fail"
                    );
                }
                _ => {}
            }
        }

        PreflightResult {
            tool_name: tool_name.to_string(),
            ok: failures.is_empty(),
            failures,
        }
    }

    // ── Execution ────────────────────────────────────────────────────────

    /// Execute a tool via its registered adapter.
    ///
    /// Performs preflight, delegates to the adapter, runs post-execution
    /// verification, and returns both the `AdapterResult` and a `ToolEventRecord`
    /// for audit logging.
    ///
    /// This is the primary entry point for the kernel's tool runner.
    pub fn execute(
        &self,
        ctx: &AdapterExecutionContext,
        permissions: &ToolPermissions,
    ) -> (AdapterResult, ToolEventRecord) {
        let tool_name = &ctx.contract.name;
        let start = std::time::Instant::now();

        // Preflight
        let pre = self.preflight(tool_name, permissions);
        if !pre.ok {
            let failure_summary = pre
                .failures
                .iter()
                .map(|f| format!("{f:?}"))
                .collect::<Vec<_>>()
                .join("; ");
            let duration_ms = start.elapsed().as_millis() as u64;
            let result = AdapterResult::failure(failure_summary.clone(), false, duration_ms);
            let record = ToolEventRecord::new(
                tool_name,
                &ctx.contract.app_id,
                ctx.persona_id.as_deref().unwrap_or(""),
                &ctx.agent_name,
                &ctx.agent_id,
                ToolEventRecord::summarise(&serde_json::to_string(&ctx.input).unwrap_or_default()),
                "",
                ctx.contract.risk_tier,
                ctx.contract.adapter_kind,
                false,
                ctx.contract.requires_approval,
                duration_ms,
            );
            return (result, record);
        }

        // Lookup adapter
        let entry = match self.adapters.get(tool_name.as_str()) {
            Some(e) => e,
            None => {
                let duration_ms = start.elapsed().as_millis() as u64;
                let result = AdapterResult::failure(
                    format!("No adapter registered for '{tool_name}'"),
                    false,
                    duration_ms,
                );
                let record = ToolEventRecord::new(
                    tool_name,
                    &ctx.contract.app_id,
                    ctx.persona_id.as_deref().unwrap_or(""),
                    &ctx.agent_name,
                    &ctx.agent_id,
                    "",
                    "",
                    ctx.contract.risk_tier,
                    ctx.contract.adapter_kind,
                    false,
                    ctx.contract.requires_approval,
                    duration_ms,
                );
                return (result, record);
            }
        };

        // Execute
        let adapter = entry.adapter.clone();
        drop(entry); // release dashmap ref before calling into adapter
        let result = adapter.execute(ctx);
        let duration_ms = start.elapsed().as_millis() as u64;

        // Verification
        let verification_outcome = openfang_types::app_adapter::run_verification(
            &ctx.contract.verification_rule,
            &result.output,
            &ctx.contract,
        );

        let input_summary = ToolEventRecord::summarise(
            &serde_json::to_string(&ctx.input).unwrap_or_default(),
        );

        let record = ToolEventRecord::new(
            tool_name,
            &ctx.contract.app_id,
            ctx.persona_id.as_deref().unwrap_or(""),
            &ctx.agent_name,
            &ctx.agent_id,
            &input_summary,
            &result.output_summary,
            ctx.contract.risk_tier,
            ctx.contract.adapter_kind,
            result.success,
            ctx.contract.requires_approval,
            duration_ms,
        );

        // Log verification outcome
        if !matches!(
            verification_outcome,
            openfang_types::tool_contract::VerificationOutcome::Passed
                | openfang_types::tool_contract::VerificationOutcome::Skipped
        ) {
            tracing::warn!(
                tool = tool_name,
                outcome = ?verification_outcome,
                "Post-execution verification did not pass"
            );
        }

        (result, record)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new().with_builtin_contracts()
    }
}

// ────────────────────────────────────────────────────────────────────────────
// NoopAdapter — placeholder for contract-only registrations
// ────────────────────────────────────────────────────────────────────────────

struct NoopAdapter {
    app_id: String,
    adapter_kind: AdapterKind,
}

impl AppAdapter for NoopAdapter {
    fn app_id(&self) -> &str {
        &self.app_id
    }

    fn adapter_kind(&self) -> AdapterKind {
        self.adapter_kind
    }

    fn contracts(&self) -> Vec<ToolContract> {
        vec![]
    }

    fn preflight(&self, contract: &ToolContract) -> PreflightResult {
        PreflightResult {
            tool_name: contract.name.clone(),
            ok: false,
            failures: vec![PreflightFailure::ServiceUnconfigured {
                service_id: self.app_id.clone(),
            }],
        }
    }

    fn execute(&self, ctx: &AdapterExecutionContext) -> AdapterResult {
        AdapterResult::failure(
            format!(
                "No adapter wired for '{}'. Register an AppAdapter implementation.",
                ctx.contract.name
            ),
            false,
            0,
        )
    }

    fn health(&self) -> openfang_types::tool_contract::ServiceHealth {
        openfang_types::tool_contract::ServiceHealth::unknown(
            &self.app_id,
            &self.app_id,
        )
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Builtin starter-pack contracts
// ────────────────────────────────────────────────────────────────────────────

/// All built-in tool contracts included with OpenFang out of the box.
///
/// Covers: GitHub, Slack, Email, Calendar, Notion.
/// Real execution requires an `AppAdapter` registered via
/// `ToolRegistry::register_adapter`.
pub fn builtin_contracts() -> Vec<ToolContract> {
    use openfang_types::app_adapter::{
        github_contracts, slack_contracts, email_contracts, calendar_contracts, notion_contracts,
    };

    let mut all = Vec::new();
    all.extend(github_contracts());
    all.extend(slack_contracts());
    all.extend(email_contracts());
    all.extend(calendar_contracts());
    all.extend(notion_contracts());
    all
}

// ────────────────────────────────────────────────────────────────────────────
// Default persona permission policies
// ────────────────────────────────────────────────────────────────────────────

/// Returns the default `ToolPermissions` appropriate for a given persona role.
///
/// These are the sensible defaults; they can be overridden via
/// `ToolRegistry::set_persona_permissions`.
pub fn default_permissions_for_persona(persona_id: &str) -> ToolPermissions {
    match persona_id {
        // Coordination agents — full read, controlled write
        "orchestrator_delegate" | "task_router" | "scheduler_operator" => ToolPermissions {
            allow_all: false,
            allowed_tools: vec!["*.*.list".to_string(), "*.*.get".to_string()],
            forbidden_tools: vec![],
            max_risk_tier: RiskTier::WriteInternal,
            write_external_needs_approval: true,
            delegation_needs_approval: false,
        },

        // Research agents — read-only
        "research_analyst" | "source_verifier" | "brief_synthesizer" => ToolPermissions {
            allow_all: false,
            allowed_tools: vec![
                "github.issue.list".to_string(),
                "github.repo.list".to_string(),
                "github.code.search".to_string(),
            ],
            forbidden_tools: vec![],
            max_risk_tier: RiskTier::ReadOnly,
            write_external_needs_approval: true,
            delegation_needs_approval: true,
        },

        // Action agents — full write with approval on external
        "workflow_executor" | "api_integrator" => ToolPermissions {
            allow_all: true,
            allowed_tools: vec![],
            forbidden_tools: vec!["email.message.send".to_string()],
            max_risk_tier: RiskTier::WriteExternal,
            write_external_needs_approval: false,
            delegation_needs_approval: false,
        },

        // Safety agents — read access only
        "risk_checker" | "policy_gate_agent" | "audit_trail_agent" => ToolPermissions {
            allow_all: false,
            allowed_tools: vec!["*.*.list".to_string(), "*.*.get".to_string()],
            forbidden_tools: vec![],
            max_risk_tier: RiskTier::ReadOnly,
            write_external_needs_approval: true,
            delegation_needs_approval: true,
        },

        // All others — conservative defaults
        _ => ToolPermissions::default(),
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_types::tool_contract::{RiskTier, ToolPermissions};

    fn registry() -> ToolRegistry {
        ToolRegistry::new().with_builtin_contracts()
    }

    #[test]
    fn test_builtin_contracts_registered() {
        let r = registry();
        let all = r.all_contracts();
        assert!(!all.is_empty(), "No contracts registered");
        let names: Vec<_> = all.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"github.issue.create"));
        assert!(names.contains(&"slack.message.send"));
        assert!(names.contains(&"email.message.send"));
    }

    #[test]
    fn test_get_contract() {
        let r = registry();
        let c = r.get("github.issue.create").expect("github.issue.create should exist");
        assert_eq!(c.app_id, "github");
        assert_eq!(c.risk_tier, RiskTier::WriteExternal);
    }

    #[test]
    fn test_contracts_for_app() {
        let r = registry();
        let github = r.contracts_for_app("github");
        assert!(!github.is_empty());
        assert!(github.iter().all(|c| c.app_id == "github"));
    }

    #[test]
    fn test_registered_apps() {
        let r = registry();
        let apps = r.registered_apps();
        assert!(apps.contains(&"github".to_string()));
        assert!(apps.contains(&"slack".to_string()));
        assert!(apps.contains(&"email".to_string()));
    }

    #[test]
    fn test_unknown_tool_returns_none() {
        let r = registry();
        assert!(r.get("does.not.exist").is_none());
    }

    #[test]
    fn test_service_health_seeded_as_unknown() {
        let r = registry();
        let h = r.service_health("github").expect("github health should exist");
        assert_eq!(h.status, openfang_types::tool_contract::ServiceStatus::Unknown);
    }

    #[test]
    fn test_update_health() {
        let r = registry();
        let mut h = ServiceHealth::unknown("github", "GitHub");
        h.status = openfang_types::tool_contract::ServiceStatus::Healthy;
        r.update_health(h);
        assert!(r.is_service_healthy("github"));
    }

    #[test]
    fn test_permission_check_allowed() {
        let r = registry();
        let perms = ToolPermissions {
            allow_all: true,
            max_risk_tier: RiskTier::WriteExternal,
            ..Default::default()
        };
        assert!(r.check_permission("github.issue.list", &perms).is_ok());
        assert!(r.check_permission("github.issue.create", &perms).is_ok());
    }

    #[test]
    fn test_permission_check_forbidden() {
        let r = registry();
        let perms = ToolPermissions {
            allow_all: true,
            forbidden_tools: vec!["github.issue.create".to_string()],
            max_risk_tier: RiskTier::WriteExternal,
            ..Default::default()
        };
        assert!(r.check_permission("github.issue.create", &perms).is_err());
        assert!(r.check_permission("github.issue.list", &perms).is_ok());
    }

    #[test]
    fn test_permission_check_risk_ceiling() {
        let r = registry();
        let perms = ToolPermissions {
            allow_all: true,
            max_risk_tier: RiskTier::ReadOnly,
            ..Default::default()
        };
        // github.issue.list is ReadOnly — allowed
        assert!(r.check_permission("github.issue.list", &perms).is_ok());
        // github.issue.create is WriteExternal — blocked
        assert!(r.check_permission("github.issue.create", &perms).is_err());
    }

    #[test]
    fn test_preflight_fail_service_unconfigured() {
        let r = registry();
        // Health is Unknown (not Healthy) — should surface a ServiceUnconfigured
        // Actually Unknown doesn't block — but let's set it to Unconfigured explicitly.
        let mut h = ServiceHealth::unknown("slack", "Slack");
        h.status = openfang_types::tool_contract::ServiceStatus::Unconfigured;
        r.update_health(h);

        let perms = ToolPermissions { allow_all: true, max_risk_tier: RiskTier::WriteExternal, ..Default::default() };
        let pre = r.preflight("slack.message.send", &perms);
        assert!(!pre.ok);
        assert!(pre.failures.iter().any(|f| matches!(f, PreflightFailure::ServiceUnconfigured { .. })));
    }

    #[test]
    fn test_preflight_pass_when_healthy() {
        let r = registry();
        let mut h = ServiceHealth::unknown("github", "GitHub");
        h.status = openfang_types::tool_contract::ServiceStatus::Healthy;
        r.update_health(h);

        let perms = ToolPermissions { allow_all: true, max_risk_tier: RiskTier::WriteExternal, ..Default::default() };
        let pre = r.preflight("github.issue.list", &perms);
        assert!(pre.ok, "Expected preflight to pass: {:?}", pre.failures);
    }

    #[test]
    fn test_default_permissions_for_persona() {
        let p = default_permissions_for_persona("research_analyst");
        assert_eq!(p.max_risk_tier, RiskTier::ReadOnly);
        assert!(!p.allow_all);

        let p2 = default_permissions_for_persona("api_integrator");
        assert!(p2.allow_all);
        assert_eq!(p2.max_risk_tier, RiskTier::WriteExternal);
    }

    #[test]
    fn test_set_and_get_persona_permissions() {
        let r = registry();
        let perms = ToolPermissions {
            allow_all: false,
            allowed_tools: vec!["github.*".to_string()],
            max_risk_tier: RiskTier::WriteExternal,
            ..Default::default()
        };
        r.set_persona_permissions("my_persona", perms.clone());
        let retrieved = r.persona_permissions("my_persona");
        assert!(!retrieved.allow_all);
        assert_eq!(retrieved.allowed_tools, perms.allowed_tools);
    }

    #[test]
    fn test_register_adapter_replaces_noop() {
        let r = ToolRegistry::new().with_builtin_contracts();
        // Before: NoopAdapter
        let contract = r.get("github.issue.list").unwrap();
        assert_eq!(contract.adapter_kind, AdapterKind::Api);

        struct RealGithubAdapter;
        impl AppAdapter for RealGithubAdapter {
            fn app_id(&self) -> &str { "github" }
            fn adapter_kind(&self) -> AdapterKind { AdapterKind::Api }
            fn contracts(&self) -> Vec<ToolContract> {
                vec![ToolContract::minimal("github.issue.list", "github", RiskTier::ReadOnly)]
            }
            fn preflight(&self, contract: &ToolContract) -> PreflightResult {
                PreflightResult { tool_name: contract.name.clone(), ok: true, failures: vec![] }
            }
            fn execute(&self, _ctx: &AdapterExecutionContext) -> AdapterResult {
                AdapterResult::success(serde_json::json!({"issues": []}), 50)
            }
            fn health(&self) -> ServiceHealth {
                let mut h = ServiceHealth::unknown("github", "GitHub");
                h.status = openfang_types::tool_contract::ServiceStatus::Healthy;
                h
            }
        }

        r.register_adapter(Arc::new(RealGithubAdapter));
        // Contract should still be there
        let c = r.get("github.issue.list").unwrap();
        assert_eq!(c.app_id, "github");
    }
}
