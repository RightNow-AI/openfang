//! Agent-related types: identity, manifests, state, and scheduling.

use crate::tool::ToolDefinition;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

/// Unique identifier for a user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(pub Uuid);

impl UserId {
    /// Generate a new random UserId.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for UserId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for UserId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Model routing configuration — auto-selects cheap/mid/expensive models by complexity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelRoutingConfig {
    /// Model to use for simple queries.
    pub simple_model: String,
    /// Model to use for medium-complexity queries.
    pub medium_model: String,
    /// Model to use for complex queries.
    pub complex_model: String,
    /// Token count threshold: below this = simple.
    pub simple_threshold: u32,
    /// Token count threshold: above this = complex.
    pub complex_threshold: u32,
}

impl Default for ModelRoutingConfig {
    fn default() -> Self {
        Self {
            simple_model: "claude-haiku-4-5-20251001".to_string(),
            medium_model: "claude-sonnet-4-20250514".to_string(),
            complex_model: "claude-sonnet-4-20250514".to_string(),
            simple_threshold: 100,
            complex_threshold: 500,
        }
    }
}

/// Autonomous agent configuration — guardrails for 24/7 agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutonomousConfig {
    /// Cron expression for quiet hours (e.g., "0 22 * * *" to "0 6 * * *").
    pub quiet_hours: Option<String>,
    /// Maximum iterations per invocation (overrides global MAX_ITERATIONS).
    pub max_iterations: u32,
    /// Maximum restarts before the agent is permanently stopped.
    pub max_restarts: u32,
    /// Heartbeat interval in seconds.
    pub heartbeat_interval_secs: u64,
    /// Channel to send heartbeat status to (e.g., "telegram", "discord").
    pub heartbeat_channel: Option<String>,
}

impl Default for AutonomousConfig {
    fn default() -> Self {
        Self {
            quiet_hours: None,
            max_iterations: 50,
            max_restarts: 10,
            heartbeat_interval_secs: 30,
            heartbeat_channel: None,
        }
    }
}

/// Hook event types that can be intercepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    /// Fires before a tool call is executed. Handler can block the call.
    BeforeToolCall,
    /// Fires after a tool call completes.
    AfterToolCall,
    /// Fires before the system prompt is constructed.
    BeforePromptBuild,
    /// Fires after the agent loop completes.
    AgentLoopEnd,
}

/// Unique identifier for an agent instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub Uuid);

impl AgentId {
    /// Generate a new random AgentId.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for AgentId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Unique identifier for a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub Uuid);

impl SessionId {
    /// Create a new random SessionId.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The current lifecycle state of an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    /// Agent has been created but not yet started.
    Created,
    /// Agent is actively running and processing events.
    Running,
    /// Agent is paused and not processing events.
    Suspended,
    /// Agent has been terminated and cannot be resumed.
    Terminated,
    /// Agent crashed and is awaiting recovery.
    Crashed,
}

/// Permission-based operational mode for an agent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentMode {
    /// Read-only: agent can observe but cannot call any tools.
    Observe,
    /// Restricted: agent can only call read-only tools (file_read, file_list, memory_recall, web_fetch, web_search).
    Assist,
    /// Unrestricted: agent can use all granted tools.
    #[default]
    Full,
}

impl AgentMode {
    /// Filter a tool list based on this mode.
    pub fn filter_tools(&self, tools: Vec<ToolDefinition>) -> Vec<ToolDefinition> {
        match self {
            Self::Observe => vec![],
            Self::Assist => {
                let read_only = [
                    "file_read",
                    "file_list",
                    "memory_recall",
                    "web_fetch",
                    "web_search",
                    "agent_list",
                ];
                tools
                    .into_iter()
                    .filter(|t| read_only.contains(&t.name.as_str()))
                    .collect()
            }
            Self::Full => tools,
        }
    }
}

/// How an agent is scheduled to run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleMode {
    /// Agent wakes up when a message/event arrives (default).
    #[default]
    Reactive,
    /// Agent wakes up on a cron schedule.
    Periodic { cron: String },
    /// Agent monitors conditions and acts when thresholds are met.
    Proactive { conditions: Vec<String> },
    /// Agent runs in a persistent loop.
    Continuous {
        #[serde(default = "default_check_interval")]
        check_interval_secs: u64,
    },
}

fn default_check_interval() -> u64 {
    60
}

/// Resource limits for an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ResourceQuota {
    /// Maximum WASM memory in bytes.
    pub max_memory_bytes: u64,
    /// Maximum CPU time per invocation in milliseconds.
    pub max_cpu_time_ms: u64,
    /// Maximum tool calls per minute.
    pub max_tool_calls_per_minute: u32,
    /// Maximum LLM tokens per hour.
    pub max_llm_tokens_per_hour: u64,
    /// Maximum network bytes per hour.
    pub max_network_bytes_per_hour: u64,
    /// Maximum cost in USD per hour.
    pub max_cost_per_hour_usd: f64,
    /// Maximum cost in USD per day (0.0 = unlimited).
    pub max_cost_per_day_usd: f64,
    /// Maximum cost in USD per month (0.0 = unlimited).
    pub max_cost_per_month_usd: f64,
}

impl Default for ResourceQuota {
    fn default() -> Self {
        Self {
            max_memory_bytes: 256 * 1024 * 1024, // 256 MB
            max_cpu_time_ms: 30_000,             // 30 seconds
            max_tool_calls_per_minute: 60,
            max_llm_tokens_per_hour: 1_000_000,
            max_network_bytes_per_hour: 100 * 1024 * 1024, // 100 MB
            max_cost_per_hour_usd: 1.0,
            max_cost_per_day_usd: 0.0,   // unlimited
            max_cost_per_month_usd: 0.0, // unlimited
        }
    }
}

/// Agent priority level for scheduling.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    /// Low priority.
    Low = 0,
    /// Normal priority (default).
    #[default]
    Normal = 1,
    /// High priority.
    High = 2,
    /// Critical priority.
    Critical = 3,
}

/// Named tool presets — expand to tool lists + derived capabilities.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolProfile {
    Minimal,
    Coding,
    Research,
    Messaging,
    Automation,
    #[default]
    Full,
    Custom,
}

impl ToolProfile {
    /// Expand profile to tool name list.
    pub fn tools(&self) -> Vec<String> {
        match self {
            Self::Minimal => vec!["file_read", "file_list"],
            Self::Coding => vec![
                "file_read",
                "file_write",
                "file_list",
                "shell_exec",
                "web_fetch",
            ],
            Self::Research => vec!["web_fetch", "web_search", "file_read", "file_write"],
            Self::Messaging => vec!["agent_send", "agent_list", "memory_store", "memory_recall"],
            Self::Automation => vec![
                "file_read",
                "file_write",
                "file_list",
                "shell_exec",
                "web_fetch",
                "web_search",
                "agent_send",
                "agent_list",
                "memory_store",
                "memory_recall",
            ],
            Self::Full | Self::Custom => vec!["*"],
        }
        .into_iter()
        .map(String::from)
        .collect()
    }

    /// Derive ManifestCapabilities implied by this profile.
    pub fn implied_capabilities(&self) -> ManifestCapabilities {
        let tools = self.tools();
        let has_net = tools.iter().any(|t| t.starts_with("web_") || t == "*");
        let has_shell = tools.iter().any(|t| t == "shell_exec" || t == "*");
        let has_agent = tools.iter().any(|t| t.starts_with("agent_") || t == "*");
        let has_memory = tools.iter().any(|t| t.starts_with("memory_") || t == "*");
        ManifestCapabilities {
            tools,
            network: if has_net { vec!["*".into()] } else { vec![] },
            shell: if has_shell { vec!["*".into()] } else { vec![] },
            agent_spawn: has_agent,
            agent_message: if has_agent { vec!["*".into()] } else { vec![] },
            memory_read: if has_memory {
                vec!["*".into()]
            } else {
                vec!["self.*".into()]
            },
            memory_write: vec!["self.*".into()],
            ofp_discover: false,
            ofp_connect: vec![],
        }
    }
}

/// LLM model configuration for an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelConfig {
    /// LLM provider name.
    pub provider: String,
    /// Model identifier.
    pub model: String,
    /// Maximum tokens for completion.
    pub max_tokens: u32,
    /// Sampling temperature.
    pub temperature: f32,
    /// System prompt for the agent.
    pub system_prompt: String,
    /// Optional API key environment variable name.
    pub api_key_env: Option<String>,
    /// Optional base URL override for the provider.
    pub base_url: Option<String>,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
            system_prompt: "You are a helpful AI agent.".to_string(),
            api_key_env: None,
            base_url: None,
        }
    }
}

/// A fallback model entry in a chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackModel {
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
}

/// Tool configuration within an agent manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfig {
    /// Tool-specific configuration parameters.
    pub params: HashMap<String, serde_json::Value>,
}

/// Complete agent manifest — defines everything about an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentManifest {
    /// Human-readable agent name.
    pub name: String,
    /// Semantic version.
    pub version: String,
    /// Description of what this agent does.
    pub description: String,
    /// Author identifier.
    pub author: String,
    /// Path to the agent module (WASM or Python file).
    pub module: String,
    /// Scheduling mode.
    pub schedule: ScheduleMode,
    /// LLM model configuration.
    pub model: ModelConfig,
    /// Fallback model chain — tried in order if the primary model fails.
    #[serde(default, deserialize_with = "crate::serde_compat::vec_lenient")]
    pub fallback_models: Vec<FallbackModel>,
    /// Resource quotas.
    pub resources: ResourceQuota,
    /// Priority level.
    pub priority: Priority,
    /// Capability grants (parsed into Capability enum by kernel).
    pub capabilities: ManifestCapabilities,
    /// Named tool profile — expands to tool list + derived capabilities.
    #[serde(default)]
    pub profile: Option<ToolProfile>,
    /// Tool-specific configurations.
    #[serde(default, deserialize_with = "crate::serde_compat::map_lenient")]
    pub tools: HashMap<String, ToolConfig>,
    /// Installed skill references (empty = all skills available).
    #[serde(default, deserialize_with = "crate::serde_compat::vec_lenient")]
    pub skills: Vec<String>,
    /// MCP server allowlist (empty = all connected MCP servers available).
    #[serde(default, deserialize_with = "crate::serde_compat::vec_lenient")]
    pub mcp_servers: Vec<String>,
    /// Custom metadata.
    #[serde(default, deserialize_with = "crate::serde_compat::map_lenient")]
    pub metadata: HashMap<String, serde_json::Value>,
    /// Tags for agent discovery and categorization.
    #[serde(default, deserialize_with = "crate::serde_compat::vec_lenient")]
    pub tags: Vec<String>,
    /// Model routing configuration — auto-select models by complexity.
    #[serde(default)]
    pub routing: Option<ModelRoutingConfig>,
    /// Autonomous agent configuration — guardrails for 24/7 agents.
    #[serde(default)]
    pub autonomous: Option<AutonomousConfig>,
    /// Pinned model override (used in Stable mode).
    #[serde(default)]
    pub pinned_model: Option<String>,
    /// Agent workspace directory. Auto-created on spawn.
    /// Default: `{workspaces_dir}/{agent_name}-{agent_id_prefix}/`
    #[serde(default)]
    pub workspace: Option<PathBuf>,
    /// Whether to generate workspace identity files (SOUL.md, USER.md, etc.) on creation.
    #[serde(default = "default_true")]
    pub generate_identity_files: bool,
    /// Per-agent exec policy override. If None, uses global exec_policy.
    #[serde(default)]
    pub exec_policy: Option<crate::config::ExecPolicy>,
    /// Tool allowlist — only these tools are available (empty = all tools).
    #[serde(default, deserialize_with = "crate::serde_compat::vec_lenient")]
    pub tool_allowlist: Vec<String>,
    /// Tool blocklist — these tools are excluded (applied after allowlist).
    #[serde(default, deserialize_with = "crate::serde_compat::vec_lenient")]
    pub tool_blocklist: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl Default for AgentManifest {
    fn default() -> Self {
        Self {
            name: "unnamed".to_string(),
            version: "0.1.0".to_string(),
            description: String::new(),
            author: String::new(),
            module: "builtin:chat".to_string(),
            schedule: ScheduleMode::default(),
            model: ModelConfig::default(),
            fallback_models: Vec::new(),
            resources: ResourceQuota::default(),
            priority: Priority::default(),
            capabilities: ManifestCapabilities::default(),
            profile: None,
            tools: HashMap::new(),
            skills: Vec::new(),
            mcp_servers: Vec::new(),
            metadata: HashMap::new(),
            tags: Vec::new(),
            routing: None,
            autonomous: None,
            pinned_model: None,
            workspace: None,
            generate_identity_files: true,
            exec_policy: None,
            tool_allowlist: Vec::new(),
            tool_blocklist: Vec::new(),
        }
    }
}

/// Capability declarations in a manifest (human-readable TOML format).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ManifestCapabilities {
    /// Allowed network hosts (e.g., ["api.anthropic.com:443"]).
    #[serde(default, deserialize_with = "crate::serde_compat::vec_lenient")]
    pub network: Vec<String>,
    /// Allowed tool IDs.
    #[serde(default, deserialize_with = "crate::serde_compat::vec_lenient")]
    pub tools: Vec<String>,
    /// Memory read scopes.
    #[serde(default, deserialize_with = "crate::serde_compat::vec_lenient")]
    pub memory_read: Vec<String>,
    /// Memory write scopes.
    #[serde(default, deserialize_with = "crate::serde_compat::vec_lenient")]
    pub memory_write: Vec<String>,
    /// Whether this agent can spawn sub-agents.
    pub agent_spawn: bool,
    /// Agent message patterns (e.g., ["*"] or ["agent-name"]).
    #[serde(default, deserialize_with = "crate::serde_compat::vec_lenient")]
    pub agent_message: Vec<String>,
    /// Allowed shell commands.
    #[serde(default, deserialize_with = "crate::serde_compat::vec_lenient")]
    pub shell: Vec<String>,
    /// Whether this agent can discover remote agents via OFP.
    pub ofp_discover: bool,
    /// Allowed OFP peer patterns.
    #[serde(default, deserialize_with = "crate::serde_compat::vec_lenient")]
    pub ofp_connect: Vec<String>,
}

/// Human-readable session label (e.g., "support inbox", "research").
/// Max 128 chars, alphanumeric + spaces + hyphens + underscores only.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SessionLabel(String);

impl SessionLabel {
    /// Create a new validated session label.
    pub fn new(label: &str) -> Result<Self, crate::error::OpenFangError> {
        let trimmed = label.trim();
        if trimmed.is_empty() || trimmed.len() > 128 {
            return Err(crate::error::OpenFangError::InvalidInput(
                "Session label must be 1-128 chars".into(),
            ));
        }
        if !trimmed
            .chars()
            .all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_')
        {
            return Err(crate::error::OpenFangError::InvalidInput(
                "Session label contains invalid chars".into(),
            ));
        }
        Ok(Self(trimmed.to_string()))
    }

    /// Get the label as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SessionLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Visual identity for an agent — emoji, avatar, color, personality.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentIdentity {
    /// Single emoji character for quick visual identification.
    pub emoji: Option<String>,
    /// Avatar URL (http/https) or data URI.
    pub avatar_url: Option<String>,
    /// Hex color code (e.g., "#FF5C00") for UI accent.
    pub color: Option<String>,
    /// Archetype: "researcher", "coder", "assistant", "writer", "devops", "support", "analyst".
    pub archetype: Option<String>,
    /// Personality vibe: "professional", "friendly", "technical", "creative", "concise", "mentor".
    pub vibe: Option<String>,
    /// Greeting style: "warm", "formal", "playful", "brief".
    pub greeting_style: Option<String>,
}

/// A registered agent entry in the kernel's registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEntry {
    /// Unique agent ID.
    pub id: AgentId,
    /// Human-readable name.
    pub name: String,
    /// Full manifest.
    pub manifest: AgentManifest,
    /// Current lifecycle state.
    pub state: AgentState,
    /// Permission-based operational mode.
    #[serde(default)]
    pub mode: AgentMode,
    /// When the agent was created.
    pub created_at: DateTime<Utc>,
    /// When the agent was last active.
    pub last_active: DateTime<Utc>,
    /// Parent agent (if spawned by another agent).
    pub parent: Option<AgentId>,
    /// Child agents spawned by this agent.
    pub children: Vec<AgentId>,
    /// Active session ID.
    pub session_id: SessionId,
    /// Tags for categorization.
    pub tags: Vec<String>,
    /// Visual identity for dashboard display.
    #[serde(default)]
    pub identity: AgentIdentity,
    /// Whether onboarding (bootstrap) has been completed.
    #[serde(default)]
    pub onboarding_completed: bool,
    /// When onboarding was completed.
    #[serde(default)]
    pub onboarding_completed_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_id_uniqueness() {
        let id1 = AgentId::new();
        let id2 = AgentId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_agent_id_display() {
        let id = AgentId::new();
        let display = format!("{}", id);
        assert!(!display.is_empty());
        assert_eq!(display.len(), 36); // UUID v4 string length
    }

    #[test]
    fn test_agent_id_serialization() {
        let id = AgentId::new();
        let json = serde_json::to_string(&id).unwrap();
        let deserialized: AgentId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }

    #[test]
    fn test_default_resource_quota() {
        let quota = ResourceQuota::default();
        assert_eq!(quota.max_memory_bytes, 256 * 1024 * 1024);
        assert_eq!(quota.max_cpu_time_ms, 30_000);
    }

    #[test]
    fn test_user_id_uniqueness() {
        let u1 = UserId::new();
        let u2 = UserId::new();
        assert_ne!(u1, u2);
    }

    #[test]
    fn test_user_id_roundtrip() {
        let u = UserId::new();
        let json = serde_json::to_string(&u).unwrap();
        let back: UserId = serde_json::from_str(&json).unwrap();
        assert_eq!(u, back);
    }

    #[test]
    fn test_model_routing_config_defaults() {
        let cfg = ModelRoutingConfig::default();
        assert!(!cfg.simple_model.is_empty());
        assert!(cfg.simple_threshold < cfg.complex_threshold);
    }

    #[test]
    fn test_model_routing_config_serde() {
        let cfg = ModelRoutingConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let back: ModelRoutingConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.simple_model, cfg.simple_model);
    }

    #[test]
    fn test_autonomous_config_defaults() {
        let cfg = AutonomousConfig::default();
        assert_eq!(cfg.max_iterations, 50);
        assert_eq!(cfg.max_restarts, 10);
        assert_eq!(cfg.heartbeat_interval_secs, 30);
        assert!(cfg.quiet_hours.is_none());
    }

    #[test]
    fn test_autonomous_config_serde() {
        let cfg = AutonomousConfig {
            quiet_hours: Some("0 22 * * *".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: AutonomousConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.quiet_hours, Some("0 22 * * *".to_string()));
    }

    #[test]
    fn test_manifest_with_routing_and_autonomous() {
        let manifest = AgentManifest {
            routing: Some(ModelRoutingConfig::default()),
            autonomous: Some(AutonomousConfig::default()),
            pinned_model: Some("claude-sonnet-4-20250514".into()),
            ..Default::default()
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let back: AgentManifest = serde_json::from_str(&json).unwrap();
        assert!(back.routing.is_some());
        assert!(back.autonomous.is_some());
        assert_eq!(
            back.pinned_model,
            Some("claude-sonnet-4-20250514".to_string())
        );
    }

    #[test]
    fn test_agent_manifest_serialization() {
        let manifest = AgentManifest {
            name: "test-agent".to_string(),
            version: "0.1.0".to_string(),
            description: "A test agent".to_string(),
            author: "test".to_string(),
            module: "test.wasm".to_string(),
            schedule: ScheduleMode::default(),
            model: ModelConfig::default(),
            fallback_models: vec![],
            resources: ResourceQuota::default(),
            priority: Priority::default(),
            capabilities: ManifestCapabilities::default(),
            profile: None,
            tools: HashMap::new(),
            skills: vec![],
            mcp_servers: vec![],
            metadata: HashMap::new(),
            tags: vec!["test".to_string()],
            routing: None,
            autonomous: None,
            pinned_model: None,
            workspace: None,
            generate_identity_files: true,
            exec_policy: None,
            tool_allowlist: Vec::new(),
            tool_blocklist: Vec::new(),
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let deserialized: AgentManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "test-agent");
        assert_eq!(deserialized.tags, vec!["test".to_string()]);
    }

    // ----- ToolProfile tests -----

    #[test]
    fn test_tool_profile_minimal() {
        let tools = ToolProfile::Minimal.tools();
        assert_eq!(tools, vec!["file_read", "file_list"]);
    }

    #[test]
    fn test_tool_profile_coding() {
        let tools = ToolProfile::Coding.tools();
        assert!(tools.contains(&"file_read".to_string()));
        assert!(tools.contains(&"shell_exec".to_string()));
        assert!(tools.contains(&"web_fetch".to_string()));
        assert_eq!(tools.len(), 5);
    }

    #[test]
    fn test_tool_profile_research() {
        let tools = ToolProfile::Research.tools();
        assert!(tools.contains(&"web_fetch".to_string()));
        assert!(tools.contains(&"web_search".to_string()));
        assert_eq!(tools.len(), 4);
    }

    #[test]
    fn test_tool_profile_messaging() {
        let tools = ToolProfile::Messaging.tools();
        assert!(tools.contains(&"agent_send".to_string()));
        assert!(tools.contains(&"memory_recall".to_string()));
        assert_eq!(tools.len(), 4);
    }

    #[test]
    fn test_tool_profile_automation() {
        let tools = ToolProfile::Automation.tools();
        assert_eq!(tools.len(), 10);
    }

    #[test]
    fn test_tool_profile_full() {
        let tools = ToolProfile::Full.tools();
        assert_eq!(tools, vec!["*"]);
    }

    #[test]
    fn test_tool_profile_implied_capabilities_coding() {
        let caps = ToolProfile::Coding.implied_capabilities();
        assert!(caps.network.contains(&"*".to_string())); // web_fetch
        assert!(caps.shell.contains(&"*".to_string())); // shell_exec
        assert!(!caps.agent_spawn); // no agent_* tools
        assert!(caps.agent_message.is_empty());
    }

    #[test]
    fn test_tool_profile_implied_capabilities_messaging() {
        let caps = ToolProfile::Messaging.implied_capabilities();
        assert!(caps.network.is_empty());
        assert!(caps.shell.is_empty());
        assert!(caps.agent_spawn);
        assert!(caps.agent_message.contains(&"*".to_string()));
        assert!(caps.memory_read.contains(&"*".to_string()));
    }

    #[test]
    fn test_tool_profile_implied_capabilities_minimal() {
        let caps = ToolProfile::Minimal.implied_capabilities();
        assert!(caps.network.is_empty());
        assert!(caps.shell.is_empty());
        assert!(!caps.agent_spawn);
        assert_eq!(caps.memory_read, vec!["self.*".to_string()]);
    }

    #[test]
    fn test_tool_profile_serde_roundtrip() {
        let profile = ToolProfile::Coding;
        let json = serde_json::to_string(&profile).unwrap();
        assert_eq!(json, "\"coding\"");
        let back: ToolProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ToolProfile::Coding);
    }

    // ----- AgentMode tests -----

    #[test]
    fn test_agent_mode_default() {
        assert_eq!(AgentMode::default(), AgentMode::Full);
    }

    #[test]
    fn test_agent_mode_observe_filters_all() {
        let tools = vec![
            ToolDefinition {
                name: "file_read".into(),
                description: String::new(),
                input_schema: serde_json::Value::Null,
            },
            ToolDefinition {
                name: "shell_exec".into(),
                description: String::new(),
                input_schema: serde_json::Value::Null,
            },
        ];
        let filtered = AgentMode::Observe.filter_tools(tools);
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_agent_mode_assist_filters_write_tools() {
        let tools = vec![
            ToolDefinition {
                name: "file_read".into(),
                description: String::new(),
                input_schema: serde_json::Value::Null,
            },
            ToolDefinition {
                name: "file_write".into(),
                description: String::new(),
                input_schema: serde_json::Value::Null,
            },
            ToolDefinition {
                name: "shell_exec".into(),
                description: String::new(),
                input_schema: serde_json::Value::Null,
            },
            ToolDefinition {
                name: "web_fetch".into(),
                description: String::new(),
                input_schema: serde_json::Value::Null,
            },
            ToolDefinition {
                name: "memory_recall".into(),
                description: String::new(),
                input_schema: serde_json::Value::Null,
            },
        ];
        let filtered = AgentMode::Assist.filter_tools(tools);
        assert_eq!(filtered.len(), 3);
        let names: Vec<&str> = filtered.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"file_read"));
        assert!(names.contains(&"web_fetch"));
        assert!(names.contains(&"memory_recall"));
        assert!(!names.contains(&"file_write"));
        assert!(!names.contains(&"shell_exec"));
    }

    #[test]
    fn test_agent_mode_full_passes_all() {
        let tools = vec![
            ToolDefinition {
                name: "file_read".into(),
                description: String::new(),
                input_schema: serde_json::Value::Null,
            },
            ToolDefinition {
                name: "shell_exec".into(),
                description: String::new(),
                input_schema: serde_json::Value::Null,
            },
        ];
        let filtered = AgentMode::Full.filter_tools(tools);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_agent_mode_serde_roundtrip() {
        let mode = AgentMode::Assist;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, "\"assist\"");
        let back: AgentMode = serde_json::from_str(&json).unwrap();
        assert_eq!(back, AgentMode::Assist);
    }

    // ----- FallbackModel tests -----

    #[test]
    fn test_fallback_model_serde() {
        let fb = FallbackModel {
            provider: "groq".to_string(),
            model: "llama-3.3-70b".to_string(),
            api_key_env: Some("GROQ_API_KEY".to_string()),
            base_url: None,
        };
        let json = serde_json::to_string(&fb).unwrap();
        let back: FallbackModel = serde_json::from_str(&json).unwrap();
        assert_eq!(back.provider, "groq");
        assert_eq!(back.model, "llama-3.3-70b");
        assert_eq!(back.api_key_env, Some("GROQ_API_KEY".to_string()));
    }

    #[test]
    fn test_manifest_with_new_fields() {
        let manifest = AgentManifest {
            profile: Some(ToolProfile::Coding),
            fallback_models: vec![FallbackModel {
                provider: "groq".to_string(),
                model: "llama-3.3-70b".to_string(),
                api_key_env: None,
                base_url: None,
            }],
            ..Default::default()
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let back: AgentManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.profile, Some(ToolProfile::Coding));
        assert_eq!(back.fallback_models.len(), 1);
    }

    #[test]
    fn test_agent_entry_with_mode() {
        let entry = AgentEntry {
            id: AgentId::new(),
            name: "test".to_string(),
            manifest: AgentManifest::default(),
            state: AgentState::Running,
            mode: AgentMode::Assist,
            created_at: Utc::now(),
            last_active: Utc::now(),
            parent: None,
            children: vec![],
            session_id: SessionId::new(),
            tags: vec![],
            identity: AgentIdentity::default(),
            onboarding_completed: false,
            onboarding_completed_at: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: AgentEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.mode, AgentMode::Assist);
    }

    #[test]
    fn test_agent_identity_default() {
        let id = AgentIdentity::default();
        assert!(id.emoji.is_none());
        assert!(id.avatar_url.is_none());
        assert!(id.color.is_none());
        assert!(id.archetype.is_none());
        assert!(id.vibe.is_none());
        assert!(id.greeting_style.is_none());
    }

    #[test]
    fn test_agent_identity_serde_roundtrip() {
        let id = AgentIdentity {
            emoji: Some("\u{1F916}".to_string()),
            avatar_url: Some("https://example.com/avatar.png".to_string()),
            color: Some("#FF5C00".to_string()),
            archetype: Some("assistant".to_string()),
            vibe: Some("friendly".to_string()),
            greeting_style: Some("warm".to_string()),
        };
        let json = serde_json::to_string(&id).unwrap();
        let back: AgentIdentity = serde_json::from_str(&json).unwrap();
        assert_eq!(back.emoji, Some("\u{1F916}".to_string()));
        assert_eq!(back.color, Some("#FF5C00".to_string()));
    }

    #[test]
    fn test_agent_identity_deserialize_missing_fields() {
        // AgentIdentity should deserialize from empty JSON thanks to #[serde(default)]
        let id: AgentIdentity = serde_json::from_str("{}").unwrap();
        assert!(id.emoji.is_none());
    }

    #[test]
    fn test_agent_entry_identity_in_serde() {
        let entry = AgentEntry {
            id: AgentId::new(),
            name: "bot".to_string(),
            manifest: AgentManifest::default(),
            state: AgentState::Running,
            mode: AgentMode::default(),
            created_at: Utc::now(),
            last_active: Utc::now(),
            parent: None,
            children: vec![],
            session_id: SessionId::new(),
            tags: vec![],
            identity: AgentIdentity {
                emoji: Some("\u{1F525}".to_string()),
                avatar_url: None,
                color: Some("#00FF00".to_string()),
                ..Default::default()
            },
            onboarding_completed: false,
            onboarding_completed_at: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: AgentEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.identity.emoji, Some("\u{1F525}".to_string()));
        assert_eq!(back.identity.color, Some("#00FF00".to_string()));
        assert!(back.identity.avatar_url.is_none());
    }

    // ----- SessionLabel tests -----

    #[test]
    fn test_session_label_valid() {
        let label = SessionLabel::new("support inbox").unwrap();
        assert_eq!(label.as_str(), "support inbox");
    }

    #[test]
    fn test_session_label_with_hyphens_underscores() {
        let label = SessionLabel::new("my-session_2024").unwrap();
        assert_eq!(label.as_str(), "my-session_2024");
    }

    #[test]
    fn test_session_label_trims_whitespace() {
        let label = SessionLabel::new("  research  ").unwrap();
        assert_eq!(label.as_str(), "research");
    }

    #[test]
    fn test_session_label_rejects_empty() {
        assert!(SessionLabel::new("").is_err());
        assert!(SessionLabel::new("   ").is_err());
    }

    #[test]
    fn test_session_label_rejects_too_long() {
        let long = "a".repeat(129);
        assert!(SessionLabel::new(&long).is_err());
    }

    #[test]
    fn test_session_label_rejects_special_chars() {
        assert!(SessionLabel::new("hello@world").is_err());
        assert!(SessionLabel::new("path/traversal").is_err());
        assert!(SessionLabel::new("<script>").is_err());
    }

    #[test]
    fn test_session_label_serde_roundtrip() {
        let label = SessionLabel::new("test label").unwrap();
        let json = serde_json::to_string(&label).unwrap();
        let back: SessionLabel = serde_json::from_str(&json).unwrap();
        assert_eq!(label, back);
    }

    // ----- generate_identity_files field tests -----

    #[test]
    fn test_manifest_generate_identity_files_default_true() {
        let manifest = AgentManifest::default();
        assert!(manifest.generate_identity_files);
    }

    #[test]
    fn test_manifest_generate_identity_files_serde() {
        let json = r#"{"name":"test","generate_identity_files":false}"#;
        let manifest: AgentManifest = serde_json::from_str(json).unwrap();
        assert!(!manifest.generate_identity_files);
    }

    #[test]
    fn test_manifest_generate_identity_files_defaults_on_missing() {
        let json = r#"{"name":"test"}"#;
        let manifest: AgentManifest = serde_json::from_str(json).unwrap();
        assert!(manifest.generate_identity_files);
    }
}
