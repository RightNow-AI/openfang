//! LLM driver implementations.
//!
//! Contains drivers for Anthropic Claude, Google Gemini, OpenAI-compatible APIs, and more.
//! Supports: Anthropic, Gemini, OpenAI, Groq, OpenRouter, DeepSeek, Together,
//! Mistral, Fireworks, Ollama, vLLM, and any OpenAI-compatible endpoint.

pub mod anthropic;
pub mod claude_code;
pub mod copilot;
pub mod fallback;
pub mod gemini;
pub mod openai;

use crate::llm_driver::{DriverConfig, LlmDriver, LlmError};
use openfang_types::model_catalog::{
    AI21_BASE_URL, ANTHROPIC_BASE_URL, CEREBRAS_BASE_URL, COHERE_BASE_URL, DEEPSEEK_BASE_URL,
    FIREWORKS_BASE_URL, GEMINI_BASE_URL, GROQ_BASE_URL, HUGGINGFACE_BASE_URL, LMSTUDIO_BASE_URL,
    MINIMAX_BASE_URL, MISTRAL_BASE_URL, MOONSHOT_BASE_URL, OLLAMA_BASE_URL, OPENAI_BASE_URL,
    OPENROUTER_BASE_URL, PERPLEXITY_BASE_URL, QIANFAN_BASE_URL, QWEN_BASE_URL,
    REPLICATE_BASE_URL, SAMBANOVA_BASE_URL, TOGETHER_BASE_URL, VLLM_BASE_URL, XAI_BASE_URL,
    ZHIPU_BASE_URL, ZHIPU_CODING_BASE_URL,
};
use std::sync::Arc;

/// Provider metadata: base URL and env var name for the API key.
struct ProviderDefaults {
    base_url: &'static str,
    api_key_env: &'static str,
    /// If true, the API key is required (error if missing).
    key_required: bool,
}

/// Get defaults for known providers.
fn provider_defaults(provider: &str) -> Option<ProviderDefaults> {
    match provider {
        "groq" => Some(ProviderDefaults {
            base_url: GROQ_BASE_URL,
            api_key_env: "GROQ_API_KEY",
            key_required: true,
        }),
        "openrouter" => Some(ProviderDefaults {
            base_url: OPENROUTER_BASE_URL,
            api_key_env: "OPENROUTER_API_KEY",
            key_required: true,
        }),
        "deepseek" => Some(ProviderDefaults {
            base_url: DEEPSEEK_BASE_URL,
            api_key_env: "DEEPSEEK_API_KEY",
            key_required: true,
        }),
        "together" => Some(ProviderDefaults {
            base_url: TOGETHER_BASE_URL,
            api_key_env: "TOGETHER_API_KEY",
            key_required: true,
        }),
        "mistral" => Some(ProviderDefaults {
            base_url: MISTRAL_BASE_URL,
            api_key_env: "MISTRAL_API_KEY",
            key_required: true,
        }),
        "fireworks" => Some(ProviderDefaults {
            base_url: FIREWORKS_BASE_URL,
            api_key_env: "FIREWORKS_API_KEY",
            key_required: true,
        }),
        "openai" => Some(ProviderDefaults {
            base_url: OPENAI_BASE_URL,
            api_key_env: "OPENAI_API_KEY",
            key_required: true,
        }),
        "gemini" | "google" => Some(ProviderDefaults {
            base_url: GEMINI_BASE_URL,
            api_key_env: "GEMINI_API_KEY",
            key_required: true,
        }),
        "ollama" => Some(ProviderDefaults {
            base_url: OLLAMA_BASE_URL,
            api_key_env: "OLLAMA_API_KEY",
            key_required: false,
        }),
        "vllm" => Some(ProviderDefaults {
            base_url: VLLM_BASE_URL,
            api_key_env: "VLLM_API_KEY",
            key_required: false,
        }),
        "lmstudio" => Some(ProviderDefaults {
            base_url: LMSTUDIO_BASE_URL,
            api_key_env: "LMSTUDIO_API_KEY",
            key_required: false,
        }),
        "perplexity" => Some(ProviderDefaults {
            base_url: PERPLEXITY_BASE_URL,
            api_key_env: "PERPLEXITY_API_KEY",
            key_required: true,
        }),
        "cohere" => Some(ProviderDefaults {
            base_url: COHERE_BASE_URL,
            api_key_env: "COHERE_API_KEY",
            key_required: true,
        }),
        "ai21" => Some(ProviderDefaults {
            base_url: AI21_BASE_URL,
            api_key_env: "AI21_API_KEY",
            key_required: true,
        }),
        "cerebras" => Some(ProviderDefaults {
            base_url: CEREBRAS_BASE_URL,
            api_key_env: "CEREBRAS_API_KEY",
            key_required: true,
        }),
        "sambanova" => Some(ProviderDefaults {
            base_url: SAMBANOVA_BASE_URL,
            api_key_env: "SAMBANOVA_API_KEY",
            key_required: true,
        }),
        "huggingface" => Some(ProviderDefaults {
            base_url: HUGGINGFACE_BASE_URL,
            api_key_env: "HF_API_KEY",
            key_required: true,
        }),
        "xai" => Some(ProviderDefaults {
            base_url: XAI_BASE_URL,
            api_key_env: "XAI_API_KEY",
            key_required: true,
        }),
        "replicate" => Some(ProviderDefaults {
            base_url: REPLICATE_BASE_URL,
            api_key_env: "REPLICATE_API_TOKEN",
            key_required: true,
        }),
        "github-copilot" | "copilot" => Some(ProviderDefaults {
            base_url: copilot::GITHUB_COPILOT_BASE_URL,
            api_key_env: "GITHUB_TOKEN",
            key_required: true,
        }),
        "codex" | "openai-codex" => Some(ProviderDefaults {
            base_url: OPENAI_BASE_URL,
            api_key_env: "OPENAI_API_KEY",
            key_required: true,
        }),
        "claude-code" => Some(ProviderDefaults {
            base_url: "",
            api_key_env: "",
            key_required: false,
        }),
        "moonshot" | "kimi" => Some(ProviderDefaults {
            base_url: MOONSHOT_BASE_URL,
            api_key_env: "MOONSHOT_API_KEY",
            key_required: true,
        }),
        "qwen" | "dashscope" => Some(ProviderDefaults {
            base_url: QWEN_BASE_URL,
            api_key_env: "DASHSCOPE_API_KEY",
            key_required: true,
        }),
        "minimax" => Some(ProviderDefaults {
            base_url: MINIMAX_BASE_URL,
            api_key_env: "MINIMAX_API_KEY",
            key_required: true,
        }),
        "zhipu" | "glm" => Some(ProviderDefaults {
            base_url: ZHIPU_BASE_URL,
            api_key_env: "ZHIPU_API_KEY",
            key_required: true,
        }),
        "zhipu_coding" | "codegeex" => Some(ProviderDefaults {
            base_url: ZHIPU_CODING_BASE_URL,
            api_key_env: "ZHIPU_API_KEY",
            key_required: true,
        }),
        "qianfan" | "baidu" => Some(ProviderDefaults {
            base_url: QIANFAN_BASE_URL,
            api_key_env: "QIANFAN_API_KEY",
            key_required: true,
        }),
        _ => None,
    }
}

/// Create an LLM driver based on provider name and configuration.
///
/// Supported providers:
/// - `anthropic` — Anthropic Claude (Messages API)
/// - `openai` — OpenAI GPT models
/// - `groq` — Groq (ultra-fast inference)
/// - `openrouter` — OpenRouter (multi-model gateway)
/// - `deepseek` — DeepSeek
/// - `together` — Together AI
/// - `mistral` — Mistral AI
/// - `fireworks` — Fireworks AI
/// - `ollama` — Ollama (local)
/// - `vllm` — vLLM (local)
/// - `lmstudio` — LM Studio (local)
/// - `perplexity` — Perplexity AI (search-augmented)
/// - `cohere` — Cohere (Command R)
/// - `ai21` — AI21 Labs (Jamba)
/// - `cerebras` — Cerebras (ultra-fast inference)
/// - `sambanova` — SambaNova
/// - `huggingface` — Hugging Face Inference API
/// - `xai` — xAI (Grok)
/// - `replicate` — Replicate
/// - Any custom provider with `base_url` set uses OpenAI-compatible format
pub fn create_driver(config: &DriverConfig) -> Result<Arc<dyn LlmDriver>, LlmError> {
    let provider = config.provider.as_str();

    // Anthropic uses a different API format — special case
    if provider == "anthropic" {
        let api_key = config
            .api_key
            .clone()
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
            .ok_or_else(|| {
                LlmError::MissingApiKey("Set ANTHROPIC_API_KEY environment variable".to_string())
            })?;
        let base_url = config
            .base_url
            .clone()
            .unwrap_or_else(|| ANTHROPIC_BASE_URL.to_string());
        return Ok(Arc::new(anthropic::AnthropicDriver::new(api_key, base_url)));
    }

    // Gemini uses a different API format — special case
    if provider == "gemini" || provider == "google" {
        let api_key = config
            .api_key
            .clone()
            .or_else(|| std::env::var("GEMINI_API_KEY").ok())
            .or_else(|| std::env::var("GOOGLE_API_KEY").ok())
            .ok_or_else(|| {
                LlmError::MissingApiKey(
                    "Set GEMINI_API_KEY or GOOGLE_API_KEY environment variable".to_string(),
                )
            })?;
        let base_url = config
            .base_url
            .clone()
            .unwrap_or_else(|| GEMINI_BASE_URL.to_string());
        return Ok(Arc::new(gemini::GeminiDriver::new(api_key, base_url)));
    }

    // Codex — reuses OpenAI driver with credential sync from Codex CLI
    if provider == "codex" || provider == "openai-codex" {
        let api_key = config
            .api_key
            .clone()
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .or_else(crate::model_catalog::read_codex_credential)
            .ok_or_else(|| {
                LlmError::MissingApiKey(
                    "Set OPENAI_API_KEY or install Codex CLI".to_string(),
                )
            })?;
        let base_url = config
            .base_url
            .clone()
            .unwrap_or_else(|| OPENAI_BASE_URL.to_string());
        return Ok(Arc::new(openai::OpenAIDriver::new(api_key, base_url)));
    }

    // Claude Code CLI — subprocess-based, no API key needed
    if provider == "claude-code" {
        let cli_path = config.base_url.clone();
        return Ok(Arc::new(claude_code::ClaudeCodeDriver::new(cli_path)));
    }

    // GitHub Copilot — wraps OpenAI-compatible driver with automatic token exchange.
    // The CopilotDriver exchanges the GitHub PAT for a Copilot API token on demand,
    // caches it, and refreshes when expired.
    if provider == "github-copilot" || provider == "copilot" {
        let github_token = config
            .api_key
            .clone()
            .or_else(|| std::env::var("GITHUB_TOKEN").ok())
            .ok_or_else(|| {
                LlmError::MissingApiKey(
                    "Set GITHUB_TOKEN environment variable for GitHub Copilot".to_string(),
                )
            })?;
        let base_url = config
            .base_url
            .clone()
            .unwrap_or_else(|| copilot::GITHUB_COPILOT_BASE_URL.to_string());
        return Ok(Arc::new(copilot::CopilotDriver::new(
            github_token,
            base_url,
        )));
    }

    // All other providers use OpenAI-compatible format
    if let Some(defaults) = provider_defaults(provider) {
        let api_key = config
            .api_key
            .clone()
            .or_else(|| std::env::var(defaults.api_key_env).ok())
            .unwrap_or_default();

        if defaults.key_required && api_key.is_empty() {
            return Err(LlmError::MissingApiKey(format!(
                "Set {} environment variable for provider '{}'",
                defaults.api_key_env, provider
            )));
        }

        let base_url = config
            .base_url
            .clone()
            .unwrap_or_else(|| defaults.base_url.to_string());

        return Ok(Arc::new(openai::OpenAIDriver::new(api_key, base_url)));
    }

    // Unknown provider — if base_url is set, treat as custom OpenAI-compatible
    if let Some(ref base_url) = config.base_url {
        let api_key = config.api_key.clone().unwrap_or_default();
        return Ok(Arc::new(openai::OpenAIDriver::new(
            api_key,
            base_url.clone(),
        )));
    }

    Err(LlmError::Api {
        status: 0,
        message: format!(
            "Unknown provider '{}'. Supported: anthropic, gemini, openai, groq, openrouter, \
             deepseek, together, mistral, fireworks, ollama, vllm, lmstudio, perplexity, \
             cohere, ai21, cerebras, sambanova, huggingface, xai, replicate, github-copilot, \
             codex, claude-code. Or set base_url for a custom OpenAI-compatible endpoint.",
            provider
        ),
    })
}

/// List all known provider names.
pub fn known_providers() -> &'static [&'static str] {
    &[
        "anthropic",
        "gemini",
        "openai",
        "groq",
        "openrouter",
        "deepseek",
        "together",
        "mistral",
        "fireworks",
        "ollama",
        "vllm",
        "lmstudio",
        "perplexity",
        "cohere",
        "ai21",
        "cerebras",
        "sambanova",
        "huggingface",
        "xai",
        "replicate",
        "github-copilot",
        "moonshot",
        "qwen",
        "minimax",
        "zhipu",
        "zhipu_coding",
        "qianfan",
        "codex",
        "claude-code",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_defaults_groq() {
        let d = provider_defaults("groq").unwrap();
        assert_eq!(d.base_url, "https://api.groq.com/openai/v1");
        assert_eq!(d.api_key_env, "GROQ_API_KEY");
        assert!(d.key_required);
    }

    #[test]
    fn test_provider_defaults_openrouter() {
        let d = provider_defaults("openrouter").unwrap();
        assert_eq!(d.base_url, "https://openrouter.ai/api/v1");
        assert!(d.key_required);
    }

    #[test]
    fn test_provider_defaults_ollama() {
        let d = provider_defaults("ollama").unwrap();
        assert!(!d.key_required);
    }

    #[test]
    fn test_unknown_provider_returns_none() {
        assert!(provider_defaults("nonexistent").is_none());
    }

    #[test]
    fn test_custom_provider_with_base_url() {
        let config = DriverConfig {
            provider: "my-custom-llm".to_string(),
            api_key: Some("test".to_string()),
            base_url: Some("http://localhost:9999/v1".to_string()),
        };
        let driver = create_driver(&config);
        assert!(driver.is_ok());
    }

    #[test]
    fn test_unknown_provider_no_url_errors() {
        let config = DriverConfig {
            provider: "nonexistent".to_string(),
            api_key: None,
            base_url: None,
        };
        let driver = create_driver(&config);
        assert!(driver.is_err());
    }

    #[test]
    fn test_provider_defaults_gemini() {
        let d = provider_defaults("gemini").unwrap();
        assert_eq!(d.base_url, "https://generativelanguage.googleapis.com");
        assert_eq!(d.api_key_env, "GEMINI_API_KEY");
        assert!(d.key_required);
    }

    #[test]
    fn test_provider_defaults_google_alias() {
        let d = provider_defaults("google").unwrap();
        assert_eq!(d.base_url, "https://generativelanguage.googleapis.com");
        assert!(d.key_required);
    }

    #[test]
    fn test_known_providers_list() {
        let providers = known_providers();
        assert!(providers.contains(&"groq"));
        assert!(providers.contains(&"openrouter"));
        assert!(providers.contains(&"anthropic"));
        assert!(providers.contains(&"gemini"));
        // New providers
        assert!(providers.contains(&"perplexity"));
        assert!(providers.contains(&"cohere"));
        assert!(providers.contains(&"ai21"));
        assert!(providers.contains(&"cerebras"));
        assert!(providers.contains(&"sambanova"));
        assert!(providers.contains(&"huggingface"));
        assert!(providers.contains(&"xai"));
        assert!(providers.contains(&"replicate"));
        assert!(providers.contains(&"github-copilot"));
        assert!(providers.contains(&"moonshot"));
        assert!(providers.contains(&"qwen"));
        assert!(providers.contains(&"minimax"));
        assert!(providers.contains(&"zhipu"));
        assert!(providers.contains(&"zhipu_coding"));
        assert!(providers.contains(&"qianfan"));
        assert!(providers.contains(&"codex"));
        assert!(providers.contains(&"claude-code"));
        assert_eq!(providers.len(), 29);
    }

    #[test]
    fn test_provider_defaults_perplexity() {
        let d = provider_defaults("perplexity").unwrap();
        assert_eq!(d.base_url, "https://api.perplexity.ai");
        assert_eq!(d.api_key_env, "PERPLEXITY_API_KEY");
        assert!(d.key_required);
    }

    #[test]
    fn test_provider_defaults_xai() {
        let d = provider_defaults("xai").unwrap();
        assert_eq!(d.base_url, "https://api.x.ai/v1");
        assert_eq!(d.api_key_env, "XAI_API_KEY");
        assert!(d.key_required);
    }

    #[test]
    fn test_provider_defaults_cohere() {
        let d = provider_defaults("cohere").unwrap();
        assert_eq!(d.base_url, "https://api.cohere.com/v2");
        assert!(d.key_required);
    }

    #[test]
    fn test_provider_defaults_cerebras() {
        let d = provider_defaults("cerebras").unwrap();
        assert_eq!(d.base_url, "https://api.cerebras.ai/v1");
        assert!(d.key_required);
    }

    #[test]
    fn test_provider_defaults_huggingface() {
        let d = provider_defaults("huggingface").unwrap();
        assert_eq!(d.base_url, "https://api-inference.huggingface.co/v1");
        assert_eq!(d.api_key_env, "HF_API_KEY");
        assert!(d.key_required);
    }
}
