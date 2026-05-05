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
/// The set is intentionally limited to the ANAI-30 validation slice:
/// - `file_read`, `file_list` — workspace-scoped, no kernel dependency
/// - `agent_list` — exercises `KernelHandle::list_agents`
/// - `channel_send` — exercises `KernelHandle::send_channel_message`,
///   one of the OpenFang-only capabilities a bare CC subprocess lacks
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
                 ANAI-30 surface: file_read, file_list, agent_list, channel_send."
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
    fn built_in_tools_has_anai30_slice() {
        let tools = built_in_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert_eq!(
            names,
            vec!["file_read", "file_list", "agent_list", "channel_send"]
        );
    }

    #[test]
    fn permitted_tools_intersects_with_dispatcher_allowed() {
        let bridge = Bridge::new(Arc::new(StubDispatcher {
            agent: "a".into(),
            // Dispatcher permits only file_read of the built-in slice;
            // agent_send is unknown to the bridge and must be ignored.
            allowed: vec!["file_read".into(), "agent_send".into()],
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
