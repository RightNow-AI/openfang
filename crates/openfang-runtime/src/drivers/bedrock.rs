//! AWS Bedrock driver using the Converse API.
//!
//! Supports all Bedrock models (Anthropic Claude, Amazon Nova, Meta Llama, Mistral, etc.)
//! via the unified Converse API with AWS Signature Version 4 authentication.

use crate::llm_driver::{CompletionRequest, CompletionResponse, LlmDriver, LlmError, StreamEvent};
use async_trait::async_trait;
use futures::StreamExt;
use hmac::{Hmac, Mac};
use openfang_types::message::{
    ContentBlock, Message, MessageContent, Role, StopReason, TokenUsage,
};
use openfang_types::tool::ToolCall;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{debug, warn};
use zeroize::Zeroizing;

type HmacSha256 = Hmac<Sha256>;

/// AWS Bedrock Converse API driver.
pub struct BedrockDriver {
    access_key_id: Zeroizing<String>,
    secret_access_key: Zeroizing<String>,
    session_token: Option<Zeroizing<String>>,
    region: String,
    base_url: String,
    client: reqwest::Client,
}

impl BedrockDriver {
    /// Create a new Bedrock driver.
    ///
    /// `base_url` should be like `https://bedrock-runtime.us-east-1.amazonaws.com`.
    /// The region is extracted from the URL, or falls back to `AWS_REGION` / `us-east-1`.
    pub fn new(
        access_key_id: String,
        secret_access_key: String,
        session_token: Option<String>,
        base_url: String,
    ) -> Self {
        let region = extract_region(&base_url)
            .or_else(|| std::env::var("AWS_REGION").ok())
            .or_else(|| std::env::var("AWS_DEFAULT_REGION").ok())
            .unwrap_or_else(|| "us-east-1".to_string());

        Self {
            access_key_id: Zeroizing::new(access_key_id),
            secret_access_key: Zeroizing::new(secret_access_key),
            session_token: session_token.map(Zeroizing::new),
            region,
            base_url,
            client: reqwest::Client::new(),
        }
    }

    /// Build and sign a request to the Converse API.
    fn build_converse_url(&self, model_id: &str) -> String {
        format!("{}/model/{}/converse", self.base_url, model_id)
    }

    /// Build and sign a request to the ConverseStream API.
    fn build_converse_stream_url(&self, model_id: &str) -> String {
        format!("{}/model/{}/converse-stream", self.base_url, model_id)
    }

    /// Sign an HTTP request using AWS Signature Version 4.
    fn sign_request(
        &self,
        method: &str,
        url: &str,
        headers: &[(&str, &str)],
        payload: &[u8],
    ) -> Result<Vec<(String, String)>, LlmError> {
        let now = chrono::Utc::now();
        let date_stamp = now.format("%Y%m%d").to_string();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();

        // Parse URL components (without url crate dependency)
        let without_scheme = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .ok_or_else(|| LlmError::Http("URL must start with http:// or https://".to_string()))?;
        let (host_and_path, canonical_querystring) = without_scheme
            .split_once('?')
            .unwrap_or((without_scheme, ""));
        let (host, canonical_uri) = host_and_path
            .split_once('/')
            .map(|(h, p)| (h, format!("/{}", p)))
            .unwrap_or((host_and_path, "/".to_string()));
        let canonical_uri = &canonical_uri;

        // Payload hash
        let payload_hash = hex_sha256(payload);

        // Build signed headers
        let mut all_headers: Vec<(String, String)> = headers
            .iter()
            .map(|(k, v)| (k.to_lowercase(), v.to_string()))
            .collect();
        all_headers.push(("host".to_string(), host.to_string()));
        all_headers.push(("x-amz-date".to_string(), amz_date.clone()));
        all_headers.push((
            "x-amz-content-sha256".to_string(),
            payload_hash.clone(),
        ));
        if let Some(ref token) = self.session_token {
            all_headers.push(("x-amz-security-token".to_string(), token.as_str().to_string()));
        }

        all_headers.sort_by(|a, b| a.0.cmp(&b.0));

        let signed_headers: String = all_headers
            .iter()
            .map(|(k, _)| k.as_str())
            .collect::<Vec<_>>()
            .join(";");

        let canonical_headers: String = all_headers
            .iter()
            .map(|(k, v)| format!("{}:{}\n", k, v.trim()))
            .collect();

        // Canonical request
        let canonical_request = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            method,
            canonical_uri,
            canonical_querystring,
            canonical_headers,
            signed_headers,
            payload_hash,
        );

        let credential_scope = format!("{}/{}/bedrock/aws4_request", date_stamp, self.region);

        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{}\n{}",
            amz_date,
            credential_scope,
            hex_sha256(canonical_request.as_bytes()),
        );

        // Derive signing key
        let k_date = hmac_sha256(
            format!("AWS4{}", self.secret_access_key.as_str()).as_bytes(),
            date_stamp.as_bytes(),
        );
        let k_region = hmac_sha256(&k_date, self.region.as_bytes());
        let k_service = hmac_sha256(&k_region, b"bedrock");
        let k_signing = hmac_sha256(&k_service, b"aws4_request");

        let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes()));

        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            self.access_key_id.as_str(),
            credential_scope,
            signed_headers,
            signature,
        );

        // Return headers to add to the request
        let mut result_headers = vec![
            ("Authorization".to_string(), authorization),
            ("x-amz-date".to_string(), amz_date),
            ("x-amz-content-sha256".to_string(), payload_hash),
        ];
        if let Some(ref token) = self.session_token {
            result_headers.push((
                "x-amz-security-token".to_string(),
                token.as_str().to_string(),
            ));
        }

        Ok(result_headers)
    }
}

// ── Converse API request types ──────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConverseRequest {
    messages: Vec<ConverseMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<Vec<SystemBlock>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    inference_config: Option<InferenceConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_config: Option<ToolConfig>,
}

#[derive(Debug, Serialize)]
struct SystemBlock {
    text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InferenceConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Serialize)]
struct ToolConfig {
    tools: Vec<ConverseTool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConverseTool {
    tool_spec: ConverseToolSpec,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConverseToolSpec {
    name: String,
    description: String,
    input_schema: ConverseInputSchema,
}

#[derive(Debug, Serialize)]
struct ConverseInputSchema {
    json: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct ConverseMessage {
    role: String,
    content: Vec<ConverseContentBlock>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ConverseContentBlock {
    Text {
        text: String,
    },
    Image {
        image: ConverseImage,
    },
    #[serde(rename_all = "camelCase")]
    ToolUse {
        #[serde(rename = "toolUse")]
        tool_use: ConverseToolUse,
    },
    #[serde(rename_all = "camelCase")]
    ToolResult {
        #[serde(rename = "toolResult")]
        tool_result: ConverseToolResult,
    },
}

#[derive(Debug, Serialize)]
struct ConverseImage {
    format: String,
    source: ConverseImageSource,
}

#[derive(Debug, Serialize)]
struct ConverseImageSource {
    bytes: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConverseToolUse {
    tool_use_id: String,
    name: String,
    input: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConverseToolResult {
    tool_use_id: String,
    content: Vec<ConverseToolResultContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
}

#[derive(Debug, Serialize)]
struct ConverseToolResultContent {
    text: String,
}

// ── Converse API response types ─────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConverseResponse {
    output: ConverseOutput,
    stop_reason: String,
    usage: ConverseUsage,
}

#[derive(Debug, Deserialize)]
struct ConverseOutput {
    message: ConverseResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ConverseResponseMessage {
    content: Vec<ConverseResponseContent>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ConverseResponseContent {
    Text {
        text: String,
    },
    ToolUse {
        #[serde(rename = "toolUse")]
        tool_use: ConverseResponseToolUse,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConverseResponseToolUse {
    tool_use_id: String,
    name: String,
    input: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConverseUsage {
    input_tokens: u64,
    output_tokens: u64,
}

/// Bedrock API error response.
#[derive(Debug, Deserialize)]
struct BedrockErrorResponse {
    message: Option<String>,
    #[serde(rename = "Message")]
    message_upper: Option<String>,
}

impl BedrockErrorResponse {
    fn error_message(&self) -> String {
        self.message
            .clone()
            .or_else(|| self.message_upper.clone())
            .unwrap_or_else(|| "Unknown error".to_string())
    }
}

#[async_trait]
impl LlmDriver for BedrockDriver {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let model_id = normalize_model_id(&request.model);

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

        let converse_messages: Vec<ConverseMessage> = request
            .messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(convert_message)
            .collect();

        let tool_config = if request.tools.is_empty() {
            None
        } else {
            Some(ToolConfig {
                tools: request
                    .tools
                    .iter()
                    .map(|t| ConverseTool {
                        tool_spec: ConverseToolSpec {
                            name: t.name.clone(),
                            description: t.description.clone(),
                            input_schema: ConverseInputSchema {
                                json: t.input_schema.clone(),
                            },
                        },
                    })
                    .collect(),
            })
        };

        let converse_request = ConverseRequest {
            messages: converse_messages,
            system: system.map(|s| vec![SystemBlock { text: s }]),
            inference_config: Some(InferenceConfig {
                max_tokens: Some(request.max_tokens),
                temperature: Some(request.temperature),
            }),
            tool_config,
        };

        let payload =
            serde_json::to_vec(&converse_request).map_err(|e| LlmError::Parse(e.to_string()))?;

        let url = self.build_converse_url(&model_id);

        // Retry loop for rate limits and throttling
        let max_retries = 3;
        for attempt in 0..=max_retries {
            debug!(url = %url, attempt, model = %model_id, "Sending Bedrock Converse request");

            let content_type_header = [("content-type", "application/json")];
            let auth_headers = self.sign_request("POST", &url, &content_type_header, &payload)?;

            let mut req = self
                .client
                .post(&url)
                .header("content-type", "application/json")
                .body(payload.clone());

            for (key, value) in &auth_headers {
                req = req.header(key.as_str(), value.as_str());
            }

            let resp = req.send().await.map_err(|e| LlmError::Http(e.to_string()))?;
            let status = resp.status().as_u16();

            if status == 429 || status == 529 {
                if attempt < max_retries {
                    let retry_ms = (attempt + 1) as u64 * 2000;
                    warn!(status, retry_ms, "Bedrock throttled, retrying");
                    tokio::time::sleep(std::time::Duration::from_millis(retry_ms)).await;
                    continue;
                }
                return Err(LlmError::RateLimited {
                    retry_after_ms: 5000,
                });
            }

            if !resp.status().is_success() {
                let body = resp.text().await.unwrap_or_default();
                let message = serde_json::from_str::<BedrockErrorResponse>(&body)
                    .map(|e| e.error_message())
                    .unwrap_or(body);
                return Err(LlmError::Api { status, message });
            }

            let body = resp
                .text()
                .await
                .map_err(|e| LlmError::Http(e.to_string()))?;
            let converse_response: ConverseResponse =
                serde_json::from_str(&body).map_err(|e| LlmError::Parse(e.to_string()))?;

            return Ok(convert_response(converse_response));
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
        let model_id = normalize_model_id(&request.model);

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

        let converse_messages: Vec<ConverseMessage> = request
            .messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(convert_message)
            .collect();

        let tool_config = if request.tools.is_empty() {
            None
        } else {
            Some(ToolConfig {
                tools: request
                    .tools
                    .iter()
                    .map(|t| ConverseTool {
                        tool_spec: ConverseToolSpec {
                            name: t.name.clone(),
                            description: t.description.clone(),
                            input_schema: ConverseInputSchema {
                                json: t.input_schema.clone(),
                            },
                        },
                    })
                    .collect(),
            })
        };

        let converse_request = ConverseRequest {
            messages: converse_messages,
            system: system.map(|s| vec![SystemBlock { text: s }]),
            inference_config: Some(InferenceConfig {
                max_tokens: Some(request.max_tokens),
                temperature: Some(request.temperature),
            }),
            tool_config,
        };

        let payload =
            serde_json::to_vec(&converse_request).map_err(|e| LlmError::Parse(e.to_string()))?;

        let url = self.build_converse_stream_url(&model_id);

        let max_retries = 3;
        for attempt in 0..=max_retries {
            debug!(url = %url, attempt, model = %model_id, "Sending Bedrock ConverseStream request");

            let content_type_header = [("content-type", "application/json")];
            let auth_headers = self.sign_request("POST", &url, &content_type_header, &payload)?;

            let mut req = self
                .client
                .post(&url)
                .header("content-type", "application/json")
                .body(payload.clone());

            for (key, value) in &auth_headers {
                req = req.header(key.as_str(), value.as_str());
            }

            let resp = req.send().await.map_err(|e| LlmError::Http(e.to_string()))?;
            let status = resp.status().as_u16();

            if status == 429 || status == 529 {
                if attempt < max_retries {
                    let retry_ms = (attempt + 1) as u64 * 2000;
                    warn!(status, retry_ms, "Bedrock stream throttled, retrying");
                    tokio::time::sleep(std::time::Duration::from_millis(retry_ms)).await;
                    continue;
                }
                return Err(LlmError::RateLimited {
                    retry_after_ms: 5000,
                });
            }

            if !resp.status().is_success() {
                let body = resp.text().await.unwrap_or_default();
                let message = serde_json::from_str::<BedrockErrorResponse>(&body)
                    .map(|e| e.error_message())
                    .unwrap_or(body);
                return Err(LlmError::Api { status, message });
            }

            // Parse Bedrock event stream.
            // Bedrock ConverseStream uses AWS event stream binary format with JSON payloads.
            // Each event is framed with headers indicating the event type.
            // We parse the raw bytes looking for JSON event payloads.
            let mut content: Vec<ContentBlock> = Vec::new();
            let mut tool_calls: Vec<ToolCall> = Vec::new();
            let mut stop_reason = StopReason::EndTurn;
            let mut usage = TokenUsage::default();

            // Current tool accumulator
            let mut current_tool_id = String::new();
            let mut current_tool_name = String::new();
            let mut current_tool_input_json = String::new();
            let mut current_text = String::new();
            let mut in_tool = false;

            let mut buffer = Vec::new();
            let mut byte_stream = resp.bytes_stream();

            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = chunk_result.map_err(|e| LlmError::Http(e.to_string()))?;
                buffer.extend_from_slice(&chunk);

                // Try to extract JSON event payloads from the buffer.
                // Bedrock event stream binary framing: we scan for JSON objects.
                while let Some((json_val, consumed)) = try_extract_json_event(&buffer) {
                    buffer = buffer[consumed..].to_vec();

                    // Process event based on its content
                    if let Some(delta) = json_val.get("contentBlockDelta") {
                        if let Some(text) = delta.get("delta").and_then(|d| d.get("text")).and_then(|t| t.as_str()) {
                            current_text.push_str(text);
                            let _ = tx
                                .send(StreamEvent::TextDelta {
                                    text: text.to_string(),
                                })
                                .await;
                        }
                        if let Some(input) = delta.get("delta").and_then(|d| d.get("toolUse")).and_then(|t| t.get("input")).and_then(|i| i.as_str()) {
                            current_tool_input_json.push_str(input);
                            let _ = tx
                                .send(StreamEvent::ToolInputDelta {
                                    text: input.to_string(),
                                })
                                .await;
                        }
                    } else if let Some(start) = json_val.get("contentBlockStart") {
                        if let Some(tool_use) = start.get("start").and_then(|s| s.get("toolUse")) {
                            let id = tool_use.get("toolUseId").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let name = tool_use.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            // Flush any accumulated text
                            if !current_text.is_empty() {
                                content.push(ContentBlock::Text {
                                    text: std::mem::take(&mut current_text),
                                });
                            }
                            in_tool = true;
                            current_tool_id = id.clone();
                            current_tool_name = name.clone();
                            current_tool_input_json.clear();
                            let _ = tx
                                .send(StreamEvent::ToolUseStart { id, name })
                                .await;
                        }
                    } else if json_val.get("contentBlockStop").is_some() {
                        if in_tool {
                            let input: serde_json::Value =
                                serde_json::from_str(&current_tool_input_json).unwrap_or_default();
                            let _ = tx
                                .send(StreamEvent::ToolUseEnd {
                                    id: current_tool_id.clone(),
                                    name: current_tool_name.clone(),
                                    input: input.clone(),
                                })
                                .await;
                            content.push(ContentBlock::ToolUse {
                                id: current_tool_id.clone(),
                                name: current_tool_name.clone(),
                                input: input.clone(),
                            });
                            tool_calls.push(ToolCall {
                                id: std::mem::take(&mut current_tool_id),
                                name: std::mem::take(&mut current_tool_name),
                                input,
                            });
                            current_tool_input_json.clear();
                            in_tool = false;
                        }
                    } else if let Some(metadata) = json_val.get("metadata") {
                        if let Some(u) = metadata.get("usage") {
                            usage.input_tokens = u.get("inputTokens").and_then(|v| v.as_u64()).unwrap_or(0);
                            usage.output_tokens = u.get("outputTokens").and_then(|v| v.as_u64()).unwrap_or(0);
                        }
                    } else if let Some(stop) = json_val.get("messageStop") {
                        if let Some(sr) = stop.get("stopReason").and_then(|v| v.as_str()) {
                            stop_reason = parse_stop_reason(sr);
                        }
                    }
                }
            }

            // Flush remaining text
            if !current_text.is_empty() {
                content.push(ContentBlock::Text {
                    text: current_text,
                });
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

// ── Helpers ─────────────────────────────────────────────────────────

/// Strip the `bedrock/` prefix from model IDs if present.
fn normalize_model_id(model: &str) -> String {
    model
        .strip_prefix("bedrock/")
        .unwrap_or(model)
        .to_string()
}

/// Extract the AWS region from a Bedrock runtime URL.
fn extract_region(base_url: &str) -> Option<String> {
    // URL format: https://bedrock-runtime.{region}.amazonaws.com
    let host = base_url
        .strip_prefix("https://")
        .or_else(|| base_url.strip_prefix("http://"))?;
    let host = host.split('/').next()?;
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() >= 3 && parts[0] == "bedrock-runtime" {
        Some(parts[1].to_string())
    } else {
        None
    }
}

/// Parse Bedrock stop reason string to our StopReason enum.
fn parse_stop_reason(s: &str) -> StopReason {
    match s {
        "end_turn" => StopReason::EndTurn,
        "tool_use" => StopReason::ToolUse,
        "max_tokens" => StopReason::MaxTokens,
        "stop_sequence" => StopReason::StopSequence,
        _ => StopReason::EndTurn,
    }
}

/// Convert an OpenFang Message to a Converse API message.
fn convert_message(msg: &Message) -> ConverseMessage {
    let role = match msg.role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "user",
    };

    let content = match &msg.content {
        MessageContent::Text(text) => vec![ConverseContentBlock::Text {
            text: text.clone(),
        }],
        MessageContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(ConverseContentBlock::Text {
                    text: text.clone(),
                }),
                ContentBlock::Image { media_type, data } => {
                    let format = media_type
                        .strip_prefix("image/")
                        .unwrap_or("png")
                        .to_string();
                    Some(ConverseContentBlock::Image {
                        image: ConverseImage {
                            format,
                            source: ConverseImageSource {
                                bytes: data.clone(),
                            },
                        },
                    })
                }
                ContentBlock::ToolUse { id, name, input } => {
                    Some(ConverseContentBlock::ToolUse {
                        tool_use: ConverseToolUse {
                            tool_use_id: id.clone(),
                            name: name.clone(),
                            input: input.clone(),
                        },
                    })
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                    ..
                } => Some(ConverseContentBlock::ToolResult {
                    tool_result: ConverseToolResult {
                        tool_use_id: tool_use_id.clone(),
                        content: vec![ConverseToolResultContent {
                            text: content.clone(),
                        }],
                        status: if *is_error {
                            Some("error".to_string())
                        } else {
                            None
                        },
                    },
                }),
                ContentBlock::Thinking { .. } | ContentBlock::Unknown => None,
            })
            .collect(),
    };

    ConverseMessage {
        role: role.to_string(),
        content,
    }
}

/// Convert a Converse API response to our CompletionResponse.
fn convert_response(resp: ConverseResponse) -> CompletionResponse {
    let mut content = Vec::new();
    let mut tool_calls = Vec::new();

    for block in resp.output.message.content {
        match block {
            ConverseResponseContent::Text { text } => {
                content.push(ContentBlock::Text { text });
            }
            ConverseResponseContent::ToolUse { tool_use } => {
                content.push(ContentBlock::ToolUse {
                    id: tool_use.tool_use_id.clone(),
                    name: tool_use.name.clone(),
                    input: tool_use.input.clone(),
                });
                tool_calls.push(ToolCall {
                    id: tool_use.tool_use_id,
                    name: tool_use.name,
                    input: tool_use.input,
                });
            }
        }
    }

    CompletionResponse {
        content,
        stop_reason: parse_stop_reason(&resp.stop_reason),
        tool_calls,
        usage: TokenUsage {
            input_tokens: resp.usage.input_tokens,
            output_tokens: resp.usage.output_tokens,
        },
    }
}

/// Try to extract a JSON event from the Bedrock event stream buffer.
/// Returns the parsed JSON value and the number of bytes consumed, or None.
fn try_extract_json_event(buffer: &[u8]) -> Option<(serde_json::Value, usize)> {
    // Bedrock event stream uses AWS binary event framing.
    // Events have a prelude (8 bytes: total length + headers length),
    // then headers, then a JSON payload, then a CRC.
    // We need at least 16 bytes for the prelude + trailer CRCs.
    if buffer.len() < 16 {
        return None;
    }

    // Total byte length of the message (including prelude and CRCs)
    let total_length = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;
    let headers_length =
        u32::from_be_bytes([buffer[4], buffer[5], buffer[6], buffer[7]]) as usize;

    if buffer.len() < total_length {
        return None; // Not enough data yet
    }

    // Prelude: 8 bytes, prelude CRC: 4 bytes, headers, payload, message CRC: 4 bytes
    let headers_start = 12; // after prelude (8) + prelude CRC (4)
    let payload_start = headers_start + headers_length;
    let payload_end = total_length - 4; // before message CRC

    if payload_start >= payload_end || payload_end > buffer.len() {
        // No payload or invalid frame — skip this event
        return Some((serde_json::Value::Null, total_length));
    }

    let payload_bytes = &buffer[payload_start..payload_end];

    match serde_json::from_slice::<serde_json::Value>(payload_bytes) {
        Ok(val) if !val.is_null() => Some((val, total_length)),
        _ => Some((serde_json::Value::Null, total_length)),
    }
}

/// Compute HMAC-SHA256.
fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac =
        HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// Compute hex-encoded SHA256.
fn hex_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_model_id() {
        assert_eq!(
            normalize_model_id("bedrock/anthropic.claude-sonnet-4-6"),
            "anthropic.claude-sonnet-4-6"
        );
        assert_eq!(
            normalize_model_id("amazon.nova-pro-v1:0"),
            "amazon.nova-pro-v1:0"
        );
    }

    #[test]
    fn test_extract_region() {
        assert_eq!(
            extract_region("https://bedrock-runtime.us-east-1.amazonaws.com"),
            Some("us-east-1".to_string())
        );
        assert_eq!(
            extract_region("https://bedrock-runtime.eu-west-1.amazonaws.com"),
            Some("eu-west-1".to_string())
        );
        assert_eq!(
            extract_region("https://bedrock-runtime.ap-northeast-1.amazonaws.com"),
            Some("ap-northeast-1".to_string())
        );
        assert_eq!(extract_region("https://example.com"), None);
    }

    #[test]
    fn test_parse_stop_reason() {
        assert_eq!(parse_stop_reason("end_turn"), StopReason::EndTurn);
        assert_eq!(parse_stop_reason("tool_use"), StopReason::ToolUse);
        assert_eq!(parse_stop_reason("max_tokens"), StopReason::MaxTokens);
        assert_eq!(parse_stop_reason("stop_sequence"), StopReason::StopSequence);
        assert_eq!(parse_stop_reason("unknown"), StopReason::EndTurn);
    }

    #[test]
    fn test_convert_message_text() {
        let msg = Message::user("Hello world");
        let converse_msg = convert_message(&msg);
        assert_eq!(converse_msg.role, "user");
        assert_eq!(converse_msg.content.len(), 1);
    }

    #[test]
    fn test_convert_response() {
        let resp = ConverseResponse {
            output: ConverseOutput {
                message: ConverseResponseMessage {
                    content: vec![
                        ConverseResponseContent::Text {
                            text: "Hello!".to_string(),
                        },
                        ConverseResponseContent::ToolUse {
                            tool_use: ConverseResponseToolUse {
                                tool_use_id: "t1".to_string(),
                                name: "web_search".to_string(),
                                input: serde_json::json!({"query": "rust"}),
                            },
                        },
                    ],
                },
            },
            stop_reason: "tool_use".to_string(),
            usage: ConverseUsage {
                input_tokens: 100,
                output_tokens: 50,
            },
        };

        let result = super::convert_response(resp);
        assert_eq!(result.stop_reason, StopReason::ToolUse);
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].name, "web_search");
        assert_eq!(result.usage.input_tokens, 100);
        assert_eq!(result.usage.output_tokens, 50);
    }

    #[test]
    fn test_hex_sha256() {
        let hash = hex_sha256(b"");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_hmac_sha256() {
        let result = hmac_sha256(b"key", b"data");
        assert_eq!(result.len(), 32);
    }

    #[test]
    fn test_sigv4_signing() {
        let driver = BedrockDriver::new(
            "AKIAIOSFODNN7EXAMPLE".to_string(),
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            None,
            "https://bedrock-runtime.us-east-1.amazonaws.com".to_string(),
        );
        assert_eq!(driver.region, "us-east-1");

        let headers = driver
            .sign_request(
                "POST",
                "https://bedrock-runtime.us-east-1.amazonaws.com/model/test/converse",
                &[("content-type", "application/json")],
                b"{}",
            )
            .unwrap();

        // Should have Authorization, x-amz-date, x-amz-content-sha256
        let header_names: Vec<&str> = headers.iter().map(|(k, _)| k.as_str()).collect();
        assert!(header_names.contains(&"Authorization"));
        assert!(header_names.contains(&"x-amz-date"));
        assert!(header_names.contains(&"x-amz-content-sha256"));

        let auth = headers
            .iter()
            .find(|(k, _)| k == "Authorization")
            .unwrap();
        assert!(auth.1.starts_with("AWS4-HMAC-SHA256"));
        assert!(auth.1.contains("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn test_sigv4_with_session_token() {
        let driver = BedrockDriver::new(
            "AKID".to_string(),
            "SECRET".to_string(),
            Some("SESSION_TOKEN".to_string()),
            "https://bedrock-runtime.us-west-2.amazonaws.com".to_string(),
        );

        let headers = driver
            .sign_request(
                "POST",
                "https://bedrock-runtime.us-west-2.amazonaws.com/model/test/converse",
                &[("content-type", "application/json")],
                b"{}",
            )
            .unwrap();

        let header_names: Vec<&str> = headers.iter().map(|(k, _)| k.as_str()).collect();
        assert!(header_names.contains(&"x-amz-security-token"));
    }

    #[test]
    fn test_try_extract_json_event_too_short() {
        assert!(try_extract_json_event(&[0; 10]).is_none());
    }

    #[test]
    fn test_converse_request_serialization() {
        let req = ConverseRequest {
            messages: vec![ConverseMessage {
                role: "user".to_string(),
                content: vec![ConverseContentBlock::Text {
                    text: "Hello".to_string(),
                }],
            }],
            system: Some(vec![SystemBlock {
                text: "You are helpful".to_string(),
            }]),
            inference_config: Some(InferenceConfig {
                max_tokens: Some(1024),
                temperature: Some(0.7),
            }),
            tool_config: None,
        };

        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("messages").is_some());
        assert!(json.get("system").is_some());
        assert!(json.get("inferenceConfig").is_some());
        assert!(json.get("toolConfig").is_none());
    }
}
