//! Workspace context auto-detection.
//!
//! Scans the workspace root for project type indicators (Cargo.toml, package.json, etc.),
//! context files (AGENTS.md, SOUL.md, TOOLS.md, IDENTITY.md, HEARTBEAT.md), and OpenFang
//! state files. Provides mtime-cached file reads to avoid redundant I/O.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::debug;

/// Maximum file size to read for context files (32KB).
const MAX_FILE_SIZE: u64 = 32_768;

/// Known context file names scanned in the workspace root.
const CONTEXT_FILES: &[&str] = &[
    "AGENTS.md",
    "SOUL.md",
    "TOOLS.md",
    "IDENTITY.md",
    "HEARTBEAT.md",
];

/// Detected project type based on marker files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectType {
    Rust,
    Node,
    Python,
    Go,
    Java,
    DotNet,
    Unknown,
}

impl ProjectType {
    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Node => "Node.js",
            Self::Python => "Python",
            Self::Go => "Go",
            Self::Java => "Java",
            Self::DotNet => ".NET",
            Self::Unknown => "Unknown",
        }
    }
}

/// Cached file content with modification time.
#[derive(Debug, Clone)]
struct CachedFile {
    content: String,
    mtime: SystemTime,
}

/// Workspace context information gathered from the project root.
#[derive(Debug)]
pub struct WorkspaceContext {
    /// The workspace root path.
    pub workspace_root: PathBuf,
    /// Detected project type.
    pub project_type: ProjectType,
    /// Whether this is a git repository.
    pub is_git_repo: bool,
    /// Whether .openfang/ directory exists.
    pub has_openfang_dir: bool,
    /// Cached context files.
    cache: HashMap<String, CachedFile>,
}

impl WorkspaceContext {
    /// Detect workspace context from the given root directory.
    pub fn detect(root: &Path) -> Self {
        let project_type = detect_project_type(root);
        let is_git_repo = root.join(".git").exists();
        let has_openfang_dir = root.join(".openfang").exists();

        let mut cache = HashMap::new();
        for &name in CONTEXT_FILES {
            let file_path = root.join(name);
            if let Some(cached) = read_cached_file(&file_path) {
                debug!(file = name, "Loaded workspace context file");
                cache.insert(name.to_string(), cached);
            }
        }

        Self {
            workspace_root: root.to_path_buf(),
            project_type,
            is_git_repo,
            has_openfang_dir,
            cache,
        }
    }

    /// Get the content of a cached context file, refreshing if mtime changed.
    pub fn get_file(&mut self, name: &str) -> Option<&str> {
        let file_path = self.workspace_root.join(name);

        // Check if we have a cached version
        if let Some(cached) = self.cache.get(name) {
            // Verify mtime hasn't changed
            if let Ok(meta) = std::fs::metadata(&file_path) {
                if let Ok(mtime) = meta.modified() {
                    if mtime == cached.mtime {
                        return self.cache.get(name).map(|c| c.content.as_str());
                    }
                }
            }
        }

        // Cache miss or mtime changed — re-read
        if let Some(new_cached) = read_cached_file(&file_path) {
            self.cache.insert(name.to_string(), new_cached);
            return self.cache.get(name).map(|c| c.content.as_str());
        }

        // File doesn't exist or is too large
        self.cache.remove(name);
        None
    }

    /// Build a prompt context section summarizing the workspace.
    pub fn build_context_section(&mut self) -> String {
        let mut parts = Vec::new();

        parts.push(format!(
            "## Workspace Context\n- Project: {} ({})",
            self.workspace_root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "workspace".to_string()),
            self.project_type.label(),
        ));

        if self.is_git_repo {
            parts.push("- Git repository: yes".to_string());
        }

        // Include context file summaries
        let file_names: Vec<String> = self.cache.keys().cloned().collect();
        for name in file_names {
            if let Some(content) = self.get_file(&name) {
                // Take first 200 chars as preview
                let preview = if content.len() > 200 {
                    format!("{}...", crate::str_utils::safe_truncate_str(content, 200))
                } else {
                    content.to_string()
                };
                parts.push(format!("### {}\n{}", name, preview));
            }
        }

        parts.join("\n")
    }
}

/// Read a file into the cache if it exists and is under the size limit.
fn read_cached_file(path: &Path) -> Option<CachedFile> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > MAX_FILE_SIZE {
        debug!(
            path = %path.display(),
            size = meta.len(),
            "Skipping oversized context file"
        );
        return None;
    }
    let mtime = meta.modified().ok()?;
    let content = std::fs::read_to_string(path).ok()?;
    Some(CachedFile { content, mtime })
}

/// Detect project type from marker files in the root.
fn detect_project_type(root: &Path) -> ProjectType {
    if root.join("Cargo.toml").exists() {
        ProjectType::Rust
    } else if root.join("package.json").exists() {
        ProjectType::Node
    } else if root.join("pyproject.toml").exists()
        || root.join("setup.py").exists()
        || root.join("requirements.txt").exists()
    {
        ProjectType::Python
    } else if root.join("go.mod").exists() {
        ProjectType::Go
    } else if root.join("pom.xml").exists() || root.join("build.gradle").exists() {
        ProjectType::Java
    } else if root.join("*.csproj").exists() || root.join("*.sln").exists() {
        // Glob patterns don't work with exists(), so check differently
        if has_extension_in_dir(root, "csproj") || has_extension_in_dir(root, "sln") {
            ProjectType::DotNet
        } else {
            ProjectType::Unknown
        }
    } else {
        ProjectType::Unknown
    }
}

/// Check if any file with the given extension exists in a directory.
fn has_extension_in_dir(dir: &Path, ext: &str) -> bool {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Some(e) = entry.path().extension() {
                if e == ext {
                    return true;
                }
            }
        }
    }
    false
}

/// Persistent workspace state, saved to `.openfang/workspace-state.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceState {
    /// State format version.
    #[serde(default = "default_version")]
    pub version: u32,
    /// Timestamp when bootstrap was first seeded.
    pub bootstrap_seeded_at: Option<String>,
    /// Timestamp when onboarding was completed.
    pub onboarding_completed_at: Option<String>,
}

fn default_version() -> u32 {
    1
}

impl WorkspaceState {
    /// Load state from the workspace's `.openfang/workspace-state.json`.
    pub fn load(workspace_root: &Path) -> Self {
        let path = workspace_root
            .join(".openfang")
            .join("workspace-state.json");
        match std::fs::read_to_string(&path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save state to the workspace's `.openfang/workspace-state.json`.
    pub fn save(&self, workspace_root: &Path) -> Result<(), String> {
        let dir = workspace_root.join(".openfang");
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create .openfang dir: {e}"))?;
        let path = dir.join("workspace-state.json");
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize state: {e}"))?;
        std::fs::write(&path, json).map_err(|e| format!("Failed to write state: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_workspace_dir() -> TempDir {
        tempfile::Builder::new()
            .prefix("openfang_ws_")
            .tempdir()
            .unwrap()
    }

    #[test]
    fn test_detect_rust_project() {
        let dir = temp_workspace_dir();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Rust);
    }

    #[test]
    fn test_detect_node_project() {
        let dir = temp_workspace_dir();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Node);
    }

    #[test]
    fn test_detect_python_project() {
        let dir = temp_workspace_dir();
        std::fs::write(dir.path().join("pyproject.toml"), "[tool.poetry]").unwrap();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Python);
    }

    #[test]
    fn test_detect_go_project() {
        let dir = temp_workspace_dir();
        std::fs::write(dir.path().join("go.mod"), "module example.com/test").unwrap();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Go);
    }

    #[test]
    fn test_detect_unknown_project() {
        let dir = temp_workspace_dir();
        assert_eq!(detect_project_type(dir.path()), ProjectType::Unknown);
    }

    #[test]
    fn test_workspace_context_detect() {
        let dir = temp_workspace_dir();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(
            dir.path().join("AGENTS.md"),
            "# Agent Guidelines\nBe helpful.",
        )
        .unwrap();

        let ctx = WorkspaceContext::detect(dir.path());
        assert_eq!(ctx.project_type, ProjectType::Rust);
        assert!(ctx.is_git_repo);
        assert!(ctx.cache.contains_key("AGENTS.md"));
    }

    #[test]
    fn test_get_file_cache_hit() {
        let dir = temp_workspace_dir();
        std::fs::write(dir.path().join("SOUL.md"), "I am a helpful agent.").unwrap();

        let mut ctx = WorkspaceContext::detect(dir.path());
        let content1 = ctx.get_file("SOUL.md").map(|s| s.to_string());
        let content2 = ctx.get_file("SOUL.md").map(|s| s.to_string());
        assert_eq!(content1, content2);
        assert!(content1.unwrap().contains("helpful agent"));
    }

    #[test]
    fn test_file_size_cap() {
        let dir = temp_workspace_dir();

        // Write a file larger than 32KB
        let big = "x".repeat(40_000);
        std::fs::write(dir.path().join("AGENTS.md"), &big).unwrap();

        let ctx = WorkspaceContext::detect(dir.path());
        assert!(!ctx.cache.contains_key("AGENTS.md"));
    }

    #[test]
    fn test_build_context_section() {
        let dir = temp_workspace_dir();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join("SOUL.md"), "Be nice").unwrap();

        let mut ctx = WorkspaceContext::detect(dir.path());
        let section = ctx.build_context_section();
        assert!(section.contains("Rust"));
        assert!(section.contains("Git repository: yes"));
        assert!(section.contains("SOUL.md"));
        assert!(section.contains("Be nice"));
    }

    #[test]
    fn test_workspace_state_round_trip() {
        let dir = temp_workspace_dir();

        let state = WorkspaceState {
            version: 1,
            bootstrap_seeded_at: Some("2026-01-01T00:00:00Z".to_string()),
            onboarding_completed_at: None,
        };
        state.save(dir.path()).unwrap();

        let loaded = WorkspaceState::load(dir.path());
        assert_eq!(loaded.version, 1);
        assert_eq!(
            loaded.bootstrap_seeded_at.as_deref(),
            Some("2026-01-01T00:00:00Z")
        );
        assert!(loaded.onboarding_completed_at.is_none());
    }

    #[test]
    fn test_workspace_state_missing_file() {
        let dir = temp_workspace_dir();

        let state = WorkspaceState::load(dir.path());
        assert_eq!(state.version, 0); // default
        assert!(state.bootstrap_seeded_at.is_none());
    }
}
