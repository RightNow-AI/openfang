<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# OpenFang

## Purpose
Open-source Agent Operating System built in Rust (v0.5.1). 14 workspace crates, 137K+ LOC, 1,767+ tests. Compiles to a single binary that runs autonomous AI agents with a web dashboard, REST API, CLI, and multi-provider LLM support (Groq, OpenAI, Anthropic, Ollama, etc.).

## Key Files

| File | Description |
|------|-------------|
| `Cargo.toml` | Workspace manifest defining all 14 crates and shared dependencies |
| `Cargo.lock` | Dependency lock file |
| `CLAUDE.md` | AI agent instructions for working in this repo |
| `README.md` | Project overview, architecture, and usage guide |
| `CHANGELOG.md` | Version history and release notes |
| `CONTRIBUTING.md` | Contribution guidelines |
| `MIGRATION.md` | Migration guide between versions |
| `SECURITY.md` | Security policy and vulnerability reporting |
| `LICENSE-APACHE` | Apache 2.0 license |
| `LICENSE-MIT` | MIT license |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `crates/` | 14 Rust crates forming the core system (see `crates/AGENTS.md`) |
| `agents/` | 33 pre-configured agent definitions with TOML manifests (see `agents/AGENTS.md`) |
| `docs/` | Project documentation and benchmarks (see `docs/AGENTS.md`) |
| `sdk/` | JavaScript and Python client SDKs (see `sdk/AGENTS.md`) |
| `scripts/` | Build, deploy, and Docker scripts (see `scripts/AGENTS.md`) |
| `deploy/` | Deployment configurations (see `deploy/AGENTS.md`) |
| `packages/` | Auxiliary packages like WhatsApp gateway (see `packages/AGENTS.md`) |
| `public/` | Static assets (logo, images) |
| `xtask/` | Cargo xtask build automation (see `xtask/AGENTS.md`) |
| `.github/` | GitHub Actions workflows and issue templates |

## For AI Agents

### Working In This Directory
- Build: `cargo build --workspace --lib` (use `--lib` if daemon has exe locked)
- Test: `cargo test --workspace`
- Lint: `cargo clippy --workspace --all-targets -- -D warnings` (zero warnings required)
- All three checks MUST pass after every change
- Config lives at `~/.openfang/config.toml`; default API at `http://127.0.0.1:4200`
- Do NOT touch `openfang-cli` — user is actively building the interactive CLI

### Testing Requirements
- Run `cargo build --workspace --lib && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
- After new endpoints/features: run live integration tests (see CLAUDE.md for full protocol)

### Common Patterns
- `KernelHandle` trait avoids circular deps between runtime and kernel
- `AppState` in `server.rs` bridges kernel to API routes
- New routes: register in `server.rs` router AND implement in `routes.rs`
- Config fields: struct field + `#[serde(default)]` + Default impl entry
- Dashboard: Alpine.js SPA in `static/index_body.html`

## Dependencies

### External (Key)
- `tokio` — async runtime
- `axum` + `tower-http` — HTTP server
- `reqwest` — HTTP client for LLM providers
- `rusqlite` — SQLite database
- `rmcp` — MCP protocol client
- `wasmtime` — WASM sandbox for extensions
- `ratatui` — TUI for interactive CLI
- `ed25519-dalek` — cryptographic signing for OFP network

<!-- MANUAL: -->
