//! Workspace filesystem sandboxing.
//!
//! Confines agent file operations to their workspace directory.
//! Prevents path traversal, symlink escapes, and access outside the sandbox.
//!
//! Defense in depth: in addition to confining to `workspace_root`, this module
//! hard-denies access to a curated set of sensitive paths under the OpenFang
//! home directory (secrets, credentials, runtime tokens, daemon logs) and under
//! `$HOME` (e.g. `~/.mcp-auth/` OAuth tokens). The deny-list fires regardless of
//! `workspace_root`, so a misconfigured manifest (e.g. `workspace_root =
//! "~/.openfang"`) or a future tool surface that bypasses workspace confinement
//! cannot exfiltrate these files.
//!
//! This sensitive-path floor ships alongside `file_policy` (path-tier
//! governance): the floor is absolute and runs before any tier evaluation, so
//! no `file_policy` rule can ever widen access past a sensitive-path deny.

use openfang_types::config::{FileAccessTier, FilePolicy};
use std::path::{Path, PathBuf};

/// Resolve the OpenFang home directory.
///
/// Priority: `OPENFANG_HOME` env var > `~/.openfang`.
///
/// Mirrors `openfang_types::config::openfang_home_dir` (private there). Kept
/// local to avoid a cross-crate dependency cycle from runtime -> types.
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

/// Returns `Some(reason)` if `path` resolves to a sensitive credential
/// directory/file under the user's `$HOME` but outside `$OPENFANG_HOME` that
/// must never be read or written by an agent, regardless of `workspace_root`.
///
/// This is the curated **hard-deny floor** (Layer 1): an absolute, curated set
/// of credential-bearing paths. It fires for *any* agent on *any* workspace,
/// including misconfigured ones, and no `file_policy` allow rule can widen past
/// it. Other `$HOME` dotfiles are NOT hard-denied here — they fall to the
/// `file_policy` prompt tier (Layer 2), so a coarse allow rule cannot silently
/// reach them.
///
/// Matching is component-sequence aware: single-component entries (e.g.
/// `.ssh`) deny the whole subtree; two-deep entries (e.g. `.config/gcloud`)
/// deny only that named child so sibling `.config/*` reads still fall to
/// prompt rather than being hard-denied.
///
/// The set is hardcoded for v1 (not config-extensible — YAGNI). Add new
/// entries conservatively: over-denial here costs more than under-denial
/// elsewhere.
///
/// `path` should be canonicalized before this check.
pub(crate) fn is_sensitive_home_path(path: &Path) -> Option<&'static str> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let canon_home = home.canonicalize().ok()?;
    let rel = path.strip_prefix(&canon_home).ok()?;
    let mut comps = rel.components();
    let first = comps.next()?.as_os_str().to_str()?;

    match first {
        // OAuth tokens for mcp-remote and similar MCP clients.
        ".mcp-auth" => Some("oauth-tokens"),
        // SSH private keys, known_hosts, client config.
        ".ssh" => Some("ssh"),
        // Cloud / cluster credentials.
        ".aws" => Some("aws-credentials"),
        ".gnupg" => Some("gpg-keyring"),
        ".kube" => Some("kube-config"),
        // Single-file credential stores.
        ".netrc" => Some("netrc"),
        ".git-credentials" => Some("git-credentials"),
        ".npmrc" => Some("npmrc"),
        ".pypirc" => Some("pypirc"),
        ".pgpass" => Some("pgpass"),
        // Two-deep credential dirs/files: match on the second component so
        // sibling entries (e.g. `.config/nvim`) fall through to prompt.
        ".config" => match comps.next()?.as_os_str().to_str()? {
            "gcloud" => Some("gcloud-credentials"),
            "gh" => Some("gh-token"),
            _ => None,
        },
        ".docker" => match comps.next()?.as_os_str().to_str()? {
            "config.json" => Some("docker-config"),
            _ => None,
        },
        ".cargo" => match comps.next()?.as_os_str().to_str()? {
            "credentials" | "credentials.toml" => Some("cargo-credentials"),
            _ => None,
        },
        _ => None,
    }
}

/// The sandbox *floor*: `..`-rejection + canonicalization + the sensitive-path
/// deny-list. Absolute and non-overridable — no `file_policy` tier rule can
/// widen past it. Returns the canonical candidate WITHOUT applying the
/// workspace clamp (that is [`enforce_workspace_clamp`]'s job).
pub(crate) fn sandbox_floor(user_path: &str, workspace_root: &Path) -> Result<PathBuf, String> {
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

    // Defense in depth (continued): hard-deny sensitive `$HOME` paths outside
    // `$OPENFANG_HOME` — e.g. `~/.mcp-auth/` OAuth tokens.
    if let Some(reason) = is_sensitive_home_path(&canon_candidate) {
        tracing::warn!(
            target: "openfang_runtime::sandbox",
            user_path = %user_path,
            resolved = %canon_candidate.display(),
            reason = reason,
            "Access denied: sensitive home path"
        );
        return Err(format!(
            "Access denied: path '{}' resolves to a protected user-home \
             resource ({}). These paths are never accessible to agents.",
            user_path, reason
        ));
    }

    Ok(canon_candidate)
}

/// The legacy workspace clamp: require the canonical candidate to live under
/// the canonical workspace root. This is what `file_policy` replaces with tier
/// evaluation when a policy is active.
fn enforce_workspace_clamp(
    canon_candidate: PathBuf,
    workspace_root: &Path,
) -> Result<PathBuf, String> {
    let canon_root = workspace_root
        .canonicalize()
        .map_err(|e| format!("Failed to resolve workspace root: {e}"))?;

    if !canon_candidate.starts_with(&canon_root) {
        return Err(format!(
            "Access denied: path '{}' resolves outside workspace. \
             If you have an MCP filesystem server configured, use the \
             mcp_filesystem_* tools (e.g. mcp_filesystem_read_file, \
             mcp_filesystem_list_directory) to access files outside \
             the workspace.",
            canon_candidate.display()
        ));
    }
    Ok(canon_candidate)
}

/// Resolve a user-supplied path within a workspace sandbox (legacy behavior:
/// floor + workspace clamp).
///
/// - Rejects `..` components outright.
/// - Relative paths are joined with `workspace_root`.
/// - Absolute paths are checked against the workspace root after canonicalization.
/// - For new files: canonicalizes the parent directory and appends the filename.
/// - The final canonical path must start with the canonical workspace root.
/// - Hard-denies sensitive OpenFang / `$HOME` paths regardless of workspace root.
///
/// Public signature unchanged so existing callers compile untouched; the
/// policy-aware path is [`resolve_with_policy`].
pub fn resolve_sandbox_path(user_path: &str, workspace_root: &Path) -> Result<PathBuf, String> {
    let canon = sandbox_floor(user_path, workspace_root)?;
    enforce_workspace_clamp(canon, workspace_root)
}

/// Resolve a user-supplied path under an optional `file_policy`.
///
/// - The floor always runs first (`..`-reject, canonicalize, sensitive-path deny).
/// - `None` or a disabled policy => legacy workspace clamp (back-compat).
/// - An enabled policy => longest-prefix tier evaluation against the canonical
///   path. `Deny` rejects; `Read` rejects writes; `Write` allows; `Prompt`
///   allows only when `prompt_preapproved` (the caller's pre-pass gated it),
///   otherwise fails closed.
pub fn resolve_with_policy(
    user_path: &str,
    workspace_root: &Path,
    file_policy: Option<&FilePolicy>,
    needs_write: bool,
    prompt_preapproved: bool,
) -> Result<PathBuf, String> {
    let canon = sandbox_floor(user_path, workspace_root)?;
    match file_policy {
        None => enforce_workspace_clamp(canon, workspace_root),
        Some(fp) if !fp.is_active() => enforce_workspace_clamp(canon, workspace_root),
        Some(fp) => {
            let canon_root = workspace_root
                .canonicalize()
                .map_err(|e| format!("Failed to resolve workspace root: {e}"))?;
            match fp.tier_for(&canon, &canon_root) {
                FileAccessTier::Deny => Err(format!(
                    "Access denied by file_policy: '{}' (deny tier)",
                    canon.display()
                )),
                FileAccessTier::Read if needs_write => Err(format!(
                    "Access denied by file_policy: '{}' is read-only",
                    canon.display()
                )),
                FileAccessTier::Read | FileAccessTier::Write => Ok(canon),
                FileAccessTier::Prompt if prompt_preapproved => Ok(canon),
                FileAccessTier::Prompt => Err(format!(
                    "Access denied by file_policy: '{}' requires approval \
                     (prompt tier) and was not pre-approved",
                    canon.display()
                )),
            }
        }
    }
}

/// F5 TOCTOU guard. Confirm the path resolved for the actual filesystem
/// operation matches the canonical path validated (and possibly approval-gated)
/// earlier in the same tool call. A mismatch means the target changed between
/// validation and use — e.g. a symlink swapped during a human approval window —
/// so fail closed.
///
/// This closes the approval-window TOCTOU. A narrow residual remains between
/// this final resolution and the kernel `open()` (the canonicalize->open
/// window); eliminating it needs `openat`/`O_NOFOLLOW` semantics that std does
/// not expose portably, tracked as a fast-follow.
pub(crate) fn assert_prevalidated(
    resolved: &Path,
    prevalidated: Option<&Path>,
) -> Result<(), String> {
    match prevalidated {
        Some(expected) if expected != resolved => Err(format!(
            "Access denied: target changed during the validation/approval window (validated '{}', now resolves to '{}') — possible TOCTOU; failing closed.",
            expected.display(),
            resolved.display()
        )),
        _ => Ok(()),
    }
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
    fn test_sensitive_mcp_auth_blocked() {
        // ~/.mcp-auth/ holds OAuth tokens for mcp-remote and similar; must
        // be denied regardless of where the workspace sits.
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .and_then(|h| h.canonicalize().ok());
        let Some(home) = home else {
            return; // $HOME unset or unreadable on this host; skip.
        };
        assert_eq!(
            is_sensitive_home_path(&home.join(".mcp-auth").join("mcp-remote-0.1.37")),
            Some("oauth-tokens")
        );
        // Sibling dotdirs must not be over-denied.
        assert_eq!(is_sensitive_home_path(&home.join(".cache")), None);
        // Paths outside $HOME must not be classified.
        assert_eq!(
            is_sensitive_home_path(std::path::Path::new("/tmp/foo")),
            None
        );
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

    // -------------------------------------------------------------------
    // file_policy tier-evaluation tests (resolve_with_policy)
    // -------------------------------------------------------------------

    use openfang_types::config::{FileAccessTier, FilePolicy, FileRule};

    fn policy(default_tier: FileAccessTier, rules: Vec<(&str, FileAccessTier)>) -> FilePolicy {
        FilePolicy::new(
            true,
            default_tier,
            rules
                .into_iter()
                .map(|(p, t)| FileRule {
                    path: p.to_string(),
                    tier: t,
                })
                .collect(),
        )
    }

    #[test]
    fn test_policy_none_matches_legacy_clamp() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("f.txt"), "x").unwrap();
        let with_none = resolve_with_policy("f.txt", dir.path(), None, false, true);
        let legacy = resolve_sandbox_path("f.txt", dir.path());
        assert_eq!(with_none.is_ok(), legacy.is_ok());
        assert_eq!(with_none.unwrap(), legacy.unwrap());
    }

    #[test]
    fn test_policy_disabled_matches_legacy_clamp() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("f.txt"), "x").unwrap();
        let mut fp = policy(FileAccessTier::Deny, vec![]);
        fp.enabled = false;
        let r = resolve_with_policy("f.txt", dir.path(), Some(&fp), true, false);
        assert!(r.is_ok(), "disabled policy must fall back to legacy clamp");
    }

    #[test]
    fn test_policy_default_deny_blocks_unmatched() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("f.txt"), "x").unwrap();
        let fp = policy(FileAccessTier::Deny, vec![]);
        let r = resolve_with_policy("f.txt", dir.path(), Some(&fp), false, false);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("deny tier"));
    }

    #[test]
    fn test_policy_read_tier_blocks_write_allows_read() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("f.txt"), "x").unwrap();
        let fp = policy(FileAccessTier::Read, vec![]);
        assert!(resolve_with_policy("f.txt", dir.path(), Some(&fp), false, false).is_ok());
        let w = resolve_with_policy("f.txt", dir.path(), Some(&fp), true, false);
        assert!(w.is_err());
        assert!(w.unwrap_err().contains("read-only"));
    }

    #[test]
    fn test_policy_longest_prefix_wins() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("f.txt"), "x").unwrap();
        std::fs::write(dir.path().join("top.txt"), "x").unwrap();
        // whole workspace = read, but sub/ = deny (longer prefix wins).
        let fp = policy(
            FileAccessTier::Deny,
            vec![(".", FileAccessTier::Read), ("sub", FileAccessTier::Deny)],
        );
        assert!(resolve_with_policy("top.txt", dir.path(), Some(&fp), false, false).is_ok());
        assert!(resolve_with_policy("sub/f.txt", dir.path(), Some(&fp), false, false).is_err());
    }

    #[test]
    fn test_policy_prompt_fail_closed_without_preapproval() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("f.txt"), "x").unwrap();
        let fp = policy(FileAccessTier::Prompt, vec![]);
        // Not pre-approved => denied.
        assert!(resolve_with_policy("f.txt", dir.path(), Some(&fp), true, false).is_err());
        // Pre-approved (execute_tool's pre-pass gated it) => allowed.
        assert!(resolve_with_policy("f.txt", dir.path(), Some(&fp), true, true).is_ok());
    }

    #[test]
    fn test_sensitive_home_credential_floor() {
        // Curated Layer-1 hard-deny set: credential-bearing $HOME paths that
        // must deny regardless of workspace_root, joining .mcp-auth.
        let Some(home) = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .and_then(|h| h.canonicalize().ok())
        else {
            return; // $HOME unset/unreadable; skip.
        };

        // Single-component dirs hard-deny their whole subtree.
        assert_eq!(
            is_sensitive_home_path(&home.join(".ssh").join("id_ed25519")),
            Some("ssh")
        );
        assert_eq!(
            is_sensitive_home_path(&home.join(".aws").join("credentials")),
            Some("aws-credentials")
        );
        assert_eq!(
            is_sensitive_home_path(&home.join(".gnupg").join("secring.gpg")),
            Some("gpg-keyring")
        );
        assert_eq!(
            is_sensitive_home_path(&home.join(".kube").join("config")),
            Some("kube-config")
        );
        // Single-file credential stores.
        assert_eq!(is_sensitive_home_path(&home.join(".netrc")), Some("netrc"));
        assert_eq!(
            is_sensitive_home_path(&home.join(".git-credentials")),
            Some("git-credentials")
        );
        assert_eq!(is_sensitive_home_path(&home.join(".npmrc")), Some("npmrc"));
        assert_eq!(
            is_sensitive_home_path(&home.join(".pypirc")),
            Some("pypirc")
        );
        assert_eq!(
            is_sensitive_home_path(&home.join(".pgpass")),
            Some("pgpass")
        );

        // Two-deep entries hard-deny only the named child...
        assert_eq!(
            is_sensitive_home_path(&home.join(".config").join("gcloud").join("creds.db")),
            Some("gcloud-credentials")
        );
        assert_eq!(
            is_sensitive_home_path(&home.join(".config").join("gh").join("hosts.yml")),
            Some("gh-token")
        );
        assert_eq!(
            is_sensitive_home_path(&home.join(".docker").join("config.json")),
            Some("docker-config")
        );
        assert_eq!(
            is_sensitive_home_path(&home.join(".cargo").join("credentials.toml")),
            Some("cargo-credentials")
        );

        // ...while sibling .config/* and .docker/* fall through to None
        // (Layer-2 prompt territory, NOT hard-denied).
        assert_eq!(
            is_sensitive_home_path(&home.join(".config").join("nvim").join("init.lua")),
            None
        );
        assert_eq!(is_sensitive_home_path(&home.join(".config")), None);
        assert_eq!(
            is_sensitive_home_path(&home.join(".docker").join("contexts")),
            None
        );
        // A non-credential dotfile is not hard-denied (prompt backstops it).
        assert_eq!(is_sensitive_home_path(&home.join(".gitconfig")), None);
    }

    #[test]
    fn test_assert_prevalidated_toctou_guard() {
        // No pre-validated path => always ok (legacy / non-policy path).
        assert!(assert_prevalidated(Path::new("/a/b"), None).is_ok());
        // Matching => ok.
        assert!(assert_prevalidated(Path::new("/a/b"), Some(Path::new("/a/b"))).is_ok());
        // Diverged after validation => fail closed.
        let e = assert_prevalidated(Path::new("/evil"), Some(Path::new("/a/b")));
        assert!(e.is_err());
        assert!(e.unwrap_err().contains("TOCTOU"));
    }
}
