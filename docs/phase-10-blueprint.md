# Phase 10 Blueprint: Production Hardening

**Date:** 2026-03-09  
**Status:** In Progress

---

## 1. Goal

This phase focuses on making the entire OpenFang system production-ready. This involves adding a comprehensive integration test suite, wiring up the observability and guardrails crates, improving health checks and graceful shutdown, and ensuring the CI/CD and Docker configurations are robust.

## 2. Task Breakdown

### Task 10.1: Integration Test Harness

**Goal:** Create a new, top-level integration test suite that treats the entire system as a black box, booting the full kernel with real dependencies and making API calls.

**Crate:** `tests/` (a new virtual crate at the workspace root)

**Tasks:**

1.  Create a `tests/` directory with a `Cargo.toml` and a `main.rs` or `lib.rs`.
2.  Add a `harness.rs` module that programmatically starts and stops `surrealdb`, `redis`, and the `openfang-api` daemon.
3.  Write a test that boots the full kernel, makes an API call to spawn an agent, sends a chat message, and verifies the reply.
4.  Write a test that activates a `Hand`, checks its status, and deactivates it.
5.  Write a test that creates a user, assigns roles, and tests RBAC permissions on API endpoints.
6.  Add this new test suite to the `ci.yml` workflow.

### Task 10.2: Health Checks & Graceful Shutdown

**Goal:** Enhance the existing health check and shutdown mechanisms to be more robust.

**Crates:** `openfang-api`, `openfang-kernel`

**Tasks:**

1.  Modify the `/health` endpoint in `openfang-api` to perform real dependency checks (ping SurrealDB, ping Redis).
2.  Add a `/ready` endpoint for readiness probes (returns 200 only after the kernel is fully booted and all systems are go).
3.  Wire the `graceful_shutdown` signal handler from `openfang-runtime` into the main `openfang-kernel` loop, ensuring all background tasks and threads are cleanly terminated on `SIGTERM`.

### Task 10.3: Observability & Guardrails Wiring

**Goal:** Connect the `maestro-observability` and `maestro-guardrails` crates to the core application logic.

**Crates:** `openfang-kernel`, `openfang-api`, `openfang-runtime`

**Tasks:**

1.  In `openfang-kernel`, add `ObservabilityEngine` and `GuardrailEngine` to the `OpenFangKernel` struct.
2.  Initialize both engines during `boot_with_config()`.
3.  In `openfang-api`, add Axum middleware to trace all incoming HTTP requests using the `ObservabilityEngine`.
4.  In `openfang-runtime`, before sending a prompt to the LLM, pass it through the `GuardrailEngine` to scan for PII and other policy violations.
5.  After receiving a response from the LLM, record the token usage and cost using the `ObservabilityEngine`.
6.  Add new integration tests to verify that traces are created and guardrails can block a prompt.

### Task 10.4: Documentation & Final Polish

**Goal:** Update all documentation to reflect the production-ready state of the system.

**Tasks:**

1.  Update `README.md` with the new integration test suite and production-readiness features.
2.  Update `CONTRIBUTING.md` with instructions on how to run the new integration tests.
3.  Update `ROADMAP.md` to mark Phase 10 as complete.
4.  Create a new `v0.3.32` release with a comprehensive changelog.
