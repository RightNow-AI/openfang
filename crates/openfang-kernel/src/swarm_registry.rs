//! Swarm Manifest Registry — validates and stores agent swarm manifests.
//!
//! Loaded at kernel startup. Every agent that participates in swarm selection
//! must have a valid `AgentSwarmManifest` registered here.
//!
//! Validation rules:
//! - `id` and `name` must be non-empty.
//! - `required_tools` entries must exist in the known tool set.
//! - `approval_policy` must align with `risk_level`.
//! - `incompatible_with` agents must not be in the same swarm (enforced by planner).
//! - Required service `env_key` fields must be present in the environment.

use chrono::Utc;
use dashmap::DashMap;
use openfang_types::swarm::{
    AgentSwarmManifest, ApprovalGatePolicy, ManifestRiskLevel, ManifestValidationResult,
    SwarmRegistryEntry, ValidationFinding, ValidationLevel,
};
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{info, warn};

/// Central registry for agent swarm manifests.
///
/// Thread-safe. Multiple readers, single writer at startup.
pub struct SwarmRegistry {
    entries: Arc<DashMap<String, SwarmRegistryEntry>>,
}

impl Default for SwarmRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SwarmRegistry {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(DashMap::new()),
        }
    }

    // -----------------------------------------------------------------------
    // Registration
    // -----------------------------------------------------------------------

    /// Validate and register an agent swarm manifest.
    ///
    /// Returns the validation result regardless of outcome so callers can
    /// inspect findings. The manifest is only stored if `valid == true`.
    pub fn register(
        &self,
        manifest: AgentSwarmManifest,
        known_tools: &HashSet<String>,
    ) -> ManifestValidationResult {
        let result = validate_manifest(&manifest, known_tools);

        if result.valid {
            let entry = SwarmRegistryEntry {
                agent_id: manifest.id.clone(),
                manifest,
                validation: result.clone(),
                registered_at: Utc::now(),
            };
            self.entries.insert(entry.agent_id.clone(), entry);
            info!(agent_id = %result.agent_id, "swarm manifest registered");
        } else {
            let errors: Vec<&str> = result
                .findings
                .iter()
                .filter(|f| f.level == ValidationLevel::Error)
                .map(|f| f.message.as_str())
                .collect();
            warn!(
                agent_id = %result.agent_id,
                errors = ?errors,
                "swarm manifest rejected — registration skipped"
            );
        }

        result
    }

    /// Register a manifest without tool validation (used in tests and fast-path boot).
    pub fn register_unchecked(&self, manifest: AgentSwarmManifest) {
        let result = ManifestValidationResult {
            agent_id: manifest.id.clone(),
            valid: true,
            findings: vec![],
            validated_at: Utc::now(),
        };
        let entry = SwarmRegistryEntry {
            agent_id: manifest.id.clone(),
            manifest,
            validation: result,
            registered_at: Utc::now(),
        };
        self.entries.insert(entry.agent_id.clone(), entry);
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    /// Retrieve a manifest entry by agent ID.
    pub fn get(&self, agent_id: &str) -> Option<SwarmRegistryEntry> {
        self.entries.get(agent_id).map(|e| e.clone())
    }

    /// List all registered entries.
    pub fn list(&self) -> Vec<SwarmRegistryEntry> {
        self.entries.iter().map(|e| e.clone()).collect()
    }

    /// List all entries whose capability tags include at least one of the given tags.
    pub fn filter_by_capabilities(
        &self,
        required: &[openfang_types::swarm::CapabilityTag],
    ) -> Vec<SwarmRegistryEntry> {
        if required.is_empty() {
            return self.list();
        }
        self.entries
            .iter()
            .filter(|e| {
                required.iter().any(|req| {
                    e.manifest
                        .capability_tags
                        .iter()
                        .any(|t| t.namespace == req.namespace && t.name == req.name)
                })
            })
            .map(|e| e.clone())
            .collect()
    }

    /// Number of registered manifests.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Manifest validation engine
// ---------------------------------------------------------------------------

/// Validate a single agent swarm manifest.
///
/// Returns a `ManifestValidationResult` with all findings.
/// The manifest is `valid` only if no `Error`-level findings exist.
pub fn validate_manifest(
    manifest: &AgentSwarmManifest,
    known_tools: &HashSet<String>,
) -> ManifestValidationResult {
    let mut findings: Vec<ValidationFinding> = Vec::new();

    // --- Identity checks -----------------------------------------------------
    if manifest.id.trim().is_empty() {
        findings.push(ValidationFinding {
            level: ValidationLevel::Error,
            code: "missing_id".into(),
            message: "Manifest `id` must not be empty.".into(),
            fields: vec!["id".into()],
        });
    }
    if manifest.name.trim().is_empty() {
        findings.push(ValidationFinding {
            level: ValidationLevel::Error,
            code: "missing_name".into(),
            message: "Manifest `name` must not be empty.".into(),
            fields: vec!["name".into()],
        });
    }
    if manifest.description.trim().is_empty() {
        findings.push(ValidationFinding {
            level: ValidationLevel::Warning,
            code: "missing_description".into(),
            message: "Manifest `description` is empty — add a description for discoverability.".into(),
            fields: vec!["description".into()],
        });
    }

    // --- Capability tags check -----------------------------------------------
    if manifest.capability_tags.is_empty() {
        findings.push(ValidationFinding {
            level: ValidationLevel::Error,
            code: "no_capability_tags".into(),
            message: "At least one `capability_tag` is required. \
                      Agents without structured capabilities cannot be selected by the swarm planner."
                .into(),
            fields: vec!["capability_tags".into()],
        });
    }
    for tag in &manifest.capability_tags {
        if tag.namespace.trim().is_empty() || tag.name.trim().is_empty() {
            findings.push(ValidationFinding {
                level: ValidationLevel::Error,
                code: "invalid_capability_tag".into(),
                message: format!(
                    "Capability tag `{}:{}` has an empty namespace or name.",
                    tag.namespace, tag.name
                ),
                fields: vec!["capability_tags".into()],
            });
        }
    }

    // --- Required tools existence --------------------------------------------
    for tool_id in &manifest.required_tools {
        if !known_tools.is_empty() && !known_tools.contains(tool_id.as_str()) {
            findings.push(ValidationFinding {
                level: ValidationLevel::Error,
                code: "unknown_required_tool".into(),
                message: format!(
                    "Required tool `{tool_id}` is not registered. \
                     Register it or remove it from `required_tools`."
                ),
                fields: vec!["required_tools".into()],
            });
        }
    }

    // --- Service dependency checks -------------------------------------------
    for svc in &manifest.required_services {
        if let Some(env_key) = &svc.env_key {
            if !env_key.trim().is_empty() && std::env::var(env_key).is_err() {
                findings.push(ValidationFinding {
                    level: ValidationLevel::Warning,
                    code: "missing_service_env_key".into(),
                    message: format!(
                        "Required service `{}` expects env var `{env_key}` but it is not set.",
                        svc.name
                    ),
                    fields: vec!["required_services".into()],
                });
            }
        }
        if svc.id.trim().is_empty() {
            findings.push(ValidationFinding {
                level: ValidationLevel::Error,
                code: "service_missing_id".into(),
                message: format!("Service dependency `{}` is missing an `id`.", svc.name),
                fields: vec!["required_services".into()],
            });
        }
    }

    // --- Approval policy vs risk_level alignment ----------------------------
    match (&manifest.risk_level, &manifest.approval_policy) {
        (ManifestRiskLevel::High, ApprovalGatePolicy::None) => {
            findings.push(ValidationFinding {
                level: ValidationLevel::Error,
                code: "approval_policy_misaligned".into(),
                message: "High-risk agents must not have `approval_policy = none`. \
                          Use `pre_execute`, `post_draft`, or `conditional`."
                    .into(),
                fields: vec!["risk_level".into(), "approval_policy".into()],
            });
        }
        (ManifestRiskLevel::High, _) if !manifest.requires_approval => {
            findings.push(ValidationFinding {
                level: ValidationLevel::Error,
                code: "high_risk_no_approval_gate".into(),
                message: "High-risk agents must set `requires_approval = true`.".into(),
                fields: vec!["risk_level".into(), "requires_approval".into()],
            });
        }
        _ => {}
    }

    // --- Safe-for-auto-run vs risk_level alignment ---------------------------
    if manifest.risk_level == ManifestRiskLevel::High && manifest.safe_for_auto_run {
        findings.push(ValidationFinding {
            level: ValidationLevel::Warning,
            code: "high_risk_auto_run".into(),
            message: "High-risk agent is marked `safe_for_auto_run = true`. \
                      Review this setting unless you have explicit approval gates in place."
                .into(),
            fields: vec!["risk_level".into(), "safe_for_auto_run".into()],
        });
    }

    // --- Artifact schema required when produces_artifact is true ------------
    if manifest.produces_artifact && manifest.artifact_schema.is_none() {
        findings.push(ValidationFinding {
            level: ValidationLevel::Warning,
            code: "missing_artifact_schema".into(),
            message: "Agent declares `produces_artifact = true` but has no `artifact_schema`. \
                      Add a schema to make output typing explicit."
                .into(),
            fields: vec!["artifact_schema".into()],
        });
    }

    // --- Subagent support requires long runtime class -----------------------
    if manifest.supports_subtasks
        && manifest.expected_runtime_class
            != openfang_types::swarm::RuntimeClass::Long
    {
        findings.push(ValidationFinding {
            level: ValidationLevel::Warning,
            code: "subtasks_runtime_mismatch".into(),
            message: "Agents that support subtasks should declare `expected_runtime_class = long`."
                .into(),
            fields: vec!["supports_subtasks".into(), "expected_runtime_class".into()],
        });
    }

    let has_errors = findings
        .iter()
        .any(|f| f.level == ValidationLevel::Error);

    ManifestValidationResult {
        agent_id: manifest.id.clone(),
        valid: !has_errors,
        findings,
        validated_at: Utc::now(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_types::swarm::*;

    fn make_valid_manifest(id: &str) -> AgentSwarmManifest {
        AgentSwarmManifest {
            id: id.to_string(),
            name: id.to_string(),
            division: SwarmDivision::Execution,
            description: "A test agent".into(),
            version: "0.1.0".into(),
            author: "test".into(),
            risk_level: ManifestRiskLevel::Low,
            capability_tags: vec![CapabilityTag::new("code", "rust")],
            input_types: vec![],
            output_types: vec![],
            required_tools: vec![],
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
        }
    }

    #[test]
    fn valid_manifest_passes_validation() {
        let m = make_valid_manifest("coder");
        let tools = HashSet::new();
        let result = validate_manifest(&m, &tools);
        assert!(result.valid, "findings: {:?}", result.findings);
        assert!(result.findings.is_empty());
    }

    #[test]
    fn missing_id_is_error() {
        let mut m = make_valid_manifest("coder");
        m.id = "".into();
        let result = validate_manifest(&m, &HashSet::new());
        assert!(!result.valid);
        assert!(result.findings.iter().any(|f| f.code == "missing_id"));
    }

    #[test]
    fn no_capability_tags_is_error() {
        let mut m = make_valid_manifest("coder");
        m.capability_tags = vec![];
        let result = validate_manifest(&m, &HashSet::new());
        assert!(!result.valid);
        assert!(result.findings.iter().any(|f| f.code == "no_capability_tags"));
    }

    #[test]
    fn unknown_required_tool_is_error() {
        let mut m = make_valid_manifest("coder");
        m.required_tools = vec!["nonexistent_tool_xyz".into()];
        let mut tools = HashSet::new();
        tools.insert("file_read".into());
        let result = validate_manifest(&m, &tools);
        assert!(!result.valid);
        assert!(result.findings.iter().any(|f| f.code == "unknown_required_tool"));
    }

    #[test]
    fn known_required_tool_passes() {
        let mut m = make_valid_manifest("coder");
        m.required_tools = vec!["file_read".into()];
        let mut tools = HashSet::new();
        tools.insert("file_read".into());
        let result = validate_manifest(&m, &tools);
        assert!(result.valid);
    }

    #[test]
    fn high_risk_with_no_approval_is_error() {
        let mut m = make_valid_manifest("risky");
        m.risk_level = ManifestRiskLevel::High;
        m.approval_policy = ApprovalGatePolicy::None;
        m.requires_approval = true; // set to avoid the second check
        let result = validate_manifest(&m, &HashSet::new());
        assert!(!result.valid);
        assert!(result
            .findings
            .iter()
            .any(|f| f.code == "approval_policy_misaligned"));
    }

    #[test]
    fn high_risk_requires_approval_flag() {
        let mut m = make_valid_manifest("risky");
        m.risk_level = ManifestRiskLevel::High;
        m.approval_policy = ApprovalGatePolicy::PreExecute;
        m.requires_approval = false; // missing the flag
        let result = validate_manifest(&m, &HashSet::new());
        assert!(!result.valid);
        assert!(result
            .findings
            .iter()
            .any(|f| f.code == "high_risk_no_approval_gate"));
    }

    #[test]
    fn high_risk_with_approval_passes() {
        let mut m = make_valid_manifest("risky");
        m.risk_level = ManifestRiskLevel::High;
        m.approval_policy = ApprovalGatePolicy::PreExecute;
        m.requires_approval = true;
        m.safe_for_auto_run = false; // avoid warning
        let result = validate_manifest(&m, &HashSet::new());
        assert!(result.valid);
    }

    #[test]
    fn registry_stores_valid_manifest_only() {
        let registry = SwarmRegistry::new();
        let valid = make_valid_manifest("coder");
        let mut invalid = make_valid_manifest("broken");
        invalid.id = "".into();

        let r1 = registry.register(valid, &HashSet::new());
        let r2 = registry.register(invalid, &HashSet::new());

        assert!(r1.valid);
        assert!(!r2.valid);
        assert_eq!(registry.len(), 1);
        assert!(registry.get("coder").is_some());
        assert!(registry.get("broken").is_none());
    }

    #[test]
    fn registry_filter_by_capabilities_returns_matching_entries() {
        let registry = SwarmRegistry::new();
        let mut rust_coder = make_valid_manifest("coder");
        rust_coder.capability_tags = vec![CapabilityTag::new("code", "rust")];

        let mut researcher = make_valid_manifest("researcher");
        researcher.capability_tags = vec![CapabilityTag::new("research", "web")];

        registry.register_unchecked(rust_coder);
        registry.register_unchecked(researcher);

        let matches =
            registry.filter_by_capabilities(&[CapabilityTag::new("research", "web")]);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].agent_id, "researcher");
    }

    #[test]
    fn incompatible_agents_detected_in_group() {
        // The registry stores manifests; incompatibility enforcement is in the planner.
        let mut m = make_valid_manifest("alpha");
        m.incompatible_with = vec!["beta".into()];
        let registry = SwarmRegistry::new();
        registry.register_unchecked(m);
        let entry = registry.get("alpha").unwrap();
        assert!(entry.manifest.incompatible_with.contains(&"beta".to_string()));
    }
}
