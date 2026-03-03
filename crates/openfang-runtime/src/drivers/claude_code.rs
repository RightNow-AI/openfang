//! Claude Code CLI backend driver.
//!
//! Spawns the `claude` CLI (Claude Code) as a subprocess in print mode (`-p`),
//! which is non-interactive and handles its own authentication.
//! This allows users with Claude Code installed to use it as an LLM provider
//! without needing a separate API key.

use crate::llm_driver::{CompletionRequest, CompletionResponse, LlmDriver, LlmError, StreamEvent};
use async_trait::async_trait;
use openfang_types::message::{ContentBlock, Role, StopReason, TokenUsage};
use serde::Deserialize;
use tokio::io::AsyncBufReadExt;
use tracing::{debug, warn};

/// LLM driver that delegates to the Claude Code CLI.
pub struct ClaudeCodeDriver {
    cli_path: String,
}

impl ClaudeCodeDriver {
    /// Create a new Claude Code driver.
    ///
    /// `cli_path` overrides the CLI binary path; defaults to `"claude"` on PATH.
    pub fn new(cli_path: Option<String>) -> Self {
        Self {
            cli_path: cli_path
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "claude".to_string()),
        }
    }

    /// Detect if the Claude Code CLI is available on PATH.
    pub fn detect() -> Option<String> {
        let output = std::process::Command::new("claude")
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

    /// Map a model ID like "claude-code/opus" to CLI --model flag value.
    fn model_flag(model: &str) -> Option<String> {
        let stripped = model
            .strip_prefix("claude-code/")
            .unwrap_or(model);
        match stripped {
            "opus" => Some("opus".to_string()),
            "sonnet" => Some("sonnet".to_string()),
            "haiku" => Some("haiku".to_string()),
            _ => Some(stripped.to_string()),
        }
    }
}

/// JSON output from `claude -p --output-format json`.
#[derive(Debug, Deserialize)]
struct ClaudeJsonOutput {
    result: Option<String>,
    #[serde(default)]
    usage: Option<ClaudeUsage>,
    #[serde(default)]
    #[allow(dead_code)]
    cost_usd: Option<f64>,
}

/// Usage stats from Claude CLI JSON output.
#[derive(Debug, Deserialize, Default)]
struct ClaudeUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
}

/// Stream JSON event from `claude -p --output-format stream-json`.
#[derive(Debug, Deserialize)]
struct ClaudeStreamEvent {
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    result: Option<String>,
    #[serde(default)]
    usage: Option<ClaudeUsage>,
}

#[async_trait]
impl LlmDriver for ClaudeCodeDriver {
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, LlmError> {
        let prompt = Self::build_prompt(&request);
        let model_flag = Self::model_flag(&request.model);

        let mut cmd = tokio::process::Command::new(&self.cli_path);
        cmd.arg("-p")
            .arg(&prompt)
            .arg("--output-format")
            .arg("json");

        if let Some(ref model) = model_flag {
            cmd.arg("--model").arg(model);
        }

        // SECURITY: Don't inherit all env vars — only safe ones
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        debug!(cli = %self.cli_path, "Spawning Claude Code CLI");

        let output = cmd
            .output()
            .await
            .map_err(|e| LlmError::Http(format!("Failed to spawn claude CLI: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(LlmError::Api {
                status: output.status.code().unwrap_or(1) as u16,
                message: format!("Claude CLI failed: {stderr}"),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Try JSON parse first
        if let Ok(parsed) = serde_json::from_str::<ClaudeJsonOutput>(&stdout) {
            let text = parsed.result.unwrap_or_default();
            let usage = parsed.usage.unwrap_or_default();
            return Ok(CompletionResponse {
                content: vec![ContentBlock::Text { text: text.clone() }],
                stop_reason: StopReason::EndTurn,
                tool_calls: Vec::new(),
                usage: TokenUsage {
                    input_tokens: usage.input_tokens,
                    output_tokens: usage.output_tokens,
                },
            });
        }

        // Fallback: treat entire stdout as plain text
        let text = stdout.trim().to_string();
        Ok(CompletionResponse {
            content: vec![ContentBlock::Text { text }],
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
        let prompt = Self::build_prompt(&request);
        let model_flag = Self::model_flag(&request.model);

        let mut cmd = tokio::process::Command::new(&self.cli_path);
        cmd.arg("-p")
            .arg(&prompt)
            .arg("--output-format")
            .arg("stream-json");

        if let Some(ref model) = model_flag {
            cmd.arg("--model").arg(model);
        }

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        debug!(cli = %self.cli_path, "Spawning Claude Code CLI (streaming)");

        let mut child = cmd
            .spawn()
            .map_err(|e| LlmError::Http(format!("Failed to spawn claude CLI: {e}")))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| LlmError::Http("No stdout from claude CLI".to_string()))?;

        let reader = tokio::io::BufReader::new(stdout);
        let mut lines = reader.lines();

        let mut full_text = String::new();
        let mut final_usage = TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
        };

        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<ClaudeStreamEvent>(&line) {
                Ok(event) => {
                    match event.r#type.as_str() {
                        "content" | "text" => {
                            if let Some(ref content) = event.content {
                                full_text.push_str(content);
                                let _ = tx
                                    .send(StreamEvent::TextDelta {
                                        text: content.clone(),
                                    })
                                    .await;
                            }
                        }
                        "result" | "done" | "complete" => {
                            if let Some(ref result) = event.result {
                                if full_text.is_empty() {
                                    full_text = result.clone();
                                    let _ = tx
                                        .send(StreamEvent::TextDelta {
                                            text: result.clone(),
                                        })
                                        .await;
                                }
                            }
                            if let Some(usage) = event.usage {
                                final_usage = TokenUsage {
                                    input_tokens: usage.input_tokens,
                                    output_tokens: usage.output_tokens,
                                };
                            }
                        }
                        _ => {
                            // Unknown event type — try content field as fallback
                            if let Some(ref content) = event.content {
                                full_text.push_str(content);
                                let _ = tx
                                    .send(StreamEvent::TextDelta {
                                        text: content.clone(),
                                    })
                                    .await;
                            }
                        }
                    }
                }
                Err(e) => {
                    // Not valid JSON — treat as raw text
                    warn!(line = %line, error = %e, "Non-JSON line from Claude CLI");
                    full_text.push_str(&line);
                    let _ = tx
                        .send(StreamEvent::TextDelta { text: line })
                        .await;
                }
            }
        }

        // Wait for process to finish
        let status = child
            .wait()
            .await
            .map_err(|e| LlmError::Http(format!("Claude CLI wait failed: {e}")))?;

        if !status.success() {
            warn!(code = ?status.code(), "Claude CLI exited with error");
        }

        let _ = tx
            .send(StreamEvent::ContentComplete {
                stop_reason: StopReason::EndTurn,
                usage: final_usage,
            })
            .await;

        Ok(CompletionResponse {
            content: vec![ContentBlock::Text { text: full_text }],
            stop_reason: StopReason::EndTurn,
            tool_calls: Vec::new(),
            usage: final_usage,
        })
    }
}

/// Check if the Claude Code CLI is available.
pub fn claude_code_available() -> bool {
    ClaudeCodeDriver::detect().is_some()
        || claude_credentials_exist()
}

/// Check if Claude credentials file exists (~/.claude/.credentials.json).
fn claude_credentials_exist() -> bool {
    if let Some(home) = home_dir() {
        home.join(".claude").join(".credentials.json").exists()
    } else {
        false
    }
}

/// Cross-platform home directory.
fn home_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE").ok().map(std::path::PathBuf::from)
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
    fn test_build_prompt_simple() {
        use openfang_types::message::{Message, MessageContent};

        let request = CompletionRequest {
            model: "claude-code/sonnet".to_string(),
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

        let prompt = ClaudeCodeDriver::build_prompt(&request);
        assert!(prompt.contains("[System]"));
        assert!(prompt.contains("You are helpful."));
        assert!(prompt.contains("[User]"));
        assert!(prompt.contains("Hello"));
    }

    #[test]
    fn test_model_flag_mapping() {
        assert_eq!(
            ClaudeCodeDriver::model_flag("claude-code/opus"),
            Some("opus".to_string())
        );
        assert_eq!(
            ClaudeCodeDriver::model_flag("claude-code/sonnet"),
            Some("sonnet".to_string())
        );
        assert_eq!(
            ClaudeCodeDriver::model_flag("claude-code/haiku"),
            Some("haiku".to_string())
        );
        assert_eq!(
            ClaudeCodeDriver::model_flag("custom-model"),
            Some("custom-model".to_string())
        );
    }

    #[test]
    fn test_new_defaults_to_claude() {
        let driver = ClaudeCodeDriver::new(None);
        assert_eq!(driver.cli_path, "claude");
    }

    #[test]
    fn test_new_with_custom_path() {
        let driver = ClaudeCodeDriver::new(Some("/usr/local/bin/claude".to_string()));
        assert_eq!(driver.cli_path, "/usr/local/bin/claude");
    }

    #[test]
    fn test_new_with_empty_path() {
        let driver = ClaudeCodeDriver::new(Some(String::new()));
        assert_eq!(driver.cli_path, "claude");
    }
}
