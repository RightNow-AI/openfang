//! Outbound attachment parser.
//!
//! Recognises `<openfang:attach path="…" [name="…"] [spoiler="true"]
//! [caption="…"]/>` markers in agent response text, validates each path
//! against an allow-root, reads the bytes, and produces
//! `ChannelContent::FileData` blocks that the wire layer (`discord::send`,
//! `telegram::send`, …) already knows how to chunk and upload.
//!
//! ## Marker syntax
//!
//! ```text
//! <openfang:attach path="/abs/path/to/report.pdf"/>
//! <openfang:attach path="/abs/path.png" caption="for the meeting"/>
//! <openfang:attach path="/abs/x.zip" name="renamed.zip" spoiler="true"/>
//! ```
//!
//! All attribute values use double quotes. The marker is self-closing.
//! Multiple markers per response are supported up to Discord's 10-attachment
//! per-message cap; the wire-layer chunker handles aggregate-size splitting.
//!
//! ## Security
//!
//! Paths are canonicalised (so symlinks are resolved) and must lie under
//! one of the caller-supplied allow-roots. The default allow-roots are
//! **empty** — callers MUST pass their per-agent `workspace_root`
//! explicitly via [`ParseOptions::allow_roots`]. Bridge and kernel both
//! compute `workspace_root = ~/.openfang/workspaces/<agent>/` from the
//! agent identity and pass it through.
//!
//! ### Why relative paths require an explicit `base`
//!
//! `tokio::fs::canonicalize` resolves relative paths against the
//! **process CWD** — whatever launchd handed the daemon at spawn (likely
//! `/` or `~/.openfang`, definitely *not* the calling agent's workspace).
//! If we naively accepted relative paths, a directive like
//! `path="../etc/passwd"` would resolve against ambient process state and
//! could land inside an `allow_roots` entry purely by accident of how the
//! daemon was started. That's the same wide-default failure mode that
//! motivated `default_allow_roots() -> Vec::new()`, one layer down.
//!
//! Invariant: **path-resolution context must be explicit, never inherited
//! from ambient process state.** Relative directive paths resolve against
//! the caller-supplied [`ParseOptions::base`] only; if no base was
//! provided the directive is rejected. The resolved absolute path is
//! still subject to canonicalisation, the `allow_roots` `starts_with`
//! check, and the hard-deny rule below — so `base` is a *resolution*
//! input, not an *authorisation* input.
//!
//! A secondary hard-deny rule rejects any canonical path under
//! `$HOME/.openfang/` that is not also under `$HOME/.openfang/workspaces/`,
//! regardless of caller-supplied allow-roots. Belt-and-suspenders against a
//! caller that mistakenly opens too wide a root: secrets, daemon state,
//! channel configs, and other agents' workspaces remain unreachable even
//! if `allow_roots` accidentally includes `~/.openfang/` itself.
//!
//! ## Failure mode
//!
//! Per-directive errors (path missing, outside allow-root, oversized) are
//! logged at WARN and the marker is silently dropped from the outgoing
//! message — partial success rather than failing the whole reply. If every
//! directive fails the caller still gets the stripped text back, so the
//! user sees the prose without the broken markers.

use crate::types::ChannelContent;
use regex_lite::Regex;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tracing::warn;

/// Per-attachment hard cap. Discord allows 25 MiB per request on the free
/// tier; we cap each file at 25 MiB and rely on the wire-layer chunker
/// (24 MiB aggregate, 10 attachments per chunk in `discord::send`) to split
/// large multi-file responses across several messages.
const MAX_FILE_BYTES: u64 = 25 * 1024 * 1024;

/// Hard cap on directives parsed from a single response. Discord refuses
/// more than 10 attachments per message; the chunker bucket-splits but
/// there's no point parsing further.
const MAX_ATTACHMENTS_PER_MESSAGE: usize = 10;

/// Outcome of parsing an outbound response.
pub enum Parsed {
    /// No `<openfang:attach .../>` marker present. Caller should take the
    /// normal text-only path.
    NoMarkers,
    /// At least one marker was found. `stripped_text` is the original text
    /// with all markers removed and any `caption=` values appended. `files`
    /// is the resolved `FileData` blocks (possibly empty if every directive
    /// failed validation).
    WithAttachments {
        stripped_text: String,
        files: Vec<ChannelContent>,
        /// Attachments whose directive parsed but whose resolution failed
        /// (missing file, outside allow-roots, oversized, …). Each tuple
        /// is `(directive_path, reason)`. The message body still sends;
        /// callers that surface tool-call results back to the agent
        /// should include this list so the agent can react to silent
        /// drops. Empty when every directive resolved.
        skipped: Vec<(String, String)>,
    },
}

/// Options controlling outbound-attachment parsing and path resolution.
///
/// Grouped into a struct so callers can extend the resolution context
/// without churning every call site. Today there are two knobs:
///
/// - [`allow_roots`](Self::allow_roots): canonical roots a resolved
///   absolute path must lie under. `None` defers to
///   [`default_allow_roots`] (which is empty — fail-closed).
/// - [`base`](Self::base): explicit base directory used to resolve
///   *relative* directive paths. `None` rejects all relative paths; we
///   never fall back to process CWD (see the module-level "Why relative
///   paths require an explicit base" note).
///
/// The two fields are independent: `allow_roots` governs authorisation,
/// `base` governs resolution. Bridge currently passes the same
/// `workspace_root` for both, but that's a caller convention — the parser
/// treats them separately.
#[derive(Clone, Copy, Debug, Default)]
pub struct ParseOptions<'a> {
    /// Override for the per-agent allow-roots. `None` uses
    /// [`default_allow_roots`] (empty / fail-closed).
    pub allow_roots: Option<&'a [PathBuf]>,
    /// Explicit base directory for resolving relative directive paths.
    /// `None` rejects all relative paths (no CWD fallback).
    pub base: Option<&'a Path>,
}

fn marker_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"<openfang:attach\s+([^>]*?)/>"#).expect("marker regex compiles")
    })
}

/// Neutralize any `<openfang:attach …/>` marker *opener* in agent-controlled
/// text so [`parse`] cannot interpret it.
///
/// Use this when composing operator-facing text (e.g. an approval prompt) out
/// of fields an agent controls. Without it, an agent could embed a marker so
/// that `parse` strips it and appends its `caption=` value — making the text a
/// human approves diverge from the real content it is supposed to represent.
///
/// HTML-escapes the leading `<` of the opener (the same transform this module's
/// `parse` doc notes will "break detection"), so the marker is no longer parsed
/// but stays visible and faithful to the human. Case-insensitive for defense in
/// depth even though [`marker_regex`] is currently case-sensitive.
pub fn neutralize_markers(text: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r#"(?i)<(openfang:attach)"#).expect("neutralize regex compiles")
    });
    re.replace_all(text, "&lt;$1").into_owned()
}

fn attr_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(\w+)\s*=\s*"([^"]*)""#).expect("attr regex compiles"))
}

#[derive(Debug)]
struct AttachDirective {
    path: String,
    name: Option<String>,
    spoiler: bool,
    caption: Option<String>,
}

fn parse_directive(attrs: &str) -> Option<AttachDirective> {
    let mut path = None;
    let mut name = None;
    let mut spoiler = false;
    let mut caption = None;
    for cap in attr_regex().captures_iter(attrs) {
        let key = cap.get(1)?.as_str();
        let val = cap.get(2)?.as_str().to_string();
        match key {
            "path" => path = Some(val),
            "name" => name = Some(val),
            "spoiler" => spoiler = matches!(val.as_str(), "true" | "1" | "yes"),
            "caption" => caption = Some(val),
            _ => {}
        }
    }
    Some(AttachDirective {
        path: path?,
        name,
        spoiler,
        caption,
    })
}

/// Extension → MIME type. Mirrors the table used by `tool_runner` for
/// `channel_send`'s `file_path` parameter so inbound and outbound paths
/// agree on the wire-format. Unknown extensions fall back to
/// `application/octet-stream`.
fn mime_from_extension(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "txt" | "md" | "log" => "text/plain",
        "csv" => "text/csv",
        "json" => "application/json",
        "xml" => "application/xml",
        "zip" => "application/zip",
        "gz" | "gzip" => "application/gzip",
        "tar" => "application/x-tar",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        _ => "application/octet-stream",
    }
}

/// Default allow-roots: **empty**. Callers MUST pass `workspace_root`
/// explicitly (via `Some(&[…])`) to grant any access. A caller that forgets
/// to plumb its per-agent workspace receives zero allow-roots and every
/// directive is rejected — fail-closed against context loss.
///
/// Historically this returned `$HOME/.openfang/` as a default, which gave a
/// caller-forgetting-to-plumb-context the union of every agent's workspace
/// *plus* the daemon's secrets directory. The wide default is the bug; the
/// `workspace_root` plumbing in `bridge.rs::send_agent_response` and the
/// kernel proactive-send path is the fix.
fn default_allow_roots() -> Vec<PathBuf> {
    Vec::new()
}

/// Canonicalised `$HOME/.openfang/` if it exists. Used by the hard-deny rule
/// in [`resolve_directive`] to reject any path inside OpenFang's home that
/// is not also inside `~/.openfang/workspaces/`, regardless of what
/// `allow_roots` the caller passed. Belt-and-suspenders against a caller
/// that mistakenly opens up too wide an allow-root.
fn openfang_home() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let mut p = PathBuf::from(home);
    p.push(".openfang");
    std::fs::canonicalize(&p).ok()
}

/// Returns `true` if `canon` is inside `home` (= `~/.openfang/`) but not
/// under `home/workspaces/`. The hard-deny rule keyed by this predicate
/// rejects access to OpenFang-internal state (secrets, daemon files,
/// other agents' workspaces' siblings) regardless of caller-supplied
/// allow-roots. Pure path logic — no I/O — so trivially testable.
fn is_hard_denied(canon: &Path, home: &Path) -> bool {
    if !canon.starts_with(home) {
        return false;
    }
    let workspaces = home.join("workspaces");
    !canon.starts_with(&workspaces)
}

async fn resolve_directive(
    d: &AttachDirective,
    allow_roots: &[PathBuf],
    base: Option<&Path>,
) -> Result<ChannelContent, String> {
    let raw = PathBuf::from(&d.path);
    // Relative paths are resolved against the caller-supplied `base`
    // only — never against process CWD. See module docs for the
    // threat model.
    let raw = if raw.is_absolute() {
        raw
    } else {
        match base {
            Some(b) => b.join(&raw),
            None => {
                return Err(format!(
                    "relative path {} requires explicit base (no CWD fallback)",
                    d.path
                ));
            }
        }
    };
    let canon = tokio::fs::canonicalize(&raw)
        .await
        .map_err(|e| format!("canonicalize {}: {e}", raw.display()))?;
    if !allow_roots.iter().any(|r| canon.starts_with(r)) {
        return Err(format!("path {} outside allow-roots", canon.display()));
    }
    // Hard-deny anything under ~/.openfang/ that isn't a per-agent workspace,
    // even if the caller's allow_roots somehow included a parent directory.
    // The only OpenFang-internal area we expose to outbound attachments is
    // `~/.openfang/workspaces/<agent>/` — secrets, daemon state, channel
    // configs, and other agents' workspaces remain unreachable regardless of
    // caller misconfiguration.
    if let Some(home) = openfang_home() {
        if is_hard_denied(&canon, &home) {
            return Err(format!(
                "path {} is inside ~/.openfang/ but outside workspaces/ (hard-deny)",
                canon.display()
            ));
        }
    }
    let metadata = tokio::fs::metadata(&canon)
        .await
        .map_err(|e| format!("stat {}: {e}", canon.display()))?;
    if !metadata.is_file() {
        return Err(format!("not a regular file: {}", canon.display()));
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(format!(
            "{} exceeds {} byte cap (size {})",
            canon.display(),
            MAX_FILE_BYTES,
            metadata.len()
        ));
    }
    // Bounded read closes the TOCTOU window between the metadata size
    // check above and the actual read. Even if an attacker (or a concurrent
    // writer) swaps or grows the file after stat, tokio's File + take()
    // caps the bytes copied into memory at MAX_FILE_BYTES, so we cannot be
    // tricked into loading an unbounded payload. We use `MAX_FILE_BYTES + 1`
    // as the take-limit so we can distinguish "file fit exactly" from
    // "file grew past the cap" and return a clear error in the latter case.
    let data = {
        use tokio::io::AsyncReadExt;
        let f = tokio::fs::File::open(&canon)
            .await
            .map_err(|e| format!("open {}: {e}", canon.display()))?;
        let cap_hint = std::cmp::min(metadata.len(), MAX_FILE_BYTES) as usize;
        let mut buf = Vec::with_capacity(cap_hint);
        f.take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut buf)
            .await
            .map_err(|e| format!("read {}: {e}", canon.display()))?;
        if buf.len() as u64 > MAX_FILE_BYTES {
            return Err(format!(
                "{} grew past {} byte cap during read (read {} bytes)",
                canon.display(),
                MAX_FILE_BYTES,
                buf.len()
            ));
        }
        buf
    };
    let basename = canon
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string();
    let mut filename = d.name.clone().unwrap_or(basename);
    if d.spoiler && !filename.starts_with("SPOILER_") {
        // Discord's `SPOILER_` filename prefix flags the attachment as a
        // spoiler. Other adapters ignore the prefix harmlessly.
        filename = format!("SPOILER_{}", filename);
    }
    let mime_type = mime_from_extension(&canon).to_string();
    Ok(ChannelContent::FileData {
        data,
        filename,
        mime_type,
    })
}

/// Parse `text`, resolve every `<openfang:attach .../>` marker against
/// `opts.allow_roots` (or the default empty allow-roots if `None`), and
/// return either `NoMarkers` or `WithAttachments`.
///
/// Relative directive paths are resolved against `opts.base`. If `base`
/// is `None`, relative paths are rejected — process CWD is never used as
/// a fallback. See the module-level "Why relative paths require an
/// explicit base" note.
///
/// The returned `stripped_text` is the original with markers removed and
/// `caption` attribute values appended (each on its own line, in
/// document order). The caller is responsible for running the channel
/// formatter over `stripped_text` — formatting *before* parsing would
/// HTML-escape `<` in markers and break detection.
pub async fn parse(text: &str, opts: ParseOptions<'_>) -> Parsed {
    let re = marker_regex();
    if !re.is_match(text) {
        return Parsed::NoMarkers;
    }
    let owned_default;
    let allow_roots: &[PathBuf] = match opts.allow_roots {
        Some(r) => r,
        None => {
            owned_default = default_allow_roots();
            &owned_default
        }
    };

    let mut stripped = String::with_capacity(text.len());
    let mut last = 0;
    let mut directives: Vec<AttachDirective> = Vec::new();
    let mut captions: Vec<String> = Vec::new();

    for cap in re.captures_iter(text) {
        let m = cap.get(0).unwrap();
        let attrs = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        stripped.push_str(&text[last..m.start()]);
        match parse_directive(attrs) {
            Some(d) => {
                if directives.len() >= MAX_ATTACHMENTS_PER_MESSAGE {
                    warn!(
                        "outbound_attach: dropping marker beyond {} attachments cap",
                        MAX_ATTACHMENTS_PER_MESSAGE
                    );
                    // Keep the marker visible — the agent should see it
                    // wasn't honoured.
                    stripped.push_str(m.as_str());
                } else {
                    if let Some(c) = &d.caption {
                        captions.push(c.clone());
                    }
                    directives.push(d);
                }
            }
            None => {
                // Malformed marker — leave it in place for debuggability.
                stripped.push_str(m.as_str());
            }
        }
        last = m.end();
    }
    stripped.push_str(&text[last..]);

    // Append captions on their own lines.
    let mut stripped_text = stripped.trim_end().to_string();
    for c in &captions {
        if !stripped_text.is_empty() {
            stripped_text.push('\n');
        }
        stripped_text.push_str(c);
    }

    let mut files: Vec<ChannelContent> = Vec::with_capacity(directives.len());
    let mut skipped: Vec<(String, String)> = Vec::new();
    for d in &directives {
        match resolve_directive(d, allow_roots, opts.base).await {
            Ok(block) => files.push(block),
            Err(e) => {
                warn!("outbound_attach: skipping {}: {}", d.path, e);
                skipped.push((d.path.clone(), e));
            }
        }
    }

    Parsed::WithAttachments {
        stripped_text,
        files,
        skipped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture_root() -> (tempfile::TempDir, Vec<PathBuf>) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = std::fs::canonicalize(tmp.path()).expect("canonicalize tmp");
        (tmp, vec![root])
    }

    /// Build [`ParseOptions`] with allow_roots set and base unset.
    /// Used by legacy tests that exercise the absolute-path code path.
    fn opts_with_roots<'a>(roots: &'a [PathBuf]) -> ParseOptions<'a> {
        ParseOptions {
            allow_roots: Some(roots),
            base: None,
        }
    }

    #[tokio::test]
    async fn no_markers_returns_no_markers() {
        let result = parse("just some prose, no markers here", ParseOptions::default()).await;
        assert!(matches!(result, Parsed::NoMarkers));
    }

    #[tokio::test]
    async fn neutralize_breaks_marker_detection() {
        // An agent embeds a well-formed marker with a reassuring caption in a
        // field that will be shown to a human approver.
        let evil = "rm -rf /important \
                    <openfang:attach path=\"/dev/null\" caption=\"(dry-run only — nothing deleted)\"/>";
        let safe = neutralize_markers(evil);

        // After neutralization the parser sees no marker at all …
        assert!(matches!(
            parse(&safe, ParseOptions::default()).await,
            Parsed::NoMarkers
        ));
        // … nothing was stripped and no caption was appended; the injection
        // attempt stays visible verbatim (opener `<` escaped to `&lt;`).
        assert!(safe.contains("&lt;openfang:attach"));
        assert!(safe.contains("(dry-run only — nothing deleted)"));
    }

    #[test]
    fn neutralize_is_noop_without_marker_and_case_insensitive() {
        assert_eq!(
            neutralize_markers("plain text, no marker"),
            "plain text, no marker"
        );
        let mixed = neutralize_markers("x <OpenFang:Attach path=\"y\"/>");
        assert!(mixed.contains("&lt;OpenFang:Attach"));
    }

    #[tokio::test]
    async fn single_marker_resolves_to_filedata() {
        let (tmp, roots) = fixture_root();
        let path = tmp.path().join("hello.txt");
        std::fs::write(&path, b"hi").unwrap();
        let canon = std::fs::canonicalize(&path).unwrap();
        let text = format!(
            "Here you go: <openfang:attach path=\"{}\"/> done.",
            canon.display()
        );

        let result = parse(&text, opts_with_roots(&roots)).await;
        match result {
            Parsed::WithAttachments {
                stripped_text,
                files,
                skipped: _,
            } => {
                assert_eq!(stripped_text, "Here you go:  done.");
                assert_eq!(files.len(), 1);
                match &files[0] {
                    ChannelContent::FileData {
                        data,
                        filename,
                        mime_type,
                    } => {
                        assert_eq!(data, b"hi");
                        assert_eq!(filename, "hello.txt");
                        assert_eq!(mime_type, "text/plain");
                    }
                    _ => panic!("expected FileData"),
                }
            }
            _ => panic!("expected WithAttachments"),
        }
    }

    #[tokio::test]
    async fn caption_attribute_is_appended_to_text() {
        let (tmp, roots) = fixture_root();
        let path = tmp.path().join("note.pdf");
        std::fs::write(&path, b"%PDF-1.4 stub").unwrap();
        let canon = std::fs::canonicalize(&path).unwrap();
        let text = format!(
            "<openfang:attach path=\"{}\" caption=\"for the meeting\"/>",
            canon.display()
        );

        let result = parse(&text, opts_with_roots(&roots)).await;
        match result {
            Parsed::WithAttachments {
                stripped_text,
                files,
                skipped: _,
            } => {
                assert_eq!(stripped_text, "for the meeting");
                assert_eq!(files.len(), 1);
                match &files[0] {
                    ChannelContent::FileData {
                        filename,
                        mime_type,
                        ..
                    } => {
                        assert_eq!(filename, "note.pdf");
                        assert_eq!(mime_type, "application/pdf");
                    }
                    _ => panic!("expected FileData"),
                }
            }
            _ => panic!("expected WithAttachments"),
        }
    }

    #[tokio::test]
    async fn spoiler_prefixes_filename() {
        let (tmp, roots) = fixture_root();
        let path = tmp.path().join("secret.png");
        std::fs::write(&path, b"\x89PNG").unwrap();
        let canon = std::fs::canonicalize(&path).unwrap();
        let text = format!(
            "<openfang:attach path=\"{}\" spoiler=\"true\"/>",
            canon.display()
        );

        let result = parse(&text, opts_with_roots(&roots)).await;
        match result {
            Parsed::WithAttachments { files, .. } => match &files[0] {
                ChannelContent::FileData { filename, .. } => {
                    assert_eq!(filename, "SPOILER_secret.png");
                }
                _ => panic!("expected FileData"),
            },
            _ => panic!("expected WithAttachments"),
        }
    }

    #[tokio::test]
    async fn name_attribute_overrides_basename() {
        let (tmp, roots) = fixture_root();
        let path = tmp.path().join("ugly-uuid-name.pdf");
        std::fs::write(&path, b"%PDF").unwrap();
        let canon = std::fs::canonicalize(&path).unwrap();
        let text = format!(
            "<openfang:attach path=\"{}\" name=\"report.pdf\"/>",
            canon.display()
        );

        let result = parse(&text, opts_with_roots(&roots)).await;
        match result {
            Parsed::WithAttachments { files, .. } => match &files[0] {
                ChannelContent::FileData { filename, .. } => {
                    assert_eq!(filename, "report.pdf");
                }
                _ => panic!("expected FileData"),
            },
            _ => panic!("expected WithAttachments"),
        }
    }

    #[tokio::test]
    async fn path_outside_allow_root_is_rejected() {
        // Use a path in /tmp that we know exists but isn't under our
        // synthetic allow-root.
        let (_keep, roots) = fixture_root();
        let outside = std::env::temp_dir().join("openfang-outbound-attach-outside.txt");
        std::fs::write(&outside, b"x").unwrap();
        let canon = std::fs::canonicalize(&outside).unwrap();

        // Sanity: outside isn't under our fixture root.
        assert!(!canon.starts_with(&roots[0]));

        let text = format!("<openfang:attach path=\"{}\"/>", canon.display());
        let result = parse(&text, opts_with_roots(&roots)).await;
        match result {
            Parsed::WithAttachments {
                stripped_text,
                files,
                skipped: _,
            } => {
                assert_eq!(stripped_text, "");
                assert!(
                    files.is_empty(),
                    "directive outside allow-root must be dropped"
                );
            }
            _ => panic!("expected WithAttachments (with empty files)"),
        }
        let _ = std::fs::remove_file(&outside);
    }

    #[tokio::test]
    async fn relative_path_without_base_is_rejected() {
        // base=None ⇒ relative paths cannot resolve. Even with an
        // allow-root set, the directive is dropped before any CWD
        // fallback could kick in.
        let (_keep, roots) = fixture_root();
        let result = parse(
            "<openfang:attach path=\"relative/path.txt\"/>",
            opts_with_roots(&roots),
        )
        .await;
        match result {
            Parsed::WithAttachments { files, .. } => {
                assert!(
                    files.is_empty(),
                    "relative path with no base must be rejected"
                );
            }
            _ => panic!("expected WithAttachments"),
        }
    }

    #[tokio::test]
    async fn relative_path_resolves_against_explicit_base() {
        // base=Some(workspace) ⇒ relative path joins under the base,
        // canonicalises, passes the allow_roots check, and resolves.
        let (tmp, roots) = fixture_root();
        let base = roots[0].clone();
        let path = tmp.path().join("inside.txt");
        std::fs::write(&path, b"yo").unwrap();

        let text = "<openfang:attach path=\"inside.txt\"/>";
        let opts = ParseOptions {
            allow_roots: Some(&roots),
            base: Some(&base),
        };
        let result = parse(text, opts).await;
        match result {
            Parsed::WithAttachments { files, .. } => {
                assert_eq!(files.len(), 1, "relative path with base must resolve");
                match &files[0] {
                    ChannelContent::FileData { data, filename, .. } => {
                        assert_eq!(data, b"yo");
                        assert_eq!(filename, "inside.txt");
                    }
                    _ => panic!("expected FileData"),
                }
            }
            _ => panic!("expected WithAttachments"),
        }
    }

    #[tokio::test]
    async fn relative_path_with_dotdot_escape_is_rejected() {
        // base.join("../escape.txt") canonicalises out of the workspace.
        // The allow_roots `starts_with` check then rejects it. This proves
        // the base is a *resolution* input, not an *authorisation* input —
        // canonicalize + allow_roots still catch traversal.
        let (_tmp, roots) = fixture_root();
        let base = roots[0].clone();
        // Create an "escape" file OUTSIDE the workspace.
        let outside = std::env::temp_dir().join("openfang-outbound-attach-dotdot-escape.txt");
        std::fs::write(&outside, b"nope").unwrap();
        let outside_canon = std::fs::canonicalize(&outside).unwrap();
        // Build a relative path from `base` to `outside_canon`. We don't
        // know the depth of `base` ahead of time, so walk up from base
        // until we share a prefix with `outside_canon`'s parent.
        let mut up = PathBuf::new();
        let mut cursor: &Path = &base;
        while !outside_canon.starts_with(cursor) {
            up.push("..");
            cursor = match cursor.parent() {
                Some(p) => p,
                None => break,
            };
        }
        // Suffix after the shared prefix.
        let suffix = outside_canon.strip_prefix(cursor).unwrap_or(&outside_canon);
        let rel = up.join(suffix);
        let rel_str = rel.to_string_lossy().to_string();

        let text = format!("<openfang:attach path=\"{}\"/>", rel_str);
        let opts = ParseOptions {
            allow_roots: Some(&roots),
            base: Some(&base),
        };
        let result = parse(&text, opts).await;
        match result {
            Parsed::WithAttachments { files, .. } => {
                assert!(
                    files.is_empty(),
                    "relative `..` escape must be caught by allow_roots after canonicalize"
                );
            }
            _ => panic!("expected WithAttachments"),
        }
        let _ = std::fs::remove_file(&outside);
    }

    #[tokio::test]
    async fn symlink_escape_inside_workspace_is_rejected() {
        // A symlink inside the workspace pointing outside it must be
        // rejected after canonicalize follows the link and the
        // allow_roots check sees the target's location.
        use std::os::unix::fs::symlink;
        let (tmp, roots) = fixture_root();
        let base = roots[0].clone();

        // Target lives outside the workspace.
        let target = std::env::temp_dir().join("openfang-outbound-attach-symlink-target.txt");
        std::fs::write(&target, b"escape").unwrap();
        let target_canon = std::fs::canonicalize(&target).unwrap();
        // Sanity: target is outside our allow-root.
        assert!(!target_canon.starts_with(&base));

        // Symlink inside the workspace pointing at the outside target.
        let link = tmp.path().join("escape_link.txt");
        symlink(&target_canon, &link).expect("symlink");

        let text = "<openfang:attach path=\"escape_link.txt\"/>";
        let opts = ParseOptions {
            allow_roots: Some(&roots),
            base: Some(&base),
        };
        let result = parse(text, opts).await;
        match result {
            Parsed::WithAttachments { files, .. } => {
                assert!(
                    files.is_empty(),
                    "symlink escaping the workspace must be rejected"
                );
            }
            _ => panic!("expected WithAttachments"),
        }
        let _ = std::fs::remove_file(&target);
    }

    #[tokio::test]
    async fn multiple_markers_are_all_resolved() {
        let (tmp, roots) = fixture_root();
        let p1 = tmp.path().join("a.txt");
        let p2 = tmp.path().join("b.txt");
        std::fs::write(&p1, b"a").unwrap();
        std::fs::write(&p2, b"b").unwrap();
        let c1 = std::fs::canonicalize(&p1).unwrap();
        let c2 = std::fs::canonicalize(&p2).unwrap();
        let text = format!(
            "first <openfang:attach path=\"{}\"/> then <openfang:attach path=\"{}\"/> end",
            c1.display(),
            c2.display()
        );

        let result = parse(&text, opts_with_roots(&roots)).await;
        match result {
            Parsed::WithAttachments {
                stripped_text,
                files,
                skipped: _,
            } => {
                assert_eq!(stripped_text, "first  then  end");
                assert_eq!(files.len(), 2);
            }
            _ => panic!("expected WithAttachments"),
        }
    }

    #[tokio::test]
    async fn malformed_marker_left_in_place() {
        // No `path=` attribute → directive is invalid.
        let result = parse(
            "before <openfang:attach foo=\"bar\"/> after",
            ParseOptions::default(),
        )
        .await;
        match result {
            Parsed::WithAttachments {
                stripped_text,
                files,
                skipped: _,
            } => {
                assert!(files.is_empty());
                assert!(
                    stripped_text.contains("<openfang:attach foo=\"bar\"/>"),
                    "malformed marker should be preserved verbatim"
                );
            }
            _ => panic!("expected WithAttachments (with malformed marker preserved)"),
        }
    }

    #[test]
    fn mime_table_covers_common_extensions() {
        assert_eq!(mime_from_extension(Path::new("x.pdf")), "application/pdf");
        assert_eq!(mime_from_extension(Path::new("x.PNG")), "image/png");
        assert_eq!(
            mime_from_extension(Path::new("x.unknown")),
            "application/octet-stream"
        );
        assert_eq!(
            mime_from_extension(Path::new("noext")),
            "application/octet-stream"
        );
    }

    #[test]
    fn default_allow_roots_is_empty() {
        // Fail-closed: callers MUST plumb their per-agent workspace root.
        // A caller that forgets and passes None receives no allow-roots
        // and every directive is rejected.
        assert!(default_allow_roots().is_empty());
    }

    #[tokio::test]
    async fn parse_with_default_opts_drops_all_attachments() {
        // Default ParseOptions has allow_roots=None (→ empty) and
        // base=None. Even an absolute path pointing at an existing file
        // is rejected because no allow-root contains it. Text portion
        // still comes through.
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("oops.txt");
        std::fs::write(&path, b"x").unwrap();
        let canon = std::fs::canonicalize(&path).unwrap();
        let text = format!(
            "before <openfang:attach path=\"{}\"/> after",
            canon.display()
        );
        let result = parse(&text, ParseOptions::default()).await;
        match result {
            Parsed::WithAttachments {
                stripped_text,
                files,
                skipped: _,
            } => {
                assert_eq!(stripped_text, "before  after");
                assert!(
                    files.is_empty(),
                    "default allow-roots is empty; attachment must be rejected"
                );
            }
            _ => panic!("expected WithAttachments (with empty files)"),
        }
    }

    #[test]
    fn hard_deny_inside_openfang_outside_workspaces() {
        // Synthetic paths — no I/O, pure policy check.
        let home = Path::new("/Users/x/.openfang");
        assert!(is_hard_denied(
            Path::new("/Users/x/.openfang/secrets/discord.toml"),
            home
        ));
        assert!(is_hard_denied(
            Path::new("/Users/x/.openfang/daemon/cron.db"),
            home
        ));
        assert!(is_hard_denied(Path::new("/Users/x/.openfang/tmp/x"), home));
    }

    #[test]
    fn hard_deny_inside_workspaces_is_allowed() {
        // Paths under ~/.openfang/workspaces/ pass the hard-deny rule;
        // the per-agent allow-roots check (in resolve_directive) further
        // narrows to a single agent.
        let home = Path::new("/Users/x/.openfang");
        assert!(!is_hard_denied(
            Path::new("/Users/x/.openfang/workspaces/debra/audio.wav"),
            home
        ));
        assert!(!is_hard_denied(
            Path::new("/Users/x/.openfang/workspaces/coder-openfang/note.md"),
            home
        ));
    }

    #[test]
    fn hard_deny_outside_openfang_is_not_triggered() {
        // The hard-deny rule only fires inside ~/.openfang/. Other
        // locations are governed solely by the caller-supplied
        // allow-roots check.
        let home = Path::new("/Users/x/.openfang");
        assert!(!is_hard_denied(Path::new("/tmp/x.txt"), home));
        assert!(!is_hard_denied(Path::new("/Users/x/Documents/x.pdf"), home));
        assert!(!is_hard_denied(Path::new("/etc/passwd"), home));
    }

    #[tokio::test]
    async fn parse_with_empty_explicit_roots_drops_attachments() {
        // Caller explicitly passes Some(&[]) — same fail-closed outcome
        // as None, but exercises the override branch.
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("a.txt");
        std::fs::write(&path, b"x").unwrap();
        let canon = std::fs::canonicalize(&path).unwrap();
        let text = format!("<openfang:attach path=\"{}\"/>", canon.display());
        let empty: &[PathBuf] = &[];
        let opts = ParseOptions {
            allow_roots: Some(empty),
            base: None,
        };
        let result = parse(&text, opts).await;
        match result {
            Parsed::WithAttachments { files, .. } => assert!(files.is_empty()),
            _ => panic!("expected WithAttachments"),
        }
    }
    #[tokio::test]
    async fn bounded_read_round_trips_multi_kib_file() {
        // Regression guard for the TOCTOU bounded-read refactor: confirm a
        // file larger than the default Vec capacity hint still reads to
        // completion when its size is well under MAX_FILE_BYTES. If the
        // take() limit were misconfigured (e.g. capped at metadata.len()
        // instead of the hard ceiling) a file that grew slightly between
        // stat and read would be truncated silently — this test pins the
        // happy path so the cap-vs-truncate distinction stays visible in
        // coverage.
        let (tmp, roots) = fixture_root();
        let path = tmp.path().join("big.bin");
        let payload: Vec<u8> = (0..(256u32 * 1024)).map(|i| (i % 251) as u8).collect();
        std::fs::write(&path, &payload).unwrap();
        let canon = std::fs::canonicalize(&path).unwrap();
        let text = format!("<openfang:attach path=\"{}\"/>", canon.display());

        let result = parse(&text, opts_with_roots(&roots)).await;
        match result {
            Parsed::WithAttachments { files, .. } => {
                assert_eq!(files.len(), 1);
                match &files[0] {
                    ChannelContent::FileData { data, .. } => {
                        assert_eq!(data.len(), payload.len());
                        assert_eq!(data, &payload);
                    }
                    _ => panic!("expected FileData"),
                }
            }
            _ => panic!("expected WithAttachments"),
        }
    }

    #[tokio::test]
    async fn missing_file_populates_skipped() {
        // A directive whose path doesn't exist must populate `skipped`
        // with (path, reason) so callers can surface it to the agent.
        // The message still parses successfully and the text portion
        // comes through — partial success is intentional (see module
        // docs); the skipped vec is the channel for telling the caller
        // *what* got dropped.
        let (_tmp, roots) = fixture_root();
        let base = roots[0].clone();
        let text = "before <openfang:attach path=\"does-not-exist.txt\"/> after";
        let opts = ParseOptions {
            allow_roots: Some(&roots),
            base: Some(&base),
        };
        match parse(text, opts).await {
            Parsed::WithAttachments {
                stripped_text,
                files,
                skipped,
            } => {
                assert_eq!(stripped_text, "before  after");
                assert!(files.is_empty());
                assert_eq!(skipped.len(), 1);
                assert_eq!(skipped[0].0, "does-not-exist.txt");
                assert!(
                    skipped[0].1.contains("canonicalize"),
                    "expected canonicalize error, got: {}",
                    skipped[0].1
                );
            }
            _ => panic!("expected WithAttachments"),
        }
    }

    #[tokio::test]
    async fn resolved_attachment_leaves_skipped_empty() {
        // Happy path: a resolvable directive yields an empty `skipped`
        // vec. Pins the invariant that `skipped` is the *failure*
        // channel, not a per-directive audit log.
        let (tmp, roots) = fixture_root();
        let path = tmp.path().join("ok.txt");
        std::fs::write(&path, b"ok").unwrap();
        let canon = std::fs::canonicalize(&path).unwrap();
        let text = format!("<openfang:attach path=\"{}\"/>", canon.display());
        match parse(&text, opts_with_roots(&roots)).await {
            Parsed::WithAttachments { files, skipped, .. } => {
                assert_eq!(files.len(), 1);
                assert!(
                    skipped.is_empty(),
                    "happy path must not populate skipped, got: {:?}",
                    skipped
                );
            }
            _ => panic!("expected WithAttachments"),
        }
    }
}
