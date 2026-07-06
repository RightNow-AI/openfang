//! System tray setup for the OMTAE desktop app.

use omtae_kernel::config::omtae_home;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_notification::NotificationExt;
use tracing::{info, warn};

/// Format seconds into a human-readable uptime string.
fn format_uptime(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        let m = secs / 60;
        let s = secs % 60;
        format!("{m}m {s}s")
    } else {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        format!("{h}h {m}m")
    }
}

/// Build and register the system tray icon with enhanced menu.
pub fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    // Action items
    let show = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
    let browser = MenuItem::with_id(app, "browser", "Open in Browser", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;

    // Informational items (disabled — display only)
    let agent_count = if let Some(ks) = app.try_state::<crate::KernelState>() {
        ks.kernel.registry.list().len()
    } else {
        0
    };
    let uptime = if let Some(ks) = app.try_state::<crate::KernelState>() {
        format_uptime(ks.started_at.elapsed().as_secs())
    } else {
        "0s".to_string()
    };
    let agents_info = MenuItem::with_id(
        app,
        "agents_info",
        format!("Agents: {agent_count} running"),
        false,
        None::<&str>,
    )?;
    let status_info = MenuItem::with_id(
        app,
        "status_info",
        format!("Status: Running ({uptime})"),
        false,
        None::<&str>,
    )?;
    let sep2 = PredefinedMenuItem::separator(app)?;

    // Settings items
    let autostart_enabled = app.autolaunch().is_enabled().unwrap_or(false);
    let launch_at_login = CheckMenuItem::with_id(
        app,
        "launch_at_login",
        "Launch at Login",
        true,
        autostart_enabled,
        None::<&str>,
    )?;
    let check_updates = MenuItem::with_id(
        app,
        "check_updates",
        "Check for Updates...",
        true,
        None::<&str>,
    )?;
    let open_config = MenuItem::with_id(
        app,
        "open_config",
        "Open Config Directory",
        true,
        None::<&str>,
    )?;
    let sep3 = PredefinedMenuItem::separator(app)?;

    let quit = MenuItem::with_id(app, "quit", "Quit OMTAE", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &show,
            &browser,
            &sep1,
            &agents_info,
            &status_info,
            &sep2,
            &launch_at_login,
            &check_updates,
            &open_config,
            &sep3,
            &quit,
        ],
    )?;

    // Load the tray icon from embedded PNG bytes
    let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))
        .expect("Failed to decode tray icon PNG");

    let _tray = TrayIconBuilder::new()
        .icon(tray_icon)
        .menu(&menu)
        .tooltip("OMTAE Agent OS")
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "show" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
            }
            "browser" => {
                if let Some(port) = app.try_state::<crate::PortState>() {
                    let url = format!("http://127.0.0.1:{}", port.0);
                    let _ = open::that(&url);
                }
            }
            "launch_at_login" => {
                let manager = app.autolaunch();
                let currently_enabled = manager.is_enabled().unwrap_or(false);
                if currently_enabled {
                    if let Err(e) = manager.disable() {
                        warn!("Failed to disable autostart: {e}");
                    }
                } else if let Err(e) = manager.enable() {
                    warn!("Failed to enable autostart: {e}");
                }
                info!(
                    "Autostart toggled: {}",
                    manager.is_enabled().unwrap_or(false)
                );
            }
            "check_updates" => {
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    // First check what's available
                    match crate::updater::check_for_update(&app_handle).await {
                        Ok(info) if info.available => {
                            let version = info.version.as_deref().unwrap_or("unknown");
                            // Notify user we're starting install
                            let _ = app_handle
                                .notification()
                                .builder()
                                .title("Installing Update...")
                                .body(format!(
                                    "Downloading OMTAE v{version}. App will restart shortly."
                                ))
                                .show();
                            // Perform install
                            if let Err(e) =
                                crate::updater::download_and_install_update(&app_handle).await
                            {
                                warn!("Manual update install failed: {e}");
                                let _ = app_handle
                                    .notification()
                                    .builder()
                                    .title("Update Failed")
                                    .body(format!("Could not install update: {e}"))
                                    .show();
                            }
                            // If we reach here, install failed (success causes restart)
                        }
                        Ok(_) => {
                            let _ = app_handle
                                .notification()
                                .builder()
                                .title("Up to Date")
                                .body("You're running the latest version of OMTAE.")
                                .show();
                        }
                        Err(e) => {
                            warn!("Tray update check failed: {e}");
                            let _ = app_handle
                                .notification()
                                .builder()
                                .title("Update Check Failed")
                                .body("Could not check for updates. Try again later.")
                                .show();
                        }
                    }
                });
            }
            "open_config" => {
                let dir = omtae_home();
                let _ = std::fs::create_dir_all(&dir);
                if let Err(e) = open::that(&dir) {
                    warn!("Failed to open config dir: {e}");
                }
            }
            "quit" => {
                info!("Quit requested from system tray");
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}
