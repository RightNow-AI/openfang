//! ChatGPT subscription-backed driver for Codex backend endpoints.
//!
//! Uses OAuth access tokens (not API keys) against:
//! `https://chatgpt.com/backend-api/codex/responses`
//! and injects `ChatGPT-Account-ID` when available.

use crate::llm_driver::{CompletionRequest, CompletionResponse, LlmDriver, LlmError};
use async_trait::async_trait;
use base64::Engine;
use openfang_types::message::{ContentBlock, MessageContent, Role, StopReason, TokenUsage};
use openfang_types::tool::ToolCall;
use serde_json::Value;
use tokio::sync::Mutex;
use tracing::{debug, warn};

const CHATGPT_REFRESH_URL: &str = "https://auth.openai.com/oauth/token";
const CHATGPT_OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

#[derive(Clone, Default)]
struct AuthState {
    access_token: String,
    refresh_token: Option<String>,
    account_id: Option<String>,
}

pub struct ChatGptDriver {
    client: reqwest::Client,
    base_url: String,
    auth: Mutex<AuthState>,
}

impl ChatGptDriver {
    pub fn new(
        access_token: String,
        refresh_token: Option<String>,
        account_id: Option<String>,
        base_url: String,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
            auth: Mutex::new(AuthState {
                access_token,
                refresh_token,
                account_id,
            }),
        }
    }

    async fn auth_snapshot(&self) -> AuthState {
        self.auth.lock().await.clone()
    }

    async fn try_refresh_token(&self) -> Result<bool, LlmError> {
        let (refresh_token, old_account) = {
            let guard = self.auth.lock().await;
            (guard.refresh_token.clone(), guard.account_id.clone())
        };
        let Some(refresh_token) = refresh_token else {
            return Ok(false);
        };

        let body = serde_json::json!({
            "client_id": CHATGPT_OAUTH_CLIENT_ID,
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
            "scope": "openid profile email"
        });

        let resp = self
            .client
            .post(CHATGPT_REFRESH_URL)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Http(e.to_string()))?;

        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let message = resp.text().await.unwrap_or_default();
            return Err(LlmError::Api { status, message });
        }

        let value: Value = resp
            .json()
            .await
            .map_err(|e| LlmError::Parse(e.to_string()))?;
        let access_token = value
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| LlmError::Parse("Refresh response missing access_token".to_string()))?
            .to_string();
        let refreshed_refresh = value
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let derived_account = value
            .get("id_token")
            .and_then(|v| v.as_str())
            .and_then(extract_chatgpt_account_id_from_jwt)
            .or_else(|| extract_chatgpt_account_id_from_jwt(&access_token));

        {
            let mut guard = self.auth.lock().await;
            guard.access_token = access_token.clone();
            if let Some(ref new_refresh) = refreshed_refresh {
                guard.refresh_token = Some(new_refresh.clone());
            }
            guard.account_id = derived_account.clone().or(old_account);
        }

        // Keep the current process env in sync so newly created drivers use refreshed values.
        std::env::set_var("CHATGPT_ACCESS_TOKEN", &access_token);
        if let Some(ref new_refresh) = refreshed_refresh {
            std::env::set_var("CHATGPT_REFRESH_TOKEN", new_refresh);
        }
        if let Some(account_id) = derived_account {
            std::env::set_var("CHATGPT_ACCOUNT_ID", account_id);
        }

        Ok(true)
    }
}

#[async_trait]
impl LlmDriver for ChatGptDriver {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let mut attempted_refresh = false;

        loop {
            let auth = self.auth_snapshot().await;
            if auth.access_token.is_empty() {
                return Err(LlmError::MissingApiKey(
                    "Set CHATGPT_ACCESS_TOKEN (or run `openfang auth login --oauth`)".to_string(),
                ));
            }

            let body = build_responses_request(&request);
            let url = format!("{}/responses", self.base_url.trim_end_matches('/'));
            debug!(url = %url, "Sending ChatGPT Codex request");

            let mut req = self
                .client
                .post(&url)
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", auth.access_token))
                .json(&body);
            if let Some(account_id) = auth.account_id.as_deref() {
                req = req.header("ChatGPT-Account-ID", account_id);
            }

            let resp = req
                .send()
                .await
                .map_err(|e| LlmError::Http(e.to_string()))?;
            let status = resp.status().as_u16();
            if status == 401 && !attempted_refresh {
                match self.try_refresh_token().await {
                    Ok(true) => {
                        attempted_refresh = true;
                        continue;
                    }
                    Ok(false) => {}
                    Err(e) => {
                        warn!(error = %e, "ChatGPT token refresh failed");
                    }
                }
            }

            if !resp.status().is_success() {
                let message = resp.text().await.unwrap_or_default();
                return Err(LlmError::Api { status, message });
            }

            let value: Value = resp
                .json()
                .await
                .map_err(|e| LlmError::Parse(e.to_string()))?;
            return parse_responses_completion(value);
        }
    }
}

fn build_responses_request(request: &CompletionRequest) -> Value {
    let mut input: Vec<Value> = Vec::new();

    if let Some(system) = request.system.as_deref() {
        let system = system.trim();
        if !system.is_empty() {
            input.push(message_input("system", system));
        }
    }

    for msg in &request.messages {
        match (&msg.role, &msg.content) {
            (Role::System, MessageContent::Text(text)) => {
                if request.system.is_none() {
                    let text = text.trim();
                    if !text.is_empty() {
                        input.push(message_input("system", text));
                    }
                }
            }
            (Role::User, MessageContent::Text(text)) => {
                let text = text.trim();
                if !text.is_empty() {
                    input.push(message_input("user", text));
                }
            }
            (Role::Assistant, MessageContent::Text(text)) => {
                let text = text.trim();
                if !text.is_empty() {
                    input.push(message_input("assistant", text));
                }
            }
            (Role::User, MessageContent::Blocks(blocks)) => {
                let mut user_text = String::new();
                for block in blocks {
                    match block {
                        ContentBlock::Text { text } => user_text.push_str(text),
                        ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            ..
                        } => {
                            input.push(serde_json::json!({
                                "type": "function_call_output",
                                "call_id": tool_use_id,
                                "output": content,
                            }));
                        }
                        _ => {}
                    }
                }
                if !user_text.trim().is_empty() {
                    input.push(message_input("user", user_text.trim()));
                }
            }
            (Role::Assistant, MessageContent::Blocks(blocks)) => {
                let mut assistant_text = String::new();
                for block in blocks {
                    match block {
                        ContentBlock::Text { text } => assistant_text.push_str(text),
                        ContentBlock::ToolUse {
                            id,
                            name,
                            input: args,
                        } => {
                            input.push(serde_json::json!({
                                "type": "function_call",
                                "call_id": id,
                                "name": name,
                                "arguments": serde_json::to_string(args).unwrap_or_else(|_| "{}".to_string()),
                            }));
                        }
                        _ => {}
                    }
                }
                if !assistant_text.trim().is_empty() {
                    input.push(message_input("assistant", assistant_text.trim()));
                }
            }
            _ => {}
        }
    }

    let tools: Vec<Value> = request
        .tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "name": t.name,
                "description": t.description,
                "parameters": openfang_types::tool::normalize_schema_for_provider(&t.input_schema, "openai"),
            })
        })
        .collect();

    let mut body = serde_json::json!({
        "model": request.model,
        "input": input,
        "temperature": request.temperature,
        "max_output_tokens": request.max_tokens,
        "stream": false,
    });
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
        body["tool_choice"] = Value::String("auto".to_string());
    }
    body
}

fn message_input(role: &str, text: &str) -> Value {
    serde_json::json!({
        "type": "message",
        "role": role,
        "content": [
            {
                "type": "input_text",
                "text": text,
            }
        ]
    })
}

fn parse_responses_completion(value: Value) -> Result<CompletionResponse, LlmError> {
    let mut content: Vec<ContentBlock> = Vec::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();

    if let Some(output) = value.get("output").and_then(|v| v.as_array()) {
        for item in output {
            let item_type = item
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            match item_type {
                "message" => {
                    if let Some(parts) = item.get("content").and_then(|v| v.as_array()) {
                        for part in parts {
                            let part_type = part
                                .get("type")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default();
                            if matches!(part_type, "output_text" | "text" | "input_text") {
                                if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                                    if !text.is_empty() {
                                        content.push(ContentBlock::Text {
                                            text: text.to_string(),
                                        });
                                    }
                                } else if let Some(text) = part
                                    .get("text")
                                    .and_then(|v| v.get("value"))
                                    .and_then(|v| v.as_str())
                                {
                                    if !text.is_empty() {
                                        content.push(ContentBlock::Text {
                                            text: text.to_string(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                "function_call" => {
                    let id = item
                        .get("call_id")
                        .or_else(|| item.get("id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("tool_call")
                        .to_string();
                    let name = item
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("tool")
                        .to_string();
                    let arguments = item
                        .get("arguments")
                        .and_then(|v| v.as_str())
                        .unwrap_or("{}");
                    let parsed_args: Value = serde_json::from_str(arguments).unwrap_or_default();
                    content.push(ContentBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: parsed_args.clone(),
                    });
                    tool_calls.push(ToolCall {
                        id,
                        name,
                        input: parsed_args,
                    });
                }
                _ => {}
            }
        }
    }

    // Some responses include a flattened output_text field.
    if content.is_empty() {
        if let Some(text) = value.get("output_text").and_then(|v| v.as_str()) {
            if !text.is_empty() {
                content.push(ContentBlock::Text {
                    text: text.to_string(),
                });
            }
        }
    }

    let stop_reason = if !tool_calls.is_empty() {
        StopReason::ToolUse
    } else {
        match value.get("status").and_then(|v| v.as_str()) {
            Some("incomplete") => StopReason::MaxTokens,
            _ => StopReason::EndTurn,
        }
    };

    let usage = value.get("usage").map(parse_usage).unwrap_or_default();

    Ok(CompletionResponse {
        content,
        stop_reason,
        tool_calls,
        usage,
    })
}

fn parse_usage(value: &Value) -> TokenUsage {
    let input_tokens = value
        .get("input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or_default();
    let output_tokens = value
        .get("output_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or_default();
    TokenUsage {
        input_tokens,
        output_tokens,
    }
}

fn extract_chatgpt_account_id_from_jwt(jwt: &str) -> Option<String> {
    let payload_b64 = jwt.split('.').nth(1)?;
    let payload = decode_base64url(payload_b64)?;
    let claims: Value = serde_json::from_slice(&payload).ok()?;
    claims
        .get("chatgpt_account_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn decode_base64url(input: &str) -> Option<Vec<u8>> {
    let mut s = input.replace('-', "+").replace('_', "/");
    while s.len() % 4 != 0 {
        s.push('=');
    }
    base64::engine::general_purpose::STANDARD.decode(s).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_chatgpt_account_from_jwt() {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"alg":"none","typ":"JWT"}"#);
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"chatgpt_account_id":"acc_123"}"#);
        let token = format!("{header}.{payload}.sig");
        assert_eq!(
            extract_chatgpt_account_id_from_jwt(&token),
            Some("acc_123".to_string())
        );
    }

    #[test]
    fn parse_responses_with_text_and_tool_call() {
        let raw = serde_json::json!({
            "status": "completed",
            "usage": {"input_tokens": 12, "output_tokens": 34},
            "output": [
                {
                    "type": "message",
                    "content": [
                        {"type": "output_text", "text": "hi"}
                    ]
                },
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "web_search",
                    "arguments": "{\"query\":\"rust\"}"
                }
            ]
        });
        let parsed = parse_responses_completion(raw).unwrap();
        assert_eq!(parsed.usage.input_tokens, 12);
        assert_eq!(parsed.usage.output_tokens, 34);
        assert_eq!(parsed.tool_calls.len(), 1);
        assert!(matches!(parsed.stop_reason, StopReason::ToolUse));
    }
}
