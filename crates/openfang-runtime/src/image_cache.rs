//! Content-addressed image tmpfile cache.
//!
//! Decodes base64 image payloads (the `ContentBlock::Image` shape used by
//! all LLM drivers) and writes them to a content-addressed file under
//! `$HOME/.openfang/tmp/images/` so out-of-process consumers — initially
//! the Claude Code CLI's Read tool, soon the outbound Discord bridge —
//! can reach the bytes by path.
//!
//! Originally lived inside `drivers/claude_code.rs`; lifted here so the
//! outbound file-sharing path can reuse the same cache without a circular
//! dep on the driver crate. Behavior is byte-identical to the previous
//! private implementation.
//!
//! Properties:
//! - **Idempotent.** Filename is the first 64 bits of SHA-256(bytes), so
//!   re-rendering the same image hits the cache.
//! - **Atomic publish.** Bytes are written to a unique sibling tmpfile
//!   then `rename(2)`-d into place; readers never see a torn file.
//! - **Time-bounded.** A best-effort sweep on first call (per process)
//!   removes files older than [`IMAGE_TMP_TTL_SECS`].

use base64::Engine;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Once;
use tracing::{debug, info, warn};

/// TTL for materialized image tmpfiles (24 hours). Files older than this
/// are swept on first use.
pub const IMAGE_TMP_TTL_SECS: u64 = 24 * 60 * 60;

/// One-shot guard so the TTL sweep only fires once per process.
static IMAGE_TMP_SWEEP_ONCE: Once = Once::new();

/// Resolve the directory used for materializing image attachments.
///
/// Lives under `$HOME/.openfang/tmp/images` so it travels with the OpenFang
/// install. Falls back to the OS temp dir when `$HOME` isn't set (which
/// shouldn't happen in our deployed daemon but is handled defensively).
pub fn image_tmp_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        let mut p = PathBuf::from(home);
        p.push(".openfang");
        p.push("tmp");
        p.push("images");
        p
    } else {
        let mut p = std::env::temp_dir();
        p.push("openfang-images");
        p
    }
}

/// Map a MIME type to a sensible filename extension.
pub fn ext_for_mime(media_type: &str) -> &'static str {
    match media_type.to_ascii_lowercase().as_str() {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/heic" => "heic",
        "image/heif" => "heif",
        "image/bmp" => "bmp",
        "image/svg+xml" => "svg",
        _ => "bin",
    }
}

/// Decode the base64 image and write it to a content-addressed file under
/// `dir`. Idempotent: if a file with the same content hash already exists,
/// the existing path is returned without rewriting. Returns `None` on
/// decode or I/O failure (caller falls back to a textual placeholder).
///
/// `original_name`, if present, is sanitized and appended to the filename
/// after the content hash (`<hash16>__<sanitized>.<ext>`) so a human
/// browsing `~/.openfang/tmp/images/` can grep/eyeball-match files to the
/// inbound attachment they came from. Cache-hit lookup globs `<hash16>*.<ext>`
/// so a re-render with a new (or no) name reuses the existing file.
pub fn materialize_image(
    media_type: &str,
    data: &str,
    dir: &Path,
    original_name: Option<&str>,
) -> Option<PathBuf> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.as_bytes())
        .ok()?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let hash = hasher.finalize();
    let hex: String = hash.iter().take(16).map(|b| format!("{:02x}", b)).collect();
    let ext = ext_for_mime(media_type);

    // Cache-hit: if any file with this hash prefix already exists (with or
    // without a name suffix, regardless of which name it carries), reuse
    // it. Two callers feeding different names for the same bytes converge
    // on whichever named the file first; the alternative would be writing
    // multiple copies of identical bytes to disk for cosmetic reasons.
    if let Some(existing) = find_existing_for_hash(dir, &hex, ext) {
        if let Err(e) = touch_mtime(&existing) {
            debug!(path = ?existing, error = %e, "failed to refresh image tmpfile mtime");
        }
        return Some(existing);
    }

    let filename = match original_name.and_then(sanitize_for_filename) {
        Some(sanitized) => format!("{hex}__{sanitized}.{ext}"),
        None => format!("{hex}.{ext}"),
    };
    let path = dir.join(filename);
    // Defensive: post-sanitize collision check (should be subsumed by the
    // hash-prefix scan above, but kept so the legacy code path below is
    // still safe if `find_existing_for_hash` ever misses).
    if path.exists() {
        // Refresh mtime on cache hit so the TTL sweep (which gates on
        // `meta.modified()`) does not GC a tmpfile still being actively
        // referenced. Without this, a long-running conversation that
        // outlives `IMAGE_TMP_TTL_SECS` would lose its image bytes
        // mid-thread, even though the content block is still in scope.
        // Best-effort: any failure is debug-logged and the cached path
        // is returned anyway — the worst case is the legacy 24h-GC
        // behavior we just had.
        if let Err(e) = touch_mtime(&path) {
            debug!(path = ?path, error = %e, "failed to refresh image tmpfile mtime");
        }
        return Some(path);
    }
    if let Err(e) = std::fs::create_dir_all(dir) {
        warn!(dir = ?dir, error = %e, "failed to create openfang image tmp dir");
        return None;
    }
    // Atomic publish: write to a unique tmp sibling, then rename into place.
    // Two concurrent renders of the same image each write their own tmpfile;
    // the rename(2) is atomic on the same filesystem, so consumers never see
    // a torn or partially-written file. If the destination already exists by
    // the time we rename (loser of a race), the rename still succeeds (POSIX
    // replaces) — and the contents are identical anyway by construction.
    let tmp_path = dir.join(format!(
        "{hex}.{pid}.{nanos}.tmp",
        pid = std::process::id(),
        nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    ));
    if let Err(e) = std::fs::write(&tmp_path, &bytes) {
        warn!(path = ?tmp_path, error = %e, "failed to write openfang image tmpfile");
        return None;
    }
    if let Err(e) = std::fs::rename(&tmp_path, &path) {
        warn!(from = ?tmp_path, to = ?path, error = %e, "failed to rename openfang image tmpfile into place");
        // Best-effort cleanup of the orphan tmpfile.
        let _ = std::fs::remove_file(&tmp_path);
        return None;
    }
    Some(path)
}

/// Sanitize a candidate filename fragment so it is safe to embed in a
/// path under `image_tmp_dir()`. Lowercases ASCII, replaces anything
/// outside `[a-z0-9._-]` with `_`, collapses runs of `_`, strips leading
/// dots (no hidden files), drops the extension if present (the caller
/// supplies the canonical extension from MIME), and caps length at 60.
/// Returns `None` if the result would be empty.
pub fn sanitize_for_filename(name: &str) -> Option<String> {
    // Drop any path components defensively — Discord filenames shouldn't
    // contain `/`, but a malicious or malformed source could try to.
    let leaf = name.rsplit(['/', '\\']).next().unwrap_or(name);
    // Strip the trailing extension if any — we'll let the caller tack on
    // the canonical one from media_type. `foo.tar.gz` → `foo.tar`, which
    // is fine: the visual hint survives.
    let stem = match leaf.rsplit_once('.') {
        Some((s, _)) if !s.is_empty() => s,
        _ => leaf,
    };
    let mut out = String::with_capacity(stem.len());
    let mut last_underscore = false;
    for c in stem.chars() {
        let lc = c.to_ascii_lowercase();
        let keep = lc.is_ascii_alphanumeric() || matches!(lc, '.' | '-');
        if keep {
            out.push(lc);
            last_underscore = false;
        } else if !last_underscore {
            out.push('_');
            last_underscore = true;
        }
    }
    let trimmed = out.trim_matches(|c: char| c == '_' || c == '.').to_string();
    if trimmed.is_empty() {
        return None;
    }
    // Cap at 60 chars to keep total path length reasonable.
    let capped: String = trimmed.chars().take(60).collect();
    Some(capped)
}

/// Look for a previously-materialized tmpfile carrying the given content
/// hash, regardless of any human-readable name suffix that may have been
/// appended. Returns the first match found; in practice there is at most
/// one because the writer enforces uniqueness on collision via the rename
/// step. Best-effort: read errors return `None` and the caller falls
/// through to a fresh write.
fn find_existing_for_hash(dir: &Path, hex: &str, ext: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let dot_ext = format!(".{ext}");
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(&dot_ext) {
            continue;
        }
        // Match `<hex>.<ext>` or `<hex>__<anything>.<ext>`.
        let stem = name.trim_end_matches(&dot_ext);
        if stem == hex || stem.starts_with(&format!("{hex}__")) {
            return Some(path);
        }
    }
    None
}

/// Refresh the mtime of `path` to "now" so it survives the next TTL
/// sweep. Uses `File::set_modified`, which on Unix calls `futimens(2)`.
/// Opening read-only is sufficient — `futimens` does not require the fd
/// to be writable, only that the caller own the file (which we do, since
/// the daemon writes them).
fn touch_mtime(path: &Path) -> std::io::Result<()> {
    let f = std::fs::File::open(path)?;
    f.set_modified(std::time::SystemTime::now())
}

/// Delete image tmpfiles older than [`IMAGE_TMP_TTL_SECS`]. Best-effort:
/// any error is logged at debug and the sweep moves on.
pub fn sweep_old_image_tmpfiles(dir: &Path) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            debug!(dir = ?dir, error = %e, "image tmp sweep: read_dir failed (likely missing dir, fine)");
            return;
        }
    };
    let now = std::time::SystemTime::now();
    let ttl = std::time::Duration::from_secs(IMAGE_TMP_TTL_SECS);
    let mut removed = 0u32;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let Ok(modified) = meta.modified() else { continue };
        if let Ok(age) = now.duration_since(modified) {
            if age > ttl {
                if let Err(e) = std::fs::remove_file(&path) {
                    debug!(path = ?path, error = %e, "image tmp sweep: remove failed");
                } else {
                    removed += 1;
                }
            }
        }
    }
    if removed > 0 {
        info!(removed, "swept stale openfang image tmpfiles");
    }
}

/// Spawn the once-per-process TTL sweep in a background thread. Safe to
/// call from any number of driver inits — the [`Once`] guard ensures only
/// the first call does work.
pub fn spawn_sweep_once() {
    IMAGE_TMP_SWEEP_ONCE.call_once(|| {
        let dir = image_tmp_dir();
        std::thread::spawn(move || sweep_old_image_tmpfiles(&dir));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use std::time::{Duration, SystemTime};

    /// A 1×1 transparent PNG, base64-encoded. Tiny enough to keep tests fast.
    const TINY_PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=";

    #[test]
    fn materialize_image_refreshes_mtime_on_cache_hit() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        // First call materializes.
        let path = materialize_image("image/png", TINY_PNG_B64, dir, None)
            .expect("first materialization should succeed");
        assert!(path.exists());

        // Backdate mtime to ~25 hours ago — past IMAGE_TMP_TTL_SECS.
        let stale = SystemTime::now() - Duration::from_secs(IMAGE_TMP_TTL_SECS + 3600);
        let f = std::fs::File::open(&path).unwrap();
        f.set_modified(stale).unwrap();
        drop(f);
        let mtime_before = std::fs::metadata(&path).unwrap().modified().unwrap();

        // Second call should hit cache AND refresh mtime.
        let path2 = materialize_image("image/png", TINY_PNG_B64, dir, None)
            .expect("cache hit should return Some");
        assert_eq!(path, path2);
        let mtime_after = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert!(
            mtime_after > mtime_before,
            "mtime should be refreshed on cache hit (before={mtime_before:?}, after={mtime_after:?})"
        );

        // And the now-touched file must NOT be GC'd by a sweep that
        // would have caught the stale mtime.
        sweep_old_image_tmpfiles(dir);
        assert!(
            path.exists(),
            "refreshed tmpfile should survive the TTL sweep"
        );
    }

    #[test]
    fn sweep_removes_stale_tmpfiles() {
        // Sanity check that the sweep actually GCs old files — pairs with
        // the test above to prove the refresh is what saves the file.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let path = materialize_image("image/png", TINY_PNG_B64, dir, None).unwrap();

        let stale = SystemTime::now() - Duration::from_secs(IMAGE_TMP_TTL_SECS + 3600);
        let f = std::fs::File::open(&path).unwrap();
        f.set_modified(stale).unwrap();
        drop(f);

        sweep_old_image_tmpfiles(dir);
        assert!(!path.exists(), "stale tmpfile should have been swept");
    }

    #[test]
    fn ext_for_mime_known_and_unknown() {
        assert_eq!(ext_for_mime("image/png"), "png");
        assert_eq!(ext_for_mime("IMAGE/JPEG"), "jpg");
        assert_eq!(ext_for_mime("image/webp"), "webp");
        assert_eq!(ext_for_mime("application/octet-stream"), "bin");
    }

    #[test]
    fn materialize_image_rejects_invalid_base64() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(
            materialize_image("image/png", "!!!not-base64!!!", tmp.path(), None).is_none()
        );
    }

    #[test]
    fn materialize_image_appends_sanitized_name() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        let path =
            materialize_image("image/png", TINY_PNG_B64, dir, Some("My Vacation Photo.PNG"))
                .expect("first materialization");
        let name = path.file_name().unwrap().to_str().unwrap();
        assert!(
            name.contains("__my_vacation_photo.png"),
            "expected sanitized name suffix, got {name}"
        );
        let stem_hex: String = name.chars().take(16).collect();
        assert!(
            stem_hex.chars().all(|c| c.is_ascii_hexdigit()),
            "expected leading hex hash, got {stem_hex}"
        );
    }

    #[test]
    fn materialize_image_cache_hit_finds_named_file() {
        // Same bytes materialized first WITH a name, then again with no
        // name: the second call must reuse the named file rather than
        // writing a duplicate `<hash>.png`.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        let first =
            materialize_image("image/png", TINY_PNG_B64, dir, Some("hello.png")).unwrap();
        let second = materialize_image("image/png", TINY_PNG_B64, dir, None).unwrap();
        assert_eq!(first, second, "cache lookup should find the named file");

        let count = std::fs::read_dir(dir)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .ok()
                    .and_then(|e| e.metadata().ok())
                    .map(|m| m.is_file())
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(count, 1, "no duplicate tmpfile on cache hit");
    }

    #[test]
    fn sanitize_for_filename_basic_cases() {
        assert_eq!(
            sanitize_for_filename("Hello World.png").as_deref(),
            Some("hello_world")
        );
        assert_eq!(
            sanitize_for_filename("/etc/passwd").as_deref(),
            Some("passwd")
        );
        assert_eq!(
            sanitize_for_filename("foo___bar.txt").as_deref(),
            Some("foo_bar")
        );
        // All-punctuation/dot input → None.
        assert_eq!(sanitize_for_filename("...png").as_deref(), None);
        // Trailing extension is stripped.
        assert_eq!(
            sanitize_for_filename("smoke-test.pdf").as_deref(),
            Some("smoke-test")
        );
        // Length cap at 60.
        let long = "a".repeat(200);
        let result = sanitize_for_filename(&format!("{long}.png")).unwrap();
        assert_eq!(result.len(), 60);
        // Non-ASCII bytes collapse to a single `_`.
        let s = sanitize_for_filename("café.jpg").unwrap();
        assert!(s.starts_with("caf"), "got {s}");
    }

    // Force-reference base64 engine to keep imports tidy in case someone
    // refactors and the const is the only consumer.
    #[allow(dead_code)]
    fn _b64_compile_check() {
        let _ = base64::engine::general_purpose::STANDARD.decode(TINY_PNG_B64);
    }
}
