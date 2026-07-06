//! System-wide keyboard shortcuts for the OMTAE desktop app.

use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, Modifiers, ShortcutState};
use tracing::warn;

/// Build the global shortcut plugin with 3 system-wide shortcuts:
///
/// - `Ctrl+Shift+O` — Show/focus the OMTAE window
/// - `Ctrl+Shift+N` — Show window + navigate to agents page
/// - `Ctrl+Shift+C` — Show window + navigate to chat page
///
/// Returns `Result` so `lib.rs` can handle registration failure gracefully.
pub fn build_shortcut_plugin<R: tauri::Runtime>(
) -> Result<tauri::plugin::TauriPlugin<R>, tauri_plugin_global_shortcut::Error> {
    let plugin = tauri_plugin_global_shortcut::Builder::new()
        .with_shortcuts(["ctrl+shift+o", "ctrl+shift+n", "ctrl+shift+c"])?
        .with_handler(|app, shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }

            // All shortcuts show/focus the window first
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }

            if shortcut.matches(Modifiers::CONTROL | Modifiers::SHIFT, Code::KeyN) {
                if let Err(e) = app.emit("navigate", "agents") {
                    warn!("Failed to emit navigate event: {e}");
                }
            } else if shortcut.matches(Modifiers::CONTROL | Modifiers::SHIFT, Code::KeyC) {
                if let Err(e) = app.emit("navigate", "chat") {
                    warn!("Failed to emit navigate event: {e}");
                }
            }
            // Ctrl+Shift+O just shows the window (already done above)
        })
        .build();

    Ok(plugin)
}
