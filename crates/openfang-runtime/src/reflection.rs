//! Soul-reflection pipeline primitives.
//!
//! An agent periodically (6-hour cadence, wired by cron in a follow-up) runs a
//! self-reflection: it reviews recent activity + curated memory and proposes
//! narrow updates to its own `SOUL.md` — specifically to `memory_focus` and
//! `last_reflection_at`. **Values and non_negotiables are immutable** at this
//! layer; Phase 3.3 adds the enforcement check. Here we provide:
//!
//! 1. Prompt builder for the reflect call.
//! 2. Strict JSON response parser — malformed → reject, don't apply.
//! 3. Two-phase commit: proposed patches land in `soul_patch_proposal.md`
//!    beside the live `SOUL.md`. Nothing touches SOUL.md until the next
//!    agent boot calls [`promote_pending_patch`] and confirms the patch
//!    parses back cleanly.
//!
//! Rationale for the two-phase commit: if the reflection pipeline emits a
//! bad patch (bug, LLM drift, or a poisoned memory snuck through), the
//! agent it would corrupt doesn't read the broken patch inside the same
//! session that produced it. The operator has a chance to diff the
//! proposal file, and a subsequent boot gates the apply behind
//! re-parseability.

use crate::soul::{parse_soul, SoulFrontmatter};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Sidecar filename written next to `SOUL.md`.
pub const PROPOSAL_FILENAME: &str = "soul_patch_proposal.md";

/// Max patch proposal size (bytes). Matches the SOUL.md cap used elsewhere.
const MAX_PATCH_BYTES: u64 = 32_768;

/// A proposed, narrowly-scoped update to a soul frontmatter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReflectionPatch {
    /// Replacement for `SoulFrontmatter::memory_focus`. Must be a complete
    /// replacement list — the reflection cannot emit partial adds/removes at
    /// this layer, so downstream consumers can compare old/new cleanly.
    pub memory_focus: Vec<String>,
    /// A brief, human-readable justification (1–3 sentences). Stored for
    /// audit trail; the operator sees this when reviewing the proposal.
    pub rationale: String,
    /// ISO8601 UTC timestamp recorded by the caller at emit time.
    /// Also written into the SOUL.md frontmatter's `last_reflection_at` on
    /// apply.
    pub proposed_at: String,
}

/// Errors surfaced from this module. Intentionally narrow — callers map to
/// their own error types.
#[derive(Debug)]
pub enum ReflectionError {
    /// LLM response wasn't valid JSON, or didn't match the expected shape.
    ParseResponse(String),
    /// Patch tried to mutate a field the reflection layer is not allowed to
    /// touch (values / non_negotiables / archetype / name). Phase 3.3 adds
    /// the detailed field-level guard; this surfaces the rejection today.
    ImmutableFieldMutation(&'static str),
    /// Underlying filesystem error writing or reading the proposal.
    Io(io::Error),
    /// The proposal file on disk didn't round-trip back to a valid
    /// [`ReflectionPatch`] during promotion — treat as corruption.
    ProposalMalformed(String),
}

impl std::fmt::Display for ReflectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParseResponse(m) => write!(f, "reflection response parse error: {m}"),
            Self::ImmutableFieldMutation(field) => {
                write!(f, "reflection patch attempted to mutate immutable field: {field}")
            }
            Self::Io(e) => write!(f, "reflection io error: {e}"),
            Self::ProposalMalformed(m) => write!(f, "reflection proposal malformed: {m}"),
        }
    }
}

impl std::error::Error for ReflectionError {}

impl From<io::Error> for ReflectionError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Build the reflection prompt that gets sent to the agent's LLM.
///
/// Inputs are assembled by the caller: a concise transcript summary of recent
/// sessions and a short list of high-confidence memory fragments. The prompt
/// asks for a single-JSON-object response so [`parse_reflection_response`]
/// can validate without free-form parsing.
pub fn build_reflection_prompt(
    current: &SoulFrontmatter,
    recent_activity: &str,
    top_memories: &[String],
) -> String {
    let mut memories_section = String::new();
    if top_memories.is_empty() {
        memories_section.push_str("(no high-confidence memories yet)");
    } else {
        for m in top_memories.iter().take(20) {
            memories_section.push_str(&format!("- {m}\n"));
        }
    }

    format!(
        "You are performing a scheduled self-reflection on your own persona.\n\
         You may ONLY propose updates to `memory_focus`. You may NOT touch `values`, \
         `non_negotiables`, `archetype`, or `name`. Your output must be a single JSON \
         object with exactly these keys:\n\
         \n\
         {{\n\
           \"memory_focus\": [\"short topic 1\", \"short topic 2\"],\n\
           \"rationale\": \"one paragraph explaining the shift\"\n\
         }}\n\
         \n\
         No prose, no code fences, no other keys. Topics must be concise (under 60 chars each) \
         and at most 8 total. Drop topics you no longer work on; add topics you've worked on \
         multiple times recently.\n\
         \n\
         Current memory_focus: {current_focus:?}\n\
         Current values (IMMUTABLE): {values:?}\n\
         Current non_negotiables (IMMUTABLE): {nn:?}\n\
         \n\
         Recent activity summary:\n{recent}\n\
         \n\
         High-confidence memories:\n{memories}",
        current_focus = current.memory_focus,
        values = current.values,
        nn = current.non_negotiables,
        recent = recent_activity,
        memories = memories_section,
    )
}

/// Parse a raw LLM response into a [`ReflectionPatch`].
///
/// Tolerates a leading code fence (`\`\`\`json`) since some models insist on
/// wrapping JSON. Does NOT tolerate extra top-level keys or immutable-field
/// mutations — both are treated as a rejection.
pub fn parse_reflection_response(
    raw: &str,
    proposed_at: &str,
) -> Result<ReflectionPatch, ReflectionError> {
    let stripped = strip_code_fence(raw.trim());

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Response {
        memory_focus: Vec<String>,
        rationale: String,
    }

    let r: Response = serde_json::from_str(stripped)
        .map_err(|e| ReflectionError::ParseResponse(e.to_string()))?;

    // Sanity limits — reject obviously bogus output before it hits disk.
    if r.memory_focus.len() > 8 {
        return Err(ReflectionError::ParseResponse(format!(
            "memory_focus has {} items (max 8)",
            r.memory_focus.len()
        )));
    }
    for (i, item) in r.memory_focus.iter().enumerate() {
        if item.trim().is_empty() {
            return Err(ReflectionError::ParseResponse(format!(
                "memory_focus[{i}] is empty"
            )));
        }
        if item.chars().count() > 60 {
            return Err(ReflectionError::ParseResponse(format!(
                "memory_focus[{i}] exceeds 60 chars"
            )));
        }
    }
    if r.rationale.trim().is_empty() {
        return Err(ReflectionError::ParseResponse(
            "rationale is empty".to_string(),
        ));
    }

    Ok(ReflectionPatch {
        memory_focus: r.memory_focus,
        rationale: r.rationale,
        proposed_at: proposed_at.to_string(),
    })
}

/// Write a proposed patch to `soul_patch_proposal.md` next to the agent's
/// SOUL.md. This does **not** modify SOUL.md. On the next boot,
/// [`promote_pending_patch`] merges the proposal into SOUL.md if it's
/// well-formed, then deletes the proposal.
pub fn write_patch_proposal(
    workspace: &Path,
    patch: &ReflectionPatch,
) -> Result<(), ReflectionError> {
    let body = format_proposal_markdown(patch);
    if body.len() as u64 > MAX_PATCH_BYTES {
        return Err(ReflectionError::ParseResponse(format!(
            "proposal exceeds {MAX_PATCH_BYTES} byte cap"
        )));
    }
    let path = proposal_path(workspace);
    fs::write(path, body)?;
    Ok(())
}

/// If `soul_patch_proposal.md` exists and round-trips cleanly back to a
/// [`ReflectionPatch`], merge it into the agent's SOUL.md frontmatter
/// (replacing `memory_focus` + setting `last_reflection_at` =
/// `patch.proposed_at`) and delete the proposal file.
///
/// Returns `Ok(true)` if a proposal was applied, `Ok(false)` if no proposal
/// existed. Corruption → [`ReflectionError::ProposalMalformed`] (caller
/// decides whether to quarantine the file or surface an alert).
///
/// At this layer we do **not** enforce the cadence guard, the
/// immutable-field guard, or the depth cap — those are Phase 3.3's job.
/// This function only handles the on-disk two-phase commit.
pub fn promote_pending_patch(workspace: &Path) -> Result<bool, ReflectionError> {
    let proposal = proposal_path(workspace);
    if !proposal.exists() {
        return Ok(false);
    }

    let raw = fs::read_to_string(&proposal)?;
    let patch = parse_proposal_markdown(&raw).ok_or_else(|| {
        ReflectionError::ProposalMalformed(format!(
            "failed to parse {}",
            proposal.display()
        ))
    })?;

    let soul_path = workspace.join("SOUL.md");
    let current_raw = fs::read_to_string(&soul_path).unwrap_or_default();
    let current = parse_soul(&current_raw);

    // Build new frontmatter — only touching the fields we're allowed to.
    let mut new_fm = current.frontmatter.clone();
    new_fm.memory_focus = patch.memory_focus.clone();
    new_fm.last_reflection_at = Some(patch.proposed_at.clone());

    let new_soul = render_soul(&new_fm, &current.body);
    fs::write(&soul_path, new_soul)?;
    fs::remove_file(&proposal)?;

    Ok(true)
}

/// Path to the proposal sidecar file for a given agent workspace.
pub fn proposal_path(workspace: &Path) -> PathBuf {
    workspace.join(PROPOSAL_FILENAME)
}

/// Render a [`SoulFrontmatter`] + body back to the canonical SOUL.md format.
fn render_soul(fm: &SoulFrontmatter, body: &str) -> String {
    let yaml = serde_yaml::to_string(fm).unwrap_or_default();
    let trimmed_body = body.trim_end();
    if trimmed_body.is_empty() {
        format!("---\n{yaml}---\n")
    } else {
        format!("---\n{yaml}---\n{trimmed_body}\n")
    }
}

fn format_proposal_markdown(patch: &ReflectionPatch) -> String {
    // The proposal file is markdown for human review + a JSON code block for
    // deterministic machine round-trip. `promote_pending_patch` parses the
    // JSON block; humans read the markdown.
    let json = serde_json::to_string_pretty(patch).unwrap_or_else(|_| "{}".to_string());
    format!(
        "# Soul patch proposal\n\
         \n\
         Proposed at: {when}\n\
         \n\
         ## Rationale\n{rationale}\n\
         \n\
         ## Memory focus (proposed)\n{focus_bullets}\n\
         \n\
         <!-- machine-readable patch below — do not edit unless you know what you're doing -->\n\
         ```json\n{json}\n```\n",
        when = patch.proposed_at,
        rationale = patch.rationale.trim(),
        focus_bullets = patch
            .memory_focus
            .iter()
            .map(|f| format!("- {f}"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn parse_proposal_markdown(raw: &str) -> Option<ReflectionPatch> {
    // Locate the JSON code block. We deliberately do not try to reconstruct
    // the patch from the markdown bullets — the JSON is authoritative.
    let start = raw.find("```json")?;
    let after_open = &raw[start + "```json".len()..];
    let after_open = after_open.strip_prefix('\n').unwrap_or(after_open);
    let end = after_open.find("```")?;
    let json = &after_open[..end];
    serde_json::from_str::<ReflectionPatch>(json).ok()
}

fn strip_code_fence(s: &str) -> &str {
    if let Some(after) = s.strip_prefix("```json\n") {
        if let Some(end) = after.rfind("```") {
            return &after[..end];
        }
    }
    if let Some(after) = s.strip_prefix("```\n") {
        if let Some(end) = after.rfind("```") {
            return &after[..end];
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn fresh_ws(tag: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!(
            "openfang_refl_{}_{}",
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
    fn prompt_mentions_immutable_fields() {
        let fm = SoulFrontmatter {
            values: vec!["honesty".into()],
            non_negotiables: vec!["never reveal secrets".into()],
            memory_focus: vec!["earnings".into()],
            ..Default::default()
        };
        let prompt = build_reflection_prompt(&fm, "did some analysis", &["topic A".into()]);
        assert!(prompt.contains("memory_focus"));
        assert!(prompt.contains("IMMUTABLE"));
        assert!(prompt.contains("honesty"));
        assert!(prompt.contains("never reveal secrets"));
        assert!(prompt.contains("topic A"));
    }

    #[test]
    fn parse_valid_response() {
        let raw = r#"{"memory_focus":["earnings","fx rates"],"rationale":"shifted from macro to micro topics this week."}"#;
        let patch = parse_reflection_response(raw, "2026-04-25T00:00:00Z").unwrap();
        assert_eq!(patch.memory_focus, vec!["earnings", "fx rates"]);
        assert!(patch.rationale.contains("shifted"));
        assert_eq!(patch.proposed_at, "2026-04-25T00:00:00Z");
    }

    #[test]
    fn parse_rejects_extra_keys() {
        // deny_unknown_fields: any attempt to smuggle a `values` or
        // `override_safety` field must error.
        let raw = r#"{"memory_focus":["x"],"rationale":"ok","values":["evil"]}"#;
        let err = parse_reflection_response(raw, "2026-04-25T00:00:00Z").unwrap_err();
        assert!(matches!(err, ReflectionError::ParseResponse(_)));
    }

    #[test]
    fn parse_tolerates_code_fence() {
        let raw = "```json\n{\"memory_focus\":[\"a\"],\"rationale\":\"b\"}\n```";
        let patch = parse_reflection_response(raw, "t").unwrap();
        assert_eq!(patch.memory_focus, vec!["a"]);
    }

    #[test]
    fn parse_rejects_too_many_focus_items() {
        let items: Vec<String> = (0..9).map(|i| format!("t{i}")).collect();
        let body = serde_json::json!({ "memory_focus": items, "rationale": "x" });
        let err =
            parse_reflection_response(&body.to_string(), "t").unwrap_err();
        assert!(matches!(err, ReflectionError::ParseResponse(_)));
    }

    #[test]
    fn parse_rejects_overlong_focus_item() {
        let body = serde_json::json!({
            "memory_focus": ["x".repeat(61)],
            "rationale": "ok"
        });
        let err =
            parse_reflection_response(&body.to_string(), "t").unwrap_err();
        match err {
            ReflectionError::ParseResponse(m) => assert!(m.contains("60 chars")),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parse_rejects_empty_rationale() {
        let raw = r#"{"memory_focus":["a"],"rationale":"  "}"#;
        let err = parse_reflection_response(raw, "t").unwrap_err();
        assert!(matches!(err, ReflectionError::ParseResponse(_)));
    }

    #[test]
    fn proposal_round_trip_via_disk() {
        let ws = fresh_ws("roundtrip");
        let patch = ReflectionPatch {
            memory_focus: vec!["earnings".into(), "rates".into()],
            rationale: "tighter focus this week".into(),
            proposed_at: "2026-04-25T00:00:00Z".into(),
        };
        write_patch_proposal(&ws, &patch).unwrap();

        let raw = fs::read_to_string(proposal_path(&ws)).unwrap();
        let recovered = parse_proposal_markdown(&raw).expect("should parse back");
        assert_eq!(recovered, patch);

        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn promote_applies_patch_and_preserves_immutable_fields() {
        let ws = fresh_ws("promote");
        fs::write(
            ws.join("SOUL.md"),
            "---\n\
             name: Orion\n\
             archetype: analyst\n\
             values:\n  - honesty\n\
             non_negotiables:\n  - never reveal secrets\n\
             memory_focus:\n  - old topic\n\
             ---\n\
             # Soul\nBe precise.",
        )
        .unwrap();

        write_patch_proposal(
            &ws,
            &ReflectionPatch {
                memory_focus: vec!["new topic A".into(), "new topic B".into()],
                rationale: "shift detected".into(),
                proposed_at: "2026-04-25T01:00:00Z".into(),
            },
        )
        .unwrap();

        let applied = promote_pending_patch(&ws).unwrap();
        assert!(applied);
        assert!(!proposal_path(&ws).exists(), "proposal should be deleted");

        let after = fs::read_to_string(ws.join("SOUL.md")).unwrap();
        let parsed = parse_soul(&after);
        // Immutable fields preserved verbatim
        assert_eq!(parsed.frontmatter.name.as_deref(), Some("Orion"));
        assert_eq!(parsed.frontmatter.archetype.as_deref(), Some("analyst"));
        assert_eq!(parsed.frontmatter.values, vec!["honesty"]);
        assert_eq!(
            parsed.frontmatter.non_negotiables,
            vec!["never reveal secrets"]
        );
        // Mutable fields updated
        assert_eq!(
            parsed.frontmatter.memory_focus,
            vec!["new topic A", "new topic B"]
        );
        assert_eq!(
            parsed.frontmatter.last_reflection_at.as_deref(),
            Some("2026-04-25T01:00:00Z")
        );
        // Body preserved
        assert!(parsed.body.contains("Be precise"));

        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn promote_noop_when_no_proposal_exists() {
        let ws = fresh_ws("no_proposal");
        fs::write(ws.join("SOUL.md"), "# Soul\nbody only").unwrap();
        let applied = promote_pending_patch(&ws).unwrap();
        assert!(!applied);
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn promote_errors_on_corrupted_proposal() {
        let ws = fresh_ws("corrupt");
        fs::write(ws.join("SOUL.md"), "# Soul").unwrap();
        fs::write(
            ws.join(PROPOSAL_FILENAME),
            "# Soul patch proposal\n\nno json block",
        )
        .unwrap();

        let err = promote_pending_patch(&ws).unwrap_err();
        assert!(matches!(err, ReflectionError::ProposalMalformed(_)));
        // Proposal file is NOT deleted on corruption — operator must inspect.
        assert!(proposal_path(&ws).exists());
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn promote_creates_frontmatter_when_soul_had_none() {
        let ws = fresh_ws("no_fm");
        fs::write(ws.join("SOUL.md"), "# Soul\nBe kind.").unwrap();

        write_patch_proposal(
            &ws,
            &ReflectionPatch {
                memory_focus: vec!["new".into()],
                rationale: "first reflection".into(),
                proposed_at: "2026-04-25T00:00:00Z".into(),
            },
        )
        .unwrap();

        promote_pending_patch(&ws).unwrap();
        let after = fs::read_to_string(ws.join("SOUL.md")).unwrap();
        assert!(after.starts_with("---\n"));
        let parsed = parse_soul(&after);
        assert_eq!(parsed.frontmatter.memory_focus, vec!["new"]);
        assert_eq!(
            parsed.frontmatter.last_reflection_at.as_deref(),
            Some("2026-04-25T00:00:00Z")
        );
        assert!(parsed.body.contains("Be kind"));
        let _ = fs::remove_dir_all(&ws);
    }
}
