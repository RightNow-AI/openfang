//! Request/response types for the OpenFang API.

use serde::{Deserialize, Serialize};

/// Request to spawn an agent from a TOML manifest string or a template name.
#[derive(Debug, Deserialize)]
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
#[derive(Debug, Serialize)]
pub struct SpawnResponse {
    pub agent_id: String,
    pub name: String,
}

/// A file attachment reference (from a prior upload).
#[derive(Debug, Clone, Deserialize)]
pub struct AttachmentRef {
    pub file_id: String,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub content_type: String,
}

/// Request to send a message to an agent.
#[derive(Debug, Deserialize)]
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
#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub response: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub iterations: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

/// Request to install a skill from the marketplace.
#[derive(Debug, Deserialize)]
pub struct SkillInstallRequest {
    pub name: String,
    /// When true, reject the install unless the bundle ships a valid
    /// Ed25519 SignedManifest envelope bound to the on-disk manifest.
    /// Maps to `InstallOptions::require_signed` (issue #1170).
    #[serde(default)]
    pub require_signed: bool,
    /// Optional hex-encoded allow-list of acceptable signer public keys.
    /// Empty = TOFU (any valid signature accepted).
    #[serde(default)]
    pub allowed_signer_keys: Vec<String>,
}

/// Request to uninstall a skill.
#[derive(Debug, Deserialize)]
pub struct SkillUninstallRequest {
    pub name: String,
}

/// Request to update an agent's manifest.
#[derive(Debug, Deserialize)]
pub struct AgentUpdateRequest {
    pub manifest_toml: String,
}

/// Request to change an agent's operational mode.
#[derive(Debug, Deserialize)]
pub struct SetModeRequest {
    pub mode: openfang_types::agent::AgentMode,
}

/// Request to run a migration.
#[derive(Debug, Deserialize)]
pub struct MigrateRequest {
    pub source: String,
    pub source_dir: String,
    pub target_dir: String,
    #[serde(default)]
    pub dry_run: bool,
}

/// Request to scan a directory for migration.
#[derive(Debug, Deserialize)]
pub struct MigrateScanRequest {
    pub path: String,
}

/// Request to install a skill from ClawHub.
#[derive(Debug, Deserialize)]
pub struct ClawHubInstallRequest {
    /// ClawHub skill slug (e.g., "github-helper").
    pub slug: String,
}

/// Query parameters for `GET /api/commands`.
#[derive(Debug, Deserialize)]
pub struct CommandsQuery {
    /// Surface filter: `web` (default), `cli`, `channel`, or `all`.
    #[serde(default)]
    pub surface: Option<String>,
}

/// Request body for `POST /api/audit/append` (issue #1174).
///
/// Lets external (instance-side) wrappers append entries to the Merkle hash
/// chain audit log. The handler maps `event_type` to an `AuditAction` and
/// records the entry through `kernel.audit_log`.
#[derive(Debug, Deserialize)]
pub struct AuditAppendRequest {
    /// Operator-supplied event category. Case-insensitive, matched against the
    /// `AuditAction` enum variants (e.g. `tool_invoke`, `ConfigChange`,
    /// `agent_message`). Unknown values fall back to `ToolInvoke`.
    pub event_type: String,
    /// Agent or wrapper identifier responsible for the event. When empty,
    /// recorded as `"external-wrapper"`.
    #[serde(default)]
    pub agent_id: String,
    /// Free-form detail string (e.g. tool name, URL, file path).
    #[serde(default)]
    pub detail: String,
    /// Optional arbitrary payload. When present it is serialised to JSON and
    /// appended onto the entry's detail so the wrapper retains structured
    /// context without changing the on-chain schema.
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
    /// Optional outcome string (`"ok"`, `"denied"`, or an error). Defaults to
    /// `"ok"` when omitted.
    #[serde(default)]
    pub outcome: Option<String>,
    /// Optional operator-supplied signing context (e.g. wrapper identity, key
    /// fingerprint). Mixed into the detail when present so the chain captures
    /// who attested to the event.
    #[serde(default)]
    pub signing_context: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_install_request_defaults_back_compat() {
        // Existing callers send `{"name": "..."}` only. New optional fields
        // must default cleanly (issue #1170).
        let req: SkillInstallRequest = serde_json::from_str(r#"{"name":"github-helper"}"#).unwrap();
        assert_eq!(req.name, "github-helper");
        assert!(!req.require_signed);
        assert!(req.allowed_signer_keys.is_empty());
    }

    #[test]
    fn skill_install_request_parses_require_signed() {
        let req: SkillInstallRequest = serde_json::from_str(
            r#"{"name":"x","require_signed":true,"allowed_signer_keys":["abc123"]}"#,
        )
        .unwrap();
        assert!(req.require_signed);
        assert_eq!(req.allowed_signer_keys, vec!["abc123".to_string()]);
    }

    #[test]
    fn audit_append_request_required_only() {
        // Only `event_type` is required; everything else must default.
        let req: AuditAppendRequest =
            serde_json::from_str(r#"{"event_type":"ToolInvoke"}"#).unwrap();
        assert_eq!(req.event_type, "ToolInvoke");
        assert!(req.agent_id.is_empty());
        assert!(req.detail.is_empty());
        assert!(req.payload.is_none());
        assert!(req.outcome.is_none());
        assert!(req.signing_context.is_none());
    }

    #[test]
    fn audit_append_request_full_payload() {
        let body = r#"{
            "event_type": "config_change",
            "agent_id": "wrapper-1",
            "detail": "rotated key",
            "payload": {"key_id": "k-42", "ts": 1700000000},
            "outcome": "ok",
            "signing_context": "ed25519:deadbeef"
        }"#;
        let req: AuditAppendRequest = serde_json::from_str(body).unwrap();
        assert_eq!(req.event_type, "config_change");
        assert_eq!(req.agent_id, "wrapper-1");
        assert_eq!(req.detail, "rotated key");
        assert_eq!(req.outcome.as_deref(), Some("ok"));
        assert_eq!(req.signing_context.as_deref(), Some("ed25519:deadbeef"));
        let payload = req.payload.expect("payload present");
        assert_eq!(payload["key_id"], "k-42");
        assert_eq!(payload["ts"], 1_700_000_000);
    }
}
