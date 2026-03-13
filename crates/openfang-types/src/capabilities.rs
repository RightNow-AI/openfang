use crate::ai::{AdapterId, CanonicalModelId, Capability};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyClass {
    Device,
    Host,
    Remote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeType {
    BrowserWorker,
    LocalHost,
    RemoteApi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeedClass {
    Fast,
    Medium,
    Slow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceClass {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelCapabilityRecord {
    pub adapter_id: AdapterId,
    pub canonical_model_id: CanonicalModelId,
    pub provider_model_id: String,
    pub family: String,
    pub runtime_type: RuntimeType,
    pub local: bool,
    pub browser: bool,
    #[serde(default)]
    pub supported_capabilities: Vec<Capability>,
    pub max_context_tokens: u32,
    pub speed_class: SpeedClass,
    pub price_class: PriceClass,
    pub privacy_class: PrivacyClass,
    pub reliability_score_bps: u16,
}

impl ModelCapabilityRecord {
    pub fn supports(&self, capability: Capability) -> bool {
        self.supported_capabilities.contains(&capability)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_lookup_reflects_record_contents() {
        let record = ModelCapabilityRecord {
            adapter_id: AdapterId::Ollama,
            canonical_model_id: CanonicalModelId("local/qwen-coder-small".into()),
            provider_model_id: "qwen2.5:latest".into(),
            family: "qwen".into(),
            runtime_type: RuntimeType::LocalHost,
            local: true,
            browser: false,
            supported_capabilities: vec![Capability::Streaming, Capability::LocalExecution],
            max_context_tokens: 32768,
            speed_class: SpeedClass::Medium,
            price_class: PriceClass::Low,
            privacy_class: PrivacyClass::Host,
            reliability_score_bps: 9000,
        };

        assert!(record.supports(Capability::Streaming));
        assert!(!record.supports(Capability::Vision));
    }
}
