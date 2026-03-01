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
            client: reqwest::Client::new(),
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
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
enum GeminiPart {
    Text {
        text: String,
    },
    InlineData {
        #[serde(rename = "inlineData")]
        inline_data: GeminiInlineData,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: GeminiFunctionCallData,
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
struct GeminiErrorDetail {
    message: String,
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
            MessageContent::Text(text) => vec![GeminiPart::Text { text: text.clone() }],
            MessageContent::Blocks(blocks) => {
                let mut parts = Vec::new();
                for block in blocks {
                    match block {
                        ContentBlock::Text { text } => {
                            parts.push(GeminiPart::Text { text: text.clone() });
                        }
                        ContentBlock::ToolUse { name, input, .. } => {
                            parts.push(GeminiPart::FunctionCall {
                                function_call: GeminiFunctionCallData {
                                    name: name.clone(),
                                    args: input.clone(),
                                },
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
        parts: vec![GeminiPart::Text { text }],
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

    if let Some(gemini_content) = candidate.content {
        for part in gemini_content.parts {
            match part {
                GeminiPart::Text { text } => {
                    if !text.is_empty() {
                        content.push(ContentBlock::Text { text });
                    }
                }
                GeminiPart::FunctionCall { function_call } => {
                    let id = format!("call_{}", uuid::Uuid::new_v4().simple());
                    content.push(ContentBlock::ToolUse {
                        id: id.clone(),
                        name: function_call.name.clone(),
                        input: function_call.args.clone(),
                    });
                    tool_calls.push(ToolCall {
                        id,
                        name: function_call.name,
                        input: function_call.args,
                    });
                }
                GeminiPart::InlineData { .. } | GeminiPart::FunctionResponse { .. } => {
                    // Shouldn't normally appear in responses, ignore
                }
            }
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
                "{}/v1beta/models/{}:generateContent",
                self.base_url, request.model
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
                let message = serde_json::from_str::<GeminiErrorResponse>(&body)
                    .map(|e| e.error.message)
                    .unwrap_or(body);
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
                "{}/v1beta/models/{}:streamGenerateContent?alt=sse",
                self.base_url, request.model
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
                let message = serde_json::from_str::<GeminiErrorResponse>(&body)
                    .map(|e| e.error.message)
                    .unwrap_or(body);
                return Err(LlmError::Api { status, message });
            }

            // Parse SSE stream
            let mut buffer = String::new();
            let mut text_content = String::new();
            // Track function calls: (name, args_json)
            let mut fn_calls: Vec<(String, serde_json::Value)> = Vec::new();
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

                    // Extract the data line
                    let data = event_text
                        .lines()
                        .find_map(|line| line.strip_prefix("data: "))
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
                                    GeminiPart::Text { text } => {
                                        if !text.is_empty() {
                                            text_content.push_str(text);
                                            let _ = tx
                                                .send(StreamEvent::TextDelta { text: text.clone() })
                                                .await;
                                        }
                                    }
                                    GeminiPart::FunctionCall { function_call } => {
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
                                        ));
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
                content.push(ContentBlock::Text { text: text_content });
            }

            for (name, args) in fn_calls {
                let id = format!("call_{}", uuid::Uuid::new_v4().simple());
                content.push(ContentBlock::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: args.clone(),
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
                }],
            }],
            system_instruction: Some(GeminiContent {
                role: None,
                parts: vec![GeminiPart::Text {
                    text: "You are helpful.".to_string(),
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
            GeminiPart::Text { text } => assert_eq!(text, "Be helpful."),
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
            GeminiPart::Text { text } => assert_eq!(text, "Be concise."),
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
            GeminiPart::Text { text } => assert_eq!(text, "System prompt here."),
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
}
