# OpenFang Core Modules

This guide maps the most important subsystems to the files that maintainers actually need to inspect when changing behavior.

## Ownership Map

| Area | Primary files | Notes |
|------|---------------|-------|
| Config schema | `crates/openfang-types/src/config.rs` | Canonical config structure, defaults, section names |
| Config loading | `crates/openfang-kernel/src/config.rs` | `OPENFANG_HOME`, include support, legacy field migration |
| Config reload | `crates/openfang-kernel/src/config_reload.rs` | Hot-reload vs restart-required decisions |
| Kernel assembly | `crates/openfang-kernel/src/kernel.rs` | Boot path, runtime ownership, persistence wiring |
| CLI | `crates/openfang-cli/src/main.rs` | `init`, `start`, `stop`, `doctor`, `status`, TUI integration |
| HTTP router | `crates/openfang-api/src/server.rs` | Route registration and middleware stack |
| HTTP handlers | `crates/openfang-api/src/routes.rs` | API behavior and operational endpoints |
| Dashboard | `crates/openfang-api/static/` | Frontend assets served by the daemon |
| Memory | `crates/openfang-memory/` | SQLite schema, usage state, session persistence |
| Agent runtime | `crates/openfang-runtime/` | Drivers, tool runner, browser, MCP, A2A, media logic |
| Channels | `crates/openfang-channels/src/` | Adapters, bridge, router, formatting |
| Hands | `crates/openfang-hands/src/` | Bundled hands, install/activate/persist state |
| Skills | `crates/openfang-skills/src/` | Bundled skills, installed skill registry, verification |
| Extensions and vault | `crates/openfang-extensions/src/` | Credentials, vault, integration registry, health |
| OFP networking | `crates/openfang-wire/src/` | Peer registry and node implementation |
| Desktop | `crates/openfang-desktop/src/` | Tauri embedding of the API/kernel |

## Critical Flows

### 1. Config to Live Runtime

Relevant files:

- `crates/openfang-types/src/config.rs`
- `crates/openfang-kernel/src/config.rs`
- `crates/openfang-kernel/src/config_reload.rs`
- `crates/openfang-kernel/src/kernel.rs`

Flow:

1. deserialize config into `KernelConfig`
2. apply env overrides at boot
3. validate and warn
4. boot subsystems
5. later diff config changes into a reload plan
6. apply only hot-reloadable actions at runtime

Edit this area when:

- adding a new config section
- changing defaults
- changing reload semantics
- adding deployment-facing behavior

### 2. Agent Spawn and Message Execution

Relevant files:

- `crates/openfang-api/src/routes.rs`
- `crates/openfang-kernel/src/kernel.rs`
- `crates/openfang-runtime/src/agent_loop.rs`

Flow:

1. route handler parses request or manifest
2. kernel registers the agent and persists it
3. runtime executes model/tool loop or delegated module
4. memory and usage state are updated
5. response is streamed or returned

Edit this area when:

- changing spawn semantics
- changing response structure
- changing tool execution or session behavior

### 3. Channel Ingress and Routing

Relevant files:

- `crates/openfang-api/src/channel_bridge.rs`
- `crates/openfang-channels/src/bridge.rs`
- `crates/openfang-channels/src/router.rs`
- specific adapters under `crates/openfang-channels/src/*.rs`

Flow:

1. channel config enables an adapter
2. adapter emits normalized messages
3. router resolves the agent using bindings/defaults
4. bridge forwards into the kernel
5. delivery tracking is recorded

Edit this area when:

- adding a channel
- changing routing rules
- changing per-channel overrides or policy behavior

### 4. Hands and Skills

Relevant files:

- `crates/openfang-hands/src/lib.rs`
- `crates/openfang-hands/src/registry.rs`
- `crates/openfang-skills/src/lib.rs`
- `crates/openfang-skills/src/registry.rs`

Hands own packaged autonomous behavior and activation lifecycle. Skills own additional tools and prompt context.

Edit this area when:

- changing hand install or activation
- changing skill loading or verification
- adding bundled assets

### 5. Operations and Observability

Relevant files:

- `crates/openfang-cli/src/main.rs`
- `crates/openfang-api/src/routes.rs`
- `crates/openfang-api/src/server.rs`
- `crates/openfang-kernel/src/kernel.rs`

Key operational surfaces:

- `openfang doctor`
- `openfang status`
- `openfang health`
- `/api/health`
- `/api/health/detail`
- `/api/status`
- `/api/metrics`
- `/api/config/reload`
- `/api/channels/reload`
- `/api/logs/stream`

Edit this area when:

- changing health checks
- changing smoke-test expectations
- changing monitoring or audit behavior

## Where to Extend

| Goal | Start here |
|------|------------|
| Add a config field | `openfang-types` config schema, then `openfang-kernel` reload logic |
| Add an API endpoint | `openfang-api/src/routes.rs` and route registration in `server.rs` |
| Add a channel | `openfang-channels`, plus config struct in `openfang-types` |
| Add a hand | `crates/openfang-hands/bundled/` or external hand path |
| Add a skill | `crates/openfang-skills/bundled/` or installed skill path |
| Add a new deployment behavior | `openfang-cli`, `openfang-kernel`, and deployment docs |
| Add operational telemetry | `openfang-api` metrics/health routes and kernel state sources |

## Maintainer Rules of Thumb

- If the change affects public configuration, update `openfang.toml.example` and [Configuration](configuration.md).
- If the change affects boot, route registration, or runtime wiring, update [Architecture](architecture.md) or [Deployment](deployment.md) as needed.
- If the change affects health, logging, reload, or backup expectations, update [Operations Runbook](operations-runbook.md) and [Troubleshooting](troubleshooting.md).
