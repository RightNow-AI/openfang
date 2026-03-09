# Contributing to OpenFang

Thank you for your interest in contributing to OpenFang. This guide covers everything you need to get started, from setting up your development environment to submitting pull requests.

## Table of Contents

- [Development Environment](#development-environment)
- [Building and Testing](#building-and-testing)
- [Code Style](#code-style)
- [Architecture Overview](#architecture-overview)
- [How to Add a New Hand](#how-to-add-a-new-hand)
- [How to Add a New Channel Adapter](#how-to-add-a-new-channel-adapter)
- [How to Add a New Tool](#how-to-add-a-new-tool)
- [Pull Request Process](#pull-request-process)
- [Code of Conduct](#code-of-conduct)

---

## Development Environment

### Prerequisites

- **Rust 1.77+** (install via [rustup](https://rustup.rs/))
- **Git**
- **Python 3.11+** and `libpython3.11-dev` (for the `maestro-rlm` crate)
- A supported LLM API key (Anthropic, OpenAI, Groq, etc.) for end-to-end testing

### Clone and Build

```bash
git clone https://github.com/ParadiseAI/maestro-legacy.git
cd maestro-legacy
cargo build
```

The first build takes several minutes because it compiles SurrealDB v3, PyO3, and Wasmtime. Subsequent builds are incremental.

### Environment Variables

For running integration tests that hit a real LLM, set at least one provider key:

```bash
export GROQ_API_KEY=gsk_...          # Recommended for fast, free-tier testing
export ANTHROPIC_API_KEY=sk-ant-...  # For Anthropic-specific tests
```

Tests that require a real LLM key will skip gracefully if the env var is absent.

---

## Building and Testing

### Build the Entire Workspace

```bash
cargo build --workspace
```

### Run All Tests

```bash
cargo test --workspace
```

The test suite is currently **1,862+ tests**. All must pass before merging.

### Run Tests for a Single Crate

```bash
cargo test -p openfang-kernel
cargo test -p maestro-rlm
cargo test -p openfang-hands
```

### Check for Clippy Warnings

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

The CI pipeline enforces zero clippy warnings.

### Format Code

```bash
cargo fmt --all
```

Always run `cargo fmt` before committing. CI will reject unformatted code.

---

## Code Style

- **Formatting**: Use `rustfmt` with default settings. Run `cargo fmt --all` before every commit.
- **Linting**: `cargo clippy --workspace -- -D warnings` must pass with zero warnings.
- **Documentation**: All public types and functions must have doc comments (`///`).
- **Error Handling**: Use `thiserror` for error types. Avoid `unwrap()` in library code; prefer `?` propagation.
- **Naming**:
  - Types: `PascalCase` (e.g., `OpenFangKernel`, `HandDefinition`)
  - Functions/methods: `snake_case`
  - Constants: `SCREAMING_SNAKE_CASE`
  - Crate names: `openfang-{name}` or `maestro-{name}` (kebab-case)
- **Dependencies**: Workspace dependencies are declared in the root `Cargo.toml`. Prefer reusing workspace deps over adding new ones. If you need a new dependency, justify it in the PR.
- **Testing**: Every new feature must include tests. Use `tempfile::TempDir` for filesystem isolation and random port binding for network tests.
- **Serde**: All config structs use `#[serde(default)]` for forward compatibility with partial TOML.

---

## Architecture Overview

OpenFang is organized as a Cargo workspace with 21 crates:

| Crate | Role |
|-------|------|
| `openfang-types` | Shared type definitions, taint tracking, manifest signing (Ed25519), model catalog, MCP/A2A config types |
| `maestro-surreal-memory` | SurrealDB v3 memory substrate with vector embeddings, usage tracking, canonical sessions, JSONL mirroring |
| `maestro-cache` | L1/L2 caching layer (Moka + Redis) on top of the memory substrate. |
| `openfang-runtime` | Agent loop, 3 LLM drivers (Anthropic/Gemini/OpenAI-compat), 38 built-in tools, WASM sandbox, MCP client/server, A2A protocol |
| `openfang-hands` | Hands system: `HAND.toml` parser, lifecycle engine, scheduler, and 7 bundled autonomous agents. |
| `openfang-skills` | Skill system: 60 bundled skills, FangHub marketplace client, OpenClaw compatibility, prompt injection scanning |
| `openfang-extensions` | Integration registry (25 bundled MCP templates), AES-256-GCM credential vault, OAuth2 PKCE |
| `openfang-kernel` | Assembles all subsystems: workflow engine, RBAC auth, heartbeat monitor, cron scheduler, config hot-reload |
| `openfang-api` | REST/WS/SSE API (Axum 0.8), 76 endpoints, 14-page SPA dashboard, OpenAI-compatible `/v1/chat/completions` |
| `openfang-channels` | 40 channel adapters (Telegram, Discord, Slack, WhatsApp, and 36 more), formatter, rate limiter |
| `openfang-wire` | OFP (OpenFang Protocol): TCP P2P networking with HMAC-SHA256 mutual authentication |
| `openfang-cli` | Clap CLI with daemon auto-detect (HTTP mode vs. in-process fallback), MCP server |
| `openfang-migrate` | Migration engine for importing from OpenClaw (and future frameworks) |
| `openfang-desktop` | Tauri 2.0 native desktop app (WebView + system tray + single-instance + notifications) |
| `maestro-observability` | OpenTelemetry integration for traces, metrics, and logging. |
| `maestro-guardrails` | PII scanning, prompt injection detection, and topic control. |
| `maestro-model-hub` | Capability-aware model routing and circuit breakers. |
| `maestro-knowledge` | RAG pipeline with SurrealDB vector search. |
| `maestro-eval` | LLM-as-judge evaluation engine and regression tracker. |
| `maestro-pai` | Self-evolution learning hooks and pattern synthesis (SurrealDB-backed). |
| `maestro-rlm` | Recursive Language Model for long-context processing (PyO3-based). |
| `xtask` | Build automation tasks |

### Key Architectural Patterns

- **`KernelHandle` trait**: Defined in `openfang-runtime`, implemented on `OpenFangKernel` in `openfang-kernel`. This avoids circular crate dependencies while enabling inter-agent tools.
- **Daemon detection**: The CLI checks `~/.openfang/daemon.json` and pings the health endpoint. If a daemon is running, commands use HTTP; otherwise, they boot an in-process kernel.
- **Capability-based security**: Every agent operation is checked against the agent's granted capabilities before execution.

---

## How to Add a New Hand

Hands are autonomous agent packages that live in the `openfang/hands/` directory. Each Hand is a folder containing a `HAND.toml` manifest, a `system_prompt.md`, and a `SKILL.md`.

### Steps

1. Create a new directory under `openfang/hands/`:

```
openfang/hands/my-hand/
├── HAND.toml
├── system_prompt.md
└── SKILL.md
```

2. Write the manifest (`HAND.toml`):

```toml
[hand]
name = "My Hand"
version = "0.1.0"
description = "A brief description of what this Hand does."
author = "Your Name"

[requirements]
min_openfang_version = "0.3.31"
required_tools = ["file_read", "web_fetch"]

[schedule]
cron = "0 0 * * *" # Run once a day at midnight

[prompts]
system_prompt_path = "system_prompt.md"
skill_prompt_path = "SKILL.md"
```

3. Write the system prompt and skill documentation.

4. Test by activating:

```bash
openfang hand activate my-hand
```

5. Submit a PR with the new Hand package.

---

## How to Add a New Channel Adapter

Channel adapters live in `crates/openfang-channels/src/`. Each adapter implements the `ChannelAdapter` trait.

### Steps

1. Create a new file: `crates/openfang-channels/src/myplatform.rs`

2. Implement the `ChannelAdapter` trait (defined in `types.rs`):

```rust
use crate::types::{ChannelAdapter, ChannelMessage, ChannelType};
use async_trait::async_trait;

pub struct MyPlatformAdapter {
    // token, client, config fields
}

#[async_trait]
impl ChannelAdapter for MyPlatformAdapter {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("myplatform".to_string())
    }

    async fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Start polling/listening for messages
        Ok(())
    }

    async fn send(&self, channel_id: &str, content: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Send a message back to the platform
        Ok(())
    }

    async fn stop(&mut self) {
        // Clean shutdown
    }
}
```

3. Register the module in `crates/openfang-channels/src/lib.rs`:

```rust
pub mod myplatform;
```

4. Wire it up in the channel bridge (`crates/openfang-api/src/channel_bridge.rs`) so the daemon starts it alongside other adapters.

5. Add configuration support in `openfang-types` config structs (add a `[channels.myplatform]` section).

6. Add CLI setup wizard instructions in `crates/openfang-cli/src/main.rs` under `cmd_channel_setup`.

7. Write tests and submit a PR.

---

## How to Add a New Tool

Built-in tools are defined in `crates/openfang-runtime/src/tool_runner.rs`.

### Steps

1. Add the tool implementation function:

```rust
async fn tool_my_tool(input: &serde_json::Value) -> Result<String, String> {
    let param = input["param"]
        .as_str()
        .ok_or("Missing 'param' field")?;

    // Tool logic here
    Ok(format!("Result: {param}"))
}
```

2. Register it in the `execute_tool` match block:

```rust
"my_tool" => tool_my_tool(input).await,
```

3. Add the tool definition to `builtin_tool_definitions()`:

```rust
ToolDefinition {
    name: "my_tool".to_string(),
    description: "Description shown to the LLM.".to_string(),
    input_schema: serde_json::json!({
        "type": "object",
        "properties": {
            "param": {
                "type": "string",
                "description": "The parameter description"
            }
        },
        "required": ["param"]
    }),
},
```

4. Agents that need the tool must list it in their manifest:

```toml
[capabilities]
tools = ["my_tool"]
```

5. Write tests for the tool function.

6. If the tool requires kernel access (e.g., inter-agent communication), accept `Option<&Arc<dyn KernelHandle>>` and handle the `None` case gracefully.

---

## Pull Request Process

1. **Fork and branch**: Create a feature branch from `main`. Use descriptive names like `feat/add-matrix-adapter` or `fix/session-restore-crash`.

2. **Make your changes**: Follow the code style guidelines above.

3. **Test thoroughly**:
   - `cargo test --workspace` must pass (all 1,862+ tests).
   - `cargo clippy --workspace --all-targets -- -D warnings` must produce zero warnings.
   - `cargo fmt --all --check` must produce no diff.

4. **Write a clear PR description**: Explain what changed and why. Include before/after examples if applicable.

5. **One concern per PR**: Keep PRs focused. A single PR should address one feature, one bug fix, or one refactor -- not all three.

6. **Review process**: At least one maintainer must approve before merge. Address review feedback promptly.

7. **CI must pass**: All automated checks must be green before merge.

### Commit Messages

Use clear, imperative-mood messages:

```
Add Matrix channel adapter with E2EE support
Fix session restore crash on kernel reboot
Refactor capability manager to use DashMap
```

---

## Code of Conduct

This project follows the [Contributor Covenant Code of Conduct](https://www.contributor-covenant.org/version/2/1/code_of_conduct/). By participating, you agree to uphold a welcoming, inclusive, and harassment-free environment for everyone.

Please report unacceptable behavior to the maintainers.

---

## Questions?

- Open a [GitHub Discussion](https://github.com/ParadiseAI/maestro-legacy/discussions) for questions.
- Open a [GitHub Issue](https://github.com/ParadiseAI/maestro-legacy/issues) for bugs or feature requests.
- Check the [docs/](docs/) directory for detailed guides on specific topics.
