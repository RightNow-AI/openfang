//! Configuration loading from `~/.openfang/config.toml` with defaults.
//!
//! Supports config includes: the `include` field specifies additional TOML files
//! to load and deep-merge before the root config (root overrides includes).

use openfang_types::agent::AgentManifest;
use openfang_types::config::KernelConfig;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::info;

/// Maximum include nesting depth.
const MAX_INCLUDE_DEPTH: u32 = 10;

/// Loaded agent configuration from a template directory.
#[derive(Debug, Clone)]
pub struct LoadedAgentConfig {
    pub path: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest_toml: String,
    pub manifest: AgentManifest,
}

/// Load kernel configuration from a TOML file, with defaults.
///
/// If the config contains an `include` field, included files are loaded
/// and deep-merged first, then the root config overrides them.
pub fn load_config(path: Option<&Path>) -> KernelConfig {
    let config_path = path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(default_config_path);

    if config_path.exists() {
        match std::fs::read_to_string(&config_path) {
            Ok(contents) => match toml::from_str::<toml::Value>(&contents) {
                Ok(mut root_value) => {
                    // Process includes before deserializing
                    let config_dir = config_path
                        .parent()
                        .unwrap_or_else(|| Path::new("."))
                        .to_path_buf();
                    let mut visited = HashSet::new();
                    if let Ok(canonical) = std::fs::canonicalize(&config_path) {
                        visited.insert(canonical);
                    } else {
                        visited.insert(config_path.clone());
                    }

                    if let Err(e) =
                        resolve_config_includes(&mut root_value, &config_dir, &mut visited, 0)
                    {
                        tracing::warn!(
                            error = %e,
                            "Config include resolution failed, using root config only"
                        );
                    }

                    // Remove the `include` field before deserializing to avoid confusion
                    if let toml::Value::Table(ref mut tbl) = root_value {
                        tbl.remove("include");
                    }

                    // Migrate misplaced api_key/api_listen from [api] section to root level.
                    // The old config schema incorrectly grouped these under [api], so many
                    // users have them in the wrong place. Move them up if not already at root.
                    if let toml::Value::Table(ref mut tbl) = root_value {
                        if let Some(toml::Value::Table(api_section)) = tbl.get("api").cloned() {
                            for key in &["api_key", "api_listen", "log_level"] {
                                if !tbl.contains_key(*key) {
                                    if let Some(val) = api_section.get(*key) {
                                        tracing::info!(
                                            key,
                                            "Migrating misplaced config field from [api] to root level"
                                        );
                                        tbl.insert(key.to_string(), val.clone());
                                    }
                                }
                            }
                        }
                    }

                    match root_value.try_into::<KernelConfig>() {
                        Ok(config) => {
                            info!(path = %config_path.display(), "Loaded configuration");
                            return config;
                        }
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                path = %config_path.display(),
                                "Failed to deserialize merged config, using defaults"
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        path = %config_path.display(),
                        "Failed to parse config, using defaults"
                    );
                }
            },
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    path = %config_path.display(),
                    "Failed to read config file, using defaults"
                );
            }
        }
    } else {
        info!(
            path = %config_path.display(),
            "Config file not found, using defaults"
        );
    }

    KernelConfig::default()
}

/// Resolve config includes by deep-merging included files into the root value.
///
/// Included files are loaded first and the root config overrides them.
/// Security: rejects absolute paths, `..` components, and circular references.
fn resolve_config_includes(
    root_value: &mut toml::Value,
    config_dir: &Path,
    visited: &mut HashSet<PathBuf>,
    depth: u32,
) -> Result<(), String> {
    if depth > MAX_INCLUDE_DEPTH {
        return Err(format!(
            "Config include depth exceeded maximum of {MAX_INCLUDE_DEPTH}"
        ));
    }

    // Extract include list from the current value
    let includes = match root_value {
        toml::Value::Table(tbl) => {
            if let Some(toml::Value::Array(arr)) = tbl.get("include") {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<_>>()
            } else {
                return Ok(());
            }
        }
        _ => return Ok(()),
    };

    if includes.is_empty() {
        return Ok(());
    }

    // Merge each include (earlier includes are overridden by later ones,
    // and the root config overrides everything).
    let mut merged_base = toml::Value::Table(toml::map::Map::new());

    for include_path_str in &includes {
        // SECURITY: reject absolute paths
        let include_path = Path::new(include_path_str);
        if include_path.is_absolute() {
            return Err(format!(
                "Config include rejects absolute path: {include_path_str}"
            ));
        }
        // SECURITY: reject `..` components
        for component in include_path.components() {
            if let std::path::Component::ParentDir = component {
                return Err(format!(
                    "Config include rejects path traversal: {include_path_str}"
                ));
            }
        }

        let resolved = config_dir.join(include_path);
        // SECURITY: verify resolved path stays within config dir
        let canonical = std::fs::canonicalize(&resolved).map_err(|e| {
            format!(
                "Config include '{}' cannot be resolved: {e}",
                include_path_str
            )
        })?;
        let canonical_dir = std::fs::canonicalize(config_dir)
            .map_err(|e| format!("Config dir cannot be canonicalized: {e}"))?;
        if !canonical.starts_with(&canonical_dir) {
            return Err(format!(
                "Config include '{}' escapes config directory",
                include_path_str
            ));
        }

        // SECURITY: circular detection
        if !visited.insert(canonical.clone()) {
            return Err(format!(
                "Circular config include detected: {include_path_str}"
            ));
        }

        info!(include = %include_path_str, "Loading config include");

        let contents = std::fs::read_to_string(&canonical)
            .map_err(|e| format!("Failed to read config include '{}': {e}", include_path_str))?;
        let mut include_value: toml::Value = toml::from_str(&contents)
            .map_err(|e| format!("Failed to parse config include '{}': {e}", include_path_str))?;

        // Recursively resolve includes in the included file
        let include_dir = canonical.parent().unwrap_or(config_dir).to_path_buf();
        resolve_config_includes(&mut include_value, &include_dir, visited, depth + 1)?;

        // Remove include field from the included file
        if let toml::Value::Table(ref mut tbl) = include_value {
            tbl.remove("include");
        }

        // Deep merge: include overrides the base built so far
        deep_merge_toml(&mut merged_base, &include_value);
    }

    // Now deep merge: root overrides the merged includes
    // Save root's current values (minus include), then merge root on top
    let root_without_include = {
        let mut v = root_value.clone();
        if let toml::Value::Table(ref mut tbl) = v {
            tbl.remove("include");
        }
        v
    };
    deep_merge_toml(&mut merged_base, &root_without_include);
    *root_value = merged_base;

    Ok(())
}

/// Deep-merge two TOML values. `overlay` values override `base` values.
/// For tables, recursively merge. For everything else, overlay wins.
pub fn deep_merge_toml(base: &mut toml::Value, overlay: &toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base_tbl), toml::Value::Table(overlay_tbl)) => {
            for (key, overlay_val) in overlay_tbl {
                if let Some(base_val) = base_tbl.get_mut(key) {
                    deep_merge_toml(base_val, overlay_val);
                } else {
                    base_tbl.insert(key.clone(), overlay_val.clone());
                }
            }
        }
        (base, overlay) => {
            *base = overlay.clone();
        }
    }
}

/// Get the default config file path.
///
/// Respects `OPENFANG_HOME` env var (e.g. `OPENFANG_HOME=/opt/openfang`).
pub fn default_config_path() -> PathBuf {
    openfang_home().join("config.toml")
}

/// Get the OpenFang home directory.
///
/// Priority: `OPENFANG_HOME` env var > `~/.openfang`.
pub fn openfang_home() -> PathBuf {
    if let Ok(home) = std::env::var("OPENFANG_HOME") {
        return PathBuf::from(home);
    }
    dirs::home_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(".openfang")
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !path.exists() {
        return;
    }
    if paths.iter().any(|existing| existing == &path) {
        return;
    }
    paths.push(path);
}

fn executable_agents_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut dir = exe.as_path();

    for _ in 0..5 {
        let parent = dir.parent()?;
        let agents = parent.join("agents");
        if agents.is_dir() {
            return Some(agents);
        }
        dir = parent;
    }

    None
}

fn validate_agent_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Agent name cannot be empty".to_string());
    }

    let path = Path::new(name);
    let mut components = path.components();
    match (components.next(), components.next()) {
        (Some(std::path::Component::Normal(_)), None) => Ok(()),
        _ => Err(format!("Invalid agent name '{name}'")),
    }
}

fn manifest_path(path: &Path) -> PathBuf {
    path.join("agent.toml")
}

/// Return agent template search paths in precedence order.
///
/// Priority: `OPENFANG_AGENTS_DIR` > `OPENFANG_HOME/agents` > `./agents`
/// > agents discovered relative to the current executable > repo `agents/`.
pub fn agent_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Ok(env_dir) = std::env::var("OPENFANG_AGENTS_DIR") {
        push_unique_path(&mut paths, PathBuf::from(env_dir));
    }

    push_unique_path(&mut paths, openfang_home().join("agents"));

    if let Ok(current_dir) = std::env::current_dir() {
        push_unique_path(&mut paths, current_dir.join("agents"));
    }

    if let Some(agents) = executable_agents_dir() {
        push_unique_path(&mut paths, agents);
    }

    let repo_agents = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../agents");
    push_unique_path(&mut paths, repo_agents);

    paths
}

/// Resolve an agent template name to an absolute template directory path.
pub fn resolve_agent_path(name: &str) -> Result<PathBuf, String> {
    validate_agent_name(name)?;

    for base_dir in agent_search_paths() {
        let candidate = base_dir.join(name);
        if let Ok(path) = validate_agent_path(&candidate) {
            return Ok(path);
        }
    }

    Err(format!("Agent template '{name}' not found"))
}

/// Validate that a path points to an agent template directory with `agent.toml`.
pub fn validate_agent_path(path: &Path) -> Result<PathBuf, String> {
    if !path.exists() {
        return Err(format!("Agent path does not exist: {}", path.display()));
    }
    if !path.is_dir() {
        return Err(format!("Agent path is not a directory: {}", path.display()));
    }

    let manifest_path = manifest_path(path);
    if !manifest_path.is_file() {
        return Err(format!("Agent manifest not found: {}", manifest_path.display()));
    }

    std::fs::canonicalize(path).map_err(|e| {
        format!(
            "Failed to canonicalize agent path '{}': {e}",
            path.display()
        )
    })
}

/// Load and parse an agent template manifest from a validated template directory.
pub fn load_agent_config(path: &Path) -> Result<LoadedAgentConfig, String> {
    let path = validate_agent_path(path)?;
    let manifest_path = manifest_path(&path);
    let manifest_toml = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("Failed to read manifest '{}': {e}", manifest_path.display()))?;
    let manifest = toml::from_str::<AgentManifest>(&manifest_toml)
        .map_err(|e| format!("Invalid manifest TOML '{}': {e}", manifest_path.display()))?;

    Ok(LoadedAgentConfig {
        path,
        manifest_path,
        manifest_toml,
        manifest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_load_config_defaults() {
        let config = load_config(None);
        assert_eq!(config.log_level, "info");
    }

    #[test]
    fn test_load_config_missing_file() {
        let config = load_config(Some(Path::new("/nonexistent/config.toml")));
        assert_eq!(config.log_level, "info");
    }

    #[test]
    fn test_deep_merge_simple() {
        let mut base: toml::Value = toml::from_str(
            r#"
            log_level = "debug"
            api_listen = "0.0.0.0:50051"
        "#,
        )
        .unwrap();
        let overlay: toml::Value = toml::from_str(
            r#"
            log_level = "info"
            network_enabled = true
        "#,
        )
        .unwrap();
        deep_merge_toml(&mut base, &overlay);
        assert_eq!(base["log_level"].as_str(), Some("info"));
        assert_eq!(base["api_listen"].as_str(), Some("0.0.0.0:50051"));
        assert_eq!(base["network_enabled"].as_bool(), Some(true));
    }

    #[test]
    fn test_deep_merge_nested_tables() {
        let mut base: toml::Value = toml::from_str(
            r#"
            [memory]
            decay_rate = 0.1
            consolidation_threshold = 10000
        "#,
        )
        .unwrap();
        let overlay: toml::Value = toml::from_str(
            r#"
            [memory]
            decay_rate = 0.5
        "#,
        )
        .unwrap();
        deep_merge_toml(&mut base, &overlay);
        let mem = base["memory"].as_table().unwrap();
        assert_eq!(mem["decay_rate"].as_float(), Some(0.5));
        assert_eq!(mem["consolidation_threshold"].as_integer(), Some(10000));
    }

    #[test]
    fn test_validate_agent_path_requires_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let agent_dir = dir.path().join("assistant");
        std::fs::create_dir_all(&agent_dir).unwrap();

        let error = validate_agent_path(&agent_dir).unwrap_err();
        assert!(error.contains("Agent manifest not found"));
    }

    #[test]
    fn test_load_agent_config_reads_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let agent_dir = dir.path().join("assistant");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(
            agent_dir.join("agent.toml"),
            r#"
name = "assistant"
description = "Test assistant"

[model]
provider = "ollama"
model = "qwen3.5:9b"
system_prompt = "Be exact"

[capabilities]
tools = []
"#,
        )
        .unwrap();

        let loaded = load_agent_config(&agent_dir).unwrap();
        assert_eq!(loaded.manifest.name, "assistant");
        assert_eq!(loaded.manifest.description, "Test assistant");
        assert!(loaded.manifest_path.ends_with("agent.toml"));
    }

    #[test]
    fn test_basic_include() {
        let dir = tempfile::tempdir().unwrap();
        let base_path = dir.path().join("base.toml");
        let root_path = dir.path().join("config.toml");

        // Base config
        let mut f = std::fs::File::create(&base_path).unwrap();
        writeln!(f, "log_level = \"debug\"").unwrap();
        writeln!(f, "api_listen = \"0.0.0.0:9999\"").unwrap();
        drop(f);

        // Root config (includes base, overrides log_level)
        let mut f = std::fs::File::create(&root_path).unwrap();
        writeln!(f, "include = [\"base.toml\"]").unwrap();
        writeln!(f, "log_level = \"warn\"").unwrap();
        drop(f);

        let config = load_config(Some(&root_path));
        assert_eq!(config.log_level, "warn"); // root overrides
        assert_eq!(config.api_listen, "0.0.0.0:9999"); // from base
    }

    #[test]
    fn test_nested_include() {
        let dir = tempfile::tempdir().unwrap();
        let grandchild = dir.path().join("grandchild.toml");
        let child = dir.path().join("child.toml");
        let root = dir.path().join("config.toml");

        let mut f = std::fs::File::create(&grandchild).unwrap();
        writeln!(f, "log_level = \"trace\"").unwrap();
        drop(f);

        let mut f = std::fs::File::create(&child).unwrap();
        writeln!(f, "include = [\"grandchild.toml\"]").unwrap();
        writeln!(f, "log_level = \"debug\"").unwrap();
        drop(f);

        let mut f = std::fs::File::create(&root).unwrap();
        writeln!(f, "include = [\"child.toml\"]").unwrap();
        writeln!(f, "log_level = \"info\"").unwrap();
        drop(f);

        let config = load_config(Some(&root));
        assert_eq!(config.log_level, "info"); // root wins
    }

    #[test]
    fn test_circular_include_detected() {
        let dir = tempfile::tempdir().unwrap();
        let a_path = dir.path().join("a.toml");
        let b_path = dir.path().join("b.toml");

        let mut f = std::fs::File::create(&a_path).unwrap();
        writeln!(f, "include = [\"b.toml\"]").unwrap();
        writeln!(f, "log_level = \"info\"").unwrap();
        drop(f);

        let mut f = std::fs::File::create(&b_path).unwrap();
        writeln!(f, "include = [\"a.toml\"]").unwrap();
        drop(f);

        // Should not panic — circular detection triggers, falls back gracefully
        let config = load_config(Some(&a_path));
        // Falls back to defaults due to the circular error
        assert!(!config.log_level.is_empty());
    }

    #[test]
    fn test_path_traversal_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("config.toml");

        let mut f = std::fs::File::create(&root).unwrap();
        writeln!(f, "include = [\"../etc/passwd\"]").unwrap();
        drop(f);

        // Should not panic — path traversal triggers error, falls back
        let config = load_config(Some(&root));
        assert_eq!(config.log_level, "info"); // defaults
    }

    #[test]
    fn test_max_depth_exceeded() {
        let dir = tempfile::tempdir().unwrap();

        // Create a chain of 12 files (exceeds MAX_INCLUDE_DEPTH=10)
        for i in (0..12).rev() {
            let name = format!("level{i}.toml");
            let path = dir.path().join(&name);
            let mut f = std::fs::File::create(&path).unwrap();
            if i < 11 {
                let next = format!("level{}.toml", i + 1);
                writeln!(f, "include = [\"{next}\"]").unwrap();
            }
            writeln!(f, "log_level = \"level{i}\"").unwrap();
            drop(f);
        }

        let root = dir.path().join("level0.toml");
        let config = load_config(Some(&root));
        // Falls back due to depth limit — but should not panic
        assert!(!config.log_level.is_empty());
    }

    #[test]
    fn test_absolute_path_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("config.toml");

        let mut f = std::fs::File::create(&root).unwrap();
        writeln!(f, "include = [\"/etc/shadow\"]").unwrap();
        drop(f);

        let config = load_config(Some(&root));
        assert_eq!(config.log_level, "info"); // defaults
    }

    #[test]
    fn test_no_includes_works() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("config.toml");

        let mut f = std::fs::File::create(&root).unwrap();
        writeln!(f, "log_level = \"trace\"").unwrap();
        drop(f);

        let config = load_config(Some(&root));
        assert_eq!(config.log_level, "trace");
    }
}
