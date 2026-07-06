//! Update checker for the OMTAE desktop app.

use serde::Serialize;
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::UpdaterExt;
use tracing::{info, warn};

/// Structured result from an update check.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    /// Whether a newer version is available.
    pub available: bool,
    /// The new version string, if available.
    pub version: Option<String>,
    /// Release notes body, if available.
    pub body: Option<String>,
}

/// Spawn a background task that checks for updates after a 10-second delay.
///
/// If an update is found, installs it silently and restarts the app.
/// All errors are logged but never panic.
pub fn spawn_startup_check(app_handle: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        match do_check(&app_handle).await {
            Ok(info) if info.available => {
                let version = info.version.as_deref().unwrap_or("unknown");
                info!("Update available: v{version}, installing silently...");
                // Notify user first, then install
                let _ = app_handle
                    .notification()
                    .builder()
                    .title("OMTAE Updating...")
                    .body(format!("Installing v{version}. App will restart shortly."))
                    .show();
                // Small delay so notification is visible
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                if let Err(e) = download_and_install_update(&app_handle).await {
                    warn!("Auto-update install failed: {e}");
                }
            }
            Ok(_) => info!("No updates available"),
            Err(e) => warn!("Startup update check failed: {e}"),
        }
    });
}

/// Perform an on-demand update check. Returns structured result.
pub async fn check_for_update(app_handle: &tauri::AppHandle) -> Result<UpdateInfo, String> {
    do_check(app_handle).await
}

/// Download and install the latest update, then restart the app.
/// Should only be called after `check_for_update()` confirms availability.
///
/// On success, calls `app_handle.restart()` which terminates the process —
/// the function never returns `Ok`. On failure, returns `Err(message)`.
pub async fn download_and_install_update(app_handle: &tauri::AppHandle) -> Result<(), String> {
    let updater = app_handle.updater().map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No update available".to_string())?;

    info!("Downloading update v{}...", update.version);
    update
        .download_and_install(|_downloaded, _total| {}, || {})
        .await
        .map_err(|e| e.to_string())?;

    info!("Update installed, restarting...");
    app_handle.restart()
}

async fn do_check(app_handle: &tauri::AppHandle) -> Result<UpdateInfo, String> {
    let updater = app_handle.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => Ok(UpdateInfo {
            available: true,
            version: Some(update.version.clone()),
            body: update.body.clone(),
        }),
        Ok(None) => Ok(UpdateInfo {
            available: false,
            version: None,
            body: None,
        }),
        Err(e) => Err(e.to_string()),
    }
}
