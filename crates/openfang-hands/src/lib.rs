//! OpenFang Hands — curated autonomous capability packages.
//!
//! A Hand is a pre-built, domain-complete agent configuration that users activate
//! from a marketplace. Unlike regular agents (you chat with them), Hands work for
//! you (you check in on them).

pub mod bundled;
pub mod registry;

use chrono::{DateTime, Utc};
use openfang_types::agent::AgentId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ─── Error types ─────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum HandError {
    #[error("Hand not found: {0}")]
    NotFound(String),
    #[error("Hand already active: {0}")]
    AlreadyActive(String),
    #[error("Hand instance not found: {0}")]
    InstanceNotFound(Uuid),
    #[error("Activation failed: {0}")]
    ActivationFailed(String),
    #[error("TOML parse error: {0}")]
    TomlParse(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Config error: {0}")]
    Config(String),
}

pub type HandResult<T> = Result<T, HandError>;

// ─── Core types ──────────────────────────────────────────────────────────────

/// Category of a Hand.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HandCategory {
    Content,
    Security,
    Productivity,
    Development,
    Communication,
    Data,
}

impl std::fmt::Display for HandCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Content => write!(f, "Content"),
            Self::Security => write!(f, "Security"),
            Self::Productivity => write!(f, "Productivity"),
            Self::Development => write!(f, "Development"),
            Self::Communication => write!(f, "Communication"),
            Self::Data => write!(f, "Data"),
        }
    }
}

/// Type of requirement check.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequirementType {
    /// A binary must exist on PATH.
    Binary,
    /// An environment variable must be set.
    EnvVar,
    /// An API key env var must be set.
    ApiKey,
}

/// Platform-specific install commands and guides for a requirement.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HandInstallInfo {
    #[serde(default)]
    pub macos: Option<String>,
    #[serde(default)]
    pub windows: Option<String>,
    #[serde(default)]
    pub linux_apt: Option<String>,
    #[serde(default)]
    pub linux_dnf: Option<String>,
    #[serde(default)]
    pub linux_pacman: Option<String>,
    #[serde(default)]
    pub pip: Option<String>,
    #[serde(default)]
    pub signup_url: Option<String>,
    #[serde(default)]
    pub docs_url: Option<String>,
    #[serde(default)]
    pub env_example: Option<String>,
    #[serde(default)]
    pub manual_url: Option<String>,
    #[serde(default)]
    pub estimated_time: Option<String>,
    #[serde(default)]
    pub steps: Vec<String>,
}

/// A single requirement the user must satisfy to use a Hand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandRequirement {
    /// Unique key for this requirement.
    pub key: String,
    /// Human-readable label.
    pub label: String,
    /// What kind of check to perform.
    pub requirement_type: RequirementType,
    /// The value to check (binary name, env var name, etc.).
    pub check_value: String,
    /// Human-readable description of why this is needed.
    #[serde(default)]
    pub description: Option<String>,
    /// Whether this requirement is optional (non-critical).
    ///
    /// Optional requirements do not block activation. When an active hand has
    /// unmet optional requirements it is reported as "degraded" rather than
    /// "requirements not met".
    #[serde(default)]
    pub optional: bool,
    /// Platform-specific installation instructions.
    #[serde(default)]
    pub install: Option<HandInstallInfo>,
}

/// A metric displayed on the Hand dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandMetric {
    /// Display label.
    pub label: String,
    /// Memory key to read from agent's structured memory.
    pub memory_key: String,
    /// Display format (e.g. "number", "duration", "bytes").
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_format() -> String {
    "number".to_string()
}

// ─── Hand settings types ────────────────────────────────────────────────────

/// Type of a hand setting control.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HandSettingType {
    Select,
    Text,
    Toggle,
}

/// A single option within a Select-type setting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandSettingOption {
    pub value: String,
    pub label: String,
    /// Env var to check for "Ready" badge (e.g. `GROQ_API_KEY`).
    #[serde(default)]
    pub provider_env: Option<String>,
    /// Binary to check on PATH for "Ready" badge (e.g. `whisper`).
    #[serde(default)]
    pub binary: Option<String>,
}

/// A configurable setting declared in HAND.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandSetting {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    pub setting_type: HandSettingType,
    #[serde(default)]
    pub default: String,
    #[serde(default)]
    pub options: Vec<HandSettingOption>,
    /// Env var name to expose when a text-type setting has a value
    /// (e.g. `ELEVENLABS_API_KEY` for an API key text field).
    #[serde(default)]
    pub env_var: Option<String>,
}

/// Result of resolving user-chosen settings against the schema.
pub struct ResolvedSettings {
    /// Markdown block to append to the system prompt (e.g. `## User Configuration\n- STT: Groq...`).
    pub prompt_block: String,
    /// Env var names the agent's subprocess should have access to.
    pub env_vars: Vec<String>,
}

/// Resolve user config values against a hand's settings schema.
///
/// For each setting, looks up the user's choice in `config` (falling back to
/// `setting.default`). For Select-type settings, finds the matching option and
/// collects its `provider_env` if present. Builds a prompt block summarising
/// the user's configuration.
pub fn resolve_settings(
    settings: &[HandSetting],
    config: &HashMap<String, serde_json::Value>,
) -> ResolvedSettings {
    let mut lines: Vec<String> = Vec::new();
    let mut env_vars: Vec<String> = Vec::new();

    for setting in settings {
        let chosen_value = config
            .get(&setting.key)
            .and_then(|v| v.as_str())
            .unwrap_or(&setting.default);

        match setting.setting_type {
            HandSettingType::Select => {
                let matched = setting.options.iter().find(|o| o.value == chosen_value);
                let display = matched.map(|o| o.label.as_str()).unwrap_or(chosen_value);
                lines.push(format!(
                    "- {}: {} ({})",
                    setting.label, display, chosen_value
                ));

                if let Some(opt) = matched {
                    if let Some(ref env) = opt.provider_env {
                        env_vars.push(env.clone());
                    }
                }
            }
            HandSettingType::Toggle => {
                let enabled = chosen_value == "true" || chosen_value == "1";
                lines.push(format!(
                    "- {}: {}",
                    setting.label,
                    if enabled { "Enabled" } else { "Disabled" }
                ));
            }
            HandSettingType::Text => {
                if !chosen_value.is_empty() {
                    lines.push(format!("- {}: {}", setting.label, chosen_value));
                    if let Some(ref env) = setting.env_var {
                        env_vars.push(env.clone());
                    }
                }
            }
        }
    }

    let prompt_block = if lines.is_empty() {
        String::new()
    } else {
        format!("## User Configuration\n\n{}", lines.join("\n"))
    };

    ResolvedSettings {
        prompt_block,
        env_vars,
    }
}

/// Dashboard schema for a Hand's metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HandDashboard {
    pub metrics: Vec<HandMetric>,
}

/// Agent configuration embedded in a Hand definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandAgentConfig {
    pub name: String,
    pub description: String,
    #[serde(default = "default_module")]
    pub module: String,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    pub system_prompt: String,
    #[serde(default)]
    pub max_iterations: Option<u32>,
}

fn default_module() -> String {
    "builtin:chat".to_string()
}
fn default_provider() -> String {
    "anthropic".to_string()
}
fn default_model() -> String {
    "claude-sonnet-4-20250514".to_string()
}
fn default_max_tokens() -> u32 {
    4096
}
fn default_temperature() -> f32 {
    0.7
}

#[derive(Deserialize)]
struct HandTomlWrapper {
    hand: HandDefinition,
}

/// Parse HAND.toml content, supporting both flat format and `[hand]` table format.
pub fn parse_hand_toml(content: &str) -> Result<HandDefinition, toml::de::Error> {
    if let Ok(def) = toml::from_str::<HandDefinition>(content) {
        return Ok(def);
    }
    let wrapper: HandTomlWrapper = toml::from_str(content)?;
    Ok(wrapper.hand)
}

/// Complete Hand definition — parsed from HAND.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandDefinition {
    /// Unique hand identifier (e.g. "clip").
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// What this Hand does.
    pub description: String,
    /// Category for marketplace browsing.
    pub category: HandCategory,
    /// Icon (emoji).
    #[serde(default)]
    pub icon: String,
    /// Tools the agent needs access to.
    #[serde(default)]
    pub tools: Vec<String>,
    /// Skill allowlist for the spawned agent (empty = all).
    #[serde(default)]
    pub skills: Vec<String>,
    /// MCP server allowlist for the spawned agent (empty = all).
    #[serde(default)]
    pub mcp_servers: Vec<String>,
    /// Requirements that must be satisfied before activation.
    #[serde(default)]
    pub requires: Vec<HandRequirement>,
    /// Configurable settings (shown in activation modal).
    #[serde(default)]
    pub settings: Vec<HandSetting>,
    /// Agent manifest template.
    pub agent: HandAgentConfig,
    /// Dashboard metrics schema.
    #[serde(default)]
    pub dashboard: HandDashboard,
    /// Bundled skill content (populated at load time, not in TOML).
    #[serde(skip)]
    pub skill_content: Option<String>,
}

/// Runtime status of a Hand instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HandStatus {
    Active,
    Paused,
    Error(String),
    Inactive,
}

impl std::fmt::Display for HandStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "Active"),
            Self::Paused => write!(f, "Paused"),
            Self::Error(msg) => write!(f, "Error: {msg}"),
            Self::Inactive => write!(f, "Inactive"),
        }
    }
}

/// A running Hand instance — links a HandDefinition to an actual agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandInstance {
    /// Unique instance identifier.
    pub instance_id: Uuid,
    /// Which hand definition this is an instance of.
    pub hand_id: String,
    /// Current status.
    pub status: HandStatus,
    /// The agent that was spawned for this hand.
    pub agent_id: Option<AgentId>,
    /// Agent name (for display).
    pub agent_name: String,
    /// User-provided configuration overrides.
    pub config: HashMap<String, serde_json::Value>,
    /// When activated.
    pub activated_at: DateTime<Utc>,
    /// Last status change.
    pub updated_at: DateTime<Utc>,
}

impl HandInstance {
    /// Create a new pending instance.
    pub fn new(
        hand_id: &str,
        agent_name: &str,
        config: HashMap<String, serde_json::Value>,
    ) -> Self {
        let now = Utc::now();
        Self {
            instance_id: Uuid::new_v4(),
            hand_id: hand_id.to_string(),
            status: HandStatus::Active,
            agent_id: None,
            agent_name: agent_name.to_string(),
            config,
            activated_at: now,
            updated_at: now,
        }
    }
}

/// Request to activate a hand.
#[derive(Debug, Deserialize)]
pub struct ActivateHandRequest {
    /// Optional configuration overrides.
    #[serde(default)]
    pub config: HashMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hand_category_display() {
        assert_eq!(HandCategory::Content.to_string(), "Content");
        assert_eq!(HandCategory::Security.to_string(), "Security");
        assert_eq!(HandCategory::Data.to_string(), "Data");
    }

    #[test]
    fn hand_status_display() {
        assert_eq!(HandStatus::Active.to_string(), "Active");
        assert_eq!(HandStatus::Paused.to_string(), "Paused");
        assert_eq!(
            HandStatus::Error("ffmpeg not found".to_string()).to_string(),
            "Error: ffmpeg not found"
        );
    }

    #[test]
    fn hand_instance_new() {
        let instance = HandInstance::new("clip", "clip-hand", HashMap::new());
        assert_eq!(instance.hand_id, "clip");
        assert_eq!(instance.agent_name, "clip-hand");
        assert_eq!(instance.status, HandStatus::Active);
        assert!(instance.agent_id.is_none());
    }

    #[test]
    fn hand_error_display() {
        let err = HandError::NotFound("clip".to_string());
        assert!(err.to_string().contains("clip"));

        let err = HandError::AlreadyActive("clip".to_string());
        assert!(err.to_string().contains("already"));
    }

    #[test]
    fn hand_definition_roundtrip() {
        let toml_str = r#"
id = "test"
name = "Test Hand"
description = "A test hand"
category = "content"
icon = "T"
tools = ["shell_exec"]

[[requires]]
key = "test_bin"
label = "test must be installed"
requirement_type = "binary"
check_value = "test"

[agent]
name = "test-hand"
description = "Test agent"
system_prompt = "You are a test agent."

[dashboard]
metrics = []
"#;
        let def: HandDefinition = toml::from_str(toml_str).unwrap();
        assert_eq!(def.id, "test");
        assert_eq!(def.category, HandCategory::Content);
        assert_eq!(def.requires.len(), 1);
        assert_eq!(def.agent.name, "test-hand");
    }

    #[test]
    fn hand_definition_with_settings() {
        let toml_str = r#"
id = "test"
name = "Test Hand"
description = "A test"
category = "content"
tools = []

[[settings]]
key = "stt_provider"
label = "STT Provider"
description = "Speech-to-text engine"
setting_type = "select"
default = "auto"

[[settings.options]]
value = "auto"
label = "Auto-detect"

[[settings.options]]
value = "groq"
label = "Groq Whisper"
provider_env = "GROQ_API_KEY"

[[settings.options]]
value = "local"
label = "Local Whisper"
binary = "whisper"

[agent]
name = "test-hand"
description = "Test"
system_prompt = "Test."

[dashboard]
metrics = []
"#;
        let def: HandDefinition = toml::from_str(toml_str).unwrap();
        assert_eq!(def.settings.len(), 1);
        assert_eq!(def.settings[0].key, "stt_provider");
        assert_eq!(def.settings[0].setting_type, HandSettingType::Select);
        assert_eq!(def.settings[0].options.len(), 3);
        assert_eq!(
            def.settings[0].options[1].provider_env.as_deref(),
            Some("GROQ_API_KEY")
        );
        assert_eq!(
            def.settings[0].options[2].binary.as_deref(),
            Some("whisper")
        );
    }

    #[test]
    fn resolve_settings_with_config() {
        let settings = vec![HandSetting {
            key: "stt".to_string(),
            label: "STT Provider".to_string(),
            description: String::new(),
            setting_type: HandSettingType::Select,
            default: "auto".to_string(),
            options: vec![
                HandSettingOption {
                    value: "auto".to_string(),
                    label: "Auto".to_string(),
                    provider_env: None,
                    binary: None,
                },
                HandSettingOption {
                    value: "groq".to_string(),
                    label: "Groq Whisper".to_string(),
                    provider_env: Some("GROQ_API_KEY".to_string()),
                    binary: None,
                },
                HandSettingOption {
                    value: "openai".to_string(),
                    label: "OpenAI Whisper".to_string(),
                    provider_env: Some("OPENAI_API_KEY".to_string()),
                    binary: None,
                },
            ],
            env_var: None,
        }];

        // User picks groq
        let mut config = HashMap::new();
        config.insert("stt".to_string(), serde_json::json!("groq"));
        let resolved = resolve_settings(&settings, &config);
        assert!(resolved.prompt_block.contains("STT Provider"));
        assert!(resolved.prompt_block.contains("Groq Whisper"));
        assert_eq!(resolved.env_vars, vec!["GROQ_API_KEY"]);
    }

    #[test]
    fn resolve_settings_defaults() {
        let settings = vec![HandSetting {
            key: "stt".to_string(),
            label: "STT".to_string(),
            description: String::new(),
            setting_type: HandSettingType::Select,
            default: "auto".to_string(),
            options: vec![
                HandSettingOption {
                    value: "auto".to_string(),
                    label: "Auto".to_string(),
                    provider_env: None,
                    binary: None,
                },
                HandSettingOption {
                    value: "groq".to_string(),
                    label: "Groq".to_string(),
                    provider_env: Some("GROQ_API_KEY".to_string()),
                    binary: None,
                },
            ],
            env_var: None,
        }];

        // Empty config → uses default "auto"
        let resolved = resolve_settings(&settings, &HashMap::new());
        assert!(resolved.prompt_block.contains("Auto"));
        assert!(
            resolved.env_vars.is_empty(),
            "only selected option env var should be collected"
        );
    }

    #[test]
    fn resolve_settings_toggle_and_text() {
        let settings = vec![
            HandSetting {
                key: "tts_enabled".to_string(),
                label: "TTS".to_string(),
                description: String::new(),
                setting_type: HandSettingType::Toggle,
                default: "false".to_string(),
                options: vec![],
                env_var: None,
            },
            HandSetting {
                key: "custom_model".to_string(),
                label: "Model".to_string(),
                description: String::new(),
                setting_type: HandSettingType::Text,
                default: String::new(),
                options: vec![],
                env_var: None,
            },
        ];

        let mut config = HashMap::new();
        config.insert("tts_enabled".to_string(), serde_json::json!("true"));
        config.insert("custom_model".to_string(), serde_json::json!("large-v3"));
        let resolved = resolve_settings(&settings, &config);
        assert!(resolved.prompt_block.contains("Enabled"));
        assert!(resolved.prompt_block.contains("large-v3"));
    }

    #[test]
    fn hand_requirement_with_install_info() {
        let toml_str = r#"
id = "test"
name = "Test Hand"
description = "A test hand"
category = "content"
tools = []

[[requires]]
key = "ffmpeg"
label = "FFmpeg must be installed"
requirement_type = "binary"
check_value = "ffmpeg"
description = "FFmpeg is the core video processing engine."

[requires.install]
macos = "brew install ffmpeg"
windows = "winget install Gyan.FFmpeg"
linux_apt = "sudo apt install ffmpeg"
linux_dnf = "sudo dnf install ffmpeg-free"
linux_pacman = "sudo pacman -S ffmpeg"
manual_url = "https://ffmpeg.org/download.html"
estimated_time = "2-5 min"

[agent]
name = "test-hand"
description = "Test agent"
system_prompt = "You are a test agent."

[dashboard]
metrics = []
"#;
        let def: HandDefinition = toml::from_str(toml_str).unwrap();
        assert_eq!(def.requires.len(), 1);
        let req = &def.requires[0];
        assert_eq!(
            req.description.as_deref(),
            Some("FFmpeg is the core video processing engine.")
        );
        let install = req.install.as_ref().unwrap();
        assert_eq!(install.macos.as_deref(), Some("brew install ffmpeg"));
        assert_eq!(
            install.windows.as_deref(),
            Some("winget install Gyan.FFmpeg")
        );
        assert_eq!(
            install.linux_apt.as_deref(),
            Some("sudo apt install ffmpeg")
        );
        assert_eq!(
            install.linux_dnf.as_deref(),
            Some("sudo dnf install ffmpeg-free")
        );
        assert_eq!(
            install.linux_pacman.as_deref(),
            Some("sudo pacman -S ffmpeg")
        );
        assert_eq!(
            install.manual_url.as_deref(),
            Some("https://ffmpeg.org/download.html")
        );
        assert_eq!(install.estimated_time.as_deref(), Some("2-5 min"));
        assert!(install.pip.is_none());
        assert!(install.signup_url.is_none());
        assert!(install.steps.is_empty());
    }

    #[test]
    fn hand_requirement_without_install_info_backward_compat() {
        let toml_str = r#"
id = "test"
name = "Test Hand"
description = "A test"
category = "content"
tools = []

[[requires]]
key = "test_bin"
label = "test must be installed"
requirement_type = "binary"
check_value = "test"

[agent]
name = "test-hand"
description = "Test"
system_prompt = "Test."

[dashboard]
metrics = []
"#;
        let def: HandDefinition = toml::from_str(toml_str).unwrap();
        assert_eq!(def.requires.len(), 1);
        assert!(def.requires[0].description.is_none());
        assert!(def.requires[0].install.is_none());
    }

    #[test]
    fn api_key_requirement_with_steps() {
        let toml_str = r#"
id = "test"
name = "Test Hand"
description = "A test"
category = "communication"
tools = []

[[requires]]
key = "API_TOKEN"
label = "API Token"
requirement_type = "api_key"
check_value = "API_TOKEN"
description = "A token from the service."

[requires.install]
signup_url = "https://example.com/signup"
docs_url = "https://example.com/docs"
env_example = "API_TOKEN=your_token_here"
estimated_time = "5-10 min"
steps = [
    "Go to example.com and sign up",
    "Navigate to API settings",
    "Generate a new token",
    "Set it as an environment variable",
]

[agent]
name = "test-hand"
description = "Test"
system_prompt = "Test."

[dashboard]
metrics = []
"#;
        let def: HandDefinition = toml::from_str(toml_str).unwrap();
        assert_eq!(def.requires.len(), 1);
        let req = &def.requires[0];
        let install = req.install.as_ref().unwrap();
        assert_eq!(
            install.signup_url.as_deref(),
            Some("https://example.com/signup")
        );
        assert_eq!(
            install.docs_url.as_deref(),
            Some("https://example.com/docs")
        );
        assert_eq!(
            install.env_example.as_deref(),
            Some("API_TOKEN=your_token_here")
        );
        assert_eq!(install.estimated_time.as_deref(), Some("5-10 min"));
        assert_eq!(install.steps.len(), 4);
        assert_eq!(install.steps[0], "Go to example.com and sign up");
        assert!(install.macos.is_none());
        assert!(install.windows.is_none());
    }

    #[test]
    fn parse_hand_toml_flat_format() {
        let toml_str = r#"
id = "test"
name = "Test Hand"
description = "A test hand"
category = "content"
tools = ["shell_exec"]

[agent]
name = "test-hand"
description = "Test agent"
system_prompt = "You are a test agent."

[dashboard]
metrics = []
"#;
        let def = parse_hand_toml(toml_str).unwrap();
        assert_eq!(def.id, "test");
        assert_eq!(def.name, "Test Hand");
    }

    #[test]
    fn parse_hand_toml_wrapped_format() {
        let toml_str = r#"
[hand]
id = "test"
name = "Test Hand"
description = "A test hand"
category = "content"
tools = ["shell_exec"]

[hand.agent]
name = "test-hand"
description = "Test agent"
system_prompt = "You are a test agent."

[hand.dashboard]
metrics = []
"#;
        let def = parse_hand_toml(toml_str).unwrap();
        assert_eq!(def.id, "test");
        assert_eq!(def.name, "Test Hand");
        assert_eq!(def.agent.name, "test-hand");
    }
}
