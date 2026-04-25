# Hardening v0.6.1 — Status of Record

> **Single source of truth.** Every cron-fired hardening session must read this file first, pick the next ⬜ phase, commit, and update this file before exiting. The combination of this file and `git log hardening/v0.6.1` uniquely determines where to resume.

## Machine-readable header

```yaml
state: active          # active | blocked | done
branch: hardening/v0.6.1
last_commit: 3188ad2
last_phase: P6.2
next_phase: P7.1
last_heartbeat: 2026-04-25T10:47:00Z
blocked_reason: null
```

## Phase map

| Phase | Scope | Status | Commit |
|---:|---|:-:|---|
| P0    | Branch + scaffolding                                 | ✅ | `cf8e6c1` |
| P1.1  | Ollama base_url loopback validation                  | ✅ | `667ad4c` |
| P1.2  | Ollama model-not-found → enriched via `/api/tags`    | ✅ | `9fbb2aa` |
| P2.1  | Reactive agents skipped from heartbeat (#1102)       | ✅ | `da62220` |
| P2.2  | Streaming heartbeat ticker (#1089)                   | ✅ | `ab674c5` |
| P3.1  | soul.md YAML frontmatter + `<persona>` tag injection | ✅ | `f485883` |
| P3.2  | Reflection pipeline + two-phase patch commit         | ✅ | `485182f` |
| P3.3  | Cadence guards + immutable-field enforcement         | ✅ | `2bf02e2` |
| P4.1  | `ExternalMemoryBackend` trait + criticality registry | ✅ | `0fb5563` |
| P4.2  | Obsidian backend (read/write `OpenFang/inbox/*`)     | ✅ | `c5007bf` |
| P4.3  | Mempalace backend skeleton (REQUIRED, Critical)      | ✅ | `e4432f3` |
| P5.1  | Universal untrusted-content channel + isolation dir  | ✅ | `d597a1d` |
| P5.2  | Security scanners (Moonlock + heuristic, fail-closed)| ✅ | `95dbc49` |
| P5.3  | Cyber-agent + classifier pipeline + vault docs       | ✅ | `482b0f0` |
| P5.4  | Pinboard storage + state machine + Obsidian render   | ✅ | `dfc158e` |
| P6.1  | Boot-warm registry primitive (Warming/Degraded/Ok/Failed)| ✅ | `5a1e494` |
| P6.2  | launchd/systemd-user/Warp/shell/brew assets          | ✅ | `3188ad2` |
| P7.1  | Release prep v0.6.1 + `tests/hardening_smoke.rs`     | ⬜ | — |

## Cron contract (for fresh sessions picking up from a heartbeat fire)

Every cron-fired session must:

1. **Read this file.** If `state != "active"`, exit with a 1-line summary. Do not retry blocked phases automatically — the user unblocks by setting state back to `active` and clearing `blocked_reason`.
2. **Check git HEAD** matches `last_commit`. If HEAD is ahead, another session already progressed — re-read this file to sync.
3. **Read the plan** at `~/.claude/plans/review-https-github-com-rightnow-ai-open-groovy-donut.md` for full phase specs.
4. **Load user memory** at `~/.claude/projects/-Users-alexeynikitine-Openfang/memory/MEMORY.md` — respects project decisions, autocommit preference, feedback.
5. **Pick the next ⬜ phase** from the map above.
6. **Implement that single phase** — one logical commit per fire. Do not batch multiple phases.
7. **Verify per-crate:**
   ```bash
   cargo build -p <crate> --lib
   cargo test -p <crate> --lib
   cargo clippy -p <crate> --all-targets -- -D warnings \
     -A clippy::collapsible_match -A clippy::unnecessary_sort_by
   ```
   The two `-A` flags isolate pre-existing lints on `main` (see memory `reference_clippy_preexisting.md`).
8. **On failure:** set `state: blocked`, write `blocked_reason: "<what failed + what to try>"`, commit this file, exit.
9. **On success:** commit the code with `hardening p<N>: <summary>` + `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>`. Update this file's header (`last_commit`, `last_phase`, `next_phase`, `last_heartbeat`). Commit this file in a second commit or amend the feature commit.
10. **On completing P7.1:** set `state: done`. The cron job can be deleted via `CronList` → `CronDelete`.

## Do-not-touch list

- `crates/openfang-cli/src/launcher.rs` — user is actively building the interactive CLI.
- Existing `recover_text_tool_calls` at `crates/openfang-runtime/src/agent_loop.rs:2232` (13 patterns, ~60 tests) — extend, don't replace.
- Do not `--no-verify` or `--force`. Do not amend past commits.
- Do not add features outside the plan file's scope.

## Known pre-existing clippy issues on main (do NOT fix inline with feature work)

- `clippy::collapsible_match` in `crates/openfang-runtime/src/drivers/{gemini.rs,openai.rs}`
- `clippy::unnecessary_sort_by` in `crates/openfang-runtime/src/session_repair.rs`

Fix as part of P7 release prep, not scattered through feature phases.

## Handoff notes (latest)

P0–P3 complete. Scope-correction notes:
- Text-fallback tool-call parser already exists (`recover_text_tool_calls` at `agent_loop.rs:2232`, 13 patterns, ~60 tests). Do NOT rebuild.
- SOUL.md is already loaded and injected at `prompt_builder.rs:355-383`; `kernel.rs:303,1973` handle generation + per-turn read. Phase 3.1 added YAML-frontmatter parsing + `<persona>` tag wrapping on top of that existing pipeline.

Phase 3 shipped:
- `soul.rs` — frontmatter parser with `deny_unknown_fields`, body-only fallback, `<persona>` tag escaping.
- `reflection.rs` — two-phase commit via `soul_patch_proposal.md`; prompt builder; strict JSON response parser; cadence log (`soul_reflection_log.jsonl`) with prune-on-write; `can_reflect_now` (4h min gap, 4/24h cap); `check_immutable_fields` defence.

What Phase 3 does NOT yet do (explicit follow-ups):
- **Cron wiring** that actually calls `can_reflect_now` + LLM + `write_patch_proposal` on the 6h cadence. The primitives are all there; the scheduler hook is a glue commit and can live in Phase 7 release prep or an earlier infra commit.
- **Boot-time promote integration** — `promote_pending_patch` exists but no agent-boot path calls it yet. Same note: glue commit, deferred.

Totals: 977 openfang-runtime tests + 263 openfang-kernel tests green. Delta-clippy clean across all phase commits.

Next up **P4.1**: `ExternalMemoryBackend` trait in openfang-memory with read_union/write_fanout semantics + a `Criticality { Critical, Degraded, Optional }` enum that governs whether a backend failure fails boot or degrades health.
