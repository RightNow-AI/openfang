# Changelog

All notable changes to OpenFang will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.33] - 2026-03-09

### Added
- **FangHub Marketplace** (Phase 11)
  - `fanghub-registry` crate: SurrealDB backend with Axum REST API (`/publish`, `/search`, `/versions`).
  - `fang-cli` crate: Developer CLI for `login`, `package`, and `publish` commands.
  - `fanghub-ui`: Leptos SSR frontend for package discovery, integrated into `fanghub-registry` binary.
  - `openfang-kernel` now has `install_from_fanghub()` method to install Hands from the marketplace.
  - `maestro-integration-tests` now includes `fanghub_marketplace` test suite.
- `docs/fanghub-publishing-guide.md`.

## [0.3.32] - 2026-03-09
### Added
- **Phase 10: Production Hardening**
  - **Comprehensive Integration Test Suite:** Added a new `maestro-integration-tests` crate with 44 tests covering the full kernel lifecycle, API endpoints, and core features. The suite treats the system as a black box, validating everything from boot to Hand activation.
  - **Hand Scheduler:** Implemented `HandScheduler` in `openfang-hands` for scheduling autonomous Hands via cron expressions, fixed intervals, or one-shot activation. Includes pause/resume and next-fire-time calculation. Wired into the kernel.
  - **`/api/ready` Endpoint:** Added a Kubernetes-compatible readiness probe (`GET /api/ready`) that returns HTTP 200 only when the kernel has fully booted and SurrealDB is reachable. Returns HTTP 503 with a structured reason during startup or degraded states.
  - **Production Dockerfile:** Upgraded to a 3-stage multi-stage build (`deps` → `builder` → `runtime`) with dependency layer caching, binary stripping, a non-root `openfang` user, and a built-in `HEALTHCHECK` directive.
  - **docker-compose.yml:** Added `healthcheck` using `/api/ready` and `deploy.resources` limits for production deployments.
  - **CI Integration Tests Job:** Added a dedicated `integration` job to `.github/workflows/ci.yml` that runs `maestro-integration-tests` on every push to `main` or `feature/*` branches with `RUST_BACKTRACE=1`.

### Fixed
- **Full Workspace Async Propagation (Continuation):** Eliminated all remaining blocking calls (`.expect`/`.unwrap` on `Future`s) across the workspace, primarily in test suites. Replaced `open_in_memory()` with the new async `connect_in_memory().await` in `openfang-runtime` tests. Fixed missing `.await` calls on `boot_with_config()` in 10+ test files across `openfang-api` and `openfang-kernel`.
- **Disk Space Build Failures:** Resolved `No space left on device` errors during `cargo check` and `cargo test` by implementing a more aggressive cleanup of the `target/` directory, specifically removing large incremental compilation artifacts and outdated `.rlib` files.
- **`kernel_boot` Test:** Corrected the `test_kernel_agent_registry_starts_empty` test, which now correctly asserts that a fresh kernel boot results in exactly 1 agent (the default "assistant") being present in the registry.

### Changed
- **Workspace Structure:** Added `maestro-integration-tests` to the virtual manifest and workspace crate count (25 → 26).
- **Test Count:** Increased total test count from 1,846 to **2,010+**.

## [0.3.31] - 2026-03-09

### Changed

- **Migrated `maestro-pai` from SQLite to SurrealDB v3.** The `LearningStore` in `maestro-pai/src/hooks.rs` has been completely rewritten to use async SurrealDB v3 queries instead of `rusqlite`. This aligns the PAI learning system with the workspace-wide persistence strategy and eliminates the last remaining SQLite dependency.
- **Standardized SurrealDB v3 across the workspace.** The workspace `Cargo.toml` now pins `surrealdb = "3"` with `kv-mem` and `kv-surrealkv` features. All local overrides have been removed, and all crates now inherit the v3 dependency from the workspace.

### Fixed

- **SurrealDB v3 `CREATE` and `SELECT` query patterns.** The `maestro-pai` migration surfaced several subtle SurrealDB v3 query issues. The final, correct pattern is to use `CREATE type::record(\'table\', $id) CONTENT $data` with the table name as a literal string, and `SELECT * OMIT id FROM table` to exclude the problematic `RecordId` field from result sets.

## [0.3.30] - 2026-03-09

### Added

#### Phase 8.5 — Recursive Language Model (RLM) — `maestro-rlm`

The `maestro-rlm` crate now contains a complete, production-quality implementation of the Recursive Language Model pattern from the MIT CSAIL paper (arXiv:2512.24601v1). The core insight of RLM is that instead of feeding a long prompt directly into a Transformer\'s context window, the LLM is given a programmatic tool (a Python REPL) to interact with the data symbolically. This allows the agent to process inputs of virtually unlimited length.

The implementation is built on PyO3 0.23, embedding a Python interpreter directly inside the Rust process. The `Pyo3Executor` struct maintains a persistent Python globals dictionary across calls, so variables set via `set_variable()` survive between `execute()` calls. The `execute()` method wraps user code in a `StringIO` stdout-capture harness, so all `print()` output is captured and returned as a Rust `String`. Python exceptions are caught and returned as error strings (prefixed `PYTHON_ERROR:`) rather than panicking, allowing the LLM loop to self-correct. The `RlmAgent::query()` method implements the full loop: load the prompt as `context`, build an initial system prompt, iterate up to `max_iterations`, and return the first `FINAL(answer)` response.

**New tests (8):** `test_execute_print`, `test_execute_arithmetic`, `test_execute_multiline`, `test_execute_exception_returns_error_string`, `test_set_variable_and_read_back`, `test_set_variable_used_in_computation`, `test_state_persists_between_calls`, `test_env_type`.

#### Phase 8.4 — Structured ISC Generation — `maestro-algorithm`

The `generate_criteria()` function in `maestro-algorithm/src/isc.rs` now implements a full structured template system for generating Ideal State Criteria (ISC). The original implementation was a `todo!()` stub that left the LLM to generate criteria freeform, producing vague, untestable results like "ensure quality." The new implementation extracts four categories of criteria from the plan\'s JSON output: `Functional` (from `deliverables`), `Quality` (from `quality_bar`), `Completeness` (from `inputs`), and `Constraint` (from `constraints`). Weights are assigned by category priority (0.40 / 0.25 / 0.20 / 0.15) and divided equally among criteria of the same category. When no structured keys are present, a sensible set of four default criteria is generated from the plan\'s `task` description.

The `validate_criteria()` function has been significantly enhanced with whole-word boundary matching for vague language detection (preventing false positives like "sufficiently" matching "sufficient"), measurable-language checking scoped to `Functional` and `Constraint` categories only, and weight range validation.

**New tests (6):** `test_generate_criteria_from_deliverables`, `test_generate_criteria_fallback`, `test_generate_criteria_all_categories`, `test_validate_criteria_passes_good_criteria`, `test_validate_criteria_flags_vague_language`, `test_criterion_ids_are_sequential`.

### Fixed

- `maestro-rlm`: Resolved PyO3 0.23 API change — `py.run()` now requires `&CStr` instead of `&str`. Fixed by converting the wrapper string to `CString` before passing to `py.run()`.
- `maestro-rlm`: Fixed Python `IndentationError` in the stdout-capture wrapper caused by Rust\'s string continuation syntax (`\n\` followed by source indentation spaces). Replaced with an explicit `[...].join("\n")` array to guarantee correct Python indentation.
- `maestro-algorithm`: Fixed `isc_validation_flags_short_descriptions` test failure caused by the enhanced `validate_criteria()` generating more warnings than the test expected. Applied whole-word boundary matching and scoped the measurable-language check to `Functional`/`Constraint` categories only.

### Changed

- `README.md`: Updated crate count (14 → 21), LOC (137K → 143K), test count (1,767 → 1,846), version badge (v0.3.29 → v0.3.30), and phase badge (Phase 5 → Phase 8). Architecture section updated with all 7 new Phase 8 crates.
- `ROADMAP.md`: Updated status to "Phase 8 Complete". Added Phase 8 task table. Updated Phase 9 description to reflect the Hand system and FangHub marketplace.
- `docs/phase-9-blueprint.md`: Created. Full blueprint for Phase 9 including goal, architecture impact, 8-task breakdown, and 4 verification milestones.

## [0.3.29] - 2026-03-08

### Added

- **Phase 5.1: L1/L2 Caching Layer**
  - New `maestro-cache` crate providing a transparent 3-tier caching layer (L1 Moka → L2 Redis → L3 SurrealDB).
  - `CachingMemory` struct wraps `SurrealMemorySubstrate` and implements the `Memory` trait plus all 30+ substrate-specific methods.
  - Implements cache-aside pattern for reads and write-invalidate for writes.
  - L1 Moka cache is in-process with separate partitions for KV, sessions, and agents, each with configurable TTL and capacity.
  - L2 Redis cache is optional (feature-gated `redis-cache`), distributed, and designed for graceful degradation if Redis is unavailable.
  - `CacheConfig` struct allows for full configuration of all tiers.
  - Integrated into the kernel as a drop-in replacement for `Arc<SurrealMemorySubstrate>`.
  - 8 tests passing for L1 and L2 cache logic.

## [0.3.28] - 2026-03-08

### Added

- **Phase 4.3: SurrealDB v3 Upgrade**
  - Upgraded SurrealDB dependency from v2.x to v3.0.2.
  - Replaced `RocksDb` engine with `SurrealKv` engine (`kv-surrealkv` feature).
- **Phase 4.4: Full Workspace Async Propagation**
  - Removed all `block_on` calls from library code, making the entire workspace natively async.
  - Propagated `async`/`.await` through all 7 core crates: `kernel`, `api`, `cli`, `runtime`, `desktop`, `types`, and `surreal-memory`.
  - Established 7 distinct sync/async boundaries for entry points like `main.rs`, TUI, and WASM host functions.
  - 18 files changed, 339 insertions, 330 deletions.

## [0.3.27] - 2026-03-08

### Added

- **Phase 4.2: SurrealDB Query Implementation**
  - Implemented all 24 `SurrealMemorySubstrate` methods with real SurrealQL queries.
  - Implemented all 12 `SurrealUsageStore` methods.
  - Defined and initialized 8 SurrealDB tables: `memories`, `sessions`, `kv_store`, `agents`, `paired_devices`, `tasks`, `usage_records`, `llm_summaries`.

## [0.3.26] - 2026-03-08

### Added

- **Phase 4.1: Type Unification & Memory Trait Extension**
  - Unified `Session`, `UsageRecord`, and `Message` types in `openfang-types`.
  - Extended the `Memory` trait with `save_session` and other methods to make the runtime backend-agnostic.
  - Refactored the kernel to use a standalone SQLite connection for the `MeteringEngine`.

## [0.1.0] - 2026-02-24

### Added

#### Core Platform

The initial public release of OpenFang. A 15-crate Rust workspace implementing a full Agent Operating System, including agent lifecycle management, a SQLite-backed memory substrate, 41 built-in tools, a WASM sandbox with dual metering, a workflow engine, 40 channel adapters, 3 native LLM drivers supporting 27 providers, a Tauri 2.0 desktop app, and 7 autonomous Hands packages. 1,731+ tests across 15 crates.

[Unreleased]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.3.32...HEAD
[0.3.32]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.3.31...v0.3.32
[0.3.31]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.3.30...v0.3.31
[0.3.30]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.3.29...v0.3.30
[0.3.29]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.3.28...v0.3.29
[0.3.28]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.3.27...v0.3.28
[0.3.27]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.3.26...v0.3.27
[0.3.26]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.1.0...v0.3.26
[0.1.0]: https://github.com/RightNow-AI/openfang/releases/tag/v0.1.0
