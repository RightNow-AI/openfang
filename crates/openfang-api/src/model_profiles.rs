//! Local vLLM model profiles (`~/.omtae/models.toml` + `active_model.txt`).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProfile {
    pub id: String,
    pub display_name: String,
    pub vllm_path: String,
    pub tensor_parallel: u32,
    pub max_model_len: u32,
    pub gpu_mem_util: f64,
    pub vllm_served_name: String,
    /// Model id in OMTAE (`config.toml` + `custom_models.json`).
    pub omtae_model_id: String,
    pub start_script: String,
    #[serde(default)]
    pub quantization: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModelsFile {
    #[serde(default)]
    profiles: Vec<ModelProfile>,
}

pub fn models_toml_path(home: &Path) -> PathBuf {
    home.join("models.toml")
}

pub fn active_profile_path(home: &Path) -> PathBuf {
    home.join("active_model.txt")
}

pub fn load_profiles(home: &Path) -> Result<Vec<ModelProfile>, String> {
    let path = models_toml_path(home);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("read {}: {e}", path.display()))?;
    let file: ModelsFile = toml::from_str(&content)
        .map_err(|e| format!("parse {}: {e}", path.display()))?;
    Ok(file.profiles)
}

pub fn read_active_profile_id(home: &Path) -> Option<String> {
    let path = active_profile_path(home);
    let id = std::fs::read_to_string(path).ok()?;
    let id = id.trim().to_string();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

pub fn write_active_profile_id(home: &Path, profile_id: &str) -> Result<(), String> {
    let path = active_profile_path(home);
    std::fs::write(&path, format!("{profile_id}\n"))
        .map_err(|e| format!("write {}: {e}", path.display()))
}

pub fn find_profile<'a>(profiles: &'a [ModelProfile], id: &str) -> Option<&'a ModelProfile> {
    profiles.iter().find(|p| p.id == id)
}

/// Update `[default_model].model` in config.toml (preserves other keys).
pub fn persist_default_model_id(home: &Path, model_id: &str) -> Result<(), String> {
    let config_path = home.join("config.toml");
    let mut table: toml::value::Table = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)
            .map_err(|e| format!("read config.toml: {e}"))?;
        toml::from_str(&content).unwrap_or_default()
    } else {
        toml::value::Table::new()
    };

    let section = table
        .entry("default_model".to_string())
        .or_insert_with(|| toml::Value::Table(toml::value::Table::new()));
    if let toml::Value::Table(ref mut t) = section {
        t.insert(
            "model".to_string(),
            toml::Value::String(model_id.to_string()),
        );
        if !t.contains_key("provider") {
            t.insert("provider".to_string(), toml::Value::String("vllm".to_string()));
        }
        if !t.contains_key("api_key_env") {
            t.insert(
                "api_key_env".to_string(),
                toml::Value::String("VLLM_API_KEY".to_string()),
            );
        }
        if !t.contains_key("base_url") {
            t.insert(
                "base_url".to_string(),
                toml::Value::String("http://127.0.0.1:8000/v1".to_string()),
            );
        }
    }

    let toml_string = toml::to_string_pretty(&table)
        .map_err(|e| format!("serialize config.toml: {e}"))?;
    std::fs::write(&config_path, toml_string)
        .map_err(|e| format!("write config.toml: {e}"))?;
    Ok(())
}

pub fn profile_available_on_disk(profile: &ModelProfile) -> bool {
    let p = Path::new(&profile.vllm_path);
    p.is_dir() && p.join("config.json").exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn load_profiles_parses_array() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("models.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[[profiles]]
id = "test"
display_name = "Test"
vllm_path = "/tmp/m"
tensor_parallel = 2
max_model_len = 8192
gpu_mem_util = 0.8
vllm_served_name = "test-model"
omtae_model_id = "test-model"
start_script = "/tmp/start.sh"
"#
        )
        .unwrap();
        let profiles = load_profiles(dir.path()).unwrap();
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].id, "test");
    }
}
