<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# CLI Source Code

## Purpose
Command-line interface for OpenFang Agent OS. Supports daemon management (`openfang start`), interactive chat, agent creation, config, integrations, diagnostics, and TUI (terminal UI) dashboard.

## Key Files
| File | Purpose |
|------|---------|
| `main.rs` | CLI entry point: command parsing (clap), daemon lifecycle, HTTP client for remote daemon, in-process kernel for single-shot commands |
| `launcher.rs` | Daemon startup: fork process, write daemon info to `~/.openfang/daemon.json`, wait for health check |
| `dotenv.rs` | Load environment variables from `.env` and `.env.local` |
| `mcp.rs` | Model Context Protocol (MCP) stdio server for IDE integration |
| `bundled_agents.rs` | Built-in agent templates (engineer, analyst, researcher, etc.) |
| `progress.rs` | Progress spinner and status reporting |
| `table.rs` | Terminal table rendering (agents, models, skills, etc.) |
| `templates.rs` | Agent and workflow template definitions |
| `ui.rs` | Terminal UI utilities |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `tui/` | Interactive terminal UI dashboard (event loop, panes, input handling) |

## For AI Agents
When modifying the CLI:
- Commands are parsed by clap: add new subcommands in `main.rs` enum
- Daemon mode: CLI forks and talks to daemon over HTTP (routes in `openfang-api`)
- Single-shot mode: CLI boots in-process kernel directly (no daemon) for one-off commands
- TUI dashboard is separate interactive mode: see `tui/` subdirectory
- Config path is always `~/.openfang/config.toml`: respect this convention
- Daemon info written to `~/.openfang/daemon.json`: CLI reads to find daemon port
- MCP server runs on stdio for IDE integration: implements Model Context Protocol
- Test commands: `openfang agent new <template>`, `openfang chat`, `openfang start`, `openfang tui`
