//! Python subprocess agent runtime.
//!
//! When an agent manifest specifies `module = "python:path/to/script.py"`,
//! the kernel delegates to this runtime instead of the LLM-based agent loop.
//!
//! Communication protocol (stdin/stdout JSON lines):
//!
//! **Input** (sent to Python script's stdin):
//! ```json
//! {"type": "message", "agent_id": "...", "message": "...", "context": {...}}
//! ```
//!
//! **Output** (read from Python script's stdout):
//! ```json
//! {"type": "response", "text": "...", "tool_calls": [...]}
//! ```
//!
//! The Python SDK (`omtae_sdk.py`) provides a helper to handle this protocol.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tracing::{debug, error, warn};

/// Error type for Python runtime operations.
#[derive(Debug, thiserror::Error)]
pub enum PythonError {
    #[error("Script not found: {0}")]
    ScriptNotFound(String),
    #[error("Python not found: {0}")]
    PythonNotFound(String),
    #[error("Spawn failed: {0}")]
    SpawnFailed(String),
    #[error("IO error: {0}")]
    Io(String),
    #[error("Timeout after {0}s")]
    Timeout(u64),
    #[error("Script error: {0}")]
    ScriptError(String),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

/// Result of running a Python agent script.
#[derive(Debug, Clone)]
pub struct PythonResult {
    /// The text response from the script.
    pub response: String,
    /// Exit code of the process.
    pub exit_code: Option<i32>,
}

/// Configuration for the Python runtime.
#[derive(Debug, Clone)]
pub struct PythonConfig {
    /// Path to the Python interpreter (default: "python3" or "python").
    pub interpreter: String,
    /// Maximum execution time in seconds.
    pub timeout_secs: u64,
    /// Working directory for the script.
    pub working_dir: Option<String>,
    /// Specific env vars to pass through (capability-gated, not secrets).
    pub allowed_env_vars: Vec<String>,
}

impl Default for PythonConfig {
    fn default() -> Self {
        Self {
            interpreter: find_python_interpreter(),
            timeout_secs: 120,
            working_dir: None,
            allowed_env_vars: Vec::new(),
        }
    }
}

/// Validate that a Python script path is safe to execute.
pub fn validate_script_path(path: &str) -> Result<(), PythonError> {
    let p = std::path::Path::new(path);
    for component in p.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(PythonError::ScriptNotFound(format!(
                "Path traversal denied: {path}"
            )));
        }
    }
    match p.extension().and_then(|e| e.to_str()) {
        Some("py") => Ok(()),
        _ => Err(PythonError::ScriptNotFound(format!(
            "Script must be a .py file: {path}"
        ))),
    }
}

/// Find the Python interpreter on this system.
fn find_python_interpreter() -> String {
    // Try python3 first, then python
    for cmd in &["python3", "python"] {
        if std::process::Command::new(cmd)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
        {
            return cmd.to_string();
        }
    }
    "python3".to_string() // default, will fail with helpful message
}

/// Extract the script path from a module string like "python:path/to/script.py".
pub fn parse_python_module(module: &str) -> Option<&str> {
    module.strip_prefix("python:")
}

/// Run a Python agent script with the given message.
///
/// Returns the script's text response.
pub async fn run_python_agent(
    script_path: &str,
    agent_id: &str,
    message: &str,
    context: &serde_json::Value,
    config: &PythonConfig,
) -> Result<PythonResult, PythonError> {
    // SECURITY: Validate script path (no traversal, must be .py)
    validate_script_path(script_path)?;

    // Validate script exists
    if !Path::new(script_path).exists() {
        return Err(PythonError::ScriptNotFound(script_path.to_string()));
    }

    debug!("Running Python agent: {script_path}");

    // Build the input JSON
    let input = serde_json::json!({
        "type": "message",
        "agent_id": agent_id,
        "message": message,
        "context": context,
    });
    let input_line = serde_json::to_string(&input).map_err(|e| PythonError::Io(e.to_string()))?;

    // Spawn the Python process
    let mut cmd = Command::new(&config.interpreter);
    cmd.arg(script_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(ref wd) = config.working_dir {
        cmd.current_dir(wd);
    }

    // SECURITY: Wipe inherited environment. Prevents credential leakage.
    cmd.env_clear();

    // Re-add ONLY safe, required vars
    cmd.env("OPENFANG_AGENT_ID", agent_id);
    cmd.env("OPENFANG_MESSAGE", message);

    // PATH — needed to find python stdlib / system tools
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    // HOME — needed for Python packages, pip cache
    if let Ok(home) = std::env::var("HOME") {
        cmd.env("HOME", home);
    }
    #[cfg(windows)]
    {
        for var in &[
            "USERPROFILE",
            "SYSTEMROOT",
            "APPDATA",
            "LOCALAPPDATA",
            "COMSPEC",
        ] {
            if let Ok(val) = std::env::var(var) {
                cmd.env(var, val);
            }
        }
    }
    // Python-specific
    if let Ok(pp) = std::env::var("PYTHONPATH") {
        cmd.env("PYTHONPATH", pp);
    }
    if let Ok(venv) = std::env::var("VIRTUAL_ENV") {
        cmd.env("VIRTUAL_ENV", venv);
    }
    // Agent-specific allowed vars (from manifest capabilities)
    for var in &config.allowed_env_vars {
        if let Ok(val) = std::env::var(var) {
            cmd.env(var, val);
        }
    }

    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            PythonError::PythonNotFound(format!(
                "Python interpreter '{}' not found. Install Python 3 or set the interpreter path.",
                config.interpreter
            ))
        } else {
            PythonError::SpawnFailed(e.to_string())
        }
    })?;

    // Write input to stdin
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input_line.as_bytes())
            .await
            .map_err(|e| PythonError::Io(e.to_string()))?;
        stdin
            .write_all(b"\n")
            .await
            .map_err(|e| PythonError::Io(e.to_string()))?;
        drop(stdin); // Close stdin to signal EOF
    }

    // Read output with timeout
    let timeout = Duration::from_secs(config.timeout_secs);
    let result = tokio::time::timeout(timeout, async {
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| PythonError::Io("Failed to capture stdout".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| PythonError::Io("Failed to capture stderr".to_string()))?;

        let mut stdout_reader = BufReader::new(stdout);
        let mut stderr_reader = BufReader::new(stderr);

        let mut stdout_lines = Vec::new();
        let mut stderr_text = String::new();

        // Read all stdout lines
        let mut line = String::new();
        loop {
            line.clear();
            match stdout_reader.read_line(&mut line).await {
                Ok(0) => break,
                Ok(_) => stdout_lines.push(line.trim_end().to_string()),
                Err(e) => {
                    warn!("Python stdout read error: {e}");
                    break;
                }
            }
        }

        // Read stderr
        let mut stderr_line = String::new();
        loop {
            stderr_line.clear();
            match stderr_reader.read_line(&mut stderr_line).await {
                Ok(0) => break,
                Ok(_) => {
                    stderr_text.push_str(&stderr_line);
                }
                Err(_) => break,
            }
        }

        let status = child
            .wait()
            .await
            .map_err(|e| PythonError::Io(e.to_string()))?;

        if !stderr_text.is_empty() {
            debug!("Python stderr: {stderr_text}");
        }

        Ok::<(Vec<String>, String, Option<i32>), PythonError>((
            stdout_lines,
            stderr_text,
            status.code(),
        ))
    })
    .await;

    match result {
        Ok(Ok((stdout_lines, stderr_text, exit_code))) => {
            if exit_code != Some(0) {
                return Err(PythonError::ScriptError(format!(
                    "Script exited with code {:?}. Stderr: {}",
                    exit_code,
                    stderr_text.trim()
                )));
            }

            // Try to parse the last JSON line as a response
            let response = parse_python_output(&stdout_lines)?;
            Ok(PythonResult {
                response,
                exit_code,
            })
        }
        Ok(Err(e)) => Err(e),
        Err(_) => {
            // Timeout — kill the process
            let _ = child.kill().await;
            error!("Python script timed out after {}s", config.timeout_secs);
            Err(PythonError::Timeout(config.timeout_secs))
        }
    }
}

/// Parse the output from a Python agent script.
///
/// Looks for a JSON response line in the output. If found, extracts the "text" field.
/// If no JSON response, returns all stdout as plain text.
fn parse_python_output(lines: &[String]) -> Result<String, PythonError> {
    // Look for JSON response (last line that parses as JSON with "type":"response")
    for line in lines.iter().rev() {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            if json["type"].as_str() == Some("response") {
                if let Some(text) = json["text"].as_str() {
                    return Ok(text.to_string());
                }
            }
        }
    }

    // Fallback: return all stdout as plain text
    let text = lines.join("\n");
    if text.is_empty() {
        return Err(PythonError::InvalidResponse(
            "Script produced no output".to_string(),
        ));
    }
    Ok(text)
}

/// Check if a module string refers to a Python script.
pub fn is_python_module(module: &str) -> bool {
    module.starts_with("python:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_python_module() {
        assert_eq!(
            parse_python_module("python:scripts/agent.py"),
            Some("scripts/agent.py")
        );
        assert_eq!(
            parse_python_module("python:./research.py"),
            Some("./research.py")
        );
        assert_eq!(parse_python_module("builtin:chat"), None);
        assert_eq!(parse_python_module("wasm:skill.wasm"), None);
    }

    #[test]
    fn test_is_python_module() {
        assert!(is_python_module("python:test.py"));
        assert!(!is_python_module("builtin:chat"));
        assert!(!is_python_module("wasm:skill.wasm"));
    }

    #[test]
    fn test_parse_python_output_json() {
        let lines = vec![
            "Loading model...".to_string(),
            r#"{"type": "response", "text": "Hello from Python!"}"#.to_string(),
        ];
        let result = parse_python_output(&lines).unwrap();
        assert_eq!(result, "Hello from Python!");
    }

    #[test]
    fn test_parse_python_output_plain() {
        let lines = vec!["Hello from Python!".to_string(), "Line two".to_string()];
        let result = parse_python_output(&lines).unwrap();
        assert_eq!(result, "Hello from Python!\nLine two");
    }

    #[test]
    fn test_parse_python_output_empty() {
        let lines: Vec<String> = vec![];
        let result = parse_python_output(&lines);
        assert!(result.is_err());
    }

    #[test]
    fn test_python_config_default() {
        let config = PythonConfig::default();
        assert!(config.interpreter == "python3" || config.interpreter == "python");
        assert_eq!(config.timeout_secs, 120);
        assert!(config.allowed_env_vars.is_empty());
    }

    #[test]
    fn test_validate_script_path() {
        assert!(validate_script_path("scripts/agent.py").is_ok());
        assert!(validate_script_path("../../etc/passwd").is_err());
        assert!(validate_script_path("agent.sh").is_err());
        assert!(validate_script_path("/bin/bash").is_err());
        assert!(validate_script_path("test.py").is_ok());
    }

    #[tokio::test]
    async fn test_run_python_missing_script() {
        let config = PythonConfig::default();
        let result = run_python_agent(
            "/nonexistent/script.py",
            "test-agent",
            "hello",
            &serde_json::json!({}),
            &config,
        )
        .await;
        assert!(matches!(result, Err(PythonError::ScriptNotFound(_))));
    }
}
