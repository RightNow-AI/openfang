<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-types — Shared Type System

## Purpose

Shared data structures and traits used across the OpenFang kernel, runtime, memory substrate, channels, and wire protocol. This crate contains **no business logic** — only type definitions, serialization, and compatibility helpers.

## Key Files

| File | Purpose |
|------|---------|
| `agent.rs` | Agent metadata — `AgentId`, `AgentEntry`, `AgentStatus` |
| `message.rs` | Message types — `Message`, `MessageId`, message threading |
| `tool.rs` | Tool definitions — `ToolDefinition`, `ToolCall`, tool schemas |
| `tool_compat.rs` | Tool format migration — OpenAI <-> OpenClaw tool schema conversion |
| `capability.rs` | Capability model — `Capability`, approval requirements |
| `approval.rs` | Approval workflow types — `ApprovalRequest`, decision tracking |
| `event.rs` | Kernel events — `KernelEvent`, lifecycle events, error events |
| `memory.rs` | Memory API — `Memory` trait, consolidation, import/export, vectors |
| `config.rs` | Config types — `KernelConfig`, agent templates, provider settings |
| `model_catalog.rs` | LLM models — model definitions, provider routing |
| `comms.rs` | Cross-machine communication — agent-to-agent message structures |
| `scheduler.rs` | Scheduling types — cron patterns, trigger definitions |
| `webhook.rs` | Webhook payloads — incoming webhook message structures |
| `media.rs` | Media handling — file attachments, MIME types |
| `error.rs` | Error types — `OpenFangError`, `OpenFangResult` |
| `manifest_signing.rs` | Agent manifest verification — digital signatures |
| `serde_compat.rs` | Serialization helpers — custom serde logic for cross-version compatibility |
| `taint.rs` | Taint tracking — untrusted data markers |

## For AI Agents

**When to read:** Understand OpenFang's type system, how agents are represented, message formats, or capability structures.

**Key types to know:**
- `AgentId` — UUID for agents
- `AgentEntry` — agent metadata (name, status, template, created_at)
- `Message` — unified message structure across channels
- `ToolDefinition` — LLM tool schema (OpenAI format)
- `Memory` trait — unified async API for knowledge/session/structured stores
- `Capability` — fine-grained permissions
- `KernelEvent` — domain events for all kernel activities

**Serialization:**
- All types derive `Serialize`/`Deserialize` for JSON/TOML
- Custom impls in `serde_compat.rs` handle schema migration between versions

**Architecture pattern:** This crate is intentionally thin and stateless — no async code, no I/O, no dependencies on kernel or runtime. It's safe to re-export publicly.
