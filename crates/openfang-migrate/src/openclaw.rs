//! OpenClaw workspace parser and migration engine.
//!
//! Real OpenClaw installations use a **single JSON5 config file** at
//! `~/.openclaw/openclaw.json` that contains everything: global config,
//! agents, channels, models, tools, cron, hooks, and more.
//!
//! ```text
//! ~/.openclaw/                          (or legacy: ~/.clawdbot, ~/.moldbot, ~/.moltbot)
//! ├── openclaw.json                     # JSON5 — THE config (everything lives here)
//! ├── auth-profiles.json                # Auth credentials
//! ├── sessions/                         # JSONL conversation logs per session key
//! │   ├── main.jsonl
//! │   └── agent:coder:main.jsonl
//! ├── memory/                           # Per-agent MEMORY.md files
//! │   ├── default/MEMORY.md
//! │   └── coder/MEMORY.md
//! ├── memory-search/                    # SQLite vector index
//! ├── skills/                           # Installed skills
//! ├── cron/                             # Cron run state
//! ├── hooks/                            # Webhook hook modules
//! └── workspaces/                       # Per-agent working directories
//! ```

use crate::report::{ItemKind, MigrateItem, MigrationReport, SkippedItem};
use crate::{MigrateError, MigrateOptions};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// OpenClaw JSON5 input types
// ---------------------------------------------------------------------------

/// Top-level openclaw.json structure.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenClawRoot {
    auth: Option<OpenClawAuth>,
    models: Option<OpenClawModels>,
    agents: Option<OpenClawAgents>,
    tools: Option<OpenClawRootTools>,
    channels: Option<OpenClawChannels>,
    cron: Option<serde_json::Value>,
    hooks: Option<serde_json::Value>,
    skills: Option<OpenClawSkills>,
    memory: Option<serde_json::Value>,
    session: Option<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenClawAuth {
    profiles: Option<serde_json::Value>,
    order: Option<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenClawModels {
    providers: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenClawRootTools {
    #[allow(dead_code)]
    profile: Option<serde_json::Value>,
    #[allow(dead_code)]
    allow: Option<serde_json::Value>,
    #[allow(dead_code)]
    deny: Option<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenClawAgents {
    defaults: Option<OpenClawAgentDefaults>,
    list: Vec<OpenClawAgentEntry>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenClawAgentDefaults {
    model: Option<OpenClawAgentModel>,
    workspace: Option<String>,
    tools: Option<OpenClawAgentTools>,
    identity: Option<String>,
}

/// Agent model reference — either `"provider/model"` or `{ primary, fallbacks }`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum OpenClawAgentModel {
    Simple(String),
    Detailed(OpenClawAgentModelDetailed),
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenClawAgentModelDetailed {
    primary: Option<String>,
    fallbacks: Vec<String>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenClawAgentEntry {
    id: String,
    name: Option<String>,
    model: Option<OpenClawAgentModel>,
    tools: Option<OpenClawAgentTools>,
    workspace: Option<String>,
    skills: Option<Vec<String>>,
    identity: Option<String>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenClawAgentTools {
    profile: Option<serde_json::Value>,
    allow: Option<serde_json::Value>,
    deny: Option<serde_json::Value>,
    also_allow: Option<serde_json::Value>,
}

/// Extract a profile name from a Value (string or {name: "..."}  object).
fn extract_profile(val: &serde_json::Value) -> Option<String> {
    val.as_str()
        .map(|s| s.to_string())
        .or_else(|| val.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()))
}

/// Extract a list of strings from a Value (array of strings, or single string).
fn extract_string_list(val: &serde_json::Value) -> Vec<String> {
    match val {
        serde_json::Value::Array(arr) => {
            arr.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()).collect()
        }
        serde_json::Value::String(s) => vec![s.clone()],
        _ => vec![],
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenClawChannels {
    telegram: Option<OpenClawTelegramConfig>,
    discord: Option<OpenClawDiscordConfig>,
    slack: Option<OpenClawSlackConfig>,
    whatsapp: Option<OpenClawWhatsAppConfig>,
    signal: Option<OpenClawSignalConfig>,
    matrix: Option<OpenClawMatrixConfig>,
    #[serde(alias = "googlechat", alias = "googleChat")]
    google_chat: Option<OpenClawGoogleChatConfig>,
    #[serde(alias = "msteams", alias = "msTeams")]
    teams: Option<OpenClawTeamsConfig>,
    irc: Option<OpenClawIrcConfig>,
    mattermost: Option<OpenClawMattermostConfig>,
    feishu: Option<OpenClawFeishuConfig>,
    imessage: Option<OpenClawIMessageConfig>,
    bluebubbles: Option<OpenClawBlueBubblesConfig>,
    #[serde(flatten)]
    other: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenClawTelegramConfig {
    bot_token: Option<String>,
    allow_from: Option<Vec<String>>,
    group_policy: Option<String>,
    dm_policy: Option<String>,
    enabled: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenClawDiscordConfig {
    token: Option<String>,
    guilds: Option<serde_json::Value>,
    dm_policy: Option<String>,
    group_policy: Option<String>,
    allow_from: Option<Vec<String>>,
    enabled: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenClawSlackConfig {
    bot_token: Option<String>,
    app_token: Option<String>,
    dm_policy: Option<String>,
    group_policy: Option<String>,
    allow_from: Option<Vec<String>>,
    enabled: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenClawWhatsAppConfig {
    auth_dir: Option<String>,
    dm_policy: Option<String>,
    allow_from: Option<Vec<String>>,
    group_policy: Option<String>,
    enabled: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenClawSignalConfig {
    http_url: Option<String>,
    http_host: Option<String>,
    http_port: Option<u16>,
    account: Option<String>,
    dm_policy: Option<String>,
    allow_from: Option<Vec<String>>,
    enabled: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenClawMatrixConfig {
    homeserver: Option<String>,
    user_id: Option<String>,
    access_token: Option<String>,
    rooms: Option<Vec<String>>,
    dm_policy: Option<String>,
    allow_from: Option<Vec<String>>,
    enabled: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenClawGoogleChatConfig {
    service_account_file: Option<String>,
    webhook_path: Option<String>,
    bot_user: Option<String>,
    dm_policy: Option<String>,
    enabled: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenClawTeamsConfig {
    app_id: Option<String>,
    app_password: Option<String>,
    tenant_id: Option<String>,
    dm_policy: Option<String>,
    allow_from: Option<Vec<String>>,
    enabled: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenClawIrcConfig {
    host: Option<String>,
    port: Option<u16>,
    tls: Option<bool>,
    nick: Option<String>,
    password: Option<String>,
    channels: Option<Vec<String>>,
    dm_policy: Option<String>,
    allow_from: Option<Vec<String>>,
    enabled: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenClawMattermostConfig {
    bot_token: Option<String>,
    base_url: Option<String>,
    dm_policy: Option<String>,
    allow_from: Option<Vec<String>>,
    enabled: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenClawFeishuConfig {
    app_id: Option<String>,
    app_secret: Option<String>,
    domain: Option<String>,
    dm_policy: Option<String>,
    enabled: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenClawIMessageConfig {
    cli_path: Option<String>,
    db_path: Option<String>,
    dm_policy: Option<String>,
    allow_from: Option<Vec<String>>,
    enabled: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenClawBlueBubblesConfig {
    server_url: Option<String>,
    password: Option<String>,
    dm_policy: Option<String>,
    allow_from: Option<Vec<String>>,
    enabled: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct OpenClawSkills {
    entries: Option<serde_json::Map<String, serde_json::Value>>,
    load: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Legacy YAML input types (backward compat for very old installs)
// ---------------------------------------------------------------------------

/// OpenClaw's legacy config.yaml structure.
#[derive(Debug, Deserialize)]
#[serde(default)]
struct LegacyYamlConfig {
    provider: String,
    model: String,
    api_key_env: Option<String>,
    base_url: Option<String>,
    #[allow(dead_code)]
    temperature: Option<f32>,
    #[allow(dead_code)]
    max_tokens: Option<u32>,
    memory: Option<LegacyYamlMemoryConfig>,
}

impl Default for LegacyYamlConfig {
    fn default() -> Self {
        Self {
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            api_key_env: None,
            base_url: None,
            temperature: None,
            max_tokens: None,
            memory: None,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct LegacyYamlMemoryConfig {
    decay_rate: Option<f32>,
}

/// OpenClaw's legacy agent.yaml structure.
#[derive(Debug, Deserialize)]
#[serde(default)]
struct LegacyYamlAgent {
    name: String,
    description: String,
    model: Option<String>,
    provider: Option<String>,
    system_prompt: Option<String>,
    tools: Vec<String>,
    tool_profile: Option<String>,
    api_key_env: Option<String>,
    base_url: Option<String>,
    tags: Vec<String>,
}

impl Default for LegacyYamlAgent {
    fn default() -> Self {
        Self {
            name: "unnamed".to_string(),
            description: String::new(),
            model: None,
            provider: None,
            system_prompt: None,
            tools: vec![],
            tool_profile: None,
            api_key_env: None,
            base_url: None,
            tags: vec![],
        }
    }
}

/// OpenClaw's legacy channel config structure.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct LegacyYamlChannelConfig {
    #[serde(rename = "type")]
    #[allow(dead_code)]
    channel_type: String,
    bot_token_env: Option<String>,
    app_token_env: Option<String>,
    #[allow(dead_code)]
    phone_number_id_env: Option<String>,
    #[allow(dead_code)]
    access_token_env: Option<String>,
    #[allow(dead_code)]
    verify_token_env: Option<String>,
    #[allow(dead_code)]
    webhook_port: Option<u16>,
    allowed_users: Vec<String>,
    default_agent: Option<String>,
}

// ---------------------------------------------------------------------------
// OpenFang output types (TOML)
// ---------------------------------------------------------------------------

/// OpenFang config.toml structure for serialization.
#[derive(Serialize)]
struct OpenFangConfig {
    default_model: OpenFangModelConfig,
    memory: OpenFangMemorySection,
    network: OpenFangNetworkSection,
    #[serde(skip_serializing_if = "Option::is_none")]
    channels: Option<toml::Value>,
}

#[derive(Serialize)]
struct OpenFangModelConfig {
    provider: String,
    model: String,
    api_key_env: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_url: Option<String>,
}

#[derive(Serialize)]
struct OpenFangMemorySection {
    decay_rate: f32,
}

#[derive(Serialize)]
struct OpenFangNetworkSection {
    listen_addr: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Secrets & policy helpers
// ---------------------------------------------------------------------------

/// Write or update a key in a secrets.env file.
/// File format: one `KEY=value` per line. Existing keys are overwritten.
fn write_secret_env(path: &Path, key: &str, value: &str) -> Result<(), std::io::Error> {
    let mut lines: Vec<String> = if path.exists() {
        std::fs::read_to_string(path)?
            .lines()
            .map(|l| l.to_string())
            .collect()
    } else {
        Vec::new()
    };

    // Upsert
    let prefix = format!("{key}=");
    if let Some(pos) = lines.iter().position(|l| l.starts_with(&prefix)) {
        lines[pos] = format!("{key}={value}");
    } else {
        lines.push(format!("{key}={value}"));
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(path, lines.join("\n") + "\n")?;

    // SECURITY: Restrict file permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }

    Ok(())
}

/// Map OpenClaw DM policy to OpenFang DM policy string.
fn map_dm_policy(oc: &str) -> &'static str {
    match oc.to_lowercase().as_str() {
        "open" => "respond",
        "allowlist" | "allow_list" => "allowed_only",
        "pairing" | "disabled" => "ignore",
        _ => "respond",
    }
}

/// Map OpenClaw group policy to OpenFang group policy string.
fn map_group_policy(oc: &str) -> &'static str {
    match oc.to_lowercase().as_str() {
        "open" => "respond",
        "mention" | "mention_only" => "mention_only",
        "disabled" => "ignore",
        _ => "respond",
    }
}

/// Build a TOML table for a channel with the given fields and optional overrides.
fn build_channel_table(
    fields: Vec<(&str, toml::Value)>,
    dm_policy: Option<&str>,
    group_policy: Option<&str>,
    allow_from: Option<&[String]>,
) -> toml::Value {
    let mut table = toml::map::Map::new();
    for (key, val) in fields {
        table.insert(key.to_string(), val);
    }

    // Add overrides sub-table if any policy is set
    let has_overrides =
        dm_policy.is_some() || group_policy.is_some() || allow_from.is_some_and(|a| !a.is_empty());

    if has_overrides {
        let mut overrides = toml::map::Map::new();
        if let Some(dp) = dm_policy {
            let mapped = map_dm_policy(dp);
            overrides.insert(
                "dm_policy".to_string(),
                toml::Value::String(mapped.to_string()),
            );
        }
        if let Some(gp) = group_policy {
            let mapped = map_group_policy(gp);
            overrides.insert(
                "group_policy".to_string(),
                toml::Value::String(mapped.to_string()),
            );
        }
        if let Some(users) = allow_from {
            if !users.is_empty() {
                let arr: Vec<toml::Value> = users
                    .iter()
                    .map(|u| toml::Value::String(u.clone()))
                    .collect();
                overrides.insert("allowed_users".to_string(), toml::Value::Array(arr));
            }
        }
        table.insert("overrides".to_string(), toml::Value::Table(overrides));
    }

    toml::Value::Table(table)
}

/// Split an OpenClaw model reference like `"provider/model"` into `(provider, model)`.
/// If there's no slash, returns `("anthropic", input)` as a fallback.
fn split_model_ref(model_ref: &str) -> (String, String) {
    if let Some(pos) = model_ref.find('/') {
        let provider = &model_ref[..pos];
        let model = &model_ref[pos + 1..];
        (map_provider(provider), model.to_string())
    } else {
        ("anthropic".to_string(), model_ref.to_string())
    }
}

/// Extract the primary model string from an agent entry, falling back to defaults.
fn extract_primary_model(
    agent: &OpenClawAgentEntry,
    defaults: Option<&OpenClawAgentDefaults>,
) -> Option<String> {
    // Try agent-level model first
    if let Some(ref m) = agent.model {
        match m {
            OpenClawAgentModel::Simple(s) => return Some(s.clone()),
            OpenClawAgentModel::Detailed(d) => {
                if let Some(ref p) = d.primary {
                    return Some(p.clone());
                }
            }
        }
    }
    // Fall back to defaults
    if let Some(defs) = defaults {
        if let Some(ref m) = defs.model {
            match m {
                OpenClawAgentModel::Simple(s) => return Some(s.clone()),
                OpenClawAgentModel::Detailed(d) => return d.primary.clone(),
            }
        }
    }
    None
}

/// Extract fallback model strings from an agent entry.
fn extract_fallback_models(
    agent: &OpenClawAgentEntry,
    defaults: Option<&OpenClawAgentDefaults>,
) -> Vec<String> {
    // Try agent-level
    if let Some(OpenClawAgentModel::Detailed(ref d)) = agent.model {
        if !d.fallbacks.is_empty() {
            return d.fallbacks.clone();
        }
    }
    // Fall back to defaults
    if let Some(defs) = defaults {
        if let Some(OpenClawAgentModel::Detailed(ref d)) = defs.model {
            if !d.fallbacks.is_empty() {
                return d.fallbacks.clone();
            }
        }
    }
    vec![]
}

/// Which config file does this dir contain? Returns the path if found.
fn find_config_file(dir: &Path) -> Option<PathBuf> {
    // Prefer JSON5 config (modern OpenClaw)
    for name in &[
        "openclaw.json",
        "clawdbot.json",
        "moldbot.json",
        "moltbot.json",
    ] {
        let p = dir.join(name);
        if p.exists() {
            return Some(p);
        }
    }
    // Fall back to YAML (very old installs)
    let yaml = dir.join("config.yaml");
    if yaml.exists() {
        return Some(yaml);
    }
    None
}

// Tool name mapping and recognition are shared with the skill system.
use openfang_types::tool_compat::{is_known_openfang_tool, map_tool_name};

/// Map OpenClaw tool profile to OpenFang capability tool list.
/// Delegates to `ToolProfile` so the migration and kernel use identical definitions.
fn tools_for_profile(profile: &str) -> Vec<String> {
    use openfang_types::agent::ToolProfile;
    let p = match profile {
        "minimal" => ToolProfile::Minimal,
        "coding" => ToolProfile::Coding,
        "research" => ToolProfile::Research,
        "messaging" => ToolProfile::Messaging,
        "automation" => ToolProfile::Automation,
        _ => ToolProfile::Full,
    };
    p.tools()
}

/// Map OpenClaw provider name to OpenFang provider name.
fn map_provider(openclaw_provider: &str) -> String {
    match openclaw_provider.to_lowercase().as_str() {
        "anthropic" | "claude" => "anthropic".to_string(),
        "openai" | "gpt" => "openai".to_string(),
        "groq" => "groq".to_string(),
        "ollama" => "ollama".to_string(),
        "openrouter" => "openrouter".to_string(),
        "deepseek" => "deepseek".to_string(),
        "together" => "together".to_string(),
        "mistral" => "mistral".to_string(),
        "fireworks" => "fireworks".to_string(),
        "google" | "gemini" => "google".to_string(),
        "xai" | "grok" => "xai".to_string(),
        "cerebras" => "cerebras".to_string(),
        "sambanova" => "sambanova".to_string(),
        other => other.to_string(),
    }
}

/// Map OpenClaw provider to its default API key env var.
fn default_api_key_env(provider: &str) -> String {
    match provider {
        "anthropic" => "ANTHROPIC_API_KEY".to_string(),
        "openai" => "OPENAI_API_KEY".to_string(),
        "groq" => "GROQ_API_KEY".to_string(),
        "openrouter" => "OPENROUTER_API_KEY".to_string(),
        "deepseek" => "DEEPSEEK_API_KEY".to_string(),
        "together" => "TOGETHER_API_KEY".to_string(),
        "mistral" => "MISTRAL_API_KEY".to_string(),
        "fireworks" => "FIREWORKS_API_KEY".to_string(),
        "google" => "GOOGLE_API_KEY".to_string(),
        "xai" => "XAI_API_KEY".to_string(),
        "cerebras" => "CEREBRAS_API_KEY".to_string(),
        "sambanova" => "SAMBANOVA_API_KEY".to_string(),
        "ollama" => String::new(), // Ollama doesn't need an API key
        _ => format!("{}_API_KEY", provider.to_uppercase()),
    }
}

/// Derive capability grants from the tool list.
fn derive_capabilities(tools: &[String]) -> AgentCapabilities {
    let mut caps = AgentCapabilities::default();

    for tool in tools {
        match tool.as_str() {
            "*" => {
                caps.shell = vec!["*".to_string()];
                caps.network = vec!["*".to_string()];
                caps.agent_message = vec!["*".to_string()];
                caps.agent_spawn = true;
            }
            "shell_exec" => {
                caps.shell = vec!["*".to_string()];
            }
            "web_fetch" | "web_search" | "browser_navigate" => {
                if caps.network.is_empty() {
                    caps.network = vec!["*".to_string()];
                }
            }
            "agent_send" | "agent_list" => {
                if caps.agent_message.is_empty() {
                    caps.agent_message = vec!["*".to_string()];
                }
                caps.agent_spawn = true;
            }
            _ => {}
        }
    }

    caps
}

#[derive(Default)]
struct AgentCapabilities {
    shell: Vec<String>,
    network: Vec<String>,
    agent_message: Vec<String>,
    agent_spawn: bool,
}

// ---------------------------------------------------------------------------
// Auto-detection
// ---------------------------------------------------------------------------

/// Try to find the OpenClaw home directory.
pub fn detect_openclaw_home() -> Option<PathBuf> {
    // Check env override first
    if let Ok(dir) = std::env::var("OPENCLAW_STATE_DIR") {
        let p = PathBuf::from(dir);
        if p.exists() && p.is_dir() {
            return Some(p);
        }
    }

    // Standard locations + legacy dir names
    let home = dirs::home_dir();
    let mut candidates: Vec<Option<PathBuf>> = vec![
        home.as_ref().map(|h| h.join(".openclaw")),
        home.as_ref().map(|h| h.join(".clawdbot")),
        home.as_ref().map(|h| h.join(".moldbot")),
        home.as_ref().map(|h| h.join(".moltbot")),
        home.as_ref().map(|h| h.join("openclaw")),
        home.as_ref().map(|h| h.join(".config").join("openclaw")),
    ];

    // Windows-specific paths
    if let Ok(p) = std::env::var("APPDATA") {
        candidates.push(Some(PathBuf::from(p).join("openclaw")));
    }
    if let Ok(p) = std::env::var("LOCALAPPDATA") {
        candidates.push(Some(PathBuf::from(p).join("openclaw")));
    }

    for candidate in candidates.into_iter().flatten() {
        if candidate.exists() && candidate.is_dir() {
            // Verify it looks like an OpenClaw workspace
            if find_config_file(&candidate).is_some() {
                return Some(candidate);
            }
            // Also accept if it has agents or sessions dirs
            if candidate.join("sessions").exists() || candidate.join("memory").exists() {
                return Some(candidate);
            }
        }
    }

    None
}

/// Scan an OpenClaw workspace and return what's available for migration.
pub fn scan_openclaw_workspace(path: &Path) -> ScanResult {
    let config_file = find_config_file(path);
    let is_json5 = config_file
        .as_ref()
        .is_some_and(|p| p.extension().is_some_and(|e| e == "json"));

    let mut result = ScanResult {
        path: path.display().to_string(),
        has_config: config_file.is_some(),
        agents: vec![],
        channels: vec![],
        skills: vec![],
        has_memory: false,
    };

    if let (true, Some(ref cf)) = (is_json5, &config_file) {
        scan_from_json5(path, cf, &mut result);
    } else {
        scan_from_legacy_yaml(path, &mut result);
    }

    result
}

fn scan_from_json5(base: &Path, config_path: &Path, result: &mut ScanResult) {
    let content = match std::fs::read_to_string(config_path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let root: OpenClawRoot = match json5::from_str(&content) {
        Ok(r) => r,
        Err(_) => return,
    };

    // Agents from JSON config
    if let Some(ref agents) = root.agents {
        for entry in &agents.list {
            let id = entry.id.clone();
            let name = entry.name.clone().unwrap_or_else(|| id.clone());

            let (provider, model) = extract_primary_model(entry, agents.defaults.as_ref())
                .map(|m| split_model_ref(&m))
                .unwrap_or_else(|| ("anthropic".to_string(), String::new()));

            let tool_count = entry
                .tools
                .as_ref()
                .and_then(|t| t.allow.as_ref())
                .map(|a| extract_string_list(a).len())
                .or_else(|| {
                    entry
                        .tools
                        .as_ref()
                        .and_then(|t| t.profile.as_ref())
                        .and_then(extract_profile)
                        .map(|p| tools_for_profile(&p).len())
                })
                .unwrap_or(3);

            // Check physical memory dirs
            let has_memory = base.join("memory").join(&id).join("MEMORY.md").exists();
            let has_sessions = base.join("sessions").exists();
            let has_workspace = base.join("workspaces").join(&id).exists();

            if has_memory {
                result.has_memory = true;
            }

            result.agents.push(ScannedAgent {
                name,
                description: String::new(),
                provider,
                model,
                tool_count,
                has_memory,
                has_sessions,
                has_workspace,
            });
        }
    }

    // Channels from JSON config — scan all 13 typed fields + catch-all
    if let Some(ref channels) = root.channels {
        if channels.telegram.is_some() {
            result.channels.push("telegram".to_string());
        }
        if channels.discord.is_some() {
            result.channels.push("discord".to_string());
        }
        if channels.slack.is_some() {
            result.channels.push("slack".to_string());
        }
        if channels.whatsapp.is_some() {
            result.channels.push("whatsapp".to_string());
        }
        if channels.signal.is_some() {
            result.channels.push("signal".to_string());
        }
        if channels.matrix.is_some() {
            result.channels.push("matrix".to_string());
        }
        if channels.google_chat.is_some() {
            result.channels.push("google_chat".to_string());
        }
        if channels.teams.is_some() {
            result.channels.push("teams".to_string());
        }
        if channels.irc.is_some() {
            result.channels.push("irc".to_string());
        }
        if channels.mattermost.is_some() {
            result.channels.push("mattermost".to_string());
        }
        if channels.feishu.is_some() {
            result.channels.push("feishu".to_string());
        }
        if channels.imessage.is_some() {
            result.channels.push("imessage".to_string());
        }
        if channels.bluebubbles.is_some() {
            result.channels.push("bluebubbles".to_string());
        }
        for key in channels.other.keys() {
            result.channels.push(key.clone());
        }
    }

    // Skills from JSON config
    if let Some(ref skills) = root.skills {
        if let Some(ref entries) = skills.entries {
            for key in entries.keys() {
                result.skills.push(key.clone());
            }
        }
    }

    // Also check physical memory dir
    let memory_dir = base.join("memory");
    if memory_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&memory_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() && entry.path().join("MEMORY.md").exists() {
                    result.has_memory = true;
                    break;
                }
            }
        }
    }
}

fn scan_from_legacy_yaml(path: &Path, result: &mut ScanResult) {
    // Scan agents from agents/ dir
    let agents_dir = path.join("agents");
    if agents_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&agents_dir) {
            for entry in entries.flatten() {
                let agent_path = entry.path();
                if !agent_path.is_dir() {
                    continue;
                }
                let agent_yaml = agent_path.join("agent.yaml");
                if !agent_yaml.exists() {
                    continue;
                }

                let name = agent_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                let has_memory = agent_path.join("MEMORY.md").exists();
                let has_sessions = agent_path.join("sessions").exists();
                let has_workspace = agent_path.join("workspace").exists();

                if has_memory {
                    result.has_memory = true;
                }

                let mut description = String::new();
                let mut provider = String::new();
                let mut model = String::new();
                let mut tool_count = 0;

                if let Ok(yaml_str) = std::fs::read_to_string(&agent_yaml) {
                    if let Ok(oc) = serde_yaml::from_str::<LegacyYamlAgent>(&yaml_str) {
                        description = oc.description.clone();
                        provider = oc.provider.unwrap_or_default();
                        model = oc.model.unwrap_or_default();
                        tool_count = if !oc.tools.is_empty() {
                            oc.tools.len()
                        } else if oc.tool_profile.is_some() {
                            tools_for_profile(oc.tool_profile.as_deref().unwrap_or("")).len()
                        } else {
                            3
                        };
                    }
                }

                result.agents.push(ScannedAgent {
                    name,
                    description,
                    provider,
                    model,
                    tool_count,
                    has_memory,
                    has_sessions,
                    has_workspace,
                });
            }
        }
    }

    // Scan channels from messaging/ dir — all 13 possible channels
    let messaging_dir = path.join("messaging");
    if messaging_dir.exists() {
        for name in &[
            "telegram",
            "discord",
            "slack",
            "whatsapp",
            "signal",
            "matrix",
            "irc",
            "mattermost",
            "feishu",
            "googlechat",
            "msteams",
            "imessage",
            "bluebubbles",
            "email",
        ] {
            if messaging_dir.join(format!("{name}.yaml")).exists() {
                result.channels.push(name.to_string());
            }
        }
    }

    // Scan skills
    let skills_dir = path.join("skills");
    if skills_dir.exists() {
        for subdir in &["community", "custom"] {
            let sub = skills_dir.join(subdir);
            if let Ok(entries) = std::fs::read_dir(&sub) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        let name = entry
                            .path()
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        if !name.is_empty() {
                            result.skills.push(name);
                        }
                    }
                }
            }
        }
    }
}

/// Result of scanning an OpenClaw workspace.
#[derive(Debug, Clone, Serialize)]
pub struct ScanResult {
    pub path: String,
    pub has_config: bool,
    pub agents: Vec<ScannedAgent>,
    pub channels: Vec<String>,
    pub skills: Vec<String>,
    pub has_memory: bool,
}

/// An agent found during scanning.
#[derive(Debug, Clone, Serialize)]
pub struct ScannedAgent {
    pub name: String,
    pub description: String,
    pub provider: String,
    pub model: String,
    pub tool_count: usize,
    pub has_memory: bool,
    pub has_sessions: bool,
    pub has_workspace: bool,
}

// ---------------------------------------------------------------------------
// Migration entry point
// ---------------------------------------------------------------------------

/// Run the OpenClaw migration.
pub fn migrate(options: &MigrateOptions) -> Result<MigrationReport, MigrateError> {
    let source = &options.source_dir;
    let target = &options.target_dir;

    if !source.exists() {
        return Err(MigrateError::SourceNotFound(source.clone()));
    }

    info!("Migrating from OpenClaw: {}", source.display());

    let mut report = MigrationReport {
        source: "OpenClaw".to_string(),
        dry_run: options.dry_run,
        ..Default::default()
    };

    // Determine config format
    let config_file = find_config_file(source);
    let is_json5 = config_file
        .as_ref()
        .is_some_and(|p| p.extension().is_some_and(|e| e == "json"));

    if is_json5 {
        migrate_from_json5(source, target, options.dry_run, &mut report)?;
    } else {
        migrate_from_legacy_yaml(source, target, options.dry_run, &mut report)?;
    }

    // Save report
    if !options.dry_run {
        let report_md = report.to_markdown();
        let report_path = target.join("migration_report.md");
        let _ = std::fs::write(&report_path, &report_md);
    }

    Ok(report)
}

// ---------------------------------------------------------------------------
// JSON5 migration flow (modern OpenClaw)
// ---------------------------------------------------------------------------

fn migrate_from_json5(
    source: &Path,
    target: &Path,
    dry_run: bool,
    report: &mut MigrationReport,
) -> Result<(), MigrateError> {
    let config_path = find_config_file(source).ok_or_else(|| {
        MigrateError::ConfigParse("No openclaw.json found in workspace".to_string())
    })?;

    let content = std::fs::read_to_string(&config_path)?;
    let root: OpenClawRoot = json5::from_str(&content)
        .map_err(|e| MigrateError::Json5Parse(format!("{}: {e}", config_path.display())))?;

    // 1. Migrate config
    migrate_config_from_json(&root, target, dry_run, report)?;

    // 2. Migrate agents
    migrate_agents_from_json(&root, target, dry_run, report)?;

    // 3. Migrate memory files
    migrate_memory_files(source, &root, target, dry_run, report)?;

    // 4. Migrate workspace dirs
    migrate_workspace_dirs(source, &root, target, dry_run, report)?;

    // 5. Migrate sessions
    migrate_sessions(source, target, dry_run, report)?;

    // 6. Report skipped features
    report_skipped_features(&root, source, report);

    info!("JSON5 migration complete");
    Ok(())
}

// ---------------------------------------------------------------------------
// Config migration from JSON5
// ---------------------------------------------------------------------------

fn migrate_config_from_json(
    root: &OpenClawRoot,
    target: &Path,
    dry_run: bool,
    report: &mut MigrationReport,
) -> Result<(), MigrateError> {
    // Extract default model from agents.defaults.model
    let (provider, model) = root
        .agents
        .as_ref()
        .and_then(|a| a.defaults.as_ref())
        .and_then(|d| d.model.as_ref())
        .and_then(|m| match m {
            OpenClawAgentModel::Simple(s) => Some(s.clone()),
            OpenClawAgentModel::Detailed(d) => d.primary.clone(),
        })
        .map(|m| split_model_ref(&m))
        .unwrap_or_else(|| {
            (
                "anthropic".to_string(),
                "claude-sonnet-4-20250514".to_string(),
            )
        });

    let api_key_env = default_api_key_env(&provider);

    // Extract channels (writes secrets.env)
    let channels = migrate_channels_from_json(root, target, dry_run, report);

    let of_config = OpenFangConfig {
        default_model: OpenFangModelConfig {
            provider,
            model,
            api_key_env,
            base_url: None,
        },
        memory: OpenFangMemorySection { decay_rate: 0.05 },
        network: OpenFangNetworkSection {
            listen_addr: "127.0.0.1:4200".to_string(),
        },
        channels,
    };

    let toml_str = toml::to_string_pretty(&of_config)?;

    let config_content = format!(
        "# OpenFang Agent OS configuration\n\
         # Migrated from OpenClaw on {}\n\n\
         {toml_str}",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
    );

    let dest = target.join("config.toml");

    if !dry_run {
        std::fs::create_dir_all(target)?;
        std::fs::write(&dest, &config_content)?;
    }

    report.imported.push(MigrateItem {
        kind: ItemKind::Config,
        name: "openclaw.json".to_string(),
        destination: dest.display().to_string(),
    });

    info!("Migrated openclaw.json -> config.toml");
    Ok(())
}

// ---------------------------------------------------------------------------
// Channel migration from JSON5
// ---------------------------------------------------------------------------

fn migrate_channels_from_json(
    root: &OpenClawRoot,
    target: &Path,
    dry_run: bool,
    report: &mut MigrationReport,
) -> Option<toml::Value> {
    let oc_channels = root.channels.as_ref()?;

    let mut channels_table = toml::map::Map::new();
    let secrets_path = target.join("secrets.env");

    /// Helper: write a secret and report it.
    fn emit_secret(
        path: &Path,
        dry_run: bool,
        key: &str,
        value: &str,
        report: &mut MigrationReport,
    ) {
        if value.is_empty() {
            return;
        }
        if !dry_run {
            if let Err(e) = write_secret_env(path, key, value) {
                report
                    .warnings
                    .push(format!("Failed to write {key} to secrets.env: {e}"));
                return;
            }
        }
        report.imported.push(MigrateItem {
            kind: ItemKind::Secret,
            name: key.to_string(),
            destination: "secrets.env".to_string(),
        });
    }

    // --- Telegram ---
    if let Some(ref tg) = oc_channels.telegram {
        if tg.enabled.unwrap_or(true) {
            if let Some(ref token) = tg.bot_token {
                emit_secret(&secrets_path, dry_run, "TELEGRAM_BOT_TOKEN", token, report);
            }
            let mut fields: Vec<(&str, toml::Value)> = vec![(
                "bot_token_env",
                toml::Value::String("TELEGRAM_BOT_TOKEN".into()),
            )];
            if let Some(ref users) = tg.allow_from {
                if !users.is_empty() {
                    let arr: Vec<toml::Value> = users
                        .iter()
                        .map(|u| toml::Value::String(u.clone()))
                        .collect();
                    fields.push(("allowed_users", toml::Value::Array(arr)));
                }
            }
            channels_table.insert(
                "telegram".to_string(),
                build_channel_table(
                    fields,
                    tg.dm_policy.as_deref(),
                    tg.group_policy.as_deref(),
                    tg.allow_from.as_deref(),
                ),
            );
            report.imported.push(MigrateItem {
                kind: ItemKind::Channel,
                name: "telegram".to_string(),
                destination: "config.toml [channels.telegram]".to_string(),
            });
        }
    }

    // --- Discord ---
    if let Some(ref dc) = oc_channels.discord {
        if dc.enabled.unwrap_or(true) {
            if let Some(ref token) = dc.token {
                emit_secret(&secrets_path, dry_run, "DISCORD_BOT_TOKEN", token, report);
            }
            let fields: Vec<(&str, toml::Value)> = vec![(
                "bot_token_env",
                toml::Value::String("DISCORD_BOT_TOKEN".into()),
            )];
            channels_table.insert(
                "discord".to_string(),
                build_channel_table(
                    fields,
                    dc.dm_policy.as_deref(),
                    dc.group_policy.as_deref(),
                    dc.allow_from.as_deref(),
                ),
            );
            report.imported.push(MigrateItem {
                kind: ItemKind::Channel,
                name: "discord".to_string(),
                destination: "config.toml [channels.discord]".to_string(),
            });
        }
    }

    // --- Slack ---
    if let Some(ref sl) = oc_channels.slack {
        if sl.enabled.unwrap_or(true) {
            if let Some(ref token) = sl.bot_token {
                emit_secret(&secrets_path, dry_run, "SLACK_BOT_TOKEN", token, report);
            }
            if let Some(ref token) = sl.app_token {
                emit_secret(&secrets_path, dry_run, "SLACK_APP_TOKEN", token, report);
            }
            let fields: Vec<(&str, toml::Value)> = vec![
                (
                    "bot_token_env",
                    toml::Value::String("SLACK_BOT_TOKEN".into()),
                ),
                (
                    "app_token_env",
                    toml::Value::String("SLACK_APP_TOKEN".into()),
                ),
            ];
            channels_table.insert(
                "slack".to_string(),
                build_channel_table(
                    fields,
                    sl.dm_policy.as_deref(),
                    sl.group_policy.as_deref(),
                    sl.allow_from.as_deref(),
                ),
            );
            report.imported.push(MigrateItem {
                kind: ItemKind::Channel,
                name: "slack".to_string(),
                destination: "config.toml [channels.slack]".to_string(),
            });
        }
    }

    // --- WhatsApp ---
    if let Some(ref wa) = oc_channels.whatsapp {
        if wa.enabled.unwrap_or(true) {
            // WhatsApp uses Baileys credential dir — copy it, warn user
            if let Some(ref auth_dir) = wa.auth_dir {
                let src_path = PathBuf::from(auth_dir);
                if src_path.exists() {
                    let dest_creds = target.join("credentials").join("whatsapp");
                    if !dry_run {
                        if let Err(e) = copy_dir_recursive(&src_path, &dest_creds) {
                            report
                                .warnings
                                .push(format!("Failed to copy WhatsApp credentials: {e}"));
                        }
                    }
                    report.imported.push(MigrateItem {
                        kind: ItemKind::Secret,
                        name: "whatsapp/credentials".to_string(),
                        destination: dest_creds.display().to_string(),
                    });
                    report.warnings.push(
                        "WhatsApp Baileys credentials copied — you may need to re-authenticate"
                            .to_string(),
                    );
                }
            }
            let mut fields: Vec<(&str, toml::Value)> = vec![(
                "access_token_env",
                toml::Value::String("WHATSAPP_ACCESS_TOKEN".into()),
            )];
            if let Some(ref users) = wa.allow_from {
                if !users.is_empty() {
                    let arr: Vec<toml::Value> = users
                        .iter()
                        .map(|u| toml::Value::String(u.clone()))
                        .collect();
                    fields.push(("allowed_users", toml::Value::Array(arr)));
                }
            }
            channels_table.insert(
                "whatsapp".to_string(),
                build_channel_table(
                    fields,
                    wa.dm_policy.as_deref(),
                    wa.group_policy.as_deref(),
                    wa.allow_from.as_deref(),
                ),
            );
            report.imported.push(MigrateItem {
                kind: ItemKind::Channel,
                name: "whatsapp".to_string(),
                destination: "config.toml [channels.whatsapp]".to_string(),
            });
        }
    }

    // --- Signal ---
    if let Some(ref sig) = oc_channels.signal {
        if sig.enabled.unwrap_or(true) {
            // Construct API URL from host+port or use http_url directly
            let api_url = sig.http_url.clone().unwrap_or_else(|| {
                let host = sig.http_host.as_deref().unwrap_or("localhost");
                let port = sig.http_port.unwrap_or(8080);
                format!("http://{host}:{port}")
            });
            let mut fields: Vec<(&str, toml::Value)> =
                vec![("api_url", toml::Value::String(api_url))];
            if let Some(ref account) = sig.account {
                fields.push(("phone_number", toml::Value::String(account.clone())));
            }
            channels_table.insert(
                "signal".to_string(),
                build_channel_table(
                    fields,
                    sig.dm_policy.as_deref(),
                    None,
                    sig.allow_from.as_deref(),
                ),
            );
            report.imported.push(MigrateItem {
                kind: ItemKind::Channel,
                name: "signal".to_string(),
                destination: "config.toml [channels.signal]".to_string(),
            });
        }
    }

    // --- Matrix ---
    if let Some(ref mx) = oc_channels.matrix {
        if mx.enabled.unwrap_or(true) {
            if let Some(ref token) = mx.access_token {
                emit_secret(&secrets_path, dry_run, "MATRIX_ACCESS_TOKEN", token, report);
            }
            let mut fields: Vec<(&str, toml::Value)> = vec![(
                "access_token_env",
                toml::Value::String("MATRIX_ACCESS_TOKEN".into()),
            )];
            if let Some(ref hs) = mx.homeserver {
                fields.push(("homeserver_url", toml::Value::String(hs.clone())));
            }
            if let Some(ref uid) = mx.user_id {
                fields.push(("user_id", toml::Value::String(uid.clone())));
            }
            if let Some(ref rooms) = mx.rooms {
                if !rooms.is_empty() {
                    let arr: Vec<toml::Value> = rooms
                        .iter()
                        .map(|r| toml::Value::String(r.clone()))
                        .collect();
                    fields.push(("rooms", toml::Value::Array(arr)));
                }
            }
            channels_table.insert(
                "matrix".to_string(),
                build_channel_table(
                    fields,
                    mx.dm_policy.as_deref(),
                    None,
                    mx.allow_from.as_deref(),
                ),
            );
            report.imported.push(MigrateItem {
                kind: ItemKind::Channel,
                name: "matrix".to_string(),
                destination: "config.toml [channels.matrix]".to_string(),
            });
        }
    }

    // --- Google Chat ---
    if let Some(ref gc) = oc_channels.google_chat {
        if gc.enabled.unwrap_or(true) {
            // Copy service account file if it exists
            if let Some(ref sa_file) = gc.service_account_file {
                let src_sa = PathBuf::from(sa_file);
                if src_sa.exists() {
                    let dest_sa = target.join("credentials").join("google_chat_sa.json");
                    if !dry_run {
                        if let Some(parent) = dest_sa.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        if let Err(e) = std::fs::copy(&src_sa, &dest_sa) {
                            report
                                .warnings
                                .push(format!("Failed to copy Google Chat SA file: {e}"));
                        }
                    }
                    report.imported.push(MigrateItem {
                        kind: ItemKind::Secret,
                        name: "google_chat/service_account".to_string(),
                        destination: dest_sa.display().to_string(),
                    });
                }
            }
            let fields: Vec<(&str, toml::Value)> = vec![(
                "service_account_env",
                toml::Value::String("GOOGLE_CHAT_SA_FILE".into()),
            )];
            channels_table.insert(
                "google_chat".to_string(),
                build_channel_table(fields, gc.dm_policy.as_deref(), None, None),
            );
            report.imported.push(MigrateItem {
                kind: ItemKind::Channel,
                name: "google_chat".to_string(),
                destination: "config.toml [channels.google_chat]".to_string(),
            });
        }
    }

    // --- Teams ---
    if let Some(ref tm) = oc_channels.teams {
        if tm.enabled.unwrap_or(true) {
            if let Some(ref pw) = tm.app_password {
                emit_secret(&secrets_path, dry_run, "TEAMS_APP_PASSWORD", pw, report);
            }
            let mut fields: Vec<(&str, toml::Value)> = vec![(
                "app_password_env",
                toml::Value::String("TEAMS_APP_PASSWORD".into()),
            )];
            if let Some(ref id) = tm.app_id {
                fields.push(("app_id", toml::Value::String(id.clone())));
            }
            if let Some(ref tenant) = tm.tenant_id {
                fields.push(("tenant_id", toml::Value::String(tenant.clone())));
            }
            channels_table.insert(
                "teams".to_string(),
                build_channel_table(
                    fields,
                    tm.dm_policy.as_deref(),
                    None,
                    tm.allow_from.as_deref(),
                ),
            );
            report.imported.push(MigrateItem {
                kind: ItemKind::Channel,
                name: "teams".to_string(),
                destination: "config.toml [channels.teams]".to_string(),
            });
        }
    }

    // --- IRC ---
    if let Some(ref irc) = oc_channels.irc {
        if irc.enabled.unwrap_or(true) {
            if let Some(ref pw) = irc.password {
                emit_secret(&secrets_path, dry_run, "IRC_PASSWORD", pw, report);
            }
            let mut fields: Vec<(&str, toml::Value)> = Vec::new();
            if let Some(ref host) = irc.host {
                fields.push(("server", toml::Value::String(host.clone())));
            }
            if let Some(port) = irc.port {
                fields.push(("port", toml::Value::Integer(port as i64)));
            }
            if let Some(ref nick) = irc.nick {
                fields.push(("nickname", toml::Value::String(nick.clone())));
            }
            if let Some(tls) = irc.tls {
                fields.push(("use_tls", toml::Value::Boolean(tls)));
            }
            if irc.password.is_some() {
                fields.push(("password_env", toml::Value::String("IRC_PASSWORD".into())));
            }
            if let Some(ref chans) = irc.channels {
                if !chans.is_empty() {
                    let arr: Vec<toml::Value> = chans
                        .iter()
                        .map(|c| toml::Value::String(c.clone()))
                        .collect();
                    fields.push(("channels", toml::Value::Array(arr)));
                }
            }
            channels_table.insert(
                "irc".to_string(),
                build_channel_table(
                    fields,
                    irc.dm_policy.as_deref(),
                    None,
                    irc.allow_from.as_deref(),
                ),
            );
            report.imported.push(MigrateItem {
                kind: ItemKind::Channel,
                name: "irc".to_string(),
                destination: "config.toml [channels.irc]".to_string(),
            });
        }
    }

    // --- Mattermost ---
    if let Some(ref mm) = oc_channels.mattermost {
        if mm.enabled.unwrap_or(true) {
            if let Some(ref token) = mm.bot_token {
                emit_secret(&secrets_path, dry_run, "MATTERMOST_TOKEN", token, report);
            }
            let mut fields: Vec<(&str, toml::Value)> = vec![(
                "bot_token_env",
                toml::Value::String("MATTERMOST_TOKEN".into()),
            )];
            if let Some(ref url) = mm.base_url {
                fields.push(("server_url", toml::Value::String(url.clone())));
            }
            channels_table.insert(
                "mattermost".to_string(),
                build_channel_table(
                    fields,
                    mm.dm_policy.as_deref(),
                    None,
                    mm.allow_from.as_deref(),
                ),
            );
            report.imported.push(MigrateItem {
                kind: ItemKind::Channel,
                name: "mattermost".to_string(),
                destination: "config.toml [channels.mattermost]".to_string(),
            });
        }
    }

    // --- Feishu ---
    if let Some(ref fs) = oc_channels.feishu {
        if fs.enabled.unwrap_or(true) {
            if let Some(ref secret) = fs.app_secret {
                emit_secret(&secrets_path, dry_run, "FEISHU_APP_SECRET", secret, report);
            }
            let mut fields: Vec<(&str, toml::Value)> = vec![(
                "app_secret_env",
                toml::Value::String("FEISHU_APP_SECRET".into()),
            )];
            if let Some(ref id) = fs.app_id {
                fields.push(("app_id", toml::Value::String(id.clone())));
            }
            if let Some(ref domain) = fs.domain {
                fields.push(("domain", toml::Value::String(domain.clone())));
            }
            channels_table.insert(
                "feishu".to_string(),
                build_channel_table(fields, fs.dm_policy.as_deref(), None, None),
            );
            report.imported.push(MigrateItem {
                kind: ItemKind::Channel,
                name: "feishu".to_string(),
                destination: "config.toml [channels.feishu]".to_string(),
            });
        }
    }

    // --- iMessage (skip — macOS-only, manual setup) ---
    if oc_channels.imessage.is_some() {
        report.skipped.push(SkippedItem {
            kind: ItemKind::Channel,
            name: "imessage".to_string(),
            reason: "macOS-only channel — requires manual setup on the target Mac".to_string(),
        });
    }

    // --- BlueBubbles (skip — no OpenFang adapter) ---
    if oc_channels.bluebubbles.is_some() {
        report.skipped.push(SkippedItem {
            kind: ItemKind::Channel,
            name: "bluebubbles".to_string(),
            reason: "No OpenFang adapter available — consider using the iMessage channel instead"
                .to_string(),
        });
    }

    // --- Unknown channels from the catch-all ---
    for key in oc_channels.other.keys() {
        report.skipped.push(SkippedItem {
            kind: ItemKind::Channel,
            name: key.clone(),
            reason: format!("Unknown channel '{key}' — not mapped to any OpenFang adapter"),
        });
    }

    if channels_table.is_empty() {
        None
    } else {
        Some(toml::Value::Table(channels_table))
    }
}

// ---------------------------------------------------------------------------
// Agent migration from JSON5
// ---------------------------------------------------------------------------

fn migrate_agents_from_json(
    root: &OpenClawRoot,
    target: &Path,
    dry_run: bool,
    report: &mut MigrationReport,
) -> Result<(), MigrateError> {
    let agents = match root.agents.as_ref() {
        Some(a) => a,
        None => {
            report
                .warnings
                .push("No agents section found in openclaw.json".to_string());
            return Ok(());
        }
    };

    let defaults = agents.defaults.as_ref();

    for entry in &agents.list {
        let id = &entry.id;
        if id.is_empty() {
            continue;
        }

        match convert_agent_from_json(entry, defaults) {
            Ok((toml_str, unmapped_tools)) => {
                let dest_dir = target.join("agents").join(id);
                let dest_file = dest_dir.join("agent.toml");

                if !dry_run {
                    std::fs::create_dir_all(&dest_dir)?;
                    std::fs::write(&dest_file, &toml_str)?;
                }

                report.imported.push(MigrateItem {
                    kind: ItemKind::Agent,
                    name: id.clone(),
                    destination: dest_file.display().to_string(),
                });

                for tool in &unmapped_tools {
                    report.warnings.push(format!(
                        "Agent '{id}': tool '{tool}' has no OpenFang equivalent and was skipped"
                    ));
                }

                info!("Migrated agent: {id}");
            }
            Err(e) => {
                warn!("Failed to migrate agent {id}: {e}");
                report.skipped.push(SkippedItem {
                    kind: ItemKind::Agent,
                    name: id.clone(),
                    reason: e.to_string(),
                });
            }
        }
    }

    Ok(())
}

fn convert_agent_from_json(
    entry: &OpenClawAgentEntry,
    defaults: Option<&OpenClawAgentDefaults>,
) -> Result<(String, Vec<String>), MigrateError> {
    let id = &entry.id;
    let display_name = entry.name.clone().unwrap_or_else(|| id.clone());

    // Resolve model
    let primary_ref = extract_primary_model(entry, defaults)
        .unwrap_or_else(|| "anthropic/claude-sonnet-4-20250514".to_string());
    let (provider, model) = split_model_ref(&primary_ref);

    // Resolve fallback models
    let fallbacks = extract_fallback_models(entry, defaults);

    // Resolve tools
    let mut unmapped_tools = Vec::new();
    let tools: Vec<String> = if let Some(ref agent_tools) = entry.tools {
        if let Some(ref allow_val) = agent_tools.allow {
            let allow = extract_string_list(allow_val);
            let mut mapped = Vec::new();
            for t in &allow {
                if is_known_openfang_tool(t) {
                    mapped.push(t.clone());
                } else if let Some(of_name) = map_tool_name(t) {
                    mapped.push(of_name.to_string());
                } else {
                    unmapped_tools.push(t.clone());
                }
            }
            // also_allow
            if let Some(ref also_val) = agent_tools.also_allow {
                let also = extract_string_list(also_val);
                for t in &also {
                    if is_known_openfang_tool(t) {
                        mapped.push(t.clone());
                    } else if let Some(of_name) = map_tool_name(t) {
                        mapped.push(of_name.to_string());
                    } else {
                        unmapped_tools.push(t.clone());
                    }
                }
            }
            mapped
        } else if let Some(ref profile_val) = agent_tools.profile {
            let profile_name = extract_profile(profile_val).unwrap_or_default();
            tools_for_profile(&profile_name)
        } else {
            resolve_default_tools(defaults)
        }
    } else {
        resolve_default_tools(defaults)
    };

    // Derive capabilities
    let caps = derive_capabilities(&tools);

    let api_key_env = {
        let env = default_api_key_env(&provider);
        if env.is_empty() {
            None
        } else {
            Some(env)
        }
    };

    // System prompt from identity
    let system_prompt = entry
        .identity
        .clone()
        .or_else(|| defaults.and_then(|d| d.identity.clone()))
        .unwrap_or_else(|| {
            format!(
                "You are {display_name}, an AI agent running on the OpenFang Agent OS. You are helpful, concise, and accurate."
            )
        });

    // Build agent TOML
    let mut toml_str = String::new();
    toml_str.push_str(&format!(
        "# OpenFang agent manifest\n# Migrated from OpenClaw agent '{id}'\n\n"
    ));
    toml_str.push_str(&format!(
        "name = \"{}\"\n",
        display_name.replace('"', "\\\"")
    ));
    toml_str.push_str("version = \"0.1.0\"\n");
    toml_str.push_str(&format!(
        "description = \"Migrated from OpenClaw agent '{id}'\"\n"
    ));
    toml_str.push_str("author = \"openfang\"\n");
    toml_str.push_str("module = \"builtin:chat\"\n");

    toml_str.push_str("\n[model]\n");
    toml_str.push_str(&format!("provider = \"{provider}\"\n"));
    toml_str.push_str(&format!("model = \"{model}\"\n"));
    toml_str.push_str(&format!(
        "system_prompt = \"\"\"\n{system_prompt}\n\"\"\"\n"
    ));

    if let Some(ref api_key) = api_key_env {
        toml_str.push_str(&format!("api_key_env = \"{api_key}\"\n"));
    }

    // Fallback models
    for fb in &fallbacks {
        let (fb_provider, fb_model) = split_model_ref(fb);
        let fb_api_key = default_api_key_env(&fb_provider);
        toml_str.push_str("\n[[fallback_models]]\n");
        toml_str.push_str(&format!("provider = \"{fb_provider}\"\n"));
        toml_str.push_str(&format!("model = \"{fb_model}\"\n"));
        if !fb_api_key.is_empty() {
            toml_str.push_str(&format!("api_key_env = \"{fb_api_key}\"\n"));
        }
    }

    // Capabilities section
    toml_str.push_str("\n[capabilities]\n");
    let tools_str: Vec<String> = tools.iter().map(|t| format!("\"{t}\"")).collect();
    toml_str.push_str(&format!("tools = [{}]\n", tools_str.join(", ")));
    toml_str.push_str("memory_read = [\"*\"]\n");
    toml_str.push_str("memory_write = [\"self.*\"]\n");

    if !caps.network.is_empty() {
        let net_str: Vec<String> = caps.network.iter().map(|n| format!("\"{n}\"")).collect();
        toml_str.push_str(&format!("network = [{}]\n", net_str.join(", ")));
    }
    if !caps.shell.is_empty() {
        let shell_str: Vec<String> = caps.shell.iter().map(|s| format!("\"{s}\"")).collect();
        toml_str.push_str(&format!("shell = [{}]\n", shell_str.join(", ")));
    }
    if !caps.agent_message.is_empty() {
        let msg_str: Vec<String> = caps
            .agent_message
            .iter()
            .map(|m| format!("\"{m}\""))
            .collect();
        toml_str.push_str(&format!("agent_message = [{}]\n", msg_str.join(", ")));
    }
    if caps.agent_spawn {
        toml_str.push_str("agent_spawn = true\n");
    }

    // Tool profile hint
    if let Some(ref agent_tools) = entry.tools {
        if let Some(ref profile_val) = agent_tools.profile {
            if let Some(profile) = extract_profile(profile_val) {
                toml_str.push_str(&format!("\nprofile = \"{profile}\"\n"));
            }
        }
    }

    Ok((toml_str, unmapped_tools))
}

fn resolve_default_tools(defaults: Option<&OpenClawAgentDefaults>) -> Vec<String> {
    if let Some(defs) = defaults {
        if let Some(ref tools) = defs.tools {
            if let Some(ref profile_val) = tools.profile {
                if let Some(profile) = extract_profile(profile_val) {
                    return tools_for_profile(&profile);
                }
            }
            if let Some(ref allow_val) = tools.allow {
                let allow = extract_string_list(allow_val);
                let mut mapped = Vec::new();
                for t in &allow {
                    if is_known_openfang_tool(t) {
                        mapped.push(t.clone());
                    } else if let Some(of_name) = map_tool_name(t) {
                        mapped.push(of_name.to_string());
                    }
                }
                if !mapped.is_empty() {
                    return mapped;
                }
            }
        }
    }
    vec!["file_read".into(), "file_list".into(), "web_fetch".into()]
}

// ---------------------------------------------------------------------------
// Memory migration
// ---------------------------------------------------------------------------

fn migrate_memory_files(
    source: &Path,
    root: &OpenClawRoot,
    target: &Path,
    dry_run: bool,
    report: &mut MigrationReport,
) -> Result<(), MigrateError> {
    // Collect agent IDs from the config
    let agent_ids: Vec<String> = root
        .agents
        .as_ref()
        .map(|a| a.list.iter().map(|e| e.id.clone()).collect())
        .unwrap_or_default();

    // Check both memory layouts:
    // Layout 1: memory/<agent>/MEMORY.md
    // Layout 2: agents/<agent>/MEMORY.md (legacy)
    let mut migrated = std::collections::HashSet::new();

    let memory_dir = source.join("memory");
    if memory_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&memory_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let memory_md = path.join("MEMORY.md");
                if !memory_md.exists() {
                    continue;
                }

                let agent_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                let content = std::fs::read_to_string(&memory_md)?;
                if content.trim().is_empty() {
                    continue;
                }

                let dest_dir = target.join("agents").join(&agent_name);
                let dest_file = dest_dir.join("imported_memory.md");

                if !dry_run {
                    std::fs::create_dir_all(&dest_dir)?;
                    std::fs::write(&dest_file, &content)?;
                }

                report.imported.push(MigrateItem {
                    kind: ItemKind::Memory,
                    name: format!("{agent_name}/MEMORY.md"),
                    destination: dest_file.display().to_string(),
                });

                migrated.insert(agent_name);
            }
        }
    }

    // Layout 2: agents/<agent>/MEMORY.md (legacy layout)
    let agents_dir = source.join("agents");
    if agents_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&agents_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }

                let agent_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                if migrated.contains(&agent_name) {
                    continue;
                }

                let memory_md = path.join("MEMORY.md");
                if !memory_md.exists() {
                    continue;
                }

                let content = std::fs::read_to_string(&memory_md)?;
                if content.trim().is_empty() {
                    continue;
                }

                let dest_dir = target.join("agents").join(&agent_name);
                let dest_file = dest_dir.join("imported_memory.md");

                if !dry_run {
                    std::fs::create_dir_all(&dest_dir)?;
                    std::fs::write(&dest_file, &content)?;
                }

                report.imported.push(MigrateItem {
                    kind: ItemKind::Memory,
                    name: format!("{agent_name}/MEMORY.md"),
                    destination: dest_file.display().to_string(),
                });
            }
        }
    }

    // Warn about agents with no memory found
    for id in &agent_ids {
        if !migrated.contains(id) {
            let has_in_agents = source.join("agents").join(id).join("MEMORY.md").exists();
            if !has_in_agents {
                // not an error, just informational
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Workspace directory migration
// ---------------------------------------------------------------------------

fn migrate_workspace_dirs(
    source: &Path,
    root: &OpenClawRoot,
    target: &Path,
    dry_run: bool,
    report: &mut MigrationReport,
) -> Result<(), MigrateError> {
    // OpenClaw stores workspaces in workspaces/<agent>/
    let workspaces_dir = source.join("workspaces");
    if workspaces_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&workspaces_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }

                let agent_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                let file_count = walkdir::WalkDir::new(&path)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().is_file())
                    .count();

                if file_count == 0 {
                    continue;
                }

                let dest_dir = target.join("agents").join(&agent_name).join("workspace");

                if !dry_run {
                    copy_dir_recursive(&path, &dest_dir)?;
                }

                report.imported.push(MigrateItem {
                    kind: ItemKind::Session, // reuse for workspace
                    name: format!("{agent_name}/workspace ({file_count} files)"),
                    destination: dest_dir.display().to_string(),
                });
            }
        }
    }

    // Also check legacy agents/<agent>/workspace/ layout
    let _ = root; // used for agent IDs if needed
    let agents_dir = source.join("agents");
    if agents_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&agents_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }

                let workspace_dir = path.join("workspace");
                if !workspace_dir.exists() || !workspace_dir.is_dir() {
                    continue;
                }

                let agent_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                // Skip if already migrated from workspaces/ dir
                let dest_dir = target.join("agents").join(&agent_name).join("workspace");
                if dest_dir.exists() {
                    continue;
                }

                let file_count = walkdir::WalkDir::new(&workspace_dir)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().is_file())
                    .count();

                if file_count == 0 {
                    continue;
                }

                if !dry_run {
                    copy_dir_recursive(&workspace_dir, &dest_dir)?;
                }

                report.imported.push(MigrateItem {
                    kind: ItemKind::Session,
                    name: format!("{agent_name}/workspace ({file_count} files)"),
                    destination: dest_dir.display().to_string(),
                });
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Session migration
// ---------------------------------------------------------------------------

fn migrate_sessions(
    source: &Path,
    target: &Path,
    dry_run: bool,
    report: &mut MigrationReport,
) -> Result<(), MigrateError> {
    let sessions_dir = source.join("sessions");
    if !sessions_dir.exists() {
        return Ok(());
    }

    let dest_dir = target.join("imported_sessions");
    let mut count = 0;

    if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            // Only copy .jsonl files
            let ext = path.extension().and_then(|e| e.to_str());
            if ext != Some("jsonl") {
                continue;
            }

            let file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            if !dry_run {
                std::fs::create_dir_all(&dest_dir)?;
                std::fs::copy(&path, dest_dir.join(&file_name))?;
            }

            count += 1;
        }
    }

    if count > 0 {
        report.imported.push(MigrateItem {
            kind: ItemKind::Session,
            name: format!("{count} session files"),
            destination: dest_dir.display().to_string(),
        });
        info!("Migrated {count} session files");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Report non-migratable features
// ---------------------------------------------------------------------------

fn report_skipped_features(root: &OpenClawRoot, source: &Path, report: &mut MigrationReport) {
    // Cron jobs
    if root.cron.is_some() {
        report.skipped.push(SkippedItem {
            kind: ItemKind::Config,
            name: "cron".to_string(),
            reason: "Cron job scheduling not yet supported — use OpenFang's ScheduleMode::Periodic instead".to_string(),
        });
    }

    // Hooks
    if root.hooks.is_some() {
        report.skipped.push(SkippedItem {
            kind: ItemKind::Config,
            name: "hooks".to_string(),
            reason: "Webhook hooks not supported — use OpenFang's event system instead".to_string(),
        });
    }

    // Auth profiles
    if let Some(ref auth) = root.auth {
        if auth.profiles.is_some() {
            report.skipped.push(SkippedItem {
                kind: ItemKind::Config,
                name: "auth-profiles".to_string(),
                reason: "Auth profiles (API keys, OAuth tokens) not migrated for security — set env vars manually".to_string(),
            });
        }
    }

    // Skills entries
    if let Some(ref skills) = root.skills {
        if let Some(ref entries) = skills.entries {
            if !entries.is_empty() {
                report.skipped.push(SkippedItem {
                    kind: ItemKind::Skill,
                    name: format!("{} skill entries", entries.len()),
                    reason: "Skills must be reinstalled via `openfang skill install`".to_string(),
                });
            }
        }
    }

    // Cron state file
    if source.join("cron").join("cron-store.json").exists() {
        report.skipped.push(SkippedItem {
            kind: ItemKind::Config,
            name: "cron-store.json".to_string(),
            reason: "Cron run state not portable".to_string(),
        });
    }

    // Vector index
    if source.join("memory-search").join("index.db").exists() {
        report.skipped.push(SkippedItem {
            kind: ItemKind::Memory,
            name: "memory-search/index.db".to_string(),
            reason: "SQLite vector index not portable — OpenFang will rebuild embeddings"
                .to_string(),
        });
    }

    // Auth profiles file
    if source.join("auth-profiles.json").exists() {
        report.skipped.push(SkippedItem {
            kind: ItemKind::Config,
            name: "auth-profiles.json".to_string(),
            reason: "Credential file not migrated for security — set API keys as env vars"
                .to_string(),
        });
    }

    // Session config
    if root.session.is_some() {
        report.skipped.push(SkippedItem {
            kind: ItemKind::Config,
            name: "session".to_string(),
            reason: "Session scope config differs — OpenFang uses per-agent sessions by default"
                .to_string(),
        });
    }

    // Memory backend config
    if root.memory.is_some() {
        report.skipped.push(SkippedItem {
            kind: ItemKind::Config,
            name: "memory".to_string(),
            reason:
                "Memory backend config not migrated — OpenFang uses SQLite with vector embeddings"
                    .to_string(),
        });
    }
}

// ---------------------------------------------------------------------------
// Legacy YAML migration (backward compat)
// ---------------------------------------------------------------------------

fn migrate_from_legacy_yaml(
    source: &Path,
    target: &Path,
    dry_run: bool,
    report: &mut MigrationReport,
) -> Result<(), MigrateError> {
    // Channel parsing
    let channels = parse_legacy_channels(source, target, dry_run, report)?;

    // Config migration
    migrate_legacy_config(source, target, dry_run, channels, report)?;

    // Agent migration
    migrate_legacy_agents(source, target, dry_run, report)?;

    // Memory migration
    migrate_legacy_memory(source, target, dry_run, report)?;

    // Workspace migration
    migrate_legacy_workspaces(source, target, dry_run, report)?;

    // Skill scanning
    scan_legacy_skills(source, report);

    info!("Legacy YAML migration complete");
    Ok(())
}

fn migrate_legacy_config(
    source: &Path,
    target: &Path,
    dry_run: bool,
    channels: Option<toml::Value>,
    report: &mut MigrationReport,
) -> Result<(), MigrateError> {
    let config_path = source.join("config.yaml");
    if !config_path.exists() {
        report
            .warnings
            .push("No config.yaml found in OpenClaw workspace".to_string());
        return Ok(());
    }

    let yaml_str = std::fs::read_to_string(&config_path)?;
    let oc_config: LegacyYamlConfig = serde_yaml::from_str(&yaml_str)
        .map_err(|e| MigrateError::ConfigParse(format!("config.yaml: {e}")))?;

    let provider = map_provider(&oc_config.provider);
    let api_key_env = oc_config
        .api_key_env
        .unwrap_or_else(|| default_api_key_env(&provider));

    let of_config = OpenFangConfig {
        default_model: OpenFangModelConfig {
            provider,
            model: oc_config.model,
            api_key_env,
            base_url: oc_config.base_url,
        },
        memory: OpenFangMemorySection {
            decay_rate: oc_config
                .memory
                .as_ref()
                .and_then(|m| m.decay_rate)
                .unwrap_or(0.05),
        },
        network: OpenFangNetworkSection {
            listen_addr: "127.0.0.1:4200".to_string(),
        },
        channels,
    };

    let toml_str = toml::to_string_pretty(&of_config)?;

    let config_content = format!(
        "# OpenFang Agent OS configuration\n\
         # Migrated from OpenClaw on {}\n\n\
         {toml_str}",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
    );

    let dest = target.join("config.toml");

    if !dry_run {
        std::fs::create_dir_all(target)?;
        std::fs::write(&dest, &config_content)?;
    }

    report.imported.push(MigrateItem {
        kind: ItemKind::Config,
        name: "config.yaml".to_string(),
        destination: dest.display().to_string(),
    });

    info!("Migrated config.yaml -> config.toml");
    Ok(())
}

fn parse_legacy_channels(
    source: &Path,
    target: &Path,
    dry_run: bool,
    report: &mut MigrationReport,
) -> Result<Option<toml::Value>, MigrateError> {
    let messaging_dir = source.join("messaging");
    if !messaging_dir.exists() {
        return Ok(None);
    }

    let mut channels_table = toml::map::Map::new();
    // Note: Legacy YAML channels use env var names (bot_token_env), not raw tokens,
    // so no secrets extraction needed. target/dry_run reserved for future use.
    let _ = (target, dry_run);

    for name in &[
        "telegram",
        "discord",
        "slack",
        "whatsapp",
        "signal",
        "matrix",
        "irc",
        "mattermost",
        "feishu",
        "googlechat",
        "msteams",
        "imessage",
        "bluebubbles",
    ] {
        let yaml_path = messaging_dir.join(format!("{name}.yaml"));
        if !yaml_path.exists() {
            continue;
        }

        let yaml_str = std::fs::read_to_string(&yaml_path)?;
        let ch: LegacyYamlChannelConfig = serde_yaml::from_str(&yaml_str).unwrap_or_default();

        match *name {
            "telegram" => {
                let token_env = ch
                    .bot_token_env
                    .unwrap_or_else(|| "TELEGRAM_BOT_TOKEN".to_string());
                let mut fields: Vec<(&str, toml::Value)> =
                    vec![("bot_token_env", toml::Value::String(token_env))];
                if !ch.allowed_users.is_empty() {
                    let arr: Vec<toml::Value> = ch
                        .allowed_users
                        .iter()
                        .map(|u| toml::Value::String(u.clone()))
                        .collect();
                    fields.push(("allowed_users", toml::Value::Array(arr)));
                }
                if let Some(ref da) = ch.default_agent {
                    fields.push(("default_agent", toml::Value::String(da.clone())));
                }
                channels_table.insert(
                    "telegram".to_string(),
                    build_channel_table(fields, None, None, None),
                );
                report.imported.push(MigrateItem {
                    kind: ItemKind::Channel,
                    name: "telegram".to_string(),
                    destination: "config.toml [channels.telegram]".to_string(),
                });
            }
            "discord" => {
                let token_env = ch
                    .bot_token_env
                    .unwrap_or_else(|| "DISCORD_BOT_TOKEN".to_string());
                let mut fields: Vec<(&str, toml::Value)> =
                    vec![("bot_token_env", toml::Value::String(token_env))];
                if let Some(ref da) = ch.default_agent {
                    fields.push(("default_agent", toml::Value::String(da.clone())));
                }
                channels_table.insert(
                    "discord".to_string(),
                    build_channel_table(fields, None, None, None),
                );
                report.imported.push(MigrateItem {
                    kind: ItemKind::Channel,
                    name: "discord".to_string(),
                    destination: "config.toml [channels.discord]".to_string(),
                });
            }
            "slack" => {
                let token_env = ch
                    .bot_token_env
                    .unwrap_or_else(|| "SLACK_BOT_TOKEN".to_string());
                let mut fields: Vec<(&str, toml::Value)> =
                    vec![("bot_token_env", toml::Value::String(token_env))];
                if let Some(ref app_tok) = ch.app_token_env {
                    fields.push(("app_token_env", toml::Value::String(app_tok.clone())));
                }
                if let Some(ref da) = ch.default_agent {
                    fields.push(("default_agent", toml::Value::String(da.clone())));
                }
                channels_table.insert(
                    "slack".to_string(),
                    build_channel_table(fields, None, None, None),
                );
                report.imported.push(MigrateItem {
                    kind: ItemKind::Channel,
                    name: "slack".to_string(),
                    destination: "config.toml [channels.slack]".to_string(),
                });
            }
            "whatsapp" => {
                let token_env = ch
                    .access_token_env
                    .clone()
                    .unwrap_or_else(|| "WHATSAPP_ACCESS_TOKEN".to_string());
                let fields: Vec<(&str, toml::Value)> =
                    vec![("access_token_env", toml::Value::String(token_env))];
                channels_table.insert(
                    "whatsapp".to_string(),
                    build_channel_table(fields, None, None, None),
                );
                report.imported.push(MigrateItem {
                    kind: ItemKind::Channel,
                    name: "whatsapp".to_string(),
                    destination: "config.toml [channels.whatsapp]".to_string(),
                });
            }
            "signal" => {
                let fields: Vec<(&str, toml::Value)> = vec![(
                    "api_url",
                    toml::Value::String("http://localhost:8080".into()),
                )];
                channels_table.insert(
                    "signal".to_string(),
                    build_channel_table(fields, None, None, None),
                );
                report.imported.push(MigrateItem {
                    kind: ItemKind::Channel,
                    name: "signal".to_string(),
                    destination: "config.toml [channels.signal]".to_string(),
                });
            }
            "matrix" => {
                let token_env = ch
                    .access_token_env
                    .clone()
                    .unwrap_or_else(|| "MATRIX_ACCESS_TOKEN".to_string());
                let fields: Vec<(&str, toml::Value)> =
                    vec![("access_token_env", toml::Value::String(token_env))];
                channels_table.insert(
                    "matrix".to_string(),
                    build_channel_table(fields, None, None, None),
                );
                report.imported.push(MigrateItem {
                    kind: ItemKind::Channel,
                    name: "matrix".to_string(),
                    destination: "config.toml [channels.matrix]".to_string(),
                });
            }
            "irc" => {
                let mut fields: Vec<(&str, toml::Value)> = Vec::new();
                if let Some(ref tok) = ch.bot_token_env {
                    fields.push(("password_env", toml::Value::String(tok.clone())));
                }
                channels_table.insert(
                    "irc".to_string(),
                    build_channel_table(fields, None, None, None),
                );
                report.imported.push(MigrateItem {
                    kind: ItemKind::Channel,
                    name: "irc".to_string(),
                    destination: "config.toml [channels.irc]".to_string(),
                });
            }
            "mattermost" => {
                let token_env = ch
                    .bot_token_env
                    .unwrap_or_else(|| "MATTERMOST_TOKEN".to_string());
                let fields: Vec<(&str, toml::Value)> =
                    vec![("bot_token_env", toml::Value::String(token_env))];
                channels_table.insert(
                    "mattermost".to_string(),
                    build_channel_table(fields, None, None, None),
                );
                report.imported.push(MigrateItem {
                    kind: ItemKind::Channel,
                    name: "mattermost".to_string(),
                    destination: "config.toml [channels.mattermost]".to_string(),
                });
            }
            "feishu" => {
                let fields: Vec<(&str, toml::Value)> = vec![(
                    "app_secret_env",
                    toml::Value::String("FEISHU_APP_SECRET".into()),
                )];
                channels_table.insert(
                    "feishu".to_string(),
                    build_channel_table(fields, None, None, None),
                );
                report.imported.push(MigrateItem {
                    kind: ItemKind::Channel,
                    name: "feishu".to_string(),
                    destination: "config.toml [channels.feishu]".to_string(),
                });
            }
            "googlechat" => {
                let fields: Vec<(&str, toml::Value)> = vec![(
                    "service_account_env",
                    toml::Value::String("GOOGLE_CHAT_SA_FILE".into()),
                )];
                channels_table.insert(
                    "google_chat".to_string(),
                    build_channel_table(fields, None, None, None),
                );
                report.imported.push(MigrateItem {
                    kind: ItemKind::Channel,
                    name: "google_chat".to_string(),
                    destination: "config.toml [channels.google_chat]".to_string(),
                });
            }
            "msteams" => {
                let fields: Vec<(&str, toml::Value)> = vec![(
                    "app_password_env",
                    toml::Value::String("TEAMS_APP_PASSWORD".into()),
                )];
                channels_table.insert(
                    "teams".to_string(),
                    build_channel_table(fields, None, None, None),
                );
                report.imported.push(MigrateItem {
                    kind: ItemKind::Channel,
                    name: "teams".to_string(),
                    destination: "config.toml [channels.teams]".to_string(),
                });
            }
            "imessage" => {
                report.skipped.push(SkippedItem {
                    kind: ItemKind::Channel,
                    name: "imessage".to_string(),
                    reason: "macOS-only channel — requires manual setup on the target Mac"
                        .to_string(),
                });
            }
            "bluebubbles" => {
                report.skipped.push(SkippedItem {
                    kind: ItemKind::Channel,
                    name: "bluebubbles".to_string(),
                    reason: "No OpenFang adapter available — consider using the iMessage channel instead".to_string(),
                });
            }
            _ => {}
        }
    }

    if channels_table.is_empty() {
        Ok(None)
    } else {
        Ok(Some(toml::Value::Table(channels_table)))
    }
}

fn migrate_legacy_agents(
    source: &Path,
    target: &Path,
    dry_run: bool,
    report: &mut MigrationReport,
) -> Result<(), MigrateError> {
    let agents_dir = source.join("agents");
    if !agents_dir.exists() {
        report
            .warnings
            .push("No agents/ directory found".to_string());
        return Ok(());
    }

    let entries = std::fs::read_dir(&agents_dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let agent_yaml = path.join("agent.yaml");
        if !agent_yaml.exists() {
            continue;
        }

        let agent_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        match convert_legacy_agent(&agent_yaml, &agent_name) {
            Ok((toml_str, unmapped_tools)) => {
                let dest_dir = target.join("agents").join(&agent_name);
                let dest_file = dest_dir.join("agent.toml");

                if !dry_run {
                    std::fs::create_dir_all(&dest_dir)?;
                    std::fs::write(&dest_file, &toml_str)?;
                }

                report.imported.push(MigrateItem {
                    kind: ItemKind::Agent,
                    name: agent_name.clone(),
                    destination: dest_file.display().to_string(),
                });

                for tool in &unmapped_tools {
                    report.warnings.push(format!(
                        "Agent '{agent_name}': tool '{tool}' has no OpenFang equivalent and was skipped"
                    ));
                }

                info!("Migrated agent: {agent_name}");
            }
            Err(e) => {
                warn!("Failed to migrate agent {agent_name}: {e}");
                report.skipped.push(SkippedItem {
                    kind: ItemKind::Agent,
                    name: agent_name,
                    reason: e.to_string(),
                });
            }
        }
    }

    Ok(())
}

fn convert_legacy_agent(
    yaml_path: &Path,
    name: &str,
) -> Result<(String, Vec<String>), MigrateError> {
    let yaml_str = std::fs::read_to_string(yaml_path)?;
    let oc: LegacyYamlAgent = serde_yaml::from_str(&yaml_str)
        .map_err(|e| MigrateError::AgentParse(format!("{name}: {e}")))?;

    // Map tools
    let mut unmapped_tools = Vec::new();
    let tools: Vec<String> = if !oc.tools.is_empty() {
        let mut mapped = Vec::new();
        for t in &oc.tools {
            if is_known_openfang_tool(t) {
                mapped.push(t.clone());
            } else if let Some(of_name) = map_tool_name(t) {
                mapped.push(of_name.to_string());
            } else {
                unmapped_tools.push(t.clone());
            }
        }
        mapped
    } else if let Some(ref profile) = oc.tool_profile {
        tools_for_profile(profile)
    } else {
        vec!["file_read".into(), "file_list".into(), "web_fetch".into()]
    };

    let caps = derive_capabilities(&tools);

    let provider = oc
        .provider
        .map(|p| map_provider(&p))
        .unwrap_or_else(|| "anthropic".to_string());

    let model = oc
        .model
        .unwrap_or_else(|| "claude-sonnet-4-20250514".to_string());

    let system_prompt = oc.system_prompt.unwrap_or_else(|| {
        format!(
            "You are {}, an AI agent running on the OpenFang Agent OS. {}",
            oc.name,
            if oc.description.is_empty() {
                "You are helpful, concise, and accurate.".to_string()
            } else {
                oc.description.clone()
            }
        )
    });

    let api_key_env = oc.api_key_env.or_else(|| {
        let env = default_api_key_env(&provider);
        if env.is_empty() {
            None
        } else {
            Some(env)
        }
    });

    let mut toml_str = String::new();
    toml_str.push_str(&format!(
        "# OpenFang agent manifest\n# Migrated from OpenClaw agent '{}'\n\n",
        oc.name
    ));
    toml_str.push_str(&format!("name = \"{}\"\n", oc.name));
    toml_str.push_str("version = \"0.1.0\"\n");
    toml_str.push_str(&format!(
        "description = \"{}\"\n",
        oc.description.replace('"', "\\\"")
    ));
    toml_str.push_str("author = \"openfang\"\n");
    toml_str.push_str("module = \"builtin:chat\"\n");

    if !oc.tags.is_empty() {
        let tags_str: Vec<String> = oc.tags.iter().map(|t| format!("\"{t}\"")).collect();
        toml_str.push_str(&format!("tags = [{}]\n", tags_str.join(", ")));
    }

    toml_str.push_str("\n[model]\n");
    toml_str.push_str(&format!("provider = \"{provider}\"\n"));
    toml_str.push_str(&format!("model = \"{model}\"\n"));
    toml_str.push_str(&format!(
        "system_prompt = \"\"\"\n{system_prompt}\n\"\"\"\n"
    ));

    if let Some(ref api_key) = api_key_env {
        toml_str.push_str(&format!("api_key_env = \"{api_key}\"\n"));
    }
    if let Some(base_url) = oc.base_url {
        toml_str.push_str(&format!("base_url = \"{base_url}\"\n"));
    }

    toml_str.push_str("\n[capabilities]\n");
    let tools_str: Vec<String> = tools.iter().map(|t| format!("\"{t}\"")).collect();
    toml_str.push_str(&format!("tools = [{}]\n", tools_str.join(", ")));
    toml_str.push_str("memory_read = [\"*\"]\n");
    toml_str.push_str("memory_write = [\"self.*\"]\n");

    if !caps.network.is_empty() {
        let net_str: Vec<String> = caps.network.iter().map(|n| format!("\"{n}\"")).collect();
        toml_str.push_str(&format!("network = [{}]\n", net_str.join(", ")));
    }
    if !caps.shell.is_empty() {
        let shell_str: Vec<String> = caps.shell.iter().map(|s| format!("\"{s}\"")).collect();
        toml_str.push_str(&format!("shell = [{}]\n", shell_str.join(", ")));
    }
    if !caps.agent_message.is_empty() {
        let msg_str: Vec<String> = caps
            .agent_message
            .iter()
            .map(|m| format!("\"{m}\""))
            .collect();
        toml_str.push_str(&format!("agent_message = [{}]\n", msg_str.join(", ")));
    }
    if caps.agent_spawn {
        toml_str.push_str("agent_spawn = true\n");
    }

    Ok((toml_str, unmapped_tools))
}

fn migrate_legacy_memory(
    source: &Path,
    target: &Path,
    dry_run: bool,
    report: &mut MigrationReport,
) -> Result<(), MigrateError> {
    let agents_dir = source.join("agents");
    if !agents_dir.exists() {
        return Ok(());
    }

    let entries = std::fs::read_dir(&agents_dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let memory_md = path.join("MEMORY.md");
        if !memory_md.exists() {
            continue;
        }

        let agent_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let content = std::fs::read_to_string(&memory_md)?;
        if content.trim().is_empty() {
            continue;
        }

        let dest_dir = target.join("agents").join(&agent_name);
        let dest_file = dest_dir.join("imported_memory.md");

        if !dry_run {
            std::fs::create_dir_all(&dest_dir)?;
            std::fs::write(&dest_file, &content)?;
        }

        report.imported.push(MigrateItem {
            kind: ItemKind::Memory,
            name: format!("{agent_name}/MEMORY.md"),
            destination: dest_file.display().to_string(),
        });
    }

    Ok(())
}

fn migrate_legacy_workspaces(
    source: &Path,
    target: &Path,
    dry_run: bool,
    report: &mut MigrationReport,
) -> Result<(), MigrateError> {
    let agents_dir = source.join("agents");
    if !agents_dir.exists() {
        return Ok(());
    }

    let entries = std::fs::read_dir(&agents_dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let workspace_dir = path.join("workspace");
        if !workspace_dir.exists() || !workspace_dir.is_dir() {
            continue;
        }

        let agent_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let file_count = walkdir::WalkDir::new(&workspace_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .count();

        if file_count == 0 {
            continue;
        }

        let dest_dir = target.join("agents").join(&agent_name).join("workspace");

        if !dry_run {
            copy_dir_recursive(&workspace_dir, &dest_dir)?;
        }

        report.imported.push(MigrateItem {
            kind: ItemKind::Session,
            name: format!("{agent_name}/workspace ({file_count} files)"),
            destination: dest_dir.display().to_string(),
        });
    }

    Ok(())
}

fn scan_legacy_skills(source: &Path, report: &mut MigrationReport) {
    let skills_dir = source.join("skills");
    if !skills_dir.exists() {
        return;
    }

    let mut scan_subdir = |subdir: &Path| {
        if let Ok(entries) = std::fs::read_dir(subdir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                let has_package_json = path.join("package.json").exists();
                let has_index = path.join("index.ts").exists() || path.join("index.js").exists();

                if has_package_json && has_index {
                    report.skipped.push(SkippedItem {
                        kind: ItemKind::Skill,
                        name: name.clone(),
                        reason: "Node.js skill — run with `openfang skill install` after migration"
                            .to_string(),
                    });
                } else {
                    report.skipped.push(SkippedItem {
                        kind: ItemKind::Skill,
                        name,
                        reason: "Unknown skill format".to_string(),
                    });
                }
            }
        }
    };

    scan_subdir(&skills_dir.join("community"));
    scan_subdir(&skills_dir.join("custom"));
}

// ---------------------------------------------------------------------------
// Shared utilities
// ---------------------------------------------------------------------------

/// Recursively copy a directory.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ===== Helper: create legacy YAML workspace =====

    fn create_legacy_yaml_workspace(dir: &Path) {
        // config.yaml
        std::fs::write(
            dir.join("config.yaml"),
            "provider: anthropic\nmodel: claude-sonnet-4-20250514\napi_key_env: ANTHROPIC_API_KEY\n",
        )
        .unwrap();

        // agents/coder/agent.yaml
        let agent_dir = dir.join("agents").join("coder");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::fs::write(
            agent_dir.join("agent.yaml"),
            "name: coder\ndescription: A coding assistant\ntools:\n  - read_file\n  - write_file\n  - execute_command\ntags:\n  - coding\n  - dev\n",
        ).unwrap();

        // agents/coder/MEMORY.md
        std::fs::write(
            agent_dir.join("MEMORY.md"),
            "## Project Context\n- Working on a Rust project\n- Uses async/await\n",
        )
        .unwrap();

        // messaging/telegram.yaml
        let msg_dir = dir.join("messaging");
        std::fs::create_dir_all(&msg_dir).unwrap();
        std::fs::write(
            msg_dir.join("telegram.yaml"),
            "type: telegram\nbot_token_env: TELEGRAM_BOT_TOKEN\ndefault_agent: coder\n",
        )
        .unwrap();
    }

    // ===== Helper: create JSON5 workspace =====

    fn create_json5_workspace(dir: &Path) {
        let json5_content = r##"{
  agents: {
    defaults: {
      model: "anthropic/claude-sonnet-4-20250514",
      tools: { profile: "coding" }
    },
    list: [
      {
        id: "coder",
        name: "Coder",
        model: {
          primary: "deepseek/deepseek-chat",
          fallbacks: ["groq/llama-3.3-70b-versatile", "anthropic/claude-haiku-4-5-20251001"]
        },
        tools: { allow: ["Read", "Write", "Bash", "WebSearch"] },
        identity: "You are an expert software engineer."
      },
      {
        id: "researcher",
        model: "google/gemini-2.5-flash",
        tools: { profile: "research" }
      }
    ]
  },
  channels: {
    telegram: {
      botToken: "123:ABC",
      allowFrom: ["user1", "user2"],
      groupPolicy: "open",
      dmPolicy: "allowlist"
    },
    discord: {
      token: "discord-token-here",
      enabled: true,
      dmPolicy: "open"
    },
    slack: {
      botToken: "xoxb-slack",
      appToken: "xapp-slack"
    },
    whatsapp: {
      dmPolicy: "open",
      allowFrom: ["phone1"],
      groupPolicy: "disabled"
    },
    signal: {
      httpHost: "signal-api.local",
      httpPort: 9090,
      account: "+15551234567"
    },
    matrix: {
      homeserver: "https://matrix.example.com",
      userId: "@bot:example.com",
      accessToken: "syt_matrix_token_xyz"
    },
    irc: {
      host: "irc.libera.chat",
      port: 6697,
      tls: true,
      nick: "openfang-bot",
      password: "irc-secret-pw",
      channels: ["#dev", "#general"]
    },
    mattermost: {
      botToken: "mm-token-abc",
      baseUrl: "https://mm.example.com"
    },
    feishu: {
      appId: "cli_feishu123",
      appSecret: "feishu-secret-xyz",
      domain: "example.feishu.cn"
    },
    googlechat: {
      webhookPath: "/webhook/gchat",
      dmPolicy: "open"
    },
    msteams: {
      appId: "teams-app-id-123",
      appPassword: "teams-pw-secret",
      tenantId: "tenant-uuid"
    },
    imessage: {
      cliPath: "/usr/local/bin/imessage-cli"
    },
    bluebubbles: {
      serverUrl: "http://localhost:1234",
      password: "bb-pw"
    }
  },
  cron: { enabled: true },
  hooks: { enabled: true, mappings: [] },
  skills: {
    entries: {
      "web-scraper": {},
      "pdf-reader": {}
    }
  },
  auth: {
    profiles: { "default": { apiKey: "sk-xxx" } }
  },
  memory: { backend: "builtin" },
  session: { scope: "per-sender" }
}"##;

        std::fs::write(dir.join("openclaw.json"), json5_content).unwrap();

        // Physical memory dirs
        let mem_coder = dir.join("memory").join("coder");
        std::fs::create_dir_all(&mem_coder).unwrap();
        std::fs::write(
            mem_coder.join("MEMORY.md"),
            "## Coder Memory\n- Prefers Rust\n",
        )
        .unwrap();

        let mem_researcher = dir.join("memory").join("researcher");
        std::fs::create_dir_all(&mem_researcher).unwrap();
        std::fs::write(
            mem_researcher.join("MEMORY.md"),
            "## Researcher Memory\n- Uses academic sources\n",
        )
        .unwrap();

        // Sessions
        let sessions_dir = dir.join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();
        std::fs::write(
            sessions_dir.join("main.jsonl"),
            "{\"role\":\"user\",\"content\":\"hello\"}\n",
        )
        .unwrap();
        std::fs::write(
            sessions_dir.join("agent_coder_main.jsonl"),
            "{\"role\":\"user\",\"content\":\"write code\"}\n",
        )
        .unwrap();

        // Workspaces
        let ws_coder = dir.join("workspaces").join("coder");
        std::fs::create_dir_all(&ws_coder).unwrap();
        std::fs::write(ws_coder.join("main.rs"), "fn main() {}").unwrap();
    }

    // ================================================================
    // JSON5 tests (new)
    // ================================================================

    #[test]
    fn test_json5_full_migration() {
        let source = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();

        create_json5_workspace(source.path());

        let options = MigrateOptions {
            source: crate::MigrateSource::OpenClaw,
            source_dir: source.path().to_path_buf(),
            target_dir: target.path().to_path_buf(),
            dry_run: false,
        };

        let report = migrate(&options).unwrap();

        // Config imported
        assert!(report.imported.iter().any(|i| i.kind == ItemKind::Config));
        assert!(target.path().join("config.toml").exists());

        // Agents imported
        let agent_items: Vec<_> = report
            .imported
            .iter()
            .filter(|i| i.kind == ItemKind::Agent)
            .collect();
        assert_eq!(agent_items.len(), 2);
        assert!(target.path().join("agents/coder/agent.toml").exists());
        assert!(target.path().join("agents/researcher/agent.toml").exists());

        // Channels imported (11 supported channels from fixture)
        let channel_items: Vec<_> = report
            .imported
            .iter()
            .filter(|i| i.kind == ItemKind::Channel)
            .collect();
        assert_eq!(channel_items.len(), 11); // 13 - imessage - bluebubbles

        let config_toml = std::fs::read_to_string(target.path().join("config.toml")).unwrap();
        assert!(config_toml.contains("[channels.telegram]"));
        assert!(config_toml.contains("[channels.discord]"));
        assert!(config_toml.contains("[channels.slack]"));
        assert!(config_toml.contains("[channels.whatsapp]"));
        assert!(config_toml.contains("[channels.signal]"));
        assert!(config_toml.contains("[channels.matrix]"));
        assert!(config_toml.contains("[channels.irc]"));
        assert!(config_toml.contains("[channels.mattermost]"));
        assert!(config_toml.contains("[channels.feishu]"));
        assert!(config_toml.contains("[channels.teams]"));
        assert!(
            config_toml.contains("[channels.google_chat]"),
            "missing google_chat in config: {config_toml}"
        );

        // Secrets extracted
        let secret_items: Vec<_> = report
            .imported
            .iter()
            .filter(|i| i.kind == ItemKind::Secret)
            .collect();
        assert!(
            secret_items.len() >= 7,
            "expected >=7 secrets, got {}",
            secret_items.len()
        );
        assert!(target.path().join("secrets.env").exists());

        let secrets = std::fs::read_to_string(target.path().join("secrets.env")).unwrap();
        assert!(secrets.contains("TELEGRAM_BOT_TOKEN=123:ABC"));
        assert!(secrets.contains("DISCORD_BOT_TOKEN=discord-token-here"));
        assert!(secrets.contains("SLACK_BOT_TOKEN=xoxb-slack"));
        assert!(secrets.contains("MATRIX_ACCESS_TOKEN=syt_matrix_token_xyz"));
        assert!(secrets.contains("IRC_PASSWORD=irc-secret-pw"));
        assert!(secrets.contains("MATTERMOST_TOKEN=mm-token-abc"));
        assert!(secrets.contains("FEISHU_APP_SECRET=feishu-secret-xyz"));
        assert!(secrets.contains("TEAMS_APP_PASSWORD=teams-pw-secret"));

        // NO raw tokens in config.toml
        assert!(
            !config_toml.contains("123:ABC"),
            "raw token leaked into config.toml"
        );
        assert!(
            !config_toml.contains("discord-token-here"),
            "raw token leaked into config.toml"
        );
        assert!(
            !config_toml.contains("xoxb-slack"),
            "raw token leaked into config.toml"
        );
        assert!(
            !config_toml.contains("syt_matrix_token_xyz"),
            "raw token leaked into config.toml"
        );

        // Skipped channels reported
        assert!(report.skipped.iter().any(|s| s.name == "imessage"));
        assert!(report.skipped.iter().any(|s| s.name == "bluebubbles"));

        // Memory imported
        assert!(report.imported.iter().any(|i| i.kind == ItemKind::Memory));
        assert!(target
            .path()
            .join("agents/coder/imported_memory.md")
            .exists());
        assert!(target
            .path()
            .join("agents/researcher/imported_memory.md")
            .exists());

        // Sessions imported
        assert!(report
            .imported
            .iter()
            .any(|i| i.kind == ItemKind::Session && i.name.contains("session")));
        assert!(target.path().join("imported_sessions/main.jsonl").exists());

        // Workspace imported
        assert!(report
            .imported
            .iter()
            .any(|i| i.kind == ItemKind::Session && i.name.contains("workspace")));

        // Skipped features reported
        assert!(report.skipped.iter().any(|s| s.name == "cron"));
        assert!(report.skipped.iter().any(|s| s.name == "hooks"));
        assert!(report.skipped.iter().any(|s| s.name == "auth-profiles"));
        assert!(report.skipped.iter().any(|s| s.name.contains("skill")));

        // Report file
        assert!(target.path().join("migration_report.md").exists());
    }

    #[test]
    fn test_json5_agent_model_parsing() {
        // Simple model ref
        let (p, m) = split_model_ref("anthropic/claude-sonnet-4-20250514");
        assert_eq!(p, "anthropic");
        assert_eq!(m, "claude-sonnet-4-20250514");

        // Provider mapping
        let (p, m) = split_model_ref("google/gemini-2.5-flash");
        assert_eq!(p, "google");
        assert_eq!(m, "gemini-2.5-flash");

        // No slash fallback
        let (p, m) = split_model_ref("claude-sonnet-4-20250514");
        assert_eq!(p, "anthropic");
        assert_eq!(m, "claude-sonnet-4-20250514");

        // Detailed model
        let json_str =
            r#"{ "primary": "deepseek/deepseek-chat", "fallbacks": ["groq/llama-3.3-70b"] }"#;
        let model: OpenClawAgentModel = serde_json::from_str(json_str).unwrap();
        match model {
            OpenClawAgentModel::Detailed(d) => {
                assert_eq!(d.primary.unwrap(), "deepseek/deepseek-chat");
                assert_eq!(d.fallbacks.len(), 1);
            }
            _ => panic!("Expected Detailed variant"),
        }

        // Simple model (string)
        let json_str = r#""anthropic/claude-sonnet-4-20250514""#;
        let model: OpenClawAgentModel = serde_json::from_str(json_str).unwrap();
        match model {
            OpenClawAgentModel::Simple(s) => {
                assert_eq!(s, "anthropic/claude-sonnet-4-20250514");
            }
            _ => panic!("Expected Simple variant"),
        }
    }

    #[test]
    fn test_json5_channel_extraction() {
        let target = TempDir::new().unwrap();
        let json5_content = r#"{
  channels: {
    telegram: { botToken: "123", allowFrom: ["alice"], enabled: true },
    discord: { token: "abc", enabled: true },
    slack: { botToken: "xoxb", appToken: "xapp" }
  }
}"#;
        let root: OpenClawRoot = json5::from_str(json5_content).unwrap();
        let mut report = MigrationReport::default();

        let channels = migrate_channels_from_json(&root, target.path(), false, &mut report);
        assert!(channels.is_some());
        let ch = channels.unwrap();
        let ch_table = ch.as_table().unwrap();
        assert!(ch_table.contains_key("telegram"));
        assert!(ch_table.contains_key("discord"));
        assert!(ch_table.contains_key("slack"));

        // Check telegram has allowed_users and bot_token_env
        let tg = ch_table["telegram"].as_table().unwrap();
        assert_eq!(tg["bot_token_env"].as_str().unwrap(), "TELEGRAM_BOT_TOKEN");
        let users = tg["allowed_users"].as_array().unwrap();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].as_str().unwrap(), "alice");

        // 3 channel imports
        assert_eq!(
            report
                .imported
                .iter()
                .filter(|i| i.kind == ItemKind::Channel)
                .count(),
            3
        );

        // 4 secrets extracted (telegram + discord + slack bot + slack app)
        assert_eq!(
            report
                .imported
                .iter()
                .filter(|i| i.kind == ItemKind::Secret)
                .count(),
            4
        );

        // Secrets file written
        let secrets = std::fs::read_to_string(target.path().join("secrets.env")).unwrap();
        assert!(secrets.contains("TELEGRAM_BOT_TOKEN=123"));
        assert!(secrets.contains("DISCORD_BOT_TOKEN=abc"));
        assert!(secrets.contains("SLACK_BOT_TOKEN=xoxb"));
    }

    #[test]
    fn test_json5_fallback_models() {
        let source = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();

        create_json5_workspace(source.path());

        let options = MigrateOptions {
            source: crate::MigrateSource::OpenClaw,
            source_dir: source.path().to_path_buf(),
            target_dir: target.path().to_path_buf(),
            dry_run: false,
        };

        migrate(&options).unwrap();

        let coder_toml =
            std::fs::read_to_string(target.path().join("agents/coder/agent.toml")).unwrap();

        // Primary model should be deepseek
        assert!(coder_toml.contains("provider = \"deepseek\""));
        assert!(coder_toml.contains("model = \"deepseek-chat\""));

        // Should have fallback models
        assert!(coder_toml.contains("[[fallback_models]]"));
        assert!(coder_toml.contains("provider = \"groq\""));
        assert!(coder_toml.contains("model = \"llama-3.3-70b-versatile\""));
        assert!(coder_toml.contains("provider = \"anthropic\""));
        assert!(coder_toml.contains("model = \"claude-haiku-4-5-20251001\""));
    }

    #[test]
    fn test_json5_tool_profile_resolution() {
        let source = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();

        create_json5_workspace(source.path());

        let options = MigrateOptions {
            source: crate::MigrateSource::OpenClaw,
            source_dir: source.path().to_path_buf(),
            target_dir: target.path().to_path_buf(),
            dry_run: false,
        };

        migrate(&options).unwrap();

        // researcher uses profile = "research", should get research tools
        let researcher_toml =
            std::fs::read_to_string(target.path().join("agents/researcher/agent.toml")).unwrap();
        assert!(researcher_toml.contains("web_fetch"));
        assert!(researcher_toml.contains("web_search"));
        assert!(researcher_toml.contains("profile = \"research\""));
    }

    #[test]
    fn test_json5_legacy_yaml_fallback() {
        let source = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();

        create_legacy_yaml_workspace(source.path());

        let options = MigrateOptions {
            source: crate::MigrateSource::OpenClaw,
            source_dir: source.path().to_path_buf(),
            target_dir: target.path().to_path_buf(),
            dry_run: false,
        };

        let report = migrate(&options).unwrap();

        // Should still work with YAML fallback
        assert!(report.imported.iter().any(|i| i.kind == ItemKind::Config));
        assert!(report.imported.iter().any(|i| i.kind == ItemKind::Agent));
        assert!(target.path().join("config.toml").exists());
        assert!(target.path().join("agents/coder/agent.toml").exists());
    }

    #[test]
    fn test_json5_detect_home() {
        let dir = TempDir::new().unwrap();

        // No config file = should not detect
        assert!(find_config_file(dir.path()).is_none());

        // With openclaw.json
        std::fs::write(dir.path().join("openclaw.json"), "{}").unwrap();
        let found = find_config_file(dir.path());
        assert!(found.is_some());
        assert!(found.unwrap().ends_with("openclaw.json"));

        // Legacy clawdbot.json
        let dir2 = TempDir::new().unwrap();
        std::fs::write(dir2.path().join("clawdbot.json"), "{}").unwrap();
        let found = find_config_file(dir2.path());
        assert!(found.is_some());
        assert!(found.unwrap().ends_with("clawdbot.json"));

        // config.yaml (legacy)
        let dir3 = TempDir::new().unwrap();
        std::fs::write(dir3.path().join("config.yaml"), "provider: anthropic\n").unwrap();
        let found = find_config_file(dir3.path());
        assert!(found.is_some());
        assert!(found.unwrap().ends_with("config.yaml"));
    }

    #[test]
    fn test_json5_session_migration() {
        let source = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();

        create_json5_workspace(source.path());

        let options = MigrateOptions {
            source: crate::MigrateSource::OpenClaw,
            source_dir: source.path().to_path_buf(),
            target_dir: target.path().to_path_buf(),
            dry_run: false,
        };

        migrate(&options).unwrap();

        let imported_dir = target.path().join("imported_sessions");
        assert!(imported_dir.exists());
        assert!(imported_dir.join("main.jsonl").exists());
        assert!(imported_dir.join("agent_coder_main.jsonl").exists());

        // Verify content preserved
        let content = std::fs::read_to_string(imported_dir.join("main.jsonl")).unwrap();
        assert!(content.contains("hello"));
    }

    #[test]
    fn test_json5_memory_both_layouts() {
        let source = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();

        // Create JSON5 config with agents
        let json5_content = r#"{
  agents: {
    list: [
      { id: "agent1" },
      { id: "agent2" }
    ]
  }
}"#;
        std::fs::write(source.path().join("openclaw.json"), json5_content).unwrap();

        // Layout 1: memory/<agent>/MEMORY.md
        let mem1 = source.path().join("memory").join("agent1");
        std::fs::create_dir_all(&mem1).unwrap();
        std::fs::write(mem1.join("MEMORY.md"), "Memory from layout 1").unwrap();

        // Layout 2: agents/<agent>/MEMORY.md (legacy)
        let mem2 = source.path().join("agents").join("agent2");
        std::fs::create_dir_all(&mem2).unwrap();
        std::fs::write(mem2.join("MEMORY.md"), "Memory from layout 2").unwrap();

        let options = MigrateOptions {
            source: crate::MigrateSource::OpenClaw,
            source_dir: source.path().to_path_buf(),
            target_dir: target.path().to_path_buf(),
            dry_run: false,
        };

        let report = migrate(&options).unwrap();

        let memory_items: Vec<_> = report
            .imported
            .iter()
            .filter(|i| i.kind == ItemKind::Memory)
            .collect();
        assert_eq!(memory_items.len(), 2);

        assert!(target
            .path()
            .join("agents/agent1/imported_memory.md")
            .exists());
        assert!(target
            .path()
            .join("agents/agent2/imported_memory.md")
            .exists());

        let c1 = std::fs::read_to_string(target.path().join("agents/agent1/imported_memory.md"))
            .unwrap();
        assert!(c1.contains("layout 1"));

        let c2 = std::fs::read_to_string(target.path().join("agents/agent2/imported_memory.md"))
            .unwrap();
        assert!(c2.contains("layout 2"));
    }

    #[test]
    fn test_json5_skipped_features() {
        let source = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();

        let json5_content = r#"{
  cron: { enabled: true },
  hooks: { enabled: true },
  auth: { profiles: { "default": {} } },
  skills: { entries: { "a": {}, "b": {} } },
  memory: { backend: "builtin" },
  session: { scope: "per-sender" }
}"#;
        std::fs::write(source.path().join("openclaw.json"), json5_content).unwrap();

        // Physical files that get skipped
        let cron_dir = source.path().join("cron");
        std::fs::create_dir_all(&cron_dir).unwrap();
        std::fs::write(cron_dir.join("cron-store.json"), "{}").unwrap();

        let mem_search = source.path().join("memory-search");
        std::fs::create_dir_all(&mem_search).unwrap();
        std::fs::write(mem_search.join("index.db"), "sqlite").unwrap();

        std::fs::write(source.path().join("auth-profiles.json"), "{}").unwrap();

        let options = MigrateOptions {
            source: crate::MigrateSource::OpenClaw,
            source_dir: source.path().to_path_buf(),
            target_dir: target.path().to_path_buf(),
            dry_run: false,
        };

        let report = migrate(&options).unwrap();

        // All should be in skipped
        assert!(report.skipped.iter().any(|s| s.name == "cron"));
        assert!(report.skipped.iter().any(|s| s.name == "hooks"));
        assert!(report.skipped.iter().any(|s| s.name == "auth-profiles"));
        assert!(report.skipped.iter().any(|s| s.name.contains("skill")));
        assert!(report.skipped.iter().any(|s| s.name == "cron-store.json"));
        assert!(report
            .skipped
            .iter()
            .any(|s| s.name.contains("memory-search")));
        assert!(report
            .skipped
            .iter()
            .any(|s| s.name == "auth-profiles.json"));
        assert!(report.skipped.iter().any(|s| s.name == "session"));
        assert!(report.skipped.iter().any(|s| s.name == "memory"));
    }

    #[test]
    fn test_json5_dry_run() {
        let source = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();

        create_json5_workspace(source.path());

        let options = MigrateOptions {
            source: crate::MigrateSource::OpenClaw,
            source_dir: source.path().to_path_buf(),
            target_dir: target.path().to_path_buf(),
            dry_run: true,
        };

        let report = migrate(&options).unwrap();
        assert!(report.dry_run);
        assert!(!report.imported.is_empty());

        // No files created
        assert!(!target.path().join("config.toml").exists());
        assert!(!target.path().join("agents").exists());
        assert!(!target.path().join("imported_sessions").exists());
    }

    #[test]
    fn test_json5_empty_config() {
        let source = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();

        std::fs::write(source.path().join("openclaw.json"), "{}").unwrap();

        let options = MigrateOptions {
            source: crate::MigrateSource::OpenClaw,
            source_dir: source.path().to_path_buf(),
            target_dir: target.path().to_path_buf(),
            dry_run: false,
        };

        let report = migrate(&options).unwrap();

        // Should still produce a config
        assert!(report.imported.iter().any(|i| i.kind == ItemKind::Config));
        assert!(target.path().join("config.toml").exists());

        // No agents should be an info, not crash
        assert!(report.warnings.iter().any(|w| w.contains("No agents")));
    }

    #[test]
    fn test_model_ref_split() {
        let (p, m) = split_model_ref("anthropic/claude-sonnet-4-20250514");
        assert_eq!(p, "anthropic");
        assert_eq!(m, "claude-sonnet-4-20250514");

        let (p, m) = split_model_ref("deepseek/deepseek-chat");
        assert_eq!(p, "deepseek");
        assert_eq!(m, "deepseek-chat");

        let (p, m) = split_model_ref("google/gemini-2.5-flash");
        assert_eq!(p, "google");
        assert_eq!(m, "gemini-2.5-flash");

        let (p, m) = split_model_ref("groq/llama-3.3-70b-versatile");
        assert_eq!(p, "groq");
        assert_eq!(m, "llama-3.3-70b-versatile");

        // No slash
        let (p, m) = split_model_ref("some-model");
        assert_eq!(p, "anthropic");
        assert_eq!(m, "some-model");

        // Empty
        let (p, m) = split_model_ref("");
        assert_eq!(p, "anthropic");
        assert_eq!(m, "");
    }

    #[test]
    fn test_json5_unknown_provider_passthrough() {
        let source = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();

        let json5_content = r#"{
  agents: {
    list: [
      { id: "test-agent", model: "mycompany/custom-llm-v3" }
    ]
  }
}"#;
        std::fs::write(source.path().join("openclaw.json"), json5_content).unwrap();

        let options = MigrateOptions {
            source: crate::MigrateSource::OpenClaw,
            source_dir: source.path().to_path_buf(),
            target_dir: target.path().to_path_buf(),
            dry_run: false,
        };

        let report = migrate(&options).unwrap();
        assert!(report.imported.iter().any(|i| i.kind == ItemKind::Agent));

        let agent_toml =
            std::fs::read_to_string(target.path().join("agents/test-agent/agent.toml")).unwrap();
        assert!(agent_toml.contains("provider = \"mycompany\""));
        assert!(agent_toml.contains("model = \"custom-llm-v3\""));
        assert!(agent_toml.contains("api_key_env = \"MYCOMPANY_API_KEY\""));
    }

    // ================================================================
    // Existing tests (kept — now test YAML legacy path + shared utils)
    // ================================================================

    #[test]
    fn test_full_migration() {
        let source = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();

        create_legacy_yaml_workspace(source.path());

        let options = MigrateOptions {
            source: crate::MigrateSource::OpenClaw,
            source_dir: source.path().to_path_buf(),
            target_dir: target.path().to_path_buf(),
            dry_run: false,
        };

        let report = migrate(&options).unwrap();

        assert!(!report.imported.is_empty());
        assert!(report.imported.iter().any(|i| i.kind == ItemKind::Config));
        assert!(report.imported.iter().any(|i| i.kind == ItemKind::Agent));
        assert!(report.imported.iter().any(|i| i.kind == ItemKind::Memory));
        assert!(report.imported.iter().any(|i| i.kind == ItemKind::Channel));

        assert!(target.path().join("config.toml").exists());
        assert!(target.path().join("agents/coder/agent.toml").exists());
        assert!(target
            .path()
            .join("agents/coder/imported_memory.md")
            .exists());

        let agent_toml =
            std::fs::read_to_string(target.path().join("agents/coder/agent.toml")).unwrap();
        assert!(
            agent_toml.contains("shell = [\"*\"]"),
            "shell_exec should derive shell capability"
        );
        assert!(agent_toml.contains("file_read"));
        assert!(agent_toml.contains("file_write"));
        assert!(agent_toml.contains("shell_exec"));

        let config_toml = std::fs::read_to_string(target.path().join("config.toml")).unwrap();
        assert!(config_toml.contains("[channels.telegram]"));
        assert!(!target.path().join("channels_import.toml").exists());

        assert!(target.path().join("migration_report.md").exists());
    }

    #[test]
    fn test_dry_run() {
        let source = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();

        create_legacy_yaml_workspace(source.path());

        let options = MigrateOptions {
            source: crate::MigrateSource::OpenClaw,
            source_dir: source.path().to_path_buf(),
            target_dir: target.path().to_path_buf(),
            dry_run: true,
        };

        let report = migrate(&options).unwrap();
        assert!(report.dry_run);
        assert!(!report.imported.is_empty());

        assert!(!target.path().join("config.toml").exists());
    }

    #[test]
    fn test_source_not_found() {
        let options = MigrateOptions {
            source: crate::MigrateSource::OpenClaw,
            source_dir: "/nonexistent/path".into(),
            target_dir: std::env::temp_dir().join("test_migrate_not_found"),
            dry_run: false,
        };

        let result = migrate(&options);
        assert!(result.is_err());
    }

    #[test]
    fn test_tool_mapping() {
        assert_eq!(map_tool_name("read_file"), Some("file_read"));
        assert_eq!(map_tool_name("write_file"), Some("file_write"));
        assert_eq!(map_tool_name("execute_command"), Some("shell_exec"));
        assert_eq!(map_tool_name("fetch_url"), Some("web_fetch"));
        assert_eq!(map_tool_name("memory_search"), Some("memory_recall"));
        assert_eq!(map_tool_name("unknown_tool"), None);
        // New Claude-style mappings
        assert_eq!(map_tool_name("Read"), Some("file_read"));
        assert_eq!(map_tool_name("Write"), Some("file_write"));
        assert_eq!(map_tool_name("Bash"), Some("shell_exec"));
        assert_eq!(map_tool_name("Glob"), Some("file_list"));
        assert_eq!(map_tool_name("Grep"), Some("file_list"));
        assert_eq!(map_tool_name("WebSearch"), Some("web_search"));
        assert_eq!(map_tool_name("WebFetch"), Some("web_fetch"));
        assert_eq!(map_tool_name("sessions_send"), Some("agent_send"));
        assert_eq!(map_tool_name("sessions_spawn"), Some("agent_send"));
    }

    #[test]
    fn test_provider_mapping() {
        assert_eq!(map_provider("anthropic"), "anthropic");
        assert_eq!(map_provider("claude"), "anthropic");
        assert_eq!(map_provider("openai"), "openai");
        assert_eq!(map_provider("gpt"), "openai");
        assert_eq!(map_provider("groq"), "groq");
        assert_eq!(map_provider("custom"), "custom");
        assert_eq!(map_provider("google"), "google");
        assert_eq!(map_provider("gemini"), "google");
        assert_eq!(map_provider("xai"), "xai");
        assert_eq!(map_provider("grok"), "xai");
    }

    #[test]
    fn test_tools_for_profile() {
        let minimal = tools_for_profile("minimal");
        assert_eq!(minimal.len(), 2);
        assert!(minimal.contains(&"file_read".to_string()));

        let coding = tools_for_profile("coding");
        assert!(coding.contains(&"shell_exec".to_string()));

        let full = tools_for_profile("full");
        assert!(full.contains(&"*".to_string()));

        let automation = tools_for_profile("automation");
        assert!(automation.len() >= 10);
        assert!(automation.contains(&"shell_exec".to_string()));
        assert!(automation.contains(&"web_fetch".to_string()));
    }

    #[test]
    fn test_convert_agent() {
        let dir = TempDir::new().unwrap();
        let yaml_path = dir.path().join("agent.yaml");
        std::fs::write(
            &yaml_path,
            "name: test-agent\ndescription: Test\ntools:\n  - read_file\n  - web_search\n",
        )
        .unwrap();

        let (toml_str, unmapped) = convert_legacy_agent(&yaml_path, "test-agent").unwrap();
        assert!(toml_str.contains("name = \"test-agent\""));
        assert!(toml_str.contains("file_read"));
        assert!(toml_str.contains("web_search"));
        assert!(
            toml_str.contains("network = [\"*\"]"),
            "web_search should derive network capability"
        );
        assert!(unmapped.is_empty());
    }

    #[test]
    fn test_capability_derivation() {
        let tools = vec!["shell_exec".into(), "web_fetch".into(), "agent_send".into()];
        let caps = derive_capabilities(&tools);
        assert_eq!(caps.shell, vec!["*".to_string()]);
        assert_eq!(caps.network, vec!["*".to_string()]);
        assert_eq!(caps.agent_message, vec!["*".to_string()]);
        assert!(caps.agent_spawn);
    }

    #[test]
    fn test_unmapped_tools_reported() {
        let dir = TempDir::new().unwrap();
        let yaml_path = dir.path().join("agent.yaml");
        std::fs::write(
            &yaml_path,
            "name: test\ntools:\n  - read_file\n  - some_custom_tool\n  - another_unknown\n",
        )
        .unwrap();

        let (toml_str, unmapped) = convert_legacy_agent(&yaml_path, "test").unwrap();
        assert!(toml_str.contains("file_read"));
        assert!(!toml_str.contains("some_custom_tool"));
        assert_eq!(unmapped.len(), 2);
        assert!(unmapped.contains(&"some_custom_tool".to_string()));
        assert!(unmapped.contains(&"another_unknown".to_string()));
    }

    #[test]
    fn test_scan_workspace() {
        let source = TempDir::new().unwrap();
        create_legacy_yaml_workspace(source.path());

        let result = scan_openclaw_workspace(source.path());
        assert!(result.has_config);
        assert_eq!(result.agents.len(), 1);
        assert_eq!(result.agents[0].name, "coder");
        assert!(result.agents[0].has_memory);
        assert_eq!(result.channels.len(), 1);
        assert!(result.channels.contains(&"telegram".to_string()));
    }

    #[test]
    fn test_scan_json5_workspace() {
        let source = TempDir::new().unwrap();
        create_json5_workspace(source.path());

        let result = scan_openclaw_workspace(source.path());
        assert!(result.has_config);
        assert_eq!(result.agents.len(), 2);
        assert!(result.agents.iter().any(|a| a.name == "Coder"));
        assert!(result.agents.iter().any(|a| a.name == "researcher"));
        // All 13 channels detected by scanner
        assert_eq!(
            result.channels.len(),
            13,
            "expected 13 channels, got {:?}",
            result.channels
        );
        assert!(result.channels.contains(&"telegram".to_string()));
        assert!(result.channels.contains(&"discord".to_string()));
        assert!(result.channels.contains(&"slack".to_string()));
        assert!(result.channels.contains(&"whatsapp".to_string()));
        assert!(result.channels.contains(&"signal".to_string()));
        assert!(result.channels.contains(&"matrix".to_string()));
        assert!(result.channels.contains(&"irc".to_string()));
        assert!(result.channels.contains(&"mattermost".to_string()));
        assert!(result.channels.contains(&"feishu".to_string()));
        assert!(result.channels.contains(&"teams".to_string()));
        assert!(result.channels.contains(&"imessage".to_string()));
        assert!(result.channels.contains(&"bluebubbles".to_string()));
        assert!(result.has_memory);
    }

    #[test]
    fn test_is_known_openfang_tool() {
        assert!(is_known_openfang_tool("file_read"));
        assert!(is_known_openfang_tool("shell_exec"));
        assert!(is_known_openfang_tool("web_fetch"));
        assert!(!is_known_openfang_tool("Read"));
        assert!(!is_known_openfang_tool("unknown"));
    }

    #[test]
    fn test_secrets_migration() {
        let source = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();

        create_json5_workspace(source.path());

        let options = MigrateOptions {
            source: crate::MigrateSource::OpenClaw,
            source_dir: source.path().to_path_buf(),
            target_dir: target.path().to_path_buf(),
            dry_run: false,
        };

        let report = migrate(&options).unwrap();

        // secrets.env must exist and contain all extracted tokens
        let secrets_path = target.path().join("secrets.env");
        assert!(secrets_path.exists(), "secrets.env not created");
        let secrets = std::fs::read_to_string(&secrets_path).unwrap();

        // Verify each token is in secrets.env
        assert!(secrets.contains("TELEGRAM_BOT_TOKEN=123:ABC"));
        assert!(secrets.contains("DISCORD_BOT_TOKEN=discord-token-here"));
        assert!(secrets.contains("SLACK_BOT_TOKEN=xoxb-slack"));
        assert!(secrets.contains("SLACK_APP_TOKEN=xapp-slack"));
        assert!(secrets.contains("MATRIX_ACCESS_TOKEN=syt_matrix_token_xyz"));
        assert!(secrets.contains("IRC_PASSWORD=irc-secret-pw"));
        assert!(secrets.contains("MATTERMOST_TOKEN=mm-token-abc"));
        assert!(secrets.contains("FEISHU_APP_SECRET=feishu-secret-xyz"));
        assert!(secrets.contains("TEAMS_APP_PASSWORD=teams-pw-secret"));

        // config.toml must NOT contain any raw secrets
        let config_toml = std::fs::read_to_string(target.path().join("config.toml")).unwrap();
        for secret in &[
            "123:ABC",
            "discord-token-here",
            "xoxb-slack",
            "xapp-slack",
            "syt_matrix_token_xyz",
            "irc-secret-pw",
            "mm-token-abc",
            "feishu-secret-xyz",
            "teams-pw-secret",
        ] {
            assert!(
                !config_toml.contains(secret),
                "Raw secret '{secret}' leaked into config.toml"
            );
        }

        // Secret items in report
        let secret_count = report
            .imported
            .iter()
            .filter(|i| i.kind == ItemKind::Secret)
            .count();
        assert!(
            secret_count >= 9,
            "expected >=9 Secret items, got {secret_count}"
        );
    }

    #[test]
    fn test_policy_migration() {
        let target = TempDir::new().unwrap();
        let json5_content = r#"{
  channels: {
    telegram: {
      botToken: "tok",
      dmPolicy: "allowlist",
      groupPolicy: "open",
      allowFrom: ["alice", "bob"]
    },
    discord: {
      token: "tok2",
      dmPolicy: "disabled"
    }
  }
}"#;
        let root: OpenClawRoot = json5::from_str(json5_content).unwrap();
        let mut report = MigrationReport::default();

        let channels = migrate_channels_from_json(&root, target.path(), false, &mut report);
        assert!(channels.is_some());
        let ch_table = channels.unwrap();
        let table = ch_table.as_table().unwrap();

        // Telegram should have overrides with mapped policies
        let tg = table["telegram"].as_table().unwrap();
        let overrides = tg["overrides"].as_table().unwrap();
        assert_eq!(overrides["dm_policy"].as_str().unwrap(), "allowed_only");
        assert_eq!(overrides["group_policy"].as_str().unwrap(), "respond");
        let users = overrides["allowed_users"].as_array().unwrap();
        assert_eq!(users.len(), 2);

        // Discord should have overrides with mapped dm_policy
        let dc = table["discord"].as_table().unwrap();
        let dc_overrides = dc["overrides"].as_table().unwrap();
        assert_eq!(dc_overrides["dm_policy"].as_str().unwrap(), "ignore");
    }

    #[test]
    fn test_idempotent_migration() {
        let source = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();

        create_json5_workspace(source.path());

        let options = MigrateOptions {
            source: crate::MigrateSource::OpenClaw,
            source_dir: source.path().to_path_buf(),
            target_dir: target.path().to_path_buf(),
            dry_run: false,
        };

        // Run migration twice
        migrate(&options).unwrap();
        let report2 = migrate(&options).unwrap();

        // Second run should still succeed
        assert!(!report2.imported.is_empty());

        // secrets.env should not have duplicate keys
        let secrets = std::fs::read_to_string(target.path().join("secrets.env")).unwrap();
        let tg_count = secrets
            .lines()
            .filter(|l| l.starts_with("TELEGRAM_BOT_TOKEN="))
            .count();
        assert_eq!(tg_count, 1, "Duplicate TELEGRAM_BOT_TOKEN in secrets.env");

        let dc_count = secrets
            .lines()
            .filter(|l| l.starts_with("DISCORD_BOT_TOKEN="))
            .count();
        assert_eq!(dc_count, 1, "Duplicate DISCORD_BOT_TOKEN in secrets.env");
    }

    #[test]
    fn test_google_chat_channel_alias() {
        // Verify that "googlechat" (camelCase variant) is parsed correctly
        let target = TempDir::new().unwrap();
        let json5_content = r#"{
  channels: {
    googlechat: {
      webhookPath: "/webhook/gchat"
    }
  }
}"#;
        let root: OpenClawRoot = json5::from_str(json5_content).unwrap();
        let mut report = MigrationReport::default();

        let channels = migrate_channels_from_json(&root, target.path(), false, &mut report);
        assert!(channels.is_some());
        let ch_table = channels.unwrap();
        let table = ch_table.as_table().unwrap();
        assert!(
            table.contains_key("google_chat"),
            "googlechat should map to google_chat"
        );
    }

    #[test]
    fn test_signal_url_construction() {
        let target = TempDir::new().unwrap();
        let json5_content = r#"{
  channels: {
    signal: {
      httpHost: "signal-api.local",
      httpPort: 9090,
      account: "+15551234567"
    }
  }
}"#;
        let root: OpenClawRoot = json5::from_str(json5_content).unwrap();
        let mut report = MigrationReport::default();

        let channels = migrate_channels_from_json(&root, target.path(), false, &mut report);
        assert!(channels.is_some());
        let ch_table = channels.unwrap();
        let table = ch_table.as_table().unwrap();
        let sig = table["signal"].as_table().unwrap();
        assert_eq!(
            sig["api_url"].as_str().unwrap(),
            "http://signal-api.local:9090"
        );
        assert_eq!(sig["phone_number"].as_str().unwrap(), "+15551234567");
    }
}
