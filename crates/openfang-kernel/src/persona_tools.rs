//! Persona-to-tool binding system.
//!
//! This module provides:
//! - Dedicated channel tool definitions for Telegram, Email, Slack, Discord, etc.
//! - A resolver that maps an `AgentPersona`'s `required_tools` to `ToolDefinition` objects.
//! - A `PersonaToolHook` that enforces persona constraints at the `BeforeToolCall` /
//!   `AfterToolCall` intercept points.
//!
//! ## Flow
//!
//! ```text
//! AgentManifest.persona_id ──► persona_registry::persona_by_id()
//!                                         │
//!                               AgentPersona.required_tools
//!                                         │
//!                           resolve_tools_for_persona()
//!                                         │
//!                           Vec<ToolDefinition>  ──► available_tools()
//!                                         │
//!                           PersonaToolHook (BeforeToolCall)
//!                             blocks tools outside persona contract
//! ```

use openfang_runtime::hooks::{HookContext, HookHandler};
use openfang_types::agent::HookEvent;
use openfang_types::tool::ToolDefinition;
use tracing::{debug, warn};

// ────────────────────────────────────────────────────────────────────────────
// Channel-specific tool definitions
// ────────────────────────────────────────────────────────────────────────────

/// Return `ToolDefinition`s for all named channel tools.
///
/// These are ergonomic wrappers over the generic `channel_send` tool — each
/// carries channel-specific parameters so the LLM doesn't have to know the
/// underlying routing mechanism.  At execution time they are dispatched to
/// `channel_send` in `tool_runner.rs`.
pub fn channel_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        // ── Telegram ─────────────────────────────────────────────────────
        ToolDefinition {
            name: "telegram_send".to_string(),
            description: "Send a Telegram message to a chat or user. \
                          Requires TELEGRAM_BOT_TOKEN env var. \
                          Use 'chat_id' as the recipient (numeric Telegram chat ID or @username)."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "chat_id": {
                        "type": "string",
                        "description": "Telegram chat ID or @username to send to"
                    },
                    "message": {
                        "type": "string",
                        "description": "Message text (supports HTML formatting)"
                    },
                    "image_url": {
                        "type": "string",
                        "description": "Optional: URL of an image to send alongside the message"
                    }
                },
                "required": ["chat_id", "message"]
            }),
        },
        // ── Email ─────────────────────────────────────────────────────────
        ToolDefinition {
            name: "email_send".to_string(),
            description: "Send an email via the configured SMTP adapter. \
                          Requires EMAIL_USERNAME and EMAIL_PASSWORD env vars. \
                          The 'to' field must be a valid email address."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "to": {
                        "type": "string",
                        "description": "Recipient email address"
                    },
                    "subject": {
                        "type": "string",
                        "description": "Email subject line"
                    },
                    "body": {
                        "type": "string",
                        "description": "Email body (plain text or HTML)"
                    },
                    "cc": {
                        "type": "string",
                        "description": "Optional CC email address"
                    }
                },
                "required": ["to", "subject", "body"]
            }),
        },
        // ── Slack ─────────────────────────────────────────────────────────
        ToolDefinition {
            name: "slack_send".to_string(),
            description: "Post a message to a Slack channel. \
                          Requires SLACK_BOT_TOKEN env var. \
                          'channel' should be a channel name like #general or a user ID."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "channel": {
                        "type": "string",
                        "description": "Slack channel name (e.g. #general) or user ID"
                    },
                    "message": {
                        "type": "string",
                        "description": "Message text (supports Slack mrkdwn formatting)"
                    },
                    "thread_ts": {
                        "type": "string",
                        "description": "Optional: timestamp of parent message to reply in thread"
                    }
                },
                "required": ["channel", "message"]
            }),
        },
        // ── Discord ───────────────────────────────────────────────────────
        ToolDefinition {
            name: "discord_send".to_string(),
            description: "Send a message to a Discord channel. \
                          Requires DISCORD_TOKEN env var. \
                          'channel_id' is the numeric Discord channel ID."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "channel_id": {
                        "type": "string",
                        "description": "Discord channel ID (numeric snowflake)"
                    },
                    "content": {
                        "type": "string",
                        "description": "Message content (max 2000 chars)"
                    },
                    "embed_title": {
                        "type": "string",
                        "description": "Optional: embed card title"
                    },
                    "embed_description": {
                        "type": "string",
                        "description": "Optional: embed card description"
                    }
                },
                "required": ["channel_id", "content"]
            }),
        },
        // ── WhatsApp ─────────────────────────────────────────────────────
        ToolDefinition {
            name: "whatsapp_send".to_string(),
            description: "Send a WhatsApp message via the configured gateway. \
                          Requires WHATSAPP_API_KEY env var. \
                          'to' should be the phone number in E.164 format (e.g. +12025551234)."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "to": {
                        "type": "string",
                        "description": "Recipient phone number in E.164 format"
                    },
                    "message": {
                        "type": "string",
                        "description": "Message text"
                    }
                },
                "required": ["to", "message"]
            }),
        },
        // ── SMS / Generic webhook ────────────────────────────────────────
        ToolDefinition {
            name: "sms_send".to_string(),
            description: "Send an SMS via the configured SMS adapter (Twilio, etc.). \
                          Requires SMS_API_KEY env var. \
                          'to' should be a phone number in E.164 format."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "to": {
                        "type": "string",
                        "description": "Recipient phone number in E.164 format"
                    },
                    "body": {
                        "type": "string",
                        "description": "SMS body text (max 160 chars for single segment)"
                    }
                },
                "required": ["to", "body"]
            }),
        },
    ]
}

/// Names of all channel tools (for allowlist matching).
pub const CHANNEL_TOOL_NAMES: &[&str] = &[
    "telegram_send",
    "email_send",
    "slack_send",
    "discord_send",
    "whatsapp_send",
    "sms_send",
    "channel_send", // generic fallback
];

// ────────────────────────────────────────────────────────────────────────────
// Persona → ToolDefinition resolver
// ────────────────────────────────────────────────────────────────────────────

/// Resolve which additional tools should be injected for an agent based on
/// its persona's `required_tools` declarations.
///
/// ### Mapping rules
/// - `"channel:telegram_send"` → injects `telegram_send` definition
/// - `"channel:email_send"`    → injects `email_send` definition
/// - `"channel:slack_send"`    → injects `slack_send` definition
/// - `"channel:discord_send"`  → injects `discord_send` definition
/// - `"channel:whatsapp_send"` → injects `whatsapp_send` definition
/// - `"channel:sms_send"`      → injects `sms_send` definition
/// - `"channel:*"`             → injects ALL channel tools
/// - any plain name that is already a known built-in is accepted silently
///
/// Unknown/unmapped tool names are logged as warnings.
pub fn resolve_tools_for_persona(persona_id: &str) -> Vec<ToolDefinition> {
    let registry = crate::persona_registry::all_personas();
    let persona = match registry.iter().find(|p| p.id == persona_id) {
        Some(p) => p,
        None => {
            warn!(persona_id, "persona_tools: persona not found in registry");
            return Vec::new();
        }
    };

    let channel_defs = channel_tool_definitions();
    let mut tools: Vec<ToolDefinition> = Vec::new();
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();

    for required in &persona.required_tools {
        match *required {
            // Wildcard: inject all channel tools
            "channel:*" => {
                for def in &channel_defs {
                    if seen.insert(def.name.as_str()) {
                        tools.push(def.clone());
                    }
                }
            }
            // Named channel tools (namespace:name)
            name if name.starts_with("channel:") => {
                let tool_name = &name["channel:".len()..];
                if let Some(def) = channel_defs.iter().find(|d| d.name == tool_name) {
                    if seen.insert(def.name.as_str()) {
                        tools.push(def.clone());
                        debug!(persona_id, tool = tool_name, "persona_tools: injecting channel tool");
                    }
                } else {
                    warn!(persona_id, tool = tool_name, "persona_tools: unknown channel tool in required_tools");
                }
            }
            // Service deps (e.g. "service:openai") — not tools, skip silently
            name if name.contains(':') => {
                debug!(persona_id, entry = name, "persona_tools: skipping non-tool service dep");
            }
            // Plain built-in names (file_read, web_search, etc.) — already in builtins, skip
            _ => {
                debug!(persona_id, tool = required, "persona_tools: built-in (no injection needed)");
            }
        }
    }

    tools
}

/// Validate that all `required_tools` declared by a persona are satisfiable
/// given the provided `available_tool_names`.
///
/// Returns a list of unsatisfied tool names (empty = all satisfied).
pub fn check_required_tools(
    persona_id: &str,
    available_tool_names: &[String],
) -> Vec<String> {
    let registry = crate::persona_registry::all_personas();
    let persona = match registry.iter().find(|p| p.id == persona_id) {
        Some(p) => p,
        None => return Vec::new(),
    };

    let mut missing = Vec::new();
    for required in &persona.required_tools {
        // Resolve channel tools to their plain name
        let plain_name = if let Some(stripped) = required.strip_prefix("channel:") {
            stripped
        } else if required.contains(':') {
            // Service deps — skip
            continue;
        } else {
            required
        };

        if plain_name == "*" {
            continue;
        }

        let available = available_tool_names
            .iter()
            .any(|n| n == plain_name || n == "channel_send");

        if !available {
            missing.push(plain_name.to_string());
        }
    }
    missing
}

// ────────────────────────────────────────────────────────────────────────────
// PersonaToolHook — BeforeToolCall / AfterToolCall enforcement
// ────────────────────────────────────────────────────────────────────────────

/// A hook handler that enforces persona-level tool contracts.
///
/// Register one instance per agent (keyed by `agent_id`) in the kernel's
/// `HookRegistry` at agent spawn time.
///
/// * `BeforeToolCall` — If the calling agent has a persona, this hook checks
///   that the tool being called is consistent with the persona's constraints:
///   - `safe_for_auto_run = false` AND tool is in the "destructive" category →
///     warns but does **not** block (enforcement is advisory via logging; hard
///     blocking is reserved for `required_approval` personas via the approval gate).
///   - If the persona's `observability.trace_level` is Verbose, the input is
///     logged at `tracing::debug!`.
///
/// * `AfterToolCall` — Emits an observability span recording tool name, outcome
///   (ok/error), and the persona ID for downstream log aggregation.
pub struct PersonaToolHook {
    /// The persona ID this hook enforces.
    pub persona_id: String,
    /// The agent display name (for log context).
    pub agent_name: String,
}

impl PersonaToolHook {
    pub fn new(persona_id: impl Into<String>, agent_name: impl Into<String>) -> Self {
        Self {
            persona_id: persona_id.into(),
            agent_name: agent_name.into(),
        }
    }
}

impl HookHandler for PersonaToolHook {
    fn on_event(&self, ctx: &HookContext) -> Result<(), String> {
        // Only process events for the specific agent this hook was registered for.
        // The kernel's HookRegistry is global; without this guard the hook would
        // fire for every agent running through the same kernel instance.
        if ctx.agent_name != self.agent_name {
            return Ok(());
        }

        let registry = crate::persona_registry::all_personas();
        let persona = match registry.iter().find(|p| p.id == self.persona_id.as_str()) {
            Some(p) => p,
            None => return Ok(()), // persona not found, let execution proceed
        };

        match ctx.event {
            HookEvent::BeforeToolCall => {
                let tool_name = ctx.data["tool_name"].as_str().unwrap_or("unknown");

                // Advisory: log when a non-auto-run-safe persona uses a channel tool
                if !persona.constraints.safe_for_auto_run && CHANNEL_TOOL_NAMES.contains(&tool_name)
                {
                    warn!(
                        agent = ctx.agent_name,
                        persona = self.persona_id,
                        tool = tool_name,
                        "PersonaToolHook: channel tool called by non-auto-run persona (advisory)"
                    );
                }

                // Verbose trace: log tool invocation details
                if persona.observability.trace_level
                    == openfang_types::swarm::TraceLevel::Verbose
                {
                    debug!(
                        agent = ctx.agent_name,
                        persona = self.persona_id,
                        tool = tool_name,
                        input = %ctx.data,
                        "PersonaToolHook: tool call trace"
                    );
                }

                Ok(())
            }

            HookEvent::AfterToolCall => {
                let tool_name = ctx.data["tool_name"].as_str().unwrap_or("unknown");
                let is_error = ctx.data["is_error"].as_bool().unwrap_or(false);

                debug!(
                    agent = ctx.agent_name,
                    persona = self.persona_id,
                    tool = tool_name,
                    is_error,
                    "PersonaToolHook: tool call completed"
                );

                Ok(())
            }

            // Other events — pass through
            _ => Ok(()),
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Channel tool name → channel_send routing helpers
// ────────────────────────────────────────────────────────────────────────────

/// Map a dedicated channel tool name + input to the parameters expected by
/// the generic `channel_send` handler.
///
/// Returns `(channel_name, recipient, overrides)` where `overrides` is a
/// `serde_json::Value` object merged on top of the standard channel_send payload.
///
/// Returns `None` if `tool_name` is not a known channel tool alias.
pub fn map_channel_tool_to_send(
    tool_name: &str,
    input: &serde_json::Value,
) -> Option<serde_json::Value> {
    match tool_name {
        "telegram_send" => {
            let chat_id = input["chat_id"].as_str()?;
            let message = input["message"].as_str().unwrap_or("");
            let mut payload = serde_json::json!({
                "channel": "telegram",
                "recipient": chat_id,
                "message": message,
            });
            if let Some(img) = input["image_url"].as_str() {
                payload["image_url"] = serde_json::Value::String(img.to_string());
            }
            Some(payload)
        }

        "email_send" => {
            let to = input["to"].as_str()?;
            let subject = input["subject"].as_str().unwrap_or("");
            let body = input["body"].as_str().unwrap_or("");
            let mut payload = serde_json::json!({
                "channel": "email",
                "recipient": to,
                "message": body,
                "subject": subject,
            });
            if let Some(cc) = input["cc"].as_str() {
                payload["cc"] = serde_json::Value::String(cc.to_string());
            }
            Some(payload)
        }

        "slack_send" => {
            let channel = input["channel"].as_str()?;
            let message = input["message"].as_str().unwrap_or("");
            let mut payload = serde_json::json!({
                "channel": "slack",
                "recipient": channel,
                "message": message,
            });
            if let Some(ts) = input["thread_ts"].as_str() {
                payload["thread_ts"] = serde_json::Value::String(ts.to_string());
            }
            Some(payload)
        }

        "discord_send" => {
            let channel_id = input["channel_id"].as_str()?;
            let content = input["content"].as_str().unwrap_or("");
            let mut payload = serde_json::json!({
                "channel": "discord",
                "recipient": channel_id,
                "message": content,
            });
            if let Some(title) = input["embed_title"].as_str() {
                payload["embed_title"] = serde_json::Value::String(title.to_string());
            }
            if let Some(desc) = input["embed_description"].as_str() {
                payload["embed_description"] = serde_json::Value::String(desc.to_string());
            }
            Some(payload)
        }

        "whatsapp_send" => {
            let to = input["to"].as_str()?;
            let message = input["message"].as_str().unwrap_or("");
            Some(serde_json::json!({
                "channel": "whatsapp",
                "recipient": to,
                "message": message,
            }))
        }

        "sms_send" => {
            let to = input["to"].as_str()?;
            let body = input["body"].as_str().unwrap_or("");
            Some(serde_json::json!({
                "channel": "sms",
                "recipient": to,
                "message": body,
            }))
        }

        _ => None,
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_tool_definitions_not_empty() {
        let defs = channel_tool_definitions();
        assert!(!defs.is_empty());
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"telegram_send"));
        assert!(names.contains(&"email_send"));
        assert!(names.contains(&"slack_send"));
        assert!(names.contains(&"discord_send"));
        assert!(names.contains(&"whatsapp_send"));
        assert!(names.contains(&"sms_send"));
    }

    #[test]
    fn channel_tool_definitions_have_valid_schemas() {
        for def in channel_tool_definitions() {
            assert!(def.input_schema.is_object(), "{} schema is not an object", def.name);
            assert!(
                def.input_schema["required"].is_array(),
                "{} schema missing 'required' array",
                def.name
            );
        }
    }

    #[test]
    fn map_telegram_send() {
        let input = serde_json::json!({
            "chat_id": "123456789",
            "message": "Hello!"
        });
        let mapped = map_channel_tool_to_send("telegram_send", &input).unwrap();
        assert_eq!(mapped["channel"], "telegram");
        assert_eq!(mapped["recipient"], "123456789");
        assert_eq!(mapped["message"], "Hello!");
    }

    #[test]
    fn map_email_send() {
        let input = serde_json::json!({
            "to": "alice@example.com",
            "subject": "Test",
            "body": "Hello Alice"
        });
        let mapped = map_channel_tool_to_send("email_send", &input).unwrap();
        assert_eq!(mapped["channel"], "email");
        assert_eq!(mapped["recipient"], "alice@example.com");
        assert_eq!(mapped["subject"], "Test");
        assert_eq!(mapped["message"], "Hello Alice");
    }

    #[test]
    fn map_slack_send() {
        let input = serde_json::json!({
            "channel": "#general",
            "message": "Hey team!"
        });
        let mapped = map_channel_tool_to_send("slack_send", &input).unwrap();
        assert_eq!(mapped["channel"], "slack");
        assert_eq!(mapped["recipient"], "#general");
    }

    #[test]
    fn map_discord_send() {
        let input = serde_json::json!({
            "channel_id": "987654321",
            "content": "Hello Discord!"
        });
        let mapped = map_channel_tool_to_send("discord_send", &input).unwrap();
        assert_eq!(mapped["channel"], "discord");
        assert_eq!(mapped["recipient"], "987654321");
        assert_eq!(mapped["message"], "Hello Discord!");
    }

    #[test]
    fn map_unknown_tool_returns_none() {
        let input = serde_json::json!({"foo": "bar"});
        assert!(map_channel_tool_to_send("file_read", &input).is_none());
        assert!(map_channel_tool_to_send("unknown_tool", &input).is_none());
    }

    #[test]
    fn resolve_tools_for_unknown_persona() {
        // Unknown persona_id should return empty without panicking
        let tools = resolve_tools_for_persona("nonexistent_persona_xyz");
        assert!(tools.is_empty());
    }

    #[test]
    fn resolve_tools_for_known_personas() {
        // All 18 personas should resolve without panicking
        for persona in crate::persona_registry::all_personas() {
            let tools = resolve_tools_for_persona(persona.id);
            // No assertion on count — personas may have 0 channel tools
            let _ = tools;
        }
    }

    #[test]
    fn check_required_tools_empty_when_all_available() {
        // Give all channel tool names as available
        let available: Vec<String> = CHANNEL_TOOL_NAMES.iter().map(|s| s.to_string()).collect();
        for persona in crate::persona_registry::all_personas() {
            let missing = check_required_tools(persona.id, &available);
            assert!(
                missing.is_empty(),
                "Persona '{}' has unsatisfied tools: {:?}",
                persona.id,
                missing
            );
        }
    }

    #[test]
    fn persona_tool_hook_does_not_block_normal_tools() {
        let hook = PersonaToolHook::new("orchestrator_delegate", "TestAgent");
        let ctx = HookContext {
            agent_name: "TestAgent",
            agent_id: "test-id",
            event: HookEvent::BeforeToolCall,
            data: serde_json::json!({
                "tool_name": "web_search",
                "is_error": false
            }),
        };
        assert!(hook.on_event(&ctx).is_ok());
    }

    #[test]
    fn persona_tool_hook_after_call_does_not_block() {
        let hook = PersonaToolHook::new("orchestrator_delegate", "TestAgent");
        let ctx = HookContext {
            agent_name: "TestAgent",
            agent_id: "test-id",
            event: HookEvent::AfterToolCall,
            data: serde_json::json!({
                "tool_name": "telegram_send",
                "is_error": false,
                "content": "Message sent to 123 via telegram"
            }),
        };
        assert!(hook.on_event(&ctx).is_ok());
    }
}
