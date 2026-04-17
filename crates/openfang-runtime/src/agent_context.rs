//! Per-turn agent context loader for external `context.md` files.
//!
//! Some agents depend on a `context.md` file updated by external tools (e.g. a
//! cron job that writes live market data, or a script that refreshes project
//! state). Before issue #843 this file was read once when the session started
//! and then cached for the lifetime of the conversation, so external updates
//! never reached the LLM.
//!
//! The default behaviour is now a small disk read per turn when the prompt is
//! assembled. Agents that depend on the old behaviour can opt back in via the
//! `cache_context` flag on their manifest.
//!
//! This module intentionally does not participate in per-token streaming — it
//! is called once per agent turn, right before the system prompt is built.
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::{collections::HashMap, fs};

use tracing::{debug, warn};

/// Maximum size of `context.md` to inject into the prompt (32 KB).
///
/// Matches the cap used by [`crate::workspace_context`] and the kernel's
/// identity-file reader so a runaway file cannot blow up the prompt.
const MAX_CONTEXT_BYTES: u64 = 32_768;

/// Filename that agents use for per-turn refreshable context.
pub const CONTEXT_FILENAME: &str = "context.md";

/// In-memory cache of the last successful read for each workspace.
///
/// Used for two purposes:
/// 1. When `cache_context = true`, the first successful read is returned on
///    every subsequent call.
/// 2. When `cache_context = false` and a re-read fails on disk (e.g. the file
///    was temporarily replaced by an external writer), we fall back to the
///    previous content instead of dropping context mid-conversation.
fn cache() -> &'static Mutex<HashMap<PathBuf, String>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Load the agent's `context.md` for this turn.
///
/// Returns the current on-disk content, or — if the read fails after a
/// previous success — the cached content with a warning. Returns `None` when
/// no context.md has ever been seen for this workspace.
///
/// When `cache_context` is true the first successful read is stored and
/// returned verbatim on every future call. Callers pass the flag straight from
/// `AgentManifest::cache_context`.
pub fn load_context_md(workspace: &Path, cache_context: bool) -> Option<String> {
    let path = workspace.join(CONTEXT_FILENAME);

    if cache_context {
        if let Some(cached) = get_cached(&path) {
            return Some(cached);
        }
    }

    match read_capped(&path) {
        Ok(Some(content)) => {
            store_cached(&path, &content);
            Some(content)
        }
        Ok(None) => {
            // File is absent or empty — do not serve a stale cache for a
            // deleted file unless the caller explicitly opted into caching.
            if cache_context {
                get_cached(&path)
            } else {
                None
            }
        }
        Err(e) => {
            if let Some(prev) = get_cached(&path) {
                warn!(
                    path = %path.display(),
                    error = %e,
                    "Failed to re-read context.md; falling back to cached content"
                );
                Some(prev)
            } else {
                debug!(path = %path.display(), error = %e, "context.md unreadable and no cache");
                None
            }
        }
    }
}

fn get_cached(path: &Path) -> Option<String> {
    cache()
        .lock()
        .ok()
        .and_then(|guard| guard.get(path).cloned())
}

fn store_cached(path: &Path, content: &str) {
    if let Ok(mut guard) = cache().lock() {
        guard.insert(path.to_path_buf(), content.to_string());
    }
}

/// Read the file, returning Ok(None) if it is missing or empty, and
/// Ok(Some(...)) if it has usable content. Oversized files are truncated to
/// [`MAX_CONTEXT_BYTES`] so prompt size remains bounded.
fn read_capped(path: &Path) -> std::io::Result<Option<String>> {
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    if !meta.is_file() {
        return Ok(None);
    }
    let content = fs::read_to_string(path)?;
    if content.trim().is_empty() {
        return Ok(None);
    }
    if meta.len() > MAX_CONTEXT_BYTES {
        let truncated = crate::str_utils::safe_truncate_str(&content, MAX_CONTEXT_BYTES as usize);
        return Ok(Some(truncated.to_string()));
    }
    Ok(Some(content))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn fresh_workspace(tag: &str) -> PathBuf {
        // Unique temp dir per test to avoid cross-test cache pollution.
        let dir = std::env::temp_dir().join(format!(
            "openfang_ctx_{}_{}",
            tag,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reread_picks_up_external_update() {
        let ws = fresh_workspace("reread");
        let path = ws.join(CONTEXT_FILENAME);

        fs::write(&path, "initial content A").unwrap();
        let first = load_context_md(&ws, false).unwrap();
        assert!(first.contains("initial content A"));

        // External writer updates the file (simulates the cron case from #843).
        {
            let mut f = fs::File::create(&path).unwrap();
            f.write_all(b"updated content B").unwrap();
        }

        let second = load_context_md(&ws, false).unwrap();
        assert!(second.contains("updated content B"));
        assert!(!second.contains("initial content A"));

        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn cache_context_true_freezes_first_read() {
        let ws = fresh_workspace("cache");
        let path = ws.join(CONTEXT_FILENAME);

        fs::write(&path, "frozen A").unwrap();
        let first = load_context_md(&ws, true).unwrap();
        assert!(first.contains("frozen A"));

        fs::write(&path, "never seen B").unwrap();
        let second = load_context_md(&ws, true).unwrap();
        assert_eq!(first, second);
        assert!(!second.contains("never seen B"));

        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn missing_file_returns_none() {
        let ws = fresh_workspace("missing");
        assert!(load_context_md(&ws, false).is_none());
        assert!(load_context_md(&ws, true).is_none());
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn read_failure_falls_back_to_cache() {
        let ws = fresh_workspace("fallback");
        let path = ws.join(CONTEXT_FILENAME);

        fs::write(&path, "cached payload").unwrap();
        let first = load_context_md(&ws, false).unwrap();
        assert!(first.contains("cached payload"));

        // Write bytes that are not valid UTF-8 so read_to_string returns an
        // IO error. This simulates a transient read failure while the cron
        // job is mid-rewrite.
        {
            let mut f = fs::File::create(&path).unwrap();
            f.write_all(&[0xff, 0xfe, 0xfd, 0x80, 0x81]).unwrap();
        }

        let second = load_context_md(&ws, false);
        assert_eq!(second.as_deref(), Some("cached payload"));

        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn empty_file_treated_as_absent() {
        let ws = fresh_workspace("empty");
        let path = ws.join(CONTEXT_FILENAME);
        fs::write(&path, "   \n\n  ").unwrap();
        assert!(load_context_md(&ws, false).is_none());
        let _ = fs::remove_dir_all(&ws);
    }
}
