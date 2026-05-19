//! Per-agent execution context resolved from an [`AgentManifest`].
//!
//! Single source of truth so the bridge IPC dispatcher
//! (`openfang-api::bridge_ipc::dispatch_call`) and the HTTP `/mcp` endpoint
//! (`openfang-api::routes::mcp_http`) apply *identical* sandbox /
//! exec_policy / env-passthrough scoping when invoking
//! [`crate::tool_runner::execute_tool`].
//!
//! Closes **S3-01** (HTTP `/mcp` lacked exec_policy + env-passthrough parity
//! with the bridge IPC path) from the bridge-v2 audit. Prior to this fix,
//! a caller authenticated against the dashboard could invoke `shell_exec`
//! over HTTP `/mcp` and execute *outside* the manifest's `ExecPolicy` —
//! effectively `ExecPolicy::Full` regardless of the agent's actual policy.
//!
//! Both call sites resolve the registry entry independently (they have
//! their own permission gates and workspace plumbing), then funnel the
//! manifest through [`AgentExecContext::from_manifest`] to derive the
//! arguments threaded into `execute_tool`.

use openfang_types::agent::AgentManifest;
use openfang_types::config::ExecPolicy;

/// Resolved per-agent execution context. Built from an [`AgentManifest`]
/// at dispatch time; held by reference into the surrounding scope so the
/// borrowed slices can be passed straight to
/// [`crate::tool_runner::execute_tool`].
#[derive(Debug, Clone)]
pub struct AgentExecContext {
    /// Per-agent `[exec_policy]` override from `agent.toml`. `None` means
    /// the agent did not override the global policy — most call sites
    /// treat this as "fall back to global" by passing `None` straight
    /// through to `execute_tool`, which then enforces the daemon-global
    /// `ExecPolicy` from `config.toml`.
    pub exec_policy: Option<ExecPolicy>,

    /// Subset of host environment variables the kernel has explicitly
    /// authorized the agent to receive in `shell_exec` subprocesses
    /// (stored in `manifest.metadata["hand_allowed_env"]` —
    /// see `openfang_kernel::kernel::OpenFangKernel::ensure_hand_metadata`
    /// at kernel.rs:3932). Empty vector means "no extra env vars beyond
    /// the global `shell_env_passthrough` list."
    pub hand_allowed_env: Vec<String>,
}

impl AgentExecContext {
    /// Resolve context from the agent's manifest. Pure read-only helper —
    /// no I/O, no lock acquisition.
    pub fn from_manifest(manifest: &AgentManifest) -> Self {
        let exec_policy = manifest.exec_policy.clone();
        let hand_allowed_env: Vec<String> = manifest
            .metadata
            .get("hand_allowed_env")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        Self {
            exec_policy,
            hand_allowed_env,
        }
    }

    /// Borrow the `exec_policy` as the `Option<&ExecPolicy>` shape
    /// `execute_tool` expects.
    pub fn exec_policy_ref(&self) -> Option<&ExecPolicy> {
        self.exec_policy.as_ref()
    }

    /// Borrow the hand-allowed env list as the `Option<&[String]>` shape
    /// `execute_tool` expects. Returns `None` (not `Some(&[])`) for an
    /// empty list — matches the call-site convention in
    /// `agent_loop.rs:953` and `bridge_ipc.rs:559` where `None` selects
    /// the runtime default instead of "explicitly grant nothing."
    pub fn allowed_env(&self) -> Option<&[String]> {
        if self.hand_allowed_env.is_empty() {
            None
        } else {
            Some(&self.hand_allowed_env)
        }
    }
}

/// Tools whose dispatch is *unsafe without* an agent-bound exec_policy
/// resolution. If any of these are requested through a surface that
/// cannot bind the call to an [`AgentExecContext`] (e.g. HTTP `/mcp`
/// without `_agent_id`), the surface MUST fail-loud rather than fall
/// through to `execute_tool` with `exec_policy = None` — that path
/// degrades to the daemon-global policy, which is typically `Full` on
/// developer setups and silently bypasses every manifest gate the
/// operator authored.
///
/// Keep this list tight. Anything added here closes one HTTP-side
/// privilege-escalation vector and breaks the corresponding caller
/// pattern simultaneously.
pub const EXEC_POLICY_REQUIRED_TOOLS: &[&str] = &["shell_exec"];

/// Returns `true` when `tool_name` is in [`EXEC_POLICY_REQUIRED_TOOLS`].
/// Constant-time string compare; safe to call per-request.
pub fn requires_exec_policy(tool_name: &str) -> bool {
    EXEC_POLICY_REQUIRED_TOOLS.contains(&tool_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_types::config::{ExecPolicy, ExecSecurityMode};
    use serde_json::json;

    fn manifest_with_metadata(kv: Vec<(&str, serde_json::Value)>) -> AgentManifest {
        let mut m = AgentManifest::default();
        for (k, v) in kv {
            m.metadata.insert(k.to_string(), v);
        }
        m
    }

    #[test]
    fn default_manifest_yields_no_policy_no_env() {
        let m = AgentManifest::default();
        let ctx = AgentExecContext::from_manifest(&m);
        assert!(ctx.exec_policy_ref().is_none());
        assert!(ctx.allowed_env().is_none());
        assert!(ctx.hand_allowed_env.is_empty());
    }

    #[test]
    fn manifest_with_exec_policy_propagates() {
        let mut m = AgentManifest::default();
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["echo".into(), "ls".into()],
            ..Default::default()
        };
        m.exec_policy = Some(policy.clone());

        let ctx = AgentExecContext::from_manifest(&m);
        let resolved = ctx.exec_policy_ref().expect("policy should be present");
        assert_eq!(resolved.mode, ExecSecurityMode::Allowlist);
        assert_eq!(resolved.allowed_commands, vec!["echo", "ls"]);
    }

    #[test]
    fn hand_allowed_env_array_is_read_from_metadata() {
        let m = manifest_with_metadata(vec![(
            "hand_allowed_env",
            json!(["OPENAI_API_KEY", "GITHUB_TOKEN"]),
        )]);
        let ctx = AgentExecContext::from_manifest(&m);
        assert_eq!(
            ctx.hand_allowed_env,
            vec!["OPENAI_API_KEY".to_string(), "GITHUB_TOKEN".to_string()]
        );
        assert_eq!(
            ctx.allowed_env(),
            Some(&["OPENAI_API_KEY".to_string(), "GITHUB_TOKEN".to_string()][..])
        );
    }

    #[test]
    fn malformed_hand_allowed_env_falls_back_to_empty() {
        // S3-01 fail-closed: a malformed metadata value should NOT propagate
        // as `Some(...)` — it would partially-bind env passthrough on a
        // best-effort basis, which is the inverse of what an operator wants
        // out of a security gate. Treat malformed as "no override".
        let m = manifest_with_metadata(vec![("hand_allowed_env", json!("not an array"))]);
        let ctx = AgentExecContext::from_manifest(&m);
        assert!(ctx.hand_allowed_env.is_empty());
        assert!(ctx.allowed_env().is_none());
    }

    #[test]
    fn empty_hand_allowed_env_returns_none_not_empty_slice() {
        let m = manifest_with_metadata(vec![("hand_allowed_env", json!([]))]);
        let ctx = AgentExecContext::from_manifest(&m);
        // Parity with bridge_ipc.rs:559 — `None` means "fall through to
        // runtime default", not "explicitly grant nothing".
        assert!(ctx.allowed_env().is_none());
    }

    #[test]
    fn requires_exec_policy_only_flags_shell_exec() {
        assert!(requires_exec_policy("shell_exec"));
        assert!(!requires_exec_policy("file_read"));
        assert!(!requires_exec_policy("agent_send"));
        assert!(!requires_exec_policy("web_fetch"));
        assert!(!requires_exec_policy(""));
    }

    #[test]
    fn exec_policy_required_tools_is_non_empty_and_stable() {
        // Drift-catcher: if someone removes shell_exec from this list
        // without replacing it with an equivalent gate elsewhere, the
        // S3-01 fix regresses silently. Force the change to land
        // intentionally.
        assert!(EXEC_POLICY_REQUIRED_TOOLS.contains(&"shell_exec"));
    }
}
