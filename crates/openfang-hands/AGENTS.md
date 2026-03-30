<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-hands

## Purpose

Hands are curated, domain-complete agent configurations that users activate from a marketplace. Unlike chatbot agents (you converse with them), Hands work autonomously for you (you check in on results). A Hand bundles a pre-built agent configuration (system prompt, LLM model, tools, skills), user-configurable settings (STT provider, voice model, etc.), deployment requirements (binaries, API keys), and dashboard metrics. Nine bundled hands ship with OpenFang (browser, researcher, trader, lead, clip, collector, etc.).

## Key Files

| File | Description |
|------|-------------|
| `src/lib.rs` | Core types: `HandDefinition` (HAND.toml schema), `HandInstance` (active hand + agent), `HandCategory`, `HandSetting` (configurable options), `HandRequirement`, `resolve_settings()`. |
| `src/registry.rs` | `HandRegistry` — loads HAND.toml, activates/deactivates hands, spawns agent instances, tracks status. |
| `src/bundled.rs` | Bundled hand loader — 9 pre-built hands extracted from `bundled/` at runtime. Injects bundled skill content into agent. |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `bundled/` | 9 pre-built hand definitions (each is a subdirectory with `HAND.toml` + bundled skill files). Examples: browser, researcher, trader, lead, clip, collector, infisical-sync, predictor, twitter. |
| `src/` | Registry, bundled hand logic, types. |

## For AI Agents

### Working In This Directory

- **Adding a hand**: Create subdirectory in `bundled/` with `HAND.toml` that defines metadata, requirements, settings, embedded agent config, and dashboard metrics.
- **Extending registry**: Modify `registry.rs` — `HandRegistry::activate()`, `deactivate()`, `list()` are the main APIs.
- **Settings resolution**: Enhance `resolve_settings()` to support new setting types or validation rules.
- **Requirements checking**: Extend requirement validation (binary on PATH, env var set, API key provided).
- **Dashboard metrics**: Add new metric types or display formats to `HandMetric`.

### Testing Requirements

- Unit tests in each module.
- Test HAND.toml parsing: both flat and `[hand]` table formats.
- Test settings resolution: verify env var collection, prompt block generation.
- Test hand activation: mocked agent spawning, instance tracking.
- Test requirement checking: mock file/env lookups.
- Test bundled hand loading and manifest validation.

### Common Patterns

- `HandDefinition` is the HAND.toml schema — must include `[agent]` section with system prompt and model.
- Settings are typed: `Select` (dropdown with options), `Text` (free-form input), `Toggle` (on/off).
- Select options can have `provider_env` (e.g., `GROQ_API_KEY`) and `binary` (e.g., `whisper`) for "Ready" badges.
- Text settings can have `env_var` to expose their value to the agent subprocess.
- Requirements are checked at activation time; optional requirements don't block (report as "degraded" instead).
- `HandCategory` is used for marketplace browsing: Content, Security, Productivity, Development, Communication, Data, Finance, Other.
- `HandStatus` tracks instance state: Active, Paused, Error(msg), Inactive.

## Dependencies

### Internal

- `openfang-types` — shared types (AgentId, etc).

### External

- `serde`, `toml`, `serde_json` — HAND.toml parsing.
- `dashmap` — concurrent hand instance map.
- `uuid`, `chrono` — metadata, timestamps.

<!-- MANUAL: -->
