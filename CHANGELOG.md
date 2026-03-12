
All notable changes to OpenFang will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.41] - 2026-03-12
### Fixed
- **Critical Security Fixes**
  - **Command Injection Prevention:** Implemented command whitelist (`ALLOWED_COMMANDS`) with 30+ safe commands, blocked command list for dangerous operations (`rm`, `curl`, `sudo`, etc.), validation for shell operators (`&&`, `||`, `|`, `;`) and output redirection (`>`, `>>`)
  - **Path Traversal Prevention:** Added path validation with sandboxing to working directory, normalization of `.` and `..` components, blocking of absolute paths outside sandbox
  - **Error Handling:** Fixed silent error swallowing in `WriteFile` action, added `FileReadFailed`, `FileWriteFailed`, `CommandBlocked`, `CommandTimedOut`, `PathBlocked` event variants
  - **Channel Safety:** Replaced `.unwrap()` with proper error handling in A2A engine, added `A2AError` enum with descriptive error variants
  - **CVE Fixes:** Updated `quinn-proto` to 0.11.14 (fixes RUSTSEC-2026-0037), updated `pyo3` to 0.24.2 (fixes RUSTSEC-2025-0020), updated `wasmtime` to 41.0.4 (multiple CVE fixes)

### Changed
- **Async I/O Migration:** Migrated `SWEAgentExecutor` from `std::fs` to `tokio::fs` for async file operations
- **Executor API:** Added `execute()` async method and `execute_sync()` wrapper for backwards compatibility
- **A2A Protocol:** Extended `SWEAgentEvent` enum with error variants for proper error propagation

### Security
- Added 17 new security tests covering command injection, path traversal, and error handling scenarios
- Zero clippy warnings after security fixes
- Risk score reduced from 32/100 (MEDIUM-HIGH) to ~15/100 (LOW)

## [0.3.40] - 2026-03-11
### Added
- **SWE Agent Framework (Phase 17-18)**
  - **SWE API Endpoints:** New `/api/swe/tasks` route family supporting `POST / GET / DELETE` with full task lifecycle for `SWEActionRequests` (ReadFile, WriteFile, ExecuteCommand), `SWEAgentEvents`, and task status management
  - **SWE Dashboard Integration:** New "Software Engineer" tab in WebChat UI with real-time progress, task queue visualization, and event streaming
  - **A2A SWE Handler System:** Created `a2a_registry` and `swe_a2a_handler` modules enabling Agent-to-Agent SWE task delegation with direct handler pattern bypassing network transport
  - **Supervisor SWE Integration:** Added `TaskType::SWE` classification in SupervisorEngine with `classify_task()` and hybrid keyword/LLM detection (`"code", "implement", "fix", "debug", "refactor", "test"` patterns)
  - **Autonomous SWE Routing:** SupervisorEngine now delegates SWE tasks directly by detecting SWE patterns using `TaskType::SWE` and routing to SWE agents
  - **Direct SWE API:** New explicit `/api/supervisor/delegate` endpoint allowing manual SWE task routing
  - **maestro-swe crate:** Added Software Engineering Agent functionality with file operations, command execution, and execution events (`SWEAgentAction`, `SWEAgentEvent`)
  - **SWE Protocol Types:** Enhanced A2A payload with `SWETaskRequest` and `SWETaskResponse` variants
  - **Task In-Memory Store:** Added `SWETaskStore` type (`Arc<RwLock<HashMap...>>`) with full CRUD endpoints in `swe_routes.rs`
  - **Real-time Event Streaming:** Added `/api/swe/tasks/{id}/events` endpoint with `content_preview` (200 char excerpt)
  - **Cancellation Support:** POST `/api/swe/tasks/{id}/cancel` endpoint plus supervisor cancellation integration 
  - **Task Status Tracking:** `SWETaskStatus::{Pending, Running, Completed, Failed, Cancelled}` with auto-update during execution
  - **Execution Events:** Full event tracking (`FileRead`, `FileWritten`, `CommandExecuted`) with preview data and error handling

### Changed
- `openfang-kernel` now has `SweA2AHandler` wired into `A2AHandlerRegistry` via `start_background_agents()`
- `openfang-api` now routes `/api/swe/*` and `/api/supervisor/delegate` endpoints to new handlers
- `openfang-api` now includes `swe_routes` module for SWE-specific API handling
- `maestro-swe` enums (`SWEAgentAction`, `SWEAgentEvent`) now `derive(Clone, Deserialize, Serialize)`
- SupervisorEngine orchestrate flow now checks `self.classify_task(task, capabilities)` before MAESTRO pipeline for SWE detection
- `a2a_engine: None` field supplemented with new `a2a_handler_registry: None` field in kernel

### Added
- **SWE Evaluation Suite (Phase 18.4)**
  - **SWE Test Types:** New `SWETestCase`, `SWETaskType`, `SWEDifficulty`, `SWETestInput`, `SWETestExpectedOutput`, `SWETestResult`, `SWETestSuite`, `SWESuiteReport` types in `maestro-eval/src/swe.rs`
  - **SWE Test Runner:** `SWETestRunner` in `maestro-eval/src/swe_runner.rs` executes test cases using `SWEAgentExecutor` with validation logic for file creation, content patterns, command outputs, and compilation checks
  - **Pre-defined Test Suites:** Four difficulty-based suites in `maestro-eval/src/swe_suites.rs`:
    - `basic` (5 tests): File read/write, command execution
    - `intermediate` (5 tests): Multi-file operations, code generation basics
    - `advanced` (4 tests): Code generation, bug fixing, refactoring
    - `expert` (3 tests): Project setup, trait implementation, lifetime fixes
  - **Evaluation API Endpoints:** New `GET /api/swe/evaluate?suite=basic|intermediate|advanced|expert` and `GET /api/swe/evaluate/suites` endpoints
  - **Dashboard Evaluation UI:** Suite selector dropdown, run button with loading state, results display (passed/failed/score/duration), and test results table in SWE page
  - **Validation-based Scoring:** Score calculated from passed validation checks (0.0-1.0), with 0.8 threshold for pass
  - **Setup/Cleanup Commands:** Each test case can define shell commands for environment setup/teardown
- **Integration tests:** SWE ↔ Supervisor Engine communication, A2A message passing with event streaming
- **Security:** Proper sandboxing for ExecuteCommand actions, read/write file path sanitization, command execution whitelisting
- **Monitoring:** SWE task metrics in observability stack, cost accounting for file/command operations

## [0.3.39] - 2026-03-11
### Added
- **A2A Registry & Handler System**
  - **A2AHandlerRegistry:** Central handler dispatcher mapping agent types (`"swe"`, `"mcp"`, etc.) to concrete handlers with direct method dispatch 
  - **A2AHandler trait:** Async `handle_message()` interface allowing in-process message handling without network serialization
  - **Direct A2A Pattern:** Bypasses network transport using handler registry for local agent communication (`SweA2AHandler`, `McpA2AHandler`)
  - **Supervisor-SWE Wiring:** Automatic delegation using `SWEA2AHandler` and classification system from Phase 15-17

### Changed
- A2A engine rewritten to support mixed local/remote agent routing via `kernel.a2a_handler_registry`
- Internal A2A dispatch now preferentially calls direct handler methods over serialized network transport
- MCP and SWE engines migrated to handler pattern (breaking change: network transport bypassed for local agents)
- SupervisorAgent now checks `kernel.a2a_handler_registry.get(agent_type)` before falling back to standard A2A network protocols

## [0.3.38] - 2026-03-10
### Added
- **MAESTRO Algorithm Enhancement** (Phase 16)
  - **AlgorithmExecutor Refactor:** Centralized orchestration logic with configurable phase thresholds and adaptive parameter tuning
  - **Parallel EXECUTE Enhancement:** `max_parallel_steps` in AlgorithmConfig controlling concurrent agent execution during EXECUTE phase when `task.parallelizable == true`
  - **Dynamic Phase Adaptation:** Automatic threshold updates based on historical performance (`OrchestrateTask` satisfaction ratings, cost efficiency)
  - **MAESTRO Pipeline:** Full 7-phase integration (PLAN > OBSERVE > ORIENT > DECIDE > EXECUTE > EVALUATE > LEARN) with intermediate status updates
  - **Phase-Specific Metrics:** Individual timing, token cost, and model attribution for each MAESTRO phase 

### Changed
- `maestro-algorithm` now supports adaptive model selection and parallel step execution based on task characteristics
- ExecutionHooks extended with phase-specific callbacks (`on_phase_start`, `on_phase_complete`, `on_phase_retry`)
- `maestro-knowledge` RAG integrated into ORIENT phase for contextual awareness during strategy planning

## [0.3.37] - 2026-03-10
### Added
- **Supervisor Engine Overhaul**
  - **Task Classification System:** Hybrid keyword matching + machine learning classification for routing tasks (`SweTask`, `ResearchTask`, `GeneralTask`) 
  - **Auto-Delegation Pipeline:** Supervisor now routes known task types (code generation, research, lead gen) to specialized agents without human intervention
  - **Orchestration Metrics:** Added per-task efficiency metrics (cost per satisfaction score, tokens per successful outcome, time-to-answer)
  - **Adaptive Planning:** Dynamic phase threshold adjustment based on complexity, past task success rates, and agent availability
  - **Knowledge Graph Integration:** Cross-linking results between related tasks with automatic fact consolidation and entity disambiguation
  - **Real-time Status:** WebSocket streaming of `PhaseProgress` events with current agent utilization and task bottlenecks

### Added
- **Agent Auto-Selection** (Phase 15)
  - **Capability Mapping:** Agent-tool compatibility matrix with constraint solving for optimal agent assignment
  - **Load Balancing:** Distributed workload routing with round-robin and cost-optimization strategies
  - **Fallback Routing:** Automatic task rerouting when primary agents unavailable with seamless state restoration
  - **Capacity Planning:** Multi-dimensional resource accounting (GPU memory, token quota, execution time) for task prioritization

[Unreleased]


## [0.3.36] - 2026-03-10
### Added
- **Desktop & UI Polish** (Phase 13)
  - **FangHub page:** New page in the SPA dashboard for browsing and installing Hands from the FangHub marketplace.
  - **Mesh page:** New page in the SPA dashboard for Multi-Agent Mesh management (peer list, connect peer, route log).
  - **Tauri desktop commands:** New `install_from_fanghub`, `list_mesh_peers`, and `connect_mesh_peer` commands expose Phase 11/12 features to the desktop UI.
  - **10 new integration tests** in `desktop_ui.rs` covering the new API routes and SPA page content.

### Changed
- `openfang-api` now has `/api/mesh/*` and `/api/fanghub/*` routes for the SPA dashboard.
- `openfang-kernel` now has a `connect_peer` method that handles the `self_arc` internally.
- `openfang-desktop` `connect_mesh_peer` command rewritten to use `Arc::clone` and avoid move errors.
- Workspace version bumped from `0.3.35` to `0.3.36`.


## [0.3.35] - 2026-03-10
### Added
- **Multi-Agent Mesh** (Phase 12)
  - **Parallel EXECUTE phase:** `maestro-algorithm` now runs parallelizable steps concurrently via `tokio::task::JoinSet`, respecting `step.parallelizable` and `max_parallel_workers`.
  - **`openfang-mesh` crate:** New crate providing `MeshRouter` (routes tasks to local agents, Hands, or remote OFP peers) and `MeshClient` (sends tasks to remote peers via OFP wire protocol).
  - **A2A per-agent routing:** `a2a_send_task` now reads an optional `agentId` from `params` to route to a specific agent.
  - **A2A SSE streaming:** New `POST /a2a/tasks/sendSubscribe` endpoint returns a Server-Sent Events stream of task progress.
  - **Per-agent A2A cards:** New `GET /a2a/agents/{id}` endpoint for per-agent card discovery.
  - **8 new integration tests** in `multi_agent_mesh.rs` covering parallel execution, A2A routing, and SSE streaming.

### Changed
- `maestro-algorithm` `ExecutionHooks` trait now uses `Arc<dyn ExecutionHooks>` to support `Send + Sync + 'static` for parallel dispatch.
- `AlgorithmExecutor` now has a `'static` bound on its `H` generic parameter.
- `openfang-api` `a2a_send_subscribe` handler rewritten to use `Arc<OpenFangKernel>` and `futures::StreamExt` to fix compilation errors.
- Workspace version bumped from `0.3.34` to `0.3.35`.


## [0.3.34] - 2026-03-10

### Added
- **Real cost tracking in `maestro-algorithm`:** `AlgorithmResult::total_cost_usd` is now computed from actual token usage via a self-contained `estimate_cost()` function. Uses a 25-model pricing table (Anthropic, OpenAI, Gemini, DeepSeek, Llama, Mistral, Grok, and more) with a conservative 70/30 input/output blended rate. No new crate dependencies required.
- **5 new unit tests** in `maestro-algorithm::tests` verifying cost math for `gpt-4o`, `claude-3-5-haiku`, `gpt-4o-mini`, unknown models, and linear scaling.
- **Phase 1-3 architecture documentation:** `ARCHITECTURE.md` now includes full narrative sections for Phase 1 (Codebase Fusion), Phase 2 (Async Migration), and Phase 3 (Wire Protocol & Channels).

### Changed
- `ARCHITECTURE.md` restructured to cover all 11 phases with consistent depth and narrative prose (708 lines).
- `maestro-development` skill updated to version `0.3.34`.
- Workspace version bumped from `0.3.33` to `0.3.34`.

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

[Unreleased]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.3.40...HEAD
[0.3.40]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.3.39...v0.3.40
[0.3.39]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.3.38...v0.3.39
[0.3.38]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.3.37...v0.3.38
[0.3.37]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.3.36...v0.3.37
[0.3.36]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.3.35...v0.3.36
[0.3.35]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.3.34...v0.3.35
[0.3.34]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.3.33...v0.3.34
[0.3.33]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.3.32...v0.3.33
[0.3.32]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.3.31...v0.3.32
[0.3.31]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.3.30...v0.3.31
[0.3.30]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.3.29...v0.3.30
[0.3.29]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.3.28...v0.3.29
[0.3.28]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.3.27...v0.3.28
[0.3.27]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.3.26...v0.3.27
[0.3.26]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.1.0...v0.3.26
[0.1.0]: https://github.com/RightNow-AI/openfang/releases/tag/v0.1.0
