<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-kernel — Core Agent Operating System Kernel

## Purpose

The kernel module implements the core runtime for the OpenFang Agent Operating System. It manages agent lifecycles, event distribution, scheduling, authentication, capabilities, memory integration, and inter-agent workflows. This is the heart of OpenFang where all subsystems converge.

## Key Files

| File | Purpose |
|------|---------|
| `kernel.rs` | Main `OpenFangKernel` struct — assembles all subsystems (auth, scheduler, supervisor, registry, event bus, metering) |
| `registry.rs` | Agent registry — agent creation, lifecycle management, agent discovery |
| `scheduler.rs` | Agent task scheduling — agent sleep/wake, cron triggers, workflow scheduling |
| `supervisor.rs` | Health monitoring — agent health checks, crash detection, restart policies |
| `event_bus.rs` | Pub-sub event distribution — events for agent lifecycle, messages, crashes, quota |
| `metering.rs` | Cost tracking — token usage, cost calculation, budget enforcement |
| `config.rs` / `config_reload.rs` | TOML config parsing and hot-reload — kernel settings, provider keys, memory backend |
| `auth.rs` | Authentication manager — API key validation, user sessions |
| `capabilities.rs` | Capability manager — dynamic capability grants to agents |
| `approval.rs` | Manual approval workflows — user-in-the-loop for sensitive actions |
| `triggers.rs` | Trigger engine — time-based, event-based, external webhook triggers |
| `workflow.rs` | Workflow orchestration — multi-step agent workflows, DAGs |
| `cron.rs` | Cron scheduling — recurring task execution |
| `background.rs` | Background task executor — non-blocking background work (metrics, cleanup) |
| `wizard.rs` | Interactive setup wizard — onboarding flow for new agents/channels |
| `whatsapp_gateway.rs` | WhatsApp integration — bi-directional WhatsApp message handling |
| `auto_reply.rs` | Automated replies — template-based responses when agent is offline |
| `pairing.rs` | Agent pairing protocol — secure pairing between local and remote agents |
| `error.rs` | Error types — `KernelError`, `KernelResult` |
| `heartbeat.rs` | Health heartbeat — kernel liveness signals |

## For AI Agents

**When to read:** Understand agent lifecycle management, event handling, configuration management, or scheduling behavior.

**Key interfaces:**
- `OpenFangKernel` — main entry point for all kernel operations
- `AgentRegistry` — spawn, list, pause, resume, delete agents
- `EventBus` — subscribe to kernel events (crashes, messages, quota)
- `Metering` — token usage, cost tracking
- `Memory` trait integration — agents access shared memory substrate

**Common tasks:**
- Modifying kernel initialization logic → see `kernel.rs` constructor
- Adding new event types → `event_bus.rs` + `event.rs` (in types)
- Implementing new scheduling rules → `scheduler.rs`
- Adding supervisor health checks → `supervisor.rs`
- Integrating new channels → `whatsapp_gateway.rs` as example

**Architecture pattern:** The kernel uses the `KernelHandle` trait (in runtime) to avoid circular dependencies with runtime's agent loop.
