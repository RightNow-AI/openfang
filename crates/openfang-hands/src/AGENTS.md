<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-hands/src

## Purpose

Core Hand system for OpenFang: registry, loader, and spawner for long-running autonomous agents with persistent memory and scheduling. Bundles 9 hands (browser, lead, researcher, twitter, trader, etc.), spawns them as durable agent processes, manages their lifecycle, provides memory/schedule/knowledge APIs, and supports marketplace updates.

## Key Files

| File | Description |
|------|-------------|
| `lib.rs` | Core types: `Hand`, `HandConfig`, `HandInfo`, `HandSpawner`, error types. Public API. |
| `registry.rs` | Hand Registry: loads bundled hands, merges with installed state, provides query API (`list_all()`, `get_by_id()`, `spawn()`), lifecycle management. |
| `bundled.rs` | Compile-time embedded HAND.toml files (9 hands) + SKILL.md documentation. Loaded as const data. |

## For AI Agents

### Working In This Directory

- `HandRegistry` is the main API: call `load_bundled()` to initialize, then query via `get_by_id()`, `list_all()`, `spawn()`.
- `HandSpawner` spawns a Hand as a long-running agent: creates a new agent instance, initializes memory state, loads system prompt from HAND.toml, wires up tools and schedules.
- Each Hand is identified by ID (directory name in `bundled/`): `get_by_id("browser")` returns the Browser Hand config.
- Hand lifecycle: Available → Spawned → Running (with periodic scheduled tasks) → Stopped.
- Hands expose configurable `settings` from HAND.toml; settings are merged into the agent's system prompt.
- Dashboard metrics are queried from agent's `memory_store` using keys defined in `dashboard.metrics`.

### Testing Requirements

- Test registry: verify bundled hands load, `get_by_id()` returns correct config, list shows all 9.
- Test spawner: verify hand spawns as an agent, system prompt loads, tools are available.
- Test settings: verify settings from HAND.toml are passed to agent, UI can display and modify them.
- Test memory: verify agent can store/recall state via `memory_store`/`memory_recall`.
- Test scheduling: verify `schedule_create` calls work, schedules trigger the Hand's task on cron schedule.
- Test dashboard: verify metrics are pulled from memory and displayed correctly.
- Test lifecycle: verify hand can stop gracefully, resources are cleaned up.

### Common Patterns

- Hand ID is always lowercase with hyphens (e.g., "browser", "lead-hand", "twitter-hand").
- HAND.toml defines: id, name, description, category, icon, required dependencies, configurable settings, agent config, dashboard metrics.
- System prompt is large (5KB+) and detailed; it includes multi-phase workflows, error recovery, security guidelines.
- Hands use `memory_store` to persist state (e.g., `lead_hand_state`, `twitter_hand_posted_count`).
- Hands use `schedule_create` to set up recurring tasks (e.g., daily lead generation, hourly tweet posting).
- Hands can be paused by stopping their agent; state is preserved in memory.
- User settings override defaults in HAND.toml; changes are persisted in registry state.

## Dependencies

### Internal
- `openfang-types` — error types, config structures
- `openfang-agent` — agent spawning and lifecycle

### External
- **Parsing:** `serde_toml` — HAND.toml parsing
- **Data:** `serde`/`serde_json` — configuration serialization
- **Async:** `tokio` — spawning agents and background tasks
- **Utilities:** `uuid` — hand instance IDs, `chrono` — timestamps, `thiserror` — error types, `tracing` — logging

<!-- MANUAL: -->
