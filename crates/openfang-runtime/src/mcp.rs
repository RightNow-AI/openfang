//! MCP (Model Context Protocol) client — connect to external MCP servers.
//!
//! Uses the official `rmcp` SDK for protocol handling.  Supports:
//! - **stdio**: subprocess with JSON-RPC over stdin/stdout
//! - **sse**: deprecated HTTP+SSE transport (protocol version 2024-11-05)
//! - **http**: Streamable HTTP transport (protocol version 2025-03-26+)
//!
//! All MCP tools are namespaced with `mcp_{server}_{tool}` to prevent collisions.

use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use http::{HeaderName, HeaderValue};
use openfang_types::event::McpEvent;
use openfang_types::tool::ToolDefinition;
use rmcp::model::{
    CallToolRequestParams, ClientCapabilities, ClientInfo, Implementation,
    ResourceUpdatedNotificationParam, ServerCapabilities,
};
use rmcp::service::{MaybeSendFuture, NotificationContext, RunningService};
use rmcp::{ClientHandler, RoleClient, ServiceExt};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Configuration types
// ---------------------------------------------------------------------------

/// Configuration for an MCP server connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Display name for this server (used in tool namespacing).
    pub name: String,
    /// Transport configuration.
    pub transport: McpTransport,
    /// Request timeout in seconds (default: 30).
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Environment variables to pass through to the subprocess (sandboxed).
    #[serde(default)]
    pub env: Vec<String>,
    /// Extra HTTP headers to send with every SSE / Streamable-HTTP request.
    /// Each entry is `"Header-Name: value"`.  Useful for authentication
    /// (`Authorization: Bearer <token>`), API keys (`X-Api-Key: ...`),
    /// or any custom headers required by a remote MCP server.
    #[serde(default)]
    pub headers: Vec<String>,
    /// Allow server-initiated MCP notifications to enter the event bus.
    #[serde(default)]
    pub allow_push_events: bool,
    /// Maximum queued push notifications before older events are dropped.
    #[serde(default = "default_push_queue_size")]
    pub push_queue_size: usize,
    /// Maximum server-initiated notifications accepted per minute.
    #[serde(default = "default_push_rate_limit_per_minute")]
    pub push_rate_limit_per_minute: u32,
}

fn default_timeout() -> u64 {
    60
}

fn default_push_queue_size() -> usize {
    256
}

fn default_push_rate_limit_per_minute() -> u32 {
    600
}

/// Transport type for MCP server connections.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpTransport {
    /// Subprocess with JSON-RPC over stdin/stdout.
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
    },
    /// Deprecated HTTP+SSE transport (protocol version 2024-11-05).
    /// Uses POST for sending and SSE for receiving.
    Sse { url: String },
    /// Streamable HTTP transport (MCP 2025-03-26+).
    /// Single endpoint, client MUST send Accept: application/json, text/event-stream.
    /// Server responds with either JSON or SSE stream.
    /// Supports Mcp-Session-Id for session management.
    Http { url: String },
}

// ---------------------------------------------------------------------------
// Connection types
// ---------------------------------------------------------------------------

/// An active connection to an MCP server.
pub struct McpConnection {
    /// Configuration for this connection.
    config: McpServerConfig,
    /// Tools discovered from the server via tools/list.
    tools: Vec<ToolDefinition>,
    /// Map from namespaced tool name → original tool name from the server.
    /// Needed because `normalize_name` replaces hyphens with underscores,
    /// but the server expects the original name (e.g. "list-connections").
    original_names: HashMap<String, String>,
    /// The rmcp client handle — type-erased because the concrete type
    /// depends on which transport was used (stdio vs HTTP).
    client: RunningService<RoleClient, OpenFangMcpClient>,
}

/// Create a bounded queue for MCP push notifications.
///
/// When the queue is full, the oldest queued notifications are dropped and a
/// `NotificationDropped` event is enqueued ahead of the newest event.
pub fn mcp_notification_channel(
    capacity: usize,
) -> (McpNotificationSender, McpNotificationReceiver) {
    let inner = Arc::new(McpNotificationChannelInner {
        queue: Mutex::new(VecDeque::new()),
        notify: Notify::new(),
        capacity: capacity.max(2),
    });

    (
        McpNotificationSender {
            inner: Arc::clone(&inner),
        },
        McpNotificationReceiver { inner },
    )
}

#[derive(Clone)]
pub struct McpNotificationSender {
    inner: Arc<McpNotificationChannelInner>,
}

pub struct McpNotificationReceiver {
    inner: Arc<McpNotificationChannelInner>,
}

struct McpNotificationChannelInner {
    queue: Mutex<VecDeque<McpEvent>>,
    notify: Notify,
    capacity: usize,
}

impl McpNotificationSender {
    pub fn push(&self, event: McpEvent) {
        let dropped_event = {
            let mut queue = self
                .inner
                .queue
                .lock()
                .expect("MCP notification queue poisoned");
            let required_slots = if queue.len() >= self.inner.capacity {
                2
            } else {
                1
            };
            let overflow = queue
                .len()
                .saturating_add(required_slots)
                .saturating_sub(self.inner.capacity);
            let mut dropped_count = 0;

            for _ in 0..overflow {
                if queue.pop_front().is_some() {
                    dropped_count += 1;
                }
            }

            let dropped_event = if dropped_count > 0 {
                Some(McpEvent::NotificationDropped {
                    server: mcp_event_server(&event).to_string(),
                    kind: mcp_event_kind(&event).to_string(),
                    dropped_count,
                })
            } else {
                None
            };

            if let Some(dropped) = dropped_event.clone() {
                queue.push_back(dropped);
            }
            queue.push_back(event);
            dropped_event
        };

        if let Some(McpEvent::NotificationDropped {
            server,
            kind,
            dropped_count,
        }) = dropped_event
        {
            warn!(
                server = %server,
                kind = %kind,
                dropped_count,
                "MCP notification queue dropped older events"
            );
        }

        self.inner.notify.notify_waiters();
    }
}

impl McpNotificationReceiver {
    pub async fn recv(&self) -> McpEvent {
        loop {
            if let Some(event) = self.try_recv() {
                return event;
            }
            self.inner.notify.notified().await;
        }
    }

    pub fn try_recv(&self) -> Option<McpEvent> {
        self.inner
            .queue
            .lock()
            .expect("MCP notification queue poisoned")
            .pop_front()
    }
}

#[derive(Clone)]
struct OpenFangMcpClient {
    info: ClientInfo,
    server_name: String,
    allow_push_events: bool,
    notification_tx: Option<McpNotificationSender>,
    rate_limiter: Option<Arc<DefaultDirectRateLimiter>>,
}

impl OpenFangMcpClient {
    fn new(
        server_name: String,
        allow_push_events: bool,
        push_rate_limit_per_minute: u32,
        notification_tx: Option<McpNotificationSender>,
    ) -> Self {
        let rate_limiter = NonZeroU32::new(push_rate_limit_per_minute)
            .map(|limit| Arc::new(RateLimiter::direct(Quota::per_minute(limit))));
        Self {
            info: ClientInfo::new(
                ClientCapabilities::default(),
                Implementation::new("openfang", env!("CARGO_PKG_VERSION")),
            ),
            server_name,
            allow_push_events,
            notification_tx,
            rate_limiter,
        }
    }

    fn dispatch(&self, event: McpEvent) {
        if !self.allow_push_events {
            debug!(
                server = %self.server_name,
                kind = mcp_event_kind(&event),
                "MCP push notification ignored because allow_push_events is false"
            );
            return;
        }

        let Some(tx) = &self.notification_tx else {
            return;
        };

        if let Some(limiter) = &self.rate_limiter {
            if limiter.check().is_err() {
                tx.push(McpEvent::NotificationDropped {
                    server: mcp_event_server(&event).to_string(),
                    kind: mcp_event_kind(&event).to_string(),
                    dropped_count: 1,
                });
                warn!(
                    server = %self.server_name,
                    kind = mcp_event_kind(&event),
                    "MCP push notification dropped by rate limit"
                );
                return;
            }
        }

        tx.push(event);
    }
}

impl ClientHandler for OpenFangMcpClient {
    fn on_resource_updated(
        &self,
        params: ResourceUpdatedNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) -> impl Future<Output = ()> + MaybeSendFuture + '_ {
        async move {
            self.dispatch(McpEvent::ResourceUpdated {
                server: self.server_name.clone(),
                uri: params.uri,
            });
        }
    }

    fn on_resource_list_changed(
        &self,
        _context: NotificationContext<RoleClient>,
    ) -> impl Future<Output = ()> + MaybeSendFuture + '_ {
        async move {
            self.dispatch(McpEvent::ResourceListChanged {
                server: self.server_name.clone(),
            });
        }
    }

    fn on_tool_list_changed(
        &self,
        _context: NotificationContext<RoleClient>,
    ) -> impl Future<Output = ()> + MaybeSendFuture + '_ {
        async move {
            self.dispatch(McpEvent::ToolListChanged {
                server: self.server_name.clone(),
            });
        }
    }

    fn on_prompt_list_changed(
        &self,
        _context: NotificationContext<RoleClient>,
    ) -> impl Future<Output = ()> + MaybeSendFuture + '_ {
        async move {
            self.dispatch(McpEvent::PromptListChanged {
                server: self.server_name.clone(),
            });
        }
    }

    fn get_info(&self) -> ClientInfo {
        self.info.clone()
    }
}

// ---------------------------------------------------------------------------
// McpConnection implementation
// ---------------------------------------------------------------------------

impl McpConnection {
    /// Connect to an MCP server, perform handshake, and discover tools.
    pub async fn connect(config: McpServerConfig) -> Result<Self, String> {
        Self::connect_with_notifications(config, None).await
    }

    /// Connect to an MCP server with a push-notification queue.
    pub async fn connect_with_notifications(
        config: McpServerConfig,
        notification_tx: Option<McpNotificationSender>,
    ) -> Result<Self, String> {
        let client_handler = OpenFangMcpClient::new(
            config.name.clone(),
            config.allow_push_events,
            config.push_rate_limit_per_minute,
            notification_tx,
        );
        let client = match &config.transport {
            McpTransport::Stdio { command, args } => {
                Self::connect_stdio(command, args, &config.env, client_handler).await?
            }
            McpTransport::Sse { url } | McpTransport::Http { url } => {
                Self::connect_http(url, &config.headers, client_handler).await?
            }
        };

        let mut conn = Self {
            config,
            tools: Vec::new(),
            original_names: HashMap::new(),
            client,
        };

        // Discover tools
        conn.discover_tools().await?;

        info!(
            server = %conn.config.name,
            tools = conn.tools.len(),
            "MCP server connected"
        );

        Ok(conn)
    }

    /// Discover available tools via `tools/list`.
    async fn discover_tools(&mut self) -> Result<(), String> {
        let tools = self
            .client
            .list_all_tools()
            .await
            .map_err(|e| format!("Failed to list MCP tools: {e}"))?;

        let server_name = &self.config.name;
        for tool in &tools {
            let raw_name = &tool.name;
            let description = tool.description.as_deref().unwrap_or("");

            let input_schema = serde_json::to_value(&tool.input_schema)
                .unwrap_or(serde_json::json!({"type": "object"}));

            // Namespace: mcp_{server}_{tool}
            let namespaced = format_mcp_tool_name(server_name, raw_name);

            // Store original name so we can send it back to the server
            self.original_names
                .insert(namespaced.clone(), raw_name.to_string());

            self.tools.push(ToolDefinition {
                name: namespaced,
                description: format!("[MCP:{server_name}] {description}"),
                input_schema,
            });
        }

        Ok(())
    }

    /// Call a tool on the MCP server.
    ///
    /// `name` should be the namespaced name (mcp_{server}_{tool}).
    pub async fn call_tool(
        &mut self,
        name: &str,
        arguments: &serde_json::Value,
    ) -> Result<String, String> {
        // Look up the original tool name from the server (preserves hyphens etc.)
        let raw_name: String = self
            .original_names
            .get(name)
            .cloned()
            .or_else(|| strip_mcp_prefix(&self.config.name, name).map(|s| s.to_string()))
            .unwrap_or_else(|| name.to_string());

        let args = arguments.as_object().cloned().unwrap_or_default();

        debug!(tool = %raw_name, server = %self.config.name, "MCP tool call");

        let params = CallToolRequestParams::new(raw_name).with_arguments(args);

        let result = self
            .client
            .call_tool(params)
            .await
            .map_err(|e| format!("MCP tool call failed: {e}"))?;

        // Extract text content from the response.
        // `Content` is `Annotated<RawContent>` which Derefs to `RawContent`.
        let texts: Vec<&str> = result
            .content
            .iter()
            .filter_map(|item| item.as_text().map(|tc| tc.text.as_str()))
            .collect();

        if texts.is_empty() {
            // Fallback: serialize the entire result
            Ok(serde_json::to_string(&result).unwrap_or_default())
        } else {
            Ok(texts.join("\n"))
        }
    }

    /// Get the discovered tool definitions.
    pub fn tools(&self) -> &[ToolDefinition] {
        &self.tools
    }

    /// Get the server name.
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Subscribe to server notifications for a resource URI.
    pub async fn subscribe_resource(&self, uri: impl Into<String>) -> Result<(), String> {
        let uri = uri.into();
        let server_capabilities = self
            .client
            .peer_info()
            .map(|info| info.capabilities.clone())
            .unwrap_or_else(ServerCapabilities::default);

        if !server_supports_resource_subscribe(&server_capabilities) {
            return Err(format!(
                "MCP server '{}' does not advertise resource subscription support",
                self.config.name
            ));
        }

        self.client
            .subscribe(rmcp::model::SubscribeRequestParams::new(uri))
            .await
            .map_err(|e| format!("MCP resource subscribe failed: {e}"))
    }

    // -- Transport constructors -----------------------------------------------

    /// Connect using stdio transport (subprocess).
    async fn connect_stdio(
        command: &str,
        args: &[String],
        env_whitelist: &[String],
        client_handler: OpenFangMcpClient,
    ) -> Result<RunningService<RoleClient, OpenFangMcpClient>, String> {
        use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
        use tokio::process::Command;

        // Validate command path (no path traversal)
        if command.contains("..") {
            return Err("MCP command path contains '..': rejected".to_string());
        }

        let cmd_str = command.to_string();
        let args_vec: Vec<String> = args.to_vec();
        let env_list: Vec<String> = env_whitelist.to_vec();

        let transport = TokioChildProcess::new(Command::new(&cmd_str).configure(move |cmd| {
            for arg in &args_vec {
                cmd.arg(arg);
            }
            // Sandbox: clear environment, only pass whitelisted vars
            cmd.env_clear();
            for var_name in &env_list {
                if let Ok(val) = std::env::var(var_name) {
                    cmd.env(var_name, val);
                }
            }
            // Always pass PATH for binary resolution
            if let Ok(path) = std::env::var("PATH") {
                cmd.env("PATH", path);
            }
            // Some stdio MCP servers launched via node/npx require a usable home
            // directory even when they do not declare any explicit secret env vars.
            for var in &["HOME", "TMP", "TEMP"] {
                if let Ok(val) = std::env::var(var) {
                    cmd.env(var, val);
                }
            }
            // On Windows, npm/node need extra vars
            if cfg!(windows) {
                for var in &[
                    "APPDATA",
                    "LOCALAPPDATA",
                    "USERPROFILE",
                    "SystemRoot",
                    "HOMEDRIVE",
                    "HOMEPATH",
                ] {
                    if let Ok(val) = std::env::var(var) {
                        cmd.env(var, val);
                    }
                }
            }
        }))
        .map_err(|e| format!("Failed to spawn MCP server '{cmd_str}': {e}"))?;

        let client = client_handler
            .serve(transport)
            .await
            .map_err(|e| format!("MCP stdio handshake failed: {e}"))?;

        Ok(client)
    }

    /// Connect using Streamable HTTP transport (or SSE fallback via the same endpoint).
    ///
    /// The `rmcp` SDK's `StreamableHttpClientTransport` handles the full
    /// Streamable HTTP protocol: Accept headers, Mcp-Session-Id tracking,
    /// SSE stream parsing, and content-type negotiation.
    async fn connect_http(
        url: &str,
        headers: &[String],
        client_handler: OpenFangMcpClient,
    ) -> Result<RunningService<RoleClient, OpenFangMcpClient>, String> {
        use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
        use rmcp::transport::StreamableHttpClientTransport;

        Self::check_ssrf(url)?;

        // Parse custom headers (e.g., "Authorization: Bearer <token>").
        let mut custom_headers: HashMap<HeaderName, HeaderValue> = HashMap::new();
        for header_str in headers {
            if let Some((name, value)) = header_str.split_once(':') {
                let name = name.trim();
                let value = value.trim();
                if let (Ok(hn), Ok(hv)) = (
                    HeaderName::from_bytes(name.as_bytes()),
                    HeaderValue::from_str(value),
                ) {
                    custom_headers.insert(hn, hv);
                }
            }
        }

        // rmcp 1.3+ marks StreamableHttpClientTransportConfig as #[non_exhaustive].
        // Use the official builder API (credit: @jefflower, PR #986).
        let config =
            StreamableHttpClientTransportConfig::with_uri(url).custom_headers(custom_headers);

        let transport = StreamableHttpClientTransport::from_config(config);

        let client = client_handler
            .serve(transport)
            .await
            .map_err(|e| format!("MCP HTTP connection failed: {e}"))?;

        Ok(client)
    }

    /// Basic SSRF check: reject obviously private/metadata URLs.
    fn check_ssrf(url: &str) -> Result<(), String> {
        let lower = url.to_lowercase();
        if lower.contains("169.254.169.254") || lower.contains("metadata.google") {
            return Err("SSRF: MCP URL targets metadata endpoint".to_string());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tool namespacing helpers
// ---------------------------------------------------------------------------

/// Format a namespaced MCP tool name: `mcp_{server}_{tool}`.
pub fn format_mcp_tool_name(server: &str, tool: &str) -> String {
    format!("mcp_{}_{}", normalize_name(server), normalize_name(tool))
}

/// Check if a tool name is an MCP-namespaced tool.
pub fn is_mcp_tool(name: &str) -> bool {
    name.starts_with("mcp_")
}

/// Extract server name from an MCP tool name.
///
/// Falls back to first-underscore heuristic, but prefer
/// `extract_mcp_server_from_known()` which handles server names containing
/// hyphens (normalized to underscores) correctly.
pub fn extract_mcp_server(tool_name: &str) -> Option<&str> {
    if !tool_name.starts_with("mcp_") {
        return None;
    }
    let rest = &tool_name[4..];
    rest.find('_').map(|pos| &rest[..pos])
}

/// Extract the original server name by matching against known server names.
///
/// This handles server names with hyphens (e.g. "bocha-search") correctly —
/// the normalized prefix "mcp_bocha_search_" is matched against each known
/// server's normalized name, returning the original (unhyphenated) name.
pub fn extract_mcp_server_from_known<'a>(
    tool_name: &str,
    server_names: &[&'a str],
) -> Option<&'a str> {
    if !tool_name.starts_with("mcp_") {
        return None;
    }
    // Sort by length descending so longer (more specific) names match first
    let mut sorted: Vec<&&str> = server_names.iter().collect();
    sorted.sort_by_key(|a| std::cmp::Reverse(a.len()));
    for name in sorted {
        let prefix = format!("mcp_{}_", normalize_name(name));
        if tool_name.starts_with(&prefix) {
            return Some(name);
        }
    }
    None
}

/// Strip the MCP namespace prefix from a tool name.
fn strip_mcp_prefix<'a>(server: &str, tool_name: &'a str) -> Option<&'a str> {
    let prefix = format!("mcp_{}_", normalize_name(server));
    tool_name.strip_prefix(&prefix)
}

fn server_supports_resource_subscribe(capabilities: &ServerCapabilities) -> bool {
    capabilities
        .resources
        .as_ref()
        .and_then(|resources| resources.subscribe)
        .unwrap_or(false)
}

fn mcp_event_server(event: &McpEvent) -> &str {
    match event {
        McpEvent::ResourceUpdated { server, .. }
        | McpEvent::ResourceListChanged { server }
        | McpEvent::ToolListChanged { server }
        | McpEvent::PromptListChanged { server }
        | McpEvent::ConnectionReconnected { server }
        | McpEvent::NotificationDropped { server, .. } => server,
    }
}

fn mcp_event_kind(event: &McpEvent) -> &'static str {
    match event {
        McpEvent::ResourceUpdated { .. } => "resource_updated",
        McpEvent::ResourceListChanged { .. } => "resource_list_changed",
        McpEvent::ToolListChanged { .. } => "tool_list_changed",
        McpEvent::PromptListChanged { .. } => "prompt_list_changed",
        McpEvent::ConnectionReconnected { .. } => "connection_reconnected",
        McpEvent::NotificationDropped { .. } => "notification_dropped",
    }
}

/// Normalize a name for use in tool namespacing (lowercase, replace hyphens).
pub fn normalize_name(name: &str) -> String {
    name.to_lowercase().replace('-', "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_tool_namespacing() {
        assert_eq!(
            format_mcp_tool_name("github", "create_issue"),
            "mcp_github_create_issue"
        );
        assert_eq!(
            format_mcp_tool_name("my-server", "do_thing"),
            "mcp_my_server_do_thing"
        );
    }

    #[test]
    fn test_is_mcp_tool() {
        assert!(is_mcp_tool("mcp_github_create_issue"));
        assert!(!is_mcp_tool("file_read"));
        assert!(!is_mcp_tool(""));
    }

    #[test]
    fn test_hyphenated_tool_name_preserved() {
        let namespaced = format_mcp_tool_name("sqlcl", "list-connections");
        assert_eq!(namespaced, "mcp_sqlcl_list_connections");

        let mut original_names = HashMap::new();
        original_names.insert(namespaced.clone(), "list-connections".to_string());

        let raw = original_names
            .get(&namespaced)
            .map(|s| s.as_str())
            .unwrap_or("list_connections");
        assert_eq!(raw, "list-connections");
    }

    #[test]
    fn test_extract_mcp_server() {
        assert_eq!(
            extract_mcp_server("mcp_github_create_issue"),
            Some("github")
        );
        assert_eq!(extract_mcp_server("file_read"), None);
    }

    #[test]
    fn test_extract_mcp_server_from_known_with_hyphens() {
        let servers = vec!["bocha-search", "github"];
        let tool = "mcp_bocha_search_bocha_web_search";
        assert_eq!(
            extract_mcp_server_from_known(tool, &servers),
            Some("bocha-search")
        );
        assert_eq!(
            extract_mcp_server_from_known("mcp_github_create_issue", &servers),
            Some("github")
        );
        assert_eq!(extract_mcp_server_from_known("file_read", &servers), None);
    }

    #[test]
    fn test_extract_mcp_server_from_known_longest_match() {
        let servers = vec!["my-api", "my-api-v2"];
        assert_eq!(
            extract_mcp_server_from_known("mcp_my_api_v2_get_users", &servers),
            Some("my-api-v2")
        );
        assert_eq!(
            extract_mcp_server_from_known("mcp_my_api_list_items", &servers),
            Some("my-api")
        );
    }

    #[test]
    fn test_mcp_transport_config_serde() {
        let config = McpServerConfig {
            name: "github".to_string(),
            transport: McpTransport::Stdio {
                command: "npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-github".to_string(),
                ],
            },
            timeout_secs: 30,
            env: vec!["GITHUB_PERSONAL_ACCESS_TOKEN".to_string()],
            headers: vec![],
            allow_push_events: false,
            push_queue_size: default_push_queue_size(),
            push_rate_limit_per_minute: default_push_rate_limit_per_minute(),
        };

        let json = serde_json::to_string(&config).unwrap();
        let back: McpServerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "github");
        assert_eq!(back.timeout_secs, 30);
        assert_eq!(back.env, vec!["GITHUB_PERSONAL_ACCESS_TOKEN"]);

        match back.transport {
            McpTransport::Stdio { command, args } => {
                assert_eq!(command, "npx");
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected Stdio transport"),
        }

        // SSE variant
        let sse_config = McpServerConfig {
            name: "test".to_string(),
            transport: McpTransport::Sse {
                url: "https://example.com/mcp".to_string(),
            },
            timeout_secs: 60,
            env: vec![],
            headers: vec![],
            allow_push_events: false,
            push_queue_size: default_push_queue_size(),
            push_rate_limit_per_minute: default_push_rate_limit_per_minute(),
        };
        let json = serde_json::to_string(&sse_config).unwrap();
        let back: McpServerConfig = serde_json::from_str(&json).unwrap();
        match back.transport {
            McpTransport::Sse { url } => assert_eq!(url, "https://example.com/mcp"),
            _ => panic!("Expected SSE transport"),
        }

        // HTTP (Streamable HTTP) variant
        let http_config = McpServerConfig {
            name: "atlassian".to_string(),
            transport: McpTransport::Http {
                url: "https://mcp.atlassian.com/v1/mcp".to_string(),
            },
            timeout_secs: 120,
            env: vec![],
            headers: vec!["Authorization: Bearer test-token-456".to_string()],
            allow_push_events: true,
            push_queue_size: 512,
            push_rate_limit_per_minute: 120,
        };
        let json = serde_json::to_string(&http_config).unwrap();
        let back: McpServerConfig = serde_json::from_str(&json).unwrap();
        assert!(back.allow_push_events);
        assert_eq!(back.push_queue_size, 512);
        assert_eq!(back.push_rate_limit_per_minute, 120);
        match back.transport {
            McpTransport::Http { url } => {
                assert_eq!(url, "https://mcp.atlassian.com/v1/mcp")
            }
            _ => panic!("Expected Http transport"),
        }
    }

    #[test]
    fn test_server_supports_resource_subscribe() {
        let mut capabilities = ServerCapabilities::default();
        assert!(!server_supports_resource_subscribe(&capabilities));

        capabilities.resources = Some(rmcp::model::ResourcesCapability {
            subscribe: Some(true),
            list_changed: None,
        });
        assert!(server_supports_resource_subscribe(&capabilities));
    }

    #[test]
    fn test_mcp_notification_queue_drops_oldest() {
        let (tx, rx) = mcp_notification_channel(2);

        tx.push(McpEvent::ResourceListChanged {
            server: "test".to_string(),
        });
        tx.push(McpEvent::ToolListChanged {
            server: "test".to_string(),
        });
        tx.push(McpEvent::PromptListChanged {
            server: "test".to_string(),
        });

        match rx.try_recv() {
            Some(McpEvent::NotificationDropped {
                server,
                kind,
                dropped_count,
            }) => {
                assert_eq!(server, "test");
                assert_eq!(kind, "prompt_list_changed");
                assert_eq!(dropped_count, 2);
            }
            other => panic!("expected NotificationDropped, got {other:?}"),
        }
        match rx.try_recv() {
            Some(McpEvent::PromptListChanged { server }) => assert_eq!(server, "test"),
            other => panic!("expected PromptListChanged, got {other:?}"),
        }
        assert!(rx.try_recv().is_none());
    }

    #[test]
    fn test_openfang_mcp_client_gates_push_events() {
        let (tx, rx) = mcp_notification_channel(4);
        let client = OpenFangMcpClient::new("test".to_string(), false, 600, Some(tx.clone()));

        client.dispatch(McpEvent::ToolListChanged {
            server: "test".to_string(),
        });
        assert!(rx.try_recv().is_none());

        let client = OpenFangMcpClient::new("test".to_string(), true, 600, Some(tx));
        client.dispatch(McpEvent::ToolListChanged {
            server: "test".to_string(),
        });
        match rx.try_recv() {
            Some(McpEvent::ToolListChanged { server }) => assert_eq!(server, "test"),
            other => panic!("expected ToolListChanged, got {other:?}"),
        }
    }

    #[test]
    fn test_openfang_mcp_client_rate_limits_push_events() {
        let (tx, rx) = mcp_notification_channel(4);
        let client = OpenFangMcpClient::new("test".to_string(), true, 1, Some(tx));

        client.dispatch(McpEvent::ResourceListChanged {
            server: "test".to_string(),
        });
        client.dispatch(McpEvent::ToolListChanged {
            server: "test".to_string(),
        });

        match rx.try_recv() {
            Some(McpEvent::ResourceListChanged { server }) => assert_eq!(server, "test"),
            other => panic!("expected ResourceListChanged, got {other:?}"),
        }
        match rx.try_recv() {
            Some(McpEvent::NotificationDropped {
                server,
                kind,
                dropped_count,
            }) => {
                assert_eq!(server, "test");
                assert_eq!(kind, "tool_list_changed");
                assert_eq!(dropped_count, 1);
            }
            other => panic!("expected NotificationDropped, got {other:?}"),
        }
    }
}
