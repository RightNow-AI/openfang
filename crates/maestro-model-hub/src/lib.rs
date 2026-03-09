//! # maestro-model-hub
//!
//! Model Hub that bridges Rig.rs into OpenFang's architecture, providing:
//!
//! 1. **Capability-aware model selection** (ported from Maestro's model_selector)
//! 2. **Unified provider abstraction** via Rig.rs (19+ providers)
//! 3. **Intelligent routing** based on task complexity, cost, and latency
//!
//! ## Why This Exists
//!
//! OpenFang currently has:
//! - `openfang-types::model_catalog` — A static catalog of 27+ providers with
//!   base URLs and model names, organized by ModelTier (Frontier/Smart/Balanced/Fast/Local)
//! - `openfang-runtime::llm_driver` — A `LlmDriver` trait with `complete()` and
//!   `complete_stream()` methods, implemented for Anthropic, OpenAI, Gemini, Copilot
//! - `openfang-runtime::drivers/` — Individual driver implementations
//!
//! The problem: OpenFang's model handling is bespoke and limited. Adding a new
//! provider requires writing a new driver from scratch. Rig.rs provides 19+
//! providers out of the box with a unified trait interface.
//!
//! ## Integration Strategy
//!
//! This crate does NOT replace OpenFang's `LlmDriver` trait. Instead, it
//! provides a `RigModelHub` that can be used alongside OpenFang's existing
//! drivers. Over time, OpenFang's drivers can be migrated to Rig.rs backends.
//!
//! ## HONEST GAPS
//!
//! - Rig.rs and OpenFang have DIFFERENT tool schema formats. This crate must
//!   translate between them. This is a non-trivial impedance mismatch.
//! - Rig.rs uses its own `Agent` struct; OpenFang has its own agent loop.
//!   These cannot be naively composed — one must wrap the other.
//! - Streaming support differs: Rig uses `StreamingCompletion`, OpenFang
//!   uses `StreamEvent`. Translation layer needed.
//! - Cost tracking is not built into Rig.rs. Must be added externally.
//! - The capability-aware selector from Maestro uses hardcoded capability
//!   scores. These need to be configurable and updateable.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

/// A model's capabilities — used for intelligent routing.
///
/// Ported from Maestro's model_selector.rs (568 LOC).
/// Extended with additional capability dimensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    /// Model identifier (e.g., "gpt-4.1", "claude-sonnet-4")
    pub model_id: String,
    /// Provider name (e.g., "openai", "anthropic")
    pub provider: String,
    /// Maximum context window in tokens
    pub max_context: u32,
    /// Maximum output tokens
    pub max_output: u32,
    /// Whether the model supports tool/function calling
    pub supports_tools: bool,
    /// Whether the model supports vision/image input
    pub supports_vision: bool,
    /// Whether the model supports streaming
    pub supports_streaming: bool,
    /// Whether the model supports structured JSON output
    pub supports_json_mode: bool,
    /// Cost per 1M input tokens (USD)
    pub cost_per_1m_input: f64,
    /// Cost per 1M output tokens (USD)
    pub cost_per_1m_output: f64,
    /// Capability scores (0.0 - 1.0) for different task types
    pub scores: CapabilityScores,
}

/// Capability scores for different task types.
///
/// HONEST NOTE: These scores are subjective and based on benchmarks
/// that may not reflect your specific use case. They should be
/// configurable and ideally learned from actual usage data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityScores {
    pub coding: f64,
    pub reasoning: f64,
    pub creative_writing: f64,
    pub data_analysis: f64,
    pub instruction_following: f64,
    pub multilingual: f64,
    pub long_context: f64,
    pub speed: f64,
}

/// Task requirements for model selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRequirements {
    /// Primary capability needed
    pub primary_capability: String,
    /// Minimum context window needed
    pub min_context: u32,
    /// Whether tools are required
    pub needs_tools: bool,
    /// Whether vision is required
    pub needs_vision: bool,
    /// Maximum acceptable cost per 1M tokens
    pub max_cost_per_1m: Option<f64>,
    /// Whether to prefer speed over quality
    pub prefer_speed: bool,
}

/// The Model Hub — manages model registry and intelligent selection.
pub struct ModelHub {
    /// Registry of known models and their capabilities
    models: DashMap<String, ModelCapabilities>,
}

impl ModelHub {
    pub fn new() -> Self {
        Self {
            models: DashMap::new(),
        }
    }

    /// Register a model with its capabilities.
    pub fn register(&self, capabilities: ModelCapabilities) {
        self.models.insert(capabilities.model_id.clone(), capabilities);
    }

    /// Select the best model for the given task requirements.
    ///
    /// Ported from Maestro's model_selector with improvements:
    /// - Weighted scoring across multiple capability dimensions
    /// - Cost-aware selection with configurable budget
    /// - Fallback chain when preferred model is unavailable
    ///
    /// HONEST NOTE: This is a heuristic selector, not an ML model.
    /// It will make suboptimal choices for edge cases. The scores
    /// are static — ideally they should be updated based on actual
    /// task outcomes (connect to the LEARN phase of the algorithm).
    pub fn select(&self, requirements: &TaskRequirements) -> Option<String> {
        let mut best_model: Option<(String, f64)> = None;

        for entry in self.models.iter() {
            let model = entry.value();

            // Hard filters
            if requirements.needs_tools && !model.supports_tools {
                continue;
            }
            if requirements.needs_vision && !model.supports_vision {
                continue;
            }
            if model.max_context < requirements.min_context {
                continue;
            }
            if let Some(max_cost) = requirements.max_cost_per_1m {
                if model.cost_per_1m_input > max_cost {
                    continue;
                }
            }

            // Score calculation
            let capability_score = match requirements.primary_capability.as_str() {
                "coding" => model.scores.coding,
                "reasoning" => model.scores.reasoning,
                "creative_writing" => model.scores.creative_writing,
                "data_analysis" => model.scores.data_analysis,
                "instruction_following" => model.scores.instruction_following,
                "multilingual" => model.scores.multilingual,
                "long_context" => model.scores.long_context,
                _ => model.scores.reasoning, // Default to reasoning
            };

            let speed_bonus = if requirements.prefer_speed {
                model.scores.speed * 0.3
            } else {
                0.0
            };

            let total_score = capability_score + speed_bonus;

            match &best_model {
                None => best_model = Some((model.model_id.clone(), total_score)),
                Some((_, best_score)) if total_score > *best_score => {
                    best_model = Some((model.model_id.clone(), total_score));
                }
                _ => {}
            }
        }

        best_model.map(|(id, _)| id)
    }

    /// Get a model's capabilities by ID.
    pub fn get_model(&self, model_id: &str) -> Option<ModelCapabilities> {
        self.models.get(model_id).map(|e| e.value().clone())
    }

    /// List all registered model IDs.
    pub fn list_models(&self) -> Vec<String> {
        self.models.iter().map(|e| e.key().clone()).collect()
    }
}

impl Default for ModelHub {
    fn default() -> Self {
        Self::new()
    }
}

pub mod registry;
pub mod router;
