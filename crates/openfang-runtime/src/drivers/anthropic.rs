//! Anthropic Claude API driver.
//!
//! Full implementation of the Anthropic Messages API with tool use support,
//! system prompt extraction, and retry on 429/529 errors.

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

/// Anthropic Claude API driver.
pub struct AnthropicDriver {
    api_key: Zeroizing<String>,
    base_url: String,
    client: reqwest::Client,
}

impl AnthropicDriver {
    /// Create a new Anthropic driver.
    pub fn new(api_key: String, base_url: String) -> Self {
        Self {
            api_key: Zeroizing::new(api_key),
            base_url,
            client: reqwest::Client::new(),
        }
    }
}

/// Anthropic Messages API request body.
#[derive(Debug, Serialize)]
struct ApiRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<ApiMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ApiTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

#[derive(Debug, Serialize)]
struct ApiMessage {
    role: String,
    content: ApiContent,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ApiContent {
    Text(String),
    Blocks(Vec<ApiContentBlock>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ApiContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source: ApiImageSource },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        is_error: bool,
    },
}

#[derive(Debug, Serialize)]
struct ApiImageSource {
    #[serde(rename = "type")]
    source_type: String,
    media_type: String,
    data: String,
}

#[derive(Debug, Serialize)]
struct ApiTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

/// Anthropic Messages API response body.
#[derive(Debug, Deserialize)]
struct ApiResponse {
    content: Vec<ResponseContentBlock>,
    stop_reason: String,
    usage: ApiUsage,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ResponseContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
}

#[derive(Debug, Deserialize)]
struct ApiUsage {
    input_tokens: u64,
    output_tokens: u64,
}

/// Anthropic API error response.
#[derive(Debug, Deserialize)]
struct ApiErrorResponse {
    error: ApiErrorDetail,
}

#[derive(Debug, Deserialize)]
struct ApiErrorDetail {
    message: String,
}

/// Accumulator for content blocks during streaming.
enum ContentBlockAccum {
    Text(String),
    Thinking(String),
    ToolUse {
        id: String,
        name: String,
        input_json: String,
    },
}

#[async_trait]
impl LlmDriver for AnthropicDriver {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        // Extract system prompt from messages or use the provided one
        let system = request.system.clone().or_else(|| {
            request.messages.iter().find_map(|m| {
                if m.role == Role::System {
                    match &m.content {
                        MessageContent::Text(t) => Some(t.clone()),
                        _ => None,
                    }
                } else {
                    None
                }
            })
        });

        // Build API messages, filtering out system messages
        let api_messages: Vec<ApiMessage> = request
            .messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(convert_message)
            .collect();

        // Build tools
        let api_tools: Vec<ApiTool> = request
            .tools
            .iter()
            .map(|t| ApiTool {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.input_schema.clone(),
            })
            .collect();

        let api_request = ApiRequest {
            model: request.model.clone(),
            max_tokens: request.max_tokens,
            system,
            messages: api_messages,
            tools: api_tools,
            temperature: Some(request.temperature),
            stream: false,
        };

        // Retry loop for rate limits and overloads
        let max_retries = 3;
        for attempt in 0..=max_retries {
            let url = format!("{}/v1/messages", self.base_url);
            debug!(url = %url, attempt, "Sending Anthropic API request");

            let resp = self
                .client
                .post(&url)
                .header("x-api-key", self.api_key.as_str())
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&api_request)
                .send()
                .await
                .map_err(|e| LlmError::Http(e.to_string()))?;

            let status = resp.status().as_u16();

            if status == 429 || status == 529 {
                if attempt < max_retries {
                    let retry_ms = (attempt + 1) as u64 * 2000;
                    warn!(status, retry_ms, "Rate limited, retrying");
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
                let message = serde_json::from_str::<ApiErrorResponse>(&body)
                    .map(|e| e.error.message)
                    .unwrap_or(body);
                return Err(LlmError::Api { status, message });
            }

            let body = resp
                .text()
                .await
                .map_err(|e| LlmError::Http(e.to_string()))?;
            let api_response: ApiResponse =
                serde_json::from_str(&body).map_err(|e| LlmError::Parse(e.to_string()))?;

            return Ok(convert_response(api_response));
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
        // Build request (same as complete but with stream: true)
        let system = request.system.clone().or_else(|| {
            request.messages.iter().find_map(|m| {
                if m.role == Role::System {
                    match &m.content {
                        MessageContent::Text(t) => Some(t.clone()),
                        _ => None,
                    }
                } else {
                    None
                }
            })
        });

        let api_messages: Vec<ApiMessage> = request
            .messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(convert_message)
            .collect();

        let api_tools: Vec<ApiTool> = request
            .tools
            .iter()
            .map(|t| ApiTool {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.input_schema.clone(),
            })
            .collect();

        let api_request = ApiRequest {
            model: request.model.clone(),
            max_tokens: request.max_tokens,
            system,
            messages: api_messages,
            tools: api_tools,
            temperature: Some(request.temperature),
            stream: true,
        };

        // Retry loop for the initial HTTP request
        let max_retries = 3;
        for attempt in 0..=max_retries {
            let url = format!("{}/v1/messages", self.base_url);
            debug!(url = %url, attempt, "Sending Anthropic streaming request");

            let resp = self
                .client
                .post(&url)
                .header("x-api-key", self.api_key.as_str())
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&api_request)
                .send()
                .await
                .map_err(|e| LlmError::Http(e.to_string()))?;

            let status = resp.status().as_u16();

            if status == 429 || status == 529 {
                if attempt < max_retries {
                    let retry_ms = (attempt + 1) as u64 * 2000;
                    warn!(status, retry_ms, "Rate limited (stream), retrying");
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
                let message = serde_json::from_str::<ApiErrorResponse>(&body)
                    .map(|e| e.error.message)
                    .unwrap_or(body);
                return Err(LlmError::Api { status, message });
            }

            // Parse the SSE stream
            let mut buffer = String::new();
            let mut blocks: Vec<ContentBlockAccum> = Vec::new();
            let mut stop_reason = StopReason::EndTurn;
            let mut usage = TokenUsage::default();

            let mut byte_stream = resp.bytes_stream();
            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = chunk_result.map_err(|e| LlmError::Http(e.to_string()))?;
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(pos) = buffer.find("\n\n") {
                    let event_text = buffer[..pos].to_string();
                    buffer = buffer[pos + 2..].to_string();

                    let mut event_type = String::new();
                    let mut data = String::new();
                    for line in event_text.lines() {
                        if let Some(et) = line.strip_prefix("event: ") {
                            event_type = et.to_string();
                        } else if let Some(d) = line.strip_prefix("data: ") {
                            data = d.to_string();
                        }
                    }

                    if data.is_empty() {
                        continue;
                    }

                    let json: serde_json::Value = match serde_json::from_str(&data) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    match event_type.as_str() {
                        "message_start" => {
                            if let Some(it) = json["message"]["usage"]["input_tokens"].as_u64() {
                                usage.input_tokens = it;
                            }
                        }
                        "content_block_start" => {
                            let block = &json["content_block"];
                            match block["type"].as_str().unwrap_or("") {
                                "text" => {
                                    blocks.push(ContentBlockAccum::Text(String::new()));
                                }
                                "tool_use" => {
                                    let id = block["id"].as_str().unwrap_or("").to_string();
                                    let name = block["name"].as_str().unwrap_or("").to_string();
                                    let _ = tx
                                        .send(StreamEvent::ToolUseStart {
                                            id: id.clone(),
                                            name: name.clone(),
                                        })
                                        .await;
                                    blocks.push(ContentBlockAccum::ToolUse {
                                        id,
                                        name,
                                        input_json: String::new(),
                                    });
                                }
                                "thinking" => {
                                    blocks.push(ContentBlockAccum::Thinking(String::new()));
                                }
                                _ => {}
                            }
                        }
                        "content_block_delta" => {
                            let delta = &json["delta"];
                            match delta["type"].as_str().unwrap_or("") {
                                "text_delta" => {
                                    if let Some(text) = delta["text"].as_str() {
                                        if let Some(ContentBlockAccum::Text(ref mut t)) =
                                            blocks.last_mut()
                                        {
                                            t.push_str(text);
                                        }
                                        let _ = tx
                                            .send(StreamEvent::TextDelta {
                                                text: text.to_string(),
                                            })
                                            .await;
                                    }
                                }
                                "input_json_delta" => {
                                    if let Some(partial) = delta["partial_json"].as_str() {
                                        if let Some(ContentBlockAccum::ToolUse {
                                            ref mut input_json,
                                            ..
                                        }) = blocks.last_mut()
                                        {
                                            input_json.push_str(partial);
                                        }
                                        let _ = tx
                                            .send(StreamEvent::ToolInputDelta {
                                                text: partial.to_string(),
                                            })
                                            .await;
                                    }
                                }
                                "thinking_delta" => {
                                    if let Some(thinking) = delta["thinking"].as_str() {
                                        if let Some(ContentBlockAccum::Thinking(ref mut t)) =
                                            blocks.last_mut()
                                        {
                                            t.push_str(thinking);
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        "content_block_stop" => {
                            if let Some(ContentBlockAccum::ToolUse {
                                id,
                                name,
                                input_json,
                            }) = blocks.last()
                            {
                                let input: serde_json::Value =
                                    serde_json::from_str(input_json).unwrap_or_default();
                                let _ = tx
                                    .send(StreamEvent::ToolUseEnd {
                                        id: id.clone(),
                                        name: name.clone(),
                                        input,
                                    })
                                    .await;
                            }
                        }
                        "message_delta" => {
                            if let Some(sr) = json["delta"]["stop_reason"].as_str() {
                                stop_reason = match sr {
                                    "end_turn" => StopReason::EndTurn,
                                    "tool_use" => StopReason::ToolUse,
                                    "max_tokens" => StopReason::MaxTokens,
                                    "stop_sequence" => StopReason::StopSequence,
                                    _ => StopReason::EndTurn,
                                };
                            }
                            if let Some(ot) = json["usage"]["output_tokens"].as_u64() {
                                usage.output_tokens = ot;
                            }
                        }
                        _ => {} // message_stop, ping, etc.
                    }
                }
            }

            // Build CompletionResponse from accumulated blocks
            let mut content = Vec::new();
            let mut tool_calls = Vec::new();
            for block in blocks {
                match block {
                    ContentBlockAccum::Text(text) => {
                        content.push(ContentBlock::Text { text });
                    }
                    ContentBlockAccum::Thinking(thinking) => {
                        content.push(ContentBlock::Thinking { thinking });
                    }
                    ContentBlockAccum::ToolUse {
                        id,
                        name,
                        input_json,
                    } => {
                        let input: serde_json::Value =
                            serde_json::from_str(&input_json).unwrap_or_default();
                        content.push(ContentBlock::ToolUse {
                            id: id.clone(),
                            name: name.clone(),
                            input: input.clone(),
                        });
                        tool_calls.push(ToolCall { id, name, input });
                    }
                }
            }

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

/// Convert an OpenFang Message to an Anthropic API message.
fn convert_message(msg: &Message) -> ApiMessage {
    let role = match msg.role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "user", // Should be filtered out, but handle gracefully
    };

    let content = match &msg.content {
        MessageContent::Text(text) => ApiContent::Text(text.clone()),
        MessageContent::Blocks(blocks) => {
            let api_blocks: Vec<ApiContentBlock> = blocks
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => {
                        Some(ApiContentBlock::Text { text: text.clone() })
                    }
                    ContentBlock::Image { media_type, data } => Some(ApiContentBlock::Image {
                        source: ApiImageSource {
                            source_type: "base64".to_string(),
                            media_type: media_type.clone(),
                            data: data.clone(),
                        },
                    }),
                    ContentBlock::ToolUse { id, name, input } => Some(ApiContentBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    }),
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                        ..
                    } => Some(ApiContentBlock::ToolResult {
                        tool_use_id: tool_use_id.clone(),
                        content: content.clone(),
                        is_error: *is_error,
                    }),
                    ContentBlock::Thinking { .. } => None,
                    ContentBlock::Unknown => None,
                })
                .collect();
            ApiContent::Blocks(api_blocks)
        }
    };

    ApiMessage {
        role: role.to_string(),
        content,
    }
}

/// Convert an Anthropic API response to our CompletionResponse.
fn convert_response(api: ApiResponse) -> CompletionResponse {
    let mut content = Vec::new();
    let mut tool_calls = Vec::new();

    for block in api.content {
        match block {
            ResponseContentBlock::Text { text } => {
                content.push(ContentBlock::Text { text });
            }
            ResponseContentBlock::ToolUse { id, name, input } => {
                content.push(ContentBlock::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                });
                tool_calls.push(ToolCall { id, name, input });
            }
            ResponseContentBlock::Thinking { thinking } => {
                content.push(ContentBlock::Thinking { thinking });
            }
        }
    }

    let stop_reason = match api.stop_reason.as_str() {
        "end_turn" => StopReason::EndTurn,
        "tool_use" => StopReason::ToolUse,
        "max_tokens" => StopReason::MaxTokens,
        "stop_sequence" => StopReason::StopSequence,
        _ => StopReason::EndTurn,
    };

    CompletionResponse {
        content,
        stop_reason,
        tool_calls,
        usage: TokenUsage {
            input_tokens: api.usage.input_tokens,
            output_tokens: api.usage.output_tokens,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_message_text() {
        let msg = Message::user("Hello");
        let api_msg = convert_message(&msg);
        assert_eq!(api_msg.role, "user");
    }

    #[test]
    fn test_convert_response() {
        let api_response = ApiResponse {
            content: vec![
                ResponseContentBlock::Text {
                    text: "I'll help you.".to_string(),
                },
                ResponseContentBlock::ToolUse {
                    id: "tool_1".to_string(),
                    name: "web_search".to_string(),
                    input: serde_json::json!({"query": "rust lang"}),
                },
            ],
            stop_reason: "tool_use".to_string(),
            usage: ApiUsage {
                input_tokens: 100,
                output_tokens: 50,
            },
        };

        let response = convert_response(api_response);
        assert_eq!(response.stop_reason, StopReason::ToolUse);
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].name, "web_search");
        assert_eq!(response.usage.total(), 150);
    }
}
