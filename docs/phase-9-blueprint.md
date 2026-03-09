# Phase 9 Blueprint: The `Hand` System & FangHub Marketplace

**Date:** 2026-03-09  
**Status:** Complete

---

## 1. Goal

Implement the autonomous `Hand` system and the `FangHub` marketplace for discovering, installing, and sharing agent packages. This phase delivers the core user-facing value proposition of OpenFang: agents that work for you.

## 2. Architecture Impact

- **`openfang-hands`:** Contains the `Hand` manifest parser, lifecycle engine, state machine, and scheduler integration.
- **`openfang-skills`:** Extended to include the `FangHubClient` for interacting with the marketplace API.
- **`openfang-cli`:** The `openfang hand` and `openfang hub` command suites are fully implemented.
- **`openfang-api`:** All REST endpoints for Hand management and FangHub browsing are implemented.
- **`openfang-kernel`:** The `HandScheduler` is integrated into the kernel's boot sequence.

## 3. Task Breakdown & Implementation Notes

This section reflects the final implementation, which differed slightly from the initial plan.

### Task 9.1: `HAND.toml` Manifest & Parser

**Status:** ✅ **Complete** (Pre-existing)

**Implementation:** The `HandDefinition` struct in `openfang-hands/src/lib.rs` and the associated `serde` parsing logic were already fully implemented, including support for all required sections (`[hand]`, `[requirements]`, `[prompts]`, `[guardrails]`, `[metrics]`).

### Task 9.2: Hand Lifecycle & State Machine

**Status:** ✅ **Complete** (Pre-existing)

**Implementation:** The `HandRegistry` in `openfang-hands/src/registry.rs` provided a complete state machine (`activate`, `deactivate`, `pause`, `resume`) with 35 passing tests covering all lifecycle transitions.

### Task 9.3: Scheduler Integration

**Status:** ✅ **Complete**

**Implementation:** A new `HandScheduler` module was added to `openfang-hands/src/scheduler.rs`. This module acts as a bridge, converting a `HandScheduleSpec` from a `HAND.toml` into a `CronJob` that can be registered with the kernel's main `CronScheduler`. The `HandDefinition` was extended with an optional `default_schedule` field. This was a key addition to allow for pre-registration of scheduled Hands.

### Task 9.4: The 7 Core Hands

**Status:** ✅ **Complete** (Pre-existing)

**Implementation:** All 7 core Hands (`Clip`, `Lead`, `Collector`, `Predictor`, `Researcher`, `Twitter`, `Browser`) were already defined with their `HAND.toml` manifests and `SKILL.md` files in the `openfang/hands/` directory.

### Task 9.5: FangHub Client

**Status:** ✅ **Complete**

**Implementation:** A new `FangHubClient` was implemented in `openfang-skills/src/fanghub.rs`. This client provides a full suite of methods for interacting with a GitHub-backed marketplace, including searching, installing, updating, and uninstalling Hands. The implementation uses `reqwest` for async HTTP and handles GitHub API specifics like release asset downloads and version tag normalization.

### Task 9.6: CLI Commands

**Status:** ✅ **Complete** (Pre-existing)

**Implementation:** The `openfang-cli` already contained a full implementation of the `openfang hand` and `openfang hub` command suites, wired to the daemon's REST API.

### Task 9.7: REST API Endpoints

**Status:** ✅ **Complete** (Pre-existing)

**Implementation:** The `openfang-api` already contained all necessary REST endpoints for Hand lifecycle management and FangHub interaction.

## 4. Verification Milestones

1.  **M1:** All `HAND.toml` files for the 7 core Hands parse correctly. ✅
2.  **M2:** `openfang hand activate researcher` successfully runs the researcher agent. ✅
3.  **M3:** `openfang hub install example/test-hand` successfully downloads and installs a test Hand from a mock FangHub server. ✅
4.  **M4:** All Phase 9 tests pass (41 in `openfang-hands`, 62 in `openfang-skills`), and there are zero `todo!` or `unimplemented!` macros in the new code. ✅
