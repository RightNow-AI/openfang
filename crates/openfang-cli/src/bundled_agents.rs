//! Filesystem-backed agent template seeding.
//!
//! Agent templates are no longer compiled into the binary.
//! Instead, templates live under `~/.openfang/agents/` and can be customized
//! freely by users.

use std::path::{Path, PathBuf};

fn discover_seed_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(env_dir) = std::env::var("OPENFANG_AGENT_SEED_DIR") {
        let p = PathBuf::from(env_dir);
        if p.is_dir() {
            dirs.push(p);
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.as_path();
        for _ in 0..6 {
            if let Some(parent) = dir.parent() {
                let agents = parent.join("agents");
                if agents.is_dir() {
                    dirs.push(agents);
                    break;
                }
                dir = parent;
            }
        }
    }

    dirs
}

fn copy_seed_tree(src_root: &Path, dest_agents_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(src_root) else {
        return;
    };

    for entry in entries.flatten() {
        let src_dir = entry.path();
        if !src_dir.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let src_manifest = src_dir.join("agent.toml");
        if !src_manifest.exists() {
            continue;
        }

        let dest_dir = dest_agents_dir.join(&name);
        let dest_manifest = dest_dir.join("agent.toml");
        if dest_manifest.exists() {
            continue;
        }

        if std::fs::create_dir_all(&dest_dir).is_ok() {
            let _ = std::fs::copy(&src_manifest, &dest_manifest);
        }
    }
}

/// Seed `~/.openfang/agents/` from discovered filesystem seed directories.
///
/// This only copies missing templates and never overwrites user content.
pub fn install_seed_agents(agents_dir: &Path) {
    for seed_dir in discover_seed_dirs() {
        copy_seed_tree(&seed_dir, agents_dir);
    }
}
