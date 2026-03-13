//! MCP (Model Context Protocol) server for OpenFang.
//!
//! Exposes running agents as MCP tools over JSON-RPC 2.0 stdio.
//! Each agent becomes a callable tool named `openfang_agent_{name}`.
//!
//! Protocol: Content-Length framing over stdin/stdout.
//! Connects to running daemon via HTTP, falls back to in-process kernel.

use openfang_kernel::OpenFangKernel;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

/// Backend for MCP: either a running daemon or an in-process kernel.
enum McpBackend {
    Daemon {
        base_url: String,
        client: reqwest::blocking::Client,
    },
    InProcess {
        kernel: Box<OpenFangKernel>,
        rt: tokio::runtime::Runtime,
    },
}

impl McpBackend {
    fn list_agents(&self) -> Vec<(String, String, String)> {
        // Returns (id, name, description) triples
        match self {
            McpBackend::Daemon { base_url, client } => {
                let resp = client
                    .get(format!("{base_url}/api/agents"))
                    .send()
                    .ok()
                    .and_then(|r| r.json::<Value>().ok());
                match resp.and_then(|v| v.as_array().cloned()) {
                    Some(agents) => agents
                        .iter()
                        .map(|a| {
                            (
                                a["id"].as_str().unwrap_or("").to_string(),
                                a["name"].as_str().unwrap_or("").to_string(),
                                a["description"].as_str().unwrap_or("").to_string(),
                            )
                        })
                        .collect(),
                    None => Vec::new(),
                }
            }
            McpBackend::InProcess { kernel, .. } => kernel
                .registry
                .list()
                .iter()
                .map(|e| {
                    (
                        e.id.to_string(),
                        e.name.clone(),
                        e.manifest.description.clone(),
                    )
                })
                .collect(),
        }
    }

    fn send_message(&self, agent_id: &str, message: &str) -> Result<String, String> {
        match self {
            McpBackend::Daemon { base_url, client } => {
                let resp = client
                    .post(format!("{base_url}/api/agents/{agent_id}/message"))
                    .json(&json!({"message": message}))
                    .send()
                    .map_err(|e| format!("HTTP error: {e}"))?;
                let body: Value = resp.json().map_err(|e| format!("Parse error: {e}"))?;
                if let Some(response) = body["response"].as_str() {
                    Ok(response.to_string())
                } else {
                    Err(body["error"]
                        .as_str()
                        .unwrap_or("Unknown error")
                        .to_string())
                }
            }
            McpBackend::InProcess { kernel, rt } => {
                let aid: openfang_types::agent::AgentId =
                    agent_id.parse().map_err(|_| "Invalid agent ID")?;
                let result = rt
                    .block_on(kernel.send_message(aid, message))
                    .map_err(|e| format!("{e}"))?;
                Ok(result.response)
            }
        }
    }

    /// Find agent ID by tool name (strip `openfang_agent_` prefix, match by name).
    fn resolve_tool_agent(&self, tool_name: &str) -> Option<String> {
        let agent_name = tool_name.strip_prefix("openfang_agent_")?.replace('_', "-");
        let agents = self.list_agents();
        // Try exact match first (with underscores replaced by hyphens)
        for (id, name, _) in &agents {
            if name.replace(' ', "-").to_lowercase() == agent_name.to_lowercase() {
                return Some(id.clone());
            }
        }
        // Try with underscores
        let agent_name_underscore = tool_name.strip_prefix("openfang_agent_")?;
        for (id, name, _) in &agents {
            if name.replace('-', "_").to_lowercase() == agent_name_underscore.to_lowercase() {
                return Some(id.clone());
            }
        }
        None
    }
}

/// Run the MCP server over stdio.
pub fn run_mcp_server(config: Option<std::path::PathBuf>) {
    let backend = create_backend(config);

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    loop {
        match read_message(&mut reader) {
            Ok(Some(msg)) => {
                let response = handle_message(&backend, &msg);
                if let Some(resp) = response {
                    write_message(&mut writer, &resp);
                }
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }
}

fn create_backend(config: Option<std::path::PathBuf>) -> McpBackend {
    // Try daemon first
    if let Some(base_url) = super::find_daemon() {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("Failed to build HTTP client");
        return McpBackend::Daemon { base_url, client };
    }

    // Fall back to in-process kernel
    let kernel = match OpenFangKernel::boot(config.as_deref()) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("Failed to boot kernel for MCP: {e}");
            std::process::exit(1);
        }
    };
    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
    McpBackend::InProcess {
        kernel: Box::new(kernel),
        rt,
    }
}

/// Read a Content-Length framed JSON-RPC message from the reader.
fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Value>> {
    // Read headers until empty line
    let mut content_length: usize = 0;
    loop {
        let mut header = String::new();
        let bytes_read = reader.read_line(&mut header)?;
        if bytes_read == 0 {
            return Ok(None); // EOF
        }

        let trimmed = header.trim();
        if trimmed.is_empty() {
            break; // End of headers
        }

        if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
            content_length = len_str.parse().unwrap_or(0);
        }
    }

    if content_length == 0 {
        return Ok(None);
    }

    // SECURITY: Reject oversized messages to prevent OOM.
    const MAX_MCP_MESSAGE_SIZE: usize = 10 * 1024 * 1024; // 10MB
    if content_length > MAX_MCP_MESSAGE_SIZE {
        // Drain the oversized body to avoid stream desync
        let mut discard = [0u8; 4096];
        let mut remaining = content_length;
        while remaining > 0 {
            let to_read = remaining.min(4096);
            if reader.read_exact(&mut discard[..to_read]).is_err() {
                break;
            }
            remaining -= to_read;
        }
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("MCP message too large: {content_length} bytes (max {MAX_MCP_MESSAGE_SIZE})"),
        ));
    }

    // Read the body
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;

    match serde_json::from_slice(&body) {
        Ok(v) => Ok(Some(v)),
        Err(_) => Ok(None),
    }
}

/// Write a Content-Length framed JSON-RPC response to the writer.
fn write_message(writer: &mut impl Write, msg: &Value) {
    let body = serde_json::to_string(msg).unwrap_or_default();
    if let Err(e) = write!(writer, "Content-Length: {}\r\n\r\n{}", body.len(), body) {
        eprintln!("MCP write error: {e}");
        return;
    }
    if let Err(e) = writer.flush() {
        eprintln!("MCP flush error: {e}");
    }
}

/// Handle a JSON-RPC message and return an optional response.
fn handle_message(backend: &McpBackend, msg: &Value) -> Option<Value> {
    let method = msg["method"].as_str().unwrap_or("");
    let id = msg.get("id").cloned();

    // Per JSON-RPC 2.0 spec: requests MUST have an id field.
    // Use null if missing so we always send a response.
    let rid = id.unwrap_or(Value::Null);

    match method {
        "initialize" => {
            let result = json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "openfang",
                    "version": env!("CARGO_PKG_VERSION")
                }
            });
            Some(jsonrpc_response(rid, result))
        }

        "notifications/initialized" => None, // Notification, no response

        "tools/list" => {
            let agents = backend.list_agents();
            let tools: Vec<Value> = agents
                .iter()
                .map(|(_, name, description)| {
                    let tool_name = format!("openfang_agent_{}", name.replace('-', "_"));
                    let desc = if description.is_empty() {
                        format!("Send a message to OpenFang agent '{name}'")
                    } else {
                        description.clone()
                    };
                    json!({
                        "name": tool_name,
                        "description": desc,
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "message": {
                                    "type": "string",
                                    "description": "Message to send to the agent"
                                }
                            },
                            "required": ["message"]
                        }
                    })
                })
                .collect();
            Some(jsonrpc_response(rid, json!({ "tools": tools })))
        }

        "tools/call" => {
            let params = &msg["params"];
            let tool_name = params["name"].as_str().unwrap_or("");
            let message = params["arguments"]["message"]
                .as_str()
                .unwrap_or("")
                .to_string();

            if message.is_empty() {
                return Some(jsonrpc_error(rid, -32602, "Missing 'message' argument"));
            }

            let agent_id = match backend.resolve_tool_agent(tool_name) {
                Some(id) => id,
                None => {
                    return Some(jsonrpc_error(
                        rid,
                        -32602,
                        &format!("Unknown tool: {tool_name}"),
                    ));
                }
            };

            match backend.send_message(&agent_id, &message) {
                Ok(response) => Some(jsonrpc_response(
                    rid,
                    json!({
                        "content": [{
                            "type": "text",
                            "text": response
                        }]
                    }),
                )),
                Err(e) => Some(jsonrpc_response(
                    rid,
                    json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Error: {e}")
                        }],
                        "isError": true
                    }),
                )),
            }
        }

        _ => {
            // Unknown method — always respond with error
            Some(jsonrpc_error(rid, -32601, &format!("Method not found: {method}")))
        }
    }
}

fn jsonrpc_response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

fn jsonrpc_error(id: Value, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_initialize() {
        let msg = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });
        // We can't easily create a backend in tests without a kernel,
        // but we can test the protocol handling
        let backend = McpBackend::Daemon {
            base_url: "http://localhost:9999".to_string(),
            client: reqwest::blocking::Client::new(),
        };
        let resp = handle_message(&backend, &msg).unwrap();
        assert_eq!(resp["id"], 1);
        assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(resp["result"]["serverInfo"]["name"], "openfang");
    }

    #[test]
    fn test_handle_notifications_initialized() {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        let backend = McpBackend::Daemon {
            base_url: "http://localhost:9999".to_string(),
            client: reqwest::blocking::Client::new(),
        };
        let resp = handle_message(&backend, &msg);
        assert!(resp.is_none()); // No response for notifications
    }

    #[test]
    fn test_handle_unknown_method() {
        let msg = json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "unknown/method"
        });
        let backend = McpBackend::Daemon {
            base_url: "http://localhost:9999".to_string(),
            client: reqwest::blocking::Client::new(),
        };
        let resp = handle_message(&backend, &msg).unwrap();
        assert_eq!(resp["error"]["code"], -32601);
    }

    #[test]
    fn test_jsonrpc_response() {
        let resp = jsonrpc_response(json!(1), json!({"status": "ok"}));
        assert_eq!(resp["jsonrpc"], "2.0");
        assert_eq!(resp["id"], 1);
        assert_eq!(resp["result"]["status"], "ok");
    }

    #[test]
    fn test_jsonrpc_error() {
        let resp = jsonrpc_error(json!(2), -32601, "Not found");
        assert_eq!(resp["jsonrpc"], "2.0");
        assert_eq!(resp["id"], 2);
        assert_eq!(resp["error"]["code"], -32601);
        assert_eq!(resp["error"]["message"], "Not found");
    }

    #[test]
    fn test_read_message() {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#;
        let input = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let mut reader = io::BufReader::new(input.as_bytes());
        let msg = read_message(&mut reader).unwrap().unwrap();
        assert_eq!(msg["method"], "initialize");
        assert_eq!(msg["id"], 1);
    }
}
