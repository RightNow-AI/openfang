<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-hands/bundled

## Purpose

9 bundled AI Hand definitions for autonomous agents with persistent capabilities. Each Hand is a HAND.toml configuration file + SKILL.md documentation bundle. Hands are long-running autonomous agents that spawn from a user prompt, manage schedules, persist memory, and interact with external tools.

## Hands

| ID | Name | Category | Purpose |
|----|------|----------|---------|
| `browser` | Browser Hand | Productivity | Autonomous web browser — navigates sites, fills forms, searches products, completes multi-step web tasks with purchase approval gates |
| `clip` | Clipboard Hand | Productivity | Monitor clipboard, extract structured data, auto-copy to cloud, trigger workflows on clipboard change |
| `collector` | Collector Hand | Data | Autonomous web scraping — discovers data sources, extracts structured records, deduplicates, delivers datasets on schedule |
| `infisical-sync` | Infisical Sync Hand | Security | Sync secrets from Infisical vault to OpenFang vault, auto-rotate credentials, sync on schedule |
| `lead` | Lead Hand | Sales/Data | Autonomous lead generation — discovers qualified leads via web search, enriches with company/person data, deduplicates, delivers reports on schedule |
| `predictor` | Predictor Hand | Analytics | Time-series forecasting agent — collects historical data, trains models, generates forecasts, tracks accuracy, triggers alerts on anomalies |
| `researcher` | Researcher Hand | Research | Deep research agent — web search, fetch, analysis, knowledge graph building, multi-source synthesis, generates research reports |
| `trader` | Trader Hand | Finance | Portfolio analysis — monitors positions, prices, signals; executes trades with user approval, tracks P&L, manages risk |
| `twitter` | Twitter Hand | Communication | Autonomous Twitter/X manager — content creation, scheduled posting, engagement handling, performance tracking, approval mode |

## Structure

Each Hand directory contains:

```
hand-name/
  HAND.toml    — Configuration: tools, settings, agent config, system prompt, dashboard metrics
  SKILL.md     — Documentation: usage patterns, examples, security guidelines
```

## HAND.toml Format

```toml
id = "hand-id"
name = "Display Name"
description = "One-line purpose"
category = "category-name"
icon = "emoji"

# Required binaries/services
[[requires]]
key = "dependency-id"
label = "Human-readable label"
requirement_type = "binary" | "api_key" | "service"
check_value = "command to verify"
optional = false

# Configurable settings
[[settings]]
key = "setting_id"
label = "Display Label"
description = "Help text"
setting_type = "text" | "select" | "toggle"
default = "default_value"

# Agent configuration
[agent]
name = "agent-name"
description = "Agent description"
module = "builtin:chat"
provider = "default"
model = "default"
max_tokens = 16384
temperature = 0.3
max_iterations = 50
system_prompt = """Agent system prompt..."""

# Dashboard metrics
[[dashboard.metrics]]
label = "Metric Name"
memory_key = "memory_store_key"
format = "number" | "text" | "percentage"
```

## For AI Agents

### Working In This Directory

- Hands are loaded at runtime by `openfang-hands` crate via `bundled.rs`.
- Each Hand's HAND.toml is parsed into `HandConfig` struct, which includes agent config, tools, settings, and dashboard metrics.
- Hand ID is the directory name (e.g., `browser` → Hand ID is "browser").
- Users trigger a Hand by name; the runtime spawns it as a long-running agent with persistent memory.
- Settings from HAND.toml are exposed in the UI and passed to the agent's system prompt.
- Dashboard metrics pull from `memory_store` using the configured memory keys.

### Adding a New Hand

1. Create a new directory: `crates/openfang-hands/bundled/hand-name/`
2. Create `HAND.toml` with all required sections (id, name, settings, agent, dashboard)
3. Create `SKILL.md` with usage documentation and examples
4. Test: verify TOML parses, settings appear in UI, agent spawns correctly

### Testing Requirements

- Verify HAND.toml parses correctly (valid TOML, required fields present)
- Verify agent spawns when Hand is triggered (system prompt loads, tools are available)
- Verify settings are passed to agent correctly (UI shows settings, agent receives them)
- Test all configured tools: verify agent can call them successfully
- Test memory persistence: agent should resume state across restarts
- Test dashboard metrics: verify memory keys are populated and displayed
- Test schedule integration (if Hand uses `schedule_create`): verify schedules trigger correctly

### Common Patterns

- Hands use `memory_store` and `memory_recall` to persist state across sessions
- Settings are exposed in UI via `setting_type`: text, select, toggle
- System prompts are large and detailed; they include phases, patterns, guidelines
- Hands integrate with external APIs: `web_search`, `web_fetch`, `shell_exec`, etc.
- All file I/O uses `file_read`, `file_write`, `file_list` tools
- Scheduling uses `schedule_create`, `schedule_list`, `schedule_delete` tools
- Knowledge graph support via `knowledge_add_entity`, `knowledge_add_relation`, `knowledge_query`

## Dependencies

### Internal
- `openfang-hands/src` — Hand registry and loader

### External
None — Hands are pure configuration (HAND.toml + SKILL.md files)

<!-- MANUAL: -->
