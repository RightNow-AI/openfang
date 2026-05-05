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
//! one of the allow-roots — by default `$HOME/.openfang/`. This covers the
//! ephemeral `~/.openfang/tmp/` scratch area and per-agent
//! `~/.openfang/workspaces/<agent>/` directories without leaking access to
//! the rest of the filesystem in the face of a prompt-injected agent.
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
    },
}

fn marker_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"<openfang:attach\s+([^>]*?)/>"#).expect("marker regex compiles")
    })
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

/// Default allow-root: canonicalised `$HOME/.openfang/`. Returns an empty
/// vec if `HOME` is unset or the directory does not exist (in which case
/// every directive will be rejected — fail-closed).
fn default_allow_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let mut p = PathBuf::from(home);
        p.push(".openfang");
        if let Ok(canon) = std::fs::canonicalize(&p) {
            roots.push(canon);
        }
    }
    roots
}

async fn resolve_directive(
    d: &AttachDirective,
    allow_roots: &[PathBuf],
) -> Result<ChannelContent, String> {
    let raw = PathBuf::from(&d.path);
    if !raw.is_absolute() {
        return Err(format!("path must be absolute: {}", d.path));
    }
    let canon = tokio::fs::canonicalize(&raw)
        .await
        .map_err(|e| format!("canonicalize {}: {e}", raw.display()))?;
    if !allow_roots.iter().any(|r| canon.starts_with(r)) {
        return Err(format!("path {} outside allow-roots", canon.display()));
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
    let data = tokio::fs::read(&canon)
        .await
        .map_err(|e| format!("read {}: {e}", canon.display()))?;
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
/// `allow_roots_override` (or the default `$HOME/.openfang/` root if
/// `None`), and return either `NoMarkers` or `WithAttachments`.
///
/// The returned `stripped_text` is the original with markers removed and
/// `caption` attribute values appended (each on its own line, in
/// document order). The caller is responsible for running the channel
/// formatter over `stripped_text` — formatting *before* parsing would
/// HTML-escape `<` in markers and break detection.
pub async fn parse(text: &str, allow_roots_override: Option<&[PathBuf]>) -> Parsed {
    let re = marker_regex();
    if !re.is_match(text) {
        return Parsed::NoMarkers;
    }
    let owned_default;
    let allow_roots: &[PathBuf] = match allow_roots_override {
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
    for d in &directives {
        match resolve_directive(d, allow_roots).await {
            Ok(block) => files.push(block),
            Err(e) => {
                warn!("outbound_attach: skipping {}: {}", d.path, e);
            }
        }
    }

    Parsed::WithAttachments {
        stripped_text,
        files,
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

    #[tokio::test]
    async fn no_markers_returns_no_markers() {
        let result = parse("just some prose, no markers here", None).await;
        assert!(matches!(result, Parsed::NoMarkers));
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

        let result = parse(&text, Some(&roots)).await;
        match result {
            Parsed::WithAttachments {
                stripped_text,
                files,
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

        let result = parse(&text, Some(&roots)).await;
        match result {
            Parsed::WithAttachments {
                stripped_text,
                files,
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

        let result = parse(&text, Some(&roots)).await;
        match result {
            Parsed::WithAttachments { files, .. } => {
                match &files[0] {
                    ChannelContent::FileData { filename, .. } => {
                        assert_eq!(filename, "SPOILER_secret.png");
                    }
                    _ => panic!("expected FileData"),
                }
            }
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

        let result = parse(&text, Some(&roots)).await;
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
        let result = parse(&text, Some(&roots)).await;
        match result {
            Parsed::WithAttachments {
                stripped_text,
                files,
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
    async fn relative_path_is_rejected() {
        let (_keep, roots) = fixture_root();
        let result = parse(
            "<openfang:attach path=\"relative/path.txt\"/>",
            Some(&roots),
        )
        .await;
        match result {
            Parsed::WithAttachments { files, .. } => {
                assert!(files.is_empty(), "relative path must be rejected");
            }
            _ => panic!("expected WithAttachments"),
        }
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

        let result = parse(&text, Some(&roots)).await;
        match result {
            Parsed::WithAttachments {
                stripped_text,
                files,
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
            None,
        )
        .await;
        match result {
            Parsed::WithAttachments {
                stripped_text,
                files,
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
        assert_eq!(mime_from_extension(Path::new("x.unknown")), "application/octet-stream");
        assert_eq!(mime_from_extension(Path::new("noext")), "application/octet-stream");
    }
}
