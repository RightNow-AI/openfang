//! # openfang-mcp-bridge
//!
//! MCP (Model Context Protocol) bridge for OpenFang Agent OS.
//!
//! ## What this crate is
//!
//! A protocol adapter that exposes OpenFang's tool surface to Claude Code
//! subprocesses (and other MCP clients) over stdio. One MCP server instance
//! per parent agent, scoped to that agent's identity and the capabilities
//! declared in its `agent.toml`.
//!
//! ## What this crate is NOT
//!
//! - It does NOT depend on `openfang-runtime`, `openfang-kernel`, or
//!   `openfang-memory` directly. The bridge consumes a narrow
//!   [`ToolDispatcher`] trait that the runtime exposes; the runtime owns
//!   identity, the kernel owns dispatch, the memory subsystem stays untouched.
//! - It does NOT define OpenFang's tool surface. As of ANAI-32 the bridge
//!   pulls schemas directly from
//!   `openfang_types::tool::registry::builtin_tool_definitions()` — the
//!   single source of truth shared with `openfang-runtime`. The bridge layers
//!   a small *policy* on top via [`BRIDGE_DENY`]: a curated list of kernel
//!   tools the bridge refuses to advertise to MCP clients regardless of the
//!   per-agent manifest (e.g. substrate-specific things we don't want CC to
//!   touch). Per-agent allowlists still apply on top of that.
//!
//! ## Project status
//!
//! ANAI-32 (capability enforcement). The bridge now:
//! - Defines the [`ToolDispatcher`] seam trait — the runtime (or, in the real
//!   topology, the daemon-bound IPC client) implements it.
//! - Advertises the full kernel built-in surface, sourced from
//!   `openfang_types::tool::registry::builtin_tool_definitions()` — the same
//!   registry the runtime uses for its LLM-driver tool list. CC sees what an
//!   API model would see (modulo per-agent allowlist + [`BRIDGE_DENY`]).
//! - Filters its advertised tool list against [`ToolDispatcher::allowed_tools`]
//!   (derived from `agent.toml`) so an agent never sees tools its capabilities
//!   don't permit, and against [`BRIDGE_DENY`] for substrate-level overrides.
//!
//! Identity is bound at construction time, but is still face-value from the
//! caller-supplied `agent_id` in the IPC handshake. The IPC client
//! implementation lives in `main.rs`. ANAI-31 will replace the in-band
//! agent-id stub with token-derived identity.

pub mod protocol;

use std::sync::Arc;

use rmcp::{model::*, service::RequestContext, ErrorData as McpError, ServerHandler};

/// Narrow seam between the bridge and the OpenFang runtime.
///
/// The runtime (or, in the real topology, an IPC-backed adapter that talks
/// to the daemon) implements this trait and hands an `Arc<dyn ToolDispatcher>`
/// to the bridge at startup, scoped to a specific agent identity. The bridge
/// translates incoming MCP `tools/call` requests into [`ToolDispatcher::call`]
/// invocations.
///
/// **Identity is bound at construction time, not per-call.** A bridge instance
/// only ever speaks for one agent. This is the security invariant tracked by
/// ANAI-31.
#[async_trait::async_trait]
pub trait ToolDispatcher: Send + Sync {
    /// Identity of the agent this dispatcher is bound to. Used for audit
    /// logging and for cross-checking against `agent.toml` capabilities.
    fn agent_id(&self) -> &str;

    /// List of tool names this dispatcher will accept, derived from
    /// `agent.toml` capabilities. The bridge filters its advertised tool list
    /// against this set.
    ///
    /// As of ANAI-32 this is sourced from the manifest's
    /// `capabilities.tools` (plumbed via `OPENFANG_BRIDGE_ALLOWED`).
    fn allowed_tools(&self) -> Vec<String>;

    /// Invoke a tool by name with a JSON argument blob. The dispatcher is
    /// responsible for capability enforcement; the bridge MUST NOT assume the
    /// caller is trusted.
    async fn call(
        &self,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<DispatchOk, ToolDispatchError>;
}

/// Successful dispatch outcome. Maps onto MCP's `CallToolResult` shape:
/// `content` becomes a single text content block, `is_error` becomes the
/// `isError` flag.
///
/// Note the distinction from [`ToolDispatchError`]: a tool that ran but
/// reported a failure to the model is `Ok(DispatchOk { is_error: true })`.
/// `Err(_)` means dispatch itself failed (unknown tool, not permitted,
/// transport error) — the bridge surfaces those as MCP errors instead.
#[derive(Debug, Clone)]
pub struct DispatchOk {
    pub content: String,
    pub is_error: bool,
}

/// Errors a [`ToolDispatcher`] can return. Bridge maps these to MCP errors.
#[derive(Debug, thiserror::Error)]
pub enum ToolDispatchError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("tool '{0}' not permitted for this agent")]
    NotPermitted(String),
    #[error("invalid arguments for tool '{tool}': {reason}")]
    InvalidArgs { tool: String, reason: String },
    #[error("tool execution failed: {0}")]
    Execution(#[from] anyhow::Error),
}

/// Substrate-level denylist. Tools in this list are NEVER advertised by the
/// bridge regardless of the per-agent manifest. Use sparingly — the right
/// place to gate per-agent capabilities is `agent.toml`. This list exists for
/// kernel tools that don't make sense to expose to MCP clients at all (e.g.
/// substrate-specific or driver-internal plumbing), or as a safety hatch when
/// CC's native surface evolves and we need to pre-empt a clash before
/// updating the kernel registry.
///
/// Treat additions to this list like a security review: every entry needs a
/// reason in the comment.
pub const BRIDGE_DENY: &[&str] = &[
    // (Empty for now. CC sees the full kernel surface, gated per-agent by
    // `agent.toml` and per-call by the dispatcher.)
];

/// Adapter from the canonical `ToolDefinition` (in `openfang-types`) into
/// rmcp's `Tool` shape. Pure, infallible — `input_schema` is already a JSON
/// object in the registry by construction.
fn to_rmcp_tool(def: &openfang_types::tool::ToolDefinition) -> Tool {
    let schema = match &def.input_schema {
        serde_json::Value::Object(m) => std::sync::Arc::new(m.clone()),
        _ => std::sync::Arc::new(serde_json::Map::new()),
    };
    Tool::new(def.name.clone(), def.description.clone(), schema)
}

/// Built-in tool definitions advertised by the bridge in `tools/list`.
///
/// Pulls from the kernel's canonical registry
/// (`openfang_types::tool::registry::builtin_tool_definitions`) and applies
/// the substrate-level [`BRIDGE_DENY`] policy. Per-agent capability filtering
/// happens later in [`Bridge::permitted_tools`] using the manifest allowlist.
///
/// Design note: we deliberately do NOT carve out `web_fetch` / `web_search`
/// or any other tool here just because CC has a native equivalent. Claude
/// Code is treated as if it were an API-only model — the kernel's tools go
/// through OpenFang, period. Locking down the corresponding CC natives is
/// ANAI-37's job (CC `disallowedTools` emission), not the bridge surface's.
pub fn built_in_tools() -> Vec<Tool> {
    openfang_types::tool::registry::builtin_tool_definitions()
        .iter()
        .filter(|d| !BRIDGE_DENY.contains(&d.name.as_str()))
        .map(to_rmcp_tool)
        .collect()
}

/// The MCP server handler — wraps a [`ToolDispatcher`] and serves the
/// ANAI-32 tool surface over MCP.
///
/// Filtering: `tools/list` advertises only tools that appear in *both*
/// [`built_in_tools`] *and* [`ToolDispatcher::allowed_tools`]. `tools/call`
/// double-checks before dispatch — defense in depth, since the dispatcher
/// itself enforces permissions too.
#[derive(Clone)]
pub struct Bridge {
    dispatcher: Arc<dyn ToolDispatcher>,
}

impl Bridge {
    pub fn new(dispatcher: Arc<dyn ToolDispatcher>) -> Self {
        Self { dispatcher }
    }

    /// Tools the bridge will both advertise and accept calls for, given the
    /// dispatcher's allowed set.
    fn permitted_tools(&self) -> Vec<Tool> {
        let allowed = self.dispatcher.allowed_tools();
        built_in_tools()
            .into_iter()
            .filter(|t| allowed.iter().any(|a| a.as_str() == t.name.as_ref()))
            .collect()
    }
}

impl ServerHandler for Bridge {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::from_build_env())
            .with_protocol_version(ProtocolVersion::V_2024_11_05)
            .with_instructions(
                "OpenFang MCP bridge. Exposes OpenFang's tool surface to MCP clients, \
                 scoped to a single parent agent's identity and capabilities. \
                 ANAI-32 surface: file_read, file_write, file_list, shell_exec, \
                 agent_send, agent_list, memory_store, memory_recall, channel_send. \
                 Advertised set is intersected with the agent's manifest allowlist."
                    .to_string(),
            )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<rmcp::service::RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(self.permitted_tools()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<rmcp::service::RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let tool_name = request.name.as_ref();
        let args = request
            .arguments
            .map(serde_json::Value::Object)
            .unwrap_or(serde_json::Value::Null);

        // Defense-in-depth: re-check the allowlist before crossing the seam.
        // The dispatcher will enforce again; that's intentional.
        let allowed = self.dispatcher.allowed_tools();
        if !allowed.iter().any(|a| a == tool_name) {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "tool '{tool_name}' not permitted for this agent"
            ))]));
        }

        match self.dispatcher.call(tool_name, args).await {
            Ok(DispatchOk { content, is_error }) => {
                let blocks = vec![Content::text(content)];
                Ok(if is_error {
                    CallToolResult::error(blocks)
                } else {
                    CallToolResult::success(blocks)
                })
            }
            Err(ToolDispatchError::UnknownTool(name)) => {
                Ok(CallToolResult::error(vec![Content::text(format!(
                    "unknown tool: {name}"
                ))]))
            }
            Err(ToolDispatchError::NotPermitted(name)) => {
                Ok(CallToolResult::error(vec![Content::text(format!(
                    "not permitted: {name}"
                ))]))
            }
            Err(ToolDispatchError::InvalidArgs { tool, reason }) => Ok(CallToolResult::error(
                vec![Content::text(format!("invalid args for {tool}: {reason}"))],
            )),
            Err(ToolDispatchError::Execution(e)) => Ok(CallToolResult::error(vec![Content::text(
                format!("tool execution failed: {e}"),
            )])),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubDispatcher {
        agent: String,
        allowed: Vec<String>,
        canned: DispatchOk,
    }

    #[async_trait::async_trait]
    impl ToolDispatcher for StubDispatcher {
        fn agent_id(&self) -> &str {
            &self.agent
        }
        fn allowed_tools(&self) -> Vec<String> {
            self.allowed.clone()
        }
        async fn call(
            &self,
            _tool_name: &str,
            _args: serde_json::Value,
        ) -> Result<DispatchOk, ToolDispatchError> {
            Ok(self.canned.clone())
        }
    }

    #[test]
    fn built_in_tools_advertises_full_kernel_surface() {
        // Bridge advertises every kernel built-in (minus BRIDGE_DENY).
        // Treat CC as if it were an API model — same surface as the runtime.
        use std::collections::HashSet;
        let bridge_names: HashSet<String> = built_in_tools()
            .into_iter()
            .map(|t| t.name.into_owned())
            .collect();
        let kernel_names: HashSet<String> =
            openfang_types::tool::registry::builtin_tool_definitions()
                .into_iter()
                .map(|d| d.name)
                .collect();
        let denied: HashSet<String> = BRIDGE_DENY.iter().map(|s| s.to_string()).collect();
        let expected: HashSet<String> = kernel_names.difference(&denied).cloned().collect();
        assert_eq!(bridge_names, expected);
    }

    #[test]
    fn anai32_canonical_surface_is_present() {
        // Concrete sanity: the nine ANAI-32 tools must round-trip through
        // bridge advertisement. If anything in this list ever drops out, the
        // bridge can no longer serve the original capability-enforcement
        // surface and someone needs to know.
        let names: std::collections::HashSet<String> = built_in_tools()
            .into_iter()
            .map(|t| t.name.into_owned())
            .collect();
        for required in [
            "file_read",
            "file_write",
            "file_list",
            "shell_exec",
            "agent_send",
            "agent_list",
            "memory_store",
            "memory_recall",
            "channel_send",
        ] {
            assert!(names.contains(required), "missing {required}");
        }
    }

    #[test]
    fn bridge_deny_entries_must_exist_in_kernel_registry() {
        // Drift sentinel: every name in BRIDGE_DENY must be a real kernel
        // tool. A stale entry (e.g. a tool that was renamed in the kernel)
        // would silently fail open. Catch it at test time instead.
        let kernel_names: std::collections::HashSet<String> =
            openfang_types::tool::registry::builtin_tool_definitions()
                .into_iter()
                .map(|d| d.name)
                .collect();
        for denied in BRIDGE_DENY {
            assert!(
                kernel_names.contains(*denied),
                "BRIDGE_DENY entry '{denied}' is not in the kernel registry — \
                 either the tool was renamed/removed (update BRIDGE_DENY) or \
                 the entry was a typo from the start"
            );
        }
    }

    #[test]
    fn permitted_tools_intersects_with_dispatcher_allowed() {
        let bridge = Bridge::new(Arc::new(StubDispatcher {
            agent: "a".into(),
            // Dispatcher permits file_read (in the built-in set) and
            // a name that does not exist in the kernel registry (must be
            // ignored by the bridge advertisement).
            allowed: vec!["file_read".into(), "nonexistent_tool_xyz".into()],
            canned: DispatchOk {
                content: String::new(),
                is_error: false,
            },
        }));
        let names: Vec<String> = bridge
            .permitted_tools()
            .into_iter()
            .map(|t| t.name.into_owned())
            .collect();
        assert_eq!(names, vec!["file_read".to_string()]);
    }

    #[test]
    fn permitted_tools_filters_to_manifest_subset() {
        // Realistic case: a coder agent with the full ANAI-32 manifest
        // surface (minus web_fetch, which is CC-native) should see all
        // bridge-advertised tools except channel_send.
        let bridge = Bridge::new(Arc::new(StubDispatcher {
            agent: "coder".into(),
            allowed: vec![
                "file_read".into(),
                "file_write".into(),
                "file_list".into(),
                "shell_exec".into(),
                "agent_send".into(),
                "agent_list".into(),
                "memory_store".into(),
                "memory_recall".into(),
            ],
            canned: DispatchOk {
                content: String::new(),
                is_error: false,
            },
        }));
        let names: Vec<String> = bridge
            .permitted_tools()
            .into_iter()
            .map(|t| t.name.into_owned())
            .collect();
        assert_eq!(
            names,
            vec![
                "file_read".to_string(),
                "file_write".to_string(),
                "file_list".to_string(),
                "shell_exec".to_string(),
                "agent_send".to_string(),
                "agent_list".to_string(),
                "memory_store".to_string(),
                "memory_recall".to_string(),
            ]
        );
    }
}
