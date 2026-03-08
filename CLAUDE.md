# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview
OpenFang is an open-source Agent Operating System written in Rust — 14 crates, 137K+ LOC.
- Config: `~/.openfang/config.toml` (supports `include` for file composition)
- Default API: `http://127.0.0.1:4200`
- CLI binary: `target/release/openfang` (or `target/debug/openfang`)
- Agent manifests: TOML files in `agents/<name>/agent.toml`

## Build & Verify Commands
After every change, run all three:
```bash
cargo build --workspace --lib          # Must compile (use --lib if exe is locked by running daemon)
cargo test --workspace                 # All tests must pass (1,767+)
cargo clippy --workspace --all-targets -- -D warnings  # Zero warnings
```

Run a single test:
```bash
cargo test -p openfang-runtime test_name        # Single test in a crate
cargo test -p openfang-kernel --test integration_test  # Single integration test file
```

Format check:
```bash
cargo fmt --all -- --check
```

## Architecture

### Crate Dependency Flow
```
openfang-types        Shared types, config structs, taint tracking, Ed25519 signing
       |
openfang-memory       SQLite persistence, vector embeddings, session management
       |
openfang-runtime      Agent loop, LLM drivers, 53 tools, WASM sandbox, MCP, A2A
       |
openfang-kernel       Orchestration: registry, scheduler, cron, workflows, metering, approval
       |
openfang-api          Axum HTTP server, 140+ REST/WS/SSE endpoints, dashboard
       |
openfang-cli          CLI binary, daemon management, TUI, MCP server mode
```

Parallel crates (depend on types/runtime but not each other):
- `openfang-channels` — 40 messaging adapters (Telegram, Discord, Slack, WhatsApp, etc.)
- `openfang-skills` — 60 bundled skills, SKILL.md parser, FangHub marketplace
- `openfang-hands` — 7 autonomous Hands, HAND.toml parser, lifecycle management
- `openfang-extensions` — MCP templates, AES-256-GCM vault, OAuth2 PKCE
- `openfang-wire` — OFP P2P protocol with HMAC-SHA256 mutual auth
- `openfang-desktop` — Tauri 2.0 native app
- `openfang-migrate` — Migration engine for OpenClaw/LangChain/AutoGPT

### Key Architectural Patterns

**KernelHandle trait** (`runtime/src/kernel_handle.rs`): Breaks the circular dependency between runtime and kernel. The runtime defines the trait; the kernel implements it. The agent loop receives a `&dyn KernelHandle` to call back into the kernel for inter-agent operations (spawn, send, kill, memory).

**AppState** (`api/src/routes.rs`): Bridges kernel to API routes. Holds `Arc<OpenFangKernel>`, peer registry, channel bridge manager, rate limiter.

**LLM Drivers** (`runtime/src/drivers/`): 5 native drivers — `anthropic.rs`, `openai.rs` (covers 20+ OpenAI-compatible providers), `gemini.rs`, `copilot.rs`, `claude_code.rs`. All implement the `LlmDriver` trait. The `fallback.rs` driver chains multiple providers.

**Agent Loop** (`runtime/src/agent_loop.rs`): Core execution cycle — receive message, recall memories, call LLM, execute tool calls, save conversation. Max 50 iterations per turn, exponential backoff on rate limits.

**Config System** (`types/src/config.rs` for types, `kernel/src/config.rs` for loading): TOML-based with deep-merge includes. All fields use `#[serde(default)]`.

### Adding New Features — Wiring Checklist

**New API route:**
1. Add handler function in `api/src/routes.rs`
2. Register route in `api/src/server.rs` router
3. Add types in `api/src/types.rs` if needed

**New config field:**
1. Add field to struct in `types/src/config.rs` with `#[serde(default)]`
2. Add to the `Default` impl for that struct (build fails otherwise)
3. Field must have Serialize + Deserialize derives

**New tool:**
1. Implement in `runtime/src/tool_runner.rs`
2. Add `ToolDefinition` to `builtin_tool_definitions()`
3. Reference by name in agent manifest `[capabilities] tools = [...]`

**New dashboard tab:**
- Dashboard is Alpine.js SPA in `static/index_body.html` — add both HTML template and JS data/methods

## Common Gotchas
- `openfang` binary may be locked if daemon is running — use `--lib` flag or kill daemon first
- `PeerRegistry` is `Option<PeerRegistry>` on kernel but `Option<Arc<PeerRegistry>>` on `AppState` — wrap with `.as_ref().map(|r| Arc::new(r.clone()))`
- Config fields added to a config struct MUST also be added to its `Default` impl
- `AgentLoopResult` field is `.response` not `.response_text`
- CLI command to start daemon is `start` not `daemon`
- TOML enum values must be lowercase (`"allowlist"` not `"Allowlist"`)
- Agent `system_prompt` goes inside `[model]` section of TOML manifest, NOT at top level
- Don't touch `openfang-cli` — user is actively building the interactive CLI

## MANDATORY: Live Integration Testing
After implementing any new endpoint, feature, or wiring change, run live integration tests. Unit tests alone can pass while the feature is dead code.

### Quick Test Procedure
```bash
# 1. Kill any running daemon
pkill -f openfang || true
sleep 3

# 2. Build and start
cargo build --release -p openfang-cli
GROQ_API_KEY=<key> target/release/openfang start &
sleep 6
curl -s http://127.0.0.1:4200/api/health

# 3. Test endpoints (GET returns real data, POST persists correctly)
curl -s http://127.0.0.1:4200/api/agents
curl -s -X POST "http://127.0.0.1:4200/api/agents/<id>/message" \
  -H "Content-Type: application/json" \
  -d '{"message": "Say hello in 5 words."}'

# 4. Verify side effects (budget tracking, etc.)
curl -s http://127.0.0.1:4200/api/budget

# 5. Cleanup
pkill -f openfang || true
```

### Key API Endpoints
| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/api/health` | GET | Health check |
| `/api/agents` | GET | List all agents |
| `/api/agents/{id}/message` | POST | Send message (triggers LLM) |
| `/api/budget` | GET/PUT | Global budget status/update |
| `/api/budget/agents` | GET | Per-agent cost ranking |
| `/api/network/status` | GET | OFP network status |
| `/api/cron/jobs` | GET | Cron job listing |
| `/api/a2a/agents` | GET | External A2A agents |
| `/api/a2a/discover` | POST | Discover A2A agent at URL |
| `/api/a2a/send` | POST | Send task to external A2A agent |
