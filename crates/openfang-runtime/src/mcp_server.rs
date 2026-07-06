//! MCP Server — expose OMTAE tools via the Model Context Protocol.
//!
//! Implements the server-side MCP protocol so external MCP clients
//! (Claude Desktop, VS Code, etc.) can use OMTAE's built-in tools.
//!
//! This module provides a reusable handler function — the CLI team
//! wires it into a stdio transport.

use omtae_types::tool::ToolDefinition;
use serde_json::json;

/// MCP protocol version supported by this server.
const PROTOCOL_VERSION: &str = "2024-11-05";

/// Handle an incoming MCP JSON-RPC request and return a response.
///
/// This is a stateless handler that can be called from any transport
/// (stdio, HTTP, etc.). The caller provides the available tool definitions.
pub async fn handle_mcp_request(
    request: &serde_json::Value,
    tools: &[ToolDefinition],
) -> serde_json::Value {
    let method = request["method"].as_str().unwrap_or("");
    let id = request.get("id").cloned();

    match method {
        "initialize" => make_response(
            id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "omtae",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        ),
        "notifications/initialized" => {
            // Notification — no response needed
            json!(null)
        }
        "tools/list" => {
            let tool_list: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": t.input_schema,
                    })
                })
                .collect();

            make_response(id, json!({ "tools": tool_list }))
        }
        "tools/call" => {
            let tool_name = request["params"]["name"].as_str().unwrap_or("");
            let _arguments = request["params"]
                .get("arguments")
                .cloned()
                .unwrap_or(json!({}));

            // Verify the tool exists
            if !tools.iter().any(|t| t.name == tool_name) {
                return make_error(id, -32602, &format!("Unknown tool: {tool_name}"));
            }

            // Tool execution is delegated to the caller (kernel/CLI).
            // This handler just validates the request format.
            // In a full implementation, the caller would wire this to execute_tool().
            make_response(
                id,
                json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Tool '{tool_name}' is available. Execution must be wired by the host.")
                    }]
                }),
            )
        }
        _ => make_error(id, -32601, &format!("Method not found: {method}")),
    }
}

/// Build a JSON-RPC 2.0 success response.
fn make_response(id: Option<serde_json::Value>, result: serde_json::Value) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

/// Build a JSON-RPC 2.0 error response.
fn make_error(id: Option<serde_json::Value>, code: i64, message: &str) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_tools() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "file_read".to_string(),
                description: "Read a file".to_string(),
                input_schema: json!({"type": "object", "properties": {"path": {"type": "string"}}}),
            },
            ToolDefinition {
                name: "web_fetch".to_string(),
                description: "Fetch a URL".to_string(),
                input_schema: json!({"type": "object"}),
            },
        ]
    }

    #[tokio::test]
    async fn test_mcp_server_tools_list() {
        let tools = test_tools();
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list",
        });

        let response = handle_mcp_request(&request, &tools).await;
        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], 1);

        let tool_list = response["result"]["tools"].as_array().unwrap();
        assert_eq!(tool_list.len(), 2);
        assert_eq!(tool_list[0]["name"], "file_read");
        assert_eq!(tool_list[1]["name"], "web_fetch");
    }

    #[tokio::test]
    async fn test_mcp_server_unknown_method() {
        let tools = test_tools();
        let request = json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "nonexistent/method",
        });

        let response = handle_mcp_request(&request, &tools).await;
        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], 5);
        assert_eq!(response["error"]["code"], -32601);
        assert!(response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not found"));
    }

    #[tokio::test]
    async fn test_mcp_server_initialize() {
        let tools = test_tools();
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "test"}
            }
        });

        let response = handle_mcp_request(&request, &tools).await;
        assert_eq!(response["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert!(response["result"]["serverInfo"]["name"]
            .as_str()
            .unwrap()
            .contains("omtae"));
    }
}
