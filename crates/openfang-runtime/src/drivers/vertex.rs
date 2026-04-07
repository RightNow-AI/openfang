//! Google Vertex AI driver with OAuth authentication.
//!
//! Uses service account credentials (`GOOGLE_APPLICATION_CREDENTIALS`) to
//! authenticate with Vertex AI's Gemini models via OAuth 2.0 bearer tokens.
//! This enables enterprise deployments without requiring consumer API keys.
//!
//! # Endpoint Format
//!
//! ```text
//! https://{region}-aiplatform.googleapis.com/v1/projects/{project}/locations/{region}/publishers/google/models/{model}:generateContent
//! ```
//!
//! # Authentication
//!
//! Uses OAuth 2.0 bearer tokens obtained via `gcloud auth print-access-token`.
//! Tokens are cached for 50 minutes and automatically refreshed.
//!
//! # Environment Variables
//!
//! - `GOOGLE_APPLICATION_CREDENTIALS` — Path to service account JSON
//! - `GOOGLE_CLOUD_PROJECT` / `GCLOUD_PROJECT` / `GCP_PROJECT` — Project ID (optional if in credentials)
//! - `GOOGLE_CLOUD_REGION` / `VERTEX_AI_REGION` — Region (default: `us-central1`)
//! - `VERTEX_AI_ACCESS_TOKEN` — Pre-generated token (optional, for testing)

use crate::llm_driver::{CompletionRequest, CompletionResponse, LlmDriver, LlmError, StreamEvent};
use async_trait::async_trait;
use futures::StreamExt;
use openfang_types::message::{
    ContentBlock, Message, MessageContent, Role, StopReason, TokenUsage,
};
use openfang_types::tool::ToolCall;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use zeroize::Zeroizing;

/// Vertex AI driver with OAuth authentication.
///
/// Authenticates using GCP service account credentials and OAuth 2.0 bearer tokens.
/// Tokens are cached with automatic refresh before expiry.
pub struct VertexAIDriver {
    project_id: String,
    region: String,
    /// Cached OAuth access token (zeroized on drop for security).
    token_cache: Arc<RwLock<TokenCache>>,
    client: reqwest::Client,
}

/// Cached OAuth token with expiry tracking.
///
/// SECURITY: Token is wrapped in `Zeroizing` to clear memory on drop.
struct TokenCache {
    token: Option<Zeroizing<String>>,
    expires_at: Option<Instant>,
}

impl TokenCache {
    fn new() -> Self {
        Self {
            token: None,
            expires_at: None,
        }
    }

    fn is_valid(&self) -> bool {
        match (&self.token, &self.expires_at) {
            (Some(_), Some(expires)) => Instant::now() < *expires,
            _ => false,
        }
    }

    fn get(&self) -> Option<String> {
        if self.is_valid() {
            self.token.as_ref().map(|t| t.as_str().to_string())
        } else {
            None
        }
    }
}

impl VertexAIDriver {
    /// Create a new Vertex AI driver.
    ///
    /// # Arguments
    /// * `project_id` - GCP project ID
    /// * `region` - GCP region (e.g., `us-central1`)
    pub fn new(project_id: String, region: String) -> Self {
        Self {
            project_id,
            region,
            token_cache: Arc::new(RwLock::new(TokenCache::new())),
            client: reqwest::Client::new(),
        }
    }

    /// Get a valid OAuth access token, refreshing if needed.
    async fn get_access_token(&self) -> Result<String, LlmError> {
        // Check cache first
        {
            let cache = self.token_cache.read().await;
            if let Some(token) = cache.get() {
                debug!("Using cached Vertex AI access token");
                return Ok(token);
            }
        }

        // Need to refresh token
        info!("Refreshing Vertex AI OAuth access token");
        let token = self.fetch_access_token().await?;

        // Cache the token (expires in ~1 hour, we refresh at 50 min)
        {
            let mut cache = self.token_cache.write().await;
            cache.token = Some(Zeroizing::new(token.clone()));
            cache.expires_at = Some(Instant::now() + Duration::from_secs(50 * 60));
        }

        Ok(token)
    }

    /// Fetch a new access token using gcloud CLI.
    ///
    /// This uses the service account specified in GOOGLE_APPLICATION_CREDENTIALS
    /// via the gcloud CLI. For production, this should use the google-auth library.
    async fn fetch_access_token(&self) -> Result<String, LlmError> {
        // First, check if a pre-generated token is available in env
        if let Ok(token) = std::env::var("VERTEX_AI_ACCESS_TOKEN") {
            if !token.is_empty() {
                debug!("Using pre-set VERTEX_AI_ACCESS_TOKEN");
                return Ok(token);
            }
        }

        // Try application-default credentials first (uses GOOGLE_APPLICATION_CREDENTIALS)
        let output = tokio::process::Command::new("gcloud")
            .args(["auth", "application-default", "print-access-token"])
            .output()
            .await;

        if let Ok(output) = output {
            if output.status.success() {
                let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !token.is_empty() {
                    debug!("Successfully obtained Vertex AI access token via application-default");
                    return Ok(token);
                }
            }
        }

        // Fall back to regular gcloud auth (requires activated service account)
        let output = tokio::process::Command::new("gcloud")
            .args(["auth", "print-access-token"])
            .output()
            .await
            .map_err(|e| LlmError::MissingApiKey(format!("Failed to run gcloud: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(LlmError::MissingApiKey(format!(
                "gcloud auth failed: {}. Ensure GOOGLE_APPLICATION_CREDENTIALS is set and \
                 run: gcloud auth activate-service-account --key-file=$GOOGLE_APPLICATION_CREDENTIALS",
                stderr.trim()
            )));
        }

        let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if token.is_empty() {
            return Err(LlmError::MissingApiKey(
                "Empty access token from gcloud".to_string(),
            ));
        }

        debug!("Successfully obtained Vertex AI access token");
        Ok(token)
    }

    /// Build the Vertex AI endpoint URL for a model.
    fn build_endpoint(&self, model: &str, streaming: bool) -> String {
        // Strip any "gemini-" prefix duplications
        let model_name = model.strip_prefix("models/").unwrap_or(model);

        let method = if streaming {
            "streamGenerateContent"
        } else {
            "generateContent"
        };

        format!(
            "https://{region}-aiplatform.googleapis.com/v1/projects/{project}/locations/{region}/publishers/google/models/{model}:{method}",
            region = self.region,
            project = self.project_id,
            model = model_name,
            method = method
        )
    }
}

// ── Request types (reusing Gemini format) ──────────────────────────────

/// Top-level Gemini/Vertex API request body.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VertexRequest {
    contents: Vec<VertexContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<VertexContent>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<VertexToolConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenerationConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct VertexContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    parts: Vec<VertexPart>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
enum VertexPart {
    Text {
        text: String,
    },
    InlineData {
        #[serde(rename = "inlineData")]
        inline_data: VertexInlineData,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: VertexFunctionCallData,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: VertexFunctionResponseData,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct VertexInlineData {
    #[serde(rename = "mimeType")]
    mime_type: String,
    data: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct VertexFunctionCallData {
    name: String,
    args: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct VertexFunctionResponseData {
    name: String,
    response: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VertexToolConfig {
    function_declarations: Vec<VertexFunctionDeclaration>,
}

#[derive(Debug, Serialize)]
struct VertexFunctionDeclaration {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
}

// ── Response types ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VertexResponse {
    #[serde(default)]
    candidates: Vec<VertexCandidate>,
    #[serde(default)]
    usage_metadata: Option<VertexUsageMetadata>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VertexCandidate {
    content: Option<VertexContent>,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VertexUsageMetadata {
    #[serde(default)]
    prompt_token_count: u64,
    #[serde(default)]
    candidates_token_count: u64,
}

#[derive(Debug, Deserialize)]
struct VertexErrorResponse {
    error: VertexErrorDetail,
}

#[derive(Debug, Deserialize)]
struct VertexErrorDetail {
    message: String,
}

// ── Message conversion ─────────────────────────────────────────────────

fn convert_messages(
    messages: &[Message],
    system: &Option<String>,
) -> (Vec<VertexContent>, Option<VertexContent>) {
    let mut contents = Vec::new();

    let system_instruction = extract_system(messages, system);

    for msg in messages {
        if msg.role == Role::System {
            continue;
        }

        let role = match msg.role {
            Role::User => "user",
            Role::Assistant => "model",
            Role::System => continue,
        };

        let parts = match &msg.content {
            MessageContent::Text(text) => vec![VertexPart::Text { text: text.clone() }],
            MessageContent::Blocks(blocks) => {
                let mut parts = Vec::new();
                for block in blocks {
                    match block {
                        ContentBlock::Text { text, .. } => {
                            parts.push(VertexPart::Text { text: text.clone() });
                        }
                        ContentBlock::ToolUse { name, input, .. } => {
                            parts.push(VertexPart::FunctionCall {
                                function_call: VertexFunctionCallData {
                                    name: name.clone(),
                                    args: input.clone(),
                                },
                            });
                        }
                        ContentBlock::Image { media_type, data } => {
                            parts.push(VertexPart::InlineData {
                                inline_data: VertexInlineData {
                                    mime_type: media_type.clone(),
                                    data: data.clone(),
                                },
                            });
                        }
                        ContentBlock::ToolResult { content, .. } => {
                            parts.push(VertexPart::FunctionResponse {
                                function_response: VertexFunctionResponseData {
                                    name: String::new(),
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
            contents.push(VertexContent {
                role: Some(role.to_string()),
                parts,
            });
        }
    }

    (contents, system_instruction)
}

fn extract_system(messages: &[Message], system: &Option<String>) -> Option<VertexContent> {
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

    Some(VertexContent {
        role: None,
        parts: vec![VertexPart::Text { text }],
    })
}

fn convert_tools(request: &CompletionRequest) -> Vec<VertexToolConfig> {
    if request.tools.is_empty() {
        return Vec::new();
    }

    let declarations: Vec<VertexFunctionDeclaration> = request
        .tools
        .iter()
        .map(|t| {
            let normalized =
                openfang_types::tool::normalize_schema_for_provider(&t.input_schema, "gemini");
            VertexFunctionDeclaration {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: normalized,
            }
        })
        .collect();

    vec![VertexToolConfig {
        function_declarations: declarations,
    }]
}

fn convert_response(resp: VertexResponse) -> Result<CompletionResponse, LlmError> {
    let candidate = resp
        .candidates
        .into_iter()
        .next()
        .ok_or_else(|| LlmError::Parse("No candidates in Vertex AI response".to_string()))?;

    let mut content = Vec::new();
    let mut tool_calls = Vec::new();

    if let Some(vertex_content) = candidate.content {
        for part in vertex_content.parts {
            match part {
                VertexPart::Text { text } => {
                    content.push(ContentBlock::Text {
                        text,
                        provider_metadata: None,
                    });
                }
                VertexPart::FunctionCall { function_call } => {
                    tool_calls.push(ToolCall {
                        id: format!("call_{}", &uuid::Uuid::new_v4().to_string()[..8]),
                        name: function_call.name,
                        input: function_call.args,
                    });
                }
                _ => {}
            }
        }
    }

    let stop_reason = match candidate.finish_reason.as_deref() {
        Some("STOP") => StopReason::EndTurn,
        Some("MAX_TOKENS") => StopReason::MaxTokens,
        Some("SAFETY") | Some("RECITATION") | Some("BLOCKLIST") => StopReason::EndTurn,
        _ if !tool_calls.is_empty() => StopReason::ToolUse,
        _ => StopReason::EndTurn,
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
impl LlmDriver for VertexAIDriver {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let (contents, system_instruction) = convert_messages(&request.messages, &request.system);
        let tools = convert_tools(&request);

        let vertex_request = VertexRequest {
            contents,
            system_instruction,
            tools,
            generation_config: Some(GenerationConfig {
                temperature: Some(request.temperature),
                max_output_tokens: Some(request.max_tokens),
            }),
        };

        let access_token = self.get_access_token().await?;

        let max_retries = 3;
        for attempt in 0..=max_retries {
            let url = self.build_endpoint(&request.model, false);
            debug!(url = %url, attempt, "Sending Vertex AI request");

            let resp = self
                .client
                .post(&url)
                .header("Authorization", format!("Bearer {}", access_token))
                .header("Content-Type", "application/json")
                .json(&vertex_request)
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
                let message = serde_json::from_str::<VertexErrorResponse>(&body)
                    .map(|e| e.error.message)
                    .unwrap_or(body);
                return Err(LlmError::Api { status, message });
            }

            let body = resp
                .text()
                .await
                .map_err(|e| LlmError::Http(e.to_string()))?;
            let vertex_response: VertexResponse =
                serde_json::from_str(&body).map_err(|e| LlmError::Parse(e.to_string()))?;

            return convert_response(vertex_response);
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

        let vertex_request = VertexRequest {
            contents,
            system_instruction,
            tools,
            generation_config: Some(GenerationConfig {
                temperature: Some(request.temperature),
                max_output_tokens: Some(request.max_tokens),
            }),
        };

        let access_token = self.get_access_token().await?;

        let max_retries = 3;
        for attempt in 0..=max_retries {
            let url = format!("{}?alt=sse", self.build_endpoint(&request.model, true));
            debug!(url = %url, attempt, "Sending Vertex AI streaming request");

            let resp = self
                .client
                .post(&url)
                .header("Authorization", format!("Bearer {}", access_token))
                .header("Content-Type", "application/json")
                .json(&vertex_request)
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
                let message = serde_json::from_str::<VertexErrorResponse>(&body)
                    .map(|e| e.error.message)
                    .unwrap_or(body);
                return Err(LlmError::Api { status, message });
            }

            // Process SSE stream
            let mut byte_stream = resp.bytes_stream();
            let mut buffer = String::new();
            let mut accumulated_text = String::new();
            let mut final_tool_calls = Vec::new();
            let mut final_usage = None;

            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = chunk_result.map_err(|e| LlmError::Http(e.to_string()))?;
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // Process complete lines
                while let Some(line_end) = buffer.find('\n') {
                    let line = buffer[..line_end].trim().to_string();
                    buffer = buffer[line_end + 1..].to_string();

                    if line.is_empty() || !line.starts_with("data: ") {
                        continue;
                    }

                    let json_str = &line[6..];
                    if json_str == "[DONE]" {
                        break;
                    }

                    if let Ok(resp) = serde_json::from_str::<VertexResponse>(json_str) {
                        if let Some(candidate) = resp.candidates.into_iter().next() {
                            if let Some(content) = candidate.content {
                                for part in content.parts {
                                    match part {
                                        VertexPart::Text { text } => {
                                            accumulated_text.push_str(&text);
                                            let _ = tx.send(StreamEvent::TextDelta { text }).await;
                                        }
                                        VertexPart::FunctionCall { function_call } => {
                                            final_tool_calls.push(ToolCall {
                                                id: format!(
                                                    "call_{}",
                                                    &uuid::Uuid::new_v4().to_string()[..8]
                                                ),
                                                name: function_call.name,
                                                input: function_call.args,
                                            });
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        if let Some(usage) = resp.usage_metadata {
                            final_usage = Some(TokenUsage {
                                input_tokens: usage.prompt_token_count,
                                output_tokens: usage.candidates_token_count,
                            });
                        }
                    }
                }
            }

            let stop_reason = if !final_tool_calls.is_empty() {
                StopReason::ToolUse
            } else {
                StopReason::EndTurn
            };

            let usage = final_usage.unwrap_or_default();

            let _ = tx
                .send(StreamEvent::ContentComplete { stop_reason, usage })
                .await;

            let content = if accumulated_text.is_empty() {
                Vec::new()
            } else {
                vec![ContentBlock::Text {
                    text: accumulated_text,
                    provider_metadata: None,
                }]
            };

            return Ok(CompletionResponse {
                content,
                stop_reason,
                tool_calls: final_tool_calls,
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

    #[test]
    fn test_vertex_driver_creation() {
        let driver = VertexAIDriver::new("test-project".to_string(), "us-central1".to_string());
        assert_eq!(driver.project_id, "test-project");
        assert_eq!(driver.region, "us-central1");
    }

    #[test]
    fn test_build_endpoint_non_streaming() {
        let driver = VertexAIDriver::new("my-project".to_string(), "us-central1".to_string());
        let endpoint = driver.build_endpoint("gemini-2.0-flash", false);
        assert_eq!(
            endpoint,
            "https://us-central1-aiplatform.googleapis.com/v1/projects/my-project/locations/us-central1/publishers/google/models/gemini-2.0-flash:generateContent"
        );
    }

    #[test]
    fn test_build_endpoint_streaming() {
        let driver = VertexAIDriver::new("my-project".to_string(), "europe-west4".to_string());
        let endpoint = driver.build_endpoint("gemini-1.5-pro", true);
        assert_eq!(
            endpoint,
            "https://europe-west4-aiplatform.googleapis.com/v1/projects/my-project/locations/europe-west4/publishers/google/models/gemini-1.5-pro:streamGenerateContent"
        );
    }

    #[test]
    fn test_build_endpoint_strips_model_prefix() {
        let driver = VertexAIDriver::new("my-project".to_string(), "us-central1".to_string());
        let endpoint = driver.build_endpoint("models/gemini-2.0-flash", false);
        assert_eq!(
            endpoint,
            "https://us-central1-aiplatform.googleapis.com/v1/projects/my-project/locations/us-central1/publishers/google/models/gemini-2.0-flash:generateContent"
        );
    }

    #[test]
    fn test_token_cache_initially_invalid() {
        let cache = TokenCache::new();
        assert!(!cache.is_valid());
        assert!(cache.token.is_none());
    }

    #[test]
    fn test_vertex_content_serialization() {
        let content = VertexContent {
            role: Some("user".to_string()),
            parts: vec![VertexPart::Text {
                text: "Hello".to_string(),
            }],
        };
        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"text\":\"Hello\""));
    }
}
