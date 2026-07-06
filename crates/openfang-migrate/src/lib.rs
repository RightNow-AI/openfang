//! Migration engine for importing from other agent frameworks into OMTAE.
//!
//! Supports importing agents, memory, sessions, skills, and channel configs
//! from OpenClaw and other frameworks.

pub mod openclaw;
pub mod report;

use std::path::PathBuf;

/// Source framework to migrate from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrateSource {
    /// OpenClaw agent framework.
    OpenClaw,
    /// LangChain (future).
    LangChain,
    /// AutoGPT (future).
    AutoGpt,
}

impl std::fmt::Display for MigrateSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpenClaw => write!(f, "OpenClaw"),
            Self::LangChain => write!(f, "LangChain"),
            Self::AutoGpt => write!(f, "AutoGPT"),
        }
    }
}

/// Options for running a migration.
#[derive(Debug, Clone)]
pub struct MigrateOptions {
    /// Source framework.
    pub source: MigrateSource,
    /// Path to the source workspace directory.
    pub source_dir: PathBuf,
    /// Path to the OMTAE home directory.
    pub target_dir: PathBuf,
    /// If true, only report what would be done without making changes.
    pub dry_run: bool,
}

/// Run a migration with the given options.
pub fn run_migration(options: &MigrateOptions) -> Result<report::MigrationReport, MigrateError> {
    match options.source {
        MigrateSource::OpenClaw => openclaw::migrate(options),
        MigrateSource::LangChain => Err(MigrateError::UnsupportedSource(
            "LangChain migration is not yet supported. Coming soon!".to_string(),
        )),
        MigrateSource::AutoGpt => Err(MigrateError::UnsupportedSource(
            "AutoGPT migration is not yet supported. Coming soon!".to_string(),
        )),
    }
}

/// Errors that can occur during migration.
#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    #[error("Source directory not found: {0}")]
    SourceNotFound(PathBuf),
    #[error("Failed to parse config: {0}")]
    ConfigParse(String),
    #[error("Failed to parse agent: {0}")]
    AgentParse(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("JSON5 parse error: {0}")]
    Json5Parse(String),
    #[error("TOML serialization error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),
    #[error("Unsupported source: {0}")]
    UnsupportedSource(String),
}
