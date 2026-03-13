use super::gateway::ModelGateway;
use super::providers::ProviderError;
use super::registry::CapabilityRegistry;
use super::routing::{PolicyEngine, RoutingContext, RoutingError};
use openfang_types::ai::{AiRequest, AiResponse, RoutingDecision};

#[derive(Debug, thiserror::Error)]
pub enum OrchestrationError {
    #[error(transparent)]
    Routing(#[from] RoutingError),
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error("missing capability record for selected model")]
    MissingCapability,
}

pub struct Orchestrator {
    pub registry: CapabilityRegistry,
    pub gateway: ModelGateway,
}

impl Orchestrator {
    pub fn new(registry: CapabilityRegistry, gateway: ModelGateway) -> Self {
        Self { registry, gateway }
    }

    pub async fn generate(
        &self,
        request: AiRequest,
        routing_ctx: &RoutingContext,
    ) -> Result<AiResponse, OrchestrationError> {
        let decision = PolicyEngine::decide(&request, &self.registry, routing_ctx)?;
        self.try_routes(request, decision).await
    }

    async fn try_routes(
        &self,
        request: AiRequest,
        decision: RoutingDecision,
    ) -> Result<AiResponse, OrchestrationError> {
        let mut routes = Vec::with_capacity(1 + decision.fallbacks.len());
        routes.push(decision.primary);
        routes.extend(decision.fallbacks);

        let mut last_err: Option<ProviderError> = None;

        for route in routes {
            let capability = self
                .registry
                .get(&route.canonical_model_id)
                .ok_or(OrchestrationError::MissingCapability)?;

            match self
                .gateway
                .generate(request.clone(), &route, capability)
                .await
            {
                Ok(response) => return Ok(response),
                Err(error) => {
                    last_err = Some(error);
                    if !route.fallback_allowed {
                        break;
                    }
                }
            }
        }

        Err(OrchestrationError::Provider(last_err.unwrap_or(
            ProviderError::Unavailable("all routes failed".into()),
        )))
    }
}
