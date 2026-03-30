<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-cli

## Purpose

The CLI binary provides both daemon and interactive TUI (terminal UI) interfaces for OpenFang. Commands can start the daemon, launch an interactive dashboard (ratatui-based), spawn agents, manage skills/hands/channels, configure integrations, and run diagnostics. When a daemon is running, the CLI talks to it over HTTP. Otherwise, single-shot commands boot an in-process kernel. The TUI has 23 screens covering agents, skills, hands, channels, peers, workflows, settings, etc.

## Key Files

| File | Description |
|------|-------------|
| `src/main.rs` | CLI entry point, command dispatch (244KB file). Handles daemon startup, HTTP client, single-shot kernel. |
| `src/launcher.rs` | Daemon launcher logic — start/stop/restart, port binding, process management. |
| `src/tui/mod.rs` | TUI event loop and screen router. |
| `src/tui/screens/*.rs` | 23 TUI screens (agents, channels, hands, skills, peers, workflows, settings, etc). Each is a ratatui component. |
| `src/mcp.rs` | MCP (Model Context Protocol) server integration. |
| `src/dotenv.rs` | `.env` file loading for API keys. |
| `src/ui.rs` | UI helpers — formatting, colors, tables. |
| `src/table.rs` | Table rendering utilities. |
| `src/progress.rs` | Progress bar for long operations. |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `src/tui/screens/` | 23 ratatui screens: agents.rs, audit.rs, channels.rs, chat.rs, comms.rs, dashboard.rs, extensions.rs, hands.rs, init_wizard.rs, logs.rs, memory.rs, peers.rs, security.rs, sessions.rs, settings.rs, skills.rs, templates.rs, triggers.rs, usage.rs, welcome.rs, wizard.rs, workflows.rs. |
| `src/` | Launcher, TUI router, MCP, dotenv, CLI command handlers. |

## For AI Agents

### Working In This Directory

**CRITICAL: The user is actively building the CLI/TUI. Do NOT modify `src/tui/screens/` or core CLI logic without explicit permission.**

- **For TUI work**: Only modify `src/tui/screens/` if explicitly asked. Coordinate with the active builder.
- **For CLI commands**: Add new subcommands in `main.rs` following existing patterns (e.g., `openfang agent`, `openfang skill`).
- **For daemon integration**: Use `launcher.rs` to manage process lifecycle.
- **For HTTP client**: Commands use the HTTP API client defined in `main.rs` to talk to daemon.
- **For single-shot kernel**: Spawn `OpenFangKernel` in-process for commands that don't need daemon.

### Testing Requirements

- Unit tests for launcher (mock process management).
- Integration tests for CLI commands (mock daemon HTTP responses).
- TUI screen tests (mock ratatui backend).
- No live daemon tests — use HTTP mocks.

### Common Patterns

- CLI uses `clap` for argument parsing; commands are subcommands (e.g., `openfang agent new`).
- Daemon communication is HTTP against `http://127.0.0.1:4200` (configurable port).
- Single-shot mode boots kernel, runs command, exits — used when daemon isn't running.
- TUI is event-driven (ratatui): keyboard input → screen handlers → HTTP calls → re-render.
- Screens implement rendering and input handling separately.
- Daemon info (PID, port) is stored in `~/.openfang/daemon.pid` for IPC.
- MCP integration allows third-party tools to introspect OpenFang via standard protocol.

## Dependencies

### Internal

- `openfang-types`, `openfang-kernel`, `openfang-api`, `openfang-skills`, `openfang-runtime`, `openfang-extensions`, `openfang-migrate` — core system.

### External

- `clap`, `clap_complete` — CLI argument parsing, shell completion.
- `ratatui` — TUI rendering.
- `tokio` — async runtime.
- `reqwest` — HTTP client for daemon API.
- `serde`, `serde_json`, `toml` — config/message serialization.
- `tracing` — logging.
- `colored` — terminal colors.
- `dirs` — platform-specific config directories.
- `zeroize` — secure memory cleanup for API keys.
- `tempfile` — temporary files for tests.

<!-- MANUAL: -->
