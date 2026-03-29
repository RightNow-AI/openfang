<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-desktop — Native Tauri Desktop App

## Purpose

Native desktop wrapper for OpenFang using Tauri 2.0. Boots the kernel and embedded API server, opens a native window pointing at the WebUI, and provides system integration (tray, notifications, global shortcuts, auto-start, auto-update).

## Key Files

| File | Purpose |
|------|---------|
| `lib.rs` | Main Tauri app setup — managed state (KernelState, PortState), window builder, event handlers |
| `main.rs` | Entry point — calls `run()` from lib.rs |
| `server.rs` | Embedded HTTP server — boots kernel + REST API on available port |
| `commands.rs` | Tauri commands — frontend→native bridge (get port, import TOML, check updates, etc.) |
| `tray.rs` | System tray — minimize-to-tray, context menu |
| `shortcuts.rs` | Global keyboard shortcuts — keybindings for common actions |
| `updater.rs` | Auto-update checker — Tauri updater plugin integration |

## For AI Agents

**When to read:** Understand desktop app integration, Tauri command handlers, or system notifications.

**Key interfaces:**
- `OpenFangKernel` — core kernel instance managed by Tauri state
- `PortState` — tracks which port the embedded server listens on
- `KernelState` — managed state for kernel + startup time
- Tauri commands — frontend invokes native functionality

**Common tasks:**
- Adding new frontend↔native commands → implement in `commands.rs` + register in `lib.rs` handler list
- Adding system notifications → subscribe to `kernel.event_bus` and send via Tauri notification plugin
- Modifying tray menu → `tray.rs` context menu builder
- Adding global shortcut → `shortcuts.rs`

**Event forwarding:** The app subscribes to kernel events (crashes, quota enforcement) and forwards only critical ones as OS notifications.

**Architecture notes:**
- Server is embedded in the binary — no separate backend process
- Port is dynamically chosen to avoid conflicts
- Window points directly at `http://127.0.0.1:{port}` — no asset loading issues
