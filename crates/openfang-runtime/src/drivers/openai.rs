//! OpenAI-compatible API driver.
//!
//! Works with OpenAI, Ollama, vLLM, and any other OpenAI-compatible endpoint.

use crate::llm_driver::{CompletionRequest, CompletionResponse, LlmDriver, LlmError, StreamEvent};
use async_trait::async_trait;
use futures::StreamExt;
use openfang_types::message::{ContentBlock, MessageContent, Role, StopReason, TokenUsage};
use openfang_types::tool::ToolCall;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use zeroize::Zeroizing;

/// OpenAI-compatible API driver.
pub struct OpenAIDriver {
    api_key: Zeroizing<String>,
    base_url: String,
    client: reqwest::Client,
}

impl OpenAIDriver {
    /// Create a new OpenAI-compatible driver.
    pub fn new(api_key: String, base_url: String) -> Self {
        Self {
            api_key: Zeroizing::new(api_key),
            base_url,
            client: reqwest::Client::new(),
        }
    }

    fn build_responses_input(request: &CompletionRequest) -> String {
        let mut lines = Vec::new();
        for msg in &request.messages {
            let role = match msg.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => "assistant",
            };

            match &msg.content {
                MessageContent::Text(text) => {
                    if !text.trim().is_empty() {
                        lines.push(format!("{role}: {text}"));
                    }
                }
                MessageContent::Blocks(blocks) => {
                    let mut parts = Vec::new();
                    for block in blocks {
                        match block {
                            ContentBlock::Text { text } => {
                                if !text.trim().is_empty() {
                                    parts.push(text.clone());
                                }
                            }
                            ContentBlock::ToolUse { name, input, .. } => {
                                parts.push(format!(
                                    "[tool_use:{name}] {}",
                                    serde_json::to_string(input).unwrap_or_default()
                                ));
                            }
                            ContentBlock::ToolResult { content, .. } => {
                                parts.push(format!("[tool_result] {content}"));
                            }
                            ContentBlock::Image { .. } => {
                                parts.push("[image omitted]".to_string());
                            }
                            ContentBlock::Thinking { .. } => {}
                            _ => {}
                        }
                    }
                    if !parts.is_empty() {
                        lines.push(format!("{role}: {}", parts.join("\n")));
                    }
                }
            }
        }
        lines.join("\n\n")
    }

    async fn complete_via_responses(
        &self,
        request: &CompletionRequest,
        api_model: &str,
    ) -> Result<CompletionResponse, LlmError> {
        let instructions = request
            .system
            .clone()
            .unwrap_or_else(|| "You are a helpful assistant.".to_string());
        let input = Self::build_responses_input(request);
        let reasoning =
            configured_reasoning_effort(api_model).map(|effort| ResponsesReasoning { effort });

        let tools: Vec<ResponsesTool> = request
            .tools
            .iter()
            .map(|t| ResponsesTool {
                tool_type: "function".to_string(),
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: openfang_types::tool::normalize_schema_for_provider(
                    &t.input_schema,
                    "openai",
                ),
            })
            .collect();

        let tool_choice = if tools.is_empty() {
            None
        } else {
            Some(serde_json::json!("auto"))
        };

        let responses_request = ResponsesRequest {
            model: api_model.to_string(),
            instructions,
            input,
            reasoning,
            max_output_tokens: Some(request.max_tokens),
            temperature: Some(request.temperature),
            tools,
            tool_choice,
            stream: false,
        };

        let url = format!("{}/responses", self.base_url.trim_end_matches('/'));
        debug!(url = %url, "Sending OpenAI Responses API request");

        let mut req_builder = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .json(&responses_request);

        if !self.api_key.as_str().is_empty() {
            req_builder =
                req_builder.header("authorization", format!("Bearer {}", self.api_key.as_str()));
        }

        let resp = req_builder
            .send()
            .await
            .map_err(|e| LlmError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(LlmError::Api {
                status,
                message: body,
            });
        }

        let body = resp
            .text()
            .await
            .map_err(|e| LlmError::Http(e.to_string()))?;
        let responses: ResponsesResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::Parse(e.to_string()))?;

        let mut content = Vec::new();
        let mut tool_calls = Vec::new();

        for (idx, item) in responses.output.into_iter().enumerate() {
            match item.item_type.as_str() {
                "message" => {
                    let text = item
                        .content
                        .into_iter()
                        .filter(|part| part.item_type == "output_text")
                        .filter_map(|part| part.text)
                        .collect::<Vec<_>>()
                        .join("");
                    if !text.is_empty() {
                        content.push(ContentBlock::Text { text });
                    }
                }
                "output_text" => {
                    if let Some(text) = item.text {
                        if !text.is_empty() {
                            content.push(ContentBlock::Text { text });
                        }
                    }
                }
                "function_call" => {
                    let id = item
                        .call_id
                        .or(item.id)
                        .unwrap_or_else(|| format!("responses_tool_{idx}"));
                    let name = item.name.unwrap_or_else(|| "tool".to_string());
                    let input: serde_json::Value = item
                        .arguments
                        .as_deref()
                        .and_then(|v| serde_json::from_str(v).ok())
                        .unwrap_or_default();

                    content.push(ContentBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    });
                    tool_calls.push(ToolCall { id, name, input });
                }
                _ => {}
            }
        }

        let stop_reason = if tool_calls.is_empty() {
            StopReason::EndTurn
        } else {
            StopReason::ToolUse
        };

        let usage = responses
            .usage
            .map(|u| TokenUsage {
                input_tokens: u.input_tokens.unwrap_or(0),
                output_tokens: u.output_tokens.unwrap_or(0),
            })
            .unwrap_or_default();

        Ok(CompletionResponse {
            content,
            stop_reason,
            tool_calls,
            usage,
        })
    }
}

#[derive(Debug, Serialize)]
struct OaiRequest {
    model: String,
    messages: Vec<OaiMessage>,
    /// Classic token limit field (used by most models).
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    /// New token limit field required by GPT-5 and o-series reasoning models.
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<u32>,
    temperature: f32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OaiTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

/// Returns true if a model uses `max_completion_tokens` instead of `max_tokens`.
fn uses_completion_tokens(model: &str) -> bool {
    let m = model.to_lowercase();
    m.starts_with("gpt-5")
        || m.starts_with("gpt5")
        || m.starts_with("o1")
        || m.starts_with("o3")
        || m.starts_with("o4")
}

/// Some providers expose reasoning-only models behind the Responses API.
/// We route known models to `/responses` while keeping chat/completions unchanged.
fn should_use_responses_api(model: &str) -> bool {
    let m = model.to_lowercase();
    m.starts_with("gpt-5.3-codex")
}

fn resolve_model_and_format(model: &str) -> (String, bool) {
    let trimmed = model.trim();
    if let Some(actual_model) = trimmed.strip_prefix("openai-responses/") {
        return (actual_model.to_string(), true);
    }
    (trimmed.to_string(), should_use_responses_api(trimmed))
}

fn reasoning_effort_from_model(model: &str) -> Option<String> {
    let m = model.to_lowercase();
    if m.contains("xhigh") {
        return Some("xhigh".to_string());
    }
    if m.ends_with("-high") {
        return Some("high".to_string());
    }
    if m.ends_with("-medium") {
        return Some("medium".to_string());
    }
    if m.ends_with("-low") {
        return Some("low".to_string());
    }
    None
}

fn configured_reasoning_effort(model: &str) -> Option<String> {
    std::env::var("OPENAI_REASONING_EFFORT")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(|| reasoning_effort_from_model(model))
}

#[derive(Debug, Serialize)]
struct OaiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<OaiMessageContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OaiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

/// Content can be a plain string or an array of content parts (for images).
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum OaiMessageContent {
    Text(String),
    Parts(Vec<OaiContentPart>),
}

/// A content part for multi-modal messages.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum OaiContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: OaiImageUrl },
}

#[derive(Debug, Serialize)]
struct OaiImageUrl {
    url: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct OaiToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OaiFunction,
}

#[derive(Debug, Serialize, Deserialize)]
struct OaiFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct OaiTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OaiToolDef,
}

#[derive(Debug, Serialize)]
struct OaiToolDef {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct OaiResponse {
    choices: Vec<OaiChoice>,
    usage: Option<OaiUsage>,
}

#[derive(Debug, Deserialize)]
struct OaiChoice {
    message: OaiResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OaiResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OaiToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OaiUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
}

#[derive(Debug, Serialize)]
struct ResponsesRequest {
    model: String,
    instructions: String,
    input: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<ResponsesReasoning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ResponsesTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

#[derive(Debug, Serialize)]
struct ResponsesReasoning {
    effort: String,
}

#[derive(Debug, Serialize)]
struct ResponsesTool {
    #[serde(rename = "type")]
    tool_type: String,
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct ResponsesResponse {
    #[serde(default)]
    output: Vec<ResponsesOutputItem>,
    #[serde(default)]
    usage: Option<ResponsesUsage>,
}

#[derive(Debug, Deserialize)]
struct ResponsesOutputItem {
    #[serde(rename = "type")]
    item_type: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    call_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    content: Vec<ResponsesContentItem>,
}

#[derive(Debug, Deserialize)]
struct ResponsesContentItem {
    #[serde(rename = "type")]
    item_type: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResponsesUsage {
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
}

#[async_trait]
impl LlmDriver for OpenAIDriver {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let (api_model, use_responses) = resolve_model_and_format(&request.model);
        if use_responses {
            return self.complete_via_responses(&request, &api_model).await;
        }

        let mut oai_messages: Vec<OaiMessage> = Vec::new();

        // Add system message if present
        if let Some(ref system) = request.system {
            oai_messages.push(OaiMessage {
                role: "system".to_string(),
                content: Some(OaiMessageContent::Text(system.clone())),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        // Convert messages
        for msg in &request.messages {
            match (&msg.role, &msg.content) {
                (Role::System, MessageContent::Text(text)) => {
                    if request.system.is_none() {
                        oai_messages.push(OaiMessage {
                            role: "system".to_string(),
                            content: Some(OaiMessageContent::Text(text.clone())),
                            tool_calls: None,
                            tool_call_id: None,
                        });
                    }
                }
                (Role::User, MessageContent::Text(text)) => {
                    oai_messages.push(OaiMessage {
                        role: "user".to_string(),
                        content: Some(OaiMessageContent::Text(text.clone())),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }
                (Role::Assistant, MessageContent::Text(text)) => {
                    oai_messages.push(OaiMessage {
                        role: "assistant".to_string(),
                        content: Some(OaiMessageContent::Text(text.clone())),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }
                (Role::User, MessageContent::Blocks(blocks)) => {
                    // Handle tool results and images in user messages
                    let mut parts: Vec<OaiContentPart> = Vec::new();
                    let mut has_tool_results = false;
                    for block in blocks {
                        match block {
                            ContentBlock::ToolResult {
                                tool_use_id,
                                content,
                                ..
                            } => {
                                has_tool_results = true;
                                oai_messages.push(OaiMessage {
                                    role: "tool".to_string(),
                                    content: Some(OaiMessageContent::Text(content.clone())),
                                    tool_calls: None,
                                    tool_call_id: Some(tool_use_id.clone()),
                                });
                            }
                            ContentBlock::Text { text } => {
                                parts.push(OaiContentPart::Text { text: text.clone() });
                            }
                            ContentBlock::Image { media_type, data } => {
                                parts.push(OaiContentPart::ImageUrl {
                                    image_url: OaiImageUrl {
                                        url: format!("data:{media_type};base64,{data}"),
                                    },
                                });
                            }
                            ContentBlock::Thinking { .. } => {}
                            _ => {}
                        }
                    }
                    if !parts.is_empty() && !has_tool_results {
                        oai_messages.push(OaiMessage {
                            role: "user".to_string(),
                            content: Some(OaiMessageContent::Parts(parts)),
                            tool_calls: None,
                            tool_call_id: None,
                        });
                    }
                }
                (Role::Assistant, MessageContent::Blocks(blocks)) => {
                    let mut text_parts = Vec::new();
                    let mut tool_calls = Vec::new();
                    for block in blocks {
                        match block {
                            ContentBlock::Text { text } => text_parts.push(text.clone()),
                            ContentBlock::ToolUse { id, name, input } => {
                                tool_calls.push(OaiToolCall {
                                    id: id.clone(),
                                    call_type: "function".to_string(),
                                    function: OaiFunction {
                                        name: name.clone(),
                                        arguments: serde_json::to_string(input).unwrap_or_default(),
                                    },
                                });
                            }
                            ContentBlock::Thinking { .. } => {}
                            _ => {}
                        }
                    }
                    oai_messages.push(OaiMessage {
                        role: "assistant".to_string(),
                        content: if text_parts.is_empty() {
                            None
                        } else {
                            Some(OaiMessageContent::Text(text_parts.join("")))
                        },
                        tool_calls: if tool_calls.is_empty() {
                            None
                        } else {
                            Some(tool_calls)
                        },
                        tool_call_id: None,
                    });
                }
                _ => {}
            }
        }

        let oai_tools: Vec<OaiTool> = request
            .tools
            .iter()
            .map(|t| OaiTool {
                tool_type: "function".to_string(),
                function: OaiToolDef {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: openfang_types::tool::normalize_schema_for_provider(
                        &t.input_schema,
                        "openai",
                    ),
                },
            })
            .collect();

        let tool_choice = if oai_tools.is_empty() {
            None
        } else {
            Some(serde_json::json!("auto"))
        };

        let (mt, mct) = if uses_completion_tokens(&api_model) {
            (None, Some(request.max_tokens))
        } else {
            (Some(request.max_tokens), None)
        };
        let mut oai_request = OaiRequest {
            model: api_model,
            messages: oai_messages,
            max_tokens: mt,
            max_completion_tokens: mct,
            temperature: request.temperature,
            tools: oai_tools,
            tool_choice,
            stream: false,
        };

        let max_retries = 3;
        for attempt in 0..=max_retries {
            let url = format!("{}/chat/completions", self.base_url);
            debug!(url = %url, attempt, "Sending OpenAI API request");

            let mut req_builder = self
                .client
                .post(&url)
                .header("content-type", "application/json")
                .json(&oai_request);

            if !self.api_key.as_str().is_empty() {
                req_builder = req_builder
                    .header("authorization", format!("Bearer {}", self.api_key.as_str()));
            }

            let resp = req_builder
                .send()
                .await
                .map_err(|e| LlmError::Http(e.to_string()))?;

            let status = resp.status().as_u16();
            if status == 429 {
                if attempt < max_retries {
                    let retry_ms = (attempt + 1) as u64 * 2000;
                    warn!(status, retry_ms, "Rate limited, retrying");
                    tokio::time::sleep(std::time::Duration::from_millis(retry_ms)).await;
                    continue;
                }
                return Err(LlmError::RateLimited {
                    retry_after_ms: 5000,
                });
            }

            if !resp.status().is_success() {
                let body = resp.text().await.unwrap_or_default();

                // Groq "tool_use_failed": model generated tool call in XML format.
                // Parse the failed_generation and convert to a proper tool call response.
                if status == 400 && body.contains("tool_use_failed") {
                    if let Some(response) = parse_groq_failed_tool_call(&body) {
                        warn!("Recovered tool call from Groq failed_generation");
                        return Ok(response);
                    }
                    // If parsing fails, retry on next attempt
                    if attempt < max_retries {
                        let retry_ms = (attempt + 1) as u64 * 1500;
                        warn!(status, attempt, retry_ms, "tool_use_failed, retrying");
                        tokio::time::sleep(std::time::Duration::from_millis(retry_ms)).await;
                        continue;
                    }
                }

                // GPT-5 / o-series: switch from max_tokens to max_completion_tokens
                if status == 400
                    && body.contains("max_tokens")
                    && (body.contains("unsupported_parameter")
                        || body.contains("max_completion_tokens"))
                    && oai_request.max_tokens.is_some()
                    && attempt < max_retries
                {
                    let val = oai_request.max_tokens.unwrap();
                    warn!(model = %oai_request.model, "Switching to max_completion_tokens for this model");
                    oai_request.max_tokens = None;
                    oai_request.max_completion_tokens = Some(val);
                    continue;
                }

                // Auto-cap max_tokens when model rejects our value (e.g. Groq Maverick limit 8192)
                if status == 400 && body.contains("max_tokens") && attempt < max_retries {
                    let current = oai_request.max_tokens.or(oai_request.max_completion_tokens).unwrap_or(4096);
                    let cap = extract_max_tokens_limit(&body).unwrap_or(current / 2);
                    warn!(old = current, new = cap, "Auto-capping max_tokens to model limit");
                    if oai_request.max_completion_tokens.is_some() {
                        oai_request.max_completion_tokens = Some(cap);
                    } else {
                        oai_request.max_tokens = Some(cap);
                    }
                    continue;
                }

                return Err(LlmError::Api {
                    status,
                    message: body,
                });
            }

            let body = resp
                .text()
                .await
                .map_err(|e| LlmError::Http(e.to_string()))?;
            let oai_response: OaiResponse =
                serde_json::from_str(&body).map_err(|e| LlmError::Parse(e.to_string()))?;

            let choice = oai_response
                .choices
                .into_iter()
                .next()
                .ok_or_else(|| LlmError::Parse("No choices in response".to_string()))?;

            let mut content = Vec::new();
            let mut tool_calls = Vec::new();

            if let Some(text) = choice.message.content {
                if !text.is_empty() {
                    content.push(ContentBlock::Text { text });
                }
            }

            if let Some(calls) = choice.message.tool_calls {
                for call in calls {
                    let input: serde_json::Value =
                        serde_json::from_str(&call.function.arguments).unwrap_or_default();
                    content.push(ContentBlock::ToolUse {
                        id: call.id.clone(),
                        name: call.function.name.clone(),
                        input: input.clone(),
                    });
                    tool_calls.push(ToolCall {
                        id: call.id,
                        name: call.function.name,
                        input,
                    });
                }
            }

            let stop_reason = match choice.finish_reason.as_deref() {
                Some("stop") => StopReason::EndTurn,
                Some("tool_calls") => StopReason::ToolUse,
                Some("length") => StopReason::MaxTokens,
                _ => {
                    if !tool_calls.is_empty() {
                        StopReason::ToolUse
                    } else {
                        StopReason::EndTurn
                    }
                }
            };

            let usage = oai_response
                .usage
                .map(|u| TokenUsage {
                    input_tokens: u.prompt_tokens,
                    output_tokens: u.completion_tokens,
                })
                .unwrap_or_default();

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

    async fn stream(
        &self,
        request: CompletionRequest,
        tx: tokio::sync::mpsc::Sender<StreamEvent>,
    ) -> Result<CompletionResponse, LlmError> {
        let (api_model, use_responses) = resolve_model_and_format(&request.model);
        if use_responses {
            let response = self.complete_via_responses(&request, &api_model).await?;
            let text = response.text();
            if !text.is_empty() {
                let _ = tx
                    .send(StreamEvent::TextDelta {
                        text: text.to_string(),
                    })
                    .await;
            }
            let _ = tx
                .send(StreamEvent::ContentComplete {
                    stop_reason: response.stop_reason,
                    usage: response.usage,
                })
                .await;
            return Ok(response);
        }

        // Build request (same as complete but with stream: true)
        let mut oai_messages: Vec<OaiMessage> = Vec::new();

        if let Some(ref system) = request.system {
            oai_messages.push(OaiMessage {
                role: "system".to_string(),
                content: Some(OaiMessageContent::Text(system.clone())),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        for msg in &request.messages {
            match (&msg.role, &msg.content) {
                (Role::System, MessageContent::Text(text)) => {
                    if request.system.is_none() {
                        oai_messages.push(OaiMessage {
                            role: "system".to_string(),
                            content: Some(OaiMessageContent::Text(text.clone())),
                            tool_calls: None,
                            tool_call_id: None,
                        });
                    }
                }
                (Role::User, MessageContent::Text(text)) => {
                    oai_messages.push(OaiMessage {
                        role: "user".to_string(),
                        content: Some(OaiMessageContent::Text(text.clone())),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }
                (Role::Assistant, MessageContent::Text(text)) => {
                    oai_messages.push(OaiMessage {
                        role: "assistant".to_string(),
                        content: Some(OaiMessageContent::Text(text.clone())),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }
                (Role::User, MessageContent::Blocks(blocks)) => {
                    for block in blocks {
                        if let ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            ..
                        } = block
                        {
                            oai_messages.push(OaiMessage {
                                role: "tool".to_string(),
                                content: Some(OaiMessageContent::Text(content.clone())),
                                tool_calls: None,
                                tool_call_id: Some(tool_use_id.clone()),
                            });
                        }
                    }
                }
                (Role::Assistant, MessageContent::Blocks(blocks)) => {
                    let mut text_parts = Vec::new();
                    let mut tool_calls_out = Vec::new();
                    for block in blocks {
                        match block {
                            ContentBlock::Text { text } => text_parts.push(text.clone()),
                            ContentBlock::ToolUse { id, name, input } => {
                                tool_calls_out.push(OaiToolCall {
                                    id: id.clone(),
                                    call_type: "function".to_string(),
                                    function: OaiFunction {
                                        name: name.clone(),
                                        arguments: serde_json::to_string(input).unwrap_or_default(),
                                    },
                                });
                            }
                            ContentBlock::Thinking { .. } => {}
                            _ => {}
                        }
                    }
                    oai_messages.push(OaiMessage {
                        role: "assistant".to_string(),
                        content: if text_parts.is_empty() {
                            None
                        } else {
                            Some(OaiMessageContent::Text(text_parts.join("")))
                        },
                        tool_calls: if tool_calls_out.is_empty() {
                            None
                        } else {
                            Some(tool_calls_out)
                        },
                        tool_call_id: None,
                    });
                }
                _ => {}
            }
        }

        let oai_tools: Vec<OaiTool> = request
            .tools
            .iter()
            .map(|t| OaiTool {
                tool_type: "function".to_string(),
                function: OaiToolDef {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: openfang_types::tool::normalize_schema_for_provider(
                        &t.input_schema,
                        "openai",
                    ),
                },
            })
            .collect();

        let tool_choice = if oai_tools.is_empty() {
            None
        } else {
            Some(serde_json::json!("auto"))
        };

        let (mt, mct) = if uses_completion_tokens(&api_model) {
            (None, Some(request.max_tokens))
        } else {
            (Some(request.max_tokens), None)
        };
        let mut oai_request = OaiRequest {
            model: api_model,
            messages: oai_messages,
            max_tokens: mt,
            max_completion_tokens: mct,
            temperature: request.temperature,
            tools: oai_tools,
            tool_choice,
            stream: true,
        };

        // Retry loop for the initial HTTP request
        let max_retries = 3;
        for attempt in 0..=max_retries {
            let url = format!("{}/chat/completions", self.base_url);
            debug!(url = %url, attempt, "Sending OpenAI streaming request");

            let mut req_builder = self
                .client
                .post(&url)
                .header("content-type", "application/json")
                .json(&oai_request);

            if !self.api_key.as_str().is_empty() {
                req_builder = req_builder
                    .header("authorization", format!("Bearer {}", self.api_key.as_str()));
            }

            let resp = req_builder
                .send()
                .await
                .map_err(|e| LlmError::Http(e.to_string()))?;

            let status = resp.status().as_u16();
            if status == 429 {
                if attempt < max_retries {
                    let retry_ms = (attempt + 1) as u64 * 2000;
                    warn!(status, retry_ms, "Rate limited (stream), retrying");
                    tokio::time::sleep(std::time::Duration::from_millis(retry_ms)).await;
                    continue;
                }
                return Err(LlmError::RateLimited {
                    retry_after_ms: 5000,
                });
            }

            if !resp.status().is_success() {
                let body = resp.text().await.unwrap_or_default();

                // Groq "tool_use_failed": parse and recover (streaming path)
                if status == 400 && body.contains("tool_use_failed") {
                    if let Some(response) = parse_groq_failed_tool_call(&body) {
                        warn!("Recovered tool call from Groq failed_generation (stream)");
                        return Ok(response);
                    }
                    if attempt < max_retries {
                        let retry_ms = (attempt + 1) as u64 * 1500;
                        warn!(
                            status,
                            attempt, retry_ms, "tool_use_failed (stream), retrying"
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(retry_ms)).await;
                        continue;
                    }
                }

                // GPT-5 / o-series: switch from max_tokens to max_completion_tokens
                if status == 400
                    && body.contains("max_tokens")
                    && (body.contains("unsupported_parameter")
                        || body.contains("max_completion_tokens"))
                    && oai_request.max_tokens.is_some()
                    && attempt < max_retries
                {
                    let val = oai_request.max_tokens.unwrap();
                    warn!(model = %oai_request.model, "Switching to max_completion_tokens for this model (stream)");
                    oai_request.max_tokens = None;
                    oai_request.max_completion_tokens = Some(val);
                    continue;
                }

                // Auto-cap max_tokens when model rejects our value
                if status == 400 && body.contains("max_tokens") && attempt < max_retries {
                    let current = oai_request.max_tokens.or(oai_request.max_completion_tokens).unwrap_or(4096);
                    let cap = extract_max_tokens_limit(&body).unwrap_or(current / 2);
                    warn!(old = current, new = cap, "Auto-capping max_tokens (stream)");
                    if oai_request.max_completion_tokens.is_some() {
                        oai_request.max_completion_tokens = Some(cap);
                    } else {
                        oai_request.max_tokens = Some(cap);
                    }
                    continue;
                }

                return Err(LlmError::Api {
                    status,
                    message: body,
                });
            }

            // Parse the SSE stream
            let mut buffer = String::new();
            let mut text_content = String::new();
            // Track tool calls: index -> (id, name, arguments)
            let mut tool_accum: Vec<(String, String, String)> = Vec::new();
            let mut finish_reason: Option<String> = None;
            let mut usage = TokenUsage::default();

            let mut byte_stream = resp.bytes_stream();
            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = chunk_result.map_err(|e| LlmError::Http(e.to_string()))?;
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // Process complete lines
                while let Some(pos) = buffer.find('\n') {
                    let line = buffer[..pos].trim_end().to_string();
                    buffer = buffer[pos + 1..].to_string();

                    if line.is_empty() || line.starts_with(':') {
                        continue;
                    }

                    let data = match line.strip_prefix("data:") {
                        Some(d) => d.trim_start(),
                        None => continue,
                    };

                    if data == "[DONE]" {
                        continue;
                    }

                    let json: serde_json::Value = match serde_json::from_str(data) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    // Extract usage if present (some providers send it in the last chunk)
                    if let Some(u) = json.get("usage") {
                        if let Some(pt) = u["prompt_tokens"].as_u64() {
                            usage.input_tokens = pt;
                        }
                        if let Some(ct) = u["completion_tokens"].as_u64() {
                            usage.output_tokens = ct;
                        }
                    }

                    let choices = match json["choices"].as_array() {
                        Some(c) => c,
                        None => continue,
                    };

                    for choice in choices {
                        let delta = &choice["delta"];

                        // Text content delta
                        if let Some(text) = delta["content"].as_str() {
                            if !text.is_empty() {
                                text_content.push_str(text);
                                let _ = tx
                                    .send(StreamEvent::TextDelta {
                                        text: text.to_string(),
                                    })
                                    .await;
                            }
                        }

                        // Tool call deltas
                        if let Some(calls) = delta["tool_calls"].as_array() {
                            for call in calls {
                                let idx = call["index"].as_u64().unwrap_or(0) as usize;

                                // Ensure tool_accum has enough entries
                                while tool_accum.len() <= idx {
                                    tool_accum.push((String::new(), String::new(), String::new()));
                                }

                                // ID (sent in first chunk for this tool)
                                if let Some(id) = call["id"].as_str() {
                                    tool_accum[idx].0 = id.to_string();
                                }

                                if let Some(func) = call.get("function") {
                                    // Name (sent in first chunk)
                                    if let Some(name) = func["name"].as_str() {
                                        tool_accum[idx].1 = name.to_string();
                                        let _ = tx
                                            .send(StreamEvent::ToolUseStart {
                                                id: tool_accum[idx].0.clone(),
                                                name: name.to_string(),
                                            })
                                            .await;
                                    }

                                    // Arguments delta
                                    if let Some(args) = func["arguments"].as_str() {
                                        tool_accum[idx].2.push_str(args);
                                        if !args.is_empty() {
                                            let _ = tx
                                                .send(StreamEvent::ToolInputDelta {
                                                    text: args.to_string(),
                                                })
                                                .await;
                                        }
                                    }
                                }
                            }
                        }

                        // Finish reason
                        if let Some(fr) = choice["finish_reason"].as_str() {
                            finish_reason = Some(fr.to_string());
                        }
                    }
                }
            }

            // Build the final response
            let mut content = Vec::new();
            let mut tool_calls = Vec::new();

            if !text_content.is_empty() {
                content.push(ContentBlock::Text { text: text_content });
            }

            for (id, name, arguments) in &tool_accum {
                let input: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
                content.push(ContentBlock::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                });
                tool_calls.push(ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    input,
                });

                let _ = tx
                    .send(StreamEvent::ToolUseEnd {
                        id: id.clone(),
                        name: name.clone(),
                        input: serde_json::from_str(arguments).unwrap_or_default(),
                    })
                    .await;
            }

            let stop_reason = match finish_reason.as_deref() {
                Some("stop") => StopReason::EndTurn,
                Some("tool_calls") => StopReason::ToolUse,
                Some("length") => StopReason::MaxTokens,
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

/// Parse Groq's `tool_use_failed` error and extract the tool call from `failed_generation`.
/// Extract the max_tokens limit from an API error message.
/// Looks for patterns like: `must be less than or equal to \`8192\``
fn extract_max_tokens_limit(body: &str) -> Option<u32> {
    // Pattern: "must be <= `N`" or "must be less than or equal to `N`"
    let patterns = [
        "less than or equal to `",
        "must be <= `",
        "maximum value for `max_tokens` is `",
    ];
    for pat in &patterns {
        if let Some(idx) = body.find(pat) {
            let after = &body[idx + pat.len()..];
            let end = after
                .find('`')
                .or_else(|| after.find('"'))
                .unwrap_or(after.len());
            if let Ok(n) = after[..end].trim().parse::<u32>() {
                return Some(n);
            }
        }
    }
    None
}

///
/// Some models (e.g. Llama 3.3) generate tool calls as XML: `<function=NAME ARGS></function>`
/// instead of the proper JSON format. Groq rejects these with `tool_use_failed` but includes
/// the raw generation. We parse it and construct a proper CompletionResponse.
fn parse_groq_failed_tool_call(body: &str) -> Option<CompletionResponse> {
    let json_body: serde_json::Value = serde_json::from_str(body).ok()?;
    let failed = json_body
        .pointer("/error/failed_generation")
        .and_then(|v| v.as_str())?;

    // Parse all tool calls from the failed generation.
    // Format: <function=tool_name{"arg":"val"}></function> or <function=tool_name {"arg":"val"}></function>
    let mut tool_calls = Vec::new();
    let mut remaining = failed;

    while let Some(start) = remaining.find("<function=") {
        remaining = &remaining[start + 10..]; // skip "<function="
                                              // Find the end tag
        let end = remaining.find("</function>")?;
        let mut call_content = &remaining[..end];
        remaining = &remaining[end + 11..]; // skip "</function>"

        // Strip trailing ">" from the XML opening tag close
        call_content = call_content.strip_suffix('>').unwrap_or(call_content);

        // Split into name and args: "tool_name{"arg":"val"}" or "tool_name {"arg":"val"}"
        let (name, args) = if let Some(brace_pos) = call_content.find('{') {
            let name = call_content[..brace_pos].trim();
            let args = &call_content[brace_pos..];
            (name, args)
        } else {
            // No args — just a tool name
            (call_content.trim(), "{}")
        };

        // Parse args as JSON Value
        let args_value: serde_json::Value =
            serde_json::from_str(args).unwrap_or(serde_json::json!({}));

        tool_calls.push(ToolCall {
            id: format!("groq_recovered_{}", tool_calls.len()),
            name: name.to_string(),
            input: args_value,
        });
    }

    if tool_calls.is_empty() {
        // No tool calls found — the model generated plain text but Groq rejected it.
        // Return it as a normal text response instead of failing.
        if !failed.trim().is_empty() {
            warn!("Recovering plain text from Groq failed_generation (no tool calls)");
            return Some(CompletionResponse {
                content: vec![ContentBlock::Text {
                    text: failed.to_string(),
                }],
                tool_calls: vec![],
                stop_reason: StopReason::EndTurn,
                usage: TokenUsage {
                    input_tokens: 0,
                    output_tokens: 0,
                },
            });
        }
        return None;
    }

    Some(CompletionResponse {
        content: vec![],
        tool_calls,
        stop_reason: StopReason::ToolUse,
        usage: TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_driver_creation() {
        let driver = OpenAIDriver::new("test-key".to_string(), "http://localhost".to_string());
        assert_eq!(driver.api_key.as_str(), "test-key");
    }

    #[test]
    fn test_parse_groq_failed_tool_call() {
        let body = r#"{"error":{"message":"Failed to call a function.","type":"invalid_request_error","code":"tool_use_failed","failed_generation":"<function=web_fetch{\"url\": \"https://example.com\"}></function>\n"}}"#;
        let result = parse_groq_failed_tool_call(body);
        assert!(result.is_some());
        let resp = result.unwrap();
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].name, "web_fetch");
        assert!(resp.tool_calls[0]
            .input
            .to_string()
            .contains("https://example.com"));
    }

    #[test]
    fn test_parse_groq_failed_tool_call_with_space() {
        let body = r#"{"error":{"message":"Failed","type":"invalid_request_error","code":"tool_use_failed","failed_generation":"<function=shell_exec {\"command\": \"ls -la\"}></function>"}}"#;
        let result = parse_groq_failed_tool_call(body);
        assert!(result.is_some());
        let resp = result.unwrap();
        assert_eq!(resp.tool_calls[0].name, "shell_exec");
    }

    #[test]
    fn test_should_use_responses_api_for_gpt53_codex() {
        assert!(should_use_responses_api("gpt-5.3-codex"));
        assert!(should_use_responses_api("gpt-5.3-codex-high"));
        assert!(!should_use_responses_api("gpt-5.2"));
    }

    #[test]
    fn test_resolve_model_and_format_with_explicit_prefix() {
        let (model, use_responses) = resolve_model_and_format("openai-responses/gpt-5.1");
        assert_eq!(model, "gpt-5.1");
        assert!(use_responses);
    }

    #[test]
    fn test_reasoning_effort_from_model_suffix() {
        assert_eq!(
            reasoning_effort_from_model("gpt-5.3-codex-xhigh").as_deref(),
            Some("xhigh")
        );
        assert_eq!(
            reasoning_effort_from_model("gpt-5.3-codex-high").as_deref(),
            Some("high")
        );
        assert_eq!(reasoning_effort_from_model("gpt-5.3-codex"), None);
    }
}
