//! Agent workspace file listing and download helpers.

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::path::{Component, Path, PathBuf};

/// Metadata returned by the workspace file listing endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceFileEntry {
    pub rel_path: String,
    pub size_bytes: u64,
    pub modified_at: Option<DateTime<Utc>>,
    pub mime_type: String,
}

/// Resolved file ready for download.
#[derive(Debug, Clone)]
pub struct WorkspaceDownload {
    pub path: PathBuf,
    pub rel_path: String,
    pub size_bytes: u64,
    pub mime_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceFileError {
    NoWorkspace,
    NotFound,
    Forbidden,
    Io(String),
}

impl std::fmt::Display for WorkspaceFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoWorkspace => write!(f, "Workspace not configured"),
            Self::NotFound => write!(f, "File not found"),
            Self::Forbidden => write!(f, "Path is outside the workspace"),
            Self::Io(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for WorkspaceFileError {}

/// List files under an agent workspace.
///
/// TODO(#1181): consult file_policy once it lands.
pub fn list_files(
    workspace_root: &Path,
    state_dir: Option<&Path>,
    prefix: Option<&str>,
    ext_filter: Option<&[String]>,
) -> Result<Vec<WorkspaceFileEntry>, WorkspaceFileError> {
    let root = canonical_dir(workspace_root)?;
    let state = canonical_optional(state_dir);
    let prefix_rel = normalize_rel_path(prefix.unwrap_or(""))?;
    let start = if prefix_rel.as_os_str().is_empty() {
        root.clone()
    } else {
        root.join(&prefix_rel)
    };

    let start = match start.canonicalize() {
        Ok(path) => path,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(WorkspaceFileError::Io(e.to_string())),
    };
    if !start.starts_with(&root) {
        return Err(WorkspaceFileError::Forbidden);
    }

    let mut entries = Vec::new();
    for entry in walkdir::WalkDir::new(start)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            let path = entry.path();
            path == root || !has_hidden_component(path.strip_prefix(&root).unwrap_or(path))
        })
    {
        let entry = entry.map_err(|e| WorkspaceFileError::Io(e.to_string()))?;
        let file_type = entry.file_type();
        if !file_type.is_file() {
            continue;
        }

        let path = entry.path();
        let canonical = match path.canonicalize() {
            Ok(path) => path,
            Err(_) => continue,
        };
        if !canonical.starts_with(&root)
            || is_private_state_path(&canonical, &root, state.as_deref())
        {
            continue;
        }

        let rel_path = canonical
            .strip_prefix(&root)
            .map_err(|_| WorkspaceFileError::Forbidden)?;
        if !matches_ext_filter(rel_path, ext_filter) {
            continue;
        }

        let metadata =
            std::fs::metadata(&canonical).map_err(|e| WorkspaceFileError::Io(e.to_string()))?;
        entries.push(WorkspaceFileEntry {
            rel_path: rel_path_to_string(rel_path),
            size_bytes: metadata.len(),
            modified_at: metadata.modified().ok().map(DateTime::<Utc>::from),
            mime_type: mime_type_for_path(rel_path).to_string(),
        });
    }

    entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(entries)
}

/// Resolve a workspace-relative path for download.
///
/// TODO(#1181): consult file_policy once it lands.
pub fn read_file_for_download(
    workspace_root: &Path,
    state_dir: Option<&Path>,
    rel_path: &str,
) -> Result<WorkspaceDownload, WorkspaceFileError> {
    let root = canonical_dir(workspace_root)?;
    let state = canonical_optional(state_dir);
    let rel = normalize_rel_path(rel_path)?;
    if rel.as_os_str().is_empty() || has_hidden_component(&rel) {
        return Err(WorkspaceFileError::Forbidden);
    }

    let candidate = root.join(&rel);
    let canonical = candidate.canonicalize().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            WorkspaceFileError::NotFound
        } else {
            WorkspaceFileError::Io(e.to_string())
        }
    })?;
    if !canonical.starts_with(&root) || is_private_state_path(&canonical, &root, state.as_deref()) {
        return Err(WorkspaceFileError::Forbidden);
    }

    let metadata = std::fs::metadata(&canonical).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            WorkspaceFileError::NotFound
        } else {
            WorkspaceFileError::Io(e.to_string())
        }
    })?;
    if !metadata.is_file() {
        return Err(WorkspaceFileError::NotFound);
    }

    let rel_path = canonical
        .strip_prefix(&root)
        .map_err(|_| WorkspaceFileError::Forbidden)?;
    let rel_path = rel_path_to_string(rel_path);
    Ok(WorkspaceDownload {
        path: canonical,
        rel_path,
        size_bytes: metadata.len(),
        mime_type: mime_type_for_path(&rel).to_string(),
    })
}

fn canonical_dir(path: &Path) -> Result<PathBuf, WorkspaceFileError> {
    let path = path
        .canonicalize()
        .map_err(|e| WorkspaceFileError::Io(e.to_string()))?;
    if path.is_dir() {
        Ok(path)
    } else {
        Err(WorkspaceFileError::NoWorkspace)
    }
}

fn canonical_optional(path: Option<&Path>) -> Option<PathBuf> {
    path.and_then(|p| p.canonicalize().ok())
}

fn normalize_rel_path(raw: &str) -> Result<PathBuf, WorkspaceFileError> {
    let mut out = PathBuf::new();
    let path = Path::new(raw.trim_matches('/'));
    for component in path.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(WorkspaceFileError::Forbidden);
            }
        }
    }
    Ok(out)
}

fn has_hidden_component(path: &Path) -> bool {
    path.components().any(|component| match component {
        Component::Normal(part) => part.to_str().is_some_and(|s| s.starts_with('.')),
        _ => false,
    })
}

fn is_private_state_path(path: &Path, root: &Path, state_dir: Option<&Path>) -> bool {
    if let Some(state_dir) = state_dir {
        if state_dir != root && path.starts_with(state_dir) {
            return true;
        }
    }

    let Ok(rel) = path.strip_prefix(root) else {
        return true;
    };
    let mut components = rel.components();
    let first = components.next().and_then(|c| match c {
        Component::Normal(part) => part.to_str(),
        _ => None,
    });
    matches!(first, Some("sessions" | "memory" | "logs" | "AGENT.json"))
}

fn matches_ext_filter(path: &Path, ext_filter: Option<&[String]>) -> bool {
    let Some(filter) = ext_filter else {
        return true;
    };
    if filter.is_empty() {
        return true;
    }
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    filter
        .iter()
        .any(|wanted| wanted.trim_start_matches('.').eq_ignore_ascii_case(ext))
}

fn rel_path_to_string(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn mime_type_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "md" | "markdown" => "text/markdown; charset=utf-8",
        "txt" | "log" => "text/plain; charset=utf-8",
        "json" => "application/json",
        "csv" => "text/csv; charset=utf-8",
        "html" | "htm" => "text/html; charset=utf-8",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_filters_hidden_state_and_ext() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("report.md"), "# Report").unwrap();
        std::fs::write(root.join("notes.txt"), "notes").unwrap();
        std::fs::create_dir(root.join(".hidden")).unwrap();
        std::fs::write(root.join(".hidden/secret.md"), "secret").unwrap();
        std::fs::create_dir(root.join("sessions")).unwrap();
        std::fs::write(root.join("sessions/session.md"), "secret").unwrap();

        let files = list_files(root, Some(root), None, Some(&["md".to_string()])).unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].rel_path, "report.md");
    }

    #[test]
    fn rejects_path_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("report.md"), "# Report").unwrap();

        let err = read_file_for_download(tmp.path(), None, "../report.md").unwrap_err();

        assert_eq!(err, WorkspaceFileError::Forbidden);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("secret.md");
        std::fs::write(&outside_file, "secret").unwrap();
        symlink(&outside_file, tmp.path().join("link.md")).unwrap();

        let err = read_file_for_download(tmp.path(), None, "link.md").unwrap_err();

        assert_eq!(err, WorkspaceFileError::Forbidden);
    }
}
