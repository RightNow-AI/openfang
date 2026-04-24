//! Pinboard — non-blocking review surface for `Suspicious` / `ScanFailed`
//! triage outcomes.
//!
//! When the upstream classifier returns `Verdict::Safe`, the orchestrator
//! releases the content to memory directly. When it returns `Malicious` it
//! lands in the quarantine dir and is never re-fed (unless an operator
//! explicitly toggles `security.pinboard_malicious`). Everything in between
//! — `Suspicious`, `Questionable`, `ScanFailed` — pins onto this board:
//!
//! * Persisted to `<base>/<id>/{decision.json, body.bin, source.txt}` so a
//!   crashed daemon doesn't lose context.
//! * No agent pauses; the agent loop has already moved on. Pinboard review
//!   is a parallel lane.
//! * Listable / inspectable / decidable from the API and CLI (P5.4
//!   plumbing will land in a follow-up commit; the primitives here are
//!   the storage layer + state machine).
//! * Each entry can be mirrored to the Obsidian vault under
//!   `OpenFang/pinboard/` so the operator can review in their normal
//!   knowledge-management workflow.

use super::classifier::ClassifierDecision;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Default pinboard root inside the OpenFang data directory.
pub const PINBOARD_DIRNAME: &str = "pinboard";

/// State of a pinboard entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinboardState {
    /// Awaiting human or follow-up cyber-agent decision.
    Pending,
    /// An operator (or re-classifier) cleared this for memory release.
    Allowed,
    /// An operator (or re-classifier) sealed this in permanent quarantine.
    Quarantined,
}

/// Action that mutates pinboard state — recorded in the entry's action log
/// for audit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinboardAction {
    /// Mark as allowed.
    Allow,
    /// Mark as quarantined.
    Quarantine,
    /// Annotate with a comment without changing state.
    Comment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinboardEvent {
    pub at: DateTime<Utc>,
    pub actor: String,
    pub action: PinboardAction,
    pub note: String,
}

/// One quarantined incident on the pinboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinboardEntry {
    pub id: String,
    pub state: PinboardState,
    pub created_at: DateTime<Utc>,
    pub source: String,
    pub content_summary: String,
    pub decision: ClassifierDecision,
    /// Append-only audit log.
    pub events: Vec<PinboardEvent>,
}

impl PinboardEntry {
    pub fn new(source: String, content_summary: String, decision: ClassifierDecision) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            state: PinboardState::Pending,
            created_at: Utc::now(),
            source,
            content_summary,
            decision,
            events: Vec::new(),
        }
    }
}

/// Errors emitted by the pinboard storage layer.
#[derive(Debug)]
pub enum PinboardError {
    Io(io::Error),
    NotFound(String),
    Corrupt(String),
    InvalidId(String),
    InvalidTransition {
        from: PinboardState,
        to: PinboardAction,
    },
}

impl std::fmt::Display for PinboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "pinboard io: {e}"),
            Self::NotFound(id) => write!(f, "pinboard entry not found: {id}"),
            Self::Corrupt(m) => write!(f, "pinboard corrupt: {m}"),
            Self::InvalidId(id) => write!(f, "invalid pinboard id: {id:?}"),
            Self::InvalidTransition { from, to } => {
                write!(f, "invalid pinboard transition: {from:?} → {to:?}")
            }
        }
    }
}

impl std::error::Error for PinboardError {}

impl From<io::Error> for PinboardError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Filesystem-backed pinboard storage.
pub struct PinboardStore {
    root: PathBuf,
}

impl PinboardStore {
    /// Create a store rooted at `root`. The directory is created lazily on
    /// first write.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Persist a new entry. Returns the freshly-assigned id.
    pub fn submit(
        &self,
        source: &str,
        content_summary: &str,
        decision: ClassifierDecision,
        body: &[u8],
    ) -> Result<PinboardEntry, PinboardError> {
        let entry = PinboardEntry::new(source.to_string(), content_summary.to_string(), decision);
        self.write_entry(&entry, Some(body))?;
        Ok(entry)
    }

    /// List entries in deterministic order: pending first (oldest first),
    /// then resolved (newest first).
    pub fn list(&self) -> Result<Vec<PinboardEntry>, PinboardError> {
        let mut entries: Vec<PinboardEntry> = Vec::new();
        let dirs = match fs::read_dir(&self.root) {
            Ok(d) => d,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(entries),
            Err(e) => return Err(e.into()),
        };
        for d in dirs.flatten() {
            let path = d.path();
            if !path.is_dir() {
                continue;
            }
            let id = match path.file_name().and_then(|n| n.to_str()) {
                Some(s) if is_safe_id(s) => s.to_string(),
                _ => continue,
            };
            match self.read_entry(&id) {
                Ok(e) => entries.push(e),
                Err(PinboardError::Corrupt(m)) => {
                    tracing::warn!(id = %id, error = %m, "skipping corrupt pinboard entry");
                }
                Err(_) => continue,
            }
        }
        entries.sort_by(|a, b| {
            let ord = sort_key_state(a.state).cmp(&sort_key_state(b.state));
            if ord != std::cmp::Ordering::Equal {
                return ord;
            }
            // Same state: pending oldest-first; resolved newest-first.
            if a.state == PinboardState::Pending {
                a.created_at.cmp(&b.created_at)
            } else {
                b.created_at.cmp(&a.created_at)
            }
        });
        Ok(entries)
    }

    /// Read a single entry by id.
    pub fn get(&self, id: &str) -> Result<PinboardEntry, PinboardError> {
        self.read_entry(id)
    }

    /// Apply a decision to an entry. Returns the updated entry.
    /// Transitions:
    ///   Pending     → Allowed | Quarantined (Allow / Quarantine)
    ///   Pending     → Pending (Comment — no state change)
    ///   Allowed     → Allowed (Comment only — no further state change)
    ///   Quarantined → Quarantined (Comment only — terminal)
    pub fn decide(
        &self,
        id: &str,
        actor: &str,
        action: PinboardAction,
        note: &str,
    ) -> Result<PinboardEntry, PinboardError> {
        let mut entry = self.read_entry(id)?;
        let new_state = match (entry.state, action) {
            (PinboardState::Pending, PinboardAction::Allow) => PinboardState::Allowed,
            (PinboardState::Pending, PinboardAction::Quarantine) => PinboardState::Quarantined,
            (PinboardState::Pending, PinboardAction::Comment) => PinboardState::Pending,
            (PinboardState::Allowed, PinboardAction::Comment) => PinboardState::Allowed,
            (PinboardState::Quarantined, PinboardAction::Comment) => PinboardState::Quarantined,
            (from, to) => return Err(PinboardError::InvalidTransition { from, to }),
        };
        entry.state = new_state;
        entry.events.push(PinboardEvent {
            at: Utc::now(),
            actor: actor.to_string(),
            action,
            note: note.to_string(),
        });
        self.write_entry(&entry, None)?;
        Ok(entry)
    }

    /// Render an entry as a single Markdown document the operator reads in
    /// Obsidian. Caller writes this under `<vault>/OpenFang/pinboard/<id>.md`.
    pub fn render_for_obsidian(&self, entry: &PinboardEntry) -> String {
        let actions: String = entry
            .events
            .iter()
            .map(|e| {
                format!(
                    "- {} — **{}** by `{}`: {}",
                    e.at.to_rfc3339(),
                    serde_json::to_string(&e.action).unwrap_or_else(|_| "?".into()),
                    e.actor,
                    if e.note.is_empty() {
                        "(no note)".to_string()
                    } else {
                        e.note.clone()
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let scanners: String = entry
            .decision
            .scan_outcomes
            .iter()
            .map(|o| {
                format!(
                    "- `{}` → **{:?}** | findings: {}",
                    o.scanner,
                    o.verdict,
                    if o.findings.is_empty() {
                        "(none)".to_string()
                    } else {
                        o.findings.join(", ")
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            "---\n\
             id: {id}\n\
             state: {state}\n\
             created_at: {created}\n\
             source: {source}\n\
             classifier_verdict: {verdict:?}\n\
             classifier_confidence: {conf}\n\
             recommended_action: {action}\n\
             ---\n\
             # Pinboard entry {id}\n\
             \n\
             ## Cyber-agent rationale\n\
             {rationale}\n\
             \n\
             ## Content summary\n\
             {summary}\n\
             \n\
             ## Scanner outcomes\n\
             {scanners}\n\
             \n\
             ## Audit log\n\
             {actions}\n",
            id = entry.id,
            state = serde_json::to_string(&entry.state)
                .unwrap_or_else(|_| "?".into())
                .trim_matches('"'),
            created = entry.created_at.to_rfc3339(),
            source = entry.source,
            verdict = entry.decision.verdict,
            conf = entry.decision.confidence,
            action = entry.decision.recommended_action,
            rationale = entry.decision.rationale,
            summary = entry.content_summary,
            scanners = if scanners.is_empty() {
                "(none)".to_string()
            } else {
                scanners
            },
            actions = if actions.is_empty() {
                "(no events)".to_string()
            } else {
                actions
            },
        )
    }

    // ---- internals ----------------------------------------------------

    fn entry_dir(&self, id: &str) -> Result<PathBuf, PinboardError> {
        if !is_safe_id(id) {
            return Err(PinboardError::InvalidId(id.to_string()));
        }
        Ok(self.root.join(id))
    }

    fn read_entry(&self, id: &str) -> Result<PinboardEntry, PinboardError> {
        let dir = self.entry_dir(id)?;
        let decision_path = dir.join("decision.json");
        if !decision_path.exists() {
            return Err(PinboardError::NotFound(id.to_string()));
        }
        let raw = fs::read_to_string(&decision_path)?;
        serde_json::from_str(&raw).map_err(|e| PinboardError::Corrupt(e.to_string()))
    }

    fn write_entry(
        &self,
        entry: &PinboardEntry,
        body: Option<&[u8]>,
    ) -> Result<(), PinboardError> {
        let dir = self.entry_dir(&entry.id)?;
        fs::create_dir_all(&dir)?;
        let json = serde_json::to_string_pretty(entry)
            .map_err(|e| PinboardError::Corrupt(e.to_string()))?;
        fs::write(dir.join("decision.json"), json)?;
        if let Some(b) = body {
            fs::write(dir.join("body.bin"), b)?;
        }
        fs::write(dir.join("source.txt"), &entry.source)?;
        Ok(())
    }
}

fn sort_key_state(s: PinboardState) -> u8 {
    match s {
        PinboardState::Pending => 0,
        PinboardState::Allowed => 1,
        PinboardState::Quarantined => 2,
    }
}

/// Defensive check used by every path-deriving helper: only accept ids that
/// look like UUIDs. Prevents traversal via `../` or absolute paths even
/// though we always join under `root`.
fn is_safe_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::triage::{ScanOutcome, Verdict};
    use std::env;

    fn fresh_root(tag: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!(
            "openfang_pinboard_{}_{}",
            tag,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn sample_decision(verdict: Verdict) -> ClassifierDecision {
        ClassifierDecision {
            verdict,
            rationale: "test rationale".into(),
            recommended_action: "pinboard".into(),
            confidence: 0.5,
            scan_outcomes: vec![ScanOutcome {
                scanner: "heuristic".into(),
                verdict: Verdict::Suspicious,
                findings: vec!["jailbreak.dan_mode".into()],
                raw: None,
            }],
            raw_response: r#"{"verdict":"questionable"}"#.into(),
        }
    }

    #[test]
    fn submit_and_get_round_trip() {
        let root = fresh_root("submit");
        let store = PinboardStore::new(&root);
        let entry = store
            .submit(
                "web:https://ex.com",
                "scraped a forum post",
                sample_decision(Verdict::Suspicious),
                b"original body bytes",
            )
            .unwrap();

        // body.bin and source.txt both written.
        let body = fs::read(root.join(&entry.id).join("body.bin")).unwrap();
        assert_eq!(body, b"original body bytes");
        let source = fs::read_to_string(root.join(&entry.id).join("source.txt")).unwrap();
        assert_eq!(source, "web:https://ex.com");

        // get() returns the same entry.
        let got = store.get(&entry.id).unwrap();
        assert_eq!(got.id, entry.id);
        assert_eq!(got.state, PinboardState::Pending);
        assert_eq!(got.source, "web:https://ex.com");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn list_returns_pending_first_then_resolved() {
        let root = fresh_root("list_order");
        let store = PinboardStore::new(&root);
        let a = store
            .submit("a", "a", sample_decision(Verdict::Suspicious), b"x")
            .unwrap();
        let b = store
            .submit("b", "b", sample_decision(Verdict::Suspicious), b"y")
            .unwrap();
        let _c = store
            .submit("c", "c", sample_decision(Verdict::Suspicious), b"z")
            .unwrap();

        // Resolve b → Allowed
        store.decide(&b.id, "alice", PinboardAction::Allow, "looks fine").unwrap();
        // Resolve a → Quarantined
        store.decide(&a.id, "bob", PinboardAction::Quarantine, "definitely evil").unwrap();

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 3);
        // Pending first.
        assert_eq!(listed[0].state, PinboardState::Pending);
        // Resolved follow.
        assert!(matches!(
            listed[1].state,
            PinboardState::Allowed | PinboardState::Quarantined
        ));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn decide_allow_transitions_state() {
        let root = fresh_root("allow");
        let store = PinboardStore::new(&root);
        let entry = store
            .submit("s", "c", sample_decision(Verdict::Suspicious), b"x")
            .unwrap();

        let updated = store
            .decide(&entry.id, "alice", PinboardAction::Allow, "released")
            .unwrap();
        assert_eq!(updated.state, PinboardState::Allowed);
        assert_eq!(updated.events.len(), 1);
        assert_eq!(updated.events[0].actor, "alice");
        assert_eq!(updated.events[0].action, PinboardAction::Allow);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn decide_quarantine_transitions_state() {
        let root = fresh_root("quarantine");
        let store = PinboardStore::new(&root);
        let entry = store
            .submit("s", "c", sample_decision(Verdict::Suspicious), b"x")
            .unwrap();
        let updated = store
            .decide(&entry.id, "bob", PinboardAction::Quarantine, "evil")
            .unwrap();
        assert_eq!(updated.state, PinboardState::Quarantined);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn decide_comment_does_not_change_state() {
        let root = fresh_root("comment");
        let store = PinboardStore::new(&root);
        let entry = store
            .submit("s", "c", sample_decision(Verdict::Suspicious), b"x")
            .unwrap();
        let updated = store
            .decide(&entry.id, "alice", PinboardAction::Comment, "noted")
            .unwrap();
        assert_eq!(updated.state, PinboardState::Pending);
        assert_eq!(updated.events.len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn decide_invalid_transition_rejected() {
        let root = fresh_root("invalid");
        let store = PinboardStore::new(&root);
        let entry = store
            .submit("s", "c", sample_decision(Verdict::Suspicious), b"x")
            .unwrap();
        // Allow once.
        store
            .decide(&entry.id, "alice", PinboardAction::Allow, "ok")
            .unwrap();
        // Trying to flip Allowed → Quarantined later should fail.
        let err = store
            .decide(&entry.id, "bob", PinboardAction::Quarantine, "wait actually")
            .unwrap_err();
        assert!(matches!(err, PinboardError::InvalidTransition { .. }));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn get_unknown_id_returns_not_found() {
        let root = fresh_root("notfound");
        let store = PinboardStore::new(&root);
        store
            .submit("s", "c", sample_decision(Verdict::Suspicious), b"x")
            .unwrap();
        let err = store.get("does-not-exist-uuid").unwrap_err();
        assert!(matches!(err, PinboardError::NotFound(_)));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn invalid_id_rejected_with_typed_error() {
        let store = PinboardStore::new(env::temp_dir().join("openfang_pinboard_invalid"));
        for bad in &["..", "a/b", "a b", "", &"x".repeat(70)] {
            let err = store.get(bad).unwrap_err();
            assert!(matches!(err, PinboardError::InvalidId(_)));
        }
    }

    #[test]
    fn list_skips_corrupt_entry() {
        let root = fresh_root("corrupt");
        let store = PinboardStore::new(&root);
        let good = store
            .submit("s", "c", sample_decision(Verdict::Suspicious), b"x")
            .unwrap();

        // Manually create a corrupt sibling dir.
        let bad_id = Uuid::new_v4().to_string();
        let bad_dir = root.join(&bad_id);
        fs::create_dir_all(&bad_dir).unwrap();
        fs::write(bad_dir.join("decision.json"), "this is not json").unwrap();

        let listed = store.list().unwrap();
        // Only the good entry should be returned.
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, good.id);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn list_on_missing_root_returns_empty() {
        let root = fresh_root("missing"); // not created
        let store = PinboardStore::new(&root);
        let listed = store.list().unwrap();
        assert!(listed.is_empty());
    }

    #[test]
    fn render_for_obsidian_includes_all_sections() {
        let root = fresh_root("render");
        let store = PinboardStore::new(&root);
        let entry = store
            .submit(
                "web:https://ex.com",
                "scraped a forum post about AGI",
                sample_decision(Verdict::Suspicious),
                b"x",
            )
            .unwrap();
        let updated = store
            .decide(&entry.id, "alice", PinboardAction::Comment, "needs review")
            .unwrap();

        let md = store.render_for_obsidian(&updated);
        assert!(md.contains("# Pinboard entry"));
        assert!(md.contains("id: "));
        assert!(md.contains("state: pending"));
        assert!(md.contains("source: web:https://ex.com"));
        assert!(md.contains("## Cyber-agent rationale"));
        assert!(md.contains("test rationale"));
        assert!(md.contains("## Content summary"));
        assert!(md.contains("scraped a forum post about AGI"));
        assert!(md.contains("## Scanner outcomes"));
        assert!(md.contains("heuristic"));
        assert!(md.contains("jailbreak.dan_mode"));
        assert!(md.contains("## Audit log"));
        assert!(md.contains("alice"));
        assert!(md.contains("needs review"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn audit_log_appends_in_order() {
        let root = fresh_root("audit");
        let store = PinboardStore::new(&root);
        let entry = store
            .submit("s", "c", sample_decision(Verdict::Suspicious), b"x")
            .unwrap();
        store.decide(&entry.id, "a", PinboardAction::Comment, "first").unwrap();
        store.decide(&entry.id, "b", PinboardAction::Comment, "second").unwrap();
        let final_ = store
            .decide(&entry.id, "c", PinboardAction::Allow, "ok")
            .unwrap();
        assert_eq!(final_.events.len(), 3);
        assert_eq!(final_.events[0].note, "first");
        assert_eq!(final_.events[1].note, "second");
        assert_eq!(final_.events[2].note, "ok");
        assert_eq!(final_.state, PinboardState::Allowed);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn is_safe_id_basics() {
        assert!(is_safe_id("01889a8c-7f22-7e2e-9000-3d6f7c8e1234"));
        assert!(is_safe_id("simple_id"));
        assert!(!is_safe_id(""));
        assert!(!is_safe_id(".."));
        assert!(!is_safe_id("a/b"));
        assert!(!is_safe_id("a b"));
        assert!(!is_safe_id(&"x".repeat(70)));
    }
}
