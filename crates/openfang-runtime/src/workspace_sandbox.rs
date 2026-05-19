//! Workspace filesystem sandboxing.
//!
//! Confines agent file operations to their workspace directory.
//! Prevents path traversal, symlink escapes, and access outside the sandbox.
//!
//! Defense in depth: in addition to confining to `workspace_root`, this module
//! hard-denies access to a curated set of sensitive paths under the OpenFang
//! home directory (secrets, credentials, runtime tokens, daemon logs). The
//! deny-list fires regardless of `workspace_root` configuration, so a
//! misconfigured agent manifest (e.g. `workspace_root = "~/.openfang"`) or a
//! future tool surface that bypasses workspace confinement cannot exfiltrate
//! these files.

use std::path::{Path, PathBuf};

/// Resolve the OpenFang home directory.
///
/// Priority: `OPENFANG_HOME` env var > `~/.openfang`.
///
/// Mirrors `openfang_types::config::openfang_home_dir` (private there). Kept
/// local to avoid a cross-crate dependency cycle from runtime → types.
fn openfang_home() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("OPENFANG_HOME") {
        return Some(PathBuf::from(home));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".openfang"))
}

/// Returns `Some(reason)` if `path` resolves to a sensitive file/dir under the
/// OpenFang home that must never be read or written by an agent, regardless of
/// the agent's `workspace_root`.
///
/// `path` should be canonicalized (or its parent canonicalized + filename
/// joined) before this check, so symlink escapes have already been resolved.
///
/// The categories returned are stable strings suitable for WARN-log audit.
pub(crate) fn is_sensitive_openfang_path(path: &Path) -> Option<&'static str> {
    let home = openfang_home()?;
    // Canonicalize the home root if it exists so we compare like-for-like with
    // the (already canonicalized) candidate. If the home doesn't exist yet,
    // there is nothing sensitive in it.
    let canon_home = home.canonicalize().ok()?;
    let rel = path.strip_prefix(&canon_home).ok()?;
    let first = rel.components().next()?.as_os_str().to_str()?;

    match first {
        // Tier 1: credentials / secrets
        "config.toml" => Some("config"),
        "secrets.env" | ".env" => Some("secrets"),
        "daemon.json" => Some("daemon-state"),
        s if s.starts_with("config.toml.bak") => Some("config-backup"),
        s if s.starts_with("gcp-key")
            || s.ends_with(".pem")
            || s.ends_with(".key")
            || s.ends_with(".p12")
            || s.ends_with(".pfx") =>
        {
            Some("credential-file")
        }
        // Tier 2: impersonation / runtime surface
        "run" => Some("runtime-tokens"),
        "vault" => Some("credential-vault"),
        "paired_devices.json" => Some("paired-devices"),
        // Tier 3: log exfil / recon
        "daemon.stderr.log" | "daemon.stdout.log" => Some("daemon-log"),
        s if s.starts_with("daemon.stderr.log.") || s.starts_with("daemon.stdout.log.") => {
            Some("daemon-log")
        }
        _ => None,
    }
}

/// Resolve a user-supplied path within a workspace sandbox.
///
/// - Rejects `..` components outright.
/// - Relative paths are joined with `workspace_root`.
/// - Absolute paths are checked against the workspace root after canonicalization.
/// - For new files: canonicalizes the parent directory and appends the filename.
/// - The final canonical path must start with the canonical workspace root.
/// - Hard-denies sensitive OpenFang paths (`config.toml`, secrets, vault, run/,
///   daemon logs, credential files) regardless of workspace root.
pub fn resolve_sandbox_path(user_path: &str, workspace_root: &Path) -> Result<PathBuf, String> {
    let path = Path::new(user_path);

    // Reject any `..` components
    for component in path.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err("Path traversal denied: '..' components are forbidden".to_string());
        }
    }

    // Build the candidate path
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace_root.join(path)
    };

    // Canonicalize the workspace root
    let canon_root = workspace_root
        .canonicalize()
        .map_err(|e| format!("Failed to resolve workspace root: {e}"))?;

    // Canonicalize the candidate (or its parent for new files)
    let canon_candidate = if candidate.exists() {
        candidate
            .canonicalize()
            .map_err(|e| format!("Failed to resolve path: {e}"))?
    } else {
        // For new files: canonicalize the parent and append the filename
        let parent = candidate
            .parent()
            .ok_or_else(|| "Invalid path: no parent directory".to_string())?;
        let filename = candidate
            .file_name()
            .ok_or_else(|| "Invalid path: no filename".to_string())?;
        let canon_parent = parent
            .canonicalize()
            .map_err(|e| format!("Failed to resolve parent directory: {e}"))?;
        canon_parent.join(filename)
    };

    // Defense in depth: hard-deny sensitive OpenFang paths regardless of
    // whether they fall inside the (possibly mis-scoped) workspace root.
    if let Some(reason) = is_sensitive_openfang_path(&canon_candidate) {
        tracing::warn!(
            target: "openfang_runtime::sandbox",
            user_path = %user_path,
            resolved = %canon_candidate.display(),
            reason = reason,
            "Access denied: sensitive OpenFang path"
        );
        return Err(format!(
            "Access denied: path '{}' resolves to a protected OpenFang \
             resource ({}). These paths are never accessible to agents.",
            user_path, reason
        ));
    }

    // Verify the canonical path is inside the workspace
    if !canon_candidate.starts_with(&canon_root) {
        return Err(format!(
            "Access denied: path '{}' resolves outside workspace. \
             If you have an MCP filesystem server configured, use the \
             mcp_filesystem_* tools (e.g. mcp_filesystem_read_file, \
             mcp_filesystem_list_directory) to access files outside \
             the workspace.",
            user_path
        ));
    }

    Ok(canon_candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_relative_path_inside_workspace() {
        let dir = TempDir::new().unwrap();
        let data_dir = dir.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::write(data_dir.join("test.txt"), "hello").unwrap();

        let result = resolve_sandbox_path("data/test.txt", dir.path());
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert!(resolved.starts_with(dir.path().canonicalize().unwrap()));
    }

    #[test]
    fn test_absolute_path_inside_workspace() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("file.txt"), "ok").unwrap();
        let abs_path = dir.path().join("file.txt");

        let result = resolve_sandbox_path(abs_path.to_str().unwrap(), dir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_absolute_path_outside_workspace_blocked() {
        let dir = TempDir::new().unwrap();
        let outside = std::env::temp_dir().join("outside_test.txt");
        std::fs::write(&outside, "nope").unwrap();

        let result = resolve_sandbox_path(outside.to_str().unwrap(), dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Access denied"));

        let _ = std::fs::remove_file(&outside);
    }

    #[test]
    fn test_dotdot_component_blocked() {
        let dir = TempDir::new().unwrap();
        let result = resolve_sandbox_path("../../../etc/passwd", dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Path traversal denied"));
    }

    #[test]
    fn test_nonexistent_file_with_valid_parent() {
        let dir = TempDir::new().unwrap();
        let data_dir = dir.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();

        let result = resolve_sandbox_path("data/new_file.txt", dir.path());
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert!(resolved.starts_with(dir.path().canonicalize().unwrap()));
        assert!(resolved.ends_with("new_file.txt"));
    }

    #[cfg(unix)]
    #[test]
    fn test_symlink_escape_blocked() {
        let dir = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "secret").unwrap();

        // Create a symlink inside the workspace pointing outside
        let link_path = dir.path().join("escape");
        std::os::unix::fs::symlink(outside.path(), &link_path).unwrap();

        let result = resolve_sandbox_path("escape/secret.txt", dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Access denied"));
    }

    // -------------------------------------------------------------------
    // Sensitive-path deny-list tests
    //
    // These tests stand up a fake OpenFang home via OPENFANG_HOME and verify
    // that `is_sensitive_openfang_path` classifies each tier correctly.
    //
    // We use a process-wide env mutex because OPENFANG_HOME is global state.
    // -------------------------------------------------------------------

    use std::sync::Mutex;
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct FakeHome {
        _dir: TempDir,
        path: PathBuf,
        _guard: std::sync::MutexGuard<'static, ()>,
        prev: Option<String>,
    }

    impl FakeHome {
        fn new() -> Self {
            let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let prev = std::env::var("OPENFANG_HOME").ok();
            let dir = TempDir::new().unwrap();
            let path = dir.path().canonicalize().unwrap();
            std::env::set_var("OPENFANG_HOME", &path);
            Self {
                _dir: dir,
                path,
                _guard: guard,
                prev,
            }
        }
    }

    impl Drop for FakeHome {
        fn drop(&mut self) {
            match &self.prev {
                Some(v) => std::env::set_var("OPENFANG_HOME", v),
                None => std::env::remove_var("OPENFANG_HOME"),
            }
        }
    }

    #[test]
    fn test_sensitive_config_toml() {
        let h = FakeHome::new();
        assert_eq!(
            is_sensitive_openfang_path(&h.path.join("config.toml")),
            Some("config")
        );
        assert_eq!(
            is_sensitive_openfang_path(&h.path.join("config.toml.bak-20260101")),
            Some("config-backup")
        );
    }

    #[test]
    fn test_sensitive_secrets_and_env() {
        let h = FakeHome::new();
        assert_eq!(
            is_sensitive_openfang_path(&h.path.join("secrets.env")),
            Some("secrets")
        );
        assert_eq!(
            is_sensitive_openfang_path(&h.path.join(".env")),
            Some("secrets")
        );
        assert_eq!(
            is_sensitive_openfang_path(&h.path.join("daemon.json")),
            Some("daemon-state")
        );
    }

    #[test]
    fn test_sensitive_credential_files() {
        let h = FakeHome::new();
        assert_eq!(
            is_sensitive_openfang_path(&h.path.join("gcp-key--annabelle-service-01.json")),
            Some("credential-file")
        );
        assert_eq!(
            is_sensitive_openfang_path(&h.path.join("tls.pem")),
            Some("credential-file")
        );
        assert_eq!(
            is_sensitive_openfang_path(&h.path.join("agent.key")),
            Some("credential-file")
        );
    }

    #[test]
    fn test_sensitive_runtime_and_vault() {
        let h = FakeHome::new();
        assert_eq!(
            is_sensitive_openfang_path(&h.path.join("run").join("mcp-config-abc.json")),
            Some("runtime-tokens")
        );
        assert_eq!(
            is_sensitive_openfang_path(&h.path.join("vault").join("anything")),
            Some("credential-vault")
        );
        assert_eq!(
            is_sensitive_openfang_path(&h.path.join("paired_devices.json")),
            Some("paired-devices")
        );
    }

    #[test]
    fn test_sensitive_daemon_logs() {
        let h = FakeHome::new();
        assert_eq!(
            is_sensitive_openfang_path(&h.path.join("daemon.stderr.log")),
            Some("daemon-log")
        );
        assert_eq!(
            is_sensitive_openfang_path(&h.path.join("daemon.stdout.log.1")),
            Some("daemon-log")
        );
        assert_eq!(
            is_sensitive_openfang_path(&h.path.join("daemon.stderr.log.2026-05-19")),
            Some("daemon-log")
        );
    }

    #[test]
    fn test_non_sensitive_paths_pass() {
        let h = FakeHome::new();
        // Workspaces, skills, bin, src, etc. must remain accessible.
        assert_eq!(
            is_sensitive_openfang_path(&h.path.join("workspaces").join("foo").join("data.txt")),
            None
        );
        assert_eq!(
            is_sensitive_openfang_path(&h.path.join("skills").join("x.md")),
            None
        );
        assert_eq!(
            is_sensitive_openfang_path(&h.path.join("bin").join("openfang")),
            None
        );
        // Paths outside OPENFANG_HOME entirely.
        assert_eq!(is_sensitive_openfang_path(Path::new("/tmp/foo")), None);
    }

    #[test]
    fn test_resolve_blocks_sensitive_even_when_inside_misconfigured_root() {
        // Simulate the bad case: workspace_root = OPENFANG_HOME itself.
        let h = FakeHome::new();
        // Plant a config.toml inside the fake home.
        std::fs::write(h.path.join("config.toml"), "secret = true").unwrap();

        let result = resolve_sandbox_path("config.toml", &h.path);
        assert!(result.is_err(), "expected sensitive-path denial");
        let err = result.unwrap_err();
        assert!(err.contains("protected OpenFang resource"), "got: {}", err);
        assert!(err.contains("config"), "got: {}", err);
    }

    #[test]
    fn test_resolve_blocks_secrets_env_via_absolute_path() {
        let h = FakeHome::new();
        std::fs::write(h.path.join("secrets.env"), "X=1").unwrap();
        // Workspace root is a subdir; absolute path attempts to escape upward.
        let ws = h.path.join("workspaces").join("agent");
        std::fs::create_dir_all(&ws).unwrap();
        let abs = h.path.join("secrets.env");
        let result = resolve_sandbox_path(abs.to_str().unwrap(), &ws);
        assert!(result.is_err());
        // Either the outside-workspace check or the sensitive-path check may
        // fire first; we only require that one of them denies access.
        let err = result.unwrap_err();
        assert!(err.contains("Access denied"), "got: {}", err);
    }
}
