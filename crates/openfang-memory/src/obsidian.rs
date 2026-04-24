//! Obsidian vault backend for external memory.
//!
//! Reads: walks the vault root for `.md` files and substring-matches the query
//! against their content. Returns matches as `MemoryFragment`s with any YAML
//! frontmatter parsed into the fragment's metadata.
//!
//! Writes: appends a new `.md` under `{vault_root}/{inbox_subdir}/` with a
//! YAML frontmatter carrying `agent_id`, `confidence`, `source_url`, and
//! `untrusted`. **Writes never escape the inbox subdir** — the target path is
//! canonicalised and verified to live underneath the canonicalised inbox dir
//! before any bytes hit disk.
//!
//! Phase 5 (agentic triage pipeline) will also index a configured
//! `CyberIntel/` subdir for the cyber-agent's retrieval context; that hook
//! is not wired in this commit.
//!
//! Criticality defaults to [`Criticality::Degraded`] — a broken vault should
//! surface on health but must not block boot. Operators can override via the
//! constructor when/if the vault becomes load-bearing.

use crate::external::{BackendHealth, Criticality, ExternalMemoryBackend, Provenance};
use async_trait::async_trait;
use chrono::Utc;
use openfang_types::agent::AgentId;
use openfang_types::memory::{MemoryFragment, MemoryId, MemorySource};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::debug;
use walkdir::WalkDir;

/// Max files scanned per search call. Bounded so a vault with millions of
/// notes doesn't freeze the agent loop.
const MAX_FILES_SCANNED: usize = 2_000;

/// Max bytes read from a single note during search. Oversized notes are
/// skipped rather than truncated-and-searched — truncation can create false
/// negatives that mislead the caller.
const MAX_NOTE_BYTES: u64 = 64 * 1024;

/// Extension set recognised as Markdown. Obsidian allows a few variants.
const MARKDOWN_EXTS: &[&str] = &["md", "markdown", "mdown"];

/// Obsidian frontmatter shape the vault backend emits on writes and parses on
/// reads. All fields optional on parse — a foreign note with any frontmatter
/// layout is accepted, we only pull keys we recognise.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct ObsidianFrontmatter {
    agent_id: Option<String>,
    confidence: Option<f32>,
    source_url: Option<String>,
    untrusted: Option<bool>,
    source: Option<String>,
    scope: Option<String>,
}

/// The Obsidian vault backend.
pub struct ObsidianVaultBackend {
    name: &'static str,
    criticality: Criticality,
    vault_root: PathBuf,
    inbox_subdir: PathBuf,
}

impl ObsidianVaultBackend {
    /// Construct a new backend rooted at `vault_root`. The inbox subdirectory
    /// is created lazily on first write.
    pub fn new(vault_root: PathBuf, inbox_subdir: impl Into<PathBuf>) -> Self {
        let inbox_subdir = inbox_subdir.into();
        let full_inbox = vault_root.join(&inbox_subdir);
        Self {
            name: "obsidian",
            criticality: Criticality::Degraded,
            vault_root,
            inbox_subdir: full_inbox,
        }
    }

    /// Override the default Degraded criticality (e.g. some deployments may
    /// want Critical).
    pub fn with_criticality(mut self, criticality: Criticality) -> Self {
        self.criticality = criticality;
        self
    }

    /// Canonicalised inbox path (or unresolved PathBuf if the dir doesn't
    /// exist yet — used only inside safe-path validation after creation).
    fn inbox_path(&self) -> &Path {
        &self.inbox_subdir
    }
}

#[async_trait]
impl ExternalMemoryBackend for ObsidianVaultBackend {
    fn name(&self) -> &str {
        self.name
    }

    fn criticality(&self) -> Criticality {
        self.criticality
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<MemoryFragment>, String> {
        let query = query.trim();
        if query.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        if !self.vault_root.exists() {
            return Err(format!(
                "vault root does not exist: {}",
                self.vault_root.display()
            ));
        }

        let query_lower = query.to_ascii_lowercase();
        let mut hits: Vec<MemoryFragment> = Vec::new();
        let mut scanned = 0usize;

        for entry in WalkDir::new(&self.vault_root)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if hits.len() >= limit {
                break;
            }
            if scanned >= MAX_FILES_SCANNED {
                debug!(
                    scanned,
                    "obsidian search hit MAX_FILES_SCANNED cap — results may be partial"
                );
                break;
            }

            let path = entry.path();
            if !entry.file_type().is_file() || !is_markdown_path(path) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.len() > MAX_NOTE_BYTES {
                continue;
            }
            scanned += 1;

            let raw = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    debug!(path = %path.display(), error = %e, "skipping unreadable note");
                    continue;
                }
            };
            if !raw.to_ascii_lowercase().contains(&query_lower) {
                continue;
            }

            let (fm, body) = split_frontmatter(&raw);
            hits.push(frag_from_note(path, &fm, body));
        }

        Ok(hits)
    }

    async fn write(
        &self,
        fragment: &MemoryFragment,
        provenance: &Provenance,
    ) -> Result<(), String> {
        let inbox = self.inbox_path().to_path_buf();
        std::fs::create_dir_all(&inbox)
            .map_err(|e| format!("mkdir -p {}: {e}", inbox.display()))?;

        let inbox_canon = std::fs::canonicalize(&inbox)
            .map_err(|e| format!("canonicalize inbox {}: {e}", inbox.display()))?;

        let slug = sanitise_slug(&fragment.content);
        let date = Utc::now().format("%Y%m%d-%H%M%S").to_string();
        let filename = format!("{date}-{slug}.md");
        let target = inbox_canon.join(&filename);

        // Defence: the final target's parent must still be the canonical inbox.
        // Prevents any future change to `sanitise_slug` from accidentally
        // allowing traversal characters.
        let parent = target
            .parent()
            .ok_or_else(|| "target has no parent dir".to_string())?;
        if parent != inbox_canon {
            return Err(format!(
                "refusing write: target parent {} escapes inbox {}",
                parent.display(),
                inbox_canon.display()
            ));
        }

        let body = render_note(fragment, provenance);
        std::fs::write(&target, body)
            .map_err(|e| format!("write {}: {e}", target.display()))?;
        Ok(())
    }

    async fn health(&self) -> BackendHealth {
        if !self.vault_root.exists() {
            return BackendHealth::Failed(format!(
                "vault root does not exist: {}",
                self.vault_root.display()
            ));
        }
        match std::fs::metadata(&self.vault_root) {
            Ok(m) if m.is_dir() => {
                // Writable check: try to create the inbox. Non-fatal if
                // inbox already exists; fatal if we can't create it.
                let inbox = self.inbox_path();
                if let Err(e) = std::fs::create_dir_all(inbox) {
                    return BackendHealth::Failed(format!(
                        "inbox mkdir failed {}: {e}",
                        inbox.display()
                    ));
                }
                BackendHealth::Ok
            }
            Ok(_) => BackendHealth::Failed(format!(
                "vault root is not a directory: {}",
                self.vault_root.display()
            )),
            Err(e) => BackendHealth::Failed(format!(
                "stat {} failed: {e}",
                self.vault_root.display()
            )),
        }
    }
}

fn is_markdown_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let lower = e.to_ascii_lowercase();
            MARKDOWN_EXTS.contains(&lower.as_str())
        })
        .unwrap_or(false)
}

/// Split a note into `(frontmatter, body)`. Unknown-shape frontmatter is
/// returned as-is; we hand the result to `serde_yaml` and tolerate a parse
/// failure by returning `ObsidianFrontmatter::default()`.
fn split_frontmatter(raw: &str) -> (ObsidianFrontmatter, &str) {
    let after_open = if let Some(rest) = raw.strip_prefix("---\r\n") {
        rest
    } else if let Some(rest) = raw.strip_prefix("---\n") {
        rest
    } else {
        return (ObsidianFrontmatter::default(), raw);
    };
    let Some(end_rel) = find_closing_delim(after_open) else {
        return (ObsidianFrontmatter::default(), raw);
    };
    let yaml = &after_open[..end_rel];
    let body_start = end_rel + closing_delim_len(&after_open[end_rel..]);
    let body = &after_open[body_start..];
    let fm = serde_yaml::from_str::<ObsidianFrontmatter>(yaml).unwrap_or_default();
    (fm, body)
}

fn find_closing_delim(s: &str) -> Option<usize> {
    let mut pos = 0usize;
    for line in s.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == "---" {
            return Some(pos);
        }
        pos += line.len();
    }
    None
}

fn closing_delim_len(s: &str) -> usize {
    if let Some(rest) = s.strip_prefix("---") {
        if let Some(after) = rest.strip_prefix("\r\n") {
            return s.len() - after.len();
        }
        if let Some(after) = rest.strip_prefix('\n') {
            return s.len() - after.len();
        }
        return 3;
    }
    0
}

fn frag_from_note(path: &Path, fm: &ObsidianFrontmatter, body: &str) -> MemoryFragment {
    let mut metadata: HashMap<String, serde_json::Value> = HashMap::new();
    metadata.insert(
        "obsidian_path".into(),
        serde_json::Value::String(path.display().to_string()),
    );
    if let Some(u) = &fm.source_url {
        metadata.insert("source_url".into(), serde_json::Value::String(u.clone()));
    }
    if let Some(u) = fm.untrusted {
        metadata.insert("untrusted".into(), serde_json::Value::Bool(u));
    }
    if let Some(s) = &fm.source {
        metadata.insert("source_label".into(), serde_json::Value::String(s.clone()));
    }

    let agent_id = fm
        .agent_id
        .as_deref()
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
        .map(AgentId)
        .unwrap_or_default();

    MemoryFragment {
        id: MemoryId::new(),
        agent_id,
        content: body.trim().to_string(),
        embedding: None,
        metadata,
        source: MemorySource::Document,
        confidence: fm.confidence.unwrap_or(0.7).clamp(0.0, 1.0),
        created_at: Utc::now(),
        accessed_at: Utc::now(),
        access_count: 0,
        scope: fm.scope.clone().unwrap_or_else(|| "obsidian".to_string()),
    }
}

fn render_note(fragment: &MemoryFragment, provenance: &Provenance) -> String {
    let fm = ObsidianFrontmatter {
        agent_id: Some(fragment.agent_id.0.to_string()),
        confidence: Some(fragment.confidence),
        source_url: provenance.source_url.clone(),
        untrusted: Some(provenance.untrusted),
        source: if provenance.source.is_empty() {
            None
        } else {
            Some(provenance.source.clone())
        },
        scope: if fragment.scope.is_empty() {
            None
        } else {
            Some(fragment.scope.clone())
        },
    };
    let yaml = serde_yaml::to_string(&fm).unwrap_or_default();
    format!("---\n{yaml}---\n{}\n", fragment.content.trim_end())
}

/// Produce a filesystem-safe slug from a fragment's content.
///
/// Rules:
/// - lowercase ASCII alphanumerics + hyphens only
/// - collapses runs of invalid chars to single hyphen
/// - max 60 chars
/// - empty input → `"note"`
/// - prevents path traversal: no `..`, no `/`, no `\`, no control chars
fn sanitise_slug(input: &str) -> String {
    let mut out = String::with_capacity(60);
    let mut last_was_hyphen = false;
    for c in input.chars().take(200) {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_was_hyphen = false;
        } else if !last_was_hyphen {
            out.push('-');
            last_was_hyphen = true;
        }
        if out.len() >= 60 {
            break;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "note".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fresh_vault(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "openfang_vault_{}_{}",
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

    fn sample_fragment(content: &str) -> MemoryFragment {
        MemoryFragment {
            id: MemoryId::new(),
            agent_id: AgentId::new(),
            content: content.to_string(),
            embedding: None,
            metadata: HashMap::new(),
            source: MemorySource::UserProvided,
            confidence: 0.9,
            created_at: Utc::now(),
            accessed_at: Utc::now(),
            access_count: 0,
            scope: "test".to_string(),
        }
    }

    #[tokio::test]
    async fn health_ok_on_fresh_vault() {
        let vault = fresh_vault("health_ok");
        let b = ObsidianVaultBackend::new(vault.clone(), "OpenFang/inbox");
        assert_eq!(b.health().await, BackendHealth::Ok);
        let _ = fs::remove_dir_all(&vault);
    }

    #[tokio::test]
    async fn health_failed_when_vault_missing() {
        let b = ObsidianVaultBackend::new(
            std::env::temp_dir().join("does_not_exist_openfang_vault_xyz"),
            "OpenFang/inbox",
        );
        match b.health().await {
            BackendHealth::Failed(_) => {}
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn write_creates_file_in_inbox_with_frontmatter() {
        let vault = fresh_vault("write_frontmatter");
        let b = ObsidianVaultBackend::new(vault.clone(), "OpenFang/inbox");
        let frag = sample_fragment("hello world, this is a note");

        b.write(
            &frag,
            &Provenance {
                source: "scrape".into(),
                untrusted: true,
                source_url: Some("https://example.com".into()),
                scan_results: None,
            },
        )
        .await
        .unwrap();

        let inbox = vault.join("OpenFang/inbox");
        let entries: Vec<_> = fs::read_dir(&inbox).unwrap().collect();
        assert_eq!(entries.len(), 1);
        let file = entries[0].as_ref().unwrap().path();
        let content = fs::read_to_string(&file).unwrap();
        assert!(content.starts_with("---\n"));
        assert!(content.contains("agent_id:"));
        assert!(content.contains("untrusted: true"));
        assert!(content.contains("source_url: https://example.com"));
        assert!(content.contains("hello world"));
        // Slug format check
        let name = file.file_name().unwrap().to_str().unwrap();
        assert!(name.ends_with(".md"));
        assert!(name.contains("hello-world"));
        let _ = fs::remove_dir_all(&vault);
    }

    #[tokio::test]
    async fn write_sanitises_slug_and_prevents_traversal() {
        let vault = fresh_vault("slug_traversal");
        let b = ObsidianVaultBackend::new(vault.clone(), "OpenFang/inbox");

        // Content including path-traversal sequences — must never escape inbox.
        let frag = sample_fragment("../../../etc/passwd\nrm -rf /");
        b.write(&frag, &Provenance::default()).await.unwrap();

        let inbox = vault.join("OpenFang/inbox");
        let entries: Vec<PathBuf> = fs::read_dir(&inbox)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        assert_eq!(entries.len(), 1);
        // Filename must not contain slashes, dots (other than .md), or other
        // suspicious characters.
        let name = entries[0].file_name().unwrap().to_str().unwrap();
        assert!(!name.contains('/'));
        assert!(!name.contains(".."));
        assert!(!name.contains('\\'));
        let _ = fs::remove_dir_all(&vault);
    }

    #[tokio::test]
    async fn search_finds_substring_match_and_parses_frontmatter() {
        let vault = fresh_vault("search_hit");
        fs::create_dir_all(vault.join("notes")).unwrap();
        fs::write(
            vault.join("notes/one.md"),
            "---\n\
             source_url: https://ex.com/one\n\
             confidence: 0.8\n\
             untrusted: true\n\
             ---\n\
             Quarterly earnings release details for Acme Corp.",
        )
        .unwrap();
        fs::write(
            vault.join("notes/two.md"),
            "Nothing related here, just a grocery list.",
        )
        .unwrap();

        let b = ObsidianVaultBackend::new(vault.clone(), "OpenFang/inbox");
        let hits = b.search("earnings", 10).await.unwrap();

        assert_eq!(hits.len(), 1);
        let h = &hits[0];
        assert!(h.content.contains("Quarterly earnings release"));
        assert_eq!(h.confidence, 0.8);
        assert_eq!(
            h.metadata
                .get("source_url")
                .and_then(|v| v.as_str()),
            Some("https://ex.com/one")
        );
        assert_eq!(
            h.metadata.get("untrusted").and_then(|v| v.as_bool()),
            Some(true)
        );
        let _ = fs::remove_dir_all(&vault);
    }

    #[tokio::test]
    async fn search_respects_limit() {
        let vault = fresh_vault("search_limit");
        fs::create_dir_all(vault.join("notes")).unwrap();
        for i in 0..5 {
            fs::write(
                vault.join(format!("notes/n{i}.md")),
                format!("note {i} about earnings"),
            )
            .unwrap();
        }
        let b = ObsidianVaultBackend::new(vault.clone(), "OpenFang/inbox");
        let hits = b.search("earnings", 2).await.unwrap();
        assert_eq!(hits.len(), 2);
        let _ = fs::remove_dir_all(&vault);
    }

    #[tokio::test]
    async fn search_empty_query_returns_empty() {
        let vault = fresh_vault("search_empty");
        fs::write(vault.join("n.md"), "anything").unwrap();
        let b = ObsidianVaultBackend::new(vault.clone(), "OpenFang/inbox");
        assert!(b.search("", 10).await.unwrap().is_empty());
        assert!(b.search("   ", 10).await.unwrap().is_empty());
        let _ = fs::remove_dir_all(&vault);
    }

    #[tokio::test]
    async fn search_skips_oversized_notes() {
        let vault = fresh_vault("search_oversized");
        let big = "earnings ".repeat(100_000); // ~900KB
        fs::write(vault.join("big.md"), big).unwrap();
        fs::write(vault.join("small.md"), "earnings").unwrap();
        let b = ObsidianVaultBackend::new(vault.clone(), "OpenFang/inbox");
        let hits = b.search("earnings", 10).await.unwrap();
        // Oversized note is skipped; small one is found.
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "earnings");
        let _ = fs::remove_dir_all(&vault);
    }

    #[tokio::test]
    async fn search_ignores_non_markdown_files() {
        let vault = fresh_vault("search_non_md");
        fs::write(vault.join("secret.txt"), "earnings").unwrap();
        fs::write(vault.join("note.md"), "earnings").unwrap();
        let b = ObsidianVaultBackend::new(vault.clone(), "OpenFang/inbox");
        let hits = b.search("earnings", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0]
            .metadata
            .get("obsidian_path")
            .and_then(|v| v.as_str())
            .unwrap()
            .ends_with("note.md"));
        let _ = fs::remove_dir_all(&vault);
    }

    #[tokio::test]
    async fn write_then_search_round_trip() {
        let vault = fresh_vault("roundtrip");
        let b = ObsidianVaultBackend::new(vault.clone(), "OpenFang/inbox");
        let frag = sample_fragment("uniquephrase foo bar");
        b.write(&frag, &Provenance::default()).await.unwrap();

        let hits = b.search("uniquephrase", 5).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].content.contains("uniquephrase"));
        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn sanitise_slug_basic() {
        assert_eq!(sanitise_slug("Hello World!"), "hello-world");
        assert_eq!(sanitise_slug("  leading & trailing "), "leading-trailing");
        assert_eq!(sanitise_slug("..//etc/passwd"), "etc-passwd");
        assert_eq!(sanitise_slug(""), "note");
        assert_eq!(sanitise_slug("!!!"), "note");
        assert!(sanitise_slug(&"x".repeat(500)).len() <= 60);
    }

    #[test]
    fn split_frontmatter_handles_absent() {
        let (fm, body) = split_frontmatter("no frontmatter here");
        assert!(fm.source_url.is_none());
        assert_eq!(body, "no frontmatter here");
    }

    #[test]
    fn split_frontmatter_tolerates_invalid_yaml() {
        let (fm, body) = split_frontmatter("---\nnot: [valid\n---\nbody");
        assert!(fm.source_url.is_none());
        assert_eq!(body, "body");
    }
}
