use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use openfang_runtime::tool_runner::builtin_tool_definitions;
use openfang_types::agent::{AgentId, AgentManifest, ModelConfig};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use crate::{
    routes::AppState,
    types::{SpawnRequest, SpawnResponse},
};

#[derive(Debug, Clone, Serialize)]
pub struct BuiltinToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct BuiltinToolsResponse {
    pub tools: Vec<BuiltinToolInfo>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpServerInfo {
    pub name: String,
    pub enabled: bool,
    pub transport: McpTransportInfo,
    pub timeout_secs: u64,
    pub tools_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum McpTransportInfo {
    Stdio { command: String, args: Vec<String> },
    Sse { url: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct McpServersResponse {
    pub servers: Vec<McpServerInfo>,
    pub total: usize,
}

#[derive(Debug, Deserialize)]
pub struct SetMcpRequest {
    pub servers: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetWorkspaceRequest {
    /// New absolute path for the agent workspace.
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SetMcpResponse {
    pub status: String,
    pub servers: Vec<String>,
}

/// POST /api/agents — Spawn a new agent.
pub async fn spawn_agent(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SpawnRequest>,
) -> impl IntoResponse {
    // Resolve template name → manifest_toml if template is provided and manifest_toml is empty
    let manifest_toml = req.manifest_toml.clone();

    // SECURITY: Reject oversized manifests to prevent parser memory exhaustion.
    const MAX_MANIFEST_SIZE: usize = 1024 * 1024; // 1MB
    if manifest_toml.len() > MAX_MANIFEST_SIZE {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({"error": "Manifest too large (max 1MB)"})),
        );
    }

    // SECURITY: Verify Ed25519 signature when a signed manifest is provided
    if let Some(ref signed_json) = req.signed_manifest {
        match state.kernel.verify_signed_manifest(signed_json) {
            Ok(verified_toml) => {
                // Ensure the signed manifest matches the provided manifest_toml
                if verified_toml.trim() != manifest_toml.trim() {
                    tracing::warn!("Signed manifest content does not match manifest_toml");
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(
                            serde_json::json!({"error": "Signed manifest content does not match manifest_toml"}),
                        ),
                    );
                }
            }
            Err(e) => {
                tracing::warn!("Manifest signature verification failed: {e}");
                state.kernel.audit_log.record(
                    "system",
                    openfang_runtime::audit::AuditAction::AuthAttempt,
                    "manifest signature verification failed",
                    format!("error: {e}"),
                );
                return (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({"error": "Manifest signature verification failed"})),
                );
            }
        }
    }

    let mut manifest: AgentManifest = match toml::from_str(&manifest_toml) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("Invalid manifest TOML: {e}");
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid manifest format"})),
            );
        }
    };

    if manifest.model.provider.is_empty() || manifest.model.provider == "default" {
        let default_model = state.kernel.config.default_model.clone();
        let mut model = ModelConfig::default();
        model.provider = default_model.provider.clone();
        model.model = default_model.model.clone();
        if !default_model.api_key_env.is_empty() {
            model.api_key_env = Some(default_model.api_key_env.clone());
        }
        model.base_url = default_model.base_url.clone();
        manifest.model = model;
    }

    let name = manifest.name.clone();
    match state.kernel.spawn_agent(manifest) {
        Ok(id) => (
            StatusCode::CREATED,
            Json(serde_json::json!(SpawnResponse {
                agent_id: id.to_string(),
                name,
            })),
        ),
        Err(e) => {
            tracing::warn!("Spawn failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Agent spawn failed"})),
            )
        }
    }
}

pub async fn get_agent_builtin_tools_config(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let agent_id: AgentId = match id.parse() {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "                        agent ID"})),
            )
        }
    };

    let entry = match state.kernel.registry.get(agent_id) {
        Some(e) => e,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Agent not found"})),
            )
        }
    };

    let all_builtins = builtin_tool_definitions();
    let tool_allowlist = &entry.manifest.tool_allowlist;
    let tool_blocklist = &entry.manifest.tool_blocklist;

    let tools: Vec<Value> = all_builtins
        .iter()
        .map(|t| {
            let allowed = tool_allowlist.is_empty() || tool_allowlist.contains(&t.name);
            let not_blocked = !tool_blocklist.contains(&t.name);
            let available = allowed && not_blocked;

            serde_json::json!({
                "available": available,
                "name": t.name.clone(),
                "description": t.description.clone(),
                "input_schema": t.input_schema.clone(),
            })
        })
        .collect();

    let total = tools.len();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "tools": tools,
            "total": total,
        })),
    )
}

pub async fn get_agent_mcp_servers(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let agent_id: AgentId = match id.parse() {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid agent ID"})),
            )
        }
    };

    let entry = match state.kernel.registry.get(agent_id) {
        Some(e) => e,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Agent not found"})),
            )
        }
    };

    let config_servers: std::collections::HashSet<String> = state
        .kernel
        .config
        .mcp_servers
        .iter()
        .map(|s| s.name.clone())
        .collect();

    let mcp_tools = match state.kernel.mcp_tools.lock() {
        Ok(tools) => tools,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to lock MCP tools"})),
            )
        }
    };

    let mut server_tools_count: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for tool in mcp_tools.iter() {
        if let Some(server_name) = extract_mcp_server_name(&tool.name) {
            *server_tools_count
                .entry(server_name.to_string())
                .or_insert(0) += 1;
        }
    }

    let enabled_servers: std::collections::HashSet<String> =
        entry.manifest.mcp_servers.iter().cloned().collect();

    let is_all_mode = entry.manifest.mcp_servers.is_empty();

    let servers: Vec<McpServerInfo> = state
        .kernel
        .config
        .mcp_servers
        .iter()
        .map(|s| {
            let enabled = if is_all_mode {
                config_servers.contains(&s.name)
            } else {
                enabled_servers.contains(&s.name)
            };

            let transport = match &s.transport {
                openfang_types::config::McpTransportEntry::Stdio { command, args } => {
                    McpTransportInfo::Stdio {
                        command: command.clone(),
                        args: args.clone(),
                    }
                }
                openfang_types::config::McpTransportEntry::Sse { url } => {
                    McpTransportInfo::Sse { url: url.clone() }
                }
            };

            McpServerInfo {
                name: s.name.clone(),
                enabled,
                transport,
                timeout_secs: s.timeout_secs,
                tools_count: *server_tools_count.get(&s.name).unwrap_or(&0),
            }
        })
        .collect();

    let total = servers.len();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "servers": servers,
            "total": total,
        })),
    )
}

pub async fn set_agent_mcp_servers(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<SetMcpRequest>,
) -> impl IntoResponse {
    let agent_id: AgentId = match id.parse() {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid agent ID"})),
            )
        }
    };

    match state
        .kernel
        .set_agent_mcp_servers(agent_id, req.servers.clone())
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "servers": req.servers,
            })),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!("{e}")})),
        ),
    }
}

fn extract_mcp_server_name(tool_name: &str) -> Option<&str> {
    if tool_name.starts_with("mcp_") {
        let parts: Vec<&str> = tool_name.splitn(2, '_').collect();
        if parts.len() >= 2 {
            return Some(parts[1]);
        }
    }
    None
}

/// PUT /api/agents/:id/workspace — Change an agent's workspace directory.
///
/// Moves all existing workspace files to the new path, then updates the
/// in-memory registry. The move first tries an atomic `rename` (same
/// filesystem); on cross-device failure it falls back to a recursive copy
/// followed by removal of the old directory.
pub async fn set_agent_workspace(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<SetWorkspaceRequest>,
) -> impl IntoResponse {
    let agent_id: AgentId = match id.parse() {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid agent ID"})),
            )
        }
    };

    let entry = match state.kernel.registry.get(agent_id) {
        Some(e) => e,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Agent not found"})),
            )
        }
    };

    let new_path = std::path::PathBuf::from(&req.path);

    if !new_path.is_absolute() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Workspace path must be absolute"})),
        );
    }

    // Reject path traversal components
    if req.path.contains("..") {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Path traversal not allowed"})),
        );
    }

    let old_path = entry.manifest.workspace.clone();

    // Release the registry lock before any blocking file I/O
    drop(entry);

    // No-op when paths are identical
    if old_path.as_deref() == Some(new_path.as_path()) {
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "old_workspace": new_path.display().to_string(),
                "new_workspace": new_path.display().to_string(),
                "files_moved": 0,
            })),
        );
    }

    // Prevent moving a workspace into one of its own subdirectories —
    // move_dir_recursive would encounter the destination while iterating
    // the source, causing infinite directory creation.
    if let Some(ref old) = old_path {
        if new_path.starts_with(old) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "New workspace path cannot be inside the current workspace"
                })),
            );
        }
    }

    let files_moved: usize = if let Some(ref old) = old_path {
        if old.exists() {
            let file_count = count_files_recursive(old);

            // Prefer atomic rename of the whole tree (same filesystem, zero
            // copies). On cross-device failure, move each entry individually:
            // rename per file, only copy+remove_file when rename is unavailable.
            if std::fs::rename(old, &new_path).is_err() {
                if let Err(e) = move_dir_recursive(old, &new_path) {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            serde_json::json!({"error": format!("Failed to move workspace: {e}")}),
                        ),
                    );
                }
                // Source root is now empty; remove it.
                if let Err(e) = std::fs::remove_dir(old) {
                    tracing::warn!("Failed to remove old workspace {}: {e}", old.display());
                }
            }

            file_count
        } else {
            // Old path recorded but missing on disk — just create the new dir
            if let Err(e) = std::fs::create_dir_all(&new_path) {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": format!("Failed to create workspace: {e}")})),
                );
            }
            0
        }
    } else {
        // No previous workspace — create the new directory
        if let Err(e) = std::fs::create_dir_all(&new_path) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to create workspace: {e}")})),
            );
        }
        0
    };

    if let Err(e) = state
        .kernel
        .registry
        .update_workspace(agent_id, Some(new_path.clone()))
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to update registry: {e}")})),
        );
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "ok",
            "old_workspace": old_path.as_ref().map(|p| p.display().to_string()),
            "new_workspace": new_path.display().to_string(),
            "files_moved": files_moved,
        })),
    )
}

fn count_files_recursive(dir: &std::path::Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(ft) = entry.file_type() {
                if ft.is_file() {
                    count += 1;
                } else if ft.is_dir() {
                    count += count_files_recursive(&entry.path());
                }
            }
        }
    }
    count
}

fn move_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        // Skip entries that are on the path to dst (infinite-recursion guard).
        if dst.starts_with(&src_path) {
            continue;
        }
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            move_dir_recursive(&src_path, &dst_path)?;
            // Directory is now empty; remove it.
            std::fs::remove_dir(&src_path)?;
        } else {
            // Try atomic rename first; only copy+remove when cross-device.
            if std::fs::rename(&src_path, &dst_path).is_err() {
                std::fs::copy(&src_path, &dst_path)?;
                std::fs::remove_file(&src_path)?;
            }
        }
    }
    Ok(())
}
