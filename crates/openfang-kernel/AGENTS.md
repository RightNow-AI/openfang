<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-kernel

## Purpose
Core kernel for the OpenFang Agent OS. Assembles all subsystems and provides the main orchestration API. Manages agent lifecycles, memory, permissions, scheduling, inter-agent communication, cost metering, event bus, RBAC authentication, and background task execution. The kernel is thread-safe and designed to run in-process within the HTTP API daemon.

## Key Files
| File | Description |
|------|-------------|
| `src/kernel.rs` | Main `OpenFangKernel` struct, subsystem initialization, public API |
| `src/config.rs` | Kernel config loading from `~/.openfang/config.toml` |
| `src/config_reload.rs` | Hot-reload config without daemon restart |
| `src/registry.rs` | Agent registry and manifest loading |
| `src/scheduler.rs` | Cron-based agent scheduling and invocation |
| `src/cron.rs` | Cron expression parsing and trigger evaluation |
| `src/metering.rs` | Cost tracking and budget enforcement per agent/model |
| `src/auth.rs` | RBAC authentication, API keys, session tokens |
| `src/event_bus.rs` | Publish/subscribe event system for inter-agent communication |
| `src/supervisor.rs` | Process lifecycle management and restart logic |
| `src/background.rs` | Background agent executor for scheduled/triggered tasks |
| `src/approval.rs` | Approval workflow for risky operations |
| `src/triggers.rs` | Event-driven trigger engine (message arrival, cost thresholds) |
| `src/workflow.rs` | Multi-step workflow execution with step agents |
| `src/heartbeat.rs` | Autonomous agent heartbeat and status monitoring |
| `src/pairing.rs` | Agent pairing and device link setup |
| `src/wizard.rs` | Onboarding wizard for agent configuration |
| `src/whatsapp_gateway.rs` | WhatsApp integration and message gateway |
| `src/error.rs` | Error types for kernel operations |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/` | Core subsystem implementations |

## For AI Agents

### Working In This Directory
- `OpenFangKernel` struct holds all subsystems — access via `kernel.<subsystem>`
- Agent state is stored via `kernel.memory` (not in kernel directly)
- New subsystems should be fields in `OpenFangKernel` and initialized in `OpenFangKernel::new()`
- Config changes need updates to both `KernelConfig` struct in `openfang-types` AND the Default impl
- Metering is automatic on LLM calls via `kernel.metering.record_llm_usage()`
- Agent execution goes through `kernel.runtime.run_agent_loop()` (runtime crate)
- Event bus is used for inter-agent messaging — publish/subscribe pattern

### Testing Requirements
- Run `cargo build --workspace --lib && cargo test --workspace` after changes
- Unit tests are in each subsystem file
- Integration tests require a live daemon (see CLAUDE.md for protocol)
- After kernel changes: verify that agent scheduling still works, cost metering updates, events propagate

### Common Patterns
- All subsystems are `Arc<...>` for shared ownership and thread safety
- Config is loaded once at startup, cached, and reloaded on demand via `config_reload`
- Agent manifests are TOML files stored in `~/.openfang/agents/` and loaded by registry
- Cron expressions use the `cron` crate (`0 9 * * *` = daily at 9am)
- Metering records cost per LLM provider, model, and agent — enforces budget limits
- RBAC uses API keys + session tokens; validate in `openfang-api/middleware.rs`
- Event bus delivers messages via channel to subscribers — async/await friendly

## Dependencies

### Internal
- `openfang-types` — config, agent, event types
- `openfang-memory` — agent memory and knowledge graph
- `openfang-runtime` — agent loop execution
- `openfang-skills` — plugin skills for agents
- `openfang-hands` — web/browser tools
- `openfang-extensions` — extension system
- `openfang-wire` — wire protocol
- `openfang-channels` — external channel integrations

### External
- `tokio` — async runtime
- `serde`, `toml` — config serialization
- `chrono`, `chrono-tz` — timezone-aware scheduling
- `crossbeam` — concurrent queues
- `dashmap` — concurrent hash map for agent state
- `tracing` — structured logging
- `thiserror` — error types
- `cron` — cron expression parsing and evaluation
- `zeroize` — secure API key cleanup from memory

<!-- MANUAL: -->
