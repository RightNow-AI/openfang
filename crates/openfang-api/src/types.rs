//! Request/response types for the OpenFang API.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Request to spawn an agent from a TOML manifest string or a template name.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SpawnRequest {
    /// Agent manifest as TOML string (optional if `template` is provided).
    #[serde(default)]
    pub manifest_toml: String,
    /// Template name from `~/.openfang/agents/{template}/agent.toml`.
    /// When provided and `manifest_toml` is empty, the template is loaded automatically.
    #[serde(default)]
    pub template: Option<String>,
    /// Optional Ed25519 signed manifest envelope (JSON).
    /// When present, the signature is verified before spawning.
    #[serde(default)]
    pub signed_manifest: Option<String>,
}

/// Response after spawning an agent.
#[derive(Debug, Serialize, ToSchema)]
pub struct SpawnResponse {
    pub agent_id: String,
    pub name: String,
}

/// A file attachment reference (from a prior upload).
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct AttachmentRef {
    pub file_id: String,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub content_type: String,
}

/// Request to send a message to an agent.
#[derive(Debug, Deserialize, ToSchema)]
pub struct MessageRequest {
    pub message: String,
    /// Optional file attachments (uploaded via /upload endpoint).
    #[serde(default)]
    pub attachments: Vec<AttachmentRef>,
    /// Sender identity (e.g. WhatsApp phone number, Telegram user ID).
    #[serde(default)]
    pub sender_id: Option<String>,
    /// Sender display name.
    #[serde(default)]
    pub sender_name: Option<String>,
}

/// Response from sending a message.
#[derive(Debug, Serialize, ToSchema)]
pub struct MessageResponse {
    pub response: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub iterations: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

/// Request to install a skill from the marketplace.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SkillInstallRequest {
    pub name: String,
}

/// Request to uninstall a skill.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SkillUninstallRequest {
    pub name: String,
}

/// Request to update an agent's manifest.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AgentUpdateRequest {
    pub manifest_toml: String,
}

/// Request to change an agent's operational mode.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetModeRequest {
    pub mode: openfang_types::agent::AgentMode,
}

/// Request to run a migration.
#[derive(Debug, Deserialize, ToSchema)]
pub struct MigrateRequest {
    pub source: String,
    pub source_dir: String,
    pub target_dir: String,
    #[serde(default)]
    pub dry_run: bool,
}

/// Request to scan a directory for migration.
#[derive(Debug, Deserialize, ToSchema)]
pub struct MigrateScanRequest {
    pub path: String,
}

/// Request to install a skill from ClawHub.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ClawHubInstallRequest {
    /// ClawHub skill slug (e.g., "github-helper").
    pub slug: String,
}

// ---------------------------------------------------------------------------
// New strongly-typed response structs (for OpenAPI schema coverage)
// ---------------------------------------------------------------------------

/// Summary of an agent as returned by GET /api/agents.
#[derive(Debug, Serialize, ToSchema)]
pub struct AgentSummary {
    pub id: String,
    pub name: String,
    /// One of: "Idle", "Running", "Stopped", "Error"
    pub state: String,
    /// One of: "autonomous", "supervised", "paused"
    pub mode: String,
    pub provider: String,
    pub model: String,
    pub tier: String,
    pub auth_status: String,
    pub ready: bool,
    pub persona: String,
    pub division: String,
}

/// Response body for GET /api/health.
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    /// "ok" or "degraded"
    pub status: String,
    pub version: String,
    pub uptime_seconds: u64,
}

/// Snapshot of budget spend vs limits.
#[derive(Debug, Serialize, ToSchema)]
pub struct BudgetSnapshot {
    pub hourly_spend: f64,
    pub hourly_limit: f64,
    pub hourly_pct: f64,
    pub daily_spend: f64,
    pub daily_limit: f64,
    pub daily_pct: f64,
    pub monthly_spend: f64,
    pub monthly_limit: f64,
    pub monthly_pct: f64,
    pub alert_threshold: f64,
}

