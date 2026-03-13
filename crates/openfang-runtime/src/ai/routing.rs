use openfang_types::ai::{
    AiRequest, AiTaskType, Capability, CostClass, LatencyClass, PrivacyLevel, RouteTarget,
    RoutingDecision,
};
use openfang_types::capabilities::{ModelCapabilityRecord, PriceClass, PrivacyClass, SpeedClass};

use super::registry::CapabilityRegistry;

#[derive(Debug, thiserror::Error)]
pub enum RoutingError {
    #[error("no route available: {0}")]
    NoRoute(String),
}

#[derive(Debug, Clone, Default)]
pub struct RoutingContext {
    pub online: bool,
    pub browser_worker_available: bool,
    pub ollama_available: bool,
    pub openrouter_available: bool,
}

pub struct PolicyEngine;

impl PolicyEngine {
    pub fn decide(
        request: &AiRequest,
        registry: &CapabilityRegistry,
        ctx: &RoutingContext,
    ) -> Result<RoutingDecision, RoutingError> {
        let mut candidates: Vec<&ModelCapabilityRecord> = registry
            .all()
            .filter(|m| Self::passes_privacy(request.privacy_level, m))
            .filter(|m| Self::passes_capabilities(request, m))
            .filter(|m| Self::passes_context(request, m))
            .filter(|m| Self::passes_family_preference(request, m))
            .filter(|m| Self::passes_runtime_health(m, ctx))
            .collect();

        if candidates.is_empty() {
            return Err(RoutingError::NoRoute(
                "no candidates after policy filters".into(),
            ));
        }

        candidates.sort_by(|a, b| Self::score(b, request).cmp(&Self::score(a, request)));

        let primary = Self::to_target(candidates[0], request);
        let fallbacks = candidates
            .into_iter()
            .skip(1)
            .take(3)
            .map(|m| Self::to_target(m, request))
            .collect();

        Ok(RoutingDecision {
            primary,
            fallbacks,
            reason: "policy-ranked candidate selection".into(),
            privacy_enforced: request.privacy_level != PrivacyLevel::RemoteAllowed,
        })
    }

    fn passes_privacy(level: PrivacyLevel, model: &ModelCapabilityRecord) -> bool {
        match level {
            PrivacyLevel::LocalOnly => model.privacy_class != PrivacyClass::Remote,
            PrivacyLevel::LocalPreferred => true,
            PrivacyLevel::RemoteAllowed => true,
        }
    }

    fn passes_capabilities(request: &AiRequest, model: &ModelCapabilityRecord) -> bool {
        request
            .required_capabilities
            .iter()
            .all(|cap| model.supported_capabilities.contains(cap))
            && (!matches!(request.latency_class, LatencyClass::Interactive)
                || model.supported_capabilities.contains(&Capability::Streaming))
    }

    fn passes_context(request: &AiRequest, model: &ModelCapabilityRecord) -> bool {
        match request.context_tokens_estimate {
            Some(estimate) => estimate <= model.max_context_tokens,
            None => true,
        }
    }

    fn passes_family_preference(request: &AiRequest, model: &ModelCapabilityRecord) -> bool {
        request
            .preferred_model_family
            .as_ref()
            .map(|family| model.family.eq_ignore_ascii_case(family) || request.privacy_level != PrivacyLevel::LocalOnly)
            .unwrap_or(true)
    }

    fn passes_runtime_health(model: &ModelCapabilityRecord, ctx: &RoutingContext) -> bool {
        match model.adapter_id {
            openfang_types::ai::AdapterId::BrowserWorker => ctx.browser_worker_available,
            openfang_types::ai::AdapterId::Ollama => ctx.ollama_available,
            openfang_types::ai::AdapterId::OpenRouter => ctx.online && ctx.openrouter_available,
        }
    }

    fn score(model: &ModelCapabilityRecord, request: &AiRequest) -> i64 {
        let mut score = i64::from(model.reliability_score_bps);

        match request.privacy_level {
            PrivacyLevel::LocalOnly => {
                if model.local {
                    score += 10_000;
                }
            }
            PrivacyLevel::LocalPreferred => {
                if model.local {
                    score += 5_000;
                }
            }
            PrivacyLevel::RemoteAllowed => {}
        }

        match request.latency_class {
            LatencyClass::Interactive => {
                if matches!(model.speed_class, SpeedClass::Fast) {
                    score += 3_000;
                }
            }
            LatencyClass::Standard => {
                if matches!(model.speed_class, SpeedClass::Medium) {
                    score += 500;
                }
            }
            LatencyClass::Background => {
                if matches!(model.speed_class, SpeedClass::Slow | SpeedClass::Medium) {
                    score += 500;
                }
            }
        }

        match request.cost_class {
            CostClass::Low => {
                if matches!(model.price_class, PriceClass::Low) {
                    score += 3_000;
                }
            }
            CostClass::Balanced => {
                if matches!(model.price_class, PriceClass::Medium) {
                    score += 1_000;
                }
            }
            CostClass::Premium => {
                if matches!(model.price_class, PriceClass::High) {
                    score += 1_000;
                }
            }
        }

        if request.task_type == AiTaskType::CodeComplete && model.family.contains("qwen") {
            score += 1_500;
        }

        if request.task_type == AiTaskType::Summarize && model.local {
            score += 700;
        }

        score
    }

    fn to_target(model: &ModelCapabilityRecord, request: &AiRequest) -> RouteTarget {
        RouteTarget {
            adapter_id: model.adapter_id.clone(),
            canonical_model_id: model.canonical_model_id.clone(),
            timeout: match request.latency_class {
                LatencyClass::Interactive => std::time::Duration::from_secs(10),
                LatencyClass::Standard => std::time::Duration::from_secs(30),
                LatencyClass::Background => std::time::Duration::from_secs(90),
            },
            stream: request.wants_streaming(),
            fallback_allowed: true,
            egress_allowed: request.privacy_level != PrivacyLevel::LocalOnly,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_types::ai::{
        AdapterId, AiMessage, CanonicalModelId, MessageRole, OutputMode,
    };
    use openfang_types::capabilities::{RuntimeType, ModelCapabilityRecord};
    use std::collections::BTreeMap;

    fn request(privacy_level: PrivacyLevel) -> AiRequest {
        AiRequest {
            request_id: "req-1".into(),
            task_type: AiTaskType::CodeComplete,
            messages: vec![AiMessage {
                role: MessageRole::User,
                content: "finish this function".into(),
                name: None,
                tool_call_id: None,
            }],
            output_mode: OutputMode::Text,
            tools: vec![],
            preferred_model_family: Some("qwen".into()),
            required_capabilities: vec![Capability::Streaming],
            privacy_level,
            latency_class: LatencyClass::Interactive,
            cost_class: CostClass::Low,
            max_output_tokens: Some(256),
            context_tokens_estimate: Some(512),
            workspace_id: None,
            user_id: None,
            metadata: BTreeMap::new(),
        }
    }

    fn registry() -> CapabilityRegistry {
        let mut registry = CapabilityRegistry::default();
        registry.insert(ModelCapabilityRecord {
            adapter_id: AdapterId::Ollama,
            canonical_model_id: CanonicalModelId("ollama/qwen2.5".into()),
            provider_model_id: "qwen2.5".into(),
            family: "qwen".into(),
            runtime_type: RuntimeType::LocalHost,
            local: true,
            browser: false,
            supported_capabilities: vec![Capability::Streaming, Capability::LocalExecution],
            max_context_tokens: 32_768,
            speed_class: SpeedClass::Fast,
            price_class: PriceClass::Low,
            privacy_class: PrivacyClass::Host,
            reliability_score_bps: 9000,
        });
        registry.insert(ModelCapabilityRecord {
            adapter_id: AdapterId::OpenRouter,
            canonical_model_id: CanonicalModelId("openrouter/qwen-2.5-72b-instruct".into()),
            provider_model_id: "openrouter/qwen/qwen-2.5-72b-instruct".into(),
            family: "qwen".into(),
            runtime_type: RuntimeType::RemoteApi,
            local: false,
            browser: false,
            supported_capabilities: vec![Capability::Streaming, Capability::RemoteExecution],
            max_context_tokens: 128_000,
            speed_class: SpeedClass::Medium,
            price_class: PriceClass::Medium,
            privacy_class: PrivacyClass::Remote,
            reliability_score_bps: 9200,
        });
        registry
    }

    #[test]
    fn local_only_blocks_remote_routes() {
        let decision = PolicyEngine::decide(
            &request(PrivacyLevel::LocalOnly),
            &registry(),
            &RoutingContext {
                online: true,
                browser_worker_available: false,
                ollama_available: true,
                openrouter_available: true,
            },
        )
        .unwrap();

        assert_eq!(decision.primary.adapter_id, AdapterId::Ollama);
        assert!(decision.fallbacks.is_empty());
    }
}
