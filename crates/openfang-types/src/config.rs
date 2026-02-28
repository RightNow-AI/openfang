//! Configuration types for the OpenFang kernel.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// DM (direct message) policy for a channel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DmPolicy {
    /// Respond to all DMs.
    #[default]
    Respond,
    /// Only respond to DMs from allowed users.
    AllowedOnly,
    /// Ignore all DMs.
    Ignore,
}

/// Group message policy for a channel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupPolicy {
    /// Respond to all group messages.
    All,
    /// Only respond when mentioned (@bot).
    #[default]
    MentionOnly,
    /// Only respond to slash commands.
    CommandsOnly,
    /// Ignore all group messages.
    Ignore,
}

/// Output format hint for channel-specific message formatting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    /// Standard Markdown (default).
    #[default]
    Markdown,
    /// Telegram HTML subset.
    TelegramHtml,
    /// Slack mrkdwn format.
    SlackMrkdwn,
    /// Plain text (no formatting).
    PlainText,
}

/// Per-channel behavior overrides.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelOverrides {
    /// Model override (uses agent's default if None).
    pub model: Option<String>,
    /// System prompt override.
    pub system_prompt: Option<String>,
    /// DM policy.
    pub dm_policy: DmPolicy,
    /// Group message policy.
    pub group_policy: GroupPolicy,
    /// Per-user rate limit (messages per minute, 0 = unlimited).
    pub rate_limit_per_user: u32,
    /// Enable thread replies.
    pub threading: bool,
    /// Output format override.
    pub output_format: Option<OutputFormat>,
    /// Usage footer mode override.
    pub usage_footer: Option<UsageFooterMode>,
    /// Typing indicator mode override.
    pub typing_mode: Option<TypingMode>,
}

/// Controls what usage info appears in response footers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageFooterMode {
    /// Don't show usage info.
    Off,
    /// Show token counts only.
    Tokens,
    /// Show estimated cost only.
    Cost,
    /// Show tokens + cost (default).
    #[default]
    Full,
}

/// Kernel operating mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KernelMode {
    /// Conservative mode — no auto-updates, pinned models, stability-first.
    Stable,
    /// Default balanced mode.
    #[default]
    Default,
    /// Developer mode — experimental features enabled.
    Dev,
}

/// User configuration for RBAC multi-user support.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfig {
    /// User display name.
    pub name: String,
    /// User role (owner, admin, user, viewer).
    #[serde(default = "default_role")]
    pub role: String,
    /// Channel bindings: maps channel platform IDs to this user.
    /// e.g., {"telegram": "123456", "discord": "987654"}
    #[serde(default)]
    pub channel_bindings: HashMap<String, String>,
    /// Optional API key hash for API authentication.
    #[serde(default)]
    pub api_key_hash: Option<String>,
}

fn default_role() -> String {
    "user".to_string()
}

/// Web search provider selection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchProvider {
    /// Brave Search API.
    Brave,
    /// Tavily AI-agent-native search.
    Tavily,
    /// Perplexity AI search.
    Perplexity,
    /// DuckDuckGo HTML (no API key needed).
    DuckDuckGo,
    /// Auto-select based on available API keys (Tavily → Brave → Perplexity → DuckDuckGo).
    #[default]
    Auto,
}

/// Web tools configuration (search + fetch).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WebConfig {
    /// Which search provider to use.
    pub search_provider: SearchProvider,
    /// Cache TTL in minutes (0 = disabled).
    pub cache_ttl_minutes: u64,
    /// Brave Search configuration.
    pub brave: BraveSearchConfig,
    /// Tavily Search configuration.
    pub tavily: TavilySearchConfig,
    /// Perplexity Search configuration.
    pub perplexity: PerplexitySearchConfig,
    /// Web fetch configuration.
    pub fetch: WebFetchConfig,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            search_provider: SearchProvider::default(),
            cache_ttl_minutes: 15,
            brave: BraveSearchConfig::default(),
            tavily: TavilySearchConfig::default(),
            perplexity: PerplexitySearchConfig::default(),
            fetch: WebFetchConfig::default(),
        }
    }
}

/// Brave Search API configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BraveSearchConfig {
    /// Env var name holding the API key.
    pub api_key_env: String,
    /// Maximum results to return.
    pub max_results: usize,
    /// Country code for search localization (e.g., "US").
    pub country: String,
    /// Search language (e.g., "en").
    pub search_lang: String,
    /// Freshness filter (e.g., "pd" = past day, "pw" = past week).
    pub freshness: String,
}

impl Default for BraveSearchConfig {
    fn default() -> Self {
        Self {
            api_key_env: "BRAVE_API_KEY".to_string(),
            max_results: 5,
            country: String::new(),
            search_lang: String::new(),
            freshness: String::new(),
        }
    }
}

/// Tavily Search API configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TavilySearchConfig {
    /// Env var name holding the API key.
    pub api_key_env: String,
    /// Search depth: "basic" or "advanced".
    pub search_depth: String,
    /// Maximum results to return.
    pub max_results: usize,
    /// Include AI-generated answer summary.
    pub include_answer: bool,
}

impl Default for TavilySearchConfig {
    fn default() -> Self {
        Self {
            api_key_env: "TAVILY_API_KEY".to_string(),
            search_depth: "basic".to_string(),
            max_results: 5,
            include_answer: true,
        }
    }
}

/// Perplexity Search API configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PerplexitySearchConfig {
    /// Env var name holding the API key.
    pub api_key_env: String,
    /// Model to use for search (e.g., "sonar").
    pub model: String,
}

impl Default for PerplexitySearchConfig {
    fn default() -> Self {
        Self {
            api_key_env: "PERPLEXITY_API_KEY".to_string(),
            model: "sonar".to_string(),
        }
    }
}

/// Web fetch configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WebFetchConfig {
    /// Maximum characters to return in content.
    pub max_chars: usize,
    /// Maximum response body size in bytes.
    pub max_response_bytes: usize,
    /// HTTP request timeout in seconds.
    pub timeout_secs: u64,
    /// Enable HTML→Markdown readability extraction.
    pub readability: bool,
}

impl Default for WebFetchConfig {
    fn default() -> Self {
        Self {
            max_chars: 50_000,
            max_response_bytes: 10 * 1024 * 1024, // 10 MB
            timeout_secs: 30,
            readability: true,
        }
    }
}

/// Browser automation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BrowserConfig {
    /// Run browser in headless mode (no visible window).
    pub headless: bool,
    /// Viewport width in pixels.
    pub viewport_width: u32,
    /// Viewport height in pixels.
    pub viewport_height: u32,
    /// Per-action timeout in seconds.
    pub timeout_secs: u64,
    /// Idle timeout — auto-close session after this many seconds of inactivity.
    pub idle_timeout_secs: u64,
    /// Maximum concurrent browser sessions.
    pub max_sessions: usize,
    /// Python executable path (e.g., "python3" on Unix, "python" on Windows).
    pub python_path: String,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            headless: true,
            viewport_width: 1280,
            viewport_height: 720,
            timeout_secs: 30,
            idle_timeout_secs: 300,
            max_sessions: 5,
            python_path: if cfg!(windows) {
                "python".to_string()
            } else {
                "python3".to_string()
            },
        }
    }
}

/// Config hot-reload mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReloadMode {
    /// No automatic reloading.
    Off,
    /// Full restart on config change.
    Restart,
    /// Hot-reload safe sections only (channels, skills, heartbeat).
    Hot,
    /// Hot-reload where possible, flag restart-required otherwise.
    #[default]
    Hybrid,
}

/// Configuration for config file watching and hot-reload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ReloadConfig {
    /// Reload mode. Default: hybrid.
    pub mode: ReloadMode,
    /// Debounce window in milliseconds. Default: 500.
    pub debounce_ms: u64,
}

impl Default for ReloadConfig {
    fn default() -> Self {
        Self {
            mode: ReloadMode::default(),
            debounce_ms: 500,
        }
    }
}

/// Webhook trigger authentication configuration.
///
/// Controls the `/hooks/wake` and `/hooks/agent` endpoints for external
/// systems to trigger agent actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WebhookTriggerConfig {
    /// Enable webhook trigger endpoints. Default: false.
    pub enabled: bool,
    /// Env var name holding the bearer token (NOT the token itself).
    /// MUST be set if enabled=true. Token must be >= 32 chars.
    pub token_env: String,
    /// Max payload size in bytes. Default: 65536.
    pub max_payload_bytes: usize,
    /// Rate limit: max requests per minute per IP. Default: 30.
    pub rate_limit_per_minute: u32,
}

impl Default for WebhookTriggerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            token_env: "OPENFANG_WEBHOOK_TOKEN".to_string(),
            max_payload_bytes: 65536,
            rate_limit_per_minute: 30,
        }
    }
}

/// Fallback provider chain — tried in order if the primary provider fails.
///
/// Configurable in `config.toml` under `[[fallback_providers]]`:
/// ```toml
/// [[fallback_providers]]
/// provider = "ollama"
/// model = "llama3.2:latest"
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FallbackProviderConfig {
    /// Provider name (e.g., "ollama", "groq").
    pub provider: String,
    /// Model to use from this provider.
    pub model: String,
    /// Environment variable for API key (empty for local providers).
    #[serde(default)]
    pub api_key_env: String,
    /// Base URL override (uses catalog default if None).
    #[serde(default)]
    pub base_url: Option<String>,
}

/// Text-to-speech configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TtsConfig {
    /// Enable TTS. Default: false.
    pub enabled: bool,
    /// Default provider: "openai" or "elevenlabs".
    pub provider: Option<String>,
    /// OpenAI TTS settings.
    pub openai: TtsOpenAiConfig,
    /// ElevenLabs TTS settings.
    pub elevenlabs: TtsElevenLabsConfig,
    /// Max text length for TTS (chars). Default: 4096.
    pub max_text_length: usize,
    /// Timeout per TTS request in seconds. Default: 30.
    pub timeout_secs: u64,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: None,
            openai: TtsOpenAiConfig::default(),
            elevenlabs: TtsElevenLabsConfig::default(),
            max_text_length: 4096,
            timeout_secs: 30,
        }
    }
}

/// OpenAI TTS settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TtsOpenAiConfig {
    /// Voice: alloy, echo, fable, onyx, nova, shimmer. Default: "alloy".
    pub voice: String,
    /// Model: "tts-1" or "tts-1-hd". Default: "tts-1".
    pub model: String,
    /// Output format: "mp3", "opus", "aac", "flac". Default: "mp3".
    pub format: String,
    /// Speed: 0.25 to 4.0. Default: 1.0.
    pub speed: f32,
}

impl Default for TtsOpenAiConfig {
    fn default() -> Self {
        Self {
            voice: "alloy".to_string(),
            model: "tts-1".to_string(),
            format: "mp3".to_string(),
            speed: 1.0,
        }
    }
}

/// ElevenLabs TTS settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TtsElevenLabsConfig {
    /// Voice ID. Default: "21m00Tcm4TlvDq8ikWAM" (Rachel).
    pub voice_id: String,
    /// Model ID. Default: "eleven_monolingual_v1".
    pub model_id: String,
    /// Stability (0.0-1.0). Default: 0.5.
    pub stability: f32,
    /// Similarity boost (0.0-1.0). Default: 0.75.
    pub similarity_boost: f32,
}

impl Default for TtsElevenLabsConfig {
    fn default() -> Self {
        Self {
            voice_id: "21m00Tcm4TlvDq8ikWAM".to_string(),
            model_id: "eleven_monolingual_v1".to_string(),
            stability: 0.5,
            similarity_boost: 0.75,
        }
    }
}

/// Docker container sandbox configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DockerSandboxConfig {
    /// Enable Docker sandbox. Default: false.
    pub enabled: bool,
    /// Docker image for exec sandbox. Default: "python:3.12-slim".
    pub image: String,
    /// Container name prefix. Default: "openfang-sandbox".
    pub container_prefix: String,
    /// Working directory inside container. Default: "/workspace".
    pub workdir: String,
    /// Network mode: "none", "bridge", or custom. Default: "none".
    pub network: String,
    /// Memory limit (e.g., "256m", "1g"). Default: "512m".
    pub memory_limit: String,
    /// CPU limit (e.g., 0.5, 1.0, 2.0). Default: 1.0.
    pub cpu_limit: f64,
    /// Max execution time in seconds. Default: 60.
    pub timeout_secs: u64,
    /// Read-only root filesystem. Default: true.
    pub read_only_root: bool,
    /// Additional capabilities to add. Default: empty (drop all).
    pub cap_add: Vec<String>,
    /// tmpfs mounts. Default: ["/tmp:size=64m"].
    pub tmpfs: Vec<String>,
    /// PID limit. Default: 100.
    pub pids_limit: u32,
    /// Docker sandbox mode: off, non_main, all. Default: off.
    #[serde(default)]
    pub mode: DockerSandboxMode,
    /// Container lifecycle scope. Default: session.
    #[serde(default)]
    pub scope: DockerScope,
    /// Cooldown before reusing a released container (seconds). Default: 300.
    #[serde(default = "default_reuse_cool_secs")]
    pub reuse_cool_secs: u64,
    /// Idle timeout — destroy containers after N seconds of inactivity. Default: 86400 (24h).
    #[serde(default = "default_docker_idle_timeout")]
    pub idle_timeout_secs: u64,
    /// Maximum age before forced destruction (seconds). Default: 604800 (7 days).
    #[serde(default = "default_docker_max_age")]
    pub max_age_secs: u64,
    /// Paths blocked from bind mounting.
    #[serde(default)]
    pub blocked_mounts: Vec<String>,
}

fn default_reuse_cool_secs() -> u64 {
    300
}
fn default_docker_idle_timeout() -> u64 {
    86400
}
fn default_docker_max_age() -> u64 {
    604800
}

impl Default for DockerSandboxConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            image: "python:3.12-slim".to_string(),
            container_prefix: "openfang-sandbox".to_string(),
            workdir: "/workspace".to_string(),
            network: "none".to_string(),
            memory_limit: "512m".to_string(),
            cpu_limit: 1.0,
            timeout_secs: 60,
            read_only_root: true,
            cap_add: Vec::new(),
            tmpfs: vec!["/tmp:size=64m".to_string()],
            pids_limit: 100,
            mode: DockerSandboxMode::Off,
            scope: DockerScope::Session,
            reuse_cool_secs: default_reuse_cool_secs(),
            idle_timeout_secs: default_docker_idle_timeout(),
            max_age_secs: default_docker_max_age(),
            blocked_mounts: Vec::new(),
        }
    }
}

/// Device pairing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PairingConfig {
    /// Enable device pairing. Default: false.
    pub enabled: bool,
    /// Max paired devices. Default: 10.
    pub max_devices: usize,
    /// Pairing token expiry in seconds. Default: 300 (5 min).
    pub token_expiry_secs: u64,
    /// Push notification provider: "none", "ntfy", "gotify".
    pub push_provider: String,
    /// Ntfy server URL (if push_provider = "ntfy").
    pub ntfy_url: Option<String>,
    /// Ntfy topic (if push_provider = "ntfy").
    pub ntfy_topic: Option<String>,
}

impl Default for PairingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_devices: 10,
            token_expiry_secs: 300,
            push_provider: "none".to_string(),
            ntfy_url: None,
            ntfy_topic: None,
        }
    }
}

/// Extensions & integrations configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExtensionsConfig {
    /// Enable auto-reconnect for MCP integrations.
    pub auto_reconnect: bool,
    /// Maximum reconnect attempts before giving up.
    pub reconnect_max_attempts: u32,
    /// Maximum backoff duration in seconds.
    pub reconnect_max_backoff_secs: u64,
    /// Health check interval in seconds.
    pub health_check_interval_secs: u64,
}

impl Default for ExtensionsConfig {
    fn default() -> Self {
        Self {
            auto_reconnect: true,
            reconnect_max_attempts: 10,
            reconnect_max_backoff_secs: 300,
            health_check_interval_secs: 60,
        }
    }
}

/// Credential vault configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VaultConfig {
    /// Whether the vault is enabled (auto-detected if vault.enc exists).
    pub enabled: bool,
    /// Custom vault file path (default: ~/.openfang/vault.enc).
    pub path: Option<PathBuf>,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: None,
        }
    }
}

/// Agent binding — routes specific channel/account/peer patterns to agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentBinding {
    /// Target agent name or ID.
    pub agent: String,
    /// Match criteria (all specified fields must match).
    pub match_rule: BindingMatchRule,
}

/// Match rule for agent bindings. All specified (non-None) fields must match.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BindingMatchRule {
    /// Channel type (e.g., "discord", "telegram", "slack").
    pub channel: Option<String>,
    /// Specific account/bot ID within the channel.
    pub account_id: Option<String>,
    /// Peer/user ID for DM routing.
    pub peer_id: Option<String>,
    /// Guild/server ID (Discord/Slack).
    pub guild_id: Option<String>,
    /// Role-based routing (user must have at least one).
    #[serde(default)]
    pub roles: Vec<String>,
}

impl BindingMatchRule {
    /// Calculate specificity score for binding priority ordering.
    /// Higher = more specific = checked first.
    pub fn specificity(&self) -> u32 {
        let mut score = 0u32;
        if self.peer_id.is_some() {
            score += 8;
        }
        if self.guild_id.is_some() {
            score += 4;
        }
        if !self.roles.is_empty() {
            score += 2;
        }
        if self.account_id.is_some() {
            score += 2;
        }
        if self.channel.is_some() {
            score += 1;
        }
        score
    }
}

/// Broadcast config — send same message to multiple agents.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BroadcastConfig {
    /// Broadcast strategy.
    pub strategy: BroadcastStrategy,
    /// Map of peer_id -> list of agent names to receive the message.
    pub routes: HashMap<String, Vec<String>>,
}

/// Broadcast delivery strategy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BroadcastStrategy {
    /// Send to all agents simultaneously.
    #[default]
    Parallel,
    /// Send to agents one at a time in order.
    Sequential,
}

/// Auto-reply engine configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoReplyConfig {
    /// Enable auto-reply engine. Default: false.
    pub enabled: bool,
    /// Max concurrent auto-reply tasks. Default: 3.
    pub max_concurrent: usize,
    /// Default timeout per reply in seconds. Default: 120.
    pub timeout_secs: u64,
    /// Patterns that suppress auto-reply (e.g., "/stop", "/pause").
    pub suppress_patterns: Vec<String>,
}

impl Default for AutoReplyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_concurrent: 3,
            timeout_secs: 120,
            suppress_patterns: vec!["/stop".to_string(), "/pause".to_string()],
        }
    }
}

/// Canvas (Agent-to-UI) configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CanvasConfig {
    /// Enable canvas tool. Default: false.
    pub enabled: bool,
    /// Max HTML size in bytes. Default: 512KB.
    pub max_html_bytes: usize,
    /// Allowed HTML tags (empty = all safe tags allowed).
    #[serde(default)]
    pub allowed_tags: Vec<String>,
}

impl Default for CanvasConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_html_bytes: 512 * 1024,
            allowed_tags: Vec::new(),
        }
    }
}

/// Shell/exec security mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecSecurityMode {
    /// Block all shell execution.
    Deny,
    /// Only allow commands in safe_bins or allowed_commands.
    #[default]
    Allowlist,
    /// Allow all commands (unsafe, dev only).
    Full,
}

/// Shell/exec security policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExecPolicy {
    /// Security mode: "deny" blocks all, "allowlist" only allows listed,
    /// "full" allows all (unsafe, dev only).
    pub mode: ExecSecurityMode,
    /// Commands that bypass allowlist (stdin-only utilities).
    pub safe_bins: Vec<String>,
    /// Global command allowlist (when mode = allowlist).
    pub allowed_commands: Vec<String>,
    /// Max execution timeout in seconds. Default: 30.
    pub timeout_secs: u64,
    /// Max output size in bytes. Default: 100KB.
    pub max_output_bytes: usize,
    /// No-output idle timeout in seconds. When > 0, kills processes that
    /// produce no stdout/stderr output for this duration. Default: 30.
    #[serde(default = "default_no_output_timeout")]
    pub no_output_timeout_secs: u64,
}

fn default_no_output_timeout() -> u64 {
    30
}

impl Default for ExecPolicy {
    fn default() -> Self {
        Self {
            mode: ExecSecurityMode::default(),
            safe_bins: vec![
                "sleep", "true", "false", "cat", "sort", "uniq", "cut", "tr", "head", "tail", "wc",
                "date", "echo", "printf", "basename", "dirname", "pwd", "env",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            allowed_commands: Vec::new(),
            timeout_secs: 30,
            max_output_bytes: 100 * 1024,
            no_output_timeout_secs: default_no_output_timeout(),
        }
    }
}

// ---------------------------------------------------------------------------
// Gap 2: No-output idle timeout for subprocess sandbox
// ---------------------------------------------------------------------------

/// Reason a subprocess was terminated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminationReason {
    /// Process exited normally.
    Exited(i32),
    /// Absolute timeout exceeded.
    AbsoluteTimeout,
    /// No output timeout exceeded.
    NoOutputTimeout,
}

// ---------------------------------------------------------------------------
// Gap 3: Auth profile rotation — multi-key per provider
// ---------------------------------------------------------------------------

/// A named authentication profile for a provider.
///
/// Multiple profiles can be configured per provider to enable key rotation
/// when one key gets rate-limited or has billing issues.
#[derive(Clone, Serialize, Deserialize)]
pub struct AuthProfile {
    /// Profile name (e.g., "primary", "secondary").
    pub name: String,
    /// Environment variable holding the API key.
    pub api_key_env: String,
    /// Priority (lower = preferred). Default: 0.
    #[serde(default)]
    pub priority: u32,
}

/// SECURITY: Custom Debug impl redacts env var name.
impl std::fmt::Debug for AuthProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthProfile")
            .field("name", &self.name)
            .field("api_key_env", &"<redacted>")
            .field("priority", &self.priority)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Gap 5: Docker sandbox maturity
// ---------------------------------------------------------------------------

/// Docker sandbox activation mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockerSandboxMode {
    /// Docker sandbox disabled.
    #[default]
    Off,
    /// Only use Docker for non-main agents.
    NonMain,
    /// Use Docker for all agents.
    All,
}

/// Docker container lifecycle scope.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockerScope {
    /// Container per session (destroyed when session ends).
    #[default]
    Session,
    /// Container per agent (reused across sessions).
    Agent,
    /// Shared container pool.
    Shared,
}

// ---------------------------------------------------------------------------
// Gap 6: Typing indicator modes
// ---------------------------------------------------------------------------

/// Typing indicator behavior mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypingMode {
    /// Send typing indicator immediately on message receipt (default).
    #[default]
    Instant,
    /// Send typing indicator only when first text delta arrives.
    Message,
    /// Send typing indicator only during LLM reasoning.
    Thinking,
    /// Never send typing indicators.
    Never,
}

// ---------------------------------------------------------------------------
// Gap 7: Thinking level support
// ---------------------------------------------------------------------------

/// Extended thinking configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThinkingConfig {
    /// Maximum tokens for thinking (budget).
    pub budget_tokens: u32,
    /// Whether to stream thinking tokens to the client.
    pub stream_thinking: bool,
}

impl Default for ThinkingConfig {
    fn default() -> Self {
        Self {
            budget_tokens: 10_000,
            stream_thinking: false,
        }
    }
}

/// Top-level kernel configuration.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KernelConfig {
    /// OpenFang home directory (default: ~/.openfang).
    pub home_dir: PathBuf,
    /// Data directory for databases (default: ~/.openfang/data).
    pub data_dir: PathBuf,
    /// Log level (trace, debug, info, warn, error).
    pub log_level: String,
    /// API listen address (e.g., "0.0.0.0:4200").
    #[serde(alias = "listen_addr")]
    pub api_listen: String,
    /// Whether to enable the OFP network layer.
    pub network_enabled: bool,
    /// Default LLM provider configuration.
    pub default_model: DefaultModelConfig,
    /// Memory substrate configuration.
    pub memory: MemoryConfig,
    /// Network configuration.
    pub network: NetworkConfig,
    /// Channel bridge configuration (Telegram, etc.).
    pub channels: ChannelsConfig,
    /// API authentication key. When set, all API endpoints (except /api/health)
    /// require a `Authorization: Bearer <key>` header.
    /// If empty, the API is unauthenticated (local development only).
    pub api_key: String,
    /// Kernel operating mode (stable, default, dev).
    #[serde(default)]
    pub mode: KernelMode,
    /// Language/locale for CLI and messages (default: "en").
    #[serde(default = "default_language")]
    pub language: String,
    /// User configurations for RBAC multi-user support.
    #[serde(default)]
    pub users: Vec<UserConfig>,
    /// MCP server configurations for external tool integration.
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfigEntry>,
    /// A2A (Agent-to-Agent) protocol configuration.
    #[serde(default)]
    pub a2a: Option<A2aConfig>,
    /// Usage footer mode (what to show after each response).
    #[serde(default)]
    pub usage_footer: UsageFooterMode,
    /// Web tools configuration (search + fetch).
    #[serde(default)]
    pub web: WebConfig,
    /// Fallback providers tried in order if the primary fails.
    /// Configure in config.toml as `[[fallback_providers]]`.
    #[serde(default)]
    pub fallback_providers: Vec<FallbackProviderConfig>,
    /// Browser automation configuration.
    #[serde(default)]
    pub browser: BrowserConfig,
    /// Extensions & integrations configuration.
    #[serde(default)]
    pub extensions: ExtensionsConfig,
    /// Credential vault configuration.
    #[serde(default)]
    pub vault: VaultConfig,
    /// Root directory for agent workspaces. Default: `~/.openfang/workspaces`
    #[serde(default)]
    pub workspaces_dir: Option<PathBuf>,
    /// Media understanding configuration.
    #[serde(default)]
    pub media: crate::media::MediaConfig,
    /// Link understanding configuration.
    #[serde(default)]
    pub links: crate::media::LinkConfig,
    /// Config hot-reload settings.
    #[serde(default)]
    pub reload: ReloadConfig,
    /// Webhook trigger configuration (external event injection).
    #[serde(default)]
    pub webhook_triggers: Option<WebhookTriggerConfig>,
    /// Execution approval policy.
    #[serde(default, alias = "approval_policy")]
    pub approval: crate::approval::ApprovalPolicy,
    /// Cron scheduler max total jobs across all agents. Default: 500.
    #[serde(default = "default_max_cron_jobs")]
    pub max_cron_jobs: usize,
    /// Config include files — loaded and deep-merged before the root config.
    /// Paths are relative to the root config file's directory.
    /// Security: absolute paths and `..` components are rejected.
    #[serde(default)]
    pub include: Vec<String>,
    /// Shell/exec security policy.
    #[serde(default)]
    pub exec_policy: ExecPolicy,
    /// Agent bindings for multi-account routing.
    #[serde(default)]
    pub bindings: Vec<AgentBinding>,
    /// Broadcast routing configuration.
    #[serde(default)]
    pub broadcast: BroadcastConfig,
    /// Auto-reply background engine configuration.
    #[serde(default)]
    pub auto_reply: AutoReplyConfig,
    /// Canvas (A2UI) configuration.
    #[serde(default)]
    pub canvas: CanvasConfig,
    /// Text-to-speech configuration.
    #[serde(default)]
    pub tts: TtsConfig,
    /// Docker container sandbox configuration.
    #[serde(default)]
    pub docker: DockerSandboxConfig,
    /// Device pairing configuration.
    #[serde(default)]
    pub pairing: PairingConfig,
    /// Auth profiles for key rotation (provider name → profiles).
    #[serde(default)]
    pub auth_profiles: HashMap<String, Vec<AuthProfile>>,
    /// Extended thinking configuration.
    #[serde(default)]
    pub thinking: Option<ThinkingConfig>,
    /// Global spending budget configuration.
    #[serde(default)]
    pub budget: BudgetConfig,
    /// Provider base URL overrides (provider ID → custom base URL).
    /// e.g. `ollama = "http://192.168.1.100:11434/v1"`
    #[serde(default)]
    pub provider_urls: HashMap<String, String>,
    /// OAuth client ID overrides for PKCE flows.
    #[serde(default)]
    pub oauth: OAuthConfig,
}

/// OAuth client ID overrides for PKCE flows.
///
/// Configure in config.toml:
/// ```toml
/// [oauth]
/// google_client_id = "your-google-client-id"
/// github_client_id = "your-github-client-id"
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OAuthConfig {
    /// Google OAuth2 client ID for PKCE flow.
    pub google_client_id: Option<String>,
    /// GitHub OAuth client ID for PKCE flow.
    pub github_client_id: Option<String>,
    /// Microsoft (Entra ID) OAuth client ID.
    pub microsoft_client_id: Option<String>,
    /// Slack OAuth client ID.
    pub slack_client_id: Option<String>,
}

/// Global spending budget configuration.
///
/// Set limits to 0.0 for unlimited. All limits apply across all agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BudgetConfig {
    /// Maximum total cost in USD per hour (0.0 = unlimited).
    pub max_hourly_usd: f64,
    /// Maximum total cost in USD per day (0.0 = unlimited).
    pub max_daily_usd: f64,
    /// Maximum total cost in USD per month (0.0 = unlimited).
    pub max_monthly_usd: f64,
    /// Alert threshold as a fraction (0.0 - 1.0). Trigger warnings at this % of any limit.
    pub alert_threshold: f64,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            max_hourly_usd: 0.0,
            max_daily_usd: 0.0,
            max_monthly_usd: 0.0,
            alert_threshold: 0.8,
        }
    }
}

fn default_max_cron_jobs() -> usize {
    500
}

/// Configuration entry for an MCP server.
///
/// This is the config.toml representation. The runtime `McpServerConfig`
/// struct is constructed from this during kernel boot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfigEntry {
    /// Display name for this server.
    pub name: String,
    /// Transport configuration.
    pub transport: McpTransportEntry,
    /// Request timeout in seconds.
    #[serde(default = "default_mcp_timeout")]
    pub timeout_secs: u64,
    /// Environment variables to pass through (e.g., ["GITHUB_PERSONAL_ACCESS_TOKEN"]).
    #[serde(default)]
    pub env: Vec<String>,
}

fn default_mcp_timeout() -> u64 {
    30
}

/// Transport configuration for an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpTransportEntry {
    /// Subprocess with JSON-RPC over stdin/stdout.
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
    },
    /// HTTP Server-Sent Events.
    Sse { url: String },
}

/// A2A (Agent-to-Agent) protocol configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct A2aConfig {
    /// Whether A2A is enabled.
    pub enabled: bool,
    /// Path to serve A2A endpoints (default: "/a2a").
    #[serde(default = "default_a2a_path")]
    pub listen_path: String,
    /// External A2A agents to connect to.
    #[serde(default)]
    pub external_agents: Vec<ExternalAgent>,
}

fn default_a2a_path() -> String {
    "/a2a".to_string()
}

/// An external A2A agent to discover and interact with.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalAgent {
    /// Display name.
    pub name: String,
    /// Agent endpoint URL.
    pub url: String,
}

fn default_language() -> String {
    "en".to_string()
}

impl Default for KernelConfig {
    fn default() -> Self {
        let home_dir = dirs_next_home().join(".openfang");
        Self {
            data_dir: home_dir.join("data"),
            home_dir,
            log_level: "info".to_string(),
            api_listen: "127.0.0.1:50051".to_string(),
            network_enabled: false,
            default_model: DefaultModelConfig::default(),
            memory: MemoryConfig::default(),
            network: NetworkConfig::default(),
            channels: ChannelsConfig::default(),
            api_key: String::new(),
            mode: KernelMode::default(),
            language: "en".to_string(),
            users: Vec::new(),
            mcp_servers: Vec::new(),
            a2a: None,
            usage_footer: UsageFooterMode::default(),
            web: WebConfig::default(),
            fallback_providers: Vec::new(),
            browser: BrowserConfig::default(),
            extensions: ExtensionsConfig::default(),
            vault: VaultConfig::default(),
            workspaces_dir: None,
            media: crate::media::MediaConfig::default(),
            links: crate::media::LinkConfig::default(),
            reload: ReloadConfig::default(),
            webhook_triggers: None,
            approval: crate::approval::ApprovalPolicy::default(),
            max_cron_jobs: default_max_cron_jobs(),
            include: Vec::new(),
            exec_policy: ExecPolicy::default(),
            bindings: Vec::new(),
            broadcast: BroadcastConfig::default(),
            auto_reply: AutoReplyConfig::default(),
            canvas: CanvasConfig::default(),
            tts: TtsConfig::default(),
            docker: DockerSandboxConfig::default(),
            pairing: PairingConfig::default(),
            auth_profiles: HashMap::new(),
            thinking: None,
            budget: BudgetConfig::default(),
            provider_urls: HashMap::new(),
            oauth: OAuthConfig::default(),
        }
    }
}

impl KernelConfig {
    /// Resolved workspaces root directory.
    pub fn effective_workspaces_dir(&self) -> PathBuf {
        self.workspaces_dir
            .clone()
            .unwrap_or_else(|| self.home_dir.join("workspaces"))
    }
}

/// SECURITY: Custom Debug impl redacts sensitive fields (api_key).
impl std::fmt::Debug for KernelConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KernelConfig")
            .field("home_dir", &self.home_dir)
            .field("data_dir", &self.data_dir)
            .field("log_level", &self.log_level)
            .field("api_listen", &self.api_listen)
            .field("network_enabled", &self.network_enabled)
            .field("default_model", &self.default_model)
            .field("memory", &self.memory)
            .field("network", &self.network)
            .field("channels", &self.channels)
            .field(
                "api_key",
                &if self.api_key.is_empty() {
                    "<empty>"
                } else {
                    "<redacted>"
                },
            )
            .field("mode", &self.mode)
            .field("language", &self.language)
            .field("users", &format!("{} user(s)", self.users.len()))
            .field(
                "mcp_servers",
                &format!("{} server(s)", self.mcp_servers.len()),
            )
            .field("a2a", &self.a2a.as_ref().map(|a| a.enabled))
            .field("usage_footer", &self.usage_footer)
            .field("web", &self.web)
            .field(
                "fallback_providers",
                &format!("{} provider(s)", self.fallback_providers.len()),
            )
            .field("browser", &self.browser)
            .field("extensions", &self.extensions)
            .field("vault", &format!("enabled={}", self.vault.enabled))
            .field("workspaces_dir", &self.workspaces_dir)
            .field(
                "media",
                &format!(
                    "image={} audio={} video={}",
                    self.media.image_description,
                    self.media.audio_transcription,
                    self.media.video_description
                ),
            )
            .field("links", &format!("enabled={}", self.links.enabled))
            .field("reload", &self.reload.mode)
            .field(
                "webhook_triggers",
                &self.webhook_triggers.as_ref().map(|w| w.enabled),
            )
            .field(
                "approval",
                &format!("{} tool(s)", self.approval.require_approval.len()),
            )
            .field("max_cron_jobs", &self.max_cron_jobs)
            .field("include", &format!("{} file(s)", self.include.len()))
            .field("exec_policy", &self.exec_policy.mode)
            .field("bindings", &format!("{} binding(s)", self.bindings.len()))
            .field(
                "broadcast",
                &format!("{} route(s)", self.broadcast.routes.len()),
            )
            .field(
                "auto_reply",
                &format!("enabled={}", self.auto_reply.enabled),
            )
            .field("canvas", &format!("enabled={}", self.canvas.enabled))
            .field("tts", &format!("enabled={}", self.tts.enabled))
            .field("docker", &format!("enabled={}", self.docker.enabled))
            .field("pairing", &format!("enabled={}", self.pairing.enabled))
            .field(
                "auth_profiles",
                &format!("{} provider(s)", self.auth_profiles.len()),
            )
            .field("thinking", &self.thinking.is_some())
            .finish()
    }
}

/// Fallback home directory resolution.
fn dirs_next_home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(std::env::temp_dir)
}

/// Default LLM model configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DefaultModelConfig {
    /// Provider name (e.g., "anthropic", "openai").
    pub provider: String,
    /// Model identifier.
    pub model: String,
    /// Environment variable name for the API key.
    pub api_key_env: String,
    /// Optional base URL override.
    pub base_url: Option<String>,
}

impl Default for DefaultModelConfig {
    fn default() -> Self {
        Self {
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            api_key_env: "ANTHROPIC_API_KEY".to_string(),
            base_url: None,
        }
    }
}

/// Memory substrate configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MemoryConfig {
    /// Path to SQLite database file.
    pub sqlite_path: Option<PathBuf>,
    /// Embedding model for semantic search.
    pub embedding_model: String,
    /// Maximum memories before consolidation is triggered.
    pub consolidation_threshold: u64,
    /// Memory decay rate (0.0 = no decay, 1.0 = aggressive decay).
    pub decay_rate: f32,
    /// Embedding provider (e.g., "openai", "ollama"). None = auto-detect.
    #[serde(default)]
    pub embedding_provider: Option<String>,
    /// Environment variable name for the embedding API key.
    #[serde(default)]
    pub embedding_api_key_env: Option<String>,
    /// How often to run memory consolidation (hours). 0 = disabled.
    #[serde(default = "default_consolidation_interval")]
    pub consolidation_interval_hours: u64,
}

fn default_consolidation_interval() -> u64 {
    24
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            sqlite_path: None,
            embedding_model: "all-MiniLM-L6-v2".to_string(),
            consolidation_threshold: 10_000,
            decay_rate: 0.1,
            embedding_provider: None,
            embedding_api_key_env: None,
            consolidation_interval_hours: default_consolidation_interval(),
        }
    }
}

/// Network layer configuration.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkConfig {
    /// libp2p listen addresses.
    pub listen_addresses: Vec<String>,
    /// Bootstrap peers for DHT.
    pub bootstrap_peers: Vec<String>,
    /// Enable mDNS for local discovery.
    pub mdns_enabled: bool,
    /// Maximum number of connected peers.
    pub max_peers: u32,
    /// Pre-shared secret for OFP HMAC authentication (required when network is enabled).
    pub shared_secret: String,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_addresses: vec!["/ip4/0.0.0.0/tcp/0".to_string()],
            bootstrap_peers: vec![],
            mdns_enabled: true,
            max_peers: 50,
            shared_secret: String::new(),
        }
    }
}

/// SECURITY: Custom Debug impl redacts sensitive fields (shared_secret).
impl std::fmt::Debug for NetworkConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkConfig")
            .field("listen_addresses", &self.listen_addresses)
            .field("bootstrap_peers", &self.bootstrap_peers)
            .field("mdns_enabled", &self.mdns_enabled)
            .field("max_peers", &self.max_peers)
            .field(
                "shared_secret",
                &if self.shared_secret.is_empty() {
                    "<empty>"
                } else {
                    "<redacted>"
                },
            )
            .finish()
    }
}

/// Channel bridge configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelsConfig {
    /// Telegram bot configuration (None = disabled).
    pub telegram: Option<TelegramConfig>,
    /// Discord bot configuration (None = disabled).
    pub discord: Option<DiscordConfig>,
    /// Slack bot configuration (None = disabled).
    pub slack: Option<SlackConfig>,
    /// WhatsApp Cloud API configuration (None = disabled).
    pub whatsapp: Option<WhatsAppConfig>,
    /// Signal (via signal-cli) configuration (None = disabled).
    pub signal: Option<SignalConfig>,
    /// Matrix protocol configuration (None = disabled).
    pub matrix: Option<MatrixConfig>,
    /// Email (IMAP/SMTP) configuration (None = disabled).
    pub email: Option<EmailConfig>,
    /// Microsoft Teams configuration (None = disabled).
    pub teams: Option<TeamsConfig>,
    /// Mattermost configuration (None = disabled).
    pub mattermost: Option<MattermostConfig>,
    /// IRC configuration (None = disabled).
    pub irc: Option<IrcConfig>,
    /// Google Chat configuration (None = disabled).
    pub google_chat: Option<GoogleChatConfig>,
    /// Twitch chat configuration (None = disabled).
    pub twitch: Option<TwitchConfig>,
    /// Rocket.Chat configuration (None = disabled).
    pub rocketchat: Option<RocketChatConfig>,
    /// Zulip configuration (None = disabled).
    pub zulip: Option<ZulipConfig>,
    /// XMPP/Jabber configuration (None = disabled).
    pub xmpp: Option<XmppConfig>,
    // Wave 3 — High-value channels
    /// LINE Messaging API configuration (None = disabled).
    pub line: Option<LineConfig>,
    /// Viber Bot API configuration (None = disabled).
    pub viber: Option<ViberConfig>,
    /// Facebook Messenger configuration (None = disabled).
    pub messenger: Option<MessengerConfig>,
    /// Reddit API configuration (None = disabled).
    pub reddit: Option<RedditConfig>,
    /// Mastodon Streaming API configuration (None = disabled).
    pub mastodon: Option<MastodonConfig>,
    /// Bluesky/AT Protocol configuration (None = disabled).
    pub bluesky: Option<BlueskyConfig>,
    /// Feishu/Lark Open Platform configuration (None = disabled).
    pub feishu: Option<FeishuConfig>,
    /// Revolt (Discord-like) configuration (None = disabled).
    pub revolt: Option<RevoltConfig>,
    // Wave 4 — Enterprise & community channels
    /// Nextcloud Talk configuration (None = disabled).
    pub nextcloud: Option<NextcloudConfig>,
    /// Guilded bot configuration (None = disabled).
    pub guilded: Option<GuildedConfig>,
    /// Keybase chat configuration (None = disabled).
    pub keybase: Option<KeybaseConfig>,
    /// Threema Gateway configuration (None = disabled).
    pub threema: Option<ThreemaConfig>,
    /// Nostr relay configuration (None = disabled).
    pub nostr: Option<NostrConfig>,
    /// Webex bot configuration (None = disabled).
    pub webex: Option<WebexConfig>,
    /// Pumble bot configuration (None = disabled).
    pub pumble: Option<PumbleConfig>,
    /// Flock bot configuration (None = disabled).
    pub flock: Option<FlockConfig>,
    /// Twist API configuration (None = disabled).
    pub twist: Option<TwistConfig>,
    // Wave 5 — Niche & differentiating channels
    /// Mumble text chat configuration (None = disabled).
    pub mumble: Option<MumbleConfig>,
    /// DingTalk robot configuration (None = disabled).
    pub dingtalk: Option<DingTalkConfig>,
    /// Discourse forum configuration (None = disabled).
    pub discourse: Option<DiscourseConfig>,
    /// Gitter streaming configuration (None = disabled).
    pub gitter: Option<GitterConfig>,
    /// ntfy.sh pub/sub configuration (None = disabled).
    pub ntfy: Option<NtfyConfig>,
    /// Gotify notification configuration (None = disabled).
    pub gotify: Option<GotifyConfig>,
    /// Generic webhook configuration (None = disabled).
    pub webhook: Option<WebhookConfig>,
    /// LinkedIn messaging configuration (None = disabled).
    pub linkedin: Option<LinkedInConfig>,
}

/// Telegram channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TelegramConfig {
    /// Env var name holding the bot token (NOT the token itself).
    pub bot_token_env: String,
    /// Telegram user IDs allowed to interact (empty = allow all).
    pub allowed_users: Vec<i64>,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Polling interval in seconds.
    pub poll_interval_secs: u64,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            bot_token_env: "TELEGRAM_BOT_TOKEN".to_string(),
            allowed_users: vec![],
            default_agent: None,
            poll_interval_secs: 1,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Discord channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DiscordConfig {
    /// Env var name holding the bot token (NOT the token itself).
    pub bot_token_env: String,
    /// Guild (server) IDs allowed to interact (empty = allow all).
    pub allowed_guilds: Vec<u64>,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Gateway intents bitmask (default: 33280 = GUILD_MESSAGES | MESSAGE_CONTENT).
    pub intents: u64,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for DiscordConfig {
    fn default() -> Self {
        Self {
            bot_token_env: "DISCORD_BOT_TOKEN".to_string(),
            allowed_guilds: vec![],
            default_agent: None,
            intents: 33280,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Slack channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SlackConfig {
    /// Env var name holding the app-level token (xapp-) for Socket Mode.
    pub app_token_env: String,
    /// Env var name holding the bot token (xoxb-) for REST API.
    pub bot_token_env: String,
    /// Channel IDs allowed to interact (empty = allow all).
    pub allowed_channels: Vec<String>,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for SlackConfig {
    fn default() -> Self {
        Self {
            app_token_env: "SLACK_APP_TOKEN".to_string(),
            bot_token_env: "SLACK_BOT_TOKEN".to_string(),
            allowed_channels: vec![],
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// WhatsApp Cloud API channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WhatsAppConfig {
    /// Env var name holding the access token (Cloud API mode).
    pub access_token_env: String,
    /// Env var name holding the webhook verify token (Cloud API mode).
    pub verify_token_env: String,
    /// WhatsApp Business phone number ID (Cloud API mode).
    pub phone_number_id: String,
    /// Port to listen for webhook callbacks (Cloud API mode).
    pub webhook_port: u16,
    /// Env var name holding the WhatsApp Web gateway URL (QR/Web mode).
    /// When set, outgoing messages are routed through the gateway instead of Cloud API.
    pub gateway_url_env: String,
    /// Allowed phone numbers (empty = allow all).
    pub allowed_users: Vec<String>,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for WhatsAppConfig {
    fn default() -> Self {
        Self {
            access_token_env: "WHATSAPP_ACCESS_TOKEN".to_string(),
            verify_token_env: "WHATSAPP_VERIFY_TOKEN".to_string(),
            phone_number_id: String::new(),
            webhook_port: 8443,
            gateway_url_env: "WHATSAPP_WEB_GATEWAY_URL".to_string(),
            allowed_users: vec![],
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Signal channel adapter configuration (via signal-cli REST API).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SignalConfig {
    /// URL of the signal-cli REST API (e.g., "http://localhost:8080").
    pub api_url: String,
    /// Registered phone number.
    pub phone_number: String,
    /// Allowed phone numbers (empty = allow all).
    pub allowed_users: Vec<String>,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for SignalConfig {
    fn default() -> Self {
        Self {
            api_url: "http://localhost:8080".to_string(),
            phone_number: String::new(),
            allowed_users: vec![],
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Matrix protocol channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MatrixConfig {
    /// Matrix homeserver URL (e.g., `"https://matrix.org"`).
    pub homeserver_url: String,
    /// Bot user ID (e.g., "@openfang:matrix.org").
    pub user_id: String,
    /// Env var name holding the access token.
    pub access_token_env: String,
    /// Room IDs to listen in (empty = all joined rooms).
    pub allowed_rooms: Vec<String>,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for MatrixConfig {
    fn default() -> Self {
        Self {
            homeserver_url: "https://matrix.org".to_string(),
            user_id: String::new(),
            access_token_env: "MATRIX_ACCESS_TOKEN".to_string(),
            allowed_rooms: vec![],
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Email (IMAP/SMTP) channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EmailConfig {
    /// IMAP server host.
    pub imap_host: String,
    /// IMAP port (993 for TLS).
    pub imap_port: u16,
    /// SMTP server host.
    pub smtp_host: String,
    /// SMTP port (587 for STARTTLS).
    pub smtp_port: u16,
    /// Email address (used for both IMAP and SMTP).
    pub username: String,
    /// Env var name holding the password.
    pub password_env: String,
    /// Poll interval in seconds.
    pub poll_interval_secs: u64,
    /// IMAP folders to monitor.
    pub folders: Vec<String>,
    /// Only process emails from these senders (empty = all).
    pub allowed_senders: Vec<String>,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            imap_host: String::new(),
            imap_port: 993,
            smtp_host: String::new(),
            smtp_port: 587,
            username: String::new(),
            password_env: "EMAIL_PASSWORD".to_string(),
            poll_interval_secs: 30,
            folders: vec!["INBOX".to_string()],
            allowed_senders: vec![],
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Microsoft Teams (Bot Framework v3) channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TeamsConfig {
    /// Azure Bot App ID.
    pub app_id: String,
    /// Env var name holding the app password.
    pub app_password_env: String,
    /// Port for the incoming webhook.
    pub webhook_port: u16,
    /// Allowed tenant IDs (empty = allow all).
    pub allowed_tenants: Vec<String>,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for TeamsConfig {
    fn default() -> Self {
        Self {
            app_id: String::new(),
            app_password_env: "TEAMS_APP_PASSWORD".to_string(),
            webhook_port: 3978,
            allowed_tenants: vec![],
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Mattermost channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MattermostConfig {
    /// Mattermost server URL (e.g., `"https://mattermost.example.com"`).
    pub server_url: String,
    /// Env var name holding the bot token.
    pub token_env: String,
    /// Allowed channel IDs (empty = all).
    pub allowed_channels: Vec<String>,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for MattermostConfig {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            token_env: "MATTERMOST_TOKEN".to_string(),
            allowed_channels: vec![],
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// IRC channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IrcConfig {
    /// IRC server hostname.
    pub server: String,
    /// IRC server port.
    pub port: u16,
    /// Bot nickname.
    pub nick: String,
    /// Env var name holding the server password (optional).
    pub password_env: Option<String>,
    /// Channels to join (e.g., `["#openfang", "#general"]`).
    pub channels: Vec<String>,
    /// Use TLS (requires tokio-native-tls).
    pub use_tls: bool,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for IrcConfig {
    fn default() -> Self {
        Self {
            server: "irc.libera.chat".to_string(),
            port: 6667,
            nick: "openfang".to_string(),
            password_env: None,
            channels: vec![],
            use_tls: false,
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Google Chat channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GoogleChatConfig {
    /// Env var name holding the service account JSON key.
    pub service_account_env: String,
    /// Space IDs to listen in.
    pub space_ids: Vec<String>,
    /// Port for the incoming webhook.
    pub webhook_port: u16,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for GoogleChatConfig {
    fn default() -> Self {
        Self {
            service_account_env: "GOOGLE_CHAT_SERVICE_ACCOUNT".to_string(),
            space_ids: vec![],
            webhook_port: 8444,
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Twitch chat channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TwitchConfig {
    /// Env var name holding the OAuth token.
    pub oauth_token_env: String,
    /// Twitch channels to join (without #).
    pub channels: Vec<String>,
    /// Bot nickname.
    pub nick: String,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for TwitchConfig {
    fn default() -> Self {
        Self {
            oauth_token_env: "TWITCH_OAUTH_TOKEN".to_string(),
            channels: vec![],
            nick: "openfang".to_string(),
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Rocket.Chat channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RocketChatConfig {
    /// Rocket.Chat server URL.
    pub server_url: String,
    /// Env var name holding the auth token.
    pub token_env: String,
    /// User ID for the bot.
    pub user_id: String,
    /// Allowed channel IDs (empty = all).
    pub allowed_channels: Vec<String>,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for RocketChatConfig {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            token_env: "ROCKETCHAT_TOKEN".to_string(),
            user_id: String::new(),
            allowed_channels: vec![],
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Zulip channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ZulipConfig {
    /// Zulip server URL.
    pub server_url: String,
    /// Bot email address.
    pub bot_email: String,
    /// Env var name holding the API key.
    pub api_key_env: String,
    /// Streams to listen in.
    pub streams: Vec<String>,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for ZulipConfig {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            bot_email: String::new(),
            api_key_env: "ZULIP_API_KEY".to_string(),
            streams: vec![],
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// XMPP/Jabber channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct XmppConfig {
    /// JID (e.g., "bot@jabber.org").
    pub jid: String,
    /// Env var name holding the password.
    pub password_env: String,
    /// XMPP server hostname (defaults to JID domain).
    pub server: String,
    /// XMPP server port.
    pub port: u16,
    /// MUC rooms to join.
    pub rooms: Vec<String>,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for XmppConfig {
    fn default() -> Self {
        Self {
            jid: String::new(),
            password_env: "XMPP_PASSWORD".to_string(),
            server: String::new(),
            port: 5222,
            rooms: vec![],
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

// ── Wave 3 channel configs ─────────────────────────────────────────

/// LINE Messaging API channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LineConfig {
    /// Env var name holding the channel secret.
    pub channel_secret_env: String,
    /// Env var name holding the channel access token.
    pub access_token_env: String,
    /// Port for the incoming webhook.
    pub webhook_port: u16,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for LineConfig {
    fn default() -> Self {
        Self {
            channel_secret_env: "LINE_CHANNEL_SECRET".to_string(),
            access_token_env: "LINE_CHANNEL_ACCESS_TOKEN".to_string(),
            webhook_port: 8450,
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Viber Bot API channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ViberConfig {
    /// Env var name holding the auth token.
    pub auth_token_env: String,
    /// Webhook URL for receiving messages.
    pub webhook_url: String,
    /// Port for the incoming webhook.
    pub webhook_port: u16,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for ViberConfig {
    fn default() -> Self {
        Self {
            auth_token_env: "VIBER_AUTH_TOKEN".to_string(),
            webhook_url: String::new(),
            webhook_port: 8451,
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Facebook Messenger Platform channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MessengerConfig {
    /// Env var name holding the page access token.
    pub page_token_env: String,
    /// Env var name holding the webhook verify token.
    pub verify_token_env: String,
    /// Port for the incoming webhook.
    pub webhook_port: u16,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for MessengerConfig {
    fn default() -> Self {
        Self {
            page_token_env: "MESSENGER_PAGE_TOKEN".to_string(),
            verify_token_env: "MESSENGER_VERIFY_TOKEN".to_string(),
            webhook_port: 8452,
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Reddit API channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RedditConfig {
    /// Reddit app client ID.
    pub client_id: String,
    /// Env var name holding the client secret.
    pub client_secret_env: String,
    /// Reddit bot username.
    pub username: String,
    /// Env var name holding the bot password.
    pub password_env: String,
    /// Subreddits to monitor.
    pub subreddits: Vec<String>,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for RedditConfig {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            client_secret_env: "REDDIT_CLIENT_SECRET".to_string(),
            username: String::new(),
            password_env: "REDDIT_PASSWORD".to_string(),
            subreddits: vec![],
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Mastodon Streaming API channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MastodonConfig {
    /// Mastodon instance URL (e.g., `"https://mastodon.social"`).
    pub instance_url: String,
    /// Env var name holding the access token.
    pub access_token_env: String,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for MastodonConfig {
    fn default() -> Self {
        Self {
            instance_url: String::new(),
            access_token_env: "MASTODON_ACCESS_TOKEN".to_string(),
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Bluesky/AT Protocol channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BlueskyConfig {
    /// Bluesky identifier (handle or DID).
    pub identifier: String,
    /// Env var name holding the app password.
    pub app_password_env: String,
    /// PDS service URL.
    pub service_url: String,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for BlueskyConfig {
    fn default() -> Self {
        Self {
            identifier: String::new(),
            app_password_env: "BLUESKY_APP_PASSWORD".to_string(),
            service_url: "https://bsky.social".to_string(),
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Feishu/Lark Open Platform channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FeishuConfig {
    /// Feishu app ID.
    pub app_id: String,
    /// Env var name holding the app secret.
    pub app_secret_env: String,
    /// Port for the incoming webhook.
    pub webhook_port: u16,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for FeishuConfig {
    fn default() -> Self {
        Self {
            app_id: String::new(),
            app_secret_env: "FEISHU_APP_SECRET".to_string(),
            webhook_port: 8453,
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Revolt (Discord-like) channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RevoltConfig {
    /// Env var name holding the bot token.
    pub bot_token_env: String,
    /// Revolt API URL.
    pub api_url: String,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for RevoltConfig {
    fn default() -> Self {
        Self {
            bot_token_env: "REVOLT_BOT_TOKEN".to_string(),
            api_url: "https://api.revolt.chat".to_string(),
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

// ── Wave 4 channel configs ─────────────────────────────────────────

/// Nextcloud Talk channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NextcloudConfig {
    /// Nextcloud server URL.
    pub server_url: String,
    /// Env var name holding the auth token.
    pub token_env: String,
    /// Room tokens to listen in (empty = all).
    pub allowed_rooms: Vec<String>,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for NextcloudConfig {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            token_env: "NEXTCLOUD_TOKEN".to_string(),
            allowed_rooms: vec![],
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Guilded bot channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GuildedConfig {
    /// Env var name holding the bot token.
    pub bot_token_env: String,
    /// Server IDs to listen in (empty = all).
    pub server_ids: Vec<String>,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for GuildedConfig {
    fn default() -> Self {
        Self {
            bot_token_env: "GUILDED_BOT_TOKEN".to_string(),
            server_ids: vec![],
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Keybase chat channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KeybaseConfig {
    /// Keybase username.
    pub username: String,
    /// Env var name holding the paper key.
    pub paperkey_env: String,
    /// Team names to listen in (empty = all DMs).
    pub allowed_teams: Vec<String>,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for KeybaseConfig {
    fn default() -> Self {
        Self {
            username: String::new(),
            paperkey_env: "KEYBASE_PAPERKEY".to_string(),
            allowed_teams: vec![],
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Threema Gateway channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThreemaConfig {
    /// Threema Gateway ID.
    pub threema_id: String,
    /// Env var name holding the API secret.
    pub secret_env: String,
    /// Port for the incoming webhook.
    pub webhook_port: u16,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for ThreemaConfig {
    fn default() -> Self {
        Self {
            threema_id: String::new(),
            secret_env: "THREEMA_SECRET".to_string(),
            webhook_port: 8454,
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Nostr relay channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NostrConfig {
    /// Env var name holding the private key (nsec or hex).
    pub private_key_env: String,
    /// Relay URLs to connect to.
    pub relays: Vec<String>,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for NostrConfig {
    fn default() -> Self {
        Self {
            private_key_env: "NOSTR_PRIVATE_KEY".to_string(),
            relays: vec!["wss://relay.damus.io".to_string()],
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Webex bot channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WebexConfig {
    /// Env var name holding the bot token.
    pub bot_token_env: String,
    /// Room IDs to listen in (empty = all).
    pub allowed_rooms: Vec<String>,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for WebexConfig {
    fn default() -> Self {
        Self {
            bot_token_env: "WEBEX_BOT_TOKEN".to_string(),
            allowed_rooms: vec![],
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Pumble bot channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PumbleConfig {
    /// Env var name holding the bot token.
    pub bot_token_env: String,
    /// Port for the incoming webhook.
    pub webhook_port: u16,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for PumbleConfig {
    fn default() -> Self {
        Self {
            bot_token_env: "PUMBLE_BOT_TOKEN".to_string(),
            webhook_port: 8455,
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Flock bot channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FlockConfig {
    /// Env var name holding the bot token.
    pub bot_token_env: String,
    /// Port for the incoming webhook.
    pub webhook_port: u16,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for FlockConfig {
    fn default() -> Self {
        Self {
            bot_token_env: "FLOCK_BOT_TOKEN".to_string(),
            webhook_port: 8456,
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Twist API v3 channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TwistConfig {
    /// Env var name holding the API token.
    pub token_env: String,
    /// Workspace ID.
    pub workspace_id: String,
    /// Channel IDs to listen in (empty = all).
    pub allowed_channels: Vec<String>,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for TwistConfig {
    fn default() -> Self {
        Self {
            token_env: "TWIST_TOKEN".to_string(),
            workspace_id: String::new(),
            allowed_channels: vec![],
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

// ── Wave 5 channel configs ─────────────────────────────────────────

/// Mumble text chat channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MumbleConfig {
    /// Mumble server hostname.
    pub host: String,
    /// Mumble server port.
    pub port: u16,
    /// Bot username.
    pub username: String,
    /// Env var name holding the server password.
    pub password_env: String,
    /// Channel to join.
    pub channel: String,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for MumbleConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 64738,
            username: "openfang".to_string(),
            password_env: "MUMBLE_PASSWORD".to_string(),
            channel: String::new(),
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// DingTalk Robot API channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DingTalkConfig {
    /// Env var name holding the webhook access token.
    pub access_token_env: String,
    /// Env var name holding the signing secret.
    pub secret_env: String,
    /// Port for the incoming webhook.
    pub webhook_port: u16,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for DingTalkConfig {
    fn default() -> Self {
        Self {
            access_token_env: "DINGTALK_ACCESS_TOKEN".to_string(),
            secret_env: "DINGTALK_SECRET".to_string(),
            webhook_port: 8457,
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Discourse forum channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DiscourseConfig {
    /// Discourse base URL.
    pub base_url: String,
    /// Env var name holding the API key.
    pub api_key_env: String,
    /// API username.
    pub api_username: String,
    /// Category slugs to monitor.
    pub categories: Vec<String>,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for DiscourseConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            api_key_env: "DISCOURSE_API_KEY".to_string(),
            api_username: "system".to_string(),
            categories: vec![],
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Gitter Streaming API channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GitterConfig {
    /// Env var name holding the auth token.
    pub token_env: String,
    /// Room ID to listen in.
    pub room_id: String,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for GitterConfig {
    fn default() -> Self {
        Self {
            token_env: "GITTER_TOKEN".to_string(),
            room_id: String::new(),
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// ntfy.sh pub/sub channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NtfyConfig {
    /// ntfy server URL.
    pub server_url: String,
    /// Topic to subscribe/publish to.
    pub topic: String,
    /// Env var name holding the auth token (optional for public topics).
    pub token_env: String,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for NtfyConfig {
    fn default() -> Self {
        Self {
            server_url: "https://ntfy.sh".to_string(),
            topic: String::new(),
            token_env: "NTFY_TOKEN".to_string(),
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Gotify WebSocket channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GotifyConfig {
    /// Gotify server URL.
    pub server_url: String,
    /// Env var name holding the app token (for sending).
    pub app_token_env: String,
    /// Env var name holding the client token (for receiving).
    pub client_token_env: String,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for GotifyConfig {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            app_token_env: "GOTIFY_APP_TOKEN".to_string(),
            client_token_env: "GOTIFY_CLIENT_TOKEN".to_string(),
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// Generic webhook channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WebhookConfig {
    /// Env var name holding the HMAC signing secret.
    pub secret_env: String,
    /// Port to listen for incoming webhooks.
    pub listen_port: u16,
    /// URL to POST outgoing messages to.
    pub callback_url: Option<String>,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            secret_env: "WEBHOOK_SECRET".to_string(),
            listen_port: 8460,
            callback_url: None,
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

/// LinkedIn Messaging API channel adapter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LinkedInConfig {
    /// Env var name holding the OAuth2 access token.
    pub access_token_env: String,
    /// Organization ID for messaging.
    pub organization_id: String,
    /// Default agent name to route messages to.
    pub default_agent: Option<String>,
    /// Per-channel behavior overrides.
    #[serde(default)]
    pub overrides: ChannelOverrides,
}

impl Default for LinkedInConfig {
    fn default() -> Self {
        Self {
            access_token_env: "LINKEDIN_ACCESS_TOKEN".to_string(),
            organization_id: String::new(),
            default_agent: None,
            overrides: ChannelOverrides::default(),
        }
    }
}

impl KernelConfig {
    /// Validate the configuration, returning a list of warnings.
    ///
    /// Checks that env vars referenced by configured channels are set.
    pub fn validate(&self) -> Vec<String> {
        let mut warnings = Vec::new();

        if let Some(ref tg) = self.channels.telegram {
            if std::env::var(&tg.bot_token_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Telegram configured but {} is not set",
                    tg.bot_token_env
                ));
            }
        }
        if let Some(ref dc) = self.channels.discord {
            if std::env::var(&dc.bot_token_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Discord configured but {} is not set",
                    dc.bot_token_env
                ));
            }
        }
        if let Some(ref sl) = self.channels.slack {
            if std::env::var(&sl.app_token_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Slack configured but {} is not set",
                    sl.app_token_env
                ));
            }
            if std::env::var(&sl.bot_token_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Slack configured but {} is not set",
                    sl.bot_token_env
                ));
            }
        }
        if let Some(ref wa) = self.channels.whatsapp {
            if std::env::var(&wa.access_token_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "WhatsApp configured but {} is not set",
                    wa.access_token_env
                ));
            }
        }
        if let Some(ref mx) = self.channels.matrix {
            if std::env::var(&mx.access_token_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Matrix configured but {} is not set",
                    mx.access_token_env
                ));
            }
        }
        if let Some(ref em) = self.channels.email {
            if std::env::var(&em.password_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Email configured but {} is not set",
                    em.password_env
                ));
            }
        }
        if let Some(ref t) = self.channels.teams {
            if std::env::var(&t.app_password_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Teams configured but {} is not set",
                    t.app_password_env
                ));
            }
        }
        if let Some(ref m) = self.channels.mattermost {
            if std::env::var(&m.token_env).unwrap_or_default().is_empty() {
                warnings.push(format!(
                    "Mattermost configured but {} is not set",
                    m.token_env
                ));
            }
        }
        if let Some(ref z) = self.channels.zulip {
            if std::env::var(&z.api_key_env).unwrap_or_default().is_empty() {
                warnings.push(format!("Zulip configured but {} is not set", z.api_key_env));
            }
        }
        if let Some(ref tw) = self.channels.twitch {
            if std::env::var(&tw.oauth_token_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Twitch configured but {} is not set",
                    tw.oauth_token_env
                ));
            }
        }
        if let Some(ref rc) = self.channels.rocketchat {
            if std::env::var(&rc.token_env).unwrap_or_default().is_empty() {
                warnings.push(format!(
                    "Rocket.Chat configured but {} is not set",
                    rc.token_env
                ));
            }
        }
        if let Some(ref gc) = self.channels.google_chat {
            if std::env::var(&gc.service_account_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Google Chat configured but {} is not set",
                    gc.service_account_env
                ));
            }
        }
        if let Some(ref x) = self.channels.xmpp {
            if std::env::var(&x.password_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!("XMPP configured but {} is not set", x.password_env));
            }
        }
        // Wave 3 channels
        if let Some(ref ln) = self.channels.line {
            if std::env::var(&ln.access_token_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "LINE configured but {} is not set",
                    ln.access_token_env
                ));
            }
        }
        if let Some(ref vb) = self.channels.viber {
            if std::env::var(&vb.auth_token_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Viber configured but {} is not set",
                    vb.auth_token_env
                ));
            }
        }
        if let Some(ref ms) = self.channels.messenger {
            if std::env::var(&ms.page_token_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Messenger configured but {} is not set",
                    ms.page_token_env
                ));
            }
        }
        if let Some(ref rd) = self.channels.reddit {
            if std::env::var(&rd.client_secret_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Reddit configured but {} is not set",
                    rd.client_secret_env
                ));
            }
        }
        if let Some(ref md) = self.channels.mastodon {
            if std::env::var(&md.access_token_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Mastodon configured but {} is not set",
                    md.access_token_env
                ));
            }
        }
        if let Some(ref bs) = self.channels.bluesky {
            if std::env::var(&bs.app_password_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Bluesky configured but {} is not set",
                    bs.app_password_env
                ));
            }
        }
        if let Some(ref fs) = self.channels.feishu {
            if std::env::var(&fs.app_secret_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Feishu configured but {} is not set",
                    fs.app_secret_env
                ));
            }
        }
        if let Some(ref rv) = self.channels.revolt {
            if std::env::var(&rv.bot_token_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Revolt configured but {} is not set",
                    rv.bot_token_env
                ));
            }
        }
        // Wave 4 channels
        if let Some(ref nc) = self.channels.nextcloud {
            if std::env::var(&nc.token_env).unwrap_or_default().is_empty() {
                warnings.push(format!(
                    "Nextcloud configured but {} is not set",
                    nc.token_env
                ));
            }
        }
        if let Some(ref gd) = self.channels.guilded {
            if std::env::var(&gd.bot_token_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Guilded configured but {} is not set",
                    gd.bot_token_env
                ));
            }
        }
        if let Some(ref kb) = self.channels.keybase {
            if std::env::var(&kb.paperkey_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Keybase configured but {} is not set",
                    kb.paperkey_env
                ));
            }
        }
        if let Some(ref tm) = self.channels.threema {
            if std::env::var(&tm.secret_env).unwrap_or_default().is_empty() {
                warnings.push(format!(
                    "Threema configured but {} is not set",
                    tm.secret_env
                ));
            }
        }
        if let Some(ref ns) = self.channels.nostr {
            if std::env::var(&ns.private_key_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Nostr configured but {} is not set",
                    ns.private_key_env
                ));
            }
        }
        if let Some(ref wx) = self.channels.webex {
            if std::env::var(&wx.bot_token_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Webex configured but {} is not set",
                    wx.bot_token_env
                ));
            }
        }
        if let Some(ref pb) = self.channels.pumble {
            if std::env::var(&pb.bot_token_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Pumble configured but {} is not set",
                    pb.bot_token_env
                ));
            }
        }
        if let Some(ref fl) = self.channels.flock {
            if std::env::var(&fl.bot_token_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Flock configured but {} is not set",
                    fl.bot_token_env
                ));
            }
        }
        if let Some(ref tw) = self.channels.twist {
            if std::env::var(&tw.token_env).unwrap_or_default().is_empty() {
                warnings.push(format!("Twist configured but {} is not set", tw.token_env));
            }
        }
        // Wave 5 channels
        if let Some(ref mb) = self.channels.mumble {
            if std::env::var(&mb.password_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Mumble configured but {} is not set",
                    mb.password_env
                ));
            }
        }
        if let Some(ref dt) = self.channels.dingtalk {
            if std::env::var(&dt.access_token_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "DingTalk configured but {} is not set",
                    dt.access_token_env
                ));
            }
        }
        if let Some(ref dc) = self.channels.discourse {
            if std::env::var(&dc.api_key_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Discourse configured but {} is not set",
                    dc.api_key_env
                ));
            }
        }
        if let Some(ref gt) = self.channels.gitter {
            if std::env::var(&gt.token_env).unwrap_or_default().is_empty() {
                warnings.push(format!("Gitter configured but {} is not set", gt.token_env));
            }
        }
        if let Some(ref nf) = self.channels.ntfy {
            if !nf.token_env.is_empty()
                && std::env::var(&nf.token_env).unwrap_or_default().is_empty()
            {
                warnings.push(format!("ntfy configured but {} is not set", nf.token_env));
            }
        }
        if let Some(ref gf) = self.channels.gotify {
            if std::env::var(&gf.app_token_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "Gotify configured but {} is not set",
                    gf.app_token_env
                ));
            }
        }
        if let Some(ref wh) = self.channels.webhook {
            if std::env::var(&wh.secret_env).unwrap_or_default().is_empty() {
                warnings.push(format!(
                    "Webhook configured but {} is not set",
                    wh.secret_env
                ));
            }
        }
        if let Some(ref li) = self.channels.linkedin {
            if std::env::var(&li.access_token_env)
                .unwrap_or_default()
                .is_empty()
            {
                warnings.push(format!(
                    "LinkedIn configured but {} is not set",
                    li.access_token_env
                ));
            }
        }

        // Web search provider validation
        match self.web.search_provider {
            SearchProvider::Brave => {
                if std::env::var(&self.web.brave.api_key_env)
                    .unwrap_or_default()
                    .is_empty()
                {
                    warnings.push(format!(
                        "Brave search selected but {} is not set",
                        self.web.brave.api_key_env
                    ));
                }
            }
            SearchProvider::Tavily => {
                if std::env::var(&self.web.tavily.api_key_env)
                    .unwrap_or_default()
                    .is_empty()
                {
                    warnings.push(format!(
                        "Tavily search selected but {} is not set",
                        self.web.tavily.api_key_env
                    ));
                }
            }
            SearchProvider::Perplexity => {
                if std::env::var(&self.web.perplexity.api_key_env)
                    .unwrap_or_default()
                    .is_empty()
                {
                    warnings.push(format!(
                        "Perplexity search selected but {} is not set",
                        self.web.perplexity.api_key_env
                    ));
                }
            }
            SearchProvider::DuckDuckGo | SearchProvider::Auto => {}
        }

        // --- Production bounds validation ---
        // Clamp dangerous zero/extreme values to safe defaults instead of crashing.
        warnings
    }

    /// Clamp configuration values to safe production bounds.
    ///
    /// Called after loading config to prevent zero timeouts, unbounded buffers,
    /// or other misconfigurations that cause silent failures at runtime.
    pub fn clamp_bounds(&mut self) {
        // Browser timeout: min 5s, max 300s
        if self.browser.timeout_secs == 0 {
            self.browser.timeout_secs = 30;
        } else if self.browser.timeout_secs > 300 {
            self.browser.timeout_secs = 300;
        }

        // Browser max sessions: min 1, max 100
        if self.browser.max_sessions == 0 {
            self.browser.max_sessions = 3;
        } else if self.browser.max_sessions > 100 {
            self.browser.max_sessions = 100;
        }

        // Web fetch max_response_bytes: min 1KB, max 50MB
        if self.web.fetch.max_response_bytes == 0 {
            self.web.fetch.max_response_bytes = 5_000_000;
        } else if self.web.fetch.max_response_bytes > 50_000_000 {
            self.web.fetch.max_response_bytes = 50_000_000;
        }

        // Web fetch timeout: min 5s, max 120s
        if self.web.fetch.timeout_secs == 0 {
            self.web.fetch.timeout_secs = 30;
        } else if self.web.fetch.timeout_secs > 120 {
            self.web.fetch.timeout_secs = 120;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = KernelConfig::default();
        assert_eq!(config.log_level, "info");
        assert_eq!(config.api_listen, "127.0.0.1:50051");
        assert!(!config.network_enabled);
    }

    #[test]
    fn test_config_serialization() {
        let config = KernelConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(toml_str.contains("log_level"));
    }

    #[test]
    fn test_discord_config_defaults() {
        let dc = DiscordConfig::default();
        assert_eq!(dc.bot_token_env, "DISCORD_BOT_TOKEN");
        assert!(dc.allowed_guilds.is_empty());
        assert_eq!(dc.intents, 33280);
    }

    #[test]
    fn test_slack_config_defaults() {
        let sl = SlackConfig::default();
        assert_eq!(sl.app_token_env, "SLACK_APP_TOKEN");
        assert_eq!(sl.bot_token_env, "SLACK_BOT_TOKEN");
        assert!(sl.allowed_channels.is_empty());
    }

    #[test]
    fn test_validate_no_channels() {
        let config = KernelConfig::default();
        let warnings = config.validate();
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_kernel_mode_default() {
        let mode = KernelMode::default();
        assert_eq!(mode, KernelMode::Default);
    }

    #[test]
    fn test_kernel_mode_serde() {
        let stable = KernelMode::Stable;
        let json = serde_json::to_string(&stable).unwrap();
        assert_eq!(json, "\"stable\"");
        let back: KernelMode = serde_json::from_str(&json).unwrap();
        assert_eq!(back, KernelMode::Stable);
    }

    #[test]
    fn test_user_config_serde() {
        let uc = UserConfig {
            name: "Alice".to_string(),
            role: "owner".to_string(),
            channel_bindings: {
                let mut m = std::collections::HashMap::new();
                m.insert("telegram".to_string(), "123456".to_string());
                m
            },
            api_key_hash: None,
        };
        let json = serde_json::to_string(&uc).unwrap();
        let back: UserConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "Alice");
        assert_eq!(back.role, "owner");
        assert_eq!(back.channel_bindings.get("telegram").unwrap(), "123456");
    }

    #[test]
    fn test_config_with_mode_and_language() {
        let config = KernelConfig {
            mode: KernelMode::Stable,
            language: "ar".to_string(),
            ..Default::default()
        };
        assert_eq!(config.mode, KernelMode::Stable);
        assert_eq!(config.language, "ar");
    }

    #[test]
    fn test_validate_missing_env_vars() {
        let mut config = KernelConfig::default();
        config.channels.discord = Some(DiscordConfig {
            bot_token_env: "OPENFANG_TEST_NONEXISTENT_VAR_DC".to_string(),
            ..Default::default()
        });
        let warnings = config.validate();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("Discord"));
    }

    #[test]
    fn test_whatsapp_config_defaults() {
        let wa = WhatsAppConfig::default();
        assert_eq!(wa.access_token_env, "WHATSAPP_ACCESS_TOKEN");
        assert_eq!(wa.webhook_port, 8443);
        assert!(wa.allowed_users.is_empty());
    }

    #[test]
    fn test_signal_config_defaults() {
        let sig = SignalConfig::default();
        assert_eq!(sig.api_url, "http://localhost:8080");
        assert!(sig.phone_number.is_empty());
    }

    #[test]
    fn test_matrix_config_defaults() {
        let mx = MatrixConfig::default();
        assert_eq!(mx.homeserver_url, "https://matrix.org");
        assert_eq!(mx.access_token_env, "MATRIX_ACCESS_TOKEN");
        assert!(mx.allowed_rooms.is_empty());
    }

    #[test]
    fn test_email_config_defaults() {
        let em = EmailConfig::default();
        assert_eq!(em.imap_port, 993);
        assert_eq!(em.smtp_port, 587);
        assert_eq!(em.password_env, "EMAIL_PASSWORD");
        assert_eq!(em.folders, vec!["INBOX".to_string()]);
    }

    #[test]
    fn test_whatsapp_config_serde() {
        let wa = WhatsAppConfig {
            phone_number_id: "12345".to_string(),
            ..Default::default()
        };
        let json = serde_json::to_string(&wa).unwrap();
        let back: WhatsAppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.phone_number_id, "12345");
    }

    #[test]
    fn test_matrix_config_serde() {
        let mx = MatrixConfig {
            user_id: "@bot:matrix.org".to_string(),
            ..Default::default()
        };
        let json = serde_json::to_string(&mx).unwrap();
        let back: MatrixConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.user_id, "@bot:matrix.org");
    }

    #[test]
    fn test_channels_config_with_new_channels() {
        let config = KernelConfig {
            channels: ChannelsConfig {
                whatsapp: Some(WhatsAppConfig::default()),
                signal: Some(SignalConfig::default()),
                matrix: Some(MatrixConfig::default()),
                email: Some(EmailConfig::default()),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(config.channels.whatsapp.is_some());
        assert!(config.channels.signal.is_some());
        assert!(config.channels.matrix.is_some());
        assert!(config.channels.email.is_some());
    }

    #[test]
    fn test_teams_config_defaults() {
        let t = TeamsConfig::default();
        assert_eq!(t.app_password_env, "TEAMS_APP_PASSWORD");
        assert_eq!(t.webhook_port, 3978);
        assert!(t.allowed_tenants.is_empty());
    }

    #[test]
    fn test_mattermost_config_defaults() {
        let m = MattermostConfig::default();
        assert_eq!(m.token_env, "MATTERMOST_TOKEN");
        assert!(m.server_url.is_empty());
    }

    #[test]
    fn test_irc_config_defaults() {
        let irc = IrcConfig::default();
        assert_eq!(irc.server, "irc.libera.chat");
        assert_eq!(irc.port, 6667);
        assert_eq!(irc.nick, "openfang");
        assert!(!irc.use_tls);
    }

    #[test]
    fn test_google_chat_config_defaults() {
        let gc = GoogleChatConfig::default();
        assert_eq!(gc.service_account_env, "GOOGLE_CHAT_SERVICE_ACCOUNT");
        assert_eq!(gc.webhook_port, 8444);
    }

    #[test]
    fn test_twitch_config_defaults() {
        let tw = TwitchConfig::default();
        assert_eq!(tw.oauth_token_env, "TWITCH_OAUTH_TOKEN");
        assert_eq!(tw.nick, "openfang");
    }

    #[test]
    fn test_rocketchat_config_defaults() {
        let rc = RocketChatConfig::default();
        assert_eq!(rc.token_env, "ROCKETCHAT_TOKEN");
        assert!(rc.server_url.is_empty());
    }

    #[test]
    fn test_zulip_config_defaults() {
        let z = ZulipConfig::default();
        assert_eq!(z.api_key_env, "ZULIP_API_KEY");
        assert!(z.bot_email.is_empty());
    }

    #[test]
    fn test_xmpp_config_defaults() {
        let x = XmppConfig::default();
        assert_eq!(x.password_env, "XMPP_PASSWORD");
        assert_eq!(x.port, 5222);
        assert!(x.rooms.is_empty());
    }

    #[test]
    fn test_all_new_channel_configs_serde() {
        let config = KernelConfig {
            channels: ChannelsConfig {
                teams: Some(TeamsConfig::default()),
                mattermost: Some(MattermostConfig::default()),
                irc: Some(IrcConfig::default()),
                google_chat: Some(GoogleChatConfig::default()),
                twitch: Some(TwitchConfig::default()),
                rocketchat: Some(RocketChatConfig::default()),
                zulip: Some(ZulipConfig::default()),
                xmpp: Some(XmppConfig::default()),
                ..Default::default()
            },
            ..Default::default()
        };
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let back: KernelConfig = toml::from_str(&toml_str).unwrap();
        assert!(back.channels.teams.is_some());
        assert!(back.channels.mattermost.is_some());
        assert!(back.channels.irc.is_some());
        assert!(back.channels.google_chat.is_some());
        assert!(back.channels.twitch.is_some());
        assert!(back.channels.rocketchat.is_some());
        assert!(back.channels.zulip.is_some());
        assert!(back.channels.xmpp.is_some());
    }

    #[test]
    fn test_channel_overrides_defaults() {
        let ov = ChannelOverrides::default();
        assert_eq!(ov.dm_policy, DmPolicy::Respond);
        assert_eq!(ov.group_policy, GroupPolicy::MentionOnly);
        assert_eq!(ov.rate_limit_per_user, 0);
        assert!(!ov.threading);
        assert!(ov.output_format.is_none());
        assert!(ov.model.is_none());
    }

    #[test]
    fn test_fallback_config_serde_roundtrip() {
        let fb = FallbackProviderConfig {
            provider: "ollama".to_string(),
            model: "llama3.2:latest".to_string(),
            api_key_env: String::new(),
            base_url: None,
        };
        let json = serde_json::to_string(&fb).unwrap();
        let back: FallbackProviderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.provider, "ollama");
        assert_eq!(back.model, "llama3.2:latest");
        assert!(back.api_key_env.is_empty());
        assert!(back.base_url.is_none());
    }

    #[test]
    fn test_fallback_config_default_empty() {
        let config = KernelConfig::default();
        assert!(config.fallback_providers.is_empty());
    }

    #[test]
    fn test_fallback_config_in_toml() {
        let toml_str = r#"
            [[fallback_providers]]
            provider = "ollama"
            model = "llama3.2:latest"

            [[fallback_providers]]
            provider = "groq"
            model = "llama-3.3-70b-versatile"
            api_key_env = "GROQ_API_KEY"
        "#;
        let config: KernelConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.fallback_providers.len(), 2);
        assert_eq!(config.fallback_providers[0].provider, "ollama");
        assert_eq!(config.fallback_providers[1].provider, "groq");
    }

    #[test]
    fn test_channel_overrides_serde() {
        let ov = ChannelOverrides {
            dm_policy: DmPolicy::Ignore,
            group_policy: GroupPolicy::CommandsOnly,
            rate_limit_per_user: 10,
            threading: true,
            output_format: Some(OutputFormat::TelegramHtml),
            ..Default::default()
        };
        let json = serde_json::to_string(&ov).unwrap();
        let back: ChannelOverrides = serde_json::from_str(&json).unwrap();
        assert_eq!(back.dm_policy, DmPolicy::Ignore);
        assert_eq!(back.group_policy, GroupPolicy::CommandsOnly);
        assert_eq!(back.rate_limit_per_user, 10);
        assert!(back.threading);
        assert_eq!(back.output_format, Some(OutputFormat::TelegramHtml));
    }

    #[test]
    fn test_clamp_bounds_zero_browser_timeout() {
        let mut config = KernelConfig::default();
        config.browser.timeout_secs = 0;
        config.clamp_bounds();
        assert_eq!(config.browser.timeout_secs, 30);
    }

    #[test]
    fn test_clamp_bounds_excessive_browser_sessions() {
        let mut config = KernelConfig::default();
        config.browser.max_sessions = 999;
        config.clamp_bounds();
        assert_eq!(config.browser.max_sessions, 100);
    }

    #[test]
    fn test_clamp_bounds_zero_fetch_bytes() {
        let mut config = KernelConfig::default();
        config.web.fetch.max_response_bytes = 0;
        config.clamp_bounds();
        assert_eq!(config.web.fetch.max_response_bytes, 5_000_000);
    }

    #[test]
    fn test_clamp_bounds_zero_fetch_timeout() {
        let mut config = KernelConfig::default();
        config.web.fetch.timeout_secs = 0;
        config.clamp_bounds();
        assert_eq!(config.web.fetch.timeout_secs, 30);
    }

    #[test]
    fn test_clamp_bounds_defaults_unchanged() {
        let mut config = KernelConfig::default();
        let browser_timeout = config.browser.timeout_secs;
        let browser_sessions = config.browser.max_sessions;
        let fetch_bytes = config.web.fetch.max_response_bytes;
        let fetch_timeout = config.web.fetch.timeout_secs;
        config.clamp_bounds();
        assert_eq!(config.browser.timeout_secs, browser_timeout);
        assert_eq!(config.browser.max_sessions, browser_sessions);
        assert_eq!(config.web.fetch.max_response_bytes, fetch_bytes);
        assert_eq!(config.web.fetch.timeout_secs, fetch_timeout);
    }
}
