# Dimension 6 — Test Coverage by Risk

**Auditor:** Copilot CLI · **Date:** 2025-07-14
**Scope:** All 13 crates under `crates/`, with focus on high-risk paths (kernel, types, runtime)

---

## Summary

OpenFang has **1,797 test annotations** (`#[test]` + `#[tokio::test]`) across 13 crates, exceeding the claimed 1,767+ count. Test coverage is structurally excellent: **76.8% of source files** contain `#[cfg(test)]` blocks, and only 3 files in high-risk crates lack unit tests entirely. The project strongly favors **real-infrastructure integration tests** over mocking — API tests boot real Axum servers, kernel integration tests call real LLM APIs, and WASM tests run real Wasmtime engines. This is a deliberate architectural choice that produces high-confidence results but introduces some determinism risks. Key gaps include: no property-based testing (proptest/quickcheck), timing-dependent bridge tests with flakiness risk, zero tests for the desktop crate, and missing error module tests in kernel and types.

---

## Test Count by Crate

| Crate | Unit Tests | Integration Tests | Total | Risk Level | Assessment |
|---|---|---|---|---|---|
| openfang-runtime | 624 | 0 | **624** | 🔴 High | ✅ Excellent |
| openfang-channels | 346 | 9 | **355** | 🟡 Medium | ✅ Good |
| openfang-types | 265 | 0 | **265** | 🔴 High | ✅ Good |
| openfang-kernel | 205 | 15 | **220** | 🔴 High | ✅ Good |
| openfang-api | 36 | 31 | **67** | 🟡 Medium | ✅ Good |
| openfang-extensions | 54 | 0 | **54** | 🟡 Medium | ✅ Adequate |
| openfang-skills | 52 | 0 | **52** | 🟡 Medium | ✅ Adequate |
| openfang-memory | 40 | 0 | **40** | 🟡 Medium | ⚠️ Light |
| openfang-hands | 35 | 0 | **35** | 🟡 Medium | ⚠️ Light |
| openfang-migrate | 33 | 0 | **33** | 🟢 Low | ✅ Adequate |
| openfang-cli | 32 | 0 | **32** | 🟢 Low | ✅ Adequate |
| openfang-wire | 20 | 0 | **20** | 🟡 Medium | ⚠️ Light |
| openfang-desktop | 0 | 0 | **0** | 🟡 Medium | ❌ No tests |
| **TOTAL** | **1,742** | **55** | **1,797** | | |

---

## Findings

### TC1 · High-risk crates have strong unit test coverage — Severity: ✅ Strength

The three highest-risk crates (runtime: 624, types: 265, kernel: 220) account for **62% of all tests**. Within these crates, nearly every source file has inline `#[cfg(test)]` blocks. Only 3 files in high-risk crates lack tests:

- `openfang-kernel/src/error.rs` — error type definitions
- `openfang-types/src/error.rs` — error type definitions
- `openfang-runtime/src/kernel_handle.rs` — kernel handle wrapper

The error modules are low-complexity (enum definitions with `thiserror` derives), so absence of tests is acceptable. `kernel_handle.rs` is a thin delegation layer.

**Files examined:** All `crates/*/src/*.rs` files (228 total, 175 with tests = 76.8%)

---

### TC2 · Integration tests cover real E2E workflows — Severity: ✅ Strength

Integration tests exist in 3 crates with 8 test files totaling 55 test functions:

| File | Tests | What It Covers |
|---|---|---|
| `api/tests/api_integration_test.rs` | 17 | Real Axum server, agent CRUD, sessions, auth, error codes |
| `api/tests/daemon_lifecycle_test.rs` | 7 | Boot→health→shutdown, PID management, stale detection |
| `api/tests/load_test.rs` | 7 | 20 concurrent spawns, latency p50/p95/p99, Prometheus metrics |
| `kernel/tests/integration_test.rs` | 2 | Full pipeline with Groq, multi-model agents |
| `kernel/tests/multi_agent_test.rs` | 1 | Fleet of 6 agents with distinct roles |
| `kernel/tests/wasm_agent_integration_test.rs` | 8 | WASM hello/echo, fuel exhaustion, host calls, streaming |
| `kernel/tests/workflow_integration_test.rs` | 4 | Workflow registration, agent resolution, triggers |
| `channels/tests/bridge_integration_test.rs` | 9 | Channel bridge dispatch, routing, message delivery |

All API tests bind to random ports (`127.0.0.1:0`) and boot real servers — no port collision risk.

**Files examined:** `crates/openfang-api/tests/`, `crates/openfang-kernel/tests/`, `crates/openfang-channels/tests/`

---

### TC3 · Failure modes are well-tested in runtime and tool runner — Severity: ✅ Strength

Approximately **282 test assertions** across the codebase exercise error/failure paths. Standout modules:

- **`agent_loop.rs` (36 tests):** Empty response recovery, malformed JSON recovery, unknown tools, buffer overflow, max iterations, text tool call recovery (multiple variants including invalid JSON, missing brackets, mixed valid/invalid)
- **`tool_runner.rs` (41 tests):** Path traversal blocking (3 variants), HTML/XSS injection (11 tests — scripts, iframes, event handlers, JavaScript URLs, data URIs), unknown tools, capability enforcement (allowed/denied), invalid schedule input
- **`llm_errors.rs` (20 tests):** LLM error classification, retry logic, backoff calculations
- **`auth_cooldown.rs` (16 tests):** Cooldown state machine, expiry, concurrent access

**Files examined:** `crates/openfang-runtime/src/agent_loop.rs`, `tool_runner.rs`, `llm_errors.rs`, `auth_cooldown.rs`

---

### TC4 · Bridge integration tests have timing-dependent sleeps — Severity: 🟡 Medium

`crates/openfang-channels/tests/bridge_integration_test.rs` contains **9 instances** of `tokio::time::sleep` (7× 100ms, 1× 200ms, 1× 150ms) used to "give the async dispatch loop time to process." These are not testing timeouts — they're synchronization hacks that wait a fixed duration instead of using deterministic signaling.

**Flakiness risk:** On overloaded CI runners, 100ms may be insufficient for async task completion, causing intermittent failures. This is the single largest determinism risk in the test suite.

**Recommendation:** Replace fixed sleeps with `tokio::sync::Notify`, `tokio::sync::mpsc` channel waits, or `tokio::time::timeout` wrapping an event-driven wait.

**File:** `crates/openfang-channels/tests/bridge_integration_test.rs:225-534`

---

### TC5 · No property-based testing or fuzzing — Severity: 🟡 Medium

No `proptest`, `quickcheck`, `arbitrary`, or fuzzing tools are present in any `Cargo.toml`. For a system that parses untrusted input (LLM responses, JSON tool calls, user manifests, webhook payloads), property-based testing would significantly improve confidence in edge cases.

**High-value targets for proptest:**
- `openfang-types` — Agent manifests, config parsing, serde roundtrips (265 tests already exist, proptest would add generative coverage)
- `openfang-runtime/src/apply_patch.rs` — Patch application with arbitrary diffs
- `openfang-runtime/src/str_utils.rs` — String manipulation utilities
- `openfang-runtime/src/agent_loop.rs` — Tool call recovery from arbitrary malformed JSON

**Files examined:** All `Cargo.toml` files in workspace

---

### TC6 · Desktop crate has zero tests — Severity: 🟡 Medium

`openfang-desktop` (7 source files: `main.rs`, `lib.rs`, `commands.rs`, `server.rs`, `shortcuts.rs`, `tray.rs`, `updater.rs`) has **0 tests**. This is a Tauri desktop application where GUI testing requires specialized frameworks, but the non-GUI logic (commands, server, updater) could have unit tests.

**Recommendation:** Add unit tests for `commands.rs`, `server.rs`, and `updater.rs` — these contain testable business logic independent of the Tauri runtime.

**File:** `crates/openfang-desktop/src/`

---

### TC7 · No mocking framework — deliberate real-infra strategy — Severity: 🟢 Info

The project contains **zero mock frameworks** (no `mockall`, `mockito`, `wiremock`). All integration tests use real infrastructure: real HTTP servers, real kernel instances, real WASM runtimes. LLM-dependent tests are gated by API key availability.

This is a valid architectural choice that maximizes integration confidence but:
- Makes tests slower (full kernel boot per test)
- Makes some tests require external credentials (`GROQ_API_KEY`)
- Prevents testing LLM response edge cases without real API calls

**Trade-off is acceptable** given the project's focus on system-level correctness over unit isolation.

**Files examined:** All `Cargo.toml` dev-dependencies, test files for mock patterns

---

### TC8 · No ignored tests — all tests are active — Severity: ✅ Strength

Zero `#[ignore]` annotations found across the entire codebase. Every test runs by default, ensuring no test rot or forgotten disabled tests.

**Files examined:** All `*.rs` files in `crates/`

---

### TC9 · Test file organization follows Rust conventions — Severity: ✅ Strength

Tests follow standard Rust organization:
- **Unit tests:** Inline `#[cfg(test)] mod tests` in 175 source files (97% of tested files)
- **Integration tests:** Separate `tests/` directories in 3 crates (api, kernel, channels)
- **Temp file cleanup:** All tests use `tempfile::TempDir` with automatic Drop cleanup — no leaked temp directories
- **No test helpers scattered** — test utilities are local to each module

**Files examined:** Directory structure of all 13 crates

---

### TC10 · Wire crate has minimal tests for its risk profile — Severity: 🟡 Medium

`openfang-wire` handles peer-to-peer communication (messages, peer registry, protocol) but has only **20 tests** across 3 files. For a networking protocol crate, this is thin coverage — especially for:
- Message serialization edge cases
- Peer connection/disconnection lifecycle
- Protocol version negotiation
- Concurrent peer operations

**Recommendation:** Add tests for malformed messages, connection drops, and concurrent peer operations.

**Files examined:** `crates/openfang-wire/src/message.rs`, `peer.rs`, `registry.rs`

---

### TC11 · Memory crate has 40 tests but handles critical persistence — Severity: 🟡 Medium

`openfang-memory` manages session persistence, semantic search, knowledge storage, and memory consolidation with only **40 tests**. Given that data loss in this crate would corrupt agent memory:

- `session.rs` (11 tests) — adequate for session CRUD
- `semantic.rs` (7 tests) — light for vector search
- `substrate.rs` (5 tests) — light for storage backend
- `consolidation.rs` (2 tests) — very light for a data-mutating operation
- `knowledge.rs` (2 tests) — very light for knowledge graph operations

**Recommendation:** Prioritize additional tests for `consolidation.rs` and `knowledge.rs` — these mutate persistent data and have the highest risk of silent corruption.

**Files examined:** `crates/openfang-memory/src/*.rs`

---

## Strengths

1. **1,797 tests exceed the claimed count** — the test suite is actively growing
2. **76.8% file-level coverage** in source files with inline test modules
3. **Real-infrastructure integration tests** — no mocking means tests catch real wiring issues
4. **Strong failure mode coverage** in the highest-risk modules (agent_loop: 36 tests, tool_runner: 41 tests)
5. **Security-focused testing** — path traversal, XSS injection, HTML sanitization, capability enforcement
6. **Zero ignored tests** — no test rot
7. **Random port binding** — no port collision risk in parallel CI
8. **Automatic temp directory cleanup** via `tempfile::TempDir` Drop semantics
9. **Load/performance tests** with concurrent agent spawning and latency percentiles
10. **WASM integration tests** covering fuel exhaustion, missing modules, and host function calls
