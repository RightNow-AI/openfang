<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# crates

## Purpose
14 interdependent Rust crates that form the core OpenFang Agent OS. Collectively provide types, memory substrate, runtime execution, networking (OFP), API server, kernel orchestration, CLI/desktop frontends, skill/hand systems, channel integrations, and extension/credential management.

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `openfang-types/` | Core types and traits for the entire system |
| `openfang-memory/` | Memory substrate — SQLite + HTTP-backed persistence |
| `openfang-runtime/` | Agent runtime and execution environment (WASM, LLM calls) |
| `openfang-wire/` | OpenFang Protocol (OFP) for agent-to-agent networking |
| `openfang-api/` | HTTP/WebSocket API server — main daemon interface |
| `openfang-kernel/` | Core kernel orchestrating runtime, memory, skills, hands |
| `openfang-cli/` | CLI binary for daemon control and interactive shell |
| `openfang-channels/` | Channel bridge layer — email, MQTT, Slack, Telegram, etc. |
| `openfang-migrate/` | Import migration engine from other agent frameworks |
| `openfang-skills/` | Skill registry, loader, marketplace, OpenClaw compatibility |
| `openfang-hands/` | Hands system — curated autonomous capability packages |
| `openfang-extensions/` | Extension system — MCP setup, credential vault, OAuth2 |
| `openfang-desktop/` | Native desktop app (Tauri 2.0) |

## For AI Agents

### Working In This Directory

1. **Read the crate, not cargo**: Each crate's Cargo.toml describes its purpose. Read `src/lib.rs` or `src/main.rs` to understand the actual implementation.
2. **Dependency order matters**: `openfang-types` is the foundation. `openfang-kernel` is the hub. Build/test bottom-up from types → memory/runtime → kernel → api.
3. **The kernel is the nexus**: If touching multiple crates, likely need changes in kernel first (contract), then consumers. The `KernelHandle` trait in types prevents circular deps.
4. **API is heavyweight**: openfang-api depends on *all* other crates. Changes to kernel/runtime/skills propagate here. Test the daemon after API-touching changes.
5. **CLI is off-limits**: User actively building the interactive CLI. Do not change CLI source without explicit request.
6. **Test fresh binaries**: After build, stop any running daemon, build with `--release`, start fresh, and run live integration tests (curl against `/api/health`, endpoints, etc.).

### Testing Requirements

- **Unit tests**: Every crate must pass `cargo test --workspace` (currently 1700+ tests).
- **Clippy zero-warnings**: `cargo clippy --workspace --all-targets -- -D warnings` must pass.
- **Live integration tests mandatory** (not optional): After any new endpoint or feature:
  ```bash
  # 1. Kill running daemon
  taskkill //PID <pid> //F && sleep 3

  # 2. Build fresh release
  cargo build --release -p openfang-cli

  # 3. Start daemon with env vars
  GROQ_API_KEY=<key> target/release/openfang.exe start &
  sleep 6

  # 4. Verify health
  curl -s http://127.0.0.1:4200/api/health

  # 5. Test your new endpoint
  curl -s http://127.0.0.1:4200/api/<endpoint>

  # 6. Verify persistence (for write endpoints)
  curl -s -X PUT http://127.0.0.1:4200/api/<endpoint> -d '...'
  curl -s http://127.0.0.1:4200/api/<endpoint>  # should reflect write

  # 7. Clean up
  taskkill //PID <pid> //F
  ```

### Common Patterns

**Adding a config field**:
- Add field to `KernelConfig` struct in `openfang-kernel/src/config.rs`
- Add `#[serde(default)]` attribute
- Add entry to `Default` impl
- Add entry to `Serialize`/`Deserialize` derives
- Read from TOML in `cli/src/main.rs` startup

**Adding an API endpoint**:
- Define route and handler in `openfang-api/src/routes.rs`
- Register route in `openfang-api/src/server.rs` router
- Live-test after daemon restart (see Testing Requirements above)

**Adding a skill or hand**:
- Implement `Skill`/`Hand` trait from `openfang-types`
- Register in `openfang-kernel` via skill registry
- Add to dashboard if needs UI (see `static/index_body.html`)

**Cross-crate communication**:
- Use `KernelHandle` trait (avoid circular deps)
- `AppState` in `server.rs` bridges kernel to API routes
- For async: use `tokio::spawn`, `tokio::sync::RwLock`, or `dashmap`

## Dependencies

### Internal
| Crate | Depends On |
|-------|-----------|
| openfang-types | (foundation — no internal deps) |
| openfang-memory | types |
| openfang-runtime | types, memory, skills |
| openfang-wire | types |
| openfang-channels | types |
| openfang-migrate | types |
| openfang-skills | types |
| openfang-hands | types |
| openfang-extensions | types |
| openfang-kernel | types, memory, runtime, skills, hands, extensions, wire, channels |
| openfang-api | types, kernel, runtime, memory, channels, wire, skills, hands, extensions, migrate |
| openfang-cli | types, kernel, api, migrate, skills, extensions, runtime |
| openfang-desktop | kernel, api, types |

### Key External
- **Async runtime**: tokio, futures
- **Web**: axum, tower, reqwest, tokio-tungstenite
- **Serialization**: serde, serde_json, toml, serde_yaml
- **Storage**: rusqlite, dashmap
- **LLM/Runtime**: rmcp, wasmtime
- **Crypto**: sha2, ed25519-dalek, hmac, aes-gcm, argon2
- **Desktop UI**: tauri 2.0, ratatui (CLI)
- **Channels**: lettre (email), rumqttc (MQTT), imap, native-tls, mailparse

<!-- MANUAL: -->
