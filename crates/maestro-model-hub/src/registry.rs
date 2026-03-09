//! Pre-populated model registry with real model capabilities and pricing.
//!
//! Prices are USD per 1M tokens as of Q1 2025. Update regularly.
//! Scores are based on public benchmarks (MMLU, HumanEval, etc.)
//! and are subjective — configure for your use case.

use crate::{CapabilityScores, ModelCapabilities, ModelHub};

/// Build a `ModelHub` pre-populated with well-known models.
pub fn default_registry() -> ModelHub {
    let hub = ModelHub::new();

    // ── OpenAI ──────────────────────────────────────────────────────────────

    hub.register(ModelCapabilities {
        model_id: "gpt-4o".to_string(),
        provider: "openai".to_string(),
        max_context: 128_000,
        max_output: 4_096,
        supports_tools: true,
        supports_vision: true,
        supports_streaming: true,
        supports_json_mode: true,
        cost_per_1m_input: 2.50,
        cost_per_1m_output: 10.00,
        scores: CapabilityScores {
            coding: 0.92,
            reasoning: 0.94,
            creative_writing: 0.88,
            data_analysis: 0.91,
            instruction_following: 0.95,
            multilingual: 0.89,
            long_context: 0.88,
            speed: 0.75,
        },
    });

    hub.register(ModelCapabilities {
        model_id: "gpt-4o-mini".to_string(),
        provider: "openai".to_string(),
        max_context: 128_000,
        max_output: 16_384,
        supports_tools: true,
        supports_vision: true,
        supports_streaming: true,
        supports_json_mode: true,
        cost_per_1m_input: 0.15,
        cost_per_1m_output: 0.60,
        scores: CapabilityScores {
            coding: 0.80,
            reasoning: 0.82,
            creative_writing: 0.78,
            data_analysis: 0.80,
            instruction_following: 0.88,
            multilingual: 0.82,
            long_context: 0.80,
            speed: 0.92,
        },
    });

    hub.register(ModelCapabilities {
        model_id: "gpt-4.1".to_string(),
        provider: "openai".to_string(),
        max_context: 1_000_000,
        max_output: 32_768,
        supports_tools: true,
        supports_vision: true,
        supports_streaming: true,
        supports_json_mode: true,
        cost_per_1m_input: 2.00,
        cost_per_1m_output: 8.00,
        scores: CapabilityScores {
            coding: 0.95,
            reasoning: 0.95,
            creative_writing: 0.90,
            data_analysis: 0.93,
            instruction_following: 0.96,
            multilingual: 0.91,
            long_context: 0.97,
            speed: 0.72,
        },
    });

    hub.register(ModelCapabilities {
        model_id: "gpt-4.1-mini".to_string(),
        provider: "openai".to_string(),
        max_context: 1_000_000,
        max_output: 32_768,
        supports_tools: true,
        supports_vision: true,
        supports_streaming: true,
        supports_json_mode: true,
        cost_per_1m_input: 0.40,
        cost_per_1m_output: 1.60,
        scores: CapabilityScores {
            coding: 0.85,
            reasoning: 0.86,
            creative_writing: 0.82,
            data_analysis: 0.84,
            instruction_following: 0.90,
            multilingual: 0.85,
            long_context: 0.90,
            speed: 0.90,
        },
    });

    // ── Anthropic ───────────────────────────────────────────────────────────

    hub.register(ModelCapabilities {
        model_id: "claude-3-5-sonnet-20241022".to_string(),
        provider: "anthropic".to_string(),
        max_context: 200_000,
        max_output: 8_192,
        supports_tools: true,
        supports_vision: true,
        supports_streaming: true,
        supports_json_mode: true,
        cost_per_1m_input: 3.00,
        cost_per_1m_output: 15.00,
        scores: CapabilityScores {
            coding: 0.95,
            reasoning: 0.95,
            creative_writing: 0.94,
            data_analysis: 0.92,
            instruction_following: 0.96,
            multilingual: 0.90,
            long_context: 0.93,
            speed: 0.73,
        },
    });

    hub.register(ModelCapabilities {
        model_id: "claude-3-5-haiku-20241022".to_string(),
        provider: "anthropic".to_string(),
        max_context: 200_000,
        max_output: 8_192,
        supports_tools: true,
        supports_vision: true,
        supports_streaming: true,
        supports_json_mode: true,
        cost_per_1m_input: 0.80,
        cost_per_1m_output: 4.00,
        scores: CapabilityScores {
            coding: 0.82,
            reasoning: 0.83,
            creative_writing: 0.82,
            data_analysis: 0.81,
            instruction_following: 0.88,
            multilingual: 0.84,
            long_context: 0.85,
            speed: 0.90,
        },
    });

    hub.register(ModelCapabilities {
        model_id: "claude-3-7-sonnet-20250219".to_string(),
        provider: "anthropic".to_string(),
        max_context: 200_000,
        max_output: 64_000,
        supports_tools: true,
        supports_vision: true,
        supports_streaming: true,
        supports_json_mode: true,
        cost_per_1m_input: 3.00,
        cost_per_1m_output: 15.00,
        scores: CapabilityScores {
            coding: 0.97,
            reasoning: 0.97,
            creative_writing: 0.95,
            data_analysis: 0.95,
            instruction_following: 0.97,
            multilingual: 0.92,
            long_context: 0.95,
            speed: 0.68,
        },
    });

    // ── Google ──────────────────────────────────────────────────────────────

    hub.register(ModelCapabilities {
        model_id: "gemini-2.0-flash".to_string(),
        provider: "google".to_string(),
        max_context: 1_000_000,
        max_output: 8_192,
        supports_tools: true,
        supports_vision: true,
        supports_streaming: true,
        supports_json_mode: true,
        cost_per_1m_input: 0.10,
        cost_per_1m_output: 0.40,
        scores: CapabilityScores {
            coding: 0.83,
            reasoning: 0.85,
            creative_writing: 0.80,
            data_analysis: 0.83,
            instruction_following: 0.87,
            multilingual: 0.90,
            long_context: 0.95,
            speed: 0.93,
        },
    });

    hub.register(ModelCapabilities {
        model_id: "gemini-2.5-pro".to_string(),
        provider: "google".to_string(),
        max_context: 1_000_000,
        max_output: 65_536,
        supports_tools: true,
        supports_vision: true,
        supports_streaming: true,
        supports_json_mode: true,
        cost_per_1m_input: 1.25,
        cost_per_1m_output: 10.00,
        scores: CapabilityScores {
            coding: 0.93,
            reasoning: 0.95,
            creative_writing: 0.88,
            data_analysis: 0.93,
            instruction_following: 0.94,
            multilingual: 0.93,
            long_context: 0.97,
            speed: 0.70,
        },
    });

    // ── Meta / Open Source ──────────────────────────────────────────────────

    hub.register(ModelCapabilities {
        model_id: "llama-3.3-70b-instruct".to_string(),
        provider: "groq".to_string(),
        max_context: 128_000,
        max_output: 32_768,
        supports_tools: true,
        supports_vision: false,
        supports_streaming: true,
        supports_json_mode: true,
        cost_per_1m_input: 0.59,
        cost_per_1m_output: 0.79,
        scores: CapabilityScores {
            coding: 0.83,
            reasoning: 0.84,
            creative_writing: 0.80,
            data_analysis: 0.82,
            instruction_following: 0.86,
            multilingual: 0.78,
            long_context: 0.80,
            speed: 0.95,
        },
    });

    hub.register(ModelCapabilities {
        model_id: "llama-3.1-8b-instant".to_string(),
        provider: "groq".to_string(),
        max_context: 128_000,
        max_output: 8_192,
        supports_tools: true,
        supports_vision: false,
        supports_streaming: true,
        supports_json_mode: true,
        cost_per_1m_input: 0.05,
        cost_per_1m_output: 0.08,
        scores: CapabilityScores {
            coding: 0.68,
            reasoning: 0.70,
            creative_writing: 0.65,
            data_analysis: 0.68,
            instruction_following: 0.75,
            multilingual: 0.65,
            long_context: 0.70,
            speed: 0.99,
        },
    });

    hub
}

/// Pre-defined fallback chains for common scenarios.
pub struct FallbackChains;

impl FallbackChains {
    /// Best quality → cost fallback for coding tasks.
    pub fn coding() -> Vec<String> {
        vec![
            "claude-3-7-sonnet-20250219".to_string(),
            "gpt-4.1".to_string(),
            "claude-3-5-sonnet-20241022".to_string(),
            "gpt-4o".to_string(),
            "gemini-2.5-pro".to_string(),
            "llama-3.3-70b-instruct".to_string(),
        ]
    }

    /// Best quality → cost fallback for reasoning tasks.
    pub fn reasoning() -> Vec<String> {
        vec![
            "claude-3-7-sonnet-20250219".to_string(),
            "gpt-4.1".to_string(),
            "gemini-2.5-pro".to_string(),
            "claude-3-5-sonnet-20241022".to_string(),
            "gpt-4o".to_string(),
            "llama-3.3-70b-instruct".to_string(),
        ]
    }

    /// Speed-optimized fallback chain.
    pub fn fast() -> Vec<String> {
        vec![
            "llama-3.1-8b-instant".to_string(),
            "gemini-2.0-flash".to_string(),
            "gpt-4o-mini".to_string(),
            "gpt-4.1-mini".to_string(),
            "claude-3-5-haiku-20241022".to_string(),
        ]
    }

    /// Cost-optimized fallback chain.
    pub fn budget() -> Vec<String> {
        vec![
            "llama-3.1-8b-instant".to_string(),
            "gemini-2.0-flash".to_string(),
            "gpt-4o-mini".to_string(),
            "gpt-4.1-mini".to_string(),
            "claude-3-5-haiku-20241022".to_string(),
            "llama-3.3-70b-instruct".to_string(),
        ]
    }

    /// Long-context fallback chain (1M+ token context).
    pub fn long_context() -> Vec<String> {
        vec![
            "gemini-2.5-pro".to_string(),
            "gemini-2.0-flash".to_string(),
            "gpt-4.1".to_string(),
            "gpt-4.1-mini".to_string(),
            "claude-3-7-sonnet-20250219".to_string(),
        ]
    }
}
