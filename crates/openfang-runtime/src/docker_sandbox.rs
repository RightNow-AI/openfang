//! Docker container sandbox — OS-level isolation for agent code execution.
//!
//! Provides secure command execution inside Docker containers with strict
//! resource limits, network isolation, and capability dropping.

use openfang_types::config::DockerSandboxConfig;
use std::path::Path;
use std::time::Duration;
use tracing::{debug, warn};

/// A running sandbox container.
#[derive(Debug, Clone)]
pub struct SandboxContainer {
    pub container_id: String,
    pub agent_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Result of executing a command in the sandbox.
#[derive(Debug, Clone)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// SECURITY: Sanitize container name — alphanumeric + dash only.
fn sanitize_container_name(name: &str) -> Result<String, String> {
    let sanitized: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if sanitized.is_empty() {
        return Err("Container name cannot be empty".into());
    }
    if sanitized.len() > 63 {
        return Err("Container name too long (max 63 chars)".into());
    }
    Ok(sanitized)
}

/// SECURITY: Validate Docker image name — only allow safe characters.
fn validate_image_name(image: &str) -> Result<(), String> {
    if image.is_empty() {
        return Err("Docker image name cannot be empty".into());
    }
    // Allow: alphanumeric, dots, colons, slashes, dashes, underscores
    if !image
        .chars()
        .all(|c| c.is_alphanumeric() || ".:/-_".contains(c))
    {
        return Err(format!("Invalid Docker image name: {image}"));
    }
    Ok(())
}

/// SECURITY: Sanitize command — reject dangerous shell metacharacters.
/// Delegates to the comprehensive subprocess_sandbox check.
fn validate_command(command: &str) -> Result<(), String> {
    if command.is_empty() {
        return Err("Command cannot be empty".into());
    }
    if let Some(reason) = crate::subprocess_sandbox::contains_shell_metacharacters(command) {
        return Err(format!(
            "Command blocked: contains {reason} — potential injection"
        ));
    }
    Ok(())
}

/// Check if Docker is available on this system.
pub async fn is_docker_available() -> bool {
    match tokio::process::Command::new("docker")
        .arg("version")
        .arg("--format")
        .arg("{{.Server.Version}}")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
    {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

/// Create and start a sandbox container for an agent.
pub async fn create_sandbox(
    config: &DockerSandboxConfig,
    agent_id: &str,
    workspace: &Path,
) -> Result<SandboxContainer, String> {
    validate_image_name(&config.image)?;
    let container_name = sanitize_container_name(&format!(
        "{}-{}",
        config.container_prefix,
        crate::str_utils::safe_truncate_str(agent_id, 8)
    ))?;

    let mut cmd = tokio::process::Command::new("docker");
    cmd.arg("run").arg("-d").arg("--name").arg(&container_name);

    // Resource limits
    cmd.arg("--memory").arg(&config.memory_limit);
    cmd.arg("--cpus").arg(config.cpu_limit.to_string());
    cmd.arg("--pids-limit").arg(config.pids_limit.to_string());

    // Security: drop ALL capabilities, prevent privilege escalation
    cmd.arg("--cap-drop").arg("ALL");
    cmd.arg("--security-opt").arg("no-new-privileges");

    // Add back specific capabilities if configured
    for cap in &config.cap_add {
        // Validate: only allow known capability names (alphanumeric + underscore)
        if cap.chars().all(|c| c.is_alphanumeric() || c == '_') {
            cmd.arg("--cap-add").arg(cap);
        } else {
            warn!("Skipping invalid capability: {cap}");
        }
    }

    // Read-only root filesystem
    if config.read_only_root {
        cmd.arg("--read-only");
    }

    // Network isolation
    cmd.arg("--network").arg(&config.network);

    // tmpfs mounts
    for tmpfs_mount in &config.tmpfs {
        cmd.arg("--tmpfs").arg(tmpfs_mount);
    }

    // Mount workspace read-only
    let ws_str = workspace.display().to_string();
    cmd.arg("-v").arg(format!("{ws_str}:{}:ro", config.workdir));

    // Working directory
    cmd.arg("-w").arg(&config.workdir);

    // Image + command to keep container alive
    cmd.arg(&config.image).arg("sleep").arg("infinity");

    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    debug!(container = %container_name, image = %config.image, "Creating Docker sandbox");

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to run docker: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Docker create failed: {}", stderr.trim()));
    }

    let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

    Ok(SandboxContainer {
        container_id,
        agent_id: agent_id.to_string(),
        created_at: chrono::Utc::now(),
    })
}

/// Execute a command inside an existing sandbox container.
pub async fn exec_in_sandbox(
    container: &SandboxContainer,
    command: &str,
    timeout: Duration,
) -> Result<ExecResult, String> {
    validate_command(command)?;

    let mut cmd = tokio::process::Command::new("docker");
    cmd.arg("exec")
        .arg(&container.container_id)
        .arg("sh")
        .arg("-c")
        .arg(command);

    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    debug!(container = %container.container_id, "Executing in Docker sandbox");

    let output = tokio::time::timeout(timeout, cmd.output())
        .await
        .map_err(|_| format!("Docker exec timed out after {}s", timeout.as_secs()))?
        .map_err(|e| format!("Docker exec failed: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    // Truncate large outputs (char-boundary safe to avoid UTF-8 panics)
    let max_output = 50_000;
    let stdout = if stdout.len() > max_output {
        let safe_end = crate::str_utils::safe_truncate_str(&stdout, max_output);
        format!("{}... [truncated, {} total bytes]", safe_end, stdout.len())
    } else {
        stdout
    };
    let stderr = if stderr.len() > max_output {
        let safe_end = crate::str_utils::safe_truncate_str(&stderr, max_output);
        format!("{}... [truncated, {} total bytes]", safe_end, stderr.len())
    } else {
        stderr
    };

    Ok(ExecResult {
        stdout,
        stderr,
        exit_code,
    })
}

/// Stop and remove a sandbox container.
pub async fn destroy_sandbox(container: &SandboxContainer) -> Result<(), String> {
    debug!(container = %container.container_id, "Destroying Docker sandbox");

    let output = tokio::process::Command::new("docker")
        .arg("rm")
        .arg("-f")
        .arg(&container.container_id)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("Failed to destroy container: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!(container = %container.container_id, "Docker rm failed: {}", stderr.trim());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Container Pool (Gap 5) — reuse containers across sessions
// ---------------------------------------------------------------------------

use dashmap::DashMap;
use std::sync::Arc;

/// Pool entry for a reusable container.
#[derive(Debug, Clone)]
struct PoolEntry {
    container: SandboxContainer,
    config_hash: u64,
    last_used: std::time::Instant,
    created: std::time::Instant,
}

/// Container pool for reusing Docker containers.
pub struct ContainerPool {
    entries: Arc<DashMap<String, PoolEntry>>,
}

impl ContainerPool {
    /// Create a new container pool.
    pub fn new() -> Self {
        Self {
            entries: Arc::new(DashMap::new()),
        }
    }

    /// Acquire a container from the pool matching the config hash, or None.
    pub fn acquire(&self, config_hash: u64, cool_secs: u64) -> Option<SandboxContainer> {
        let mut found_key = None;
        for entry in self.entries.iter() {
            if entry.config_hash == config_hash && entry.last_used.elapsed().as_secs() >= cool_secs
            {
                found_key = Some(entry.key().clone());
                break;
            }
        }
        if let Some(key) = found_key {
            self.entries.remove(&key).map(|(_, e)| e.container)
        } else {
            None
        }
    }

    /// Release a container back to the pool.
    pub fn release(&self, container: SandboxContainer, config_hash: u64) {
        self.entries.insert(
            container.container_id.clone(),
            PoolEntry {
                container,
                config_hash,
                last_used: std::time::Instant::now(),
                created: std::time::Instant::now(),
            },
        );
    }

    /// Cleanup containers older than max_age or idle longer than idle_timeout.
    pub async fn cleanup(&self, idle_timeout_secs: u64, max_age_secs: u64) {
        let to_remove: Vec<(String, SandboxContainer)> = self
            .entries
            .iter()
            .filter(|e| {
                e.last_used.elapsed().as_secs() > idle_timeout_secs
                    || e.created.elapsed().as_secs() > max_age_secs
            })
            .map(|e| (e.key().clone(), e.container.clone()))
            .collect();

        for (key, container) in to_remove {
            debug!(container_id = %container.container_id, "Cleaning up stale pool container");
            let _ = destroy_sandbox(&container).await;
            self.entries.remove(&key);
        }
    }

    /// Number of containers in the pool.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the pool is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for ContainerPool {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Bind Mount Validation (Gap 5) — prevent mounting sensitive host paths
// ---------------------------------------------------------------------------

/// Default blocked mount paths (always blocked regardless of config).
const BLOCKED_MOUNT_PATHS: &[&str] = &[
    "/etc",
    "/proc",
    "/sys",
    "/dev",
    "/var/run/docker.sock",
    "/root",
    "/boot",
];

/// Validate a bind mount path for security.
///
/// Blocks:
/// - Sensitive system paths (/etc, /proc, /sys, Docker socket)
/// - Non-absolute paths
/// - Symlink escape attempts
/// - Paths in the configured blocked_mounts list
pub fn validate_bind_mount(path: &str, blocked: &[String]) -> Result<(), String> {
    let p = std::path::Path::new(path);

    // Must be absolute (Docker bind mounts use Unix paths, so check for '/' prefix
    // in addition to platform-native is_absolute check)
    if !p.is_absolute() && !path.starts_with('/') {
        return Err(format!("Bind mount path must be absolute: {path}"));
    }

    // Check for path traversal
    for component in p.components() {
        if let std::path::Component::ParentDir = component {
            return Err(format!("Bind mount path contains '..': {path}"));
        }
    }

    // Check default blocked paths
    for blocked_path in BLOCKED_MOUNT_PATHS {
        if path.starts_with(blocked_path) {
            return Err(format!(
                "Bind mount to '{blocked_path}' is blocked for security"
            ));
        }
    }

    // Check user-configured blocked paths
    for bp in blocked {
        if path.starts_with(bp.as_str()) {
            return Err(format!("Bind mount to '{bp}' is blocked by configuration"));
        }
    }

    // Check for symlink escape (best-effort — canonicalize if path exists)
    if p.exists() {
        match p.canonicalize() {
            Ok(canonical) => {
                let canonical_str = canonical.to_string_lossy();
                for blocked_path in BLOCKED_MOUNT_PATHS {
                    if canonical_str.starts_with(blocked_path) {
                        return Err(format!(
                            "Bind mount resolves to blocked path via symlink: {} → {}",
                            path, canonical_str
                        ));
                    }
                }
            }
            Err(_) => {
                // Can't canonicalize — path doesn't exist yet, allow it
            }
        }
    }

    Ok(())
}

/// Hash a Docker sandbox config for pool matching.
pub fn config_hash(config: &DockerSandboxConfig) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    config.image.hash(&mut hasher);
    config.network.hash(&mut hasher);
    config.memory_limit.hash(&mut hasher);
    config.workdir.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_container_name_valid() {
        let result = sanitize_container_name("openfang-sandbox-abc123").unwrap();
        assert_eq!(result, "openfang-sandbox-abc123");
    }

    #[test]
    fn test_sanitize_container_name_special_chars() {
        let result = sanitize_container_name("test;rm -rf /").unwrap();
        assert!(!result.contains(';'));
        assert!(!result.contains(' '));
    }

    #[test]
    fn test_sanitize_container_name_empty() {
        assert!(sanitize_container_name("").is_err());
    }

    #[test]
    fn test_sanitize_container_name_too_long() {
        let long = "a".repeat(100);
        assert!(sanitize_container_name(&long).is_err());
    }

    #[test]
    fn test_validate_image_name_valid() {
        assert!(validate_image_name("python:3.12-slim").is_ok());
        assert!(validate_image_name("ubuntu:22.04").is_ok());
        assert!(validate_image_name("registry.example.com/my-image:latest").is_ok());
    }

    #[test]
    fn test_validate_image_name_empty() {
        assert!(validate_image_name("").is_err());
    }

    #[test]
    fn test_validate_image_name_invalid() {
        assert!(validate_image_name("image;rm -rf /").is_err());
        assert!(validate_image_name("image`whoami`").is_err());
        assert!(validate_image_name("image$(id)").is_err());
    }

    #[test]
    fn test_validate_command_valid() {
        assert!(validate_command("python script.py").is_ok());
        assert!(validate_command("ls -la /workspace").is_ok());
    }

    #[test]
    fn test_validate_command_pipe_blocked() {
        // SECURITY: Pipes now blocked by comprehensive metacharacter check
        assert!(validate_command("echo hello | grep h").is_err());
    }

    #[test]
    fn test_validate_command_empty() {
        assert!(validate_command("").is_err());
    }

    #[test]
    fn test_validate_command_backticks() {
        assert!(validate_command("echo `whoami`").is_err());
    }

    #[test]
    fn test_validate_command_dollar_paren() {
        assert!(validate_command("echo $(id)").is_err());
    }

    #[test]
    fn test_validate_command_dollar_brace() {
        assert!(validate_command("echo ${HOME}").is_err());
    }

    #[tokio::test]
    async fn test_docker_available() {
        // Just verify it doesn't panic — result depends on Docker installation
        let _ = is_docker_available().await;
    }

    #[test]
    fn test_config_defaults() {
        let config = DockerSandboxConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.image, "python:3.12-slim");
        assert_eq!(config.container_prefix, "openfang-sandbox");
        assert_eq!(config.workdir, "/workspace");
        assert_eq!(config.network, "none");
        assert_eq!(config.memory_limit, "512m");
        assert_eq!(config.cpu_limit, 1.0);
        assert_eq!(config.timeout_secs, 60);
        assert!(config.read_only_root);
        assert!(config.cap_add.is_empty());
        assert_eq!(config.tmpfs, vec!["/tmp:size=64m"]);
        assert_eq!(config.pids_limit, 100);
    }

    #[test]
    fn test_exec_result_fields() {
        let result = ExecResult {
            stdout: "hello".to_string(),
            stderr: String::new(),
            exit_code: 0,
        };
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "hello");
    }

    // ── Container Pool tests ──────────────────────────────────────────

    #[test]
    fn test_container_pool_empty() {
        let pool = ContainerPool::new();
        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn test_container_pool_release_acquire() {
        let pool = ContainerPool::new();
        let container = SandboxContainer {
            container_id: "test123".to_string(),
            agent_id: "agent1".to_string(),
            created_at: chrono::Utc::now(),
        };
        pool.release(container, 12345);
        assert_eq!(pool.len(), 1);

        // Acquire with same hash — should succeed (cool_secs=0 for test)
        let acquired = pool.acquire(12345, 0);
        assert!(acquired.is_some());
        assert_eq!(acquired.unwrap().container_id, "test123");
        assert!(pool.is_empty());
    }

    #[test]
    fn test_container_pool_hash_mismatch() {
        let pool = ContainerPool::new();
        let container = SandboxContainer {
            container_id: "test123".to_string(),
            agent_id: "agent1".to_string(),
            created_at: chrono::Utc::now(),
        };
        pool.release(container, 12345);

        // Acquire with different hash — should fail
        let acquired = pool.acquire(99999, 0);
        assert!(acquired.is_none());
    }

    // ── Bind Mount Validation tests ──────────────────────────────────

    #[test]
    fn test_validate_bind_mount_valid() {
        assert!(validate_bind_mount("/home/user/workspace", &[]).is_ok());
        assert!(validate_bind_mount("/tmp/sandbox", &[]).is_ok());
    }

    #[test]
    fn test_validate_bind_mount_non_absolute() {
        assert!(validate_bind_mount("relative/path", &[]).is_err());
    }

    #[test]
    fn test_validate_bind_mount_blocked_paths() {
        assert!(validate_bind_mount("/etc/passwd", &[]).is_err());
        assert!(validate_bind_mount("/proc/self", &[]).is_err());
        assert!(validate_bind_mount("/sys/kernel", &[]).is_err());
        assert!(validate_bind_mount("/var/run/docker.sock", &[]).is_err());
    }

    #[test]
    fn test_validate_bind_mount_traversal() {
        assert!(validate_bind_mount("/home/user/../etc/passwd", &[]).is_err());
    }

    #[test]
    fn test_validate_bind_mount_custom_blocked() {
        let blocked = vec!["/data/secrets".to_string()];
        assert!(validate_bind_mount("/data/secrets/vault", &blocked).is_err());
        assert!(validate_bind_mount("/data/public", &blocked).is_ok());
    }

    #[test]
    fn test_config_hash_deterministic() {
        let c1 = DockerSandboxConfig::default();
        let c2 = DockerSandboxConfig::default();
        assert_eq!(config_hash(&c1), config_hash(&c2));
    }

    #[test]
    fn test_config_hash_different_images() {
        let c1 = DockerSandboxConfig::default();
        let c2 = DockerSandboxConfig {
            image: "node:20-slim".to_string(),
            ..Default::default()
        };
        assert_ne!(config_hash(&c1), config_hash(&c2));
    }
}
