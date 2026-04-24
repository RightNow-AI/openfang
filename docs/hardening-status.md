# Hardening v0.6.1 — Status of Record

> **Single source of truth.** Every cron-fired hardening session must read this file first, pick the next ⬜ phase, commit, and update this file before exiting. The combination of this file and `git log hardening/v0.6.1` uniquely determines where to resume.

## Machine-readable header

```yaml
state: active          # active | blocked | done
branch: hardening/v0.6.1
last_commit: f485883
last_phase: P3.1
next_phase: P3.2
last_heartbeat: 2026-04-25T06:10:00Z
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
| P3.2  | Reflection cron (6h cadence) + two-phase patch       | ⬜ | — |
| P3.3  | Recursion guards (4/24h, 4h gap, immutable fields)   | ⬜ | — |
| P4.1  | `ExternalMemoryBackend` trait + criticality enum     | ⬜ | — |
| P4.2  | Obsidian backend (read/write `OpenFang/inbox/*`)     | ⬜ | — |
| P4.3  | Mempalace backend (REQUIRED; boot-fail if absent)    | ⬜ | — |
| P5.1  | Universal untrusted-content channel + isolation dir  | ⬜ | — |
| P5.2  | Security scanners (Moonlock deepscan + heuristic)    | ⬜ | — |
| P5.3  | Cyber-agent + classifier pipeline                    | ⬜ | — |
| P5.4  | Pinboard + escalation surface                        | ⬜ | — |
| P6.1  | Boot-warm health gating (`warming` → `degraded`/`ok`)| ⬜ | — |
| P6.2  | launchd/systemd/Warp/shell assets                    | ⬜ | — |
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

P0–P2 landed cleanly: loopback validation on Ollama base_url, enriched model-not-found errors via `/api/tags` probe, reactive-agent skip in heartbeat (#1102), 30s heartbeat ticker during streams (#1089). 942 runtime tests + 263 kernel tests green; delta-clippy clean across all commits. Phase 3 is substantially more invasive (new `soul.rs` module, YAML frontmatter parser, cron integration, two-phase `soul_patch_proposal.md` commit, immutable-field enforcement) — budget it as 3 separate cron fires, one per sub-commit.
