# Hardening Plan — v0.6.1

This document is a short pointer to the full hardening plan driving work on
the `hardening/v0.6.1` branch.

The full plan (context, phases, critical files, verification) lives outside
the repo at `~/.claude/plans/review-https-github-com-rightnow-ai-open-groovy-donut.md`.
Upstream PR descriptions will reference individual phases from that plan.

## Phase map

| Phase | Scope | Commits |
|------:|-------|---------|
| 0 | Branch + scaffolding (this doc, `scripts/live-smoke.sh`) | 1 |
| 1 | Ollama text-fallback tool parser + base_url hardening | 3 |
| 2 | Heartbeat state-machine fix (#1102, #1089) | 2 |
| 3 | `soul.md` persona loader + 6h reflection loop + recursion guards | 3 |
| 4 | `ExternalMemoryBackend` trait + Obsidian + Mempalace (required) | 3 |
| 5 | Agentic triage pipeline (quarantine + scanners + cyber-agent + pinboard) | 4 |
| 6 | Boot-warm health gating + launchd/systemd/Warp/shell assets | 2 |
| 7 | Release prep for v0.6.1 + hardening smoke test | 1 |

## Invariants

- Every commit buildable: `cargo build --workspace --lib` clean.
- Every commit green: `cargo test --workspace` — existing 1744+ tests stay green.
- Every commit lint-clean: `cargo clippy --workspace --all-targets -- -D warnings`.
- Every phase ends with `scripts/live-smoke.sh` passing.
- No changes to `openfang-cli` interactive launcher ([crates/openfang-cli/src/launcher.rs](../crates/openfang-cli/src/launcher.rs)).
- No breaking changes to existing `/api/*` routes.
