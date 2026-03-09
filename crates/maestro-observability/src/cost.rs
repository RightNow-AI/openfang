//! Cost tracking — model pricing catalog and cost estimation.
//!
//! Maintains a pricing catalog for all supported models and provides
//! cost estimation before and after LLM calls. Integrates with the
//! `MetricsStore` for per-agent cost dashboards.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::warn;

/// Pricing for a single model (per 1M tokens).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub model_id: String,
    /// Cost per 1M input tokens in USD.
    pub input_per_1m_usd: f64,
    /// Cost per 1M output tokens in USD.
    pub output_per_1m_usd: f64,
}

impl ModelPricing {
    /// Calculate cost for a given token usage.
    pub fn calculate(&self, input_tokens: u64, output_tokens: u64) -> f64 {
        let input_cost = (input_tokens as f64 / 1_000_000.0) * self.input_per_1m_usd;
        let output_cost = (output_tokens as f64 / 1_000_000.0) * self.output_per_1m_usd;
        input_cost + output_cost
    }
}

/// The cost tracker — maintains a pricing catalog and computes costs.
#[derive(Clone)]
pub struct CostTracker {
    /// model_id → pricing
    catalog: Arc<DashMap<String, ModelPricing>>,
}

impl CostTracker {
    /// Create a new cost tracker with default pricing for common models.
    pub fn new() -> Self {
        let tracker = Self {
            catalog: Arc::new(DashMap::new()),
        };
        tracker.load_defaults();
        tracker
    }

    /// Load default pricing for well-known models (as of 2025).
    fn load_defaults(&self) {
        let defaults = vec![
            // OpenAI
            ModelPricing { model_id: "gpt-4o".into(), input_per_1m_usd: 2.50, output_per_1m_usd: 10.00 },
            ModelPricing { model_id: "gpt-4o-mini".into(), input_per_1m_usd: 0.15, output_per_1m_usd: 0.60 },
            ModelPricing { model_id: "gpt-4.1".into(), input_per_1m_usd: 2.00, output_per_1m_usd: 8.00 },
            ModelPricing { model_id: "gpt-4.1-mini".into(), input_per_1m_usd: 0.40, output_per_1m_usd: 1.60 },
            ModelPricing { model_id: "gpt-4.1-nano".into(), input_per_1m_usd: 0.10, output_per_1m_usd: 0.40 },
            ModelPricing { model_id: "o3".into(), input_per_1m_usd: 10.00, output_per_1m_usd: 40.00 },
            ModelPricing { model_id: "o4-mini".into(), input_per_1m_usd: 1.10, output_per_1m_usd: 4.40 },
            // Anthropic
            ModelPricing { model_id: "claude-opus-4-5".into(), input_per_1m_usd: 15.00, output_per_1m_usd: 75.00 },
            ModelPricing { model_id: "claude-sonnet-4-5".into(), input_per_1m_usd: 3.00, output_per_1m_usd: 15.00 },
            ModelPricing { model_id: "claude-haiku-3-5".into(), input_per_1m_usd: 0.80, output_per_1m_usd: 4.00 },
            // Google
            ModelPricing { model_id: "gemini-2.5-pro".into(), input_per_1m_usd: 1.25, output_per_1m_usd: 10.00 },
            ModelPricing { model_id: "gemini-2.5-flash".into(), input_per_1m_usd: 0.15, output_per_1m_usd: 0.60 },
            ModelPricing { model_id: "gemini-2.0-flash".into(), input_per_1m_usd: 0.10, output_per_1m_usd: 0.40 },
            // Meta / open source (self-hosted, near-zero cost)
            ModelPricing { model_id: "llama-3.3-70b".into(), input_per_1m_usd: 0.23, output_per_1m_usd: 0.40 },
            ModelPricing { model_id: "llama-3.1-8b".into(), input_per_1m_usd: 0.05, output_per_1m_usd: 0.08 },
            // Mistral
            ModelPricing { model_id: "mistral-large".into(), input_per_1m_usd: 2.00, output_per_1m_usd: 6.00 },
            ModelPricing { model_id: "mistral-small".into(), input_per_1m_usd: 0.10, output_per_1m_usd: 0.30 },
        ];
        for pricing in defaults {
            self.catalog.insert(pricing.model_id.clone(), pricing);
        }
    }

    /// Calculate cost for a completed LLM call.
    ///
    /// Returns 0.0 if the model is not in the catalog (with a warning).
    pub fn calculate(&self, model_id: &str, input_tokens: u64, output_tokens: u64) -> f64 {
        if let Some(pricing) = self.catalog.get(model_id) {
            pricing.calculate(input_tokens, output_tokens)
        } else {
            // Try prefix matching (e.g., "gpt-4o-2024-11-20" → "gpt-4o")
            let prefix_match = self.catalog.iter().find(|entry| {
                model_id.starts_with(entry.key().as_str())
            });
            if let Some(entry) = prefix_match {
                entry.calculate(input_tokens, output_tokens)
            } else {
                warn!(model_id = %model_id, "No pricing found for model, cost will be 0.0");
                0.0
            }
        }
    }

    /// Estimate cost before a call (using expected token counts).
    pub fn estimate(&self, model_id: &str, expected_input: u64, expected_output: u64) -> f64 {
        self.calculate(model_id, expected_input, expected_output)
    }

    /// Add or update pricing for a model.
    pub fn upsert(&self, pricing: ModelPricing) {
        self.catalog.insert(pricing.model_id.clone(), pricing);
    }

    /// Get pricing for a specific model.
    pub fn get(&self, model_id: &str) -> Option<ModelPricing> {
        self.catalog.get(model_id).map(|e| e.clone())
    }

    /// Get all models in the catalog.
    pub fn all_models(&self) -> Vec<ModelPricing> {
        self.catalog.iter().map(|e| e.clone()).collect()
    }

    /// Get the cheapest model for a given capability tier.
    pub fn cheapest_for_tier(&self, tier: ModelTier) -> Option<ModelPricing> {
        let candidates: Vec<&str> = match tier {
            ModelTier::Fast => vec![
                "gpt-4.1-nano", "gpt-4o-mini", "gemini-2.0-flash",
                "mistral-small", "llama-3.1-8b",
            ],
            ModelTier::Balanced => vec![
                "gpt-4.1-mini", "gemini-2.5-flash", "claude-haiku-3-5",
                "mistral-small", "llama-3.3-70b",
            ],
            ModelTier::Powerful => vec![
                "gpt-4.1", "gpt-4o", "claude-sonnet-4-5",
                "gemini-2.5-pro", "mistral-large",
            ],
            ModelTier::Frontier => vec![
                "o3", "claude-opus-4-5", "gemini-2.5-pro",
            ],
        };
        candidates
            .iter()
            .filter_map(|id| self.catalog.get(*id).map(|e| e.clone()))
            .min_by(|a, b| {
                let cost_a = a.input_per_1m_usd + a.output_per_1m_usd;
                let cost_b = b.input_per_1m_usd + b.output_per_1m_usd;
                cost_a.partial_cmp(&cost_b).unwrap()
            })
    }
}

impl Default for CostTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Model capability tiers for cost-aware routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelTier {
    /// Fast, cheap models for simple tasks.
    Fast,
    /// Balanced models for most tasks.
    Balanced,
    /// Powerful models for complex reasoning.
    Powerful,
    /// Frontier models for the hardest tasks.
    Frontier,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cost_calculation() {
        let tracker = CostTracker::new();
        // gpt-4o: $2.50/1M input, $10.00/1M output
        let cost = tracker.calculate("gpt-4o", 1_000, 500);
        let expected = (1_000.0 / 1_000_000.0) * 2.50 + (500.0 / 1_000_000.0) * 10.00;
        assert!((cost - expected).abs() < 1e-9);
    }

    #[test]
    fn test_prefix_matching() {
        let tracker = CostTracker::new();
        // "gpt-4o-2024-11-20" should match "gpt-4o"
        let cost = tracker.calculate("gpt-4o-2024-11-20", 1_000, 500);
        assert!(cost > 0.0);
    }

    #[test]
    fn test_unknown_model_returns_zero() {
        let tracker = CostTracker::new();
        let cost = tracker.calculate("unknown-model-xyz", 1_000, 500);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn test_cheapest_for_tier() {
        let tracker = CostTracker::new();
        let cheapest = tracker.cheapest_for_tier(ModelTier::Fast);
        assert!(cheapest.is_some());
    }
}
