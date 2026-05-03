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
pub fn materialize_image(media_type: &str, data: &str, dir: &Path) -> Option<PathBuf> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.as_bytes())
        .ok()?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let hash = hasher.finalize();
    let hex: String = hash.iter().take(16).map(|b| format!("{:02x}", b)).collect();
    let filename = format!("{hex}.{ext}", ext = ext_for_mime(media_type));
    let path = dir.join(filename);
    if path.exists() {
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
