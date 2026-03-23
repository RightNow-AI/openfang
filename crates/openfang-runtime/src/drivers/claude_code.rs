//! Claude Code CLI backend driver.
//!
//! Spawns the `claude` CLI (Claude Code) as a subprocess in print mode (`-p`),
//! which is non-interactive and handles its own authentication.
//! This allows users with Claude Code installed to use it as an LLM provider
//! without needing a separate API key.
//!
//! Tracks active subprocess PIDs and enforces message timeouts to prevent
//! hung CLI processes from blocking agents indefinitely.

use crate::llm_driver::{CompletionRequest, CompletionResponse, LlmDriver, LlmError, StreamEvent};
use async_trait::async_trait;
use dashmap::DashMap;
use openfang_types::message::{ContentBlock, Role, StopReason, TokenUsage};
use serde::Deserialize;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt};
use tracing::{debug, info, warn};

/// Environment variable names (and suffixes) to strip from the subprocess
/// to prevent leaking API keys from other providers. We keep the full env
/// intact (so Node.js, NVM, SSL, proxies, etc. all work) and only remove
/// secrets that belong to other LLM providers.
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
/// unless it starts with `CLAUDE_`.
const SENSITIVE_SUFFIXES: &[&str] = &["_SECRET", "_TOKEN", "_PASSWORD"];

/// Default subprocess timeout in seconds (5 minutes).
const DEFAULT_MESSAGE_TIMEOUT_SECS: u64 = 300;

/// LLM driver that delegates to the Claude Code CLI.
pub struct ClaudeCodeDriver {
    cli_path: String,
    skip_permissions: bool,
    /// Active subprocess PIDs keyed by a caller-provided label (e.g. agent name).
    /// Allows external code to check if a subprocess is running and kill it.
    active_pids: Arc<DashMap<String, u32>>,
    /// Message timeout in seconds. CLI subprocesses that exceed this are killed.
    message_timeout_secs: u64,
}

impl ClaudeCodeDriver {
    /// Create a new Claude Code driver.
    ///
    /// `cli_path` overrides the CLI binary path; defaults to `"claude"` on PATH.
    /// `skip_permissions` adds `--dangerously-skip-permissions` to the spawned
    /// command so that the CLI runs non-interactively (required for daemon mode).
    pub fn new(cli_path: Option<String>, skip_permissions: bool) -> Self {
        if skip_permissions {
            warn!(
                "Claude Code driver: --dangerously-skip-permissions enabled. \
                 The CLI will not prompt for tool approvals. \
                 OpenFang's own capability/RBAC system enforces access control."
            );
        }

        Self {
            cli_path: cli_path
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "claude".to_string()),
            skip_permissions,
            active_pids: Arc::new(DashMap::new()),
            message_timeout_secs: DEFAULT_MESSAGE_TIMEOUT_SECS,
        }
    }

    /// Create a new Claude Code driver with a custom timeout.
    pub fn with_timeout(
        cli_path: Option<String>,
        skip_permissions: bool,
        timeout_secs: u64,
    ) -> Self {
        let mut driver = Self::new(cli_path, skip_permissions);
        driver.message_timeout_secs = timeout_secs;
        driver
    }

    /// Get a snapshot of active subprocess PIDs.
    /// Returns a vec of (label, pid) pairs.
    pub fn active_pids(&self) -> Vec<(String, u32)> {
        self.active_pids
            .iter()
            .map(|entry| (entry.key().clone(), *entry.value()))
            .collect()
    }

    /// Get the shared PID map for external monitoring.
    pub fn pid_map(&self) -> Arc<DashMap<String, u32>> {
        Arc::clone(&self.active_pids)
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
        let stripped = model.strip_prefix("claude-code/").unwrap_or(model);
        match stripped {
            "opus" => Some("opus".to_string()),
            "sonnet" => Some("sonnet".to_string()),
            "haiku" => Some("haiku".to_string()),
            _ => Some(stripped.to_string()),
        }
    }

    /// Apply security env filtering to a command.
    ///
    /// Instead of `env_clear()` (which breaks Node.js, NVM, SSL, proxies),
    /// we keep the full environment and only remove known sensitive API keys
    /// from other LLM providers.
    fn apply_env_filter(cmd: &mut tokio::process::Command) {
        for key in SENSITIVE_ENV_EXACT {
            cmd.env_remove(key);
        }
        // Remove any env var with a sensitive suffix, unless it's CLAUDE_*
        for (key, _) in std::env::vars() {
            if key.starts_with("CLAUDE_") {
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
}

/// JSON output from `claude -p --output-format json`.
///
/// The CLI may return the response text in different fields depending on
/// version: `result`, `content`, or `text`. We try all three.
/// All fields use `#[serde(default)]` so deserialization never fails on
/// missing keys — older and newer CLI versions differ in which fields are emitted.
#[derive(Debug, Deserialize)]
struct ClaudeJsonOutput {
    // Fix: `result` now has #[serde(default)] so deserialization succeeds
    // even when the CLI emits the response in `content` or `text` instead.
    #[serde(default)]
    result: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    text: Option<String>,
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

/// A single content block inside an `assistant` stream-json event.
/// The CLI emits `{"type":"text","text":"..."}` blocks inside `message.content`.
#[derive(Debug, Deserialize, Default)]
struct ClaudeMessageBlock {
    #[serde(default, rename = "type")]
    block_type: String,
    #[serde(default)]
    text: String,
}

/// Nested `message` object carried by `type=assistant` stream-json events.
#[derive(Debug, Deserialize, Default)]
struct ClaudeAssistantMessage {
    #[serde(default)]
    content: Vec<ClaudeMessageBlock>,
}

/// Stream JSON event from `claude -p --output-format stream-json --verbose`.
///
/// Newer CLI versions (≥2.x) carry the response text inside the nested
/// `message.content[].text` of `type=assistant` events rather than a
/// flat `content` string.  Both layouts are handled here so that real-time
/// token streaming works across CLI versions.
#[derive(Debug, Deserialize)]
struct ClaudeStreamEvent {
    #[serde(default)]
    r#type: String,
    /// Flat content string — used by older CLI versions and some event types.
    #[serde(default)]
    content: Option<String>,
    /// Final result text carried by `type=result` events.
    #[serde(default)]
    result: Option<String>,
    /// Nested assistant message — used by newer CLI `type=assistant` events.
    #[serde(default)]
    message: Option<ClaudeAssistantMessage>,
    #[serde(default)]
    usage: Option<ClaudeUsage>,
}

#[async_trait]
impl LlmDriver for ClaudeCodeDriver {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let prompt = Self::build_prompt(&request);
        let model_flag = Self::model_flag(&request.model);

        let mut cmd = tokio::process::Command::new(&self.cli_path);
        cmd.arg("-p")
            .arg(&prompt)
            .arg("--output-format")
            .arg("json");

        if self.skip_permissions {
            cmd.arg("--dangerously-skip-permissions");
        }

        if let Some(ref model) = model_flag {
            cmd.arg("--model").arg(model);
        }

        Self::apply_env_filter(&mut cmd);

        // Inject HOME so the CLI can find its credentials (~/.claude/) when
        // OpenFang runs as a service without a login shell.
        if let Some(home) = home_dir() {
            cmd.env("HOME", &home);
        }
        // Detach stdin so the CLI does not block waiting for interactive input.
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        debug!(cli = %self.cli_path, skip_permissions = self.skip_permissions, "Spawning Claude Code CLI");

        // Spawn child process instead of cmd.output() so we can track PID and timeout
        let mut child = cmd.spawn().map_err(|e| {
            LlmError::Http(format!(
                "Claude Code CLI not found or failed to start ({}). \
                 Install: npm install -g @anthropic-ai/claude-code && claude auth",
                e
            ))
        })?;

        // Track the PID using the model name as label (best identifier available)
        let pid_label = request.model.clone();
        if let Some(pid) = child.id() {
            self.active_pids.insert(pid_label.clone(), pid);
            debug!(pid = pid, model = %pid_label, "Claude Code CLI subprocess started");
        }

        // Drain stdout and stderr concurrently while waiting for the process.
        // Sequential drain (wait → read) deadlocks when the subprocess writes
        // more than the OS pipe buffer (~64 KB): the child blocks on write,
        // child.wait() never returns, the timeout fires, and output is lost.
        let child_stdout = child.stdout.take();
        let child_stderr = child.stderr.take();

        let stdout_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            if let Some(mut out) = child_stdout {
                let _ = out.read_to_end(&mut buf).await;
            }
            buf
        });
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            if let Some(mut err) = child_stderr {
                let _ = err.read_to_end(&mut buf).await;
            }
            buf
        });

        // Wait with timeout
        let timeout_duration = std::time::Duration::from_secs(self.message_timeout_secs);
        let wait_result = tokio::time::timeout(timeout_duration, child.wait()).await;

        // Collect pipe output — tasks complete once the process closes its end
        let stdout_bytes = stdout_task.await.unwrap_or_default();
        let stderr_bytes = stderr_task.await.unwrap_or_default();

        // Clear PID tracking regardless of outcome
        self.active_pids.remove(&pid_label);

        let status = match wait_result {
            Ok(Ok(status)) => status,
            Ok(Err(e)) => {
                warn!(error = %e, model = %pid_label, "Claude Code CLI subprocess failed");
                return Err(LlmError::Http(format!(
                    "Claude Code CLI subprocess failed: {e}"
                )));
            }
            Err(_elapsed) => {
                // Timeout — kill the process
                warn!(
                    timeout_secs = self.message_timeout_secs,
                    model = %pid_label,
                    "Claude Code CLI subprocess timed out, killing process"
                );
                let _ = child.kill().await;
                return Err(LlmError::Http(format!(
                    "Claude Code CLI subprocess timed out after {}s — process killed",
                    self.message_timeout_secs
                )));
            }
        };

        if !status.success() {
            let stderr = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
            let stdout_str = String::from_utf8_lossy(&stdout_bytes).trim().to_string();
            let detail = if !stderr.is_empty() {
                &stderr
            } else {
                &stdout_str
            };
            let code = status.code().unwrap_or(1);

            warn!(
                exit_code = code,
                model = %pid_label,
                stderr = %detail,
                "Claude Code CLI exited with error"
            );

            // Provide actionable error messages
            let message = if detail.contains("not authenticated")
                || detail.contains("auth")
                || detail.contains("login")
                || detail.contains("credentials")
            {
                format!("Claude Code CLI is not authenticated. Run: claude auth\nDetail: {detail}")
            } else if detail.contains("permission")
                || detail.contains("--dangerously-skip-permissions")
            {
                format!(
                    "Claude Code CLI requires permissions acceptance. \
                     Run: claude --dangerously-skip-permissions (once to accept)\nDetail: {detail}"
                )
            } else {
                format!("Claude Code CLI exited with code {code}: {detail}")
            };

            return Err(LlmError::Api {
                status: code as u16,
                message,
            });
        }

        info!(model = %pid_label, "Claude Code CLI subprocess completed successfully");

        let stdout = String::from_utf8_lossy(&stdout_bytes);

        // Try JSON parse first
        if let Ok(parsed) = serde_json::from_str::<ClaudeJsonOutput>(&stdout) {
            let text = parsed
                .result
                .or(parsed.content)
                .or(parsed.text)
                .unwrap_or_default();
            let usage = parsed.usage.unwrap_or_default();
            return Ok(CompletionResponse {
                content: vec![ContentBlock::Text {
                    text: text.clone(),
                    provider_metadata: None,
                }],
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
        let prompt = Self::build_prompt(&request);
        let model_flag = Self::model_flag(&request.model);

        let mut cmd = tokio::process::Command::new(&self.cli_path);
        cmd.arg("-p")
            .arg(&prompt)
            .arg("--output-format")
            .arg("stream-json")
            .arg("--verbose");

        if self.skip_permissions {
            cmd.arg("--dangerously-skip-permissions");
        }

        if let Some(ref model) = model_flag {
            cmd.arg("--model").arg(model);
        }

        Self::apply_env_filter(&mut cmd);

        // Same HOME and stdin hygiene as the non-streaming path.
        if let Some(home) = home_dir() {
            cmd.env("HOME", &home);
        }
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        debug!(cli = %self.cli_path, "Spawning Claude Code CLI (streaming)");

        let mut child = cmd.spawn().map_err(|e| {
            LlmError::Http(format!(
                "Claude Code CLI not found or failed to start ({}). \
                 Install: npm install -g @anthropic-ai/claude-code && claude auth",
                e
            ))
        })?;

        // Track PID
        let pid_label = format!("{}-stream", request.model);
        if let Some(pid) = child.id() {
            self.active_pids.insert(pid_label.clone(), pid);
            debug!(pid = pid, model = %pid_label, "Claude Code CLI streaming subprocess started");
        }

        let stdout = child.stdout.take().ok_or_else(|| {
            self.active_pids.remove(&pid_label);
            LlmError::Http("No stdout from claude CLI".to_string())
        })?;

        let reader = tokio::io::BufReader::new(stdout);
        let mut lines = reader.lines();

        let mut full_text = String::new();
        let mut final_usage = TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
        };

        let timeout_duration = std::time::Duration::from_secs(self.message_timeout_secs);
        let stream_result = tokio::time::timeout(timeout_duration, async {
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }

                match serde_json::from_str::<ClaudeStreamEvent>(&line) {
                    Ok(event) => {
                        match event.r#type.as_str() {
                            "content" | "text" | "assistant" | "content_block_delta" => {
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
                        let _ = tx.send(StreamEvent::TextDelta { text: line }).await;
                    }
                }
            }
        })
        .await;

        // Clear PID tracking
        self.active_pids.remove(&pid_label);

        if stream_result.is_err() {
            warn!(
                timeout_secs = self.message_timeout_secs,
                model = %pid_label,
                "Claude Code CLI streaming subprocess timed out, killing process"
            );
            let _ = child.kill().await;
            return Err(LlmError::Http(format!(
                "Claude Code CLI streaming subprocess timed out after {}s — process killed",
                self.message_timeout_secs
            )));
        }

        // Wait for process to finish
        let status = child
            .wait()
            .await
            .map_err(|e| LlmError::Http(format!("Claude CLI wait failed: {e}")))?;

        if !status.success() {
            let code = status.code().unwrap_or(1);
            // Read stderr for diagnostic info
            let stderr_text = if let Some(mut err) = child.stderr.take() {
                let mut buf = Vec::new();
                let _ = err.read_to_end(&mut buf).await;
                String::from_utf8_lossy(&buf).trim().to_string()
            } else {
                String::new()
            };
            warn!(
                exit_code = code,
                model = %pid_label,
                stderr = %stderr_text,
                "Claude Code CLI streaming subprocess exited with error"
            );
            return Err(LlmError::Api {
                status: code as u16,
                message: format!(
                    "Claude Code CLI streaming exited with code {code}: {}",
                    if stderr_text.is_empty() {
                        "no stderr"
                    } else {
                        &stderr_text
                    }
                ),
            });
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

/// Check if the Claude Code CLI is available.
pub fn claude_code_available() -> bool {
    ClaudeCodeDriver::detect().is_some() || claude_credentials_exist()
}

/// Check if Claude credentials file exists.
///
/// Different Claude CLI versions store credentials at different paths:
/// - `~/.claude/.credentials.json` (older versions)
/// - `~/.claude/credentials.json` (newer versions)
fn claude_credentials_exist() -> bool {
    if let Some(home) = home_dir() {
        let claude_dir = home.join(".claude");
        claude_dir.join(".credentials.json").exists()
            || claude_dir.join("credentials.json").exists()
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
        let driver = ClaudeCodeDriver::new(None, true);
        assert_eq!(driver.cli_path, "claude");
        assert_eq!(driver.message_timeout_secs, DEFAULT_MESSAGE_TIMEOUT_SECS);
        assert!(driver.active_pids().is_empty());
    }

    #[test]
    fn test_new_with_custom_path() {
        let driver = ClaudeCodeDriver::new(Some("/usr/local/bin/claude".to_string()), true);
        assert_eq!(driver.cli_path, "/usr/local/bin/claude");
    }

    #[test]
    fn test_new_with_empty_path() {
        let driver = ClaudeCodeDriver::new(Some(String::new()), true);
        assert_eq!(driver.cli_path, "claude");
    }

    #[test]
    fn test_with_timeout() {
        let driver = ClaudeCodeDriver::with_timeout(None, true, 600);
        assert_eq!(driver.message_timeout_secs, 600);
        assert_eq!(driver.cli_path, "claude");
    }

    #[test]
    fn test_pid_map_shared() {
        let driver = ClaudeCodeDriver::new(None, true);
        let map = driver.pid_map();
        map.insert("test-agent".to_string(), 12345);
        assert_eq!(driver.active_pids().len(), 1);
        assert_eq!(driver.active_pids()[0], ("test-agent".to_string(), 12345));
    }

    #[test]
    fn test_sensitive_env_list_coverage() {
        // Ensure all major provider keys are in the strip list
        assert!(SENSITIVE_ENV_EXACT.contains(&"OPENAI_API_KEY"));
        assert!(SENSITIVE_ENV_EXACT.contains(&"ANTHROPIC_API_KEY"));
        assert!(SENSITIVE_ENV_EXACT.contains(&"GEMINI_API_KEY"));
        assert!(SENSITIVE_ENV_EXACT.contains(&"GROQ_API_KEY"));
        assert!(SENSITIVE_ENV_EXACT.contains(&"DEEPSEEK_API_KEY"));
    }
}
