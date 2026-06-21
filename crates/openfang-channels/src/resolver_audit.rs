//! Recipient-resolution audit log (ANAI-55, security follow-up).
//!
//! Writes one line per successful recipient resolution to a dedicated file
//! (`$OPENFANG_HOME/daemon/resolver_audit.log`, mode `0o600`), bypassing the
//! `tracing` subscriber stack entirely.
//!
//! ## Why a dedicated sink, not a `tracing` target?
//!
//! Security review (ANAI-55 §6.1, F1 disposition) required that the audit
//! stream be **structurally** isolated from payload logs so that a log scrape
//! cannot correlate "agent X resolved recipient Y" with adjacent message
//! bodies by timestamp.
//!
//! Routing an audit `tracing` target to a different file requires composing a
//! split `tracing-subscriber` layer graph at every binary entry point
//! (`openfang-cli`, `openfang-desktop`, ...). That correctness is fragile —
//! any future EnvFilter change or wildcard layer can silently re-merge the
//! streams. Writing directly to a dedicated file makes isolation a property
//! of the sink, not a consequence of the subscriber graph.
//!
//! ## Concurrency
//!
//! Resolutions are rare (cache-miss path), so `std::sync::Mutex<File>` is
//! sufficient and avoids any drop-on-shutdown hazard an mpsc writer task
//! would introduce. The lock window is one `writeln!`. **Never** hold this
//! lock across an `.await` — the file write is synchronous by design.
//!
//! ## File mode
//!
//! Created with `0o600` on Unix (audit log MUST NOT be world-readable). On
//! non-Unix platforms the mode hint is dropped silently — those targets are
//! not supported for production daemon use today.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

/// Process-wide singleton. `None` if the audit log could not be opened on
/// first use (e.g., permission denied); resolution proceeds without auditing
/// rather than failing closed on a logging error. The failure is reported
/// via `tracing::error!` exactly once.
static AUDIT_FILE: OnceLock<Option<Mutex<File>>> = OnceLock::new();

fn openfang_home() -> PathBuf {
    if let Ok(h) = std::env::var("OPENFANG_HOME") {
        return PathBuf::from(h);
    }
    if let Ok(h) = std::env::var("HOME") {
        return PathBuf::from(h).join(".openfang");
    }
    PathBuf::from(".openfang")
}

fn audit_log_path() -> PathBuf {
    openfang_home().join("daemon").join("resolver_audit.log")
}

fn open_audit_file(path: &std::path::Path) -> Option<Mutex<File>> {
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::error!(
                "resolver_audit: failed to create log directory {}: {}",
                parent.display(),
                e
            );
            return None;
        }
    }

    let mut opts = OpenOptions::new();
    opts.create(true).append(true);
    #[cfg(unix)]
    {
        // O_NOFOLLOW: refuse to open if the audit path is a symlink.
        // Defense-in-depth: $OPENFANG_HOME is user-owned, but a pre-staged
        // symlink at the audit path would otherwise redirect daemon writes.
        opts.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let file = match opts.open(path) {
        Ok(f) => f,
        Err(e) => {
            tracing::error!(
                "resolver_audit: failed to open log {}: {}",
                path.display(),
                e
            );
            return None;
        }
    };

    // `OpenOptions::mode` only applies on file *creation*. If the file
    // pre-exists with looser permissions (manual touch, stray test, prior
    // build under different umask), converge it to 0o600 unconditionally.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
            tracing::error!(
                "resolver_audit: failed to set mode 0o600 on {}: {}",
                path.display(),
                e
            );
            // Continue: better to audit with looser perms than not at all.
            // The error is recorded above for the operator.
        }
    }

    Some(Mutex::new(file))
}

/// Record a successful recipient resolution. Best-effort: a failure to write
/// the audit line is logged at error level but never propagated to the
/// caller.
///
/// `input` is the raw recipient string as supplied by the agent.
/// `platform_id` is the resolved platform-native ID.
/// `via` is a short tag for the resolution path (e.g. `"snowflake"`,
/// `"channel_mention"`, `"username_cache"`).
pub fn record_resolution(adapter: &str, input: &str, platform_id: &str, via: &str) {
    let slot = AUDIT_FILE.get_or_init(|| open_audit_file(&audit_log_path()));
    let Some(mu) = slot else {
        return;
    };
    let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let line = format!(
        "{ts} adapter={adapter} input={input:?} resolved_platform_id={platform_id} via={via}\n"
    );
    // Recover from a poisoned mutex: with only `writeln!` in the critical
    // section, panic-while-holding is nearly impossible, but if it ever
    // happens we'd silently mute the audit log forever. Take the inner.
    let mut f = mu.lock().unwrap_or_else(|p| p.into_inner());
    if let Err(e) = f.write_all(line.as_bytes()) {
        tracing::error!("resolver_audit: write failed: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `audit_log_path()` composes `$OPENFANG_HOME/daemon/resolver_audit.log`.
    /// We test this via env mutation in exactly one place; all other tests
    /// pass an explicit path into `open_audit_file` to avoid env-var races
    /// under parallel test execution.
    #[test]
    fn audit_log_path_composes_under_openfang_home() {
        let tmp = tempfile::tempdir().unwrap();
        // Single-test env mutation; the result is consumed before any other
        // test could observe it. Still, prefer the explicit-path tests below
        // for new coverage.
        let prev = std::env::var("OPENFANG_HOME").ok();
        std::env::set_var("OPENFANG_HOME", tmp.path());
        let p = audit_log_path();
        match prev {
            Some(v) => std::env::set_var("OPENFANG_HOME", v),
            None => std::env::remove_var("OPENFANG_HOME"),
        }
        assert!(p.starts_with(tmp.path()));
        assert!(p.ends_with("daemon/resolver_audit.log"));
    }

    /// `open_audit_file` creates the file with mode `0o600` on Unix and
    /// creates the parent directory lazily.
    #[test]
    fn open_audit_file_creates_0600_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("daemon").join("resolver_audit.log");

        assert!(open_audit_file(&path).is_some());
        assert!(
            path.exists(),
            "audit file not created at {}",
            path.display()
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let md = std::fs::metadata(&path).unwrap();
            let mode = md.permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "audit file mode is {:o}, expected 0600", mode);
        }
    }

    /// Defense-in-depth: if the audit path pre-exists as a symlink,
    /// `O_NOFOLLOW` must cause the open to fail closed (returns `None`)
    /// instead of redirecting daemon writes to the symlink target.
    #[cfg(unix)]
    #[test]
    fn open_audit_file_refuses_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("daemon").join("resolver_audit.log");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();

        let target = tmp.path().join("symlink_target.log");
        std::fs::write(&target, b"").unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();

        assert!(
            open_audit_file(&path).is_none(),
            "expected None when audit path is a symlink"
        );
        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"",
            "symlink target was written to despite O_NOFOLLOW"
        );
    }

    /// `OpenOptions::mode` only applies on creation. If the file pre-exists
    /// with looser perms, we must converge it to 0o600 on open.
    #[cfg(unix)]
    #[test]
    fn open_audit_file_converges_mode_to_0600() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("daemon").join("resolver_audit.log");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        assert!(open_audit_file(&path).is_some());
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected mode 0o600, got {:o}", mode);
    }
}
