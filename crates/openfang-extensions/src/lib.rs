//! OpenFang Extensions — one-click integration system.
//!
//! This crate provides:
//! - **Integration Registry**: 25 bundled MCP server templates (GitHub, Slack, etc.)
//! - **Credential Vault**: AES-256-GCM encrypted storage with OS keyring support
//! - **OAuth2 PKCE**: Localhost callback flows for Google/GitHub/Microsoft/Slack
//! - **Health Monitor**: Auto-reconnect with exponential backoff
//! - **Installer**: One-click `openfang add <name>` flow

pub mod bundled;
pub mod credentials;
pub mod health;
pub mod installer;
pub mod oauth;
pub mod registry;
pub mod vault;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Error types ─────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum ExtensionError {
    #[error("Integration not found: {0}")]
    NotFound(String),
    #[error("Integration already installed: {0}")]
    AlreadyInstalled(String),
    #[error("Integration not installed: {0}")]
    NotInstalled(String),
    #[error("Credential not found: {0}")]
    CredentialNotFound(String),
    #[error("Vault error: {0}")]
    Vault(String),
    #[error("Vault locked — unlock with vault key or OPENFANG_VAULT_KEY env var")]
    VaultLocked,
    #[error("OAuth error: {0}")]
    OAuth(String),
    #[error("TOML parse error: {0}")]
    TomlParse(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("Health check failed: {0}")]
    HealthCheck(String),
}

pub type ExtensionResult<T> = Result<T, ExtensionError>;

// ─── Core types ──────────────────────────────────────────────────────────────

/// Category of an integration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IntegrationCategory {
    DevTools,
    Productivity,
    Communication,
    Data,
    Cloud,
    AI,
}

impl std::fmt::Display for IntegrationCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DevTools => write!(f, "Dev Tools"),
            Self::Productivity => write!(f, "Productivity"),
            Self::Communication => write!(f, "Communication"),
            Self::Data => write!(f, "Data"),
            Self::Cloud => write!(f, "Cloud"),
            Self::AI => write!(f, "AI & Search"),
        }
    }
}

/// MCP transport template — how to launch the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpTransportTemplate {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
    },
    Sse {
        url: String,
    },
    Http {
        url: String,
    },
}

/// An environment variable required by an integration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequiredEnvVar {
    /// Env var name (e.g., "GITHUB_PERSONAL_ACCESS_TOKEN").
    pub name: String,
    /// Human-readable label (e.g., "Personal Access Token").
    pub label: String,
    /// How to obtain this credential.
    pub help: String,
    /// Whether this is a secret (should be stored in vault).
    #[serde(default = "default_true")]
    pub is_secret: bool,
    /// URL where the user can create the key.
    #[serde(default)]
    pub get_url: Option<String>,
}

fn default_true() -> bool {
    true
}

/// OAuth provider configuration template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTemplate {
    /// OAuth provider (google, github, microsoft, slack).
    pub provider: String,
    /// OAuth scopes required.
    pub scopes: Vec<String>,
    /// Authorization URL.
    pub auth_url: String,
    /// Token exchange URL.
    pub token_url: String,
}

/// Health check configuration for an integration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HealthCheckConfig {
    /// How often to check health (seconds).
    pub interval_secs: u64,
    /// Consider unhealthy after this many consecutive failures.
    pub unhealthy_threshold: u32,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            interval_secs: 60,
            unhealthy_threshold: 3,
        }
    }
}

/// A bundled integration template — describes how to set up an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationTemplate {
    /// Unique identifier (e.g., "github").
    pub id: String,
    /// Human-readable name (e.g., "GitHub").
    pub name: String,
    /// Short description.
    pub description: String,
    /// Category for browsing.
    pub category: IntegrationCategory,
    /// Icon (emoji).
    #[serde(default)]
    pub icon: String,
    /// MCP transport configuration.
    pub transport: McpTransportTemplate,
    /// Required credentials.
    #[serde(default)]
    pub required_env: Vec<RequiredEnvVar>,
    /// OAuth configuration (None = API key only).
    #[serde(default)]
    pub oauth: Option<OAuthTemplate>,
    /// Searchable tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Setup instructions (displayed in TUI detail view).
    #[serde(default)]
    pub setup_instructions: String,
    /// Health check configuration.
    #[serde(default)]
    pub health_check: HealthCheckConfig,
}

/// Status of an installed integration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationStatus {
    /// Configured and MCP server running.
    Ready,
    /// Installed but credentials missing.
    Setup,
    /// Not installed.
    Available,
    /// MCP server errored.
    Error(String),
    /// Disabled by user.
    Disabled,
}

impl std::fmt::Display for IntegrationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ready => write!(f, "Ready"),
            Self::Setup => write!(f, "Setup"),
            Self::Available => write!(f, "Available"),
            Self::Error(msg) => write!(f, "Error: {msg}"),
            Self::Disabled => write!(f, "Disabled"),
        }
    }
}

/// An installed integration record (persisted in integrations.toml).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledIntegration {
    /// Template ID.
    pub id: String,
    /// When installed.
    pub installed_at: DateTime<Utc>,
    /// Whether enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// OAuth provider if using OAuth (e.g., "google").
    #[serde(default)]
    pub oauth_provider: Option<String>,
    /// Custom configuration overrides.
    #[serde(default)]
    pub config: HashMap<String, String>,
}

/// Top-level structure for `~/.openfang/integrations.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IntegrationsFile {
    #[serde(default)]
    pub installed: Vec<InstalledIntegration>,
}

/// Combined view of an integration (template + install state).
#[derive(Debug, Clone, Serialize)]
pub struct IntegrationInfo {
    pub template: IntegrationTemplate,
    pub status: IntegrationStatus,
    pub installed: Option<InstalledIntegration>,
    pub tool_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_display() {
        assert_eq!(IntegrationCategory::DevTools.to_string(), "Dev Tools");
        assert_eq!(
            IntegrationCategory::Productivity.to_string(),
            "Productivity"
        );
        assert_eq!(IntegrationCategory::AI.to_string(), "AI & Search");
    }

    #[test]
    fn status_display() {
        assert_eq!(IntegrationStatus::Ready.to_string(), "Ready");
        assert_eq!(IntegrationStatus::Setup.to_string(), "Setup");
        assert_eq!(
            IntegrationStatus::Error("timeout".to_string()).to_string(),
            "Error: timeout"
        );
    }

    #[test]
    fn integration_template_roundtrip() {
        let toml_str = r#"
id = "test"
name = "Test Integration"
description = "A test"
category = "devtools"
icon = "T"
tags = ["test"]
setup_instructions = "Just test it."

[transport]
type = "stdio"
command = "test-server"
args = ["--flag"]

[[required_env]]
name = "TEST_KEY"
label = "Test Key"
help = "Get it from test.com"
is_secret = true
get_url = "https://test.com/keys"

[health_check]
interval_secs = 30
unhealthy_threshold = 5
"#;
        let template: IntegrationTemplate = toml::from_str(toml_str).unwrap();
        assert_eq!(template.id, "test");
        assert_eq!(template.category, IntegrationCategory::DevTools);
        assert_eq!(template.required_env.len(), 1);
        assert!(template.required_env[0].is_secret);
        assert_eq!(template.health_check.interval_secs, 30);
    }

    #[test]
    fn installed_integration_roundtrip() {
        let toml_str = r#"
[[installed]]
id = "github"
installed_at = "2026-02-23T10:00:00Z"
enabled = true

[[installed]]
id = "google-calendar"
installed_at = "2026-02-23T10:05:00Z"
enabled = true
oauth_provider = "google"
"#;
        let file: IntegrationsFile = toml::from_str(toml_str).unwrap();
        assert_eq!(file.installed.len(), 2);
        assert_eq!(file.installed[0].id, "github");
        assert!(file.installed[0].enabled);
        assert_eq!(file.installed[1].oauth_provider.as_deref(), Some("google"));
    }

    #[test]
    fn error_display() {
        let err = ExtensionError::NotFound("github".to_string());
        assert!(err.to_string().contains("github"));
        let err = ExtensionError::VaultLocked;
        assert!(err.to_string().contains("vault"));
    }
}
