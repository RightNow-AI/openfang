<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-types

## Purpose
Core types and traits for the OpenFang Agent OS. Defines all shared data structures used across the kernel, runtime, memory substrate, and wire protocol. Contains no business logic — purely type definitions, serialization, and minimal utility functions (e.g., `truncate_str` for safe UTF-8 truncation).

## Key Files
| File | Description |
|------|-------------|
| `src/agent.rs` | Agent ID, manifest, state, config, user/session types |
| `src/message.rs` | Message, content blocks, roles (user/assistant/system), stop reasons |
| `src/tool.rs` | Tool definition, tool calls, tool results |
| `src/config.rs` | `KernelConfig` struct with all daemon settings (memory, scheduling, budget, etc.) |
| `src/event.rs` | Event types for event bus (agent_started, message_sent, cost_updated, etc.) |
| `src/error.rs` | `OpenFangError` and `OpenFangResult` types |
| `src/memory.rs` | Memory trait, filter, source, consolidation, semantic types |
| `src/capability.rs` | Agent capabilities (what tools/features agent can use) |
| `src/approval.rs` | Approval workflow types (pending, approved, rejected) |
| `src/webhook.rs` | Webhook payload and signature types |
| `src/comms.rs` | Communication channel definitions (Telegram, WhatsApp, Discord) |
| `src/media.rs` | Media types (image, audio, video, document) |
| `src/taint.rs` | Taint tracking for sensitive data (passwords, API keys, PII) |
| `src/scheduler.rs` | Scheduling types (cron, interval, once) |
| `src/model_catalog.rs` | LLM model definitions and availability status |
| `src/manifest_signing.rs` | Ed25519 manifest signing for code integrity |
| `src/tool_compat.rs` | Tool schema compatibility helpers |
| `src/serde_compat.rs` | Serialization compatibility for older versions |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/` | Type definitions and minimal utilities |

## For AI Agents

### Working In This Directory
- **Add new types here first** — all other crates import from `openfang-types`
- Use `#[serde(default)]` and `#[serde(rename_all = "snake_case")]` for config compatibility
- Ensure all types implement `Serialize`, `Deserialize`, `Clone`, `Debug`
- ID types use `uuid::Uuid` with custom wrappers (e.g., `AgentId(Uuid)`)
- Timestamps use `chrono::DateTime<Utc>`
- Error types use `thiserror` for context and display messages
- Keep types simple — no circular dependencies, no trait objects
- Use `Option<T>` instead of `Result<T>` for optional fields

### Testing Requirements
- Run `cargo test --package openfang-types` to verify serialization roundtrips
- All new types should have roundtrip tests: serialize → deserialize → equals original
- Config types need `Default` impl tests to catch missing `#[serde(default)]` annotations
- String types should test UTF-8 safety (see `truncate_str` tests)

### Common Patterns
- Message content uses variant enum: `ContentBlock::Text { text, ... }` | `ToolCall { ... }` | `ToolResult { ... }`
- Tool definitions include jsonschema for params — SDK/UI uses this to generate forms
- Config uses `serde(default)` to support partial TOML updates without breaking on missing fields
- Agent manifests are TOML files deserialized into `AgentManifest` struct
- Stop reasons: `end_turn` (normal stop) | `max_tokens` (truncated) | `tool_use` (pending tool call)
- Memory filters support source (session/knowledge/semantic), date range, agent scope
- Events use `chrono::DateTime<Utc>` for ordering and filtering

## Dependencies

### Internal
None — this is the foundational crate.

### External
- `serde` — serialization framework
- `serde_json` — JSON handling
- `chrono` — datetime types
- `uuid` — unique identifiers
- `thiserror` — error types
- `dirs` — config directory paths
- `toml` — TOML parsing
- `ed25519-dalek` — manifest signing
- `sha2`, `hex`, `rand` — hashing and randomness

<!-- MANUAL: -->
