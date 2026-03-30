<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-desktop

## Purpose
Native Tauri 2.0 desktop application that boots the OpenFang kernel and embedded HTTP API server in-process, then displays the WebUI in a native window. Provides system tray integration, single-instance enforcement, OS notifications, global hotkeys, auto-start, and auto-update capabilities.

## Key Files
| File | Description |
|------|-------------|
| `src/lib.rs` | Tauri app setup: kernel boot, server binding, window creation, notification forwarding from kernel event bus. |
| `src/main.rs` | Entry point. Invokes `openfang_desktop::run()`. |
| `src/server.rs` | Embedded server lifecycle: boots kernel on background thread with dedicated tokio runtime, handles graceful shutdown. |
| `src/commands.rs` | Tauri IPC handlers: `get_port`, `get_status`, `get_agent_count`, `import_agent_toml`, `import_skill_file`, autostart, updates, file dialogs. |
| `src/tray.rs` | System tray menu (desktop only): show/hide window, quit application. |
| `src/shortcuts.rs` | Global keyboard shortcuts registration (desktop only, non-fatal on failure). |
| `src/updater.rs` | Background update checker and installer (desktop only). |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/` | Tauri app and server lifecycle code. |
| `capabilities/` | Tauri v2 granular permissions (read/write/execute scopes). |
| `gen/schemas/` | Auto-generated TypeScript type definitions from Rust commands. |
| `icons/` | PNG icons for window and tray. |

## For AI Agents

### Working In This Directory
- `lib.rs` is the core: kernel boot → server bind → Tauri builder setup → window creation.
- Server runs on a dedicated OS thread with its own tokio runtime; do not block the main Tauri event loop.
- `PortState` and `KernelState` are managed state; accessed via `app.state::<T>()` in commands.
- Kernel event bus is subscribed to in `setup()` — only critical events (crashes, quota, shutdown) trigger OS notifications.
- Window navigation is to `http://127.0.0.1:{port}` (embedded server), not file:// assets. Do NOT define windows in `tauri.conf.json`.
- Commands in `commands.rs` are async Tauri handlers; they receive kernel via `KernelState`.
- Tray and shortcuts are desktop-only; guard with `#[cfg(desktop)]`.

### Testing Requirements
- Verify kernel boots and server binds to a port (check via `curl http://127.0.0.1:{port}/api/health`).
- Test single-instance enforcement: launching a second instance should focus the existing window.
- Verify system tray appears on desktop and responds to clicks.
- Check that critical kernel events (agent crash, quota hit, kernel stop) trigger desktop notifications.
- Verify the Tauri window closes to tray (not quit) and can be restored from tray.

### Common Patterns
- Kernel events are received via `event_bus.subscribe_all()` on a spawned async task.
- Close-to-tray pattern: `on_window_event` intercepts `CloseRequested`, calls `window.hide()`, and prevents close.
- Graceful shutdown: `ServerHandle::shutdown()` is called when Tauri app exits.
- Commands use `state.kernel.method()` to invoke kernel operations (e.g., load config, import agents).

## Dependencies

### Internal
- `openfang-kernel` — kernel instance and event bus.
- `openfang-api` — HTTP router (via `build_router()`).
- `openfang-types` — event types, config structures.

### External
- `tauri` (v2) with plugins: `notification`, `shell`, `single-instance`, `autostart`, `updater`, `dialog`, `global-shortcut`.
- `tokio` + `axum` — async runtime and HTTP server (for embedded API).
- `serde_json` — JSON serialization for config/state.
- `tracing`/`tracing-subscriber` — structured logging.
- `toml` — TOML config parsing.
- `open` — open external URLs (e.g., config directory in file explorer).

<!-- MANUAL: -->
