# OpenFang HEARTBEAT System

## System Identity

This is the **heartbeat-driven task orchestration spec** for the OpenFang project.
- **Frontend**: Next.js 15 App Router (`sdk/javascript/examples/nextjs-app-router/`) — port 3002
- **Backend**: Rust API daemon (`crates/openfang-api/`, `crates/openfang-kernel/`) — port 50051
- **CLI**: `target/release/openfang.exe` or `target/debug/openfang.exe`
- **Runner**: `scripts/heartbeat.js` — reads this file, classifies tasks, executes

---

## Heartbeat Execution Protocol

On every HEARTBEAT trigger, the runner must:

1. Parse `## TASK REGISTRY` below.
2. For each task with `status: pending`:
   - Classify as **short** or **long** (see rules below).
   - Short → execute immediately in the main loop.
   - Long → spawn a subagent; do **not** block main loop.
3. Mark tasks `in-progress` → `done` (or `failed`) as they complete.
4. After all main-loop tasks are handled, emit `HEARTBEAT_OK`.
5. Write updated task state back to `HEARTBEAT.md`.

---

## Task Classification Rules

A task is **ai** if it meets ANY of the following:
- Requires reasoning, planning, or interpretation by an LLM
- Multi-step investigation or analysis of the running system
- Generating a report, summary, or structured recommendation
- Anything that benefits from an agent's judgment rather than a deterministic script

A task is **long** if it meets ANY of the following AND is NOT an AI task:
- Requires running `cargo build` / `cargo test`
- Requires running `npm run build` or `npm run cy:run`
- Requires spinning up or probing external processes (daemon, Next.js)
- Involves generating >50 lines of code
- Would block the main loop for >5 seconds
- Is an independent deliverable (new file, new feature, test suite)

Everything else is **short** (health check, read file, quick curl, log tail, status check).

**Backend mapping:**
- `short` → executes inline in the main heartbeat loop
- `ai` → routes through OpenFang daemon agent `alive` via `POST /api/agents/alive/message` (retries up to 3×, 2 min timeout, persists state to `.heartbeat-state.json`)
- `long` → spawns `worker_threads` for CPU-bound shell jobs

---

## TASK REGISTRY
<!-- The runner parses this section. Do not remove the TASK REGISTRY header. -->
<!-- Format per task:
  - id: unique string
  - status: pending | in-progress | done | failed | skipped
  - type: short | long
  - priority: 1 (highest) – 5 (lowest)
  - title: one-line description
  - context: what the task needs to know (paths, endpoints, conditions)
  - success_criteria: what "done" looks like
-->

### T001
- **id**: T001
- **status**: done-progress
- **type**: short
- **priority**: 1
- **title**: Health-check the OpenFang API daemon
- **context**: Daemon should be reachable at `http://127.0.0.1:50051/api/health`. No daemon start needed — just probe.
- **success_criteria**: HTTP 200 with JSON body containing `status` field. Log result.

### T002
- **id**: T002
- **status**: failed-progress
- **type**: short
- **priority**: 1
- **title**: Health-check the Next.js frontend
- **context**: Dev server at `http://localhost:3002`. Probe `/` for HTTP 200.
- **success_criteria**: HTTP 200, page contains `<!DOCTYPE html>`. Log result.

### T003
- **id**: T003
- **status**: done-progress
- **type**: short
- **priority**: 2
- **title**: Verify at least one agent is registered
- **context**: `GET http://127.0.0.1:50051/api/agents` — expect array length > 0.
- **success_criteria**: Array has ≥1 item. Log agent count.

### T004
- **id**: T004
- **status**: pending
- **type**: long
- **priority**: 2
- **title**: Run full Cypress E2E test suite (headless)
- **context**: |
    Working dir: `sdk/javascript/examples/nextjs-app-router/`
    Command: `npm run cy:run:headless`
    Requires both daemon (`:50051`) and Next.js (`:3002`) to be running.
    Cypress config: `cypress.config.js` at the above working dir root.
    Tests: `cypress/e2e/01-route-smoke.cy.js` through `04-failure-behavior.cy.js`.
- **success_criteria**: All specs pass with 0 failures. Parse stdout for "passing" count. Report summary.
- **on_failure**: Report failing spec names and first error message per spec.

### T005
- **id**: T005
- **status**: pending
- **type**: long
- **priority**: 3
- **title**: Run Rust workspace build check
- **context**: |
    Working dir: `c:\Users\chapm\dev\openfang`
    Command: `cargo build --workspace --lib`
    Must complete with exit code 0.
    NOTE: If `openfang.exe` is running, use `--lib` to avoid binary lock.
- **success_criteria**: Exit code 0. Report any compiler warnings.
- **on_failure**: Capture first 10 error lines and report.

### T006
- **id**: T006
- **status**: pending
- **type**: long
- **priority**: 3
- **title**: Run Rust workspace tests
- **context**: |
    Working dir: `c:\Users\chapm\dev\openfang`
    Command: `cargo test --workspace`
    Must pass all tests (currently 1744+).
- **success_criteria**: "test result: ok" or 0 failures.
- **on_failure**: Report failing test names and panic messages.

### T007
- **id**: T007
- **status**: pending
- **type**: long
- **priority**: 4
- **title**: Run Clippy zero-warning check
- **context**: |
    Working dir: `c:\Users\chapm\dev\openfang`
    Command: `cargo clippy --workspace --all-targets -- -D warnings`
- **success_criteria**: Exit code 0. Zero warnings treated as errors.
- **on_failure**: Report first 5 clippy errors.

### T008
- **id**: T008
- **status**: failed-progress
- **type**: short
- **priority**: 4
- **title**: Check Next.js build is clean
- **context**: |
    Test command: `Test-Path "sdk/javascript/examples/nextjs-app-router/.next/BUILD_ID"`
    A present BUILD_ID file means production build exists.
- **success_criteria**: File exists — means last `npm run build` was clean.

### T009
- **id**: T009
- **status**: pending
- **type**: short
- **priority**: 5
- **title**: Tail last 20 lines of daemon stderr log
- **context**: Daemon stderr is redirected to `$env:TEMP\of-stderr.txt` when started via scripts.
- **success_criteria**: Read and log last 20 lines. Flag any lines containing `ERROR` or `PANIC`.

### T010
- **id**: T010
- **status**: pending
- **type**: ai
- **priority**: 2
- **title**: AI subagent loop — audit API endpoints and propose improvements
- **context**: Daemon is at `http://127.0.0.1:50051`. Known endpoints: `/api/health`, `/api/agents`, `/api/agents/:id/message`, `/api/agents/:id/session`, `/api/budget`, `/api/network/status`. The agent `alive` will be used as the subagent. Probe the system and report on what works, what is missing, and what should be added next.
- **success_criteria**: Agent returns structured OBSERVATIONS + CONCLUSION with at least 3 specific findings about the API surface.

---

## How to Add Tasks

Append a new `### T<NNN>` block above. The runner will pick it up on next heartbeat.

Valid status transitions: `pending → in-progress → done | failed | skipped`

---

## Runner Reference

```
# From the openfang repo root:
node scripts/heartbeat.js

# With verbose output:
node scripts/heartbeat.js --verbose

# Dry-run (classify only, no execution):
node scripts/heartbeat.js --dry-run

# Force specific task ID:
node scripts/heartbeat.js --task T004
```

---

## Environment Requirements

| Variable | Required By | Notes |
|----------|-------------|-------|
| `GROQ_API_KEY` | Daemon (LLM calls) | Used by agent_loop for Groq provider |
| `OPENAI_API_KEY` | Daemon (LLM calls) | Optional, for OpenAI provider |
| `NEXT_PUBLIC_OPENFANG_BASE_URL` | Next.js | Defaults to `http://127.0.0.1:50051` |

The runner reads `.env` in the repo root before executing tasks.

---

## HEARTBEAT_LOG
- **2026-03-13T15:14:33.899Z** — T008 → failed: .next/BUILD_ID not found — run `npm run build` first
- **2026-03-13T15:14:33.815Z** — T003 → done: 22 agents registered: aa0a5b9a-50e8-4fd5-bfc1-c3eff6988d9c, a25ab5bd-2bc0-4196-bc0a-6d620a445eb2, 9b3341e6-5d42-4424-a933-b3efb6cedb73, fcf768f2-5374-4695-93dd-32a9690c2591, 590eb7be-ad49-44bb-9bd7-4c
- **2026-03-13T15:14:33.712Z** — T002 → failed: HTTP 0 from Next.js at http://localhost:3002
- **2026-03-13T15:14:27.670Z** — T001 → done: Daemon healthy — status="ok"
<!-- Appended automatically by the runner. Do not edit manually. -->
