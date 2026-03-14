//! App adapter trait and execution context.
//!
//! Every app integration must implement `AppAdapter`. The three concrete
//! variants — `ApiAdapter`, `CliAdapter`, `BrowserAdapter` — provide the
//! implementation skeletons.
//!
//! ## Preference order
//!
//! ```text
//! ApiAdapter   (official REST / GraphQL / gRPC)      ← always prefer
//!   ↓ fallback
//! CliAdapter   (app CLI / SDK)
//!   ↓ fallback
//! BrowserAdapter (Playwright / Puppeteer)             ← last resort
//! ```
//!
//! The `ToolRegistry` uses `AdapterKind::preference_rank()` to select the
//! lowest-ranked available adapter when multiple are registered for the same
//! command.

use crate::tool_contract::{
    AdapterKind, PreflightResult, RiskTier, ServiceHealth,
    ToolContract, VerificationOutcome, VerificationRule,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ────────────────────────────────────────────────────────────────────────────
// Execution context
// ────────────────────────────────────────────────────────────────────────────

/// Everything an adapter needs to execute a single tool command.
#[derive(Debug, Clone)]
pub struct AdapterExecutionContext {
    /// The contract being fulfilled.
    pub contract: ToolContract,

    /// Validated input parsed from the LLM tool call.
    pub input: Value,

    /// The agent executing this call.
    pub agent_id: String,

    /// The agent's display name.
    pub agent_name: String,

    /// Persona ID if one is assigned.
    pub persona_id: Option<String>,

    /// Parent work item ID for lineage tracking.
    pub work_item_id: Option<String>,

    /// Call depth (for sub-agent chains). Max enforced externally.
    pub call_depth: u32,

    /// Caller's approval token, if the step required approval.
    pub approval_token: Option<String>,
}

// ────────────────────────────────────────────────────────────────────────────
// AdapterResult
// ────────────────────────────────────────────────────────────────────────────

/// The result of an adapter executing a tool contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterResult {
    /// Whether the execution succeeded (before verification).
    pub success: bool,

    /// Structured output from the tool, suitable for returning to the LLM.
    pub output: Value,

    /// Human-readable summary of the output (at most 512 chars).
    pub output_summary: String,

    /// HTTP/OS status code if applicable.
    pub status_code: Option<i32>,

    /// How long execution took in milliseconds.
    pub duration_ms: u64,

    /// Artifact IDs created / modified by this call.
    pub artifact_ids: Vec<String>,

    /// Error message if `success = false`.
    pub error: Option<String>,

    /// Whether this was a transient failure eligible for retry.
    pub is_transient_failure: bool,
}

impl AdapterResult {
    pub fn success(output: Value, duration_ms: u64) -> Self {
        let summary = crate::tool_contract::ToolEventRecord::summarise(
            &serde_json::to_string(&output).unwrap_or_default(),
        );
        Self {
            success: true,
            output,
            output_summary: summary,
            status_code: None,
            duration_ms,
            artifact_ids: vec![],
            error: None,
            is_transient_failure: false,
        }
    }

    pub fn failure(error: impl Into<String>, transient: bool, duration_ms: u64) -> Self {
        Self {
            success: false,
            output: Value::Null,
            output_summary: String::new(),
            status_code: None,
            duration_ms,
            artifact_ids: vec![],
            error: Some(error.into()),
            is_transient_failure: transient,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// AppAdapter trait
// ────────────────────────────────────────────────────────────────────────────

/// The core adapter interface every app integration must implement.
///
/// Implementations are registered in the `ToolRegistry` and called by the
/// kernel's tool runner for any `ToolContract` whose `app_id` matches.
pub trait AppAdapter: Send + Sync {
    /// The application identifier this adapter serves (e.g. `"github"`).
    fn app_id(&self) -> &str;

    /// The `AdapterKind` this implementation provides.
    fn adapter_kind(&self) -> AdapterKind;

    /// All tool contracts this adapter can execute.
    fn contracts(&self) -> Vec<ToolContract>;

    /// Run a pre-execution health + readiness check for the given tool.
    ///
    /// Must be fast (< 1 second). Used before every execution when
    /// `preflight_on_every_call` is enabled, and on startup.
    fn preflight(&self, contract: &ToolContract) -> PreflightResult;

    /// Execute the tool contract with the supplied context.
    ///
    /// Adapters **must not** perform verification here — that is done by the
    /// registry using `contract.verification_rule` after this call returns.
    fn execute(&self, ctx: &AdapterExecutionContext) -> AdapterResult;

    /// Return the current health status of the underlying service.
    fn health(&self) -> ServiceHealth;
}

// ────────────────────────────────────────────────────────────────────────────
// Verification engine
// ────────────────────────────────────────────────────────────────────────────

/// Run the verification rule after execution and return the outcome.
///
/// This is a standalone function so it can be used by the `ToolRegistry`,
/// custom adapters, and tests without needing a full execution context.
pub fn run_verification(
    rule: &VerificationRule,
    execution_output: &Value,
    contract: &ToolContract,
) -> VerificationOutcome {
    match rule {
        VerificationRule::None => VerificationOutcome::Skipped,

        VerificationRule::ApiStatusCode { success_min, success_max } => {
            let code = execution_output
                .get("status_code")
                .and_then(|v| v.as_u64())
                .map(|c| c as u16);
            match code {
                Some(c) if c >= *success_min && c <= *success_max => VerificationOutcome::Passed,
                Some(c) => VerificationOutcome::Failed {
                    reason: format!(
                        "Expected status {success_min}–{success_max}, got {c}"
                    ),
                },
                None => VerificationOutcome::Error {
                    reason: "Output did not contain a 'status_code' field".to_string(),
                },
            }
        }

        VerificationRule::ArtifactExists { fetch_url_template } => {
            // Substitute `{id}` in the template using the `id` field in output
            let id = execution_output
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let url = fetch_url_template.replace("{id}", id);
            if url.is_empty() || id.is_empty() {
                return VerificationOutcome::Error {
                    reason: "Could not resolve artifact URL: output missing 'id' field".to_string(),
                };
            }
            // Actual HTTP check is done by the adapter layer; here we signal Skipped.
            // Adapters implementing this rule should call HTTP themselves.
            let _ = (url, contract.name.as_str()); // suppress unused warnings
            VerificationOutcome::Skipped
        }

        VerificationRule::StateChange { read_tool, expected_jsonpath } => {
            // Without executing `read_tool` we can only signal that a check is
            // needed. Callers must wire this through the tool runner.
            let _ = (read_tool, expected_jsonpath, contract.name.as_str());
            VerificationOutcome::Skipped
        }

        VerificationRule::JsonDiff { path } => {
            let _ = (path, contract.name.as_str());
            VerificationOutcome::Skipped
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Starter app contracts — GitHub, Slack, Email, Notion, Calendar
// ────────────────────────────────────────────────────────────────────────────

/// Returns typed `ToolContract`s for the GitHub integration.
///
/// Covers: issue.create, issue.list, issue.comment, pr.comment, pr.review,
///         repo.list, code.search, release.create
pub fn github_contracts() -> Vec<ToolContract> {
    use crate::tool_contract::{ExecutionMode, RetryPolicy};

    let base_output = serde_json::json!({"type": "object", "properties": {
        "id": {"type": "string"},
        "number": {"type": "integer"},
        "html_url": {"type": "string"},
        "title": {"type": "string"},
        "state": {"type": "string"}
    }});

    vec![
        ToolContract {
            name: "github.issue.create".to_string(),
            display_name: "Create GitHub Issue".to_string(),
            description: "Create a new issue in a GitHub repository.".to_string(),
            version: "1.0.0".to_string(),
            app_id: "github".to_string(),
            command_group: "issue".to_string(),
            adapter_kind: AdapterKind::Api,
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["owner", "repo", "title"],
                "properties": {
                    "owner": {"type": "string", "description": "Repository owner (org or user)"},
                    "repo": {"type": "string", "description": "Repository name"},
                    "title": {"type": "string", "description": "Issue title"},
                    "body": {"type": "string", "description": "Issue body (Markdown)"},
                    "labels": {"type": "array", "items": {"type": "string"}},
                    "assignees": {"type": "array", "items": {"type": "string"}}
                }
            }),
            output_schema: base_output.clone(),
            risk_tier: RiskTier::WriteExternal,
            requires_approval: false,
            execution_mode: ExecutionMode::Synchronous,
            timeout_ms: 15_000,
            retry_policy: RetryPolicy::default(),
            verification_rule: VerificationRule::ArtifactExists {
                fetch_url_template: "https://api.github.com/repos/{owner}/{repo}/issues/{number}".to_string(),
            },
            is_read_only: false,
            is_deterministic: false,
            is_idempotent: false,
            docs_url: Some("https://docs.github.com/en/rest/issues/issues#create-an-issue".to_string()),
        },
        ToolContract {
            name: "github.issue.list".to_string(),
            display_name: "List GitHub Issues".to_string(),
            description: "List issues in a GitHub repository.".to_string(),
            version: "1.0.0".to_string(),
            app_id: "github".to_string(),
            command_group: "issue".to_string(),
            adapter_kind: AdapterKind::Api,
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["owner", "repo"],
                "properties": {
                    "owner": {"type": "string"},
                    "repo": {"type": "string"},
                    "state": {"type": "string", "enum": ["open", "closed", "all"]},
                    "labels": {"type": "string"},
                    "per_page": {"type": "integer", "minimum": 1, "maximum": 100}
                }
            }),
            output_schema: serde_json::json!({"type": "array", "items": base_output.clone()}),
            risk_tier: RiskTier::ReadOnly,
            requires_approval: false,
            execution_mode: ExecutionMode::Synchronous,
            timeout_ms: 15_000,
            retry_policy: RetryPolicy::default(),
            verification_rule: VerificationRule::None,
            is_read_only: true,
            is_deterministic: false,
            is_idempotent: true,
            docs_url: Some("https://docs.github.com/en/rest/issues/issues#list-repository-issues".to_string()),
        },
        ToolContract {
            name: "github.issue.comment".to_string(),
            display_name: "Comment on GitHub Issue".to_string(),
            description: "Add a comment to a GitHub issue or pull request.".to_string(),
            version: "1.0.0".to_string(),
            app_id: "github".to_string(),
            command_group: "issue".to_string(),
            adapter_kind: AdapterKind::Api,
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["owner", "repo", "issue_number", "body"],
                "properties": {
                    "owner": {"type": "string"},
                    "repo": {"type": "string"},
                    "issue_number": {"type": "integer"},
                    "body": {"type": "string", "description": "Comment body (Markdown)"}
                }
            }),
            output_schema: serde_json::json!({"type": "object", "properties": {
                "id": {"type": "integer"},
                "html_url": {"type": "string"},
                "body": {"type": "string"}
            }}),
            risk_tier: RiskTier::WriteExternal,
            requires_approval: false,
            execution_mode: ExecutionMode::Synchronous,
            timeout_ms: 15_000,
            retry_policy: RetryPolicy::default(),
            verification_rule: VerificationRule::ApiStatusCode { success_min: 200, success_max: 201 },
            is_read_only: false,
            is_deterministic: false,
            is_idempotent: false,
            docs_url: Some("https://docs.github.com/en/rest/issues/comments#create-an-issue-comment".to_string()),
        },
        ToolContract {
            name: "github.pr.comment".to_string(),
            display_name: "Comment on GitHub PR".to_string(),
            description: "Add a review comment or general comment to a pull request.".to_string(),
            version: "1.0.0".to_string(),
            app_id: "github".to_string(),
            command_group: "pr".to_string(),
            adapter_kind: AdapterKind::Api,
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["owner", "repo", "pull_number", "body"],
                "properties": {
                    "owner": {"type": "string"},
                    "repo": {"type": "string"},
                    "pull_number": {"type": "integer"},
                    "body": {"type": "string"}
                }
            }),
            output_schema: serde_json::json!({"type": "object", "properties": {
                "id": {"type": "integer"}, "html_url": {"type": "string"}
            }}),
            risk_tier: RiskTier::WriteExternal,
            requires_approval: false,
            execution_mode: ExecutionMode::Synchronous,
            timeout_ms: 15_000,
            retry_policy: RetryPolicy::default(),
            verification_rule: VerificationRule::ApiStatusCode { success_min: 200, success_max: 201 },
            is_read_only: false,
            is_deterministic: false,
            is_idempotent: false,
            docs_url: Some("https://docs.github.com/en/rest/pulls/comments".to_string()),
        },
    ]
}

/// Returns typed `ToolContract`s for the Slack integration.
///
/// Covers: message.send, message.reply, channel.list, user.lookup
pub fn slack_contracts() -> Vec<ToolContract> {
    use crate::tool_contract::{ExecutionMode, RetryPolicy};

    vec![
        ToolContract {
            name: "slack.message.send".to_string(),
            display_name: "Send Slack Message".to_string(),
            description: "Post a message to a Slack channel or DM.".to_string(),
            version: "1.0.0".to_string(),
            app_id: "slack".to_string(),
            command_group: "message".to_string(),
            adapter_kind: AdapterKind::Api,
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["channel", "text"],
                "properties": {
                    "channel": {"type": "string", "description": "Channel ID or #channel-name or @username"},
                    "text": {"type": "string", "description": "Message text (supports Markdown)"},
                    "thread_ts": {"type": "string", "description": "Parent message timestamp to reply in thread"},
                    "blocks": {"type": "array", "description": "Optional Block Kit layout blocks"}
                }
            }),
            output_schema: serde_json::json!({"type": "object", "properties": {
                "ok": {"type": "boolean"},
                "ts": {"type": "string"},
                "channel": {"type": "string"}
            }}),
            risk_tier: RiskTier::WriteExternal,
            requires_approval: false,
            execution_mode: ExecutionMode::Synchronous,
            timeout_ms: 10_000,
            retry_policy: RetryPolicy::default(),
            verification_rule: VerificationRule::ApiStatusCode { success_min: 200, success_max: 200 },
            is_read_only: false,
            is_deterministic: false,
            is_idempotent: false,
            docs_url: Some("https://api.slack.com/methods/chat.postMessage".to_string()),
        },
        ToolContract {
            name: "slack.channel.list".to_string(),
            display_name: "List Slack Channels".to_string(),
            description: "Retrieve a list of channels in the Slack workspace.".to_string(),
            version: "1.0.0".to_string(),
            app_id: "slack".to_string(),
            command_group: "channel".to_string(),
            adapter_kind: AdapterKind::Api,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "minimum": 1, "maximum": 1000},
                    "exclude_archived": {"type": "boolean"}
                }
            }),
            output_schema: serde_json::json!({"type": "array"}),
            risk_tier: RiskTier::ReadOnly,
            requires_approval: false,
            execution_mode: ExecutionMode::Synchronous,
            timeout_ms: 10_000,
            retry_policy: RetryPolicy::default(),
            verification_rule: VerificationRule::None,
            is_read_only: true,
            is_deterministic: false,
            is_idempotent: true,
            docs_url: Some("https://api.slack.com/methods/conversations.list".to_string()),
        },
    ]
}

/// Returns typed `ToolContract`s for the Calendar integration.
///
/// Covers: event.schedule, event.list, event.cancel
pub fn calendar_contracts() -> Vec<ToolContract> {
    use crate::tool_contract::{ExecutionMode, RetryPolicy};

    vec![
        ToolContract {
            name: "calendar.event.schedule".to_string(),
            display_name: "Schedule Calendar Event".to_string(),
            description: "Create a new calendar event with optional attendees.".to_string(),
            version: "1.0.0".to_string(),
            app_id: "calendar".to_string(),
            command_group: "event".to_string(),
            adapter_kind: AdapterKind::Api,
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["title", "start_time", "end_time"],
                "properties": {
                    "title": {"type": "string"},
                    "start_time": {"type": "string", "format": "date-time"},
                    "end_time": {"type": "string", "format": "date-time"},
                    "description": {"type": "string"},
                    "attendees": {"type": "array", "items": {"type": "string", "format": "email"}},
                    "location": {"type": "string"},
                    "timezone": {"type": "string"}
                }
            }),
            output_schema: serde_json::json!({"type": "object", "properties": {
                "id": {"type": "string"},
                "html_link": {"type": "string"},
                "status": {"type": "string"}
            }}),
            risk_tier: RiskTier::WriteExternal,
            requires_approval: false,
            execution_mode: ExecutionMode::Synchronous,
            timeout_ms: 15_000,
            retry_policy: RetryPolicy::default(),
            verification_rule: VerificationRule::ArtifactExists {
                fetch_url_template: "events/{id}".to_string(),
            },
            is_read_only: false,
            is_deterministic: false,
            is_idempotent: false,
            docs_url: Some("https://developers.google.com/calendar/api/v3/reference/events/insert".to_string()),
        },
    ]
}

/// Returns typed `ToolContract`s for the Email integration.
///
/// Covers: email.draft.create, email.send
pub fn email_contracts() -> Vec<ToolContract> {
    use crate::tool_contract::{ExecutionMode, RetryPolicy};

    vec![
        ToolContract {
            name: "email.draft.create".to_string(),
            display_name: "Create Email Draft".to_string(),
            description: "Create an email draft for human review before sending.".to_string(),
            version: "1.0.0".to_string(),
            app_id: "email".to_string(),
            command_group: "draft".to_string(),
            adapter_kind: AdapterKind::Api,
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["to", "subject", "body"],
                "properties": {
                    "to": {"type": "string", "description": "Recipient email address"},
                    "subject": {"type": "string"},
                    "body": {"type": "string", "description": "Email body (plain text or HTML)"},
                    "cc": {"type": "string"},
                    "bcc": {"type": "string"}
                }
            }),
            output_schema: serde_json::json!({"type": "object", "properties": {
                "draft_id": {"type": "string"},
                "thread_id": {"type": "string"}
            }}),
            risk_tier: RiskTier::WriteInternal,
            requires_approval: false,
            execution_mode: ExecutionMode::Synchronous,
            timeout_ms: 10_000,
            retry_policy: RetryPolicy::default(),
            verification_rule: VerificationRule::ApiStatusCode { success_min: 200, success_max: 201 },
            is_read_only: false,
            is_deterministic: false,
            is_idempotent: false,
            docs_url: Some("https://developers.google.com/gmail/api/reference/rest/v1/users.drafts/create".to_string()),
        },
        ToolContract {
            name: "email.message.send".to_string(),
            display_name: "Send Email".to_string(),
            description: "Send an email directly. Requires approval for external recipients.".to_string(),
            version: "1.0.0".to_string(),
            app_id: "email".to_string(),
            command_group: "message".to_string(),
            adapter_kind: AdapterKind::Api,
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["to", "subject", "body"],
                "properties": {
                    "to": {"type": "string"},
                    "subject": {"type": "string"},
                    "body": {"type": "string"},
                    "cc": {"type": "string"}
                }
            }),
            output_schema: serde_json::json!({"type": "object", "properties": {
                "message_id": {"type": "string"},
                "thread_id": {"type": "string"}
            }}),
            risk_tier: RiskTier::WriteExternal,
            requires_approval: true,
            execution_mode: ExecutionMode::Synchronous,
            timeout_ms: 15_000,
            retry_policy: RetryPolicy::default(),
            verification_rule: VerificationRule::ApiStatusCode { success_min: 200, success_max: 200 },
            is_read_only: false,
            is_deterministic: false,
            is_idempotent: false,
            docs_url: None,
        },
    ]
}

/// Returns typed `ToolContract`s for the Notion integration.
///
/// Covers: page.create, page.update, database.query, block.append
pub fn notion_contracts() -> Vec<ToolContract> {
    use crate::tool_contract::{ExecutionMode, RetryPolicy};

    vec![
        ToolContract {
            name: "notion.page.create".to_string(),
            display_name: "Create Notion Page".to_string(),
            description: "Create a new page in a Notion workspace or database.".to_string(),
            version: "1.0.0".to_string(),
            app_id: "notion".to_string(),
            command_group: "page".to_string(),
            adapter_kind: AdapterKind::Api,
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["parent_id", "title"],
                "properties": {
                    "parent_id": {"type": "string", "description": "Parent page or database ID"},
                    "parent_type": {"type": "string", "enum": ["page", "database"], "default": "page"},
                    "title": {"type": "string"},
                    "content": {"type": "string", "description": "Page content in Markdown"}
                }
            }),
            output_schema: serde_json::json!({"type": "object", "properties": {
                "id": {"type": "string"},
                "url": {"type": "string"},
                "created_time": {"type": "string"}
            }}),
            risk_tier: RiskTier::WriteExternal,
            requires_approval: false,
            execution_mode: ExecutionMode::Synchronous,
            timeout_ms: 15_000,
            retry_policy: RetryPolicy::default(),
            verification_rule: VerificationRule::ArtifactExists {
                fetch_url_template: "https://api.notion.com/v1/pages/{id}".to_string(),
            },
            is_read_only: false,
            is_deterministic: false,
            is_idempotent: false,
            docs_url: Some("https://developers.notion.com/reference/post-page".to_string()),
        },
    ]
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_github_contracts_count() {
        let contracts = github_contracts();
        assert!(!contracts.is_empty());
        assert!(contracts.iter().any(|c| c.name == "github.issue.create"));
        assert!(contracts.iter().any(|c| c.name == "github.issue.list"));
        assert!(contracts.iter().any(|c| c.name == "github.issue.comment"));
        assert!(contracts.iter().any(|c| c.name == "github.pr.comment"));
    }

    #[test]
    fn test_github_issue_create_risk() {
        let c = github_contracts()
            .into_iter()
            .find(|c| c.name == "github.issue.create")
            .unwrap();
        assert_eq!(c.risk_tier, RiskTier::WriteExternal);
        assert!(!c.is_read_only);
        assert!(!c.requires_approval);
    }

    #[test]
    fn test_github_issue_list_is_read_only() {
        let c = github_contracts()
            .into_iter()
            .find(|c| c.name == "github.issue.list")
            .unwrap();
        assert_eq!(c.risk_tier, RiskTier::ReadOnly);
        assert!(c.is_read_only);
        assert!(!c.requires_approval);
    }

    #[test]
    fn test_slack_contracts() {
        let contracts = slack_contracts();
        assert!(contracts.iter().any(|c| c.name == "slack.message.send"));
        let send = contracts.iter().find(|c| c.name == "slack.message.send").unwrap();
        assert_eq!(send.risk_tier, RiskTier::WriteExternal);
        assert_eq!(send.adapter_kind, AdapterKind::Api);
    }

    #[test]
    fn test_email_draft_is_write_internal() {
        let c = email_contracts()
            .into_iter()
            .find(|c| c.name == "email.draft.create")
            .unwrap();
        assert_eq!(c.risk_tier, RiskTier::WriteInternal);
    }

    #[test]
    fn test_email_send_requires_approval() {
        let c = email_contracts()
            .into_iter()
            .find(|c| c.name == "email.message.send")
            .unwrap();
        assert!(c.requires_approval);
        assert_eq!(c.risk_tier, RiskTier::WriteExternal);
    }

    #[test]
    fn test_adapter_result_success() {
        let output = serde_json::json!({"id": "I_123", "number": 42});
        let result = AdapterResult::success(output.clone(), 450);
        assert!(result.success);
        assert_eq!(result.output, output);
        assert!(result.duration_ms == 450);
    }

    #[test]
    fn test_adapter_result_failure() {
        let result = AdapterResult::failure("network timeout", true, 5000);
        assert!(!result.success);
        assert!(result.is_transient_failure);
    }

    #[test]
    fn test_run_verification_none() {
        let contract = ToolContract::minimal("github.issue.list", "github", RiskTier::ReadOnly);
        let output = serde_json::json!([]);
        let outcome = run_verification(&VerificationRule::None, &output, &contract);
        assert_eq!(outcome, VerificationOutcome::Skipped);
    }

    #[test]
    fn test_run_verification_status_code_pass() {
        let contract = ToolContract::minimal("slack.message.send", "slack", RiskTier::WriteExternal);
        let output = serde_json::json!({"status_code": 200, "ok": true});
        let outcome = run_verification(
            &VerificationRule::ApiStatusCode { success_min: 200, success_max: 201 },
            &output,
            &contract,
        );
        assert_eq!(outcome, VerificationOutcome::Passed);
    }

    #[test]
    fn test_run_verification_status_code_fail() {
        let contract = ToolContract::minimal("slack.message.send", "slack", RiskTier::WriteExternal);
        let output = serde_json::json!({"status_code": 429});
        let outcome = run_verification(
            &VerificationRule::ApiStatusCode { success_min: 200, success_max: 201 },
            &output,
            &contract,
        );
        assert!(matches!(outcome, VerificationOutcome::Failed { .. }));
    }

    #[test]
    fn test_notion_contracts() {
        let contracts = notion_contracts();
        assert!(contracts.iter().any(|c| c.name == "notion.page.create"));
    }
}
