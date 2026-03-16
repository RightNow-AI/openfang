//! Google Gemini API driver.
//!
//! Native implementation of the Gemini generateContent API.
//! Gemini uses a different format from both Anthropic and OpenAI:
//! - Model goes in the URL path, not the request body
//! - Auth via `x-goog-api-key` header (not `Authorization: Bearer`)
//! - System prompt via `systemInstruction` field
//! - Tool definitions via `functionDeclarations` inside `tools[]`
//! - Response: `candidates[0].content.parts[]`

use crate::llm_driver::{CompletionRequest, CompletionResponse, LlmDriver, LlmError, StreamEvent};
use async_trait::async_trait;
use futures::StreamExt;
use openfang_types::message::{
    ContentBlock, Message, MessageContent, Role, StopReason, TokenUsage,
};
use openfang_types::tool::ToolCall;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use zeroize::Zeroizing;

/// Google Gemini API driver.
pub struct GeminiDriver {
    api_key: Zeroizing<String>,
    base_url: String,
    client: reqwest::Client,
}

impl GeminiDriver {
    /// Create a new Gemini driver.
    pub fn new(api_key: String, base_url: String) -> Self {
        Self {
            api_key: Zeroizing::new(api_key),
            base_url,
            client: reqwest::Client::builder()
                .user_agent(crate::USER_AGENT)
                .build()
                .unwrap_or_default(),
        }
    }
}

// ── Request types ──────────────────────────────────────────────────────

/// Top-level Gemini API request body.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<GeminiToolConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenerationConfig>,
}

/// A content entry (user/model turn).
#[derive(Debug, Serialize, Deserialize, Clone)]
struct GeminiContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    parts: Vec<GeminiPart>,
}

/// A part within a content entry.
///
/// Gemini 2.5+/3.x thinking models include `thoughtSignature` at the **part
/// level** (sibling of `functionCall`/`text`, not nested inside them).  When
/// echoing model turns back in the conversation history, the signature must be
/// preserved on every part that originally carried one — otherwise the API
/// returns `INVALID_ARGUMENT: Function call is missing a thought_signature`.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
enum GeminiPart {
    /// Thinking/reasoning part emitted by Gemini 2.5+ thinking models.
    /// JSON shape: `{ "text": "...", "thought": true }`.
    /// Must be listed **before** `Text` so `serde(untagged)` matches it first.
    Thought {
        text: String,
        thought: bool,
        #[serde(
            rename = "thoughtSignature",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        thought_signature: Option<String>,
    },
    /// Text part, optionally carrying a thought signature.
    Text {
        text: String,
        /// Part-level thought signature (Gemini 2.5+/3.x thinking models).
        #[serde(
            rename = "thoughtSignature",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        thought_signature: Option<String>,
    },
    InlineData {
        #[serde(rename = "inlineData")]
        inline_data: GeminiInlineData,
    },
    /// Function call part with an optional part-level thought signature.
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: GeminiFunctionCallData,
        /// Part-level thought signature (Gemini 2.5+/3.x thinking models).
        #[serde(
            rename = "thoughtSignature",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        thought_signature: Option<String>,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: GeminiFunctionResponseData,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct GeminiInlineData {
    #[serde(rename = "mimeType")]
    mime_type: String,
    data: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct GeminiFunctionCallData {
    name: String,
    args: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct GeminiFunctionResponseData {
    name: String,
    response: serde_json::Value,
}

/// Tool configuration containing function declarations.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiToolConfig {
    function_declarations: Vec<GeminiFunctionDeclaration>,
}

/// A function declaration for tool use.
#[derive(Debug, Serialize)]
struct GeminiFunctionDeclaration {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

/// Generation configuration (temperature, max tokens, etc.).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
}

// ── Response types ─────────────────────────────────────────────────────

/// Top-level Gemini API response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiResponse {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
    #[serde(default)]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiCandidate {
    content: Option<GeminiContent>,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiUsageMetadata {
    #[serde(default)]
    prompt_token_count: u64,
    #[serde(default)]
    candidates_token_count: u64,
}

/// Gemini API error response.
#[derive(Debug, Deserialize)]
struct GeminiErrorResponse {
    error: GeminiErrorDetail,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GeminiErrorDetail {
    message: String,
    #[serde(default)]
    code: Option<u16>,
    #[serde(default)]
    status: Option<String>,
}

/// Parse a Gemini error response body, handling multiple Google API error formats.
fn parse_gemini_error(body: &str) -> String {
    if let Ok(e) = serde_json::from_str::<GeminiErrorResponse>(body) {
        let mut msg = e.error.message;
        if let Some(status) = e.error.status {
            msg = format!("{status}: {msg}");
        }
        return msg;
    }
    // Google sometimes returns bare JSON arrays or HTML error pages
    if body.starts_with('<') {
        return "Google API returned an HTML error page — check your API key and model name"
            .to_string();
    }
    body.to_string()
}

// ── Message conversion ─────────────────────────────────────────────────

/// Convert OpenFang messages into Gemini content entries.
fn convert_messages(
    messages: &[Message],
    system: &Option<String>,
) -> (Vec<GeminiContent>, Option<GeminiContent>) {
    let mut contents = Vec::new();

    // Build system instruction
    let system_instruction = extract_system(messages, system);

    for msg in messages {
        if msg.role == Role::System {
            continue; // handled separately
        }

        let role = match msg.role {
            Role::User => "user",
            Role::Assistant => "model",
            Role::System => continue,
        };

        let parts = match &msg.content {
            MessageContent::Text(text) => vec![GeminiPart::Text {
                text: text.clone(),
                thought_signature: None,
            }],
            MessageContent::Blocks(blocks) => {
                let mut parts = Vec::new();
                for block in blocks {
                    match block {
                        ContentBlock::Text {
                            text,
                            provider_metadata,
                        } => {
                            // Echo back thought_signature from provider_metadata
                            // if present — required by Gemini 3.x thinking models
                            // on ALL parts, including text parts.
                            let thought_signature = provider_metadata
                                .as_ref()
                                .and_then(|m| m.get("thought_signature"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                            parts.push(GeminiPart::Text {
                                text: text.clone(),
                                thought_signature,
                            });
                        }
                        ContentBlock::ToolUse {
                            name,
                            input,
                            provider_metadata,
                            ..
                        } => {
                            // Echo back thought_signature from provider_metadata
                            // if present — required by Gemini 2.5+/3.x thinking models.
                            // The signature lives at the part level (sibling of
                            // functionCall), not inside the functionCall object.
                            let thought_signature = provider_metadata
                                .as_ref()
                                .and_then(|m| m.get("thought_signature"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                            parts.push(GeminiPart::FunctionCall {
                                function_call: GeminiFunctionCallData {
                                    name: name.clone(),
                                    args: input.clone(),
                                },
                                thought_signature,
                            });
                        }
                        ContentBlock::Image { media_type, data } => {
                            parts.push(GeminiPart::InlineData {
                                inline_data: GeminiInlineData {
                                    mime_type: media_type.clone(),
                                    data: data.clone(),
                                },
                            });
                        }
                        ContentBlock::ToolResult {
                            content, tool_name, ..
                        } => {
                            let fn_name = if tool_name.is_empty() {
                                "unknown_function".to_string()
                            } else {
                                tool_name.clone()
                            };
                            parts.push(GeminiPart::FunctionResponse {
                                function_response: GeminiFunctionResponseData {
                                    name: fn_name,
                                    response: serde_json::json!({ "result": content }),
                                },
                            });
                        }
                        ContentBlock::Thinking { .. } => {}
                        _ => {}
                    }
                }
                parts
            }
        };

        if !parts.is_empty() {
            contents.push(GeminiContent {
                role: Some(role.to_string()),
                parts,
            });
        }
    }

    (contents, system_instruction)
}

/// Extract system prompt from messages or the explicit system field.
fn extract_system(messages: &[Message], system: &Option<String>) -> Option<GeminiContent> {
    let text = system.clone().or_else(|| {
        messages.iter().find_map(|m| {
            if m.role == Role::System {
                match &m.content {
                    MessageContent::Text(t) => Some(t.clone()),
                    _ => None,
                }
            } else {
                None
            }
        })
    })?;

    Some(GeminiContent {
        role: None, // systemInstruction doesn't use a role
        parts: vec![GeminiPart::Text {
            text,
            thought_signature: None,
        }],
    })
}

/// Convert tool definitions to Gemini function declarations.
fn convert_tools(request: &CompletionRequest) -> Vec<GeminiToolConfig> {
    if request.tools.is_empty() {
        return Vec::new();
    }

    let declarations: Vec<GeminiFunctionDeclaration> = request
        .tools
        .iter()
        .map(|t| {
            // Normalize schema for Gemini (strips $schema, flattens anyOf)
            let normalized =
                openfang_types::tool::normalize_schema_for_provider(&t.input_schema, "gemini");
            GeminiFunctionDeclaration {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: normalized,
            }
        })
        .collect();

    vec![GeminiToolConfig {
        function_declarations: declarations,
    }]
}

/// Convert a Gemini response into our CompletionResponse.
fn convert_response(resp: GeminiResponse) -> Result<CompletionResponse, LlmError> {
    let candidate = resp
        .candidates
        .into_iter()
        .next()
        .ok_or_else(|| LlmError::Parse("No candidates in Gemini response".to_string()))?;

    let mut content = Vec::new();
    let mut tool_calls = Vec::new();

    match candidate.content {
        Some(gemini_content) => {
            for part in gemini_content.parts {
                match part {
                    GeminiPart::Text {
                        text,
                        thought_signature,
                    } => {
                        if !text.is_empty() {
                            // Preserve thought_signature in provider_metadata so
                            // it gets echoed back on the next request.  Gemini
                            // 3.x thinking models include thoughtSignature on
                            // ALL parts (text + functionCall).
                            let provider_metadata = thought_signature
                                .map(|sig| serde_json::json!({ "thought_signature": sig }));
                            content.push(ContentBlock::Text {
                                text,
                                provider_metadata,
                            });
                        }
                    }
                    GeminiPart::FunctionCall {
                        function_call,
                        thought_signature,
                    } => {
                        let id = format!("call_{}", uuid::Uuid::new_v4().simple());
                        // Preserve thought_signature in provider_metadata so it
                        // gets echoed back on the next request (Gemini 2.5+/3.x).
                        // The signature lives at the part level, not inside
                        // functionCall.
                        let provider_metadata = thought_signature
                            .map(|sig| serde_json::json!({ "thought_signature": sig }));
                        content.push(ContentBlock::ToolUse {
                            id: id.clone(),
                            name: function_call.name.clone(),
                            input: function_call.args.clone(),
                            provider_metadata,
                        });
                        tool_calls.push(ToolCall {
                            id,
                            name: function_call.name,
                            input: function_call.args,
                        });
                    }
                    GeminiPart::Thought { text, .. } => {
                        // Gemini 2.5+ thinking parts — internal reasoning.
                        // Store as Thinking content block so the UI can
                        // optionally display it (like <think> blocks).
                        if !text.is_empty() {
                            content.push(ContentBlock::Thinking { thinking: text });
                        }
                    }
                    GeminiPart::InlineData { .. } | GeminiPart::FunctionResponse { .. } => {
                        // Shouldn't normally appear in responses, ignore
                    }
                }
            }
        }
        None => {
            let reason = candidate.finish_reason.as_deref().unwrap_or("unknown");
            warn!(finish_reason = %reason, "Gemini returned candidate with no content");
            return Err(LlmError::Parse(format!(
                "Gemini returned empty response (finish_reason: {reason})"
            )));
        }
    }

    // Gemini uses "STOP" for both end-of-turn and function calls,
    // so check tool_calls to determine the actual stop reason.
    let stop_reason = if !tool_calls.is_empty() {
        StopReason::ToolUse
    } else {
        match candidate.finish_reason.as_deref() {
            Some("MAX_TOKENS") => StopReason::MaxTokens,
            _ => StopReason::EndTurn,
        }
    };

    let usage = resp
        .usage_metadata
        .map(|u| TokenUsage {
            input_tokens: u.prompt_token_count,
            output_tokens: u.candidates_token_count,
        })
        .unwrap_or_default();

    Ok(CompletionResponse {
        content,
        stop_reason,
        tool_calls,
        usage,
    })
}

// ── LlmDriver implementation ──────────────────────────────────────────

#[async_trait]
impl LlmDriver for GeminiDriver {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let (contents, system_instruction) = convert_messages(&request.messages, &request.system);
        let tools = convert_tools(&request);

        let gemini_request = GeminiRequest {
            contents,
            system_instruction,
            tools,
            generation_config: Some(GenerationConfig {
                temperature: Some(request.temperature),
                max_output_tokens: Some(request.max_tokens),
            }),
        };

        let max_retries = 3;
        for attempt in 0..=max_retries {
            let url = format!(
                "{}/v1beta/models/{}:generateContent?key={}",
                self.base_url,
                request.model,
                self.api_key.as_str()
            );
            debug!(url = %url, attempt, "Sending Gemini API request");

            let resp = self
                .client
                .post(&url)
                .header("x-goog-api-key", self.api_key.as_str())
                .header("content-type", "application/json")
                .json(&gemini_request)
                .send()
                .await
                .map_err(|e| LlmError::Http(e.to_string()))?;

            let status = resp.status().as_u16();

            if status == 429 || status == 503 {
                if attempt < max_retries {
                    let retry_ms = (attempt + 1) as u64 * 2000;
                    warn!(status, retry_ms, "Rate limited/overloaded, retrying");
                    tokio::time::sleep(std::time::Duration::from_millis(retry_ms)).await;
                    continue;
                }
                return Err(if status == 429 {
                    LlmError::RateLimited {
                        retry_after_ms: 5000,
                    }
                } else {
                    LlmError::Overloaded {
                        retry_after_ms: 5000,
                    }
                });
            }

            if !resp.status().is_success() {
                let body = resp.text().await.unwrap_or_default();
                let message = parse_gemini_error(&body);
                if status == 401 || status == 403 {
                    return Err(LlmError::AuthenticationFailed(message));
                }
                if status == 404 {
                    return Err(LlmError::ModelNotFound(message));
                }
                return Err(LlmError::Api { status, message });
            }

            let body = resp
                .text()
                .await
                .map_err(|e| LlmError::Http(e.to_string()))?;
            let gemini_response: GeminiResponse =
                serde_json::from_str(&body).map_err(|e| LlmError::Parse(e.to_string()))?;

            return convert_response(gemini_response);
        }

        Err(LlmError::Api {
            status: 0,
            message: "Max retries exceeded".to_string(),
        })
    }

    async fn stream(
        &self,
        request: CompletionRequest,
        tx: tokio::sync::mpsc::Sender<StreamEvent>,
    ) -> Result<CompletionResponse, LlmError> {
        let (contents, system_instruction) = convert_messages(&request.messages, &request.system);
        let tools = convert_tools(&request);

        let gemini_request = GeminiRequest {
            contents,
            system_instruction,
            tools,
            generation_config: Some(GenerationConfig {
                temperature: Some(request.temperature),
                max_output_tokens: Some(request.max_tokens),
            }),
        };

        let max_retries = 3;
        for attempt in 0..=max_retries {
            let url = format!(
                "{}/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
                self.base_url,
                request.model,
                self.api_key.as_str()
            );
            debug!(url = %url, attempt, "Sending Gemini streaming request");

            let resp = self
                .client
                .post(&url)
                .header("x-goog-api-key", self.api_key.as_str())
                .header("content-type", "application/json")
                .json(&gemini_request)
                .send()
                .await
                .map_err(|e| LlmError::Http(e.to_string()))?;

            let status = resp.status().as_u16();

            if status == 429 || status == 503 {
                if attempt < max_retries {
                    let retry_ms = (attempt + 1) as u64 * 2000;
                    warn!(
                        status,
                        retry_ms, "Rate limited/overloaded (stream), retrying"
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(retry_ms)).await;
                    continue;
                }
                return Err(if status == 429 {
                    LlmError::RateLimited {
                        retry_after_ms: 5000,
                    }
                } else {
                    LlmError::Overloaded {
                        retry_after_ms: 5000,
                    }
                });
            }

            if !resp.status().is_success() {
                let body = resp.text().await.unwrap_or_default();
                let message = parse_gemini_error(&body);
                if status == 401 || status == 403 {
                    return Err(LlmError::AuthenticationFailed(message));
                }
                if status == 404 {
                    return Err(LlmError::ModelNotFound(message));
                }
                return Err(LlmError::Api { status, message });
            }

            // Parse SSE stream
            let mut buffer = String::new();
            let mut text_content = String::new();
            // Thought signature for accumulated text content (last one wins)
            let mut text_thought_sig: Option<String> = None;
            // Track function calls: (name, args_json, thought_signature)
            let mut fn_calls: Vec<(String, serde_json::Value, Option<String>)> = Vec::new();
            let mut finish_reason: Option<String> = None;
            let mut usage = TokenUsage::default();

            let mut byte_stream = resp.bytes_stream();
            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = chunk_result.map_err(|e| LlmError::Http(e.to_string()))?;
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // Process complete SSE events (delimited by \n\n or \r\n\r\n)
                while let Some(pos) = buffer.find("\n\n") {
                    let event_text = buffer[..pos].to_string();
                    buffer = buffer[pos + 2..].to_string();

                    // Extract the data line (handle both "data: " and "data:" formats)
                    let data = event_text
                        .lines()
                        .find_map(|line| line.strip_prefix("data:").map(|d| d.trim_start()))
                        .unwrap_or("");

                    if data.is_empty() {
                        continue;
                    }

                    let json: GeminiResponse = match serde_json::from_str(data) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    // Extract usage from each chunk (last one wins)
                    if let Some(ref u) = json.usage_metadata {
                        usage.input_tokens = u.prompt_token_count;
                        usage.output_tokens = u.candidates_token_count;
                    }

                    for candidate in &json.candidates {
                        if let Some(fr) = &candidate.finish_reason {
                            finish_reason = Some(fr.clone());
                        }

                        if let Some(ref content) = candidate.content {
                            for part in &content.parts {
                                match part {
                                    GeminiPart::Text {
                                        text,
                                        thought_signature,
                                    } => {
                                        if !text.is_empty() {
                                            text_content.push_str(text);
                                            let _ = tx
                                                .send(StreamEvent::TextDelta { text: text.clone() })
                                                .await;
                                        }
                                        // Capture thought signature for text parts
                                        // (last one wins across streamed chunks).
                                        if thought_signature.is_some() {
                                            text_thought_sig = thought_signature.clone();
                                        }
                                    }
                                    GeminiPart::FunctionCall {
                                        function_call,
                                        thought_signature,
                                    } => {
                                        let id = format!("call_{}", uuid::Uuid::new_v4().simple());
                                        let _ = tx
                                            .send(StreamEvent::ToolUseStart {
                                                id: id.clone(),
                                                name: function_call.name.clone(),
                                            })
                                            .await;
                                        let args_str = serde_json::to_string(&function_call.args)
                                            .unwrap_or_default();
                                        let _ = tx
                                            .send(StreamEvent::ToolInputDelta { text: args_str })
                                            .await;
                                        let _ = tx
                                            .send(StreamEvent::ToolUseEnd {
                                                id,
                                                name: function_call.name.clone(),
                                                input: function_call.args.clone(),
                                            })
                                            .await;
                                        fn_calls.push((
                                            function_call.name.clone(),
                                            function_call.args.clone(),
                                            thought_signature.clone(),
                                        ));
                                    }
                                    GeminiPart::Thought { ref text, .. } => {
                                        // Gemini 2.5+ thinking chunk — emit as
                                        // thinking delta so UIs can optionally
                                        // show it; do NOT mix into text_content.
                                        if !text.is_empty() {
                                            let _ = tx
                                                .send(StreamEvent::ThinkingDelta {
                                                    text: text.clone(),
                                                })
                                                .await;
                                        }
                                    }
                                    GeminiPart::InlineData { .. }
                                    | GeminiPart::FunctionResponse { .. } => {}
                                }
                            }
                        }
                    }
                }
            }

            // Build final response
            let mut content = Vec::new();
            let mut tool_calls = Vec::new();

            if !text_content.is_empty() {
                let provider_metadata =
                    text_thought_sig.map(|sig| serde_json::json!({ "thought_signature": sig }));
                content.push(ContentBlock::Text {
                    text: text_content,
                    provider_metadata,
                });
            }

            for (name, args, thought_sig) in fn_calls {
                let id = format!("call_{}", uuid::Uuid::new_v4().simple());
                let provider_metadata = thought_sig
                    .as_ref()
                    .map(|sig| serde_json::json!({ "thought_signature": sig }));
                content.push(ContentBlock::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: args.clone(),
                    provider_metadata,
                });
                tool_calls.push(ToolCall {
                    id,
                    name,
                    input: args,
                });
            }

            let stop_reason = match finish_reason.as_deref() {
                Some("STOP") => StopReason::EndTurn,
                Some("MAX_TOKENS") => StopReason::MaxTokens,
                Some("SAFETY") => StopReason::EndTurn,
                _ => {
                    if !tool_calls.is_empty() {
                        StopReason::ToolUse
                    } else {
                        StopReason::EndTurn
                    }
                }
            };

            let _ = tx
                .send(StreamEvent::ContentComplete { stop_reason, usage })
                .await;

            return Ok(CompletionResponse {
                content,
                stop_reason,
                tool_calls,
                usage,
            });
        }

        Err(LlmError::Api {
            status: 0,
            message: "Max retries exceeded".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_types::tool::ToolDefinition;

    #[test]
    fn test_gemini_driver_creation() {
        let driver = GeminiDriver::new(
            "test-key".to_string(),
            "https://generativelanguage.googleapis.com".to_string(),
        );
        assert_eq!(driver.api_key.as_str(), "test-key");
        assert_eq!(driver.base_url, "https://generativelanguage.googleapis.com");
    }

    #[test]
    fn test_gemini_request_serialization() {
        let req = GeminiRequest {
            contents: vec![GeminiContent {
                role: Some("user".to_string()),
                parts: vec![GeminiPart::Text {
                    text: "Hello".to_string(),
                    thought_signature: None,
                }],
            }],
            system_instruction: Some(GeminiContent {
                role: None,
                parts: vec![GeminiPart::Text {
                    text: "You are helpful.".to_string(),
                    thought_signature: None,
                }],
            }),
            tools: vec![],
            generation_config: Some(GenerationConfig {
                temperature: Some(0.7),
                max_output_tokens: Some(1024),
            }),
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["contents"][0]["role"], "user");
        assert_eq!(json["contents"][0]["parts"][0]["text"], "Hello");
        assert_eq!(
            json["systemInstruction"]["parts"][0]["text"],
            "You are helpful."
        );
        assert!(json["systemInstruction"]["role"].is_null());
        let temp = json["generationConfig"]["temperature"].as_f64().unwrap();
        assert!(
            (temp - 0.7).abs() < 0.001,
            "temperature should be ~0.7, got {temp}"
        );
        assert_eq!(json["generationConfig"]["maxOutputTokens"], 1024);
    }

    #[test]
    fn test_gemini_response_deserialization() {
        let json = serde_json::json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{"text": "Hello! How can I help?"}]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 8
            }
        });

        let resp: GeminiResponse = serde_json::from_value(json).unwrap();
        assert_eq!(resp.candidates.len(), 1);
        assert_eq!(resp.candidates[0].finish_reason.as_deref(), Some("STOP"));
        let usage = resp.usage_metadata.unwrap();
        assert_eq!(usage.prompt_token_count, 10);
        assert_eq!(usage.candidates_token_count, 8);
    }

    #[test]
    fn test_gemini_function_call_response() {
        let json = serde_json::json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{
                        "functionCall": {
                            "name": "web_search",
                            "args": {"query": "rust programming"}
                        }
                    }]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 20,
                "candidatesTokenCount": 15
            }
        });

        let resp: GeminiResponse = serde_json::from_value(json).unwrap();
        let completion = convert_response(resp).unwrap();
        assert_eq!(completion.tool_calls.len(), 1);
        assert_eq!(completion.tool_calls[0].name, "web_search");
        assert_eq!(
            completion.tool_calls[0].input,
            serde_json::json!({"query": "rust programming"})
        );
        assert_eq!(completion.stop_reason, StopReason::ToolUse);
    }

    #[test]
    fn test_convert_messages_with_system() {
        let messages = vec![Message::user("Hello")];
        let system = Some("Be helpful.".to_string());
        let (contents, sys_instruction) = convert_messages(&messages, &system);

        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].role.as_deref(), Some("user"));
        assert!(sys_instruction.is_some());
        let sys = sys_instruction.unwrap();
        assert!(sys.role.is_none());
        match &sys.parts[0] {
            GeminiPart::Text { text, .. } => assert_eq!(text, "Be helpful."),
            _ => panic!("Expected text part"),
        }
    }

    #[test]
    fn test_convert_messages_assistant_role() {
        let messages = vec![Message::user("Hello"), Message::assistant("Hi there!")];
        let (contents, _) = convert_messages(&messages, &None);
        assert_eq!(contents.len(), 2);
        assert_eq!(contents[0].role.as_deref(), Some("user"));
        assert_eq!(contents[1].role.as_deref(), Some("model"));
    }

    #[test]
    fn test_convert_tools() {
        let request = CompletionRequest {
            model: "gemini-2.0-flash".to_string(),
            messages: vec![],
            tools: vec![ToolDefinition {
                name: "web_search".to_string(),
                description: "Search the web".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"}
                    }
                }),
            }],
            max_tokens: 1024,
            temperature: 0.7,
            system: None,
            thinking: None,
        };

        let tools = convert_tools(&request);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].function_declarations.len(), 1);
        assert_eq!(tools[0].function_declarations[0].name, "web_search");
    }

    #[test]
    fn test_convert_tools_empty() {
        let request = CompletionRequest {
            model: "gemini-2.0-flash".to_string(),
            messages: vec![],
            tools: vec![],
            max_tokens: 1024,
            temperature: 0.7,
            system: None,
            thinking: None,
        };

        let tools = convert_tools(&request);
        assert!(tools.is_empty());
    }

    #[test]
    fn test_convert_response_text_only() {
        let resp = GeminiResponse {
            candidates: vec![GeminiCandidate {
                content: Some(GeminiContent {
                    role: Some("model".to_string()),
                    parts: vec![GeminiPart::Text {
                        text: "Hello!".to_string(),
                        thought_signature: None,
                    }],
                }),
                finish_reason: Some("STOP".to_string()),
            }],
            usage_metadata: Some(GeminiUsageMetadata {
                prompt_token_count: 5,
                candidates_token_count: 3,
            }),
        };

        let completion = convert_response(resp).unwrap();
        assert_eq!(completion.content.len(), 1);
        assert!(completion.tool_calls.is_empty());
        assert_eq!(completion.stop_reason, StopReason::EndTurn);
        assert_eq!(completion.usage.input_tokens, 5);
        assert_eq!(completion.usage.output_tokens, 3);
        assert_eq!(completion.usage.total(), 8);
    }

    #[test]
    fn test_convert_response_no_candidates() {
        let resp = GeminiResponse {
            candidates: vec![],
            usage_metadata: None,
        };

        let result = convert_response(resp);
        assert!(result.is_err());
    }

    #[test]
    fn test_convert_response_max_tokens() {
        let resp = GeminiResponse {
            candidates: vec![GeminiCandidate {
                content: Some(GeminiContent {
                    role: Some("model".to_string()),
                    parts: vec![GeminiPart::Text {
                        text: "Truncated...".to_string(),
                        thought_signature: None,
                    }],
                }),
                finish_reason: Some("MAX_TOKENS".to_string()),
            }],
            usage_metadata: None,
        };

        let completion = convert_response(resp).unwrap();
        assert_eq!(completion.stop_reason, StopReason::MaxTokens);
    }

    #[test]
    fn test_gemini_error_response_deserialization() {
        let json = serde_json::json!({
            "error": {
                "message": "API key not valid."
            }
        });

        let err: GeminiErrorResponse = serde_json::from_value(json).unwrap();
        assert_eq!(err.error.message, "API key not valid.");
    }

    #[test]
    fn test_extract_system_from_explicit() {
        let messages = vec![Message::user("Hi")];
        let system = Some("Be concise.".to_string());
        let result = extract_system(&messages, &system);
        assert!(result.is_some());
        match &result.unwrap().parts[0] {
            GeminiPart::Text { text, .. } => assert_eq!(text, "Be concise."),
            _ => panic!("Expected text"),
        }
    }

    #[test]
    fn test_extract_system_from_messages() {
        let messages = vec![
            Message {
                role: Role::System,
                content: MessageContent::Text("System prompt here.".to_string()),
            },
            Message::user("Hi"),
        ];
        let result = extract_system(&messages, &None);
        assert!(result.is_some());
        match &result.unwrap().parts[0] {
            GeminiPart::Text { text, .. } => assert_eq!(text, "System prompt here."),
            _ => panic!("Expected text"),
        }
    }

    #[test]
    fn test_extract_system_none() {
        let messages = vec![Message::user("Hi")];
        let result = extract_system(&messages, &None);
        assert!(result.is_none());
    }

    #[test]
    fn test_generation_config_serialization() {
        let config = GenerationConfig {
            temperature: Some(0.5),
            max_output_tokens: Some(2048),
        };
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["temperature"], 0.5);
        assert_eq!(json["maxOutputTokens"], 2048);
    }

    // --- Issue #501/#506: thought_signature round-trip tests ---
    //
    // Gemini 2.5+/3.x thinking models include `thoughtSignature` at the PART
    // level (sibling of `functionCall`/`text`).  All signatures must be echoed
    // back exactly as received in follow-up requests.

    #[test]
    fn test_thought_signature_captured_from_function_call_response() {
        // Gemini 3.x: thoughtSignature at the part level (sibling of functionCall).
        let json = serde_json::json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{
                        "functionCall": {
                            "name": "web_search",
                            "args": {"query": "rust lang"}
                        },
                        "thoughtSignature": "abc123signature"
                    }]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 20,
                "candidatesTokenCount": 15
            }
        });

        let resp: GeminiResponse = serde_json::from_value(json).unwrap();
        let completion = convert_response(resp).unwrap();
        assert_eq!(completion.tool_calls.len(), 1);
        assert_eq!(completion.tool_calls[0].name, "web_search");
        assert_eq!(completion.stop_reason, StopReason::ToolUse);

        // The thought_signature should be stored in provider_metadata
        let tool_use_block = &completion.content[0];
        match tool_use_block {
            ContentBlock::ToolUse {
                provider_metadata, ..
            } => {
                let meta = provider_metadata
                    .as_ref()
                    .expect("provider_metadata should be set");
                assert_eq!(meta["thought_signature"], "abc123signature");
            }
            _ => panic!("Expected ToolUse content block"),
        }
    }

    #[test]
    fn test_thought_signature_captured_from_text_response() {
        // Gemini 3.x: thoughtSignature on text parts too.
        let json = serde_json::json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{
                        "text": "Let me think about this...",
                        "thoughtSignature": "text_sig_456"
                    }]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 8
            }
        });

        let resp: GeminiResponse = serde_json::from_value(json).unwrap();
        let completion = convert_response(resp).unwrap();

        match &completion.content[0] {
            ContentBlock::Text {
                text,
                provider_metadata,
            } => {
                assert_eq!(text, "Let me think about this...");
                let meta = provider_metadata
                    .as_ref()
                    .expect("provider_metadata should be set for text with thoughtSignature");
                assert_eq!(meta["thought_signature"], "text_sig_456");
            }
            _ => panic!("Expected Text content block"),
        }
    }

    #[test]
    fn test_thought_signature_echoed_in_request_function_call() {
        // When a ToolUse block carries provider_metadata with thought_signature,
        // convert_messages should echo it back at the part level.
        let messages = vec![
            Message::user("Search for rust"),
            Message {
                role: Role::Assistant,
                content: MessageContent::Blocks(vec![ContentBlock::ToolUse {
                    id: "call_123".to_string(),
                    name: "web_search".to_string(),
                    input: serde_json::json!({"query": "rust"}),
                    provider_metadata: Some(serde_json::json!({
                        "thought_signature": "sig_xyz789"
                    })),
                }]),
            },
            Message {
                role: Role::User,
                content: MessageContent::Blocks(vec![ContentBlock::ToolResult {
                    tool_use_id: "call_123".to_string(),
                    tool_name: "web_search".to_string(),
                    content: "Results about Rust programming".to_string(),
                    is_error: false,
                }]),
            },
        ];

        let (contents, _) = convert_messages(&messages, &None);

        // The assistant's turn (index 1) should have a FunctionCall with the
        // thought_signature at the part level.
        let assistant_turn = &contents[1];
        assert_eq!(assistant_turn.role.as_deref(), Some("model"));

        let fc_part = &assistant_turn.parts[0];
        match fc_part {
            GeminiPart::FunctionCall {
                function_call,
                thought_signature,
            } => {
                assert_eq!(function_call.name, "web_search");
                assert_eq!(thought_signature.as_deref(), Some("sig_xyz789"));
            }
            _ => panic!("Expected FunctionCall part"),
        }
    }

    #[test]
    fn test_thought_signature_echoed_in_request_text() {
        // When a Text block carries provider_metadata with thought_signature,
        // convert_messages should echo it back on the text part.
        let messages = vec![
            Message::user("Hello"),
            Message {
                role: Role::Assistant,
                content: MessageContent::Blocks(vec![ContentBlock::Text {
                    text: "Let me think...".to_string(),
                    provider_metadata: Some(serde_json::json!({
                        "thought_signature": "text_sig_abc"
                    })),
                }]),
            },
        ];

        let (contents, _) = convert_messages(&messages, &None);
        let assistant_turn = &contents[1];
        assert_eq!(assistant_turn.role.as_deref(), Some("model"));

        match &assistant_turn.parts[0] {
            GeminiPart::Text {
                text,
                thought_signature,
            } => {
                assert_eq!(text, "Let me think...");
                assert_eq!(thought_signature.as_deref(), Some("text_sig_abc"));
            }
            _ => panic!("Expected Text part"),
        }
    }

    #[test]
    fn test_thought_signature_none_when_absent() {
        // When there's no thought_signature, provider_metadata should be None
        let json = serde_json::json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{
                        "functionCall": {
                            "name": "read_file",
                            "args": {"path": "/tmp/test.txt"}
                        }
                    }]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 5
            }
        });

        let resp: GeminiResponse = serde_json::from_value(json).unwrap();
        let completion = convert_response(resp).unwrap();

        match &completion.content[0] {
            ContentBlock::ToolUse {
                provider_metadata, ..
            } => {
                assert!(
                    provider_metadata.is_none(),
                    "provider_metadata should be None when no thoughtSignature"
                );
            }
            _ => panic!("Expected ToolUse"),
        }
    }

    #[test]
    fn test_thought_signature_not_echoed_without_metadata() {
        // ToolUse blocks without provider_metadata should produce
        // GeminiPart::FunctionCall with thought_signature: None
        let messages = vec![
            Message::user("Hello"),
            Message {
                role: Role::Assistant,
                content: MessageContent::Blocks(vec![ContentBlock::ToolUse {
                    id: "call_456".to_string(),
                    name: "read_file".to_string(),
                    input: serde_json::json!({"path": "/tmp/test"}),
                    provider_metadata: None,
                }]),
            },
        ];

        let (contents, _) = convert_messages(&messages, &None);
        let assistant_turn = &contents[1];

        match &assistant_turn.parts[0] {
            GeminiPart::FunctionCall {
                thought_signature, ..
            } => {
                assert!(
                    thought_signature.is_none(),
                    "thought_signature should be None when no provider_metadata"
                );
            }
            _ => panic!("Expected FunctionCall part"),
        }
    }

    #[test]
    fn test_thought_signature_serialization_round_trip() {
        // Verify the part-level thoughtSignature serializes correctly
        let part = GeminiPart::FunctionCall {
            function_call: GeminiFunctionCallData {
                name: "web_search".to_string(),
                args: serde_json::json!({"query": "test"}),
            },
            thought_signature: Some("my_sig_abc".to_string()),
        };
        let json = serde_json::to_value(&part).unwrap();
        // thoughtSignature should be at the part level (sibling of functionCall),
        // NOT nested inside functionCall.
        assert_eq!(json["thoughtSignature"], "my_sig_abc");
        assert_eq!(json["functionCall"]["name"], "web_search");
        assert!(
            json["functionCall"].get("thoughtSignature").is_none(),
            "thoughtSignature must NOT be nested inside functionCall"
        );

        // Verify it can round-trip through deserialization
        let deserialized: GeminiPart = serde_json::from_value(json).unwrap();
        match deserialized {
            GeminiPart::FunctionCall {
                thought_signature, ..
            } => {
                assert_eq!(thought_signature.as_deref(), Some("my_sig_abc"));
            }
            _ => panic!("Expected FunctionCall"),
        }
    }

    #[test]
    fn test_thought_signature_omitted_when_none() {
        // When thought_signature is None, the JSON should not contain the field
        let part = GeminiPart::FunctionCall {
            function_call: GeminiFunctionCallData {
                name: "read_file".to_string(),
                args: serde_json::json!({}),
            },
            thought_signature: None,
        };
        let json = serde_json::to_value(&part).unwrap();
        assert!(
            json.get("thoughtSignature").is_none(),
            "thoughtSignature should be omitted when None"
        );
    }

    #[test]
    fn test_thought_signature_omitted_on_text_when_none() {
        let part = GeminiPart::Text {
            text: "Hello".to_string(),
            thought_signature: None,
        };
        let json = serde_json::to_value(&part).unwrap();
        assert!(
            json.get("thoughtSignature").is_none(),
            "thoughtSignature should be omitted on text parts when None"
        );
        assert_eq!(json["text"], "Hello");
    }

    #[test]
    fn test_text_thought_signature_round_trip() {
        // Verify text part thought signature serializes at part level
        let part = GeminiPart::Text {
            text: "Thinking...".to_string(),
            thought_signature: Some("text_sig_xyz".to_string()),
        };
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(json["text"], "Thinking...");
        assert_eq!(json["thoughtSignature"], "text_sig_xyz");

        // Round-trip
        let deserialized: GeminiPart = serde_json::from_value(json).unwrap();
        match deserialized {
            GeminiPart::Text {
                text,
                thought_signature,
            } => {
                assert_eq!(text, "Thinking...");
                assert_eq!(thought_signature.as_deref(), Some("text_sig_xyz"));
            }
            _ => panic!("Expected Text"),
        }
    }

    #[test]
    fn test_multiple_function_calls_with_mixed_signatures() {
        // Response with multiple function calls, some with signatures, some without.
        // Gemini 3.x: thoughtSignature at part level.
        let json = serde_json::json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [
                        {
                            "functionCall": {
                                "name": "web_search",
                                "args": {"query": "rust"}
                            },
                            "thoughtSignature": "sig_1"
                        },
                        {
                            "functionCall": {
                                "name": "read_file",
                                "args": {"path": "/tmp/test"}
                            }
                        }
                    ]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 30,
                "candidatesTokenCount": 20
            }
        });

        let resp: GeminiResponse = serde_json::from_value(json).unwrap();
        let completion = convert_response(resp).unwrap();
        assert_eq!(completion.tool_calls.len(), 2);

        // First call has signature
        match &completion.content[0] {
            ContentBlock::ToolUse {
                name,
                provider_metadata,
                ..
            } => {
                assert_eq!(name, "web_search");
                let meta = provider_metadata.as_ref().unwrap();
                assert_eq!(meta["thought_signature"], "sig_1");
            }
            _ => panic!("Expected ToolUse"),
        }

        // Second call has no signature
        match &completion.content[1] {
            ContentBlock::ToolUse {
                name,
                provider_metadata,
                ..
            } => {
                assert_eq!(name, "read_file");
                assert!(provider_metadata.is_none());
            }
            _ => panic!("Expected ToolUse"),
        }
    }

    #[test]
    fn test_gemini_3x_text_and_function_call_both_have_signatures() {
        // Gemini 3.x thinking models include thoughtSignature on ALL parts:
        // text parts AND functionCall parts.  Both must round-trip.
        let json = serde_json::json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [
                        {
                            "text": "I'll search for that.",
                            "thoughtSignature": "text_sig_aaa"
                        },
                        {
                            "functionCall": {
                                "name": "web_search",
                                "args": {"query": "rust"}
                            },
                            "thoughtSignature": "fc_sig_bbb"
                        }
                    ]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 20,
                "candidatesTokenCount": 15
            }
        });

        let resp: GeminiResponse = serde_json::from_value(json).unwrap();
        let completion = convert_response(resp).unwrap();

        // Text part should have its signature
        match &completion.content[0] {
            ContentBlock::Text {
                text,
                provider_metadata,
            } => {
                assert_eq!(text, "I'll search for that.");
                let meta = provider_metadata.as_ref().unwrap();
                assert_eq!(meta["thought_signature"], "text_sig_aaa");
            }
            _ => panic!("Expected Text"),
        }

        // Function call part should have its signature
        match &completion.content[1] {
            ContentBlock::ToolUse {
                name,
                provider_metadata,
                ..
            } => {
                assert_eq!(name, "web_search");
                let meta = provider_metadata.as_ref().unwrap();
                assert_eq!(meta["thought_signature"], "fc_sig_bbb");
            }
            _ => panic!("Expected ToolUse"),
        }

        // Now convert back to Gemini format and verify signatures are echoed
        let messages = vec![
            Message::user("Search for rust"),
            Message {
                role: Role::Assistant,
                content: MessageContent::Blocks(completion.content),
            },
        ];
        let (contents, _) = convert_messages(&messages, &None);
        let model_turn = &contents[1];

        // Verify text part echoes back its signature
        match &model_turn.parts[0] {
            GeminiPart::Text {
                thought_signature, ..
            } => {
                assert_eq!(
                    thought_signature.as_deref(),
                    Some("text_sig_aaa"),
                    "Text part must echo back its thoughtSignature"
                );
            }
            _ => panic!("Expected Text part"),
        }

        // Verify function call part echoes back its signature
        match &model_turn.parts[1] {
            GeminiPart::FunctionCall {
                thought_signature, ..
            } => {
                assert_eq!(
                    thought_signature.as_deref(),
                    Some("fc_sig_bbb"),
                    "FunctionCall part must echo back its thoughtSignature"
                );
            }
            _ => panic!("Expected FunctionCall part"),
        }

        // Verify the serialized JSON has signatures at the part level
        let serialized = serde_json::to_value(&model_turn.parts[0]).unwrap();
        assert_eq!(serialized["thoughtSignature"], "text_sig_aaa");
        let serialized = serde_json::to_value(&model_turn.parts[1]).unwrap();
        assert_eq!(serialized["thoughtSignature"], "fc_sig_bbb");
        assert!(
            serialized["functionCall"].get("thoughtSignature").is_none(),
            "thoughtSignature must be at part level, NOT inside functionCall"
        );
    }

    #[test]
    fn test_thought_part_deserialization() {
        // Gemini 2.5+ thinking models emit { "text": "...", "thought": true }
        let json = r#"{"text": "Let me think about this...", "thought": true}"#;
        let part: GeminiPart = serde_json::from_str(json).unwrap();
        match part {
            GeminiPart::Thought { text, thought, .. } => {
                assert_eq!(text, "Let me think about this...");
                assert!(thought);
            }
            _ => panic!("Expected Thought variant, got {:?}", part),
        }
    }

    #[test]
    fn test_thought_part_with_signature() {
        let json = r#"{"text": "reasoning...", "thought": true, "thoughtSignature": "sig_abc123"}"#;
        let part: GeminiPart = serde_json::from_str(json).unwrap();
        match part {
            GeminiPart::Thought {
                text,
                thought,
                thought_signature,
            } => {
                assert_eq!(text, "reasoning...");
                assert!(thought);
                assert_eq!(thought_signature.as_deref(), Some("sig_abc123"));
            }
            _ => panic!("Expected Thought variant"),
        }
    }

    #[test]
    fn test_text_part_still_works_without_thought() {
        // Regular text parts (no `thought` field) must still deserialize as Text
        let json = r#"{"text": "Hello world"}"#;
        let part: GeminiPart = serde_json::from_str(json).unwrap();
        match part {
            GeminiPart::Text { text, .. } => assert_eq!(text, "Hello world"),
            // Thought variant with thought=false would also be acceptable
            GeminiPart::Thought { text, thought, .. } => {
                assert_eq!(text, "Hello world");
                assert!(!thought);
            }
            _ => panic!("Expected Text or Thought variant"),
        }
    }

    #[test]
    fn test_thought_part_in_response_produces_thinking_block() {
        let resp = GeminiResponse {
            candidates: vec![GeminiCandidate {
                content: Some(GeminiContent {
                    role: Some("model".to_string()),
                    parts: vec![
                        GeminiPart::Thought {
                            text: "Let me reason...".to_string(),
                            thought: true,
                            thought_signature: None,
                        },
                        GeminiPart::Text {
                            text: "Here is my answer.".to_string(),
                            thought_signature: None,
                        },
                    ],
                }),
                finish_reason: Some("STOP".to_string()),
            }],
            usage_metadata: Some(GeminiUsageMetadata {
                prompt_token_count: 10,
                candidates_token_count: 20,
            }),
        };
        let completion = convert_response(resp).unwrap();
        // Should have a Thinking block and a Text block
        assert_eq!(completion.content.len(), 2);
        match &completion.content[0] {
            ContentBlock::Thinking { thinking } => {
                assert_eq!(thinking, "Let me reason...");
            }
            _ => panic!("Expected Thinking block, got {:?}", completion.content[0]),
        }
        match &completion.content[1] {
            ContentBlock::Text { text, .. } => {
                assert_eq!(text, "Here is my answer.");
            }
            _ => panic!("Expected Text block"),
        }
    }
}
