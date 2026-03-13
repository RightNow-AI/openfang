use crate::tool::{ToolCall, ToolDefinition};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyLevel {
    LocalOnly,
    LocalPreferred,
    RemoteAllowed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LatencyClass {
    Interactive,
    Standard,
    Background,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostClass {
    Low,
    Balanced,
    Premium,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    Text,
    Json,
    ToolCall,
    Embedding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiTaskType {
    Chat,
    Summarize,
    CodeComplete,
    CodeReview,
    Extract,
    Classify,
    Embed,
    AgentStep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiMessage {
    pub role: MessageRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct CanonicalModelId(pub String);

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct ProviderModelId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum AdapterId {
    BrowserWorker,
    Ollama,
    OpenRouter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Streaming,
    Tools,
    JsonMode,
    Embeddings,
    Vision,
    LongContext,
    ReasoningStrong,
    LowLatency,
    LocalExecution,
    RemoteExecution,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiRequest {
    pub request_id: String,
    pub task_type: AiTaskType,
    pub messages: Vec<AiMessage>,
    pub output_mode: OutputMode,
    #[serde(default)]
    pub tools: Vec<ToolDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_model_family: Option<String>,
    #[serde(default)]
    pub required_capabilities: Vec<Capability>,
    pub privacy_level: PrivacyLevel,
    pub latency_class: LatencyClass,
    pub cost_class: CostClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_tokens_estimate: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ToolCall,
    Error,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Usage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_cost_usd_micros: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiResponse {
    pub request_id: String,
    pub adapter_id: AdapterId,
    pub canonical_model_id: CanonicalModelId,
    pub provider_model_id: ProviderModelId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_json: Option<serde_json::Value>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: FinishReason,
    pub usage: Usage,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamEvent {
    MessageStart,
    TextDelta(String),
    ToolCall(ToolCall),
    JsonDelta(String),
    MessageEnd(FinishReason),
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteTarget {
    pub adapter_id: AdapterId,
    pub canonical_model_id: CanonicalModelId,
    pub timeout: Duration,
    pub stream: bool,
    pub fallback_allowed: bool,
    pub egress_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingDecision {
    pub primary: RouteTarget,
    #[serde(default)]
    pub fallbacks: Vec<RouteTarget>,
    pub reason: String,
    pub privacy_enforced: bool,
}

impl AiRequest {
    pub fn wants_streaming(&self) -> bool {
        matches!(self.latency_class, LatencyClass::Interactive | LatencyClass::Standard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_streaming_defaults_follow_latency() {
        let request = AiRequest {
            request_id: "req-1".into(),
            task_type: AiTaskType::Chat,
            messages: vec![AiMessage {
                role: MessageRole::User,
                content: "hello".into(),
                name: None,
                tool_call_id: None,
            }],
            output_mode: OutputMode::Text,
            tools: vec![],
            preferred_model_family: None,
            required_capabilities: vec![],
            privacy_level: PrivacyLevel::LocalPreferred,
            latency_class: LatencyClass::Interactive,
            cost_class: CostClass::Balanced,
            max_output_tokens: None,
            context_tokens_estimate: None,
            workspace_id: None,
            user_id: None,
            metadata: BTreeMap::new(),
        };

        assert!(request.wants_streaming());
    }
}
