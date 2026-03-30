//! OpenCode CLI backend driver.
//!
//! Spawns the `opencode` CLI as a subprocess in non-interactive `run` mode.
//! The driver is intentionally conservative about flags to maximize compatibility
//! across OpenCode CLI versions.

use crate::llm_driver::{CompletionRequest, CompletionResponse, LlmDriver, LlmError, StreamEvent};
use async_trait::async_trait;
use openfang_types::message::{ContentBlock, Role, StopReason, TokenUsage};
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;
use tokio::io::AsyncBufReadExt;
use tracing::{debug, warn};

/// Environment variable names to strip from the subprocess to prevent
/// leaking API keys from other providers.
const SENSITIVE_ENV_EXACT: &[&str] = &[
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "GROQ_API_KEY",
    "DEEPSEEK_API_KEY",
    "MISTRAL_API_KEY",
    "TOGETHER_API_KEY",
    "FIREWORKS_API_KEY",
    "OPENROUTER_API_KEY",
    "PERPLEXITY_API_KEY",
    "COHERE_API_KEY",
    "AI21_API_KEY",
    "CEREBRAS_API_KEY",
    "SAMBANOVA_API_KEY",
    "HUGGINGFACE_API_KEY",
    "XAI_API_KEY",
    "REPLICATE_API_TOKEN",
    "BRAVE_API_KEY",
    "TAVILY_API_KEY",
    "ELEVENLABS_API_KEY",
];

/// Suffixes that indicate a secret — remove any env var ending with these
/// unless it starts with `OPENCODE_`.
const SENSITIVE_SUFFIXES: &[&str] = &["_SECRET", "_TOKEN", "_PASSWORD"];

/// LLM driver that delegates to the OpenCode CLI.
pub struct OpenCodeDriver {
    cli_path: String,
    skip_permissions: bool,
}

impl OpenCodeDriver {
    /// Create a new OpenCode driver.
    ///
    /// `cli_path` overrides the CLI binary path; defaults to `"opencode"` on PATH.
    pub fn new(cli_path: Option<String>, skip_permissions: bool) -> Self {
        if skip_permissions {
            warn!(
                "OpenCode driver: skip_permissions requested, but no stable non-interactive \
                 permission flag is assumed for compatibility across OpenCode versions."
            );
        }

        Self {
            cli_path: cli_path
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "opencode".to_string()),
            skip_permissions,
        }
    }

    /// Detect if the OpenCode CLI is available on PATH.
    pub fn detect() -> Option<String> {
        let output = std::process::Command::new("opencode")
            .arg("--version")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;

        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            None
        }
    }

    /// Build a text prompt from completion request messages.
    fn build_prompt(request: &CompletionRequest) -> String {
        let mut parts = Vec::new();

        if let Some(ref sys) = request.system {
            parts.push(format!("[System]\n{sys}"));
        }

        for msg in &request.messages {
            let role_label = match msg.role {
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::System => "System",
            };
            let text = msg.content.text_content();
            if !text.is_empty() {
                parts.push(format!("[{role_label}]\n{text}"));
            }
        }

        parts.join("\n\n")
    }

    /// Map a model ID like "opencode/sonnet" to CLI --model flag value.
    fn model_flag(model: &str) -> Option<String> {
        let model = model.trim();
        if model.is_empty() {
            return None;
        }

        match model {
            // Backward-compatible aliases used by older manifests.
            "sonnet" | "opencode/sonnet" | "opencode-sonnet" => {
                Some("opencode/mimo-v2-omni-free".to_string())
            }
            "opus" | "opencode/opus" | "opencode-opus" => Some("opencode/big-pickle".to_string()),
            "haiku" | "opencode/haiku" | "opencode-haiku" => {
                Some("opencode/gpt-5-nano".to_string())
            }
            // Keep fully-qualified IDs as-is, otherwise default to opencode namespace.
            _ if model.contains('/') => Some(model.to_string()),
            _ => Some(format!("opencode/{model}")),
        }
    }

    /// Apply security env filtering to a command.
    fn apply_env_filter(cmd: &mut tokio::process::Command) {
        for key in SENSITIVE_ENV_EXACT {
            cmd.env_remove(key);
        }
        for (key, _) in std::env::vars() {
            if key.starts_with("OPENCODE_") {
                continue;
            }
            let upper = key.to_uppercase();
            for suffix in SENSITIVE_SUFFIXES {
                if upper.ends_with(suffix) {
                    cmd.env_remove(&key);
                    break;
                }
            }
        }
    }

    fn build_base_command(&self, request: &CompletionRequest) -> tokio::process::Command {
        let prompt = Self::build_prompt(request);
        let mut cmd = tokio::process::Command::new(&self.cli_path);
        // OpenCode non-interactive mode is `run`, not the top-level TUI command.
        // `--format json` yields machine-readable events for both complete + stream.
        cmd.arg("run").arg(prompt).arg("--format").arg("json");

        if let Some(model) = Self::model_flag(&request.model) {
            cmd.arg("--model").arg(model);
        }

        // Align CLI cwd with the agent workspace exposed in the system prompt
        // (e.g. /data/workspaces/<agent-name>) so file operations are scoped correctly.
        if let Some(workspace_dir) = Self::workspace_dir_from_request(request) {
            cmd.current_dir(workspace_dir);
        }

        Self::apply_env_filter(&mut cmd);
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd
    }

    fn workspace_dir_from_request(request: &CompletionRequest) -> Option<PathBuf> {
        let system = request.system.as_deref()?;
        for line in system.lines() {
            let trimmed = line.trim();
            if let Some(path_str) = trimmed.strip_prefix("Workspace: ") {
                let path = PathBuf::from(path_str.trim());
                if path.exists() {
                    return Some(path);
                }
            }
        }
        None
    }

    fn parse_usage(value: &Value) -> Option<TokenUsage> {
        let usage = value
            .get("usage")
            .or_else(|| value.get("part").and_then(|p| p.get("tokens")))?;
        let input_tokens = usage
            .get("input_tokens")
            .or_else(|| usage.get("prompt_tokens"))
            .or_else(|| usage.get("input"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let output_tokens = usage
            .get("output_tokens")
            .or_else(|| usage.get("completion_tokens"))
            .or_else(|| usage.get("output"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        Some(TokenUsage {
            input_tokens,
            output_tokens,
        })
    }

    fn extract_text_field(value: &Value) -> Option<String> {
        fn from_value(v: &Value) -> Option<String> {
            match v {
                Value::String(s) if !s.is_empty() => Some(s.clone()),
                Value::Array(arr) => {
                    let joined = arr
                        .iter()
                        .filter_map(from_value)
                        .collect::<Vec<_>>()
                        .join("");
                    if joined.is_empty() {
                        None
                    } else {
                        Some(joined)
                    }
                }
                Value::Object(map) => {
                    if let Some(p) = map.get("part") {
                        return from_value(p);
                    }
                    if let Some(c) = map.get("content") {
                        return from_value(c);
                    }
                    if let Some(t) = map.get("text") {
                        return from_value(t);
                    }
                    if let Some(d) = map.get("delta") {
                        return from_value(d);
                    }
                    if let Some(o) = map.get("output") {
                        return from_value(o);
                    }
                    if let Some(r) = map.get("result") {
                        return from_value(r);
                    }
                    if let Some(s) = map.get("state") {
                        return from_value(s);
                    }
                    if let Some(m) = map.get("metadata") {
                        return from_value(m);
                    }
                    None
                }
                _ => None,
            }
        }

        for key in [
            "delta", "text", "content", "result", "output", "message", "part", "state",
        ] {
            if let Some(v) = value.get(key) {
                if let Some(text) = from_value(v) {
                    return Some(text);
                }
            }
        }

        if let Some(part) = value.get("part") {
            return from_value(part);
        }

        None
    }

    fn parse_stream_line(line: &str) -> Option<(Option<String>, bool, Option<TokenUsage>)> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        let payload = if let Some(rest) = trimmed.strip_prefix("data:") {
            rest.trim()
        } else {
            trimmed
        };

        if payload == "[DONE]" {
            return Some((None, true, None));
        }

        let value: Value = serde_json::from_str(payload).ok()?;
        let event_type = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let is_terminal = matches!(event_type, "done" | "complete" | "result" | "end");
        let text = Self::extract_text_field(&value);
        let usage = Self::parse_usage(&value);
        Some((text, is_terminal, usage))
    }
}

#[derive(Debug, Deserialize)]
struct OpenCodeJsonOutput {
    #[serde(default)]
    result: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    output: Option<String>,
}

#[async_trait]
impl LlmDriver for OpenCodeDriver {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let mut cmd = self.build_base_command(&request);

        debug!(
            cli = %self.cli_path,
            skip_permissions = self.skip_permissions,
            "Spawning OpenCode CLI"
        );

        let output = cmd.output().await.map_err(|e| {
            LlmError::Http(format!(
                "OpenCode CLI not found or failed to start ({}). Install: npm install -g opencode-ai && opencode",
                e
            ))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let detail = if !stderr.is_empty() { &stderr } else { &stdout };
            let code = output.status.code().unwrap_or(1);

            let message = if detail.contains("not authenticated")
                || detail.contains("auth")
                || detail.contains("login")
                || detail.contains("credentials")
            {
                format!("OpenCode CLI is not authenticated. Run OpenCode login/auth first. Detail: {detail}")
            } else {
                format!("OpenCode CLI exited with code {code}: {detail}")
            };

            return Err(LlmError::Api {
                status: code as u16,
                message,
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Preferred path: parse line-delimited JSON events from `opencode run --format json`.
        let mut text_from_events = String::new();
        let mut usage_from_events = TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
        };
        let mut saw_event = false;
        for line in stdout.lines() {
            if let Some((maybe_text, _is_terminal, maybe_usage)) = Self::parse_stream_line(line) {
                saw_event = true;
                if let Some(text) = maybe_text {
                    text_from_events.push_str(&text);
                }
                if let Some(usage) = maybe_usage {
                    usage_from_events = usage;
                }
            }
        }
        if saw_event {
            return Ok(CompletionResponse {
                content: vec![ContentBlock::Text {
                    text: text_from_events,
                    provider_metadata: None,
                }],
                stop_reason: StopReason::EndTurn,
                tool_calls: Vec::new(),
                usage: usage_from_events,
            });
        }

        if let Ok(parsed) = serde_json::from_str::<OpenCodeJsonOutput>(&stdout) {
            let text = parsed
                .result
                .or(parsed.content)
                .or(parsed.text)
                .or(parsed.message)
                .or(parsed.output)
                .unwrap_or_default();
            return Ok(CompletionResponse {
                content: vec![ContentBlock::Text {
                    text,
                    provider_metadata: None,
                }],
                stop_reason: StopReason::EndTurn,
                tool_calls: Vec::new(),
                usage: TokenUsage {
                    input_tokens: 0,
                    output_tokens: 0,
                },
            });
        }

        let text = stdout.trim().to_string();
        Ok(CompletionResponse {
            content: vec![ContentBlock::Text {
                text,
                provider_metadata: None,
            }],
            stop_reason: StopReason::EndTurn,
            tool_calls: Vec::new(),
            usage: TokenUsage {
                input_tokens: 0,
                output_tokens: 0,
            },
        })
    }

    async fn stream(
        &self,
        request: CompletionRequest,
        tx: tokio::sync::mpsc::Sender<StreamEvent>,
    ) -> Result<CompletionResponse, LlmError> {
        let mut cmd = self.build_base_command(&request);

        debug!(
            cli = %self.cli_path,
            skip_permissions = self.skip_permissions,
            "Spawning OpenCode CLI (streaming)"
        );

        let mut child = cmd.spawn().map_err(|e| {
            LlmError::Http(format!(
                "OpenCode CLI not found or failed to start ({}). Install: npm install -g opencode-ai",
                e
            ))
        })?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| LlmError::Http("No stdout from opencode CLI".to_string()))?;
        let reader = tokio::io::BufReader::new(stdout);
        let mut lines = reader.lines();

        let mut full_text = String::new();
        let mut final_usage = TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
        };

        while let Ok(Some(line)) = lines.next_line().await {
            if let Some((maybe_text, _is_terminal, maybe_usage)) = Self::parse_stream_line(&line) {
                if let Some(text) = maybe_text {
                    if !text.is_empty() {
                        full_text.push_str(&text);
                        let _ = tx.send(StreamEvent::TextDelta { text }).await;
                    }
                }
                if let Some(usage) = maybe_usage {
                    final_usage = usage;
                }
                continue;
            }

            // Non-JSON stream chunks are treated as text deltas.
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                full_text.push_str(trimmed);
                let _ = tx
                    .send(StreamEvent::TextDelta {
                        text: trimmed.to_string(),
                    })
                    .await;
            }
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| LlmError::Http(format!("OpenCode CLI wait failed: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let detail = if !stderr.is_empty() { &stderr } else { &stdout };
            let code = output.status.code().unwrap_or(1);
            return Err(LlmError::Api {
                status: code as u16,
                message: format!("OpenCode CLI streaming exited with code {code}: {detail}"),
            });
        }

        if full_text.is_empty() {
            // Fallback: some CLI versions may buffer and emit only final stdout.
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !stdout.is_empty() {
                full_text = stdout.clone();
                let _ = tx.send(StreamEvent::TextDelta { text: stdout }).await;
            }
        }

        let _ = tx
            .send(StreamEvent::ContentComplete {
                stop_reason: StopReason::EndTurn,
                usage: final_usage,
            })
            .await;

        Ok(CompletionResponse {
            content: vec![ContentBlock::Text {
                text: full_text,
                provider_metadata: None,
            }],
            stop_reason: StopReason::EndTurn,
            tool_calls: Vec::new(),
            usage: final_usage,
        })
    }
}

/// Check if the OpenCode CLI is available.
pub fn opencode_available() -> bool {
    OpenCodeDriver::detect().is_some() || opencode_credentials_exist()
}

/// Check if OpenCode credentials exist.
fn opencode_credentials_exist() -> bool {
    if let Some(home) = home_dir() {
        let opencode_dir = home.join(".opencode");
        let xdg_config = home.join(".config").join("opencode");
        opencode_dir.join("auth.json").exists()
            || opencode_dir.join("credentials.json").exists()
            || xdg_config.join("auth.json").exists()
            || xdg_config.join("credentials.json").exists()
    } else {
        false
    }
}

/// Cross-platform home directory.
fn home_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .ok()
            .map(std::path::PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok().map(std::path::PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_flag_mapping() {
        assert_eq!(
            OpenCodeDriver::model_flag("opencode/sonnet"),
            Some("opencode/mimo-v2-omni-free".to_string())
        );
        assert_eq!(
            OpenCodeDriver::model_flag("opencode/opus"),
            Some("opencode/big-pickle".to_string())
        );
        assert_eq!(
            OpenCodeDriver::model_flag("opencode/haiku"),
            Some("opencode/gpt-5-nano".to_string())
        );
        assert_eq!(
            OpenCodeDriver::model_flag("custom-model"),
            Some("opencode/custom-model".to_string())
        );
        assert_eq!(
            OpenCodeDriver::model_flag("opencode/mimo-v2-omni-free"),
            Some("opencode/mimo-v2-omni-free".to_string())
        );
    }

    #[test]
    fn test_new_defaults_to_opencode() {
        let driver = OpenCodeDriver::new(None, true);
        assert_eq!(driver.cli_path, "opencode");
        assert!(driver.skip_permissions);
    }

    #[test]
    fn test_new_with_custom_path() {
        let driver = OpenCodeDriver::new(Some("/usr/local/bin/opencode".to_string()), true);
        assert_eq!(driver.cli_path, "/usr/local/bin/opencode");
    }

    #[test]
    fn test_build_prompt_includes_system_and_user() {
        use openfang_types::message::{Message, MessageContent};

        let request = CompletionRequest {
            model: "opencode/sonnet".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::text("Hello"),
            }],
            tools: vec![],
            max_tokens: 1024,
            temperature: 0.7,
            system: Some("You are helpful.".to_string()),
            thinking: None,
        };

        let prompt = OpenCodeDriver::build_prompt(&request);
        assert!(prompt.contains("[System]"));
        assert!(prompt.contains("[User]"));
        assert!(prompt.contains("Hello"));
    }

    #[test]
    fn test_parse_stream_line_data_json() {
        let parsed =
            OpenCodeDriver::parse_stream_line("data: {\"type\":\"delta\",\"text\":\"Hello\"}")
                .unwrap();
        assert_eq!(parsed.0.unwrap(), "Hello");
        assert!(!parsed.1);
    }

    #[test]
    fn test_parse_stream_line_done() {
        let parsed = OpenCodeDriver::parse_stream_line("data: [DONE]").unwrap();
        assert!(parsed.0.is_none());
        assert!(parsed.1);
    }

    #[test]
    fn test_parse_stream_line_with_usage() {
        let parsed = OpenCodeDriver::parse_stream_line(
            "{\"type\":\"result\",\"result\":\"ok\",\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":5}}",
        )
        .unwrap();
        let usage = parsed.2.unwrap();
        assert_eq!(usage.input_tokens, 12);
        assert_eq!(usage.output_tokens, 5);
    }

    #[test]
    fn test_parse_stream_line_part_text_and_tokens() {
        let parsed = OpenCodeDriver::parse_stream_line(
            "{\"type\":\"text\",\"part\":{\"text\":\"OK\",\"tokens\":{\"input\":9,\"output\":2}}}",
        )
        .unwrap();
        assert_eq!(parsed.0.as_deref(), Some("OK"));
        let usage = parsed.2.unwrap();
        assert_eq!(usage.input_tokens, 9);
        assert_eq!(usage.output_tokens, 2);
    }

    #[test]
    fn test_parse_stream_line_tool_use_output() {
        let parsed = OpenCodeDriver::parse_stream_line(
            "{\"type\":\"tool_use\",\"part\":{\"state\":{\"output\":\"OK\\n\"}}}",
        )
        .unwrap();
        assert_eq!(parsed.0.as_deref(), Some("OK\n"));
        assert!(!parsed.1);
    }

    #[test]
    fn test_workspace_dir_from_system_prompt() {
        use openfang_types::message::{Message, MessageContent};

        let request = CompletionRequest {
            model: "opencode/sonnet".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::text("Hello"),
            }],
            tools: vec![],
            max_tokens: 256,
            temperature: 0.7,
            system: Some("## Workspace\nWorkspace: /does/not/exist".to_string()),
            thinking: None,
        };

        // Should not return non-existent paths.
        assert!(OpenCodeDriver::workspace_dir_from_request(&request).is_none());
    }
}
