//! OpenAI Codex app-server backend driver.
//!
//! Spawns `codex app-server --listen stdio://` and talks to the documented
//! JSON-line app-server protocol. Authentication is owned by the Codex CLI
//! (`codex login`), so OpenFang does not require or forward an OpenAI API key
//! for this provider. Usage is still reported, but catalog and metering rates
//! are zero because ChatGPT subscription billing is flat-rate rather than
//! per-token API billing.

use crate::llm_driver::{CompletionRequest, CompletionResponse, LlmDriver, LlmError, StreamEvent};
use async_trait::async_trait;
use openfang_types::message::{ContentBlock, Message, MessageContent, Role, StopReason, TokenUsage};
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tracing::{debug, warn};

const DEFAULT_MESSAGE_TIMEOUT_SECS: u64 = 300;

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

const SENSITIVE_SUFFIXES: &[&str] = &["_SECRET", "_TOKEN", "_PASSWORD"];

/// LLM driver backed by the local Codex app-server process.
pub struct CodexAppServerDriver {
    cli_path: String,
    state: tokio::sync::Mutex<DriverState>,
    message_timeout_secs: u64,
}

#[derive(Default)]
struct DriverState {
    session: Option<AppServerSession>,
    threads: HashMap<String, ThreadState>,
}

struct ThreadState {
    thread_id: String,
    model: String,
    sent_message_count: usize,
}

struct AppServerSession {
    child: Child,
    stdin: ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
    next_id: u64,
}

struct TurnOutcome {
    text: String,
    usage: TokenUsage,
}

impl CodexAppServerDriver {
    /// Create a new Codex app-server driver.
    ///
    /// `cli_path` overrides the CLI binary path; defaults to `"codex"` on PATH.
    pub fn new(cli_path: Option<String>) -> Self {
        Self {
            cli_path: cli_path
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "codex".to_string()),
            state: tokio::sync::Mutex::new(DriverState::default()),
            message_timeout_secs: DEFAULT_MESSAGE_TIMEOUT_SECS,
        }
    }

    /// Create a new Codex app-server driver with a custom turn timeout.
    pub fn with_timeout(cli_path: Option<String>, timeout_secs: u64) -> Self {
        let mut driver = Self::new(cli_path);
        driver.message_timeout_secs = timeout_secs;
        driver
    }

    /// Detect if the Codex CLI app-server command is available on PATH.
    pub fn detect() -> Option<String> {
        let output = std::process::Command::new("codex")
            .arg("app-server")
            .arg("--help")
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

    fn model_flag(model: &str) -> Option<String> {
        let stripped = model
            .strip_prefix("codex_app_server/")
            .or_else(|| model.strip_prefix("codex-app-server/"))
            .unwrap_or(model);

        match stripped {
            "" | "default" => None,
            other => Some(other.to_string()),
        }
    }

    fn conversation_key(request: &CompletionRequest) -> String {
        request
            .messages
            .iter()
            .find(|m| m.role != Role::System)
            .map(|m| m.msg_id.clone())
            .unwrap_or_else(|| "default".to_string())
    }

    fn messages_for_turn<'a>(
        request: &'a CompletionRequest,
        thread_state: Option<&ThreadState>,
    ) -> Vec<&'a Message> {
        if let Some(thread) = thread_state {
            if request.messages.len() > thread.sent_message_count {
                return request.messages[thread.sent_message_count..].iter().collect();
            }
            if let Some(last_user) = request.messages.iter().rev().find(|m| m.role == Role::User) {
                return vec![last_user];
            }
        }
        request.messages.iter().collect()
    }

    fn build_prompt(messages: &[&Message], system: Option<&str>) -> String {
        let mut parts = Vec::new();

        if let Some(sys) = system {
            if !sys.trim().is_empty() {
                parts.push(format!("[System]\n{sys}"));
            }
        }

        for msg in messages {
            let role_label = match msg.role {
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::System => "System",
            };
            let rendered = Self::render_content(&msg.content);
            if !rendered.is_empty() {
                parts.push(format!("[{role_label}]\n{rendered}"));
            }
        }

        parts.join("\n\n")
    }

    fn render_content(content: &MessageContent) -> String {
        match content {
            MessageContent::Text(s) => s.clone(),
            MessageContent::Blocks(blocks) => blocks
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text { text, .. } => {
                        if text.is_empty() {
                            None
                        } else {
                            Some(text.clone())
                        }
                    }
                    ContentBlock::Image { media_type, data } => {
                        let approx_kb = (data.len().saturating_mul(3) / 4) / 1024;
                        Some(format!(
                            "[attachment: {media_type} image, ~{approx_kb} KB - not viewable on this provider]"
                        ))
                    }
                    ContentBlock::ToolResult {
                        tool_name,
                        content,
                        is_error,
                        ..
                    } => Some(format!(
                        "[tool result: {tool_name}{}]\n{content}",
                        if *is_error { " error" } else { "" }
                    )),
                    ContentBlock::ToolUse { name, input, .. } => {
                        Some(format!("[tool use: {name}]\n{input}"))
                    }
                    ContentBlock::Thinking { .. }
                    | ContentBlock::RedactedThinking { .. }
                    | ContentBlock::Unknown => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    async fn ensure_session<'a>(
        &'a self,
        state: &'a mut DriverState,
    ) -> Result<&'a mut AppServerSession, LlmError> {
        let needs_spawn = match state.session.as_mut() {
            Some(session) => match session.child.try_wait() {
                Ok(Some(status)) => {
                    warn!(%status, "Codex app-server exited; respawning on next request");
                    true
                }
                Ok(None) => false,
                Err(e) => {
                    warn!(error = %e, "Failed to inspect Codex app-server child; respawning");
                    true
                }
            },
            None => true,
        };

        if needs_spawn {
            state.session = None;
            let mut session = self.spawn_session().await?;
            self.initialize(&mut session).await?;
            state.session = Some(session);
        }

        state.session.as_mut().ok_or_else(|| {
            LlmError::Http("Codex app-server session was not available after spawn".to_string())
        })
    }

    async fn spawn_session(&self) -> Result<AppServerSession, LlmError> {
        let mut cmd = tokio::process::Command::new(&self.cli_path);
        cmd.arg("app-server").arg("--listen").arg("stdio://");
        Self::apply_env_filter(&mut cmd);
        if let Some(home) = home_dir() {
            cmd.env("HOME", home);
        }
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        debug!(cli = %self.cli_path, "Spawning Codex app-server");
        let mut child = cmd.spawn().map_err(|e| {
            LlmError::Http(format!(
                "Codex CLI app-server not found or failed to start ({e}). \
                 Install Codex CLI, verify `codex --version`, then run `codex login`."
            ))
        })?;

        if let Some(mut stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(&mut stderr);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => break,
                        Ok(_) => {
                            let trimmed = line.trim();
                            if !trimmed.is_empty() {
                                debug!(stderr = %trimmed, "Codex app-server stderr");
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| LlmError::Http("No stdin for Codex app-server".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| LlmError::Http("No stdout from Codex app-server".to_string()))?;

        Ok(AppServerSession {
            child,
            stdin,
            lines: BufReader::new(stdout).lines(),
            next_id: 1,
        })
    }

    async fn initialize(&self, session: &mut AppServerSession) -> Result<(), LlmError> {
        let params = json!({
            "clientInfo": {
                "name": "openfang",
                "title": "OpenFang",
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "experimentalApi": true,
                "requestAttestation": false,
                "optOutNotificationMethods": []
            }
        });
        self.request(session, "initialize", params).await.map(|_| ())
    }

    async fn start_thread(
        &self,
        session: &mut AppServerSession,
        request: &CompletionRequest,
    ) -> Result<String, LlmError> {
        let cwd = std::env::current_dir()
            .ok()
            .and_then(|p| p.to_str().map(|s| s.to_string()));
        let model = Self::model_flag(&request.model);
        let mut params = json!({
            "model": model,
            "modelProvider": Value::Null,
            "cwd": cwd,
            "approvalPolicy": "never",
            "sandbox": "read-only",
            "serviceName": "openfang",
            "baseInstructions": request.system,
            "ephemeral": true,
            "experimentalRawEvents": false,
            "persistExtendedHistory": false
        });

        if request.system.is_none() {
            params["baseInstructions"] = Value::Null;
        }

        let response = self.request(session, "thread/start", params).await?;
        response
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                LlmError::Parse(format!(
                    "Codex app-server thread/start response missing thread id: {response}"
                ))
            })
    }

    async fn start_turn(
        &self,
        session: &mut AppServerSession,
        thread_id: &str,
        request: &CompletionRequest,
        prompt: String,
    ) -> Result<String, LlmError> {
        let params = json!({
            "threadId": thread_id,
            "input": [{
                "type": "text",
                "text": prompt,
                "text_elements": []
            }],
            "model": Self::model_flag(&request.model),
            "cwd": std::env::current_dir().ok().and_then(|p| p.to_str().map(|s| s.to_string())),
            "approvalPolicy": "never"
        });
        let response = self.request(session, "turn/start", params).await?;
        response
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                LlmError::Parse(format!(
                    "Codex app-server turn/start response missing turn id: {response}"
                ))
            })
    }

    async fn run_turn(
        &self,
        request: CompletionRequest,
        tx: Option<tokio::sync::mpsc::Sender<StreamEvent>>,
    ) -> Result<CompletionResponse, LlmError> {
        // Driver declares supports_tools: false in the catalog. If a caller
        // still passes tools, surface that loudly rather than silently
        // dropping them and producing text-only output the caller didn't ask
        // for.
        if !request.tools.is_empty() {
            tracing::warn!(
                tools_dropped = request.tools.len(),
                model = %request.model,
                "codex_app_server driver does not support tools; ignoring \
                 {n} tool(s) in this request",
                n = request.tools.len()
            );
        }
        let timeout = std::time::Duration::from_secs(self.message_timeout_secs);
        let result = tokio::time::timeout(timeout, self.run_turn_inner(request, tx)).await;
        match result {
            Ok(result) => result,
            Err(_) => Err(LlmError::Http(format!(
                "Codex app-server turn timed out after {}s",
                self.message_timeout_secs
            ))),
        }
    }

    async fn run_turn_inner(
        &self,
        request: CompletionRequest,
        tx: Option<tokio::sync::mpsc::Sender<StreamEvent>>,
    ) -> Result<CompletionResponse, LlmError> {
        let key = Self::conversation_key(&request);
        let model_key = request.model.clone();
        let mut state = self.state.lock().await;

        let existing_thread = state
            .threads
            .get(&key)
            .filter(|thread| thread.model == model_key);
        let messages = Self::messages_for_turn(&request, existing_thread);
        let prompt = Self::build_prompt(&messages, request.system.as_deref());

        let thread_id = match existing_thread {
            Some(thread) => thread.thread_id.clone(),
            None => {
                let session = self.ensure_session(&mut state).await?;
                let thread_id = self.start_thread(session, &request).await?;
                state.threads.insert(
                    key.clone(),
                    ThreadState {
                        thread_id: thread_id.clone(),
                        model: model_key.clone(),
                        sent_message_count: 0,
                    },
                );
                thread_id
            }
        };

        let outcome_result = {
            let session = self.ensure_session(&mut state).await?;
            let turn_id = self
                .start_turn(session, &thread_id, &request, prompt)
                .await?;
            self.read_turn(session, &thread_id, &turn_id, tx.clone())
                .await
        };
        if outcome_result.is_err() {
            state.session = None;
        }
        let outcome = outcome_result?;

        if let Some(thread) = state.threads.get_mut(&key) {
            thread.sent_message_count = request.messages.len().saturating_add(1);
        }

        if let Some(tx) = tx {
            let _ = tx
                .send(StreamEvent::ContentComplete {
                    stop_reason: StopReason::EndTurn,
                    usage: outcome.usage,
                })
                .await;
        }

        Ok(CompletionResponse {
            content: vec![ContentBlock::Text {
                text: outcome.text,
                provider_metadata: None,
            }],
            stop_reason: StopReason::EndTurn,
            tool_calls: Vec::new(),
            usage: outcome.usage,
        })
    }

    async fn request(
        &self,
        session: &mut AppServerSession,
        method: &str,
        params: Value,
    ) -> Result<Value, LlmError> {
        let id = session.next_id;
        session.next_id += 1;
        let frame = json!({
            "id": id,
            "method": method,
            "params": params
        });
        Self::write_frame(session, &frame).await?;

        loop {
            let msg = Self::read_frame(session).await?;
            if let Some(request_id) = msg.get("id").and_then(Value::as_u64) {
                if request_id == id {
                    if let Some(error) = msg.get("error") {
                        return Err(Self::jsonrpc_error(error));
                    }
                    return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
                }
                if msg.get("method").and_then(Value::as_str).is_some() {
                    Self::reject_server_request(session, &msg).await?;
                }
            }
        }
    }

    async fn read_turn(
        &self,
        session: &mut AppServerSession,
        thread_id: &str,
        turn_id: &str,
        tx: Option<tokio::sync::mpsc::Sender<StreamEvent>>,
    ) -> Result<TurnOutcome, LlmError> {
        let mut text = String::new();
        let mut usage = TokenUsage::default();

        loop {
            let msg = Self::read_frame(session).await?;
            if msg.get("id").is_some() && msg.get("method").is_some() {
                Self::reject_server_request(session, &msg).await?;
                continue;
            }

            let method = msg.get("method").and_then(Value::as_str).unwrap_or_default();
            let params = msg.get("params").unwrap_or(&Value::Null);
            match method {
                "item/agentMessage/delta"
                    if params.get("threadId").and_then(Value::as_str) == Some(thread_id)
                        && params.get("turnId").and_then(Value::as_str) == Some(turn_id) =>
                {
                    if let Some(delta) = params.get("delta").and_then(Value::as_str) {
                        text.push_str(delta);
                        if let Some(ref tx) = tx {
                            let _ = tx
                                .send(StreamEvent::TextDelta {
                                    text: delta.to_string(),
                                })
                                .await;
                        }
                    }
                }
                "thread/tokenUsage/updated"
                    if params.get("threadId").and_then(Value::as_str) == Some(thread_id)
                        && params.get("turnId").and_then(Value::as_str) == Some(turn_id) =>
                {
                    usage = Self::parse_usage(params);
                }
                "turn/completed"
                    if params.get("threadId").and_then(Value::as_str) == Some(thread_id) =>
                {
                    if text.is_empty() {
                        text = Self::extract_turn_text(params);
                    }
                    return Ok(TurnOutcome { text, usage });
                }
                "error"
                    if params.get("threadId").and_then(Value::as_str) == Some(thread_id)
                        && params.get("turnId").and_then(Value::as_str) == Some(turn_id) =>
                {
                    return Err(Self::turn_error(params));
                }
                "account/rateLimits/updated" if Self::rate_limit_reached(params) => {
                    return Err(LlmError::RateLimited {
                        retry_after_ms: 60_000,
                    });
                }
                _ => {}
            }
        }
    }

    async fn write_frame(session: &mut AppServerSession, frame: &Value) -> Result<(), LlmError> {
        let encoded =
            serde_json::to_string(frame).map_err(|e| LlmError::Parse(e.to_string()))?;
        session
            .stdin
            .write_all(encoded.as_bytes())
            .await
            .map_err(|e| LlmError::Http(format!("Failed to write to Codex app-server: {e}")))?;
        session
            .stdin
            .write_all(b"\n")
            .await
            .map_err(|e| LlmError::Http(format!("Failed to write to Codex app-server: {e}")))?;
        session
            .stdin
            .flush()
            .await
            .map_err(|e| LlmError::Http(format!("Failed to flush Codex app-server stdin: {e}")))
    }

    async fn read_frame(session: &mut AppServerSession) -> Result<Value, LlmError> {
        loop {
            match session.lines.next_line().await {
                Ok(Some(line)) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    return serde_json::from_str(trimmed).map_err(|e| {
                        LlmError::Parse(format!(
                            "Failed to parse Codex app-server JSON frame: {e}: {trimmed}"
                        ))
                    });
                }
                Ok(None) => {
                    return Err(LlmError::Http(
                        "Codex app-server closed stdout unexpectedly".to_string(),
                    ));
                }
                Err(e) => {
                    return Err(LlmError::Http(format!(
                        "Failed to read from Codex app-server: {e}"
                    )));
                }
            }
        }
    }

    async fn reject_server_request(
        session: &mut AppServerSession,
        msg: &Value,
    ) -> Result<(), LlmError> {
        let Some(id) = msg.get("id").cloned() else {
            return Ok(());
        };
        let method = msg
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let response = match method {
            "item/commandExecution/requestApproval" => {
                json!({"id": id, "result": {"decision": "decline"}})
            }
            "item/fileChange/requestApproval" => {
                json!({"id": id, "result": {"decision": "decline"}})
            }
            "applyPatchApproval" | "execCommandApproval" => {
                json!({"id": id, "result": {"decision": "denied"}})
            }
            "item/tool/requestUserInput" => json!({"id": id, "result": {"answers": {}}}),
            "item/tool/call" => json!({
                "id": id,
                "result": {
                    "contentItems": [],
                    "success": false
                }
            }),
            _ => json!({
                "id": id,
                "error": {
                    "code": -32601,
                    "message": format!("OpenFang Codex app-server driver does not support server request `{method}`")
                }
            }),
        };
        Self::write_frame(session, &response).await
    }

    fn parse_usage(params: &Value) -> TokenUsage {
        let last = params.pointer("/tokenUsage/last");
        let total = params.pointer("/tokenUsage/total");
        let source = last.or(total).unwrap_or(&Value::Null);
        TokenUsage {
            input_tokens: source
                .get("inputTokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            output_tokens: source
                .get("outputTokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        }
    }

    fn extract_turn_text(params: &Value) -> String {
        params
            .pointer("/turn/items")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter(|item| item.get("type").and_then(Value::as_str) == Some("agentMessage"))
                    .filter_map(|item| item.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default()
    }

    fn jsonrpc_error(error: &Value) -> LlmError {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Codex app-server request failed")
            .to_string();
        Self::map_error(&message, error.get("code").and_then(Value::as_i64))
    }

    fn turn_error(params: &Value) -> LlmError {
        let error = params.get("error").unwrap_or(params);
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Codex app-server turn failed")
            .to_string();
        if error.pointer("/codexErrorInfo").and_then(Value::as_str) == Some("unauthorized") {
            return LlmError::AuthenticationFailed(format!(
                "Codex CLI is not authenticated. Run `codex login`. Detail: {message}"
            ));
        }
        if error.pointer("/codexErrorInfo").and_then(Value::as_str) == Some("usageLimitExceeded") {
            return LlmError::RateLimited {
                retry_after_ms: 60_000,
            };
        }
        Self::map_error(&message, None)
    }

    fn map_error(message: &str, code: Option<i64>) -> LlmError {
        let lower = message.to_lowercase();
        if lower.contains("unauthorized")
            || lower.contains("not authenticated")
            || lower.contains("login")
            || lower.contains("auth")
        {
            return LlmError::AuthenticationFailed(format!(
                "Codex CLI is not authenticated. Run `codex login`. Detail: {message}"
            ));
        }
        if lower.contains("rate limit") || lower.contains("usage limit") {
            return LlmError::RateLimited {
                retry_after_ms: 60_000,
            };
        }
        LlmError::Api {
            status: code.unwrap_or(0).max(0) as u16,
            message: message.to_string(),
        }
    }

    fn rate_limit_reached(params: &Value) -> bool {
        params
            .pointer("/rateLimits/rateLimitReachedType")
            .and_then(Value::as_str)
            .is_some()
    }

    fn apply_env_filter(cmd: &mut tokio::process::Command) {
        for key in SENSITIVE_ENV_EXACT {
            cmd.env_remove(key);
        }
        for (key, _) in std::env::vars() {
            if key.starts_with("CODEX_") {
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

#[async_trait]
impl LlmDriver for CodexAppServerDriver {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.run_turn(request, None).await
    }

    async fn stream(
        &self,
        request: CompletionRequest,
        tx: tokio::sync::mpsc::Sender<StreamEvent>,
    ) -> Result<CompletionResponse, LlmError> {
        self.run_turn(request, Some(tx)).await
    }
}

/// Detect if Codex app-server is available.
pub fn codex_app_server_available() -> bool {
    CodexAppServerDriver::detect().is_some()
}

fn home_dir() -> Option<String> {
    std::env::var("HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("USERPROFILE").ok().filter(|s| !s.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_flag_strips_provider_prefix() {
        assert_eq!(
            CodexAppServerDriver::model_flag("codex_app_server/gpt-5.4"),
            Some("gpt-5.4".to_string())
        );
        assert_eq!(
            CodexAppServerDriver::model_flag("codex-app-server/gpt-5.4"),
            Some("gpt-5.4".to_string())
        );
        assert_eq!(CodexAppServerDriver::model_flag("default"), None);
    }

    #[test]
    fn test_build_prompt_renders_roles() {
        let user = Message::user("hello");
        let assistant = Message::assistant("hi");
        let prompt = CodexAppServerDriver::build_prompt(&[&user, &assistant], Some("sys"));
        assert!(prompt.contains("[System]\nsys"));
        assert!(prompt.contains("[User]\nhello"));
        assert!(prompt.contains("[Assistant]\nhi"));
    }

    #[test]
    fn test_parse_usage_prefers_last_turn() {
        let params = json!({
            "tokenUsage": {
                "total": {"inputTokens": 100, "outputTokens": 50},
                "last": {"inputTokens": 10, "outputTokens": 5}
            }
        });
        let usage = CodexAppServerDriver::parse_usage(&params);
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.output_tokens, 5);
    }
}
