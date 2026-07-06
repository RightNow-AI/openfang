//! Obsidian brain vault — list/read/write markdown files under a configured root.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use omtae_types::config::BrainConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use crate::routes::AppState;

const MAX_READ_BYTES: u64 = 2 * 1024 * 1024;
const MAX_WRITE_BYTES: usize = 512 * 1024;
const RECENT_LEADS_LIMIT: usize = 20;

#[derive(Debug, Clone, Serialize)]
pub struct BrainEntry {
    pub name: String,
    pub path: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrainListResponse {
    pub enabled: bool,
    pub vault_path: String,
    pub vault_name: String,
    pub path: String,
    pub entries: Vec<BrainEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recent_leads: Vec<BrainEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrainFileResponse {
    pub path: String,
    pub content: String,
    pub size: u64,
    pub modified: Option<String>,
    pub obsidian_uri: String,
}

#[derive(Debug, Deserialize)]
pub struct BrainPathQuery {
    #[serde(default)]
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct BrainWriteRequest {
    pub path: String,
    pub content: String,
}

fn system_time_to_rfc3339(time: SystemTime) -> Option<String> {
    use chrono::{DateTime, Utc};
    Some(DateTime::<Utc>::from(time).to_rfc3339())
}

fn vault_name_from_path(root: &Path) -> String {
    root.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("omtae-brain")
        .to_string()
}

fn percent_encode_uri(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'/' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn obsidian_uri(vault_name: &str, rel_path: &str) -> String {
    format!(
        "obsidian://open?vault={}&file={}",
        percent_encode_uri(vault_name),
        percent_encode_uri(rel_path)
    )
}

/// Resolve and canonicalize the configured vault root.
pub fn resolve_vault_root(brain: &BrainConfig) -> Result<PathBuf, String> {
    let raw = brain
        .path
        .clone()
        .unwrap_or_else(omtae_types::config::default_brain_vault_path);
    if !raw.exists() {
        std::fs::create_dir_all(&raw).map_err(|e| format!("create vault dir: {e}"))?;
    }
    raw.canonicalize()
        .map_err(|e| format!("resolve vault path {}: {e}", raw.display()))
}

/// Join a relative vault path and ensure it stays under the root.
pub fn resolve_safe_path(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let trimmed = rel.trim().trim_start_matches('/');
    if trimmed.is_empty() {
        return Ok(root.to_path_buf());
    }
    for component in Path::new(trimmed).components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => {}
            _ => return Err("path traversal not allowed".to_string()),
        }
    }
    let joined = root.join(trimmed);
    let canonical = if joined.exists() {
        joined
            .canonicalize()
            .map_err(|e| format!("resolve path: {e}"))?
    } else {
        joined
            .parent()
            .and_then(|p| p.canonicalize().ok())
            .map(|p| p.join(joined.file_name().unwrap_or_default()))
            .ok_or_else(|| "invalid path".to_string())?
    };
    if !canonical.starts_with(root) {
        return Err("path escapes vault root".to_string());
    }
    Ok(canonical)
}

fn rel_path(root: &Path, abs: &Path) -> String {
    abs.strip_prefix(root)
        .unwrap_or(abs)
        .to_string_lossy()
        .replace('\\', "/")
}

fn should_skip_name(name: &str) -> bool {
    name.starts_with('.') || name == "node_modules"
}

fn entry_from_path(root: &Path, abs: &Path, kind: &str) -> BrainEntry {
    let meta = std::fs::metadata(abs).ok();
    BrainEntry {
        name: abs
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string(),
        path: rel_path(root, abs),
        kind: kind.to_string(),
        size: meta.as_ref().map(|m| m.len()),
        modified: meta
            .and_then(|m| m.modified().ok())
            .and_then(system_time_to_rfc3339),
    }
}

fn list_directory(root: &Path, rel: &str) -> Result<Vec<BrainEntry>, String> {
    let dir = resolve_safe_path(root, rel)?;
    if !dir.is_dir() {
        return Err("not a directory".to_string());
    }
    let mut entries = Vec::new();
    for item in std::fs::read_dir(&dir).map_err(|e| format!("read dir: {e}"))? {
        let item = item.map_err(|e| format!("read entry: {e}"))?;
        let name = item.file_name().to_string_lossy().to_string();
        if should_skip_name(&name) {
            continue;
        }
        let path = item.path();
        let kind = if path.is_dir() { "dir" } else { "file" };
        entries.push(entry_from_path(root, &path, kind));
    }
    entries.sort_by(|a, b| {
        match (a.kind.as_str(), b.kind.as_str()) {
            ("dir", "file") => std::cmp::Ordering::Less,
            ("file", "dir") => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });
    Ok(entries)
}

fn recent_leads(root: &Path) -> Vec<BrainEntry> {
    let leads_dir = root.join("leads");
    if !leads_dir.is_dir() {
        return Vec::new();
    }
    let mut items: Vec<(SystemTime, BrainEntry)> = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(&leads_dir) else {
        return Vec::new();
    };
    for item in read_dir.flatten() {
        let path = item.path();
        if path.is_dir() {
            if let Ok(sub) = std::fs::read_dir(&path) {
                for sub_item in sub.flatten() {
                    let sub_path = sub_item.path();
                    if sub_path.is_file() && sub_path.extension().is_some_and(|e| e == "md") {
                        if let Ok(meta) = sub_path.metadata() {
                            if let Ok(modified) = meta.modified() {
                                items.push((modified, entry_from_path(root, &sub_path, "file")));
                            }
                        }
                    }
                }
            }
            continue;
        }
        if path.extension().is_some_and(|e| e == "md") {
            if let Ok(meta) = path.metadata() {
                if let Ok(modified) = meta.modified() {
                    items.push((modified, entry_from_path(root, &path, "file")));
                }
            }
        }
    }
    items.sort_by(|a, b| b.0.cmp(&a.0));
    items
        .into_iter()
        .take(RECENT_LEADS_LIMIT)
        .map(|(_, entry)| entry)
        .collect()
}

fn read_text_file(root: &Path, rel: &str, vault_name: &str) -> Result<BrainFileResponse, String> {
    let abs = resolve_safe_path(root, rel)?;
    if !abs.is_file() {
        return Err("not a file".to_string());
    }
    let meta = std::fs::metadata(&abs).map_err(|e| format!("stat file: {e}"))?;
    if meta.len() > MAX_READ_BYTES {
        return Err(format!("file too large (max {} bytes)", MAX_READ_BYTES));
    }
    let content = std::fs::read_to_string(&abs).map_err(|e| format!("read file: {e}"))?;
    let rel_s = rel_path(root, &abs);
    Ok(BrainFileResponse {
        path: rel_s.clone(),
        size: meta.len(),
        modified: meta.modified().ok().and_then(system_time_to_rfc3339),
        obsidian_uri: obsidian_uri(vault_name, &rel_s),
        content,
    })
}

fn write_text_file(root: &Path, rel: &str, content: &str) -> Result<BrainFileResponse, String> {
    if content.len() > MAX_WRITE_BYTES {
        return Err(format!("content too large (max {} bytes)", MAX_WRITE_BYTES));
    }
    let abs = resolve_safe_path(root, rel)?;
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create parent: {e}"))?;
    }
    let tmp = abs.with_extension("tmp");
    std::fs::write(&tmp, content.as_bytes()).map_err(|e| format!("write temp: {e}"))?;
    std::fs::rename(&tmp, &abs).map_err(|e| format!("commit write: {e}"))?;
    let vault_name = vault_name_from_path(root);
    read_text_file(root, rel, &vault_name)
}

fn brain_disabled() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({ "error": "brain vault disabled in config" })),
    )
}

fn brain_error(status: StatusCode, msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({ "error": msg })))
}

/// GET /api/brain/list — List vault directory entries.
pub async fn brain_list(
    State(state): State<Arc<AppState>>,
    Query(query): Query<BrainPathQuery>,
) -> impl IntoResponse {
    let brain = &state.kernel.config.brain;
    if !brain.enabled {
        return brain_disabled().into_response();
    }
    let root = match resolve_vault_root(brain) {
        Ok(r) => r,
        Err(e) => return brain_error(StatusCode::INTERNAL_SERVER_ERROR, &e).into_response(),
    };
    let vault_name = vault_name_from_path(&root);
    let rel = if query.path.is_empty() { "" } else { &query.path };
    let entries = match list_directory(&root, rel) {
        Ok(e) => e,
        Err(e) => return brain_error(StatusCode::BAD_REQUEST, &e).into_response(),
    };
    let recent = if rel.is_empty() {
        recent_leads(&root)
    } else {
        Vec::new()
    };
    Json(BrainListResponse {
        enabled: true,
        vault_path: root.display().to_string(),
        vault_name,
        path: rel.to_string(),
        entries,
        recent_leads: recent,
    })
    .into_response()
}

/// GET /api/brain/file — Read a vault file.
pub async fn brain_file(
    State(state): State<Arc<AppState>>,
    Query(query): Query<BrainPathQuery>,
) -> impl IntoResponse {
    let brain = &state.kernel.config.brain;
    if !brain.enabled {
        return brain_disabled().into_response();
    }
    if query.path.trim().is_empty() {
        return brain_error(StatusCode::BAD_REQUEST, "path required").into_response();
    }
    let root = match resolve_vault_root(brain) {
        Ok(r) => r,
        Err(e) => return brain_error(StatusCode::INTERNAL_SERVER_ERROR, &e).into_response(),
    };
    let vault_name = vault_name_from_path(&root);
    match read_text_file(&root, &query.path, &vault_name) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => brain_error(StatusCode::BAD_REQUEST, &e).into_response(),
    }
}

/// POST /api/brain/file — Write a vault file.
pub async fn brain_write(
    State(state): State<Arc<AppState>>,
    Json(body): Json<BrainWriteRequest>,
) -> impl IntoResponse {
    let brain = &state.kernel.config.brain;
    if !brain.enabled {
        return brain_disabled().into_response();
    }
    if body.path.trim().is_empty() {
        return brain_error(StatusCode::BAD_REQUEST, "path required").into_response();
    }
    let root = match resolve_vault_root(brain) {
        Ok(r) => r,
        Err(e) => return brain_error(StatusCode::INTERNAL_SERVER_ERROR, &e).into_response(),
    };
    match write_text_file(&root, &body.path, &body.content) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => brain_error(StatusCode::BAD_REQUEST, &e).into_response(),
    }
}

/// GET /api/brain/status — Vault metadata for dashboard header.
pub async fn brain_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let brain = &state.kernel.config.brain;
    let path = state.kernel.config.brain_vault_path();
    let exists = path.exists();
    let mut counts: HashMap<String, u64> = HashMap::new();
    if exists {
        if let Ok(root) = resolve_vault_root(brain) {
            for top in ["leads", "genetics", "history", "refining"] {
                let dir = root.join(top);
                if dir.is_dir() {
                    if let Ok(n) = dir.read_dir().map(|d| d.count() as u64) {
                        counts.insert(top.to_string(), n);
                    }
                }
            }
        }
    }
    Json(serde_json::json!({
        "enabled": brain.enabled,
        "vault_path": path.display().to_string(),
        "vault_name": path.file_name().and_then(|s| s.to_str()).unwrap_or("omtae-brain"),
        "exists": exists,
        "counts": counts,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn safe_path_blocks_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        fs::write(root.join("note.md"), "# hi").unwrap();
        assert!(resolve_safe_path(&root, "../etc/passwd").is_err());
        assert!(resolve_safe_path(&root, "note.md").is_ok());
    }

    #[test]
    fn list_and_read_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        fs::create_dir_all(root.join("leads")).unwrap();
        fs::write(root.join("leads/a.md"), "# A").unwrap();
        let entries = list_directory(&root, "leads").unwrap();
        assert_eq!(entries.len(), 1);
        let file = read_text_file(&root, "leads/a.md", "test-vault").unwrap();
        assert!(file.content.contains("# A"));
        assert!(file.obsidian_uri.contains("obsidian://open"));
    }
}
