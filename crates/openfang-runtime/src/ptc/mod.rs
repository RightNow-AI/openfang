//! Programmatic Tool Calling (PTC) for OpenFang.
//!
//! Instead of sending LLMs 50+ tool JSON schemas (consuming thousands of context
//! tokens), PTC replaces them with a single `execute_code` tool. The LLM writes
//! Python code that calls tools as plain functions, and only `print()` output
//! enters the context window.
//!
//! This approach:
//! - Reduces context usage by 30-40%+ (tool schemas removed from prompt)
//! - Eliminates multi-turn tool roundtrips (batch operations in a single code block)
//! - Keeps intermediate tool results out of context (processed in code)
//! - Works with any LLM model, not just Anthropic
//!
//! Architecture:
//! ```text
//! LLM → execute_code(code="...") → agent loop
//!   → python3 subprocess with auto-generated SDK preamble
//!   → Python calls tool functions via HTTP to localhost IPC server
//!   → IPC server sends request over channel to agent loop
//!   → agent loop calls execute_tool() with full context
//!   → result sent back through channel → IPC server → Python
//!   → only print() output returned to LLM context
//! ```

pub mod executor;
pub mod ipc_server;
pub mod sdk_generator;
pub mod tool_classifier;

use openfang_types::tool::ToolDefinition;
use std::path::Path;

pub use executor::{execute_python, is_python3_available};
pub use ipc_server::{PtcIpcServer, PtcToolRequest};
pub use sdk_generator::{generate_compact_reference, generate_python_sdk, wrap_user_code};
pub use tool_classifier::{classify_tools, PtcMode};

/// Configuration for Programmatic Tool Calling.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PtcConfig {
    /// Whether PTC is enabled (global default: false).
    /// PTC shells out to a Python subprocess, so it is opt-in.
    pub enabled: bool,
    /// Timeout for Python subprocess execution in seconds.
    pub timeout_secs: u64,
    /// Maximum stdout size in bytes before truncation.
    pub max_stdout_bytes: usize,
}

impl Default for PtcConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: 120,
            max_stdout_bytes: 100_000,
        }
    }
}

/// A running PTC instance for a single agent loop execution.
///
/// Created at agent loop start, shut down at loop end.
pub struct PtcInstance {
    /// The IPC server handling tool calls from Python.
    pub ipc_server: PtcIpcServer,
    /// Tools passed directly to the LLM (with full JSON schemas).
    pub direct_tools: Vec<ToolDefinition>,
    /// Tools callable only via execute_code (schemas removed from prompt).
    pub ptc_tools: Vec<ToolDefinition>,
    /// The execute_code tool definition (includes compact function reference).
    pub execute_code_tool: ToolDefinition,
}

impl PtcInstance {
    /// Get the tool list to pass to the LLM: direct tools + execute_code.
    pub fn agent_tools(&self) -> Vec<ToolDefinition> {
        let mut tools = self.direct_tools.clone();
        tools.push(self.execute_code_tool.clone());
        tools
    }
}

/// Initialize PTC for an agent loop execution.
///
/// Starts the IPC server, classifies tools, generates the execute_code tool,
/// and returns the PTC instance. The caller must poll `ptc_instance.ipc_server.request_rx`
/// to dispatch tool calls from the IPC server.
pub async fn init_ptc(all_tools: &[ToolDefinition]) -> Result<PtcInstance, String> {
    // Check python3 availability first (cached after first call)
    if !is_python3_available() {
        return Err("python3 not available".to_string());
    }

    // Classify tools into direct vs PTC
    let (direct_tools, ptc_tools) = classify_tools(all_tools);

    // Start the IPC server
    let ipc_server = ipc_server::start_ipc_server(&ptc_tools)
        .await
        .map_err(|e| format!("Failed to start PTC IPC server: {e}"))?;

    // Generate the execute_code tool with compact function reference
    let compact_ref = generate_compact_reference(&ptc_tools);
    let execute_code_tool = build_execute_code_definition(&compact_ref);

    tracing::info!(
        direct = direct_tools.len(),
        ptc = ptc_tools.len(),
        port = ipc_server.port(),
        "PTC initialized"
    );

    Ok(PtcInstance {
        ipc_server,
        direct_tools,
        ptc_tools,
        execute_code_tool,
    })
}

/// Build the `execute_code` tool definition.
fn build_execute_code_definition(compact_ref: &str) -> ToolDefinition {
    let description = format!(
        "Execute Python code with access to tool functions. \
         Tools are plain synchronous functions — call them directly, NO async/await. \
         ONLY print() output enters your context window — \
         tool results are processed in code, not loaded into context. \
         Use this for multi-step workflows, data filtering, batch operations, \
         and any task where intermediate results should be processed before you see them. \
         Always wrap code in try/except. Some params are renamed: type -> type_, class -> class_.\n\n\
         Available functions:\n{compact_ref}"
    );

    ToolDefinition {
        name: "execute_code".to_string(),
        description,
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "code": {
                    "type": "string",
                    "description": "Python code to execute. Tool functions are available as synchronous calls. Use print() to output results — ONLY print() output enters your context."
                },
                "timeout": {
                    "type": "integer",
                    "description": "Execution timeout in seconds (default 120, max 600)."
                }
            },
            "required": ["code"]
        }),
    }
}

/// Execute the `execute_code` tool: generate SDK, run Python, return output.
///
/// This is a standalone helper for executing PTC code without the select loop.
/// The agent loop uses an inline version that concurrently polls the IPC channel.
/// This function is useful for testing or MCP-server tool execution.
#[allow(dead_code)]
pub async fn run_execute_code(
    code: &str,
    timeout_secs: u64,
    ptc_tools: &[ToolDefinition],
    ipc_port: u16,
    config: &PtcConfig,
    workspace_root: Option<&Path>,
    allowed_env: &[String],
) -> String {
    // Generate the full SDK preamble + wrap user code
    let sdk = generate_python_sdk(ptc_tools, ipc_port);
    let full_script = wrap_user_code(&sdk, code);

    tracing::debug!(
        code_len = code.len(),
        timeout_secs,
        ptc_tools = ptc_tools.len(),
        "Executing PTC code"
    );

    let result = execute_python(&full_script, timeout_secs, workspace_root, allowed_env).await;

    // Combine stdout and stderr for the response
    let mut parts: Vec<String> = Vec::new();
    if !result.stdout.trim().is_empty() {
        let stdout = if result.stdout.len() > config.max_stdout_bytes {
            let truncated = &result.stdout[..config.max_stdout_bytes];
            format!(
                "{}\n\n[output truncated at {} bytes]",
                truncated, config.max_stdout_bytes
            )
        } else {
            result.stdout.trim().to_string()
        };
        parts.push(stdout);
    }
    if result.exit_code != 0 {
        if !result.stderr.trim().is_empty() {
            parts.push(format!("\n[stderr]\n{}", result.stderr.trim()));
        }
        parts.push(format!("\n[exit code: {}]", result.exit_code));
    }

    if parts.is_empty() {
        "(no output)".to_string()
    } else {
        parts.join("\n")
    }
}
