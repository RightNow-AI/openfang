# OpenFang v1.0 Roadmap

This document defines the roadmap from v0.4.4 (Agent OS feature-complete) to v1.0 (production-stable release).

It is intentionally different from the 12-phase Agent OS roadmap:
- the 12-phase roadmap focused on building the Agent OS capability model (routing, builder, extensions, catalog, tracing)
- this roadmap focuses on production hardening, multi-account operations, cross-provider parity, SDK completeness, and measurable performance

The core product direction remains:
- the user describes a goal
- OpenFang routes that goal to an existing capability
- if no capability exists, OpenFang helps design a new one with the user in the loop
- workflows become reusable compositions of those capabilities

What changes for v1.0 is the operational contract:
- no breaking API changes after v1.0
- every feature is measured and benchmarked
- every channel adapter supports multi-account dispatch
- decision traces survive daemon restarts
- SDKs cover the full API surface

## Principles

- Favor operational reliability over new features.
- Measure everything before calling it production-ready.
- Close every gap between what the API exposes and what the SDKs cover.
- Treat multi-account channel dispatch as a first-class operational requirement, not an afterthought.
- Persist all observability data — in-memory-only is a pre-1.0 compromise.
- Apply security review systematically across all endpoints before tagging v1.0.

## Current Progress Snapshot

- Phase 1 not started
- Phase 2 not started
- Phase 3 not started
- Phase 4 not started
- Phase 5 not started
- Phase 6 not started
- Phase 7 not started
- Phase 8 not started

## Phase 1: Multi-Bot Channel Dispatch

Goal: route incoming channel messages to the correct agent and bot token based on binding rules.

Scope:
- implement binding lookup in `channel_bridge.rs` that matches incoming messages against `AgentBinding` entries by channel type, account ID, and match rule
- select the correct agent ID and bot token per matched binding instead of falling back to `default_agent`
- support multiple bot tokens per channel type (e.g. two Telegram bots, three Discord bots) each bound to different agents
- add fallback behavior when no binding matches (use `default_agent` as today)
- expose binding match diagnostics via decision trace

Why:
- `add_binding`, `remove_binding`, and `list_bindings` exist on the kernel but the channel bridge does not consult them
- without dispatch logic, the binding framework is dead code in production
- multi-bot operation is a baseline expectation for any team deploying agents across channels

Affected modules:
- `crates/openfang-api/src/channel_bridge.rs`
- `crates/openfang-channels/src/bridge.rs`
- `crates/openfang-kernel/src/kernel.rs` (binding lookup query)
- `crates/openfang-types/src/config.rs` (AgentBinding, MatchRule)
- `crates/openfang-api/src/routes.rs` (binding CRUD endpoints if not yet wired)

Done when:
- an incoming Telegram message to bot token A routes to agent X while bot token B routes to agent Y
- binding specificity ordering is respected (exact match beats wildcard)
- fallback to `default_agent` works when no binding matches
- integration test covers multi-bot dispatch with two different bindings

## Phase 2: Multi-Account Channel Configuration

Goal: support multiple bot tokens per channel type in `config.toml`.

Scope:
- extend channel config schema to accept an array of accounts per channel type, each with its own token, agent binding, and display name
- migrate single-token `[channels.telegram]` format to `[[channels.telegram.accounts]]` array format with backward compatibility
- update channel adapter initialization to spawn one listener per account
- update dashboard Channels page to display per-account status and health

Why:
- Phase 1 adds dispatch logic but the config layer currently supports only one token per channel type
- teams running multiple bots on the same platform (e.g. support bot + sales bot on Telegram) need per-account configuration
- this is the config-level complement to Phase 1

Affected modules:
- `crates/openfang-types/src/config.rs` (channel config structs)
- `crates/openfang-channels/src/bridge.rs` (multi-account spawn)
- `crates/openfang-channels/src/adapters/` (adapter initialization per account)
- `crates/openfang-api/static/js/pages/comms.js` (dashboard Channels display)
- `docs/getting-started.md` (config examples)
- `docs/cli-reference.md` (channel status per account)

Done when:
- `config.toml` accepts both legacy single-token and new multi-account format
- two Telegram accounts with different tokens can run simultaneously
- dashboard shows per-account connection status
- backward compatibility test passes with old single-token configs

## Phase 3: OpenAI and Gemini Prompt Cache Parsing

Goal: parse and track prompt cache usage from OpenAI and Gemini response payloads.

Scope:
- parse `cache_creation_tokens` and `cache_read_tokens` fields from OpenAI chat completion responses in the `openai.rs` driver
- parse `cached_content_token_count` from Gemini `usageMetadata` response fields in the `gemini.rs` driver
- feed parsed cache token counts into the existing usage telemetry pipeline (cost estimation, per-model usage, `/api/usage` endpoints)
- add `cache_creation_tokens` and `cache_read_tokens` to `UsageEvent` struct
- expose cache hit rate as a metric on `/api/metrics`

Why:
- Anthropic `cache_control` is implemented (Phase 12) but OpenAI and Gemini cache data is silently discarded
- prompt caching can reduce costs by 50-90% on supported providers — invisible savings are wasted optimization opportunities
- cache hit rate is a key operational metric for cost management

Affected modules:
- `crates/openfang-runtime/src/drivers/openai.rs` (response parsing)
- `crates/openfang-runtime/src/drivers/gemini.rs` (response parsing)
- `crates/openfang-types/src/lib.rs` (UsageEvent fields)
- `crates/openfang-kernel/src/kernel.rs` (usage tracking aggregation)
- `crates/openfang-api/src/routes.rs` (cache metrics in usage endpoints)

Done when:
- OpenAI responses with cache fields populate usage telemetry correctly
- Gemini responses with cached content metadata populate usage telemetry correctly
- `/api/usage/summary` includes cache_creation_tokens and cache_read_tokens
- `/api/metrics` exposes prompt cache hit rate
- unit tests cover response payloads with and without cache fields

## Phase 4: SDK Completion

Goal: bring Python and JavaScript SDKs to full API coverage.

Scope:
- add missing endpoint wrappers to `openfang_client.py`:
  - `GET /api/routing/capabilities` — list routing capabilities
  - `GET /api/routing/decisions` — list decision traces
  - `POST /api/routing/proposals` — submit builder proposal
  - `POST /api/routing/proposals/apply` — apply approved proposal
  - `GET /api/extensions` — list extensions
  - `POST /api/extensions/install` — install extension
  - `DELETE /api/extensions/{name}` — uninstall extension
  - `GET /api/extensions/{name}` — extension details
  - `POST /api/backup` — create backup
  - `GET /api/backups` — list backups
  - `DELETE /api/backups/{filename}` — delete backup
  - `POST /api/restore` — restore from backup
  - `PUT /api/cron/jobs/{id}` — update cron job
- add the same wrappers to `sdk/javascript/index.js`
- add TypeScript type declarations for all new methods and response shapes in `sdk/javascript/index.d.ts`
- add integration tests for the new SDK methods in `sdk/python/tests/` and `sdk/javascript/test/`

Why:
- the SDKs currently cover agents, comms, models, cron (create/delete/list), and basic endpoints
- routing, decisions, extensions, backup/restore, and cron update are missing — these are all v0.4.4 features with no SDK coverage
- SDK completeness is a hard requirement for v1.0

Affected modules:
- `sdk/python/openfang_client.py`
- `sdk/python/tests/`
- `sdk/javascript/index.js`
- `sdk/javascript/index.d.ts`
- `sdk/javascript/test/`
- `sdk/javascript/package.json` (version bump)

Done when:
- every public API endpoint has a corresponding SDK method in both Python and JavaScript
- TypeScript declarations cover all new methods and response types
- SDK integration tests pass against a running daemon
- SDK README or docstrings document the new methods

## Phase 5: Performance Benchmarks

Goal: establish baseline performance numbers with reproducible benchmarks.

Scope:
- cold start benchmark: measure time from process spawn to first successful `/api/health` response
- message latency benchmark: measure end-to-end time for `/api/agents/{id}/message` with a mock LLM backend (isolate framework overhead from provider latency)
- throughput benchmark: concurrent agent message sends (10, 50, 100 simultaneous agents) measuring requests per second and p50/p95/p99 latencies
- memory under load benchmark: measure RSS growth over 1000 sequential messages across 10 agents
- routing benchmark: measure deterministic router dispatch time for 100 sequential goals against a full capability registry
- publish results as machine-readable JSON and human-readable markdown in `docs/benchmarks/`
- use `criterion` for micro-benchmarks (router, catalog lookup) and a custom harness for integration benchmarks (cold start, message latency)

Why:
- Phase 12 said "performance features are measured, not guessed" — no actual measurements exist
- the README claims <200ms cold start and 40MB idle memory but these are not reproduced by automated benchmarks
- v1.0 must ship with verifiable performance claims

Affected modules:
- new `benches/` directory at repo root (criterion benchmarks)
- new `tests/bench_integration.rs` or similar harness for integration benchmarks
- `docs/benchmarks/` (results and methodology)
- `crates/openfang-kernel/src/router.rs` (routing micro-benchmark target)
- `crates/openfang-runtime/src/model_catalog.rs` (catalog lookup micro-benchmark target)

Done when:
- `cargo bench` runs criterion benchmarks for router dispatch and catalog lookup
- integration benchmark script measures cold start, message latency, throughput, and memory
- results are committed as baseline in `docs/benchmarks/`
- CI runs benchmarks on every release tag (not on every PR)

## Phase 6: Catalog Hot-Reload

Goal: hot-reload TOML catalog files without daemon restart.

Scope:
- add file watcher on the `catalog/` directory using `notify` crate (or equivalent)
- on file change, parse the modified TOML file and merge into the live `ModelCatalog`
- handle parse errors gracefully: log the error, keep the previous catalog state
- emit a kernel event when catalog reloads successfully (for dashboard and metrics)
- add `/api/catalog/reload` endpoint for manual trigger
- update dashboard Settings page to show catalog last-reload timestamp

Why:
- `config.toml` already hot-reloads on a 30-second polling interval
- TOML catalog files (`catalog/providers.toml`, `catalog/models.toml`, `catalog/aliases.toml`) were added in Phase 11 but require a daemon restart to pick up changes
- operators adding new models or providers should not need downtime

Affected modules:
- `crates/openfang-runtime/src/model_catalog.rs` (reload logic, merge strategy)
- `crates/openfang-kernel/src/kernel.rs` (file watcher setup, event emission)
- `crates/openfang-api/src/routes.rs` (manual reload endpoint)
- `crates/openfang-api/src/server.rs` (route registration)
- `crates/openfang-api/static/js/pages/settings.js` (reload timestamp display)
- `Cargo.toml` (add `notify` dependency if not already present)

Done when:
- editing `catalog/models.toml` while the daemon is running causes the new model to appear in `/api/models` within 5 seconds
- a malformed TOML edit does not crash the daemon or wipe the existing catalog
- `/api/catalog/reload` triggers an immediate reload
- kernel event log records catalog reload events

## Phase 7: Decision Trace Persistence

Goal: persist decision traces to durable storage and expose paginated queries.

Scope:
- replace the in-memory `VecDeque<DecisionTraceEntry>` (currently capped at 500 entries) with SQLite-backed storage
- create `decision_traces` table with columns: id, timestamp, agent_id, message_preview, target, explanation, gap_detected, workflow_run_id
- update `record_decision_trace` to write to SQLite instead of the ring buffer
- update `list_decision_traces` to query SQLite with pagination (offset + limit)
- update `GET /api/routing/decisions` to accept `?offset=N&limit=N` query parameters
- link decision traces to workflow run IDs when the routed target is a workflow
- add `GET /api/routing/decisions/{id}` for single trace lookup
- add retention policy: auto-prune traces older than configurable days (default 30)

Why:
- the in-memory ring buffer loses all traces on daemon restart
- 500 entries is insufficient for production systems with high message volume
- decision traces are the primary debugging tool for routing behavior — losing them on restart defeats the purpose
- workflow correlation requires trace persistence to be useful across sessions

Affected modules:
- `crates/openfang-kernel/src/kernel.rs` (trace recording and querying)
- `crates/openfang-memory/` (SQLite schema migration for decision_traces table)
- `crates/openfang-api/src/routes.rs` (pagination parameters, single trace endpoint)
- `crates/openfang-api/src/server.rs` (route registration for new endpoint)
- `crates/openfang-types/src/lib.rs` or dedicated traces module (DecisionTraceEntry serde)

Done when:
- decision traces survive daemon restart
- `GET /api/routing/decisions?offset=0&limit=50` returns paginated results
- `GET /api/routing/decisions/{id}` returns a single trace
- traces older than the retention period are automatically pruned
- integration test verifies traces persist across daemon restart

## Phase 8: v1.0 Release Hardening

Goal: final security, stability, and compatibility pass before tagging v1.0.

Scope:
- security audit of all 170+ API endpoints: verify authentication requirements, input validation, rate limiting coverage, and RBAC enforcement
- add `X-OpenFang-API-Version: 1.0` response header on all endpoints
- document deprecation notices for any pre-1.0 endpoint shapes that will change
- write migration guide from v0.x to v1.0 covering config format changes (multi-account channels), SDK method renames, and any removed endpoints
- run full fuzzing pass on JSON request parsing (cargo-fuzz or afl)
- verify all 40 channel adapters initialize and shut down cleanly under error conditions
- verify backup/restore round-trip integrity with production-sized datasets
- update README version badge, stability notice, and benchmark claims with Phase 5 data
- tag v1.0.0 release with full changelog

Why:
- v1.0 is a stability contract — breaking changes require a major version bump after this point
- the 170+ endpoint surface has grown rapidly across 12 phases and needs a systematic security review
- API versioning headers enable clients to detect compatibility
- a migration guide is necessary for users upgrading from v0.x pinned commits

Affected modules:
- `crates/openfang-api/src/server.rs` (versioning middleware)
- `crates/openfang-api/src/routes.rs` (input validation audit)
- `crates/openfang-api/src/` (auth and RBAC enforcement review)
- `crates/openfang-channels/src/adapters/` (error handling review)
- `crates/openfang-kernel/src/backup.rs` (round-trip integrity test)
- `docs/migration-v1.md` (new migration guide)
- `docs/api-reference.md` (deprecation notices)
- `README.md` (version bump, benchmark data)
- `CHANGELOG.md` (v1.0.0 release notes)

Done when:
- every endpoint has documented auth requirements and validated inputs
- `X-OpenFang-API-Version: 1.0` header is present on all responses
- migration guide covers all breaking changes from v0.x
- fuzzing pass completes with zero panics on malformed input
- all existing tests pass (1,848+)
- v1.0.0 tag is created with full changelog

## Priority Summary

Multi-bot operations (immediate operational value):
- Phase 1: Multi-Bot Channel Dispatch
- Phase 2: Multi-Account Channel Configuration

Cross-provider parity and SDK coverage:
- Phase 3: OpenAI and Gemini Prompt Cache Parsing
- Phase 4: SDK Completion

Measurement and runtime improvements:
- Phase 5: Performance Benchmarks
- Phase 6: Catalog Hot-Reload

Persistence and release:
- Phase 7: Decision Trace Persistence
- Phase 8: v1.0 Release Hardening

## Notes

- Phases 1 and 2 are tightly coupled — Phase 1 adds the dispatch logic, Phase 2 adds the config layer. They can be implemented together or sequentially.
- Phase 4 (SDK Completion) should track the full endpoint list from `docs/api-reference.md` as the source of truth.
- Phase 5 benchmarks should be automated but not blocking on every PR — only on release tags.
- Phase 7 reuses the existing SQLite infrastructure from `openfang-memory` rather than introducing a new storage backend.
- Phase 8 is explicitly the last phase — no new features after this point, only hardening and documentation.
