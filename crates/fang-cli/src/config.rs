//! Persistent CLI configuration stored in ~/.config/fang/config.toml
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FangConfig {
    pub auth: Option<AuthConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub github_login: String,
    pub token: String,
    pub registry_url: String,
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("fang")
        .join("config.toml")
}

pub fn load_config() -> Result<FangConfig> {
    let path = config_path();
    if !path.exists() {
        return Ok(FangConfig::default());
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config at {}", path.display()))?;
    toml::from_str(&content).context("Failed to parse config file")
}

pub fn save_config(config: &FangConfig) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(config)?;
    std::fs::write(&path, content)
        .with_context(|| format!("Failed to write config to {}", path.display()))
}
