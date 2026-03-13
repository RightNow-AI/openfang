use std::collections::BTreeMap;
use std::sync::Arc;

use openfang_types::ai::{AdapterId, AiRequest, AiResponse, RouteTarget};
use openfang_types::capabilities::ModelCapabilityRecord;

use super::providers::{ProviderAdapter, ProviderError, ProviderHealth, ProviderStream};

pub struct ModelGateway {
    adapters: BTreeMap<AdapterId, Arc<dyn ProviderAdapter>>,
}

impl ModelGateway {
    pub fn new(adapters: BTreeMap<AdapterId, Arc<dyn ProviderAdapter>>) -> Self {
        Self { adapters }
    }

    pub fn register(&mut self, adapter: Arc<dyn ProviderAdapter>) {
        self.adapters.insert(adapter.id(), adapter);
    }

    pub async fn generate(
        &self,
        request: AiRequest,
        target: &RouteTarget,
        capability: &ModelCapabilityRecord,
    ) -> Result<AiResponse, ProviderError> {
        let adapter = self
            .adapters
            .get(&target.adapter_id)
            .ok_or_else(|| ProviderError::Unavailable(format!("missing adapter {:?}", target.adapter_id)))?;
        adapter.generate(request, capability).await
    }

    pub async fn stream(
        &self,
        request: AiRequest,
        target: &RouteTarget,
        capability: &ModelCapabilityRecord,
    ) -> Result<ProviderStream, ProviderError> {
        let adapter = self
            .adapters
            .get(&target.adapter_id)
            .ok_or_else(|| ProviderError::Unavailable(format!("missing adapter {:?}", target.adapter_id)))?;
        adapter.stream(request, capability).await
    }

    pub async fn health(&self) -> Vec<ProviderHealth> {
        let mut out = Vec::new();
        for adapter in self.adapters.values() {
            out.push(adapter.health().await);
        }
        out
    }
}
