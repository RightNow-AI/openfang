use std::collections::BTreeMap;

use openfang_types::ai::{AdapterId, CanonicalModelId, Capability};
use openfang_types::capabilities::{
    ModelCapabilityRecord, PriceClass, PrivacyClass, RuntimeType, SpeedClass,
};
use openfang_types::model_catalog::{AuthStatus, ModelCatalogEntry, ModelTier};

use crate::model_catalog::ModelCatalog;

#[derive(Default)]
pub struct CapabilityRegistry {
    by_model: BTreeMap<CanonicalModelId, ModelCapabilityRecord>,
    by_adapter: BTreeMap<AdapterId, Vec<CanonicalModelId>>,
}

impl CapabilityRegistry {
    pub fn insert(&mut self, record: ModelCapabilityRecord) {
        self.by_adapter
            .entry(record.adapter_id.clone())
            .or_default()
            .push(record.canonical_model_id.clone());
        self.by_model
            .insert(record.canonical_model_id.clone(), record);
    }

    pub fn get(&self, model_id: &CanonicalModelId) -> Option<&ModelCapabilityRecord> {
        self.by_model.get(model_id)
    }

    pub fn all(&self) -> impl Iterator<Item = &ModelCapabilityRecord> {
        self.by_model.values()
    }

    pub fn models_for_adapter(
        &self,
        adapter_id: &AdapterId,
    ) -> impl Iterator<Item = &ModelCapabilityRecord> {
        self.by_adapter
            .get(adapter_id)
            .into_iter()
            .flat_map(|ids| ids.iter())
            .filter_map(|id| self.by_model.get(id))
    }

    pub fn from_model_catalog(catalog: &ModelCatalog) -> Self {
        let mut registry = Self::default();
        for entry in catalog.list_models() {
            if let Some(record) = record_from_catalog(catalog, entry) {
                registry.insert(record);
            }
        }
        registry
    }
}

fn record_from_catalog(
    catalog: &ModelCatalog,
    entry: &ModelCatalogEntry,
) -> Option<ModelCapabilityRecord> {
    let provider = catalog.get_provider(&entry.provider)?;
    let adapter_id = match entry.provider.as_str() {
        "ollama" => AdapterId::Ollama,
        "openrouter" => AdapterId::OpenRouter,
        _ => return None,
    };
    let local = provider.auth_status == AuthStatus::NotRequired || matches!(entry.tier, ModelTier::Local);
    let browser = matches!(adapter_id, AdapterId::BrowserWorker);

    Some(ModelCapabilityRecord {
        adapter_id,
        canonical_model_id: CanonicalModelId(canonical_id(entry)),
        provider_model_id: entry.id.clone(),
        family: infer_family(entry),
        runtime_type: if local {
            RuntimeType::LocalHost
        } else {
            RuntimeType::RemoteApi
        },
        local,
        browser,
        supported_capabilities: infer_capabilities(entry, local, browser),
        max_context_tokens: entry.context_window.min(u64::from(u32::MAX)) as u32,
        speed_class: infer_speed(entry),
        price_class: infer_price(entry),
        privacy_class: if browser {
            PrivacyClass::Device
        } else if local {
            PrivacyClass::Host
        } else {
            PrivacyClass::Remote
        },
        reliability_score_bps: infer_reliability(entry, local),
    })
}

fn canonical_id(entry: &ModelCatalogEntry) -> String {
    if entry.id.starts_with(&format!("{}/", entry.provider)) {
        entry.id.clone()
    } else {
        format!("{}/{}", entry.provider, entry.id)
    }
}

fn infer_family(entry: &ModelCatalogEntry) -> String {
    let lower = entry.id.to_lowercase();
    for family in ["qwen", "claude", "gpt", "llama", "gemma", "deepseek", "mistral", "phi"] {
        if lower.contains(family) {
            return family.to_string();
        }
    }
    entry.provider.to_lowercase()
}

fn infer_capabilities(
    entry: &ModelCatalogEntry,
    local: bool,
    browser: bool,
) -> Vec<Capability> {
    let mut capabilities = vec![];
    if entry.supports_streaming {
        capabilities.push(Capability::Streaming);
    }
    if entry.supports_tools {
        capabilities.push(Capability::Tools);
        capabilities.push(Capability::JsonMode);
    }
    if entry.supports_vision {
        capabilities.push(Capability::Vision);
    }
    if entry.context_window >= 128_000 {
        capabilities.push(Capability::LongContext);
    }
    if local {
        capabilities.push(Capability::LocalExecution);
    } else {
        capabilities.push(Capability::RemoteExecution);
    }
    if browser {
        capabilities.push(Capability::LowLatency);
    }
    if matches!(entry.tier, ModelTier::Frontier | ModelTier::Smart) {
        capabilities.push(Capability::ReasoningStrong);
    }
    capabilities
}

fn infer_speed(entry: &ModelCatalogEntry) -> SpeedClass {
    match entry.tier {
        ModelTier::Fast | ModelTier::Local => SpeedClass::Fast,
        ModelTier::Balanced | ModelTier::Smart => SpeedClass::Medium,
        ModelTier::Frontier | ModelTier::Custom => SpeedClass::Slow,
    }
}

fn infer_price(entry: &ModelCatalogEntry) -> PriceClass {
    let total = entry.input_cost_per_m + entry.output_cost_per_m;
    if total <= 0.5 {
        PriceClass::Low
    } else if total <= 5.0 {
        PriceClass::Medium
    } else {
        PriceClass::High
    }
}

fn infer_reliability(entry: &ModelCatalogEntry, local: bool) -> u16 {
    let base: u16 = match entry.tier {
        ModelTier::Frontier => 9300,
        ModelTier::Smart => 9000,
        ModelTier::Balanced => 8500,
        ModelTier::Fast => 8000,
        ModelTier::Local => 7800,
        ModelTier::Custom => 7000,
    };
    if local {
        base.saturating_add(100)
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_imports_ollama_and_openrouter_records() {
        let mut catalog = ModelCatalog::new();
        catalog.detect_auth();
        let registry = CapabilityRegistry::from_model_catalog(&catalog);

        assert!(registry.models_for_adapter(&AdapterId::Ollama).next().is_some());
        assert!(registry.models_for_adapter(&AdapterId::OpenRouter).next().is_some());
    }
}
