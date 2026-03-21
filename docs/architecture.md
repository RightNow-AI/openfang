# OpenFang Architecture

This document explains the architecture that actually exists in this repository today. It is written for maintainers who need to trace ownership, debug runtime behavior, and decide where to extend the system.

## System Layers

At a high level, OpenFang is a layered Rust workspace:

```text
CLI / Desktop / API
        ↓
      Kernel
        ↓
Runtime / Memory / Channels / Wire / Skills / Hands / Extensions
        ↓
 Config, state, external providers, external tools, local workspaces
```

The important design rule is that orchestration lives in the kernel, while transport- or runtime-specific code lives in supporting crates.

## Workspace Layout

The workspace currently has 14 members: 13 product crates plus `xtask`.

| Crate | Responsibility |
|-------|----------------|
| `openfang-types` | Shared types and config schema used everywhere else |
| `openfang-memory` | SQLite-backed state, sessions, usage, memory persistence |
| `openfang-runtime` | Agent loop, drivers, tools, WASM/Python execution, MCP/A2A runtime pieces |
| `openfang-wire` | OFP peer networking and peer registry |
| `openfang-api` | Axum router, REST routes, dashboard, auth middleware |
| `openfang-kernel` | Main assembly point and operational core |
| `openfang-cli` | User-facing CLI and daemon launcher |
| `openfang-channels` | Channel adapters, bridge, formatter, routing |
| `openfang-migrate` | Migration from legacy ecosystems |
| `openfang-skills` | Bundled and installed skill registry |
| `openfang-desktop` | Tauri desktop embedding of the daemon |
| `openfang-hands` | Curated hand definitions and active hand state |
| `openfang-extensions` | Vault, credentials, integration registry, health checks |
| `xtask` | Workspace automation tasks |

## Core Runtime Boundary

`OpenFangKernel` in `crates/openfang-kernel/src/kernel.rs` is the center of the system. It owns:

- runtime state and configuration
- the agent registry and scheduler
- memory access
- workflow, trigger, cron, approval, and broadcast engines
- skill, hand, extension, and MCP integration registries
- channel adapter registration
- provider routing, metering, audit, and health data

Everything else either feeds requests into the kernel or extends what the kernel can do.

## Boot Flow

The standard daemon path is:

1. `openfang start` in `openfang-cli`
2. `load_config()` in `openfang-kernel/src/config.rs`
3. `OpenFangKernel::boot_with_config()`
4. `openfang-api::server::build_router()`
5. channel bridge startup
6. background agent, hand, MCP, and watcher startup
7. HTTP server bind and `daemon.json` write

Important boot-time behaviors:

- `OPENFANG_HOME` changes the config/state root
- `OPENFANG_LISTEN` overrides `config.toml` `api_listen`
- `OPENFANG_API_KEY` overrides `config.api_key` when it is set to a non-empty value
- provider credentials are resolved through vault -> `~/.openfang/.env` -> process environment
- the kernel restores persisted agents and hands during boot

## Request and Message Flow

### CLI/API to Agent

For a normal agent request, the path is:

1. CLI or dashboard calls the HTTP API.
2. `openfang-api` route handler resolves the target agent.
3. The kernel checks auth, registry state, quotas, and execution mode.
4. The runtime runs the agent loop or delegated module.
5. Memory, usage, audit, and delivery state are updated.
6. Response is returned over HTTP, WebSocket, or SSE.

### Channel to Agent

For Telegram, Discord, Slack, and other channels:

1. `openfang-api::channel_bridge` starts adapters from config.
2. Adapters emit normalized `ChannelMessage` events through `openfang-channels`.
3. `AgentRouter` resolves the agent using bindings, direct routes, channel defaults, and global defaults.
4. The bridge forwards the message into the kernel.
5. The bridge records delivery status and optional channel-specific metadata.

This separation is why channel transport logic belongs in `openfang-channels`, not in route handlers or the kernel.

## State and Persistence

The default state root is `~/.openfang/` unless `OPENFANG_HOME` overrides it.

Important persisted assets:

- `config.toml`
- `.env`
- `vault.enc`
- `daemon.json`
- the runtime sqlite database at `[memory].sqlite_path` or `data/openfang.db` by default
- `agents/`
- `skills/`
- `workspaces/`
- `workflows/`
- `hand_state.json`
- `cron_jobs.json`

Agent workspaces are created under `~/.openfang/workspaces/<agent>/` and include `data`, `output`, `sessions`, `skills`, `logs`, and `memory` directories.

## Key Domain Concepts

### Agents

Agents are runtime instances created from manifests. They are persisted, scheduled, addressed by ID, and executed through the kernel/runtime boundary.

### Hands

Hands are curated packages defined by `HAND.toml` and optional `SKILL.md`. The hand registry loads bundled and installed definitions, activates them into agent instances, and persists active hand state for restart recovery.

External hands may also ship a `workspace-scaffold/` directory. OpenFang copies any files found there into the activated workspace before generating the default identity files, so partial scaffolds can override only the files they need while the kernel fills the rest of the standard workspace set.

### Skills

Skills are pluggable tool bundles and prompt-only context packages. Bundled skills ship with the binary; installed skills live under the OpenFang home directory. Skills can add tools, requirements, and prompt context.

### Channels

Channels convert external platform traffic into normalized messages. Routing policy is centralized in `AgentRouter`, while transport adapters stay isolated in `openfang-channels`.

### Workflows, Triggers, and Cron

The kernel owns multi-step automation:

- workflows define multi-agent steps
- triggers react to events
- cron jobs schedule repeated execution

These are operationally distinct from hands, but hands often use them under the hood.

### Extensions, MCP, and A2A

OpenFang supports external tool connectivity through:

- extension templates and health monitoring
- MCP servers
- A2A external agents

The kernel merges these into the effective runtime tool surface.

## Security and Operational Boundaries

Several boundaries matter to maintainers:

- config loading and hot reload are separate from live application of changes
- loopback-only exposure is enforced when auth is absent
- some config changes are hot-reloadable, others require restart
- channel routing and auth are separate concerns
- audit stream and daemon logs are not the same thing

See [Configuration](configuration.md), [Operations Runbook](operations-runbook.md), and [Troubleshooting](troubleshooting.md) for those operational implications.

## Repository-Specific Note

This fork also contains a project-specific `shipinbot` integration under `projects/shipinbot/`. Keep core platform architecture decisions in `docs/`, and keep integration-specific behavior documented under `projects/shipinbot/docs/`.
