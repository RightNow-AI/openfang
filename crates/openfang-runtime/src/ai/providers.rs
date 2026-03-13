use async_trait::async_trait;
use openfang_types::ai::{
    AdapterId, AiRequest, AiResponse, Capability, FinishReason, MessageRole, OutputMode,
    ProviderModelId, StreamEvent as AiStreamEvent, Usage,
};
use openfang_types::capabilities::ModelCapabilityRecord;
use openfang_types::message::{Message, MessageContent, Role, StopReason, TokenUsage};
use std::pin::Pin;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::drivers::create_driver;
use crate::llm_driver::{
    CompletionRequest, CompletionResponse, DriverConfig, LlmError,
    StreamEvent as DriverStreamEvent,
};

pub type ProviderStream = Pin<Box<dyn futures::Stream<Item = Result<AiStreamEvent, ProviderError>> + Send>>;

#[derive(Debug, Clone, Default)]
pub struct ProviderTelemetry {
    pub route_label: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("unsupported request: {0}")]
    Unsupported(String),
    #[error("provider unavailable: {0}")]
    Unavailable(String),
    #[error("timeout")]
    Timeout,
    #[error("rate limited")]
    RateLimited,
    #[error("privacy policy blocked remote egress")]
    PrivacyBlocked,
    #[error("context too large")]
    ContextTooLarge,
    #[error("bad response: {0}")]
    BadResponse(String),
    #[error("transport error: {0}")]
    Transport(String),
}

#[derive(Debug, Clone)]
pub struct ProviderHealth {
    pub provider_name: String,
    pub healthy: bool,
    pub detail: Option<String>,
}

#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    fn id(&self) -> AdapterId;

    async fn list_capabilities(&self) -> Result<Vec<ModelCapabilityRecord>, ProviderError>;

    async fn health(&self) -> ProviderHealth;

    fn supports(&self, request: &AiRequest, capability: &ModelCapabilityRecord) -> bool;

    async fn generate(
        &self,
        request: AiRequest,
        model: &ModelCapabilityRecord,
    ) -> Result<AiResponse, ProviderError>;

    async fn stream(
        &self,
        request: AiRequest,
        model: &ModelCapabilityRecord,
    ) -> Result<ProviderStream, ProviderError>;

    async fn embed(
        &self,
        _request: AiRequest,
        _model: &ModelCapabilityRecord,
    ) -> Result<AiResponse, ProviderError> {
        Err(ProviderError::Unsupported(
            "embedding route not implemented yet".to_string(),
        ))
    }
}

#[derive(Clone)]
pub struct OllamaAdapter {
    config: DriverConfig,
    capabilities: Vec<ModelCapabilityRecord>,
    telemetry: ProviderTelemetry,
}

#[derive(Clone)]
pub struct OpenRouterAdapter {
    config: DriverConfig,
    capabilities: Vec<ModelCapabilityRecord>,
    telemetry: ProviderTelemetry,
}

#[derive(Clone)]
pub struct BrowserWorkerAdapter {
    capabilities: Vec<ModelCapabilityRecord>,
}

impl OllamaAdapter {
    pub fn new(
        base_url: Option<String>,
        capabilities: Vec<ModelCapabilityRecord>,
    ) -> Self {
        Self {
            config: DriverConfig {
                provider: "ollama".to_string(),
                api_key: None,
                base_url,
            },
            capabilities,
            telemetry: ProviderTelemetry {
                route_label: "ollama".to_string(),
            },
        }
    }
}

impl OpenRouterAdapter {
    pub fn new(
        api_key: Option<String>,
        base_url: Option<String>,
        capabilities: Vec<ModelCapabilityRecord>,
    ) -> Self {
        Self {
            config: DriverConfig {
                provider: "openrouter".to_string(),
                api_key,
                base_url,
            },
            capabilities,
            telemetry: ProviderTelemetry {
                route_label: "openrouter".to_string(),
            },
        }
    }
}

impl BrowserWorkerAdapter {
    pub fn new(capabilities: Vec<ModelCapabilityRecord>) -> Self {
        Self { capabilities }
    }
}

#[async_trait]
impl ProviderAdapter for OllamaAdapter {
    fn id(&self) -> AdapterId {
        AdapterId::Ollama
    }

    async fn list_capabilities(&self) -> Result<Vec<ModelCapabilityRecord>, ProviderError> {
        Ok(self.capabilities.clone())
    }

    async fn health(&self) -> ProviderHealth {
        ProviderHealth {
            provider_name: self.telemetry.route_label.clone(),
            healthy: true,
            detail: self.config.base_url.clone(),
        }
    }

    fn supports(&self, request: &AiRequest, capability: &ModelCapabilityRecord) -> bool {
        supports_request(request, capability)
    }

    async fn generate(
        &self,
        request: AiRequest,
        model: &ModelCapabilityRecord,
    ) -> Result<AiResponse, ProviderError> {
        generate_with_driver(&self.config, self.id(), request, model).await
    }

    async fn stream(
        &self,
        request: AiRequest,
        model: &ModelCapabilityRecord,
    ) -> Result<ProviderStream, ProviderError> {
        stream_with_driver(&self.config, request, model).await
    }
}

#[async_trait]
impl ProviderAdapter for OpenRouterAdapter {
    fn id(&self) -> AdapterId {
        AdapterId::OpenRouter
    }

    async fn list_capabilities(&self) -> Result<Vec<ModelCapabilityRecord>, ProviderError> {
        Ok(self.capabilities.clone())
    }

    async fn health(&self) -> ProviderHealth {
        ProviderHealth {
            provider_name: self.telemetry.route_label.clone(),
            healthy: self.config.api_key.is_some(),
            detail: if self.config.api_key.is_some() {
                self.config.base_url.clone()
            } else {
                Some("missing OPENROUTER_API_KEY".to_string())
            },
        }
    }

    fn supports(&self, request: &AiRequest, capability: &ModelCapabilityRecord) -> bool {
        supports_request(request, capability)
    }

    async fn generate(
        &self,
        request: AiRequest,
        model: &ModelCapabilityRecord,
    ) -> Result<AiResponse, ProviderError> {
        generate_with_driver(&self.config, self.id(), request, model).await
    }

    async fn stream(
        &self,
        request: AiRequest,
        model: &ModelCapabilityRecord,
    ) -> Result<ProviderStream, ProviderError> {
        stream_with_driver(&self.config, request, model).await
    }
}

#[async_trait]
impl ProviderAdapter for BrowserWorkerAdapter {
    fn id(&self) -> AdapterId {
        AdapterId::BrowserWorker
    }

    async fn list_capabilities(&self) -> Result<Vec<ModelCapabilityRecord>, ProviderError> {
        Ok(self.capabilities.clone())
    }

    async fn health(&self) -> ProviderHealth {
        ProviderHealth {
            provider_name: "browser_worker".to_string(),
            healthy: false,
            detail: Some("browser worker execution is not wired into the Rust runtime yet".to_string()),
        }
    }

    fn supports(&self, request: &AiRequest, capability: &ModelCapabilityRecord) -> bool {
        supports_request(request, capability)
    }

    async fn generate(
        &self,
        _request: AiRequest,
        _model: &ModelCapabilityRecord,
    ) -> Result<AiResponse, ProviderError> {
        Err(ProviderError::Unavailable(
            "browser worker execution is only available through the frontend bridge".to_string(),
        ))
    }

    async fn stream(
        &self,
        _request: AiRequest,
        _model: &ModelCapabilityRecord,
    ) -> Result<ProviderStream, ProviderError> {
        Err(ProviderError::Unavailable(
            "browser worker streaming is not available from the Rust runtime".to_string(),
        ))
    }
}

fn supports_request(request: &AiRequest, capability: &ModelCapabilityRecord) -> bool {
    if matches!(request.output_mode, OutputMode::Embedding)
        && !capability.supported_capabilities.contains(&Capability::Embeddings)
    {
        return false;
    }

    request
        .required_capabilities
        .iter()
        .all(|required| capability.supported_capabilities.contains(required))
}

async fn generate_with_driver(
    config: &DriverConfig,
    adapter_id: AdapterId,
    request: AiRequest,
    model: &ModelCapabilityRecord,
) -> Result<AiResponse, ProviderError> {
    let driver = create_driver(config).map_err(map_llm_error)?;
    let completion_request = to_completion_request(&request, &model.provider_model_id)?;
    let started = Instant::now();
    let response = driver.complete(completion_request).await.map_err(map_llm_error)?;
    Ok(to_ai_response(adapter_id, request.request_id, model, response, started.elapsed().as_millis() as u64))
}

async fn stream_with_driver(
    config: &DriverConfig,
    request: AiRequest,
    model: &ModelCapabilityRecord,
) -> Result<ProviderStream, ProviderError> {
    let driver = create_driver(config).map_err(map_llm_error)?;
    let completion_request = to_completion_request(&request, &model.provider_model_id)?;
    let (tx, rx) = mpsc::channel(32);
    tokio::spawn(async move {
        let _ = tx.send(Ok(AiStreamEvent::MessageStart)).await;
        let result = driver.stream(completion_request, tx_for_driver(tx.clone())).await;
        if let Err(error) = result {
            let _ = tx.send(Err(map_llm_error(error))).await;
        }
        let _ = tx.send(Ok(AiStreamEvent::MessageEnd(FinishReason::Stop))).await;
    });

    Ok(Box::pin(ReceiverStream::new(rx)))
}

fn tx_for_driver(
    tx: mpsc::Sender<Result<AiStreamEvent, ProviderError>>,
) -> mpsc::Sender<DriverStreamEvent> {
    let (driver_tx, mut driver_rx) = mpsc::channel(32);
    tokio::spawn(async move {
        while let Some(event) = driver_rx.recv().await {
            let mapped = match event {
                DriverStreamEvent::TextDelta { text } => Ok(AiStreamEvent::TextDelta(text)),
                DriverStreamEvent::ToolUseEnd { id, name, input } => Ok(AiStreamEvent::ToolCall(
                    openfang_types::tool::ToolCall { id, name, input },
                )),
                DriverStreamEvent::ContentComplete { stop_reason, .. } => {
                    Ok(AiStreamEvent::MessageEnd(map_finish_reason(stop_reason)))
                }
                DriverStreamEvent::ThinkingDelta { text }
                | DriverStreamEvent::ToolInputDelta { text } => Ok(AiStreamEvent::JsonDelta(text)),
                DriverStreamEvent::ToolUseStart { .. }
                | DriverStreamEvent::PhaseChange { .. }
                | DriverStreamEvent::ToolExecutionResult { .. } => continue,
            };
            let _ = tx.send(mapped).await;
        }
    });
    driver_tx
}

fn to_completion_request(
    request: &AiRequest,
    provider_model_id: &str,
) -> Result<CompletionRequest, ProviderError> {
    if request.messages.is_empty() {
        return Err(ProviderError::BadResponse(
            "AI request must contain at least one message".to_string(),
        ));
    }

    let system = request
        .messages
        .iter()
        .filter(|message| matches!(message.role, MessageRole::System))
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");

    let messages = request
        .messages
        .iter()
        .map(|message| Message {
            role: match message.role {
                MessageRole::System => Role::System,
                MessageRole::User => Role::User,
                MessageRole::Assistant | MessageRole::Tool => Role::Assistant,
            },
            content: MessageContent::Text(message.content.clone()),
        })
        .collect();

    Ok(CompletionRequest {
        model: provider_model_id.to_string(),
        messages,
        tools: request.tools.clone(),
        max_tokens: request.max_output_tokens.unwrap_or(2048),
        temperature: default_temperature(request),
        system: if system.is_empty() { None } else { Some(system) },
        thinking: None,
    })
}

fn default_temperature(request: &AiRequest) -> f32 {
    match request.task_type {
        openfang_types::ai::AiTaskType::CodeComplete
        | openfang_types::ai::AiTaskType::Classify
        | openfang_types::ai::AiTaskType::Extract => 0.1,
        _ => 0.7,
    }
}

fn to_ai_response(
    adapter_id: AdapterId,
    request_id: String,
    model: &ModelCapabilityRecord,
    response: CompletionResponse,
    latency_ms: u64,
) -> AiResponse {
    let output_text = Some(response.text()).filter(|text| !text.is_empty());
    AiResponse {
        request_id,
        adapter_id,
        canonical_model_id: model.canonical_model_id.clone(),
        provider_model_id: ProviderModelId(model.provider_model_id.clone()),
        output_text,
        output_json: None,
        tool_calls: response.tool_calls,
        finish_reason: map_finish_reason(response.stop_reason),
        usage: map_usage(response.usage, model),
        latency_ms,
    }
}

fn map_usage(usage: TokenUsage, model: &ModelCapabilityRecord) -> Usage {
    let input_token_count = usage.input_tokens.min(u64::from(u32::MAX)) as u32;
    let output_token_count = usage.output_tokens.min(u64::from(u32::MAX)) as u32;
    let input_tokens = Some(input_token_count);
    let output_tokens = Some(output_token_count);
    let total_tokens = Some(input_token_count.saturating_add(output_token_count));
    let estimated_cost_usd_micros = estimate_cost_micros(input_token_count, output_token_count, model);

    Usage {
        input_tokens,
        output_tokens,
        total_tokens,
        estimated_cost_usd_micros,
    }
}

fn estimate_cost_micros(
    input_tokens: u32,
    output_tokens: u32,
    model: &ModelCapabilityRecord,
) -> Option<u64> {
    let input = u128::from(input_tokens);
    let output = u128::from(output_tokens);
    let price_micros = match model.adapter_id {
        AdapterId::Ollama | AdapterId::BrowserWorker => return Some(0),
        AdapterId::OpenRouter => 1_000_u128,
    };
    Some(((input + output) * price_micros / 1_000_000) as u64)
}

fn map_finish_reason(reason: StopReason) -> FinishReason {
    match reason {
        StopReason::EndTurn | StopReason::StopSequence => FinishReason::Stop,
        StopReason::ToolUse => FinishReason::ToolCall,
        StopReason::MaxTokens => FinishReason::Length,
    }
}

fn map_llm_error(error: LlmError) -> ProviderError {
    match error {
        LlmError::RateLimited { .. } => ProviderError::RateLimited,
        LlmError::Overloaded { .. } => ProviderError::Unavailable("provider overloaded".to_string()),
        LlmError::ModelNotFound(message) => ProviderError::BadResponse(message),
        LlmError::MissingApiKey(message) | LlmError::AuthenticationFailed(message) => {
            ProviderError::Unavailable(message)
        }
        LlmError::Api { message, .. } | LlmError::Http(message) | LlmError::Parse(message) => {
            ProviderError::Transport(message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_types::ai::{
        AiMessage, AiTaskType, CanonicalModelId, CostClass, LatencyClass, PrivacyLevel,
    };
    use openfang_types::capabilities::{PriceClass, PrivacyClass, RuntimeType, SpeedClass};
    use std::collections::BTreeMap;

    fn sample_request() -> AiRequest {
        AiRequest {
            request_id: "req-123".to_string(),
            task_type: AiTaskType::Chat,
            messages: vec![AiMessage {
                role: MessageRole::User,
                content: "hello".to_string(),
                name: None,
                tool_call_id: None,
            }],
            output_mode: OutputMode::Text,
            tools: vec![],
            preferred_model_family: Some("qwen".to_string()),
            required_capabilities: vec![Capability::Streaming],
            privacy_level: PrivacyLevel::LocalPreferred,
            latency_class: LatencyClass::Interactive,
            cost_class: CostClass::Balanced,
            max_output_tokens: Some(512),
            context_tokens_estimate: Some(128),
            workspace_id: None,
            user_id: None,
            metadata: BTreeMap::new(),
        }
    }

    fn sample_model() -> ModelCapabilityRecord {
        ModelCapabilityRecord {
            adapter_id: AdapterId::Ollama,
            canonical_model_id: CanonicalModelId("local/qwen".to_string()),
            provider_model_id: "qwen2.5:latest".to_string(),
            family: "qwen".to_string(),
            runtime_type: RuntimeType::LocalHost,
            local: true,
            browser: false,
            supported_capabilities: vec![Capability::Streaming, Capability::LocalExecution],
            max_context_tokens: 32768,
            speed_class: SpeedClass::Medium,
            price_class: PriceClass::Low,
            privacy_class: PrivacyClass::Host,
            reliability_score_bps: 9000,
        }
    }

    #[test]
    fn support_checks_follow_required_capabilities() {
        assert!(supports_request(&sample_request(), &sample_model()));
    }

    #[test]
    fn completion_conversion_preserves_provider_model_id() {
        let request = sample_request();
        let completion = to_completion_request(&request, "qwen2.5:latest").unwrap();
        assert_eq!(completion.model, "qwen2.5:latest");
        assert_eq!(completion.messages.len(), 1);
    }
}
