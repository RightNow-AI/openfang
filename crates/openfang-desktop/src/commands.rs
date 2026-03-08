//! Tauri IPC command handlers.

use crate::{KernelState, PortState};
use openfang_kernel::config::openfang_home;
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_dialog::DialogExt;
use tracing::info;

/// Get the port the embedded server is listening on.
#[tauri::command]
pub fn get_port(port: tauri::State<'_, PortState>) -> u16 {
    port.0
}

/// Get a status summary of the running kernel.
#[tauri::command]
pub fn get_status(
    port: tauri::State<'_, PortState>,
    kernel_state: tauri::State<'_, KernelState>,
) -> serde_json::Value {
    let agents = kernel_state.kernel.registry.list().len();
    let uptime_secs = kernel_state.started_at.elapsed().as_secs();

    serde_json::json!({
        "status": "running",
        "port": port.0,
        "agents": agents,
        "uptime_secs": uptime_secs,
    })
}

/// Get the number of registered agents.
#[tauri::command]
pub fn get_agent_count(kernel_state: tauri::State<'_, KernelState>) -> usize {
    kernel_state.kernel.registry.list().len()
}

/// Open a native file picker to import an agent TOML manifest.
///
/// Validates the TOML as a valid `AgentManifest`, copies it to
/// `~/.openfang/agents/{name}/agent.toml`, then spawns the agent.
#[tauri::command]
pub async fn import_agent_toml(
    app: tauri::AppHandle,
    kernel_state: tauri::State<'_, KernelState>,
) -> Result<String, String> {
    let path = app
        .dialog()
        .file()
        .set_title("Import Agent Manifest")
        .add_filter("TOML files", &["toml"])
        .blocking_pick_file();

    let file_path = match path {
        Some(p) => p,
        None => return Err("No file selected".to_string()),
    };

    let content = std::fs::read_to_string(file_path.as_path().ok_or("Invalid file path")?)
        .map_err(|e| format!("Failed to read file: {e}"))?;

    let manifest: openfang_types::agent::AgentManifest =
        toml::from_str(&content).map_err(|e| format!("Invalid agent manifest: {e}"))?;

    let agent_name = manifest.name.clone();
    let agent_dir = openfang_home().join("agents").join(&agent_name);
    std::fs::create_dir_all(&agent_dir)
        .map_err(|e| format!("Failed to create agent directory: {e}"))?;

    let dest = agent_dir.join("agent.toml");
    std::fs::write(&dest, &content).map_err(|e| format!("Failed to write manifest: {e}"))?;

    kernel_state
        .kernel
        .spawn_agent(manifest).await
        .map_err(|e| format!("Failed to spawn agent: {e}"))?;

    info!("Imported and spawned agent \"{agent_name}\"");
    Ok(agent_name)
}

/// Open a native file picker to import a skill file.
///
/// Copies the selected file to `~/.openfang/skills/` and triggers a
/// hot-reload of the skill registry.
#[tauri::command]
pub fn import_skill_file(
    app: tauri::AppHandle,
    kernel_state: tauri::State<'_, KernelState>,
) -> Result<String, String> {
    let path = app
        .dialog()
        .file()
        .set_title("Import Skill File")
        .add_filter("Skill files", &["md", "toml", "py", "js", "wasm"])
        .blocking_pick_file();

    let file_path = match path {
        Some(p) => p,
        None => return Err("No file selected".to_string()),
    };

    let src = file_path.as_path().ok_or("Invalid file path")?;
    let file_name = src
        .file_name()
        .ok_or("No filename")?
        .to_string_lossy()
        .to_string();

    let skills_dir = openfang_home().join("skills");
    std::fs::create_dir_all(&skills_dir)
        .map_err(|e| format!("Failed to create skills directory: {e}"))?;

    let dest = skills_dir.join(&file_name);
    std::fs::copy(src, &dest).map_err(|e| format!("Failed to copy skill file: {e}"))?;

    kernel_state.kernel.reload_skills();

    info!("Imported skill file \"{file_name}\" and reloaded registry");
    Ok(file_name)
}

/// Check whether auto-start on login is enabled.
#[tauri::command]
pub fn get_autostart(app: tauri::AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

/// Enable or disable auto-start on login.
#[tauri::command]
pub fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<bool, String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| e.to_string())?;
    } else {
        manager.disable().map_err(|e| e.to_string())?;
    }
    manager.is_enabled().map_err(|e| e.to_string())
}

/// Perform an on-demand update check.
#[tauri::command]
pub async fn check_for_updates(
    app: tauri::AppHandle,
) -> Result<crate::updater::UpdateInfo, String> {
    crate::updater::check_for_update(&app).await
}

/// Download and install the latest update, then restart the app.
/// Returns Ok(()) which triggers an app restart — the command will not return
/// if the update succeeds (the app restarts). On error, returns Err(message).
#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    crate::updater::download_and_install_update(&app).await
}

/// Open the OpenFang config directory (`~/.openfang/`) in the OS file manager.
#[tauri::command]
pub fn open_config_dir() -> Result<(), String> {
    let dir = openfang_home();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create config dir: {e}"))?;
    open::that(&dir).map_err(|e| format!("Failed to open directory: {e}"))
}

/// Open the OpenFang logs directory (`~/.openfang/logs/`) in the OS file manager.
#[tauri::command]
pub fn open_logs_dir() -> Result<(), String> {
    let dir = openfang_home().join("logs");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create logs dir: {e}"))?;
    open::that(&dir).map_err(|e| format!("Failed to open directory: {e}"))
}
