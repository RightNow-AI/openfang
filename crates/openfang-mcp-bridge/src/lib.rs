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
//! - It does NOT define OpenFang's tool surface beyond a small built-in slice
//!   (see [`built_in_tools`]). The schemas declared here mirror
//!   `openfang_runtime::tool_runner::builtin_tool_definitions()` for the
//!   four ANAI-30 allowlisted tools — kept in lockstep deliberately.
//!
//! ## Project status
//!
//! ANAI-30 step 3. The bridge now:
//! - Defines the [`ToolDispatcher`] seam trait — the runtime (or, in the real
//!   topology, the daemon-bound IPC client) implements it.
//! - Registers the four-tool ANAI-30 surface (`file_read`, `file_list`,
//!   `agent_list`, `channel_send`) and translates `tools/call` into
//!   [`ToolDispatcher::call`].
//! - Filters its advertised tool list against [`ToolDispatcher::allowed_tools`]
//!   so an agent never sees tools its capabilities don't permit.
//!
//! Identity is bound at construction time. The IPC client implementation
//! lives in `main.rs`. ANAI-31 will replace the in-band-agent-id stub with
//! token-derived identity.

pub mod protocol;

use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, ServerHandler,
    model::*,
    service::RequestContext,
};

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
    /// For ANAI-30 this is the static four-tool slice; ANAI-31+ will derive
    /// it from `agent.toml`.
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

/// Built-in tool definitions advertised by the bridge in `tools/list`.
///
/// **These schemas mirror the equivalent entries in
/// `openfang_runtime::tool_runner::builtin_tool_definitions()`.** They are
/// duplicated here rather than imported because the bridge crate is
/// runtime-free by design (see crate-level docs). If the runtime's schemas
/// drift, update both sides.
///
/// Current surface:
/// - `file_read`, `file_list` — workspace-scoped, no kernel dependency
/// - `agent_list` — exercises `KernelHandle::list_agents`
/// - `channel_send` — exercises `KernelHandle::send_channel_message`,
///   one of the OpenFang-only capabilities a bare CC subprocess lacks
/// - `agent_send` — inter-agent messaging; first tool added past the
///   ANAI-30 slice. Per-agent gating via `OPENFANG_BRIDGE_ALLOWED`
///   (sourced from each agent's `agent.toml` capabilities) decides
///   whether any given bridge instance actually advertises it.
/// Default tool allowlist used by `main.rs` when [`OPENFANG_BRIDGE_ALLOWED`]
/// is unset (legacy/dev path). Lives in the library — not in `main.rs` — so
/// the bridge_ipc drift-catcher test in `openfang-api` can assert three-way
/// correspondence between this set, [`built_in_tools`], and the daemon-side
/// `bridge_ipc::ALLOWED_TOOLS`. Three files, one truth.
///
/// [`OPENFANG_BRIDGE_ALLOWED`]: ../../openfang_mcp_bridge/index.html
pub const DEFAULT_ALLOWED: &[&str] = &[
    "file_read",
    "file_list",
    "file_write",
    "web_fetch",
    "agent_list",
    "channel_send",
    "agent_send",
    "agent_spawn",
    "agent_kill",
    "memory_store",
    "memory_recall",
    "agent_activate",
    "agent_find",
    "shell_exec",
    "web_search",
    "apply_patch",
];

pub fn built_in_tools() -> Vec<Tool> {
    use serde_json::json;

    fn obj(v: serde_json::Value) -> std::sync::Arc<serde_json::Map<String, serde_json::Value>> {
        match v {
            serde_json::Value::Object(m) => std::sync::Arc::new(m),
            _ => std::sync::Arc::new(serde_json::Map::new()),
        }
    }

    vec![
        Tool::new(
            "file_read",
            "Read the contents of a file. Paths are relative to the agent workspace.",
            obj(json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "The file path to read" }
                },
                "required": ["path"]
            })),
        ),
        Tool::new(
            "file_list",
            "List files in a directory. Paths are relative to the agent workspace.",
            obj(json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "The directory path to list" }
                },
                "required": ["path"]
            })),
        ),
        // Mirrors `openfang_runtime::tool_runner` → `file_write`. Workspace-
        // scoped via the daemon-side `FS_SANDBOXED_TOOLS` gate in `bridge_ipc`.
        Tool::new(
            "file_write",
            "Write content to a file. Paths are relative to the agent workspace.",
            obj(json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "The file path to write to" },
                    "content": { "type": "string", "description": "The content to write" }
                },
                "required": ["path", "content"]
            })),
        ),
        // Mirrors `openfang_runtime::tool_runner` → `web_fetch`. No FS touch,
        // so it does not appear in `FS_SANDBOXED_TOOLS`; SSRF protection lives
        // in the runtime implementation.
        Tool::new(
            "web_fetch",
            "Fetch a URL with SSRF protection. Supports GET/POST/PUT/PATCH/DELETE. \
             For GET, HTML is converted to Markdown. For other methods, returns raw \
             response body.",
            obj(json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "The URL to fetch (http/https only)" },
                    "method": { "type": "string", "enum": ["GET","POST","PUT","PATCH","DELETE"], "description": "HTTP method (default: GET)" },
                    "headers": { "type": "object", "description": "Custom HTTP headers as key-value pairs" },
                    "body": { "type": "string", "description": "Request body for POST/PUT/PATCH" }
                },
                "required": ["url"]
            })),
        ),
        Tool::new(
            "agent_list",
            "List all currently running agents with their IDs, names, states, and models.",
            obj(json!({
                "type": "object",
                "properties": {}
            })),
        ),
        Tool::new(
            "channel_send",
            "Send a message to a user on a configured channel (email, telegram, slack, \
             discord, etc). For email: recipient is the email address; optionally set \
             subject. Use thread_id to reply in a specific thread/topic.",
            obj(json!({
                "type": "object",
                "properties": {
                    "channel": { "type": "string", "description": "Channel adapter name (e.g., 'email', 'telegram', 'slack', 'discord')" },
                    "recipient": { "type": "string", "description": "Platform-specific recipient identifier (email address, user ID, etc.)" },
                    "subject": { "type": "string", "description": "Optional subject line (used for email; ignored for other channels)" },
                    "message": { "type": "string", "description": "The message body to send" },
                    "thread_id": { "type": "string", "description": "Thread/topic ID to reply in" }
                },
                "required": ["channel", "recipient"]
            })),
        ),
        // Mirrors `openfang_runtime::tool_runner` → `agent_send`. Kept in
        // sync with that schema by hand; the bridge crate is runtime-free
        // by design and can't import the source. Per-agent gating via
        // `OPENFANG_BRIDGE_ALLOWED` decides whether this tool is actually
        // advertised + dispatchable for any given bridge instance.
        Tool::new(
            "agent_send",
            "Send a message to another agent and receive their response. \
             Accepts UUID or agent name. Use agent_find first to discover agents.",
            obj(json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "The target agent's UUID or name" },
                    "message": { "type": "string", "description": "The message to send to the agent" }
                },
                "required": ["agent_id", "message"]
            })),
        ),
        // Mirrors `openfang_runtime::tool_runner` → `agent_spawn`. High-
        // capability tool (creates new agents). Gated per-agent via
        // agent.toml; daemon-side Gate 2 enforces.
        Tool::new(
            "agent_spawn",
            "Spawn a new agent from a TOML manifest. Returns the new agent's ID and name.",
            obj(json!({
                "type": "object",
                "properties": {
                    "manifest_toml": {
                        "type": "string",
                        "description": "The agent manifest in TOML format (must include name, module, [model], and [capabilities])"
                    }
                },
                "required": ["manifest_toml"]
            })),
        ),
        // Mirrors `openfang_runtime::tool_runner` → `agent_kill`. High-
        // capability tool (terminates another agent). Gated per-agent.
        Tool::new(
            "agent_kill",
            "Kill (terminate) another agent by its ID.",
            obj(json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "The agent's UUID to kill" }
                },
                "required": ["agent_id"]
            })),
        ),
        // Mirrors `openfang_runtime::tool_runner` → `agent_activate`. Wakes
        // a Suspended/Crashed/Created agent. Terminated agents cannot be
        // revived.
        Tool::new(
            "agent_activate",
            "Activate (wake up) an inactive agent so it can receive messages \
             and process events. Use this when agent_list shows an agent in a \
             Suspended, Crashed, or Created state and you want to delegate work \
             to it via agent_send. Terminated agents cannot be revived.",
            obj(json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "The target agent's UUID or human-readable name"
                    }
                },
                "required": ["agent_id"]
            })),
        ),
        // Mirrors `openfang_runtime::tool_runner` → `memory_store`. Kernel-
        // managed shared memory; no FS sandbox needed (kernel scopes writes).
        Tool::new(
            "memory_store",
            "Store a value in shared memory accessible by all agents. Use for cross-agent coordination and data sharing.",
            obj(json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "The storage key" },
                    "value": { "type": "string", "description": "The value to store (JSON-encode objects/arrays, or pass a plain string)" }
                },
                "required": ["key", "value"]
            })),
        ),
        // Mirrors `openfang_runtime::tool_runner` → `memory_recall`. Read-only
        // companion to memory_store.
        Tool::new(
            "memory_recall",
            "Recall a value from shared memory by key.",
            obj(json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "The storage key to recall" }
                },
                "required": ["key"]
            })),
        ),
        // Mirrors `openfang_runtime::tool_runner` → `agent_find`. Read-only
        // discovery; pairs with agent_send.
        Tool::new(
            "agent_find",
            "Discover agents by name, tag, tool, or description. Use to find specialists before delegating work.",
            obj(json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query (matches agent name, tags, tools, description)" }
                },
                "required": ["query"]
            })),
        ),
        // Mirrors `openfang_runtime::tool_runner` → `shell_exec`. Daemon-side
        // `bridge_ipc` enforces workspace cwd sandbox + `exec_policy` (Full /
        // Allowlist / Denylist / None) from the calling agent's agent.toml.
        // The bridge advertises the tool unconditionally; per-agent capability
        // gating via `OPENFANG_BRIDGE_ALLOWED` decides whether any given bridge
        // instance actually exposes it, and Gate 2 in `bridge_ipc` rejects
        // commands that fall outside the agent's exec_policy.
        Tool::new(
            "shell_exec",
            "Execute a shell command and return its output. Runs in the agent's \
             workspace directory; commands are subject to the agent's exec_policy \
             (Full / Allowlist / Denylist / None) as declared in agent.toml.",
            obj(json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "The command to execute" },
                    "timeout_seconds": { "type": "integer", "description": "Timeout in seconds (default: 30)" }
                },
                "required": ["command"]
            })),
        ),
        // Mirrors `openfang_runtime::tool_runner` → `apply_patch`. Workspace-
        // scoped via the daemon-side `FS_SANDBOXED_TOOLS` gate in `bridge_ipc`
        // — `tool_apply_patch` resolves every patch-embedded path against
        // `workspace_root`, so the no-workspace fail-closed gate is critical
        // (same sibling-leak surface as `file_write`).
        //
        // Why this is bridged: serves as a surgical-edit alternative to
        // whole-file `file_write` rewrites for CC subprocesses that lack
        // CC's native `Edit` tool. A native `string_edit` follow-up may
        // replace this as the primary edit ergonomic; for now, apply_patch
        // is the closest thing we have to Edit's emit-cost profile.
        Tool::new(
            "apply_patch",
            "Apply a multi-hunk diff patch to add, update, move, or delete files. \
             Use this for targeted edits instead of full file overwrites. Paths in \
             the patch are resolved relative to the agent workspace.",
            obj(json!({
                "type": "object",
                "properties": {
                    "patch": {
                        "type": "string",
                        "description": "The patch in *** Begin Patch / *** End Patch format. Use *** Add File:, *** Update File:, *** Delete File: markers. Hunks use @@ headers with space (context), - (remove), + (add) prefixed lines."
                    }
                },
                "required": ["patch"]
            })),
        ),
        // Mirrors `openfang_runtime::tool_runner` → `web_search`. Pure-net,
        // no FS sandbox. Multi-provider (Tavily → Brave → Perplexity → DDG
        // fallback chain) configured via the daemon's `WebToolsContext`.
        Tool::new(
            "web_search",
            "Search the web using multiple providers (Tavily, Brave, Perplexity, \
             DuckDuckGo) with automatic fallback. Returns structured results with \
             titles, URLs, and snippets.",
            obj(json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "The search query" },
                    "max_results": { "type": "integer", "description": "Maximum number of results to return (default: 5, max: 20)" }
                },
                "required": ["query"]
            })),
        ),
    ]
}

/// The MCP server handler — wraps a [`ToolDispatcher`] and serves the
/// four-tool ANAI-30 surface over MCP.
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
                 Per-agent gating via OPENFANG_BRIDGE_ALLOWED narrows the advertised \
                 set to the calling agent's agent.toml capabilities."
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
            Err(ToolDispatchError::UnknownTool(name)) => Ok(CallToolResult::error(vec![
                Content::text(format!("unknown tool: {name}")),
            ])),
            Err(ToolDispatchError::NotPermitted(name)) => Ok(CallToolResult::error(vec![
                Content::text(format!("not permitted: {name}")),
            ])),
            Err(ToolDispatchError::InvalidArgs { tool, reason }) => {
                Ok(CallToolResult::error(vec![Content::text(format!(
                    "invalid args for {tool}: {reason}"
                ))]))
            }
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
    fn built_in_tools_surface() {
        let tools = built_in_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert_eq!(
            names,
            vec![
                "file_read",
                "file_list",
                "file_write",
                "web_fetch",
                "agent_list",
                "channel_send",
                "agent_send",
                "agent_spawn",
                "agent_kill",
                "agent_activate",
                "memory_store",
                "memory_recall",
                "agent_find",
                "shell_exec",
                "apply_patch",
                "web_search",
            ],
            "surface drift — update both this test and the runtime tool_runner \
             schema when adding or removing built-in bridge tools"
        );
    }

    #[test]
    fn permitted_tools_intersects_with_dispatcher_allowed() {
        let bridge = Bridge::new(Arc::new(StubDispatcher {
            agent: "a".into(),
            // Dispatcher permits only file_read of the built-in slice;
            // not_a_real_tool is unknown to the bridge and must be ignored.
            allowed: vec!["file_read".into(), "not_a_real_tool".into()],
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
}
