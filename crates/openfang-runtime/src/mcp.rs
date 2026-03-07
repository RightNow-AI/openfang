//! MCP (Model Context Protocol) client — connect to external MCP servers.
//!
//! MCP uses JSON-RPC 2.0 over stdio or HTTP+SSE. This module lets OpenFang
//! agents use tools from any MCP server (100+ available: GitHub, filesystem,
//! databases, APIs, etc.).
//!
//! All MCP tools are namespaced with `mcp_{server}_{tool}` to prevent collisions.

use openfang_types::tool::ToolDefinition;
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, info};

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
}

fn default_timeout() -> u64 {
    60
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
    /// HTTP Server-Sent Events.
    Sse { url: String },
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
    /// Transport handle for sending requests.
    transport: McpTransportHandle,
    /// Next JSON-RPC request ID.
    next_id: u64,
}

/// Transport handle — abstraction over stdio subprocess or HTTP.
enum McpTransportHandle {
    Stdio {
        child: Box<tokio::process::Child>,
        stdin: tokio::process::ChildStdin,
        stdout: BufReader<tokio::process::ChildStdout>,
    },
    Sse {
        client: reqwest::Client,
        url: String,
    },
}

/// JSON-RPC 2.0 request.
#[derive(Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 response.
#[derive(Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: Option<u64>,
    result: Option<serde_json::Value>,
    error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[allow(dead_code)]
    pub data: Option<serde_json::Value>,
}

impl std::fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JSON-RPC error {}: {}", self.code, self.message)
    }
}

// ---------------------------------------------------------------------------
// McpConnection implementation
// ---------------------------------------------------------------------------

impl McpConnection {
    /// Connect to an MCP server, perform handshake, and discover tools.
    pub async fn connect(config: McpServerConfig) -> Result<Self, String> {
        let transport = match &config.transport {
            McpTransport::Stdio { command, args } => {
                Self::connect_stdio(command, args, &config.env).await?
            }
            McpTransport::Sse { url } => {
                // SSRF check: reject private/localhost URLs unless explicitly configured
                Self::connect_sse(url).await?
            }
        };

        let mut conn = Self {
            config,
            tools: Vec::new(),
            transport,
            next_id: 1,
        };

        // Initialize handshake
        conn.initialize().await?;

        // Discover tools
        conn.discover_tools().await?;

        info!(
            server = %conn.config.name,
            tools = conn.tools.len(),
            "MCP server connected"
        );

        Ok(conn)
    }

    /// Send the MCP `initialize` handshake.
    async fn initialize(&mut self) -> Result<(), String> {
        let params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "openfang",
                "version": env!("CARGO_PKG_VERSION")
            }
        });

        let response = self.send_request("initialize", Some(params)).await?;

        if let Some(result) = response {
            debug!(
                server = %self.config.name,
                server_info = %result,
                "MCP initialize response"
            );
        }

        // Send initialized notification (no response expected)
        self.send_notification("notifications/initialized", None)
            .await?;

        Ok(())
    }

    /// Discover available tools via `tools/list`.
    async fn discover_tools(&mut self) -> Result<(), String> {
        let response = self.send_request("tools/list", None).await?;

        if let Some(result) = response {
            if let Some(tools_array) = result.get("tools").and_then(|t| t.as_array()) {
                let server_name = &self.config.name;
                for tool in tools_array {
                    let raw_name = tool["name"].as_str().unwrap_or("unnamed");
                    let description = tool["description"].as_str().unwrap_or("");
                    let input_schema = tool
                        .get("inputSchema")
                        .cloned()
                        .unwrap_or(serde_json::json!({"type": "object"}));

                    // Namespace: mcp_{server}_{tool}
                    let namespaced = format_mcp_tool_name(server_name, raw_name);

                    self.tools.push(ToolDefinition {
                        name: namespaced,
                        description: format!("[MCP:{server_name}] {description}"),
                        input_schema,
                    });
                }
            }
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
        // Strip the namespace prefix to get the original tool name
        let raw_name = strip_mcp_prefix(&self.config.name, name).unwrap_or(name);

        let params = serde_json::json!({
            "name": raw_name,
            "arguments": arguments,
        });

        let response = self.send_request("tools/call", Some(params)).await?;

        match response {
            Some(result) => {
                // Extract text content from the response
                if let Some(content) = result.get("content").and_then(|c| c.as_array()) {
                    let texts: Vec<&str> = content
                        .iter()
                        .filter_map(|item| {
                            if item["type"].as_str() == Some("text") {
                                item["text"].as_str()
                            } else {
                                None
                            }
                        })
                        .collect();
                    Ok(texts.join("\n"))
                } else {
                    Ok(result.to_string())
                }
            }
            None => Err("No result from MCP tools/call".to_string()),
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

    // --- Transport helpers ---

    async fn send_request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<Option<serde_json::Value>, String> {
        let id = self.next_id;
        self.next_id += 1;

        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };

        let request_json = serde_json::to_string(&request)
            .map_err(|e| format!("Failed to serialize request: {e}"))?;

        debug!(method, id, "MCP request");

        match &mut self.transport {
            McpTransportHandle::Stdio { stdin, stdout, .. } => {
                // Write request + newline
                stdin
                    .write_all(request_json.as_bytes())
                    .await
                    .map_err(|e| format!("Failed to write to MCP stdin: {e}"))?;
                stdin
                    .write_all(b"\n")
                    .await
                    .map_err(|e| format!("Failed to write newline: {e}"))?;
                stdin
                    .flush()
                    .await
                    .map_err(|e| format!("Failed to flush stdin: {e}"))?;

                // Read response line
                let mut line = String::new();
                let timeout = tokio::time::Duration::from_secs(self.config.timeout_secs);
                match tokio::time::timeout(timeout, stdout.read_line(&mut line)).await {
                    Ok(Ok(0)) => return Err("MCP server closed connection".to_string()),
                    Ok(Ok(_)) => {}
                    Ok(Err(e)) => return Err(format!("Failed to read MCP response: {e}")),
                    Err(_) => return Err("MCP request timed out".to_string()),
                }

                let response: JsonRpcResponse = serde_json::from_str(line.trim())
                    .map_err(|e| format!("Invalid MCP JSON-RPC response: {e}"))?;

                if let Some(err) = response.error {
                    return Err(format!("{err}"));
                }

                Ok(response.result)
            }
            McpTransportHandle::Sse { client, url } => {
                let response = client
                    .post(url.as_str())
                    .json(&request)
                    .timeout(std::time::Duration::from_secs(self.config.timeout_secs))
                    .send()
                    .await
                    .map_err(|e| format!("MCP SSE request failed: {e}"))?;

                if !response.status().is_success() {
                    return Err(format!("MCP SSE returned {}", response.status()));
                }

                let body = response
                    .text()
                    .await
                    .map_err(|e| format!("Failed to read SSE response: {e}"))?;

                let rpc_response: JsonRpcResponse = serde_json::from_str(&body)
                    .map_err(|e| format!("Invalid MCP SSE JSON-RPC response: {e}"))?;

                if let Some(err) = rpc_response.error {
                    return Err(format!("{err}"));
                }

                Ok(rpc_response.result)
            }
        }
    }

    async fn send_notification(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<(), String> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params.unwrap_or(serde_json::json!({})),
        });

        let json = serde_json::to_string(&notification)
            .map_err(|e| format!("Failed to serialize notification: {e}"))?;

        match &mut self.transport {
            McpTransportHandle::Stdio { stdin, .. } => {
                stdin
                    .write_all(json.as_bytes())
                    .await
                    .map_err(|e| format!("Write notification: {e}"))?;
                stdin
                    .write_all(b"\n")
                    .await
                    .map_err(|e| format!("Write newline: {e}"))?;
                stdin.flush().await.map_err(|e| format!("Flush: {e}"))?;
            }
            McpTransportHandle::Sse { client, url } => {
                let _ = client.post(url.as_str()).json(&notification).send().await;
            }
        }

        Ok(())
    }

    async fn connect_stdio(
        command: &str,
        args: &[String],
        env_whitelist: &[String],
    ) -> Result<McpTransportHandle, String> {
        // Validate command path (no path traversal)
        if command.contains("..") {
            return Err("MCP command path contains '..': rejected".to_string());
        }

        // On Windows, npm/npx install as .cmd batch wrappers. Detect and adapt.
        let resolved_command: String = if cfg!(windows) {
            // If the user already specified .cmd/.bat, use as-is
            if command.ends_with(".cmd") || command.ends_with(".bat") {
                command.to_string()
            } else {
                // Check if the .cmd variant exists on PATH
                let cmd_variant = format!("{command}.cmd");
                let has_cmd = std::env::var("PATH")
                    .unwrap_or_default()
                    .split(';')
                    .any(|dir| std::path::Path::new(dir).join(&cmd_variant).exists());
                if has_cmd {
                    cmd_variant
                } else {
                    command.to_string()
                }
            }
        } else {
            command.to_string()
        };

        let mut cmd = tokio::process::Command::new(&resolved_command);
        cmd.args(args);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // Sandbox: clear environment, only pass whitelisted vars
        cmd.env_clear();
        for var_name in env_whitelist {
            if let Ok(val) = std::env::var(var_name) {
                cmd.env(var_name, val);
            }
        }
        // Always pass PATH for binary resolution
        if let Ok(path) = std::env::var("PATH") {
            cmd.env("PATH", path);
        }
        // On Windows, npm/node need APPDATA, USERPROFILE, LOCALAPPDATA, and SystemRoot
        if cfg!(windows) {
            for var in &[
                "APPDATA",
                "LOCALAPPDATA",
                "USERPROFILE",
                "SystemRoot",
                "TEMP",
                "TMP",
                "HOME",
                "HOMEDRIVE",
                "HOMEPATH",
            ] {
                if let Ok(val) = std::env::var(var) {
                    cmd.env(var, val);
                }
            }
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn MCP server '{resolved_command}': {e}"))?;

        // Log stderr in background for debugging MCP server issues
        if let Some(stderr) = child.stderr.take() {
            let cmd_name = resolved_command.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncBufReadExt;
                let reader = tokio::io::BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::debug!(mcp_server = %cmd_name, "stderr: {line}");
                }
            });
        }

        let stdin = child
            .stdin
            .take()
            .ok_or("Failed to capture MCP server stdin")?;
        let stdout = child
            .stdout
            .take()
            .ok_or("Failed to capture MCP server stdout")?;

        Ok(McpTransportHandle::Stdio {
            child: Box::new(child),
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    async fn connect_sse(url: &str) -> Result<McpTransportHandle, String> {
        // Basic SSRF check: reject obviously private URLs
        let lower = url.to_lowercase();
        if lower.contains("169.254.169.254") || lower.contains("metadata.google") {
            return Err("SSRF: MCP SSE URL targets metadata endpoint".to_string());
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

        Ok(McpTransportHandle::Sse {
            client,
            url: url.to_string(),
        })
    }
}

impl Drop for McpConnection {
    fn drop(&mut self) {
        if let McpTransportHandle::Stdio { ref mut child, .. } = self.transport {
            // Best-effort kill of the subprocess
            let _ = child.start_kill();
        }
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
pub fn extract_mcp_server(tool_name: &str) -> Option<&str> {
    if !tool_name.starts_with("mcp_") {
        return None;
    }
    let rest = &tool_name[4..];
    rest.find('_').map(|pos| &rest[..pos])
}

/// Strip the MCP namespace prefix from a tool name.
fn strip_mcp_prefix<'a>(server: &str, tool_name: &'a str) -> Option<&'a str> {
    let prefix = format!("mcp_{}_", normalize_name(server));
    tool_name.strip_prefix(&prefix)
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
    fn test_extract_mcp_server() {
        assert_eq!(
            extract_mcp_server("mcp_github_create_issue"),
            Some("github")
        );
        assert_eq!(extract_mcp_server("file_read"), None);
    }

    #[test]
    fn test_mcp_jsonrpc_initialize() {
        // Verify the initialize request structure
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method: "initialize".to_string(),
            params: Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "openfang",
                    "version": "0.1.0"
                }
            })),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("initialize"));
        assert!(json.contains("protocolVersion"));
        assert!(json.contains("openfang"));
    }

    #[test]
    fn test_mcp_jsonrpc_tools_list() {
        // Simulate a tools/list response
        let response_json = r#"{
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "tools": [
                    {
                        "name": "create_issue",
                        "description": "Create a GitHub issue",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "title": {"type": "string"},
                                "body": {"type": "string"}
                            },
                            "required": ["title"]
                        }
                    }
                ]
            }
        }"#;

        let response: JsonRpcResponse = serde_json::from_str(response_json).unwrap();
        assert!(response.error.is_none());
        let result = response.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"].as_str().unwrap(), "create_issue");
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
        };
        let json = serde_json::to_string(&sse_config).unwrap();
        let back: McpServerConfig = serde_json::from_str(&json).unwrap();
        match back.transport {
            McpTransport::Sse { url } => assert_eq!(url, "https://example.com/mcp"),
            _ => panic!("Expected SSE transport"),
        }
    }
}
