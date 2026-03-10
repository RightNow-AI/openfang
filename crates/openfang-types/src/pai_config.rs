use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// PAI self-evolution configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PaiConfig {
    /// Enable the PAI self-evolution engine.
    pub enabled: bool,
    /// Path to the LearningStore database.
    pub store_path: Option<PathBuf>,
}

impl Default for PaiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            store_path: None,
        }
    }
}
