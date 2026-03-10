# Phase 10 Blueprint: Production Hardening

**Date:** 2026-03-09  
**Status:** In Progress

---

## 1. Goal

This phase focuses on making the entire OpenFang system production-ready. This involves adding a comprehensive integration test suite, wiring up the observability and guardrails crates, improving health checks and graceful shutdown, and ensuring the CI/CD and Docker configurations are robust.

## 2. Task Breakdown & Status

| Task | Status | Description |
|---|---|---|
| **10.1** | ✅ Done | Integration Test Harness — 44 tests in `maestro-integration-tests` |
| **10.2** | ✅ Done | Hand Scheduler — cron/interval/one-shot scheduling in `openfang-hands` |
| **10.3** | ✅ Done | Full Async & Bug Fixes — eliminated all remaining blocking calls |
| **10.4** | 🟡 In Progress | Health & Readiness Probes — `/api/ready` endpoint |
| **10.5** | ⬜ To Do | Graceful Shutdown — wire `SIGTERM` into the kernel loop |
| **10.6** | ⬜ To Do | CI/CD & Docker — add integration tests to CI, create production Dockerfile |
| **10.7** | ✅ Done | Documentation Update — CHANGELOG, ROADMAP, README, blueprints |

---

### Task 10.1: Integration Test Harness ✅

**Goal:** Create a new, top-level integration test suite that treats the entire system as a black box, booting the full kernel with real dependencies and making API calls.

**Crate:** `crates/maestro-integration-tests/`

**Delivered:**
- 6 test files with 44 tests total.
- `kernel_boot.rs` (6 tests): Full kernel boot, guardrails wiring, default assistant spawn, agent registry.
- `hand_lifecycle.rs` (8 tests): HandRegistry bundled hands, activate/deactivate/pause/resume lifecycle.
- `guardrails_pipeline.rs` (6 tests): GuardrailEngine PII detection, topic blocking, policy enforcement.
- `observability_traces.rs` (6 tests): TraceStore record/retrieve/filter/stats.
- `eval_suite.rs` (8 tests): ScoringEngine, RegressionTracker, BenchmarkRunner.
- `algorithm_maestro.rs` (10 tests): ISC criteria generation and validation.

---

### Task 10.2: Hand Scheduler ✅

**Goal:** Implement a scheduler for autonomous `Hand` execution.

**Crate:** `openfang-hands`

**Delivered:**
- `HandScheduler` struct in `crates/openfang-hands/src/scheduler.rs`.
- Supports `Cron(String)`, `Interval { seconds }`, and `Once` schedule types.
- `schedule()`, `cancel()`, `pause()`, `resume()`, `next_fire_time()` methods.
- Wired into `OpenFangKernel` via the `openfang-hands` dependency.

---

### Task 10.3: Full Async & Bug Fixes ✅

**Goal:** Eliminate all remaining blocking calls and fix bugs found by the new integration tests.

**Delivered:**
- Replaced all synchronous `.expect()` and `.unwrap()` calls on `Future`s with `.await` across 10+ test files.
- Fixed `open_in_memory()` calls in `openfang-runtime` to use the new async `connect_in_memory().await`.
- Corrected the `kernel_boot` test to assert 1 initial agent (the default assistant), not 0.
- Resolved persistent `No space left on device` build errors by removing incremental build artifacts.

---

### Task 10.4: Health & Readiness Probes 🟡

**Goal:** Enhance the existing health check and shutdown mechanisms for production environments like Kubernetes.

**Remaining Tasks:**
1. Modify the `/health` endpoint in `openfang-api` to perform real dependency checks (ping SurrealDB, ping Redis).
2. Add a `/ready` endpoint for readiness probes (returns 200 only after the kernel is fully booted).

---

### Task 10.5: Graceful Shutdown ⬜

**Goal:** Ensure the kernel and all its components shut down cleanly on `SIGTERM`.

**Tasks:**
1. Wire the `graceful_shutdown` signal handler from `openfang-runtime` into the main `openfang-kernel` loop.
2. Ensure all background tasks, threads, and open connections are cleanly terminated.

---

### Task 10.6: CI/CD & Docker ⬜

**Goal:** Finalize the continuous integration pipeline and create a minimal Docker image for production deployment.

**Tasks:**
1. Add the new `maestro-integration-tests` suite to the `ci.yml` workflow.
2. Create a multi-stage `Dockerfile` that builds the project and copies the final binary into a minimal `distroless` or `alpine` base image.

---

### Task 10.7: Documentation Update ✅

**Goal:** Update all project documentation to reflect the new features and current status.

**Delivered:**
- Updated `CHANGELOG.md` with a detailed entry for `v0.3.32`.
- Updated `ROADMAP.md` to show Phase 10 as in progress, with detailed task statuses.
- Updated `README.md` with the new version (v0.3.32), phase badge, crate count (26), and test count (2,010+).
- Updated this blueprint to reflect the latest progress.
