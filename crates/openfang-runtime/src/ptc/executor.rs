//! Python subprocess executor for Programmatic Tool Calling.
//!
//! Spawns `python3 -u -c <script>` as a child process, captures stdout/stderr,
//! and enforces a timeout by killing the process.

use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};
use tracing::{debug, warn};

/// Cached python3 availability: 0 = unknown, 1 = available, 2 = not available.
static PYTHON3_AVAILABLE: AtomicU8 = AtomicU8::new(0);

/// Check if python3 is available on the system. Result is cached after first call.
pub fn is_python3_available() -> bool {
    let cached = PYTHON3_AVAILABLE.load(Ordering::Relaxed);
    if cached != 0 {
        return cached == 1;
    }

    let available = std::process::Command::new("python3")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    PYTHON3_AVAILABLE.store(if available { 1 } else { 2 }, Ordering::Relaxed);

    if !available {
        warn!(
            "python3 not found — Programmatic Tool Calling (PTC) will be disabled. \
               Install Python 3 to enable PTC."
        );
    }

    available
}

/// Result of a Python execution.
#[derive(Debug)]
pub struct PythonResult {
    /// Captured stdout.
    pub stdout: String,
    /// Captured stderr.
    pub stderr: String,
    /// Process exit code (0 = success).
    pub exit_code: i32,
}

/// Execute a Python script in a subprocess.
///
/// The script is passed via `-c` flag. The process runs with:
/// - Environment sandboxed (secrets stripped, only safe vars inherited)
/// - `PYTHONUNBUFFERED=1` to prevent output buffering
/// - `cwd` set to the workspace root (if provided)
///
/// If the process exceeds `timeout_secs`, it is killed.
pub async fn execute_python(
    script: &str,
    timeout_secs: u64,
    workspace_root: Option<&Path>,
    allowed_env: &[String],
) -> PythonResult {
    use tokio::io::AsyncReadExt;
    use tokio::process::Command;

    let mut cmd = Command::new("python3");
    cmd.arg("-u").arg("-c").arg(script);

    // Set working directory
    if let Some(root) = workspace_root {
        cmd.current_dir(root);
    }

    // SECURITY: Strip environment to prevent credential leakage into
    // LLM-generated Python code. Only safe vars (PATH, HOME, etc.) and
    // explicitly allowed vars from hand_allowed_env are inherited.
    crate::subprocess_sandbox::sandbox_command(&mut cmd, allowed_env);

    // Unbuffered Python output (set after sandbox_command which calls env_clear)
    cmd.env("PYTHONUNBUFFERED", "1");

    // Spawn with piped stdio
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    cmd.stdin(std::process::Stdio::null());

    // Kill on drop ensures cleanup if the future is cancelled
    cmd.kill_on_drop(true);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to spawn python3: {e}");
            return PythonResult {
                stdout: String::new(),
                stderr: format!("Failed to spawn python3: {e}. Is Python 3 installed?"),
                exit_code: 1,
            };
        }
    };

    let pid = child.id();
    debug!(pid, timeout_secs, "Python subprocess started");

    // Read stdout/stderr concurrently with timeout
    let mut stdout_pipe = child.stdout.take().unwrap();
    let mut stderr_pipe = child.stderr.take().unwrap();

    let result = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), async {
        let (stdout_result, stderr_result) = tokio::join!(
            async {
                let mut buf = Vec::new();
                stdout_pipe.read_to_end(&mut buf).await.ok();
                String::from_utf8_lossy(&buf).into_owned()
            },
            async {
                let mut buf = Vec::new();
                stderr_pipe.read_to_end(&mut buf).await.ok();
                String::from_utf8_lossy(&buf).into_owned()
            }
        );

        let status = child.wait().await;
        let exit_code = status.map(|s| s.code().unwrap_or(1)).unwrap_or(1);

        PythonResult {
            stdout: stdout_result,
            stderr: stderr_result,
            exit_code,
        }
    })
    .await;

    match result {
        Ok(r) => r,
        Err(_) => {
            warn!(pid, timeout_secs, "Python subprocess timed out, killing");
            // kill_on_drop will handle cleanup, but try explicit kill too
            let _ = child.kill().await;

            PythonResult {
                stdout: String::new(),
                stderr: format!("Execution timed out after {timeout_secs}s"),
                exit_code: 1,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_simple_python() {
        let result = execute_python("print('hello world')", 10, None, &[]).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "hello world");
    }

    #[tokio::test]
    async fn test_execute_python_error() {
        let result = execute_python("raise ValueError('test error')", 10, None, &[]).await;
        assert_ne!(result.exit_code, 0);
        assert!(result.stderr.contains("ValueError"));
    }

    #[tokio::test]
    async fn test_execute_python_timeout() {
        let result = execute_python("import time; time.sleep(60)", 1, None, &[]).await;
        assert_ne!(result.exit_code, 0);
        assert!(result.stderr.contains("timed out") || result.exit_code != 0);
    }
}
