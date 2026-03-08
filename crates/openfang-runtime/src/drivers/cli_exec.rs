//! Generic CLI exec driver.
//!
//! Spawns any CLI tool as a subprocess, supporting configurable command, args,
//! input mode (stdin vs arg), and output parsing (text vs JSON).
//! This allows using CLI tools like `claude`, `codex`, `gemini` as LLM providers
//! without needing direct API keys — the CLI handles its own authentication.

use crate::llm_driver::{CompletionRequest, CompletionResponse, LlmDriver, LlmError, StreamEvent};
use async_trait::async_trait;
use openfang_types::config::{CliBackendConfig, CliInputMode, CliOutputFormat};
use openfang_types::message::{ContentBlock, Role, StopReason, TokenUsage};
use tokio::io::AsyncWriteExt;
use tracing::{debug, warn};

/// LLM driver that delegates to an arbitrary CLI tool.
pub struct CliExecDriver {
    config: CliBackendConfig,
}

impl CliExecDriver {
    /// Create a new CLI exec driver from a backend config.
    pub fn new(config: CliBackendConfig) -> Self {
        Self { config }
    }

    /// Check if the configured command is available on PATH.
    pub fn detect(command: &str) -> bool {
        std::process::Command::new(command)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Build a text prompt from the completion request messages.
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

    /// Extract the raw model name from a qualified model ID.
    ///
    /// Strips "cli-exec/{backend_id}/" prefix if present.
    /// Examples:
    ///   "cli-exec/codex/gpt-5.4" → "gpt-5.4"
    ///   "codex/gpt-5.4"          → "gpt-5.4"
    ///   "gpt-5.4"                → "gpt-5.4"
    ///   "opus"                   → "opus"
    fn extract_model<'a>(model: &'a str, backend_id: &str) -> &'a str {
        let stripped = model.strip_prefix("cli-exec/").unwrap_or(model);
        let stripped = stripped
            .strip_prefix(backend_id)
            .and_then(|s| s.strip_prefix('/'))
            .unwrap_or(stripped);
        stripped
    }

    /// Extract text from a JSON value using configured field names.
    fn extract_json_text(&self, value: &serde_json::Value) -> Option<String> {
        for field in &self.config.json_text_fields {
            if let Some(serde_json::Value::String(s)) = value.get(field) {
                if !s.is_empty() {
                    return Some(s.clone());
                }
            }
        }
        None
    }
}

#[async_trait]
impl LlmDriver for CliExecDriver {
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, LlmError> {
        let prompt = Self::build_prompt(&request);
        let model = Self::extract_model(&request.model, &self.config.id);

        let mut cmd = tokio::process::Command::new(&self.config.command);

        // Add configured args
        for arg in &self.config.args {
            cmd.arg(arg);
        }

        // Add model flag if configured
        if !self.config.model_arg.is_empty() && !model.is_empty() {
            cmd.arg(&self.config.model_arg).arg(model);
        }

        // Handle input mode
        match self.config.input {
            CliInputMode::Arg => {
                cmd.arg(&prompt);
                cmd.stdin(std::process::Stdio::null());
            }
            CliInputMode::Stdin => {
                cmd.stdin(std::process::Stdio::piped());
            }
        }

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        // Inherit safe env vars only — explicitly exclude CLAUDECODE so nested
        // claude CLI invocations are not blocked by the parent process guard.
        cmd.env_clear();
        for var in &["PATH", "HOME", "USER", "LANG", "TERM", "SHELL", "TMPDIR",
                     "XDG_CONFIG_HOME", "XDG_DATA_HOME"] {
            if let Ok(val) = std::env::var(var) {
                cmd.env(var, val);
            }
        }

        debug!(
            command = %self.config.command,
            backend = %self.config.id,
            model = %model,
            "Spawning CLI exec"
        );

        let mut child = cmd
            .spawn()
            .map_err(|e| LlmError::Http(format!(
                "Failed to spawn '{}': {e}. Is it installed?",
                self.config.command
            )))?;

        // Write prompt to stdin if needed
        if matches!(self.config.input, CliInputMode::Stdin) {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(prompt.as_bytes()).await.map_err(|e| {
                    LlmError::Http(format!("Failed to write to stdin: {e}"))
                })?;
                drop(stdin);
            }
        }

        // Apply timeout
        let timeout_duration = if self.config.timeout_secs > 0 {
            std::time::Duration::from_secs(self.config.timeout_secs)
        } else {
            std::time::Duration::from_secs(600)
        };

        let output = tokio::time::timeout(timeout_duration, child.wait_with_output())
            .await
            .map_err(|_| LlmError::Http(format!(
                "CLI '{}' timed out after {}s",
                self.config.command, self.config.timeout_secs
            )))?
            .map_err(|e| LlmError::Http(format!(
                "CLI '{}' failed: {e}",
                self.config.command
            )))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            warn!(
                command = %self.config.command,
                code = ?output.status.code(),
                stderr = %stderr,
                "CLI exec failed"
            );
            // Some CLIs print the result to stdout even with non-zero exit
            if !stdout.trim().is_empty() {
                return Ok(CompletionResponse {
                    content: vec![ContentBlock::Text {
                        text: stdout.trim().to_string(),
                    }],
                    stop_reason: StopReason::EndTurn,
                    tool_calls: Vec::new(),
                    usage: TokenUsage::default(),
                });
            }
            return Err(LlmError::Api {
                status: output.status.code().unwrap_or(1) as u16,
                message: format!("CLI '{}' failed: {stderr}", self.config.command),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse output based on format
        let text = match self.config.output {
            CliOutputFormat::Json => {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&stdout) {
                    self.extract_json_text(&parsed)
                        .unwrap_or_else(|| stdout.trim().to_string())
                } else {
                    stdout.trim().to_string()
                }
            }
            CliOutputFormat::Text => stdout.trim().to_string(),
        };

        Ok(CompletionResponse {
            content: vec![ContentBlock::Text { text }],
            stop_reason: StopReason::EndTurn,
            tool_calls: Vec::new(),
            usage: TokenUsage::default(),
        })
    }

    async fn stream(
        &self,
        request: CompletionRequest,
        tx: tokio::sync::mpsc::Sender<StreamEvent>,
    ) -> Result<CompletionResponse, LlmError> {
        // Generic CLIs don't support streaming — delegate to complete()
        let response = self.complete(request).await?;
        let text = response.text();
        if !text.is_empty() {
            let _ = tx.send(StreamEvent::TextDelta { text }).await;
        }
        let _ = tx
            .send(StreamEvent::ContentComplete {
                stop_reason: response.stop_reason,
                usage: response.usage,
            })
            .await;
        Ok(response)
    }
}

/// Built-in CLI backend presets for common tools.
pub fn builtin_backends() -> Vec<CliBackendConfig> {
    vec![
        CliBackendConfig {
            id: "claude-code".into(),
            command: "claude".into(),
            args: vec!["--print".into()],
            output: CliOutputFormat::Json,
            input: CliInputMode::Stdin,
            model_arg: "--model".into(),
            json_text_fields: vec!["result".into(), "content".into(), "text".into()],
            timeout_secs: 300,
        },
        CliBackendConfig {
            id: "codex".into(),
            command: "codex".into(),
            args: vec!["exec".into()],
            output: CliOutputFormat::Text,
            input: CliInputMode::Arg,
            model_arg: "--model".into(),
            json_text_fields: vec![],
            timeout_secs: 300,
        },
        CliBackendConfig {
            id: "gemini".into(),
            command: "gemini".into(),
            args: vec!["-p".into()],
            output: CliOutputFormat::Text,
            input: CliInputMode::Arg,
            model_arg: "--model".into(),
            json_text_fields: vec![],
            timeout_secs: 300,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_types::message::{Message, MessageContent};

    fn test_config() -> CliBackendConfig {
        CliBackendConfig {
            id: "test-cli".into(),
            command: "echo".into(),
            args: vec![],
            output: CliOutputFormat::Text,
            input: CliInputMode::Arg,
            model_arg: "--model".into(),
            json_text_fields: vec!["result".into(), "text".into()],
            timeout_secs: 10,
        }
    }

    #[test]
    fn test_extract_model_full_prefix() {
        assert_eq!(
            CliExecDriver::extract_model("cli-exec/codex/gpt-5.4", "codex"),
            "gpt-5.4"
        );
    }

    #[test]
    fn test_extract_model_backend_prefix() {
        assert_eq!(
            CliExecDriver::extract_model("codex/gpt-5.4", "codex"),
            "gpt-5.4"
        );
    }

    #[test]
    fn test_extract_model_raw() {
        assert_eq!(
            CliExecDriver::extract_model("opus", "claude-code"),
            "opus"
        );
    }

    #[test]
    fn test_extract_model_claude_code() {
        assert_eq!(
            CliExecDriver::extract_model("cli-exec/claude-code/opus", "claude-code"),
            "opus"
        );
    }

    #[test]
    fn test_build_prompt() {
        let request = CompletionRequest {
            model: "test".into(),
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::text("Hello"),
            }],
            tools: vec![],
            max_tokens: 1024,
            temperature: 0.7,
            system: Some("Be helpful.".into()),
            thinking: None,
        };

        let prompt = CliExecDriver::build_prompt(&request);
        assert!(prompt.contains("[System]"));
        assert!(prompt.contains("Be helpful."));
        assert!(prompt.contains("[User]"));
        assert!(prompt.contains("Hello"));
    }

    #[test]
    fn test_extract_json_text() {
        let driver = CliExecDriver::new(test_config());
        let value = serde_json::json!({"result": "Hello world", "other": 42});
        assert_eq!(driver.extract_json_text(&value), Some("Hello world".into()));
    }

    #[test]
    fn test_extract_json_text_fallback_field() {
        let driver = CliExecDriver::new(test_config());
        let value = serde_json::json!({"text": "Fallback text"});
        assert_eq!(driver.extract_json_text(&value), Some("Fallback text".into()));
    }

    #[test]
    fn test_extract_json_text_none() {
        let driver = CliExecDriver::new(test_config());
        let value = serde_json::json!({"unrelated": "data"});
        assert_eq!(driver.extract_json_text(&value), None);
    }

    #[test]
    fn test_detect_missing_binary() {
        assert!(!CliExecDriver::detect("nonexistent_binary_xxx_12345"));
    }

    #[test]
    fn test_new_from_config() {
        let config = test_config();
        let driver = CliExecDriver::new(config.clone());
        assert_eq!(driver.config.id, "test-cli");
        assert_eq!(driver.config.command, "echo");
    }

    #[tokio::test]
    async fn test_complete_with_echo() {
        let config = CliBackendConfig {
            id: "echo-test".into(),
            command: "echo".into(),
            args: vec![],
            output: CliOutputFormat::Text,
            input: CliInputMode::Arg,
            model_arg: String::new(),
            json_text_fields: vec![],
            timeout_secs: 5,
        };
        let driver = CliExecDriver::new(config);

        let request = CompletionRequest {
            model: "test".into(),
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::text("hello world"),
            }],
            tools: vec![],
            max_tokens: 100,
            temperature: 0.0,
            system: None,
            thinking: None,
        };

        let response = driver.complete(request).await.unwrap();
        let text = response.text();
        assert!(text.contains("hello world"));
    }

    /// Live integration test with real claude CLI.
    /// Run with: OPENFANG_LIVE_CLI_TEST=1 cargo test -p openfang-runtime test_live_claude_cli -- --nocapture
    #[tokio::test]
    async fn test_live_claude_cli() {
        if std::env::var("OPENFANG_LIVE_CLI_TEST").is_err() {
            eprintln!("OPENFANG_LIVE_CLI_TEST not set, skipping live CLI test");
            return;
        }

        let backends = super::builtin_backends();
        let claude_config = backends
            .into_iter()
            .find(|b| b.id == "claude-code")
            .expect("claude-code backend should exist");

        let driver = CliExecDriver::new(claude_config);

        let request = CompletionRequest {
            model: "cli-exec/claude-code/claude-sonnet-4-6".into(),
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::text("Respond with exactly one word: PONG"),
            }],
            tools: vec![],
            max_tokens: 50,
            temperature: 0.0,
            system: Some("You respond with exactly the word requested, nothing else.".into()),
            thinking: None,
        };

        let response = driver.complete(request).await;
        assert!(response.is_ok(), "Claude CLI should succeed: {:?}", response.err());
        let resp = response.unwrap();
        let text = resp.text();
        eprintln!("Claude CLI response: {text:?}");
        assert!(!text.is_empty(), "Response should not be empty");
        assert!(
            text.to_uppercase().contains("PONG"),
            "Response should contain PONG, got: {text}"
        );
    }
}
