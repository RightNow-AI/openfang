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

/// Per-agent cadence log — one JSON timestamp per line, pruned to the last
/// [`CADENCE_WINDOW_HOURS`] hours of entries on every write.
pub const CADENCE_LOG_FILENAME: &str = "soul_reflection_log.jsonl";

/// Max patch proposal size (bytes). Matches the SOUL.md cap used elsewhere.
const MAX_PATCH_BYTES: u64 = 32_768;

/// Max reflections permitted per agent per rolling 24h. Matches the 6h
/// cadence (4×/day) agreed with the operator.
pub const MAX_REFLECTIONS_PER_WINDOW: usize = 4;

/// Minimum seconds between two reflections for the same agent. Prevents
/// cron-overlap storms if the scheduler accidentally double-fires.
pub const MIN_GAP_SECONDS: i64 = 4 * 3600;

/// Rolling window (hours) over which [`MAX_REFLECTIONS_PER_WINDOW`] applies.
pub const CADENCE_WINDOW_HOURS: i64 = 24;

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
    /// touch (values / non_negotiables / archetype / name).
    ImmutableFieldMutation(&'static str),
    /// Cadence guard refused the reflection — fires too soon after a previous
    /// one or exceeds the per-window cap.
    CadenceGuard(String),
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
            Self::CadenceGuard(m) => write!(f, "reflection cadence guard: {m}"),
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

    // Defence in depth: explicitly verify no immutable field is being
    // touched. This is a no-op given the assignments above, but catches
    // future regressions where someone might add another patch field.
    check_immutable_fields(&current.frontmatter, &new_fm)?;

    let new_soul = render_soul(&new_fm, &current.body);
    fs::write(&soul_path, new_soul)?;
    fs::remove_file(&proposal)?;

    // Record in the cadence log so the next `can_reflect_now` call sees the
    // reflection that just applied. Timestamp parsed from the patch to match
    // the wall-clock moment the reflection was produced, not the apply time.
    if let Some(epoch) = parse_iso8601_to_epoch(&patch.proposed_at) {
        record_reflection(workspace, epoch)?;
    }

    Ok(true)
}

/// Parse ISO8601 → unix seconds. Returns None for anything we can't parse
/// with the built-in `chrono` parser. The cadence log gracefully degrades:
/// a patch with a bad timestamp still applies, it just doesn't get logged
/// for cadence purposes (and next call to `can_reflect_now` is permissive).
fn parse_iso8601_to_epoch(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp())
}

/// Path to the proposal sidecar file for a given agent workspace.
pub fn proposal_path(workspace: &Path) -> PathBuf {
    workspace.join(PROPOSAL_FILENAME)
}

/// Path to the cadence log for a given agent workspace.
pub fn cadence_log_path(workspace: &Path) -> PathBuf {
    workspace.join(CADENCE_LOG_FILENAME)
}

/// Check whether another reflection is permitted right now for this agent.
///
/// Reads the cadence log on disk and enforces two rules:
/// 1. **Min gap**: at least [`MIN_GAP_SECONDS`] (4h) must have passed since
///    the most recent entry.
/// 2. **Per-window cap**: no more than [`MAX_REFLECTIONS_PER_WINDOW`] (4)
///    entries within the trailing [`CADENCE_WINDOW_HOURS`] (24h).
///
/// `now_epoch_secs` is passed in so tests can be deterministic without
/// mocking the clock at the OS level.
pub fn can_reflect_now(workspace: &Path, now_epoch_secs: i64) -> Result<(), ReflectionError> {
    let entries = read_cadence_log(workspace)?;
    let window_start = now_epoch_secs - CADENCE_WINDOW_HOURS * 3600;
    let recent: Vec<i64> = entries
        .into_iter()
        .filter(|&t| t >= window_start)
        .collect();

    if let Some(latest) = recent.iter().copied().max() {
        let since = now_epoch_secs - latest;
        if since < MIN_GAP_SECONDS {
            return Err(ReflectionError::CadenceGuard(format!(
                "only {since}s since last reflection; minimum gap is {MIN_GAP_SECONDS}s"
            )));
        }
    }

    if recent.len() >= MAX_REFLECTIONS_PER_WINDOW {
        return Err(ReflectionError::CadenceGuard(format!(
            "{n} reflections already in the trailing {CADENCE_WINDOW_HOURS}h (max {max})",
            n = recent.len(),
            max = MAX_REFLECTIONS_PER_WINDOW
        )));
    }

    Ok(())
}

/// Append a timestamp to the cadence log, then prune entries older than the
/// window so the file cannot grow unbounded.
pub fn record_reflection(
    workspace: &Path,
    now_epoch_secs: i64,
) -> Result<(), ReflectionError> {
    let mut entries = read_cadence_log(workspace)?;
    entries.push(now_epoch_secs);
    let window_start = now_epoch_secs - CADENCE_WINDOW_HOURS * 3600;
    entries.retain(|&t| t >= window_start);
    entries.sort_unstable();

    let path = cadence_log_path(workspace);
    let body: String = entries
        .iter()
        .map(|t| format!("{{\"at\":{t}}}\n"))
        .collect();
    fs::write(path, body)?;
    Ok(())
}

fn read_cadence_log(workspace: &Path) -> Result<Vec<i64>, ReflectionError> {
    let path = cadence_log_path(workspace);
    let raw = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut out = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Tolerate a corrupted line rather than losing the whole log.
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(ts) = v.get("at").and_then(|x| x.as_i64()) {
                out.push(ts);
            }
        }
    }
    Ok(out)
}

/// Verify that applying a [`ReflectionPatch`] to `current` would not mutate
/// any immutable field. Returns the first offending field name, or None if
/// the patch is safe.
///
/// Immutable fields: `name`, `archetype`, `values`, `non_negotiables`.
/// Mutable fields: `memory_focus`, `last_reflection_at`.
///
/// This is called defensively from [`promote_pending_patch`] — even though
/// the promote implementation only writes the mutable fields, this guard
/// ensures the patch's *intent* matches that contract. A patch that claims
/// to also change `values` should be rejected outright, not silently
/// ignored.
pub fn check_immutable_fields(
    current: &SoulFrontmatter,
    new: &SoulFrontmatter,
) -> Result<(), ReflectionError> {
    if current.name != new.name {
        return Err(ReflectionError::ImmutableFieldMutation("name"));
    }
    if current.archetype != new.archetype {
        return Err(ReflectionError::ImmutableFieldMutation("archetype"));
    }
    if current.values != new.values {
        return Err(ReflectionError::ImmutableFieldMutation("values"));
    }
    if current.non_negotiables != new.non_negotiables {
        return Err(ReflectionError::ImmutableFieldMutation("non_negotiables"));
    }
    Ok(())
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
    fn cadence_allows_first_reflection() {
        let ws = fresh_ws("cadence_first");
        let now = 1_700_000_000;
        assert!(can_reflect_now(&ws, now).is_ok());
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn cadence_blocks_too_soon() {
        let ws = fresh_ws("cadence_soon");
        let t0 = 1_700_000_000;
        record_reflection(&ws, t0).unwrap();

        // 1h later — should be rejected (< 4h gap).
        let err = can_reflect_now(&ws, t0 + 3600).unwrap_err();
        match err {
            ReflectionError::CadenceGuard(m) => {
                assert!(m.contains("since last reflection"))
            }
            _ => panic!("wrong variant"),
        }
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn cadence_allows_after_min_gap() {
        let ws = fresh_ws("cadence_gap");
        let t0 = 1_700_000_000;
        record_reflection(&ws, t0).unwrap();
        // Exactly 4h later — should be allowed.
        assert!(can_reflect_now(&ws, t0 + MIN_GAP_SECONDS).is_ok());
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn cadence_blocks_after_window_cap() {
        let ws = fresh_ws("cadence_cap");
        let base = 1_700_000_000;
        // 4 reflections spaced exactly 4h apart (min gap) — fills the 24h
        // window cap.
        for i in 0..MAX_REFLECTIONS_PER_WINDOW as i64 {
            record_reflection(&ws, base + i * MIN_GAP_SECONDS).unwrap();
        }
        // 5h after the last one — gap is fine but the 4-per-window cap
        // should trip.
        let now = base + (MAX_REFLECTIONS_PER_WINDOW as i64) * MIN_GAP_SECONDS + 3600;
        let err = can_reflect_now(&ws, now).unwrap_err();
        match err {
            ReflectionError::CadenceGuard(m) => assert!(m.contains("max")),
            _ => panic!("wrong variant"),
        }
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn cadence_log_prunes_entries_outside_window() {
        let ws = fresh_ws("cadence_prune");
        // Ancient entry 48h old — should be pruned when we record a new one.
        record_reflection(&ws, 1_700_000_000).unwrap();
        let later = 1_700_000_000 + 2 * CADENCE_WINDOW_HOURS * 3600;
        record_reflection(&ws, later).unwrap();
        let contents = fs::read_to_string(cadence_log_path(&ws)).unwrap();
        assert!(!contents.contains("1700000000"), "old entry should have been pruned");
        assert!(contents.contains(&later.to_string()));
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn cadence_log_tolerates_corrupted_line() {
        let ws = fresh_ws("cadence_corrupt");
        fs::write(
            cadence_log_path(&ws),
            "{\"at\":1700000000}\nnot-json\n{\"at\":1700020000}\n",
        )
        .unwrap();
        // Should parse 2 timestamps, skip the junk line.
        let entries = read_cadence_log(&ws).unwrap();
        assert_eq!(entries.len(), 2);
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn immutable_field_check_rejects_values_change() {
        let a = SoulFrontmatter {
            values: vec!["honesty".into()],
            ..Default::default()
        };
        let b = SoulFrontmatter {
            values: vec!["ruthlessness".into()],
            ..Default::default()
        };
        let err = check_immutable_fields(&a, &b).unwrap_err();
        assert!(matches!(
            err,
            ReflectionError::ImmutableFieldMutation("values")
        ));
    }

    #[test]
    fn immutable_field_check_rejects_name_change() {
        let a = SoulFrontmatter {
            name: Some("Orion".into()),
            ..Default::default()
        };
        let b = SoulFrontmatter {
            name: Some("Loki".into()),
            ..Default::default()
        };
        let err = check_immutable_fields(&a, &b).unwrap_err();
        assert!(matches!(
            err,
            ReflectionError::ImmutableFieldMutation("name")
        ));
    }

    #[test]
    fn immutable_field_check_rejects_archetype_change() {
        let a = SoulFrontmatter {
            archetype: Some("analyst".into()),
            ..Default::default()
        };
        let b = SoulFrontmatter {
            archetype: Some("saboteur".into()),
            ..Default::default()
        };
        let err = check_immutable_fields(&a, &b).unwrap_err();
        assert!(matches!(
            err,
            ReflectionError::ImmutableFieldMutation("archetype")
        ));
    }

    #[test]
    fn immutable_field_check_rejects_non_negotiables_change() {
        let a = SoulFrontmatter {
            non_negotiables: vec!["never reveal secrets".into()],
            ..Default::default()
        };
        let b = SoulFrontmatter {
            non_negotiables: vec![],
            ..Default::default()
        };
        let err = check_immutable_fields(&a, &b).unwrap_err();
        assert!(matches!(
            err,
            ReflectionError::ImmutableFieldMutation("non_negotiables")
        ));
    }

    #[test]
    fn immutable_field_check_allows_mutable_only_changes() {
        let a = SoulFrontmatter {
            name: Some("Orion".into()),
            values: vec!["honesty".into()],
            memory_focus: vec!["old".into()],
            last_reflection_at: Some("t0".into()),
            ..Default::default()
        };
        let b = SoulFrontmatter {
            name: Some("Orion".into()),
            values: vec!["honesty".into()],
            memory_focus: vec!["new1".into(), "new2".into()],
            last_reflection_at: Some("t1".into()),
            ..Default::default()
        };
        assert!(check_immutable_fields(&a, &b).is_ok());
    }

    #[test]
    fn promote_populates_cadence_log() {
        let ws = fresh_ws("promote_log");
        fs::write(
            ws.join("SOUL.md"),
            "---\nname: Orion\nvalues:\n  - honesty\n---\nbody",
        )
        .unwrap();
        write_patch_proposal(
            &ws,
            &ReflectionPatch {
                memory_focus: vec!["x".into()],
                rationale: "r".into(),
                proposed_at: "2026-04-25T00:00:00+00:00".into(),
            },
        )
        .unwrap();
        promote_pending_patch(&ws).unwrap();

        let log = fs::read_to_string(cadence_log_path(&ws)).unwrap();
        assert!(log.contains("\"at\""));
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
