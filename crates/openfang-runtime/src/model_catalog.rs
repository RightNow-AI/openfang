//! Model catalog — registry of known models with metadata, pricing, and auth detection.
//!
//! Provides a comprehensive catalog of 130+ builtin models across 27 providers,
//! with alias resolution, auth status detection, and pricing lookups.

use openfang_types::model_catalog::{
    AuthStatus, ModelCatalogEntry, ModelTier, ProviderInfo, AI21_BASE_URL, ANTHROPIC_BASE_URL,
    BEDROCK_BASE_URL, CEREBRAS_BASE_URL, COHERE_BASE_URL, DEEPSEEK_BASE_URL, FIREWORKS_BASE_URL,
    GEMINI_BASE_URL, GITHUB_COPILOT_BASE_URL, GROQ_BASE_URL, HUGGINGFACE_BASE_URL,
    LMSTUDIO_BASE_URL, MINIMAX_BASE_URL, MISTRAL_BASE_URL, MOONSHOT_BASE_URL, OLLAMA_BASE_URL,
    OPENAI_BASE_URL, OPENROUTER_BASE_URL, PERPLEXITY_BASE_URL, QIANFAN_BASE_URL, QWEN_BASE_URL,
    REPLICATE_BASE_URL, SAMBANOVA_BASE_URL, TOGETHER_BASE_URL, VLLM_BASE_URL, XAI_BASE_URL,
    ZHIPU_BASE_URL, ZHIPU_CODING_BASE_URL,
};
use std::collections::HashMap;

/// The model catalog — registry of all known models and providers.
pub struct ModelCatalog {
    models: Vec<ModelCatalogEntry>,
    aliases: HashMap<String, String>,
    providers: Vec<ProviderInfo>,
}

impl ModelCatalog {
    /// Create a new catalog populated with builtin models and providers.
    pub fn new() -> Self {
        let models = builtin_models();
        let aliases = builtin_aliases();
        let mut providers = builtin_providers();

        // Set model counts on providers
        for provider in &mut providers {
            provider.model_count = models.iter().filter(|m| m.provider == provider.id).count();
        }

        Self {
            models,
            aliases,
            providers,
        }
    }

    /// Detect which providers have API keys configured.
    ///
    /// Checks `std::env::var()` for each provider's API key env var.
    /// Only checks presence — never reads or stores the actual secret.
    pub fn detect_auth(&mut self) {
        for provider in &mut self.providers {
            if !provider.key_required {
                provider.auth_status = AuthStatus::NotRequired;
                continue;
            }

            // Primary: check the provider's declared env var
            let has_key = std::env::var(&provider.api_key_env).is_ok();

            // Secondary: provider-specific fallback auth
            let has_fallback = match provider.id.as_str() {
                "gemini" => std::env::var("GOOGLE_API_KEY").is_ok(),
                "codex" => {
                    std::env::var("OPENAI_API_KEY").is_ok()
                        || read_codex_credential().is_some()
                }
                "claude-code" => crate::drivers::claude_code::claude_code_available(),
                _ => false,
            };

            provider.auth_status = if has_key || has_fallback {
                AuthStatus::Configured
            } else {
                AuthStatus::Missing
            };
        }
    }

    /// List all models in the catalog.
    pub fn list_models(&self) -> &[ModelCatalogEntry] {
        &self.models
    }

    /// Find a model by its canonical ID or by alias.
    pub fn find_model(&self, id_or_alias: &str) -> Option<&ModelCatalogEntry> {
        let lower = id_or_alias.to_lowercase();
        // Direct ID match first
        if let Some(entry) = self.models.iter().find(|m| m.id.to_lowercase() == lower) {
            return Some(entry);
        }
        // Alias resolution
        if let Some(canonical) = self.aliases.get(&lower) {
            return self.models.iter().find(|m| m.id == *canonical);
        }
        None
    }

    /// Resolve an alias to a canonical model ID, or None if not an alias.
    pub fn resolve_alias(&self, alias: &str) -> Option<&str> {
        self.aliases.get(&alias.to_lowercase()).map(|s| s.as_str())
    }

    /// List all providers.
    pub fn list_providers(&self) -> &[ProviderInfo] {
        &self.providers
    }

    /// Get a provider by ID.
    pub fn get_provider(&self, provider_id: &str) -> Option<&ProviderInfo> {
        self.providers.iter().find(|p| p.id == provider_id)
    }

    /// List models from a specific provider.
    pub fn models_by_provider(&self, provider: &str) -> Vec<&ModelCatalogEntry> {
        self.models
            .iter()
            .filter(|m| m.provider == provider)
            .collect()
    }

    /// List models that are available (from configured providers only).
    pub fn available_models(&self) -> Vec<&ModelCatalogEntry> {
        let configured: Vec<&str> = self
            .providers
            .iter()
            .filter(|p| p.auth_status != AuthStatus::Missing)
            .map(|p| p.id.as_str())
            .collect();
        self.models
            .iter()
            .filter(|m| configured.contains(&m.provider.as_str()))
            .collect()
    }

    /// Get pricing for a model: (input_cost_per_million, output_cost_per_million).
    pub fn pricing(&self, model_id: &str) -> Option<(f64, f64)> {
        self.find_model(model_id)
            .map(|m| (m.input_cost_per_m, m.output_cost_per_m))
    }

    /// List all alias mappings.
    pub fn list_aliases(&self) -> &HashMap<String, String> {
        &self.aliases
    }

    /// Set a custom base URL for a provider, overriding the default.
    ///
    /// Returns `true` if the provider was found and updated.
    pub fn set_provider_url(&mut self, provider: &str, url: &str) -> bool {
        if let Some(p) = self.providers.iter_mut().find(|p| p.id == provider) {
            p.base_url = url.to_string();
            true
        } else {
            false
        }
    }

    /// Apply a batch of provider URL overrides from config.
    ///
    /// Each entry maps a provider ID to a custom base URL.
    /// Unknown providers are silently skipped.
    pub fn apply_url_overrides(&mut self, overrides: &HashMap<String, String>) {
        for (provider, url) in overrides {
            self.set_provider_url(provider, url);
        }
    }

    /// List models filtered by tier.
    pub fn models_by_tier(&self, tier: ModelTier) -> Vec<&ModelCatalogEntry> {
        self.models.iter().filter(|m| m.tier == tier).collect()
    }

    /// Merge dynamically discovered models from a local provider.
    ///
    /// Adds models not already in the catalog with `Local` tier and zero cost.
    /// Also updates the provider's `model_count`.
    pub fn merge_discovered_models(&mut self, provider: &str, model_ids: &[String]) {
        let existing_ids: std::collections::HashSet<String> = self
            .models
            .iter()
            .filter(|m| m.provider == provider)
            .map(|m| m.id.to_lowercase())
            .collect();

        let mut added = 0usize;
        for id in model_ids {
            if existing_ids.contains(&id.to_lowercase()) {
                continue;
            }
            // Generate a human-friendly display name
            let display = format!("{} ({})", id, provider);
            self.models.push(ModelCatalogEntry {
                id: id.clone(),
                display_name: display,
                provider: provider.to_string(),
                tier: ModelTier::Local,
                context_window: 32_768,
                max_output_tokens: 4_096,
                input_cost_per_m: 0.0,
                output_cost_per_m: 0.0,
                supports_tools: true,
                supports_vision: false,
                supports_streaming: true,
                aliases: Vec::new(),
            });
            added += 1;
        }

        // Update model count on the provider
        if added > 0 {
            if let Some(p) = self.providers.iter_mut().find(|p| p.id == provider) {
                p.model_count = self
                    .models
                    .iter()
                    .filter(|m| m.provider == provider)
                    .count();
            }
        }
    }
}

impl Default for ModelCatalog {
    fn default() -> Self {
        Self::new()
    }
}

/// Read an OpenAI API key from the Codex CLI credential file.
///
/// Checks `$CODEX_HOME/auth.json` or `~/.codex/auth.json`.
/// Returns `Some(api_key)` if the file exists and contains a valid, non-expired token.
/// Only checks presence — the actual key value is used transiently, never stored.
pub fn read_codex_credential() -> Option<String> {
    let codex_home = std::env::var("CODEX_HOME")
        .map(std::path::PathBuf::from)
        .ok()
        .or_else(|| {
            #[cfg(target_os = "windows")]
            {
                std::env::var("USERPROFILE")
                    .ok()
                    .map(|h| std::path::PathBuf::from(h).join(".codex"))
            }
            #[cfg(not(target_os = "windows"))]
            {
                std::env::var("HOME")
                    .ok()
                    .map(|h| std::path::PathBuf::from(h).join(".codex"))
            }
        })?;

    let auth_path = codex_home.join("auth.json");
    let content = std::fs::read_to_string(&auth_path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;

    // Check expiry if present
    if let Some(expires_at) = parsed.get("expires_at").and_then(|v| v.as_i64()) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        if now >= expires_at {
            return None; // Expired
        }
    }

    parsed
        .get("api_key")
        .or_else(|| parsed.get("token"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

// ---------------------------------------------------------------------------
// Builtin data
// ---------------------------------------------------------------------------

fn builtin_providers() -> Vec<ProviderInfo> {
    vec![
        ProviderInfo {
            id: "anthropic".into(),
            display_name: "Anthropic".into(),
            api_key_env: "ANTHROPIC_API_KEY".into(),
            base_url: ANTHROPIC_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        ProviderInfo {
            id: "openai".into(),
            display_name: "OpenAI".into(),
            api_key_env: "OPENAI_API_KEY".into(),
            base_url: OPENAI_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        ProviderInfo {
            id: "gemini".into(),
            display_name: "Google Gemini".into(),
            api_key_env: "GEMINI_API_KEY".into(),
            base_url: GEMINI_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        ProviderInfo {
            id: "deepseek".into(),
            display_name: "DeepSeek".into(),
            api_key_env: "DEEPSEEK_API_KEY".into(),
            base_url: DEEPSEEK_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        ProviderInfo {
            id: "groq".into(),
            display_name: "Groq".into(),
            api_key_env: "GROQ_API_KEY".into(),
            base_url: GROQ_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        ProviderInfo {
            id: "openrouter".into(),
            display_name: "OpenRouter".into(),
            api_key_env: "OPENROUTER_API_KEY".into(),
            base_url: OPENROUTER_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        ProviderInfo {
            id: "mistral".into(),
            display_name: "Mistral AI".into(),
            api_key_env: "MISTRAL_API_KEY".into(),
            base_url: MISTRAL_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        ProviderInfo {
            id: "together".into(),
            display_name: "Together AI".into(),
            api_key_env: "TOGETHER_API_KEY".into(),
            base_url: TOGETHER_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        ProviderInfo {
            id: "fireworks".into(),
            display_name: "Fireworks AI".into(),
            api_key_env: "FIREWORKS_API_KEY".into(),
            base_url: FIREWORKS_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        ProviderInfo {
            id: "ollama".into(),
            display_name: "Ollama".into(),
            api_key_env: "OLLAMA_API_KEY".into(),
            base_url: OLLAMA_BASE_URL.into(),
            key_required: false,
            auth_status: AuthStatus::NotRequired,
            model_count: 0,
        },
        ProviderInfo {
            id: "vllm".into(),
            display_name: "vLLM".into(),
            api_key_env: "VLLM_API_KEY".into(),
            base_url: VLLM_BASE_URL.into(),
            key_required: false,
            auth_status: AuthStatus::NotRequired,
            model_count: 0,
        },
        ProviderInfo {
            id: "lmstudio".into(),
            display_name: "LM Studio".into(),
            api_key_env: "LMSTUDIO_API_KEY".into(),
            base_url: LMSTUDIO_BASE_URL.into(),
            key_required: false,
            auth_status: AuthStatus::NotRequired,
            model_count: 0,
        },
        // ── New providers (8) ──────────────────────────────────────
        ProviderInfo {
            id: "perplexity".into(),
            display_name: "Perplexity AI".into(),
            api_key_env: "PERPLEXITY_API_KEY".into(),
            base_url: PERPLEXITY_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        ProviderInfo {
            id: "cohere".into(),
            display_name: "Cohere".into(),
            api_key_env: "COHERE_API_KEY".into(),
            base_url: COHERE_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        ProviderInfo {
            id: "ai21".into(),
            display_name: "AI21 Labs".into(),
            api_key_env: "AI21_API_KEY".into(),
            base_url: AI21_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        ProviderInfo {
            id: "cerebras".into(),
            display_name: "Cerebras".into(),
            api_key_env: "CEREBRAS_API_KEY".into(),
            base_url: CEREBRAS_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        ProviderInfo {
            id: "sambanova".into(),
            display_name: "SambaNova".into(),
            api_key_env: "SAMBANOVA_API_KEY".into(),
            base_url: SAMBANOVA_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        ProviderInfo {
            id: "huggingface".into(),
            display_name: "Hugging Face".into(),
            api_key_env: "HF_API_KEY".into(),
            base_url: HUGGINGFACE_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        ProviderInfo {
            id: "xai".into(),
            display_name: "xAI".into(),
            api_key_env: "XAI_API_KEY".into(),
            base_url: XAI_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        ProviderInfo {
            id: "replicate".into(),
            display_name: "Replicate".into(),
            api_key_env: "REPLICATE_API_TOKEN".into(),
            base_url: REPLICATE_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        // ── GitHub Copilot ───────────────────────────────────────────
        ProviderInfo {
            id: "github-copilot".into(),
            display_name: "GitHub Copilot".into(),
            api_key_env: "GITHUB_TOKEN".into(),
            base_url: GITHUB_COPILOT_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        // ── Chinese providers (5) ────────────────────────────────────
        ProviderInfo {
            id: "qwen".into(),
            display_name: "Qwen (Alibaba)".into(),
            api_key_env: "DASHSCOPE_API_KEY".into(),
            base_url: QWEN_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        ProviderInfo {
            id: "minimax".into(),
            display_name: "MiniMax".into(),
            api_key_env: "MINIMAX_API_KEY".into(),
            base_url: MINIMAX_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        ProviderInfo {
            id: "zhipu".into(),
            display_name: "Zhipu AI (GLM)".into(),
            api_key_env: "ZHIPU_API_KEY".into(),
            base_url: ZHIPU_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        ProviderInfo {
            id: "zhipu_coding".into(),
            display_name: "Zhipu Coding (CodeGeeX)".into(),
            api_key_env: "ZHIPU_API_KEY".into(),
            base_url: ZHIPU_CODING_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        ProviderInfo {
            id: "moonshot".into(),
            display_name: "Moonshot (Kimi)".into(),
            api_key_env: "MOONSHOT_API_KEY".into(),
            base_url: MOONSHOT_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        ProviderInfo {
            id: "qianfan".into(),
            display_name: "Baidu Qianfan".into(),
            api_key_env: "QIANFAN_API_KEY".into(),
            base_url: QIANFAN_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        // ── AWS Bedrock ──────────────────────────────────────────────
        ProviderInfo {
            id: "bedrock".into(),
            display_name: "AWS Bedrock".into(),
            api_key_env: "AWS_ACCESS_KEY_ID".into(),
            base_url: BEDROCK_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        // ── OpenAI Codex ────────────────────────────────────────────
        ProviderInfo {
            id: "codex".into(),
            display_name: "OpenAI Codex".into(),
            api_key_env: "OPENAI_API_KEY".into(),
            base_url: OPENAI_BASE_URL.into(),
            key_required: true,
            auth_status: AuthStatus::Missing,
            model_count: 0,
        },
        // ── Claude Code CLI ─────────────────────────────────────────
        ProviderInfo {
            id: "claude-code".into(),
            display_name: "Claude Code".into(),
            api_key_env: String::new(),
            base_url: String::new(),
            key_required: false,
            auth_status: AuthStatus::NotRequired,
            model_count: 0,
        },
    ]
}

fn builtin_aliases() -> HashMap<String, String> {
    let pairs = [
        ("sonnet", "claude-sonnet-4-6"),
        ("claude-sonnet", "claude-sonnet-4-6"),
        ("haiku", "claude-haiku-4-5-20251001"),
        ("claude-haiku", "claude-haiku-4-5-20251001"),
        ("opus", "claude-opus-4-6"),
        ("claude-opus", "claude-opus-4-6"),
        ("gpt4", "gpt-4o"),
        ("gpt4o", "gpt-4o"),
        ("gpt4-mini", "gpt-4o-mini"),
        ("gpt5", "gpt-5.2"),
        ("gpt5-mini", "gpt-5-mini"),
        ("flash", "gemini-3-flash"),
        ("gemini-flash", "gemini-3-flash"),
        ("gemini-pro", "gemini-3.1-pro"),
        ("deepseek", "deepseek-chat"),
        ("llama", "llama-3.3-70b-versatile"),
        ("llama-70b", "llama-3.3-70b-versatile"),
        ("mixtral", "mixtral-8x7b-32768"),
        ("mistral", "mistral-large-latest"),
        ("codestral", "codestral-latest"),
        // DeepSeek aliases
        ("deepseek-v3", "deepseek-chat"),
        ("deepseek-r1", "deepseek-reasoner"),
        // Mistral aliases
        ("mistral-nemo", "open-mistral-nemo"),
        ("pixtral", "pixtral-large-latest"),
        // xAI aliases
        ("grok", "grok-4"),
        ("grok-mini", "grok-2-mini"),
        ("grok3", "grok-3"),
        ("grok-fast", "grok-4.1-fast"),
        // Perplexity alias
        ("sonar", "sonar-pro"),
        // AI21 aliases
        ("jamba", "jamba-1.5-large"),
        // Cohere aliases
        ("command-r", "command-r-plus"),
        ("command", "command-a"),
        // GitHub Copilot aliases
        ("copilot", "copilot/gpt-4o"),
        ("copilot-4o", "copilot/gpt-4o"),
        ("copilot-4", "copilot/gpt-4"),
        ("copilot-gpt4o", "copilot/gpt-4o"),
        ("copilot-gpt4", "copilot/gpt-4"),
        // Chinese model aliases
        ("qwen", "qwen-plus"),
        ("glm", "glm-4-plus"),
        ("ernie", "ernie-4.5-8k"),
        ("kimi", "moonshot-v1-128k"),
        ("minimax", "MiniMax-M2.5"),
        ("minimax-m2.5", "MiniMax-M2.5"),
        ("minimax-m2.1", "MiniMax-M2.1"),
        ("codegeex", "codegeex-4"),
        // Codex aliases
        ("codex", "codex/gpt-4.1"),
        ("codex-4.1", "codex/gpt-4.1"),
        ("codex-o4", "codex/o4-mini"),
        // Claude Code aliases
        ("claude-code", "claude-code/sonnet"),
        ("claude-code-opus", "claude-code/opus"),
        ("claude-code-sonnet", "claude-code/sonnet"),
        ("claude-code-haiku", "claude-code/haiku"),
    ];
    pairs
        .into_iter()
        .map(|(k, v)| (k.to_lowercase(), v.to_string()))
        .collect()
}

fn builtin_models() -> Vec<ModelCatalogEntry> {
    vec![
        // ══════════════════════════════════════════════════════════════
        // Anthropic (7)
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "claude-opus-4-6".into(),
            display_name: "Claude Opus 4.6".into(),
            provider: "anthropic".into(),
            tier: ModelTier::Frontier,
            context_window: 200_000,
            max_output_tokens: 128_000,
            input_cost_per_m: 5.0,
            output_cost_per_m: 25.0,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec!["opus".into(), "claude-opus".into()],
        },
        ModelCatalogEntry {
            id: "claude-sonnet-4-6".into(),
            display_name: "Claude Sonnet 4.6".into(),
            provider: "anthropic".into(),
            tier: ModelTier::Smart,
            context_window: 200_000,
            max_output_tokens: 64_000,
            input_cost_per_m: 3.0,
            output_cost_per_m: 15.0,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec!["sonnet".into(), "claude-sonnet".into()],
        },
        ModelCatalogEntry {
            id: "claude-opus-4-20250514".into(),
            display_name: "Claude Opus 4".into(),
            provider: "anthropic".into(),
            tier: ModelTier::Frontier,
            context_window: 200_000,
            max_output_tokens: 32_000,
            input_cost_per_m: 15.0,
            output_cost_per_m: 75.0,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "claude-sonnet-4-20250514".into(),
            display_name: "Claude Sonnet 4".into(),
            provider: "anthropic".into(),
            tier: ModelTier::Smart,
            context_window: 200_000,
            max_output_tokens: 64_000,
            input_cost_per_m: 3.0,
            output_cost_per_m: 15.0,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "claude-haiku-4-5-20251001".into(),
            display_name: "Claude Haiku 4.5".into(),
            provider: "anthropic".into(),
            tier: ModelTier::Fast,
            context_window: 200_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.25,
            output_cost_per_m: 1.25,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec!["haiku".into(), "claude-haiku".into()],
        },
        ModelCatalogEntry {
            id: "claude-sonnet-4-5-20250514".into(),
            display_name: "Claude Sonnet 4.5".into(),
            provider: "anthropic".into(),
            tier: ModelTier::Smart,
            context_window: 200_000,
            max_output_tokens: 64_000,
            input_cost_per_m: 3.0,
            output_cost_per_m: 15.0,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "claude-3-5-sonnet-20241022".into(),
            display_name: "Claude 3.5 Sonnet".into(),
            provider: "anthropic".into(),
            tier: ModelTier::Smart,
            context_window: 200_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 3.0,
            output_cost_per_m: 15.0,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        // ══════════════════════════════════════════════════════════════
        // OpenAI (16)
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "gpt-4o".into(),
            display_name: "GPT-4o".into(),
            provider: "openai".into(),
            tier: ModelTier::Smart,
            context_window: 128_000,
            max_output_tokens: 16_384,
            input_cost_per_m: 2.50,
            output_cost_per_m: 10.0,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec!["gpt4".into(), "gpt4o".into()],
        },
        ModelCatalogEntry {
            id: "gpt-4o-mini".into(),
            display_name: "GPT-4o Mini".into(),
            provider: "openai".into(),
            tier: ModelTier::Fast,
            context_window: 128_000,
            max_output_tokens: 16_384,
            input_cost_per_m: 0.15,
            output_cost_per_m: 0.60,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec!["gpt4-mini".into()],
        },
        ModelCatalogEntry {
            id: "gpt-4.1".into(),
            display_name: "GPT-4.1".into(),
            provider: "openai".into(),
            tier: ModelTier::Frontier,
            context_window: 1_047_576,
            max_output_tokens: 32_768,
            input_cost_per_m: 2.00,
            output_cost_per_m: 8.00,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "gpt-4.1-mini".into(),
            display_name: "GPT-4.1 Mini".into(),
            provider: "openai".into(),
            tier: ModelTier::Balanced,
            context_window: 1_047_576,
            max_output_tokens: 32_768,
            input_cost_per_m: 0.40,
            output_cost_per_m: 1.60,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "gpt-4.1-nano".into(),
            display_name: "GPT-4.1 Nano".into(),
            provider: "openai".into(),
            tier: ModelTier::Fast,
            context_window: 1_047_576,
            max_output_tokens: 32_768,
            input_cost_per_m: 0.10,
            output_cost_per_m: 0.40,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "o3".into(),
            display_name: "o3".into(),
            provider: "openai".into(),
            tier: ModelTier::Frontier,
            context_window: 200_000,
            max_output_tokens: 100_000,
            input_cost_per_m: 2.00,
            output_cost_per_m: 8.00,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "o3-mini".into(),
            display_name: "o3-mini".into(),
            provider: "openai".into(),
            tier: ModelTier::Smart,
            context_window: 200_000,
            max_output_tokens: 100_000,
            input_cost_per_m: 1.10,
            output_cost_per_m: 4.40,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "o4-mini".into(),
            display_name: "o4-mini".into(),
            provider: "openai".into(),
            tier: ModelTier::Smart,
            context_window: 200_000,
            max_output_tokens: 100_000,
            input_cost_per_m: 1.10,
            output_cost_per_m: 4.40,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "gpt-4-turbo".into(),
            display_name: "GPT-4 Turbo".into(),
            provider: "openai".into(),
            tier: ModelTier::Smart,
            context_window: 128_000,
            max_output_tokens: 4_096,
            input_cost_per_m: 10.00,
            output_cost_per_m: 30.00,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "gpt-3.5-turbo".into(),
            display_name: "GPT-3.5 Turbo".into(),
            provider: "openai".into(),
            tier: ModelTier::Fast,
            context_window: 16_385,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.50,
            output_cost_per_m: 1.50,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "gpt-5".into(),
            display_name: "GPT-5".into(),
            provider: "openai".into(),
            tier: ModelTier::Frontier,
            context_window: 400_000,
            max_output_tokens: 128_000,
            input_cost_per_m: 1.25,
            output_cost_per_m: 10.0,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "gpt-5-mini".into(),
            display_name: "GPT-5 Mini".into(),
            provider: "openai".into(),
            tier: ModelTier::Balanced,
            context_window: 400_000,
            max_output_tokens: 128_000,
            input_cost_per_m: 0.25,
            output_cost_per_m: 2.0,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec!["gpt5-mini".into()],
        },
        ModelCatalogEntry {
            id: "gpt-5-nano".into(),
            display_name: "GPT-5 Nano".into(),
            provider: "openai".into(),
            tier: ModelTier::Fast,
            context_window: 400_000,
            max_output_tokens: 128_000,
            input_cost_per_m: 0.05,
            output_cost_per_m: 0.40,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "gpt-5.1".into(),
            display_name: "GPT-5.1".into(),
            provider: "openai".into(),
            tier: ModelTier::Frontier,
            context_window: 400_000,
            max_output_tokens: 128_000,
            input_cost_per_m: 1.25,
            output_cost_per_m: 10.0,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "gpt-5.2".into(),
            display_name: "GPT-5.2".into(),
            provider: "openai".into(),
            tier: ModelTier::Frontier,
            context_window: 400_000,
            max_output_tokens: 128_000,
            input_cost_per_m: 1.75,
            output_cost_per_m: 14.0,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec!["gpt5".into()],
        },
        ModelCatalogEntry {
            id: "gpt-5.2-pro".into(),
            display_name: "GPT-5.2 Pro".into(),
            provider: "openai".into(),
            tier: ModelTier::Frontier,
            context_window: 400_000,
            max_output_tokens: 128_000,
            input_cost_per_m: 1.75,
            output_cost_per_m: 14.0,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        // ══════════════════════════════════════════════════════════════
        // Google Gemini (10)
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "gemini-3.1-pro".into(),
            display_name: "Gemini 3.1 Pro".into(),
            provider: "gemini".into(),
            tier: ModelTier::Frontier,
            context_window: 1_048_576,
            max_output_tokens: 65_536,
            input_cost_per_m: 2.50,
            output_cost_per_m: 15.0,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec!["gemini-pro".into()],
        },
        ModelCatalogEntry {
            id: "gemini-3-flash".into(),
            display_name: "Gemini 3 Flash".into(),
            provider: "gemini".into(),
            tier: ModelTier::Smart,
            context_window: 1_048_576,
            max_output_tokens: 65_536,
            input_cost_per_m: 0.50,
            output_cost_per_m: 3.0,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec!["flash".into(), "gemini-flash".into()],
        },
        ModelCatalogEntry {
            id: "gemini-3-deep-think".into(),
            display_name: "Gemini 3 Deep Think".into(),
            provider: "gemini".into(),
            tier: ModelTier::Frontier,
            context_window: 1_048_576,
            max_output_tokens: 65_536,
            input_cost_per_m: 2.50,
            output_cost_per_m: 15.0,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "gemini-2.5-flash-lite".into(),
            display_name: "Gemini 2.5 Flash Lite".into(),
            provider: "gemini".into(),
            tier: ModelTier::Fast,
            context_window: 1_048_576,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.04,
            output_cost_per_m: 0.15,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "gemini-2.5-pro".into(),
            display_name: "Gemini 2.5 Pro".into(),
            provider: "gemini".into(),
            tier: ModelTier::Frontier,
            context_window: 1_048_576,
            max_output_tokens: 65_536,
            input_cost_per_m: 1.25,
            output_cost_per_m: 10.0,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "gemini-2.5-flash".into(),
            display_name: "Gemini 2.5 Flash".into(),
            provider: "gemini".into(),
            tier: ModelTier::Smart,
            context_window: 1_048_576,
            max_output_tokens: 65_536,
            input_cost_per_m: 0.15,
            output_cost_per_m: 0.60,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "gemini-2.0-flash".into(),
            display_name: "Gemini 2.0 Flash".into(),
            provider: "gemini".into(),
            tier: ModelTier::Fast,
            context_window: 1_048_576,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.10,
            output_cost_per_m: 0.40,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "gemini-2.0-flash-lite".into(),
            display_name: "Gemini 2.0 Flash Lite".into(),
            provider: "gemini".into(),
            tier: ModelTier::Fast,
            context_window: 1_048_576,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.075,
            output_cost_per_m: 0.30,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "gemini-1.5-pro".into(),
            display_name: "Gemini 1.5 Pro".into(),
            provider: "gemini".into(),
            tier: ModelTier::Smart,
            context_window: 2_097_152,
            max_output_tokens: 8_192,
            input_cost_per_m: 1.25,
            output_cost_per_m: 5.00,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "gemini-1.5-flash".into(),
            display_name: "Gemini 1.5 Flash".into(),
            provider: "gemini".into(),
            tier: ModelTier::Fast,
            context_window: 1_048_576,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.075,
            output_cost_per_m: 0.30,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        // ══════════════════════════════════════════════════════════════
        // DeepSeek (4)
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "deepseek-chat".into(),
            display_name: "DeepSeek V3".into(),
            provider: "deepseek".into(),
            tier: ModelTier::Smart,
            context_window: 64_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.27,
            output_cost_per_m: 1.10,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec!["deepseek".into(), "deepseek-v3".into()],
        },
        ModelCatalogEntry {
            id: "deepseek-reasoner".into(),
            display_name: "DeepSeek R1".into(),
            provider: "deepseek".into(),
            tier: ModelTier::Frontier,
            context_window: 64_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.55,
            output_cost_per_m: 2.19,
            supports_tools: false,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec!["deepseek-r1".into()],
        },
        ModelCatalogEntry {
            id: "deepseek-coder".into(),
            display_name: "DeepSeek Coder V2".into(),
            provider: "deepseek".into(),
            tier: ModelTier::Balanced,
            context_window: 128_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.14,
            output_cost_per_m: 0.28,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "deepseek-chat-v3-0324".into(),
            display_name: "DeepSeek V3 0324".into(),
            provider: "deepseek".into(),
            tier: ModelTier::Smart,
            context_window: 64_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.27,
            output_cost_per_m: 1.10,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        // ══════════════════════════════════════════════════════════════
        // Groq (11)
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "llama-3.3-70b-versatile".into(),
            display_name: "Llama 3.3 70B".into(),
            provider: "groq".into(),
            tier: ModelTier::Balanced,
            context_window: 128_000,
            max_output_tokens: 32_768,
            input_cost_per_m: 0.059,
            output_cost_per_m: 0.079,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec!["llama".into(), "llama-70b".into()],
        },
        ModelCatalogEntry {
            id: "llama-3.1-8b-instant".into(),
            display_name: "Llama 3.1 8B".into(),
            provider: "groq".into(),
            tier: ModelTier::Fast,
            context_window: 128_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.05,
            output_cost_per_m: 0.08,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "llama-3.2-90b-vision-preview".into(),
            display_name: "Llama 3.2 90B Vision".into(),
            provider: "groq".into(),
            tier: ModelTier::Smart,
            context_window: 128_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.90,
            output_cost_per_m: 0.90,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "llama-3.2-11b-vision-preview".into(),
            display_name: "Llama 3.2 11B Vision".into(),
            provider: "groq".into(),
            tier: ModelTier::Balanced,
            context_window: 128_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.18,
            output_cost_per_m: 0.18,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "llama-3.2-3b-preview".into(),
            display_name: "Llama 3.2 3B".into(),
            provider: "groq".into(),
            tier: ModelTier::Fast,
            context_window: 128_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.06,
            output_cost_per_m: 0.06,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "llama-3.2-1b-preview".into(),
            display_name: "Llama 3.2 1B".into(),
            provider: "groq".into(),
            tier: ModelTier::Fast,
            context_window: 128_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.04,
            output_cost_per_m: 0.04,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "mixtral-8x7b-32768".into(),
            display_name: "Mixtral 8x7B".into(),
            provider: "groq".into(),
            tier: ModelTier::Balanced,
            context_window: 32_768,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.024,
            output_cost_per_m: 0.024,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec!["mixtral".into()],
        },
        ModelCatalogEntry {
            id: "gemma2-9b-it".into(),
            display_name: "Gemma 2 9B".into(),
            provider: "groq".into(),
            tier: ModelTier::Fast,
            context_window: 8_192,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.02,
            output_cost_per_m: 0.02,
            supports_tools: false,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "qwen-qwq-32b".into(),
            display_name: "Qwen QWQ 32B".into(),
            provider: "groq".into(),
            tier: ModelTier::Balanced,
            context_window: 128_000,
            max_output_tokens: 16_384,
            input_cost_per_m: 0.20,
            output_cost_per_m: 0.20,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "meta-llama/llama-4-scout-17b-16e-instruct".into(),
            display_name: "Llama 4 Scout 17B".into(),
            provider: "groq".into(),
            tier: ModelTier::Balanced,
            context_window: 128_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.11,
            output_cost_per_m: 0.34,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        // ══════════════════════════════════════════════════════════════
        // OpenRouter (5)
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "openrouter/auto".into(),
            display_name: "OpenRouter Auto".into(),
            provider: "openrouter".into(),
            tier: ModelTier::Smart,
            context_window: 200_000,
            max_output_tokens: 32_000,
            input_cost_per_m: 1.0,
            output_cost_per_m: 3.0,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "openrouter/optimus".into(),
            display_name: "OpenRouter Optimus".into(),
            provider: "openrouter".into(),
            tier: ModelTier::Balanced,
            context_window: 200_000,
            max_output_tokens: 32_000,
            input_cost_per_m: 0.50,
            output_cost_per_m: 1.50,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "openrouter/nitro".into(),
            display_name: "OpenRouter Nitro".into(),
            provider: "openrouter".into(),
            tier: ModelTier::Fast,
            context_window: 128_000,
            max_output_tokens: 16_000,
            input_cost_per_m: 0.20,
            output_cost_per_m: 0.60,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "openrouter/anthropic/claude-sonnet-4".into(),
            display_name: "Claude Sonnet 4 (OpenRouter)".into(),
            provider: "openrouter".into(),
            tier: ModelTier::Smart,
            context_window: 200_000,
            max_output_tokens: 64_000,
            input_cost_per_m: 3.0,
            output_cost_per_m: 15.0,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "openrouter/google/gemini-2.5-flash".into(),
            display_name: "Gemini 2.5 Flash (OpenRouter)".into(),
            provider: "openrouter".into(),
            tier: ModelTier::Smart,
            context_window: 1_048_576,
            max_output_tokens: 65_536,
            input_cost_per_m: 0.15,
            output_cost_per_m: 0.60,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        // ══════════════════════════════════════════════════════════════
        // Mistral (6)
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "mistral-large-latest".into(),
            display_name: "Mistral Large".into(),
            provider: "mistral".into(),
            tier: ModelTier::Smart,
            context_window: 128_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 2.00,
            output_cost_per_m: 6.00,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec!["mistral".into()],
        },
        ModelCatalogEntry {
            id: "mistral-medium-latest".into(),
            display_name: "Mistral Medium".into(),
            provider: "mistral".into(),
            tier: ModelTier::Balanced,
            context_window: 128_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 2.70,
            output_cost_per_m: 8.10,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "mistral-small-latest".into(),
            display_name: "Mistral Small".into(),
            provider: "mistral".into(),
            tier: ModelTier::Fast,
            context_window: 128_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.10,
            output_cost_per_m: 0.30,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "codestral-latest".into(),
            display_name: "Codestral".into(),
            provider: "mistral".into(),
            tier: ModelTier::Smart,
            context_window: 32_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.30,
            output_cost_per_m: 0.90,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec!["codestral".into()],
        },
        ModelCatalogEntry {
            id: "open-mistral-nemo".into(),
            display_name: "Mistral Nemo".into(),
            provider: "mistral".into(),
            tier: ModelTier::Fast,
            context_window: 128_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.15,
            output_cost_per_m: 0.15,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec!["mistral-nemo".into()],
        },
        ModelCatalogEntry {
            id: "pixtral-large-latest".into(),
            display_name: "Pixtral Large".into(),
            provider: "mistral".into(),
            tier: ModelTier::Smart,
            context_window: 128_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 2.00,
            output_cost_per_m: 6.00,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec!["pixtral".into()],
        },
        // ══════════════════════════════════════════════════════════════
        // Together (8)
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "meta-llama/Meta-Llama-3.1-405B-Instruct-Turbo".into(),
            display_name: "Llama 3.1 405B (Together)".into(),
            provider: "together".into(),
            tier: ModelTier::Frontier,
            context_window: 130_000,
            max_output_tokens: 4_096,
            input_cost_per_m: 3.50,
            output_cost_per_m: 3.50,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "meta-llama/Llama-3.3-70B-Instruct-Turbo".into(),
            display_name: "Llama 3.3 70B (Together)".into(),
            provider: "together".into(),
            tier: ModelTier::Smart,
            context_window: 128_000,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.88,
            output_cost_per_m: 0.88,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8".into(),
            display_name: "Llama 4 Maverick (Together)".into(),
            provider: "together".into(),
            tier: ModelTier::Smart,
            context_window: 1_048_576,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.27,
            output_cost_per_m: 0.35,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "meta-llama/Llama-4-Scout-17B-16E-Instruct".into(),
            display_name: "Llama 4 Scout (Together)".into(),
            provider: "together".into(),
            tier: ModelTier::Balanced,
            context_window: 512_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.18,
            output_cost_per_m: 0.30,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "deepseek-ai/DeepSeek-R1".into(),
            display_name: "DeepSeek R1 (Together)".into(),
            provider: "together".into(),
            tier: ModelTier::Frontier,
            context_window: 64_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 3.00,
            output_cost_per_m: 7.00,
            supports_tools: false,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "deepseek-ai/DeepSeek-V3".into(),
            display_name: "DeepSeek V3 (Together)".into(),
            provider: "together".into(),
            tier: ModelTier::Smart,
            context_window: 64_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.90,
            output_cost_per_m: 0.90,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "Qwen/Qwen2.5-72B-Instruct-Turbo".into(),
            display_name: "Qwen 2.5 72B (Together)".into(),
            provider: "together".into(),
            tier: ModelTier::Smart,
            context_window: 32_768,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.20,
            output_cost_per_m: 0.60,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "mistralai/Mixtral-8x22B-Instruct-v0.1".into(),
            display_name: "Mixtral 8x22B (Together)".into(),
            provider: "together".into(),
            tier: ModelTier::Balanced,
            context_window: 65_536,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.60,
            output_cost_per_m: 0.60,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        // ══════════════════════════════════════════════════════════════
        // Fireworks (5)
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "accounts/fireworks/models/llama-v3p1-405b-instruct".into(),
            display_name: "Llama 3.1 405B (Fireworks)".into(),
            provider: "fireworks".into(),
            tier: ModelTier::Frontier,
            context_window: 131_072,
            max_output_tokens: 16_384,
            input_cost_per_m: 3.00,
            output_cost_per_m: 3.00,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "accounts/fireworks/models/llama-v3p3-70b-instruct".into(),
            display_name: "Llama 3.3 70B (Fireworks)".into(),
            provider: "fireworks".into(),
            tier: ModelTier::Smart,
            context_window: 131_072,
            max_output_tokens: 16_384,
            input_cost_per_m: 0.90,
            output_cost_per_m: 0.90,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "accounts/fireworks/models/deepseek-r1".into(),
            display_name: "DeepSeek R1 (Fireworks)".into(),
            provider: "fireworks".into(),
            tier: ModelTier::Frontier,
            context_window: 64_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 3.00,
            output_cost_per_m: 8.00,
            supports_tools: false,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "accounts/fireworks/models/deepseek-v3".into(),
            display_name: "DeepSeek V3 (Fireworks)".into(),
            provider: "fireworks".into(),
            tier: ModelTier::Smart,
            context_window: 64_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.90,
            output_cost_per_m: 0.90,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "accounts/fireworks/models/mixtral-8x22b-instruct".into(),
            display_name: "Mixtral 8x22B (Fireworks)".into(),
            provider: "fireworks".into(),
            tier: ModelTier::Balanced,
            context_window: 65_536,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.90,
            output_cost_per_m: 0.90,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        // ══════════════════════════════════════════════════════════════
        // Ollama (6) — local, no key required + dynamic discovery
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "llama3.2".into(),
            display_name: "Llama 3.2 (Ollama)".into(),
            provider: "ollama".into(),
            tier: ModelTier::Local,
            context_window: 128_000,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.0,
            output_cost_per_m: 0.0,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "llama3.1".into(),
            display_name: "Llama 3.1 (Ollama)".into(),
            provider: "ollama".into(),
            tier: ModelTier::Local,
            context_window: 128_000,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.0,
            output_cost_per_m: 0.0,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "mistral:latest".into(),
            display_name: "Mistral (Ollama)".into(),
            provider: "ollama".into(),
            tier: ModelTier::Local,
            context_window: 32_768,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.0,
            output_cost_per_m: 0.0,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "qwen2.5".into(),
            display_name: "Qwen 2.5 (Ollama)".into(),
            provider: "ollama".into(),
            tier: ModelTier::Local,
            context_window: 32_768,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.0,
            output_cost_per_m: 0.0,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "phi3".into(),
            display_name: "Phi-3 (Ollama)".into(),
            provider: "ollama".into(),
            tier: ModelTier::Local,
            context_window: 128_000,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.0,
            output_cost_per_m: 0.0,
            supports_tools: false,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "deepseek-r1:latest".into(),
            display_name: "DeepSeek R1 (Ollama)".into(),
            provider: "ollama".into(),
            tier: ModelTier::Local,
            context_window: 64_000,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.0,
            output_cost_per_m: 0.0,
            supports_tools: false,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        // ══════════════════════════════════════════════════════════════
        // vLLM (1) — generic local entry + dynamic discovery
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "vllm-local".into(),
            display_name: "vLLM Local Model".into(),
            provider: "vllm".into(),
            tier: ModelTier::Local,
            context_window: 32_768,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.0,
            output_cost_per_m: 0.0,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        // ══════════════════════════════════════════════════════════════
        // LM Studio (1) — generic local entry + dynamic discovery
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "lmstudio-local".into(),
            display_name: "LM Studio Local Model".into(),
            provider: "lmstudio".into(),
            tier: ModelTier::Local,
            context_window: 32_768,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.0,
            output_cost_per_m: 0.0,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        // ══════════════════════════════════════════════════════════════
        // Perplexity (4)
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "sonar-pro".into(),
            display_name: "Sonar Pro".into(),
            provider: "perplexity".into(),
            tier: ModelTier::Smart,
            context_window: 200_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 3.0,
            output_cost_per_m: 15.0,
            supports_tools: false,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec!["sonar".into()],
        },
        ModelCatalogEntry {
            id: "sonar-reasoning-pro".into(),
            display_name: "Sonar Reasoning Pro".into(),
            provider: "perplexity".into(),
            tier: ModelTier::Frontier,
            context_window: 200_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 2.0,
            output_cost_per_m: 8.0,
            supports_tools: false,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "sonar-reasoning".into(),
            display_name: "Sonar Reasoning".into(),
            provider: "perplexity".into(),
            tier: ModelTier::Balanced,
            context_window: 128_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 1.0,
            output_cost_per_m: 5.0,
            supports_tools: false,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "sonar-basic".into(),
            display_name: "Sonar".into(),
            provider: "perplexity".into(),
            tier: ModelTier::Fast,
            context_window: 128_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 1.0,
            output_cost_per_m: 5.0,
            supports_tools: false,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        // ══════════════════════════════════════════════════════════════
        // Cohere (4)
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "command-r-plus".into(),
            display_name: "Command R+".into(),
            provider: "cohere".into(),
            tier: ModelTier::Smart,
            context_window: 128_000,
            max_output_tokens: 4_096,
            input_cost_per_m: 2.50,
            output_cost_per_m: 10.0,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec!["command-r".into()],
        },
        ModelCatalogEntry {
            id: "command-r-08-2024".into(),
            display_name: "Command R (Aug 2024)".into(),
            provider: "cohere".into(),
            tier: ModelTier::Balanced,
            context_window: 128_000,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.15,
            output_cost_per_m: 0.60,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "command-a".into(),
            display_name: "Command A".into(),
            provider: "cohere".into(),
            tier: ModelTier::Smart,
            context_window: 256_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 2.50,
            output_cost_per_m: 10.0,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "command-light".into(),
            display_name: "Command Light".into(),
            provider: "cohere".into(),
            tier: ModelTier::Fast,
            context_window: 4_096,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.30,
            output_cost_per_m: 0.60,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        // ══════════════════════════════════════════════════════════════
        // AI21 (3)
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "jamba-1.5-large".into(),
            display_name: "Jamba 1.5 Large".into(),
            provider: "ai21".into(),
            tier: ModelTier::Smart,
            context_window: 256_000,
            max_output_tokens: 4_096,
            input_cost_per_m: 2.0,
            output_cost_per_m: 8.0,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec!["jamba".into()],
        },
        ModelCatalogEntry {
            id: "jamba-1.5-mini".into(),
            display_name: "Jamba 1.5 Mini".into(),
            provider: "ai21".into(),
            tier: ModelTier::Fast,
            context_window: 256_000,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.20,
            output_cost_per_m: 0.40,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "jamba-instruct".into(),
            display_name: "Jamba Instruct".into(),
            provider: "ai21".into(),
            tier: ModelTier::Balanced,
            context_window: 256_000,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.50,
            output_cost_per_m: 0.70,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        // ══════════════════════════════════════════════════════════════
        // Cerebras (4)
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "cerebras/llama3.3-70b".into(),
            display_name: "Llama 3.3 70B (Cerebras)".into(),
            provider: "cerebras".into(),
            tier: ModelTier::Balanced,
            context_window: 128_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.06,
            output_cost_per_m: 0.06,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "cerebras/llama3.1-8b".into(),
            display_name: "Llama 3.1 8B (Cerebras)".into(),
            provider: "cerebras".into(),
            tier: ModelTier::Fast,
            context_window: 128_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.01,
            output_cost_per_m: 0.01,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "cerebras/llama-4-scout-17b".into(),
            display_name: "Llama 4 Scout (Cerebras)".into(),
            provider: "cerebras".into(),
            tier: ModelTier::Smart,
            context_window: 512_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.10,
            output_cost_per_m: 0.10,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "cerebras/qwen-2.5-32b".into(),
            display_name: "Qwen 2.5 32B (Cerebras)".into(),
            provider: "cerebras".into(),
            tier: ModelTier::Balanced,
            context_window: 32_768,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.06,
            output_cost_per_m: 0.06,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        // ══════════════════════════════════════════════════════════════
        // SambaNova (3)
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "sambanova/llama-3.3-70b".into(),
            display_name: "Llama 3.3 70B (SambaNova)".into(),
            provider: "sambanova".into(),
            tier: ModelTier::Balanced,
            context_window: 128_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.06,
            output_cost_per_m: 0.06,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "sambanova/deepseek-r1".into(),
            display_name: "DeepSeek R1 (SambaNova)".into(),
            provider: "sambanova".into(),
            tier: ModelTier::Smart,
            context_window: 64_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.06,
            output_cost_per_m: 0.06,
            supports_tools: false,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "sambanova/qwen-2.5-72b".into(),
            display_name: "Qwen 2.5 72B (SambaNova)".into(),
            provider: "sambanova".into(),
            tier: ModelTier::Smart,
            context_window: 32_768,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.06,
            output_cost_per_m: 0.06,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        // ══════════════════════════════════════════════════════════════
        // xAI (6)
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "grok-4".into(),
            display_name: "Grok 4".into(),
            provider: "xai".into(),
            tier: ModelTier::Frontier,
            context_window: 256_000,
            max_output_tokens: 32_768,
            input_cost_per_m: 3.0,
            output_cost_per_m: 15.0,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec!["grok".into()],
        },
        ModelCatalogEntry {
            id: "grok-4.1-fast".into(),
            display_name: "Grok 4.1 Fast".into(),
            provider: "xai".into(),
            tier: ModelTier::Fast,
            context_window: 2_000_000,
            max_output_tokens: 32_768,
            input_cost_per_m: 0.20,
            output_cost_per_m: 0.50,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec!["grok-fast".into()],
        },
        ModelCatalogEntry {
            id: "grok-3".into(),
            display_name: "Grok 3".into(),
            provider: "xai".into(),
            tier: ModelTier::Frontier,
            context_window: 131_072,
            max_output_tokens: 32_768,
            input_cost_per_m: 3.0,
            output_cost_per_m: 15.0,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec!["grok3".into()],
        },
        ModelCatalogEntry {
            id: "grok-3-mini".into(),
            display_name: "Grok 3 Mini".into(),
            provider: "xai".into(),
            tier: ModelTier::Balanced,
            context_window: 131_072,
            max_output_tokens: 32_768,
            input_cost_per_m: 0.30,
            output_cost_per_m: 0.50,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "grok-2".into(),
            display_name: "Grok 2".into(),
            provider: "xai".into(),
            tier: ModelTier::Smart,
            context_window: 131_072,
            max_output_tokens: 32_768,
            input_cost_per_m: 2.0,
            output_cost_per_m: 10.0,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "grok-2-mini".into(),
            display_name: "Grok 2 Mini".into(),
            provider: "xai".into(),
            tier: ModelTier::Fast,
            context_window: 131_072,
            max_output_tokens: 32_768,
            input_cost_per_m: 0.30,
            output_cost_per_m: 0.50,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec!["grok-mini".into()],
        },
        // ══════════════════════════════════════════════════════════════
        // Hugging Face (3) + dynamic discovery
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "hf/meta-llama/Llama-3.3-70B-Instruct".into(),
            display_name: "Llama 3.3 70B (HF)".into(),
            provider: "huggingface".into(),
            tier: ModelTier::Balanced,
            context_window: 128_000,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.30,
            output_cost_per_m: 0.30,
            supports_tools: false,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "hf/deepseek-ai/DeepSeek-R1".into(),
            display_name: "DeepSeek R1 (HF)".into(),
            provider: "huggingface".into(),
            tier: ModelTier::Smart,
            context_window: 64_000,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.30,
            output_cost_per_m: 0.30,
            supports_tools: false,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "hf/Qwen/Qwen2.5-72B-Instruct".into(),
            display_name: "Qwen 2.5 72B (HF)".into(),
            provider: "huggingface".into(),
            tier: ModelTier::Balanced,
            context_window: 32_768,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.30,
            output_cost_per_m: 0.30,
            supports_tools: false,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        // ══════════════════════════════════════════════════════════════
        // Replicate (3)
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "replicate/meta-llama-3.3-70b-instruct".into(),
            display_name: "Llama 3.3 70B (Replicate)".into(),
            provider: "replicate".into(),
            tier: ModelTier::Balanced,
            context_window: 128_000,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.40,
            output_cost_per_m: 0.40,
            supports_tools: false,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "replicate/deepseek-r1".into(),
            display_name: "DeepSeek R1 (Replicate)".into(),
            provider: "replicate".into(),
            tier: ModelTier::Smart,
            context_window: 64_000,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.40,
            output_cost_per_m: 0.40,
            supports_tools: false,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "replicate/mistral-7b-instruct".into(),
            display_name: "Mistral 7B (Replicate)".into(),
            provider: "replicate".into(),
            tier: ModelTier::Fast,
            context_window: 32_768,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.05,
            output_cost_per_m: 0.25,
            supports_tools: false,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        // ══════════════════════════════════════════════════════════════
        // GitHub Copilot (2) — free for subscribers
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "copilot/gpt-4o".into(),
            display_name: "GPT-4o (Copilot)".into(),
            provider: "github-copilot".into(),
            tier: ModelTier::Smart,
            context_window: 128_000,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.0,
            output_cost_per_m: 0.0,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec!["copilot-gpt4o".into()],
        },
        ModelCatalogEntry {
            id: "copilot/gpt-4".into(),
            display_name: "GPT-4 (Copilot)".into(),
            provider: "github-copilot".into(),
            tier: ModelTier::Frontier,
            context_window: 128_000,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.0,
            output_cost_per_m: 0.0,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec!["copilot-gpt4".into()],
        },
        // ══════════════════════════════════════════════════════════════
        // Qwen / Alibaba (6)
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "qwen-max".into(),
            display_name: "Qwen Max".into(),
            provider: "qwen".into(),
            tier: ModelTier::Frontier,
            context_window: 32_768,
            max_output_tokens: 8_192,
            input_cost_per_m: 4.00,
            output_cost_per_m: 12.00,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "qwen-plus".into(),
            display_name: "Qwen Plus".into(),
            provider: "qwen".into(),
            tier: ModelTier::Smart,
            context_window: 131_072,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.80,
            output_cost_per_m: 2.00,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec!["qwen".into()],
        },
        ModelCatalogEntry {
            id: "qwen-turbo".into(),
            display_name: "Qwen Turbo".into(),
            provider: "qwen".into(),
            tier: ModelTier::Fast,
            context_window: 131_072,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.30,
            output_cost_per_m: 0.60,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "qwen-vl-plus".into(),
            display_name: "Qwen VL Plus".into(),
            provider: "qwen".into(),
            tier: ModelTier::Smart,
            context_window: 32_768,
            max_output_tokens: 8_192,
            input_cost_per_m: 1.50,
            output_cost_per_m: 4.50,
            supports_tools: false,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "qwen-coder-plus".into(),
            display_name: "Qwen Coder Plus".into(),
            provider: "qwen".into(),
            tier: ModelTier::Smart,
            context_window: 131_072,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.80,
            output_cost_per_m: 2.00,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "qwen-long".into(),
            display_name: "Qwen Long".into(),
            provider: "qwen".into(),
            tier: ModelTier::Balanced,
            context_window: 1_000_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.50,
            output_cost_per_m: 2.00,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        // ══════════════════════════════════════════════════════════════
        // MiniMax (4)
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "minimax-text-01".into(),
            display_name: "MiniMax Text 01".into(),
            provider: "minimax".into(),
            tier: ModelTier::Smart,
            context_window: 1_048_576,
            max_output_tokens: 16_384,
            input_cost_per_m: 1.00,
            output_cost_per_m: 3.00,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec!["minimax".into()],
        },
        ModelCatalogEntry {
            id: "MiniMax-M2.5".into(),
            display_name: "MiniMax M2.5".into(),
            provider: "minimax".into(),
            tier: ModelTier::Frontier,
            context_window: 1_048_576,
            max_output_tokens: 16_384,
            input_cost_per_m: 1.10,
            output_cost_per_m: 4.40,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec!["minimax-m2.5".into()],
        },
        ModelCatalogEntry {
            id: "MiniMax-M2.1".into(),
            display_name: "MiniMax M2.1".into(),
            provider: "minimax".into(),
            tier: ModelTier::Smart,
            context_window: 1_048_576,
            max_output_tokens: 16_384,
            input_cost_per_m: 1.00,
            output_cost_per_m: 3.00,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec!["minimax-m2.1".into()],
        },
        ModelCatalogEntry {
            id: "abab6.5-chat".into(),
            display_name: "ABAB 6.5 Chat".into(),
            provider: "minimax".into(),
            tier: ModelTier::Balanced,
            context_window: 245_760,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.50,
            output_cost_per_m: 1.50,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        // ══════════════════════════════════════════════════════════════
        // Zhipu AI / GLM (4)
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "glm-4-plus".into(),
            display_name: "GLM-4 Plus".into(),
            provider: "zhipu".into(),
            tier: ModelTier::Smart,
            context_window: 131_072,
            max_output_tokens: 8_192,
            input_cost_per_m: 1.50,
            output_cost_per_m: 5.00,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec!["glm".into()],
        },
        ModelCatalogEntry {
            id: "glm-4-flash".into(),
            display_name: "GLM-4 Flash".into(),
            provider: "zhipu".into(),
            tier: ModelTier::Fast,
            context_window: 131_072,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.10,
            output_cost_per_m: 0.10,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "glm-4v-plus".into(),
            display_name: "GLM-4V Plus".into(),
            provider: "zhipu".into(),
            tier: ModelTier::Smart,
            context_window: 8_192,
            max_output_tokens: 4_096,
            input_cost_per_m: 2.00,
            output_cost_per_m: 5.00,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "glm-4-long".into(),
            display_name: "GLM-4 Long".into(),
            provider: "zhipu".into(),
            tier: ModelTier::Balanced,
            context_window: 1_000_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.10,
            output_cost_per_m: 0.10,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        // ══════════════════════════════════════════════════════════════
        // Zhipu Coding / CodeGeeX (1)
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "codegeex-4".into(),
            display_name: "CodeGeeX 4".into(),
            provider: "zhipu_coding".into(),
            tier: ModelTier::Smart,
            context_window: 131_072,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.10,
            output_cost_per_m: 0.10,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec!["codegeex".into()],
        },
        // ══════════════════════════════════════════════════════════════
        // Moonshot / Kimi (3)
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "moonshot-v1-128k".into(),
            display_name: "Moonshot V1 128K".into(),
            provider: "moonshot".into(),
            tier: ModelTier::Smart,
            context_window: 131_072,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.80,
            output_cost_per_m: 0.80,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec!["kimi".into()],
        },
        ModelCatalogEntry {
            id: "moonshot-v1-32k".into(),
            display_name: "Moonshot V1 32K".into(),
            provider: "moonshot".into(),
            tier: ModelTier::Balanced,
            context_window: 32_768,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.30,
            output_cost_per_m: 0.30,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "moonshot-v1-8k".into(),
            display_name: "Moonshot V1 8K".into(),
            provider: "moonshot".into(),
            tier: ModelTier::Fast,
            context_window: 8_192,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.10,
            output_cost_per_m: 0.10,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        // ══════════════════════════════════════════════════════════════
        // Baidu Qianfan / ERNIE (3)
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "ernie-4.5-8k".into(),
            display_name: "ERNIE 4.5 8K".into(),
            provider: "qianfan".into(),
            tier: ModelTier::Smart,
            context_window: 8_192,
            max_output_tokens: 4_096,
            input_cost_per_m: 2.00,
            output_cost_per_m: 6.00,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec!["ernie".into()],
        },
        ModelCatalogEntry {
            id: "ernie-4.0-turbo-8k".into(),
            display_name: "ERNIE 4.0 Turbo 8K".into(),
            provider: "qianfan".into(),
            tier: ModelTier::Balanced,
            context_window: 8_192,
            max_output_tokens: 4_096,
            input_cost_per_m: 1.00,
            output_cost_per_m: 3.00,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "ernie-speed-128k".into(),
            display_name: "ERNIE Speed 128K".into(),
            provider: "qianfan".into(),
            tier: ModelTier::Fast,
            context_window: 131_072,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.0,
            output_cost_per_m: 0.0,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        // ══════════════════════════════════════════════════════════════
        // AWS Bedrock (8)
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "bedrock/anthropic.claude-opus-4-6".into(),
            display_name: "Claude Opus 4.6 (Bedrock)".into(),
            provider: "bedrock".into(),
            tier: ModelTier::Frontier,
            context_window: 200_000,
            max_output_tokens: 128_000,
            input_cost_per_m: 5.00,
            output_cost_per_m: 25.00,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "bedrock/anthropic.claude-sonnet-4-6".into(),
            display_name: "Claude Sonnet 4.6 (Bedrock)".into(),
            provider: "bedrock".into(),
            tier: ModelTier::Smart,
            context_window: 200_000,
            max_output_tokens: 64_000,
            input_cost_per_m: 3.00,
            output_cost_per_m: 15.00,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "bedrock/anthropic.claude-opus-4-20250514".into(),
            display_name: "Claude Opus 4 (Bedrock)".into(),
            provider: "bedrock".into(),
            tier: ModelTier::Frontier,
            context_window: 200_000,
            max_output_tokens: 32_000,
            input_cost_per_m: 15.00,
            output_cost_per_m: 75.00,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "bedrock/anthropic.claude-sonnet-4-20250514".into(),
            display_name: "Claude Sonnet 4 (Bedrock)".into(),
            provider: "bedrock".into(),
            tier: ModelTier::Smart,
            context_window: 200_000,
            max_output_tokens: 64_000,
            input_cost_per_m: 3.00,
            output_cost_per_m: 15.00,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "bedrock/anthropic.claude-haiku-4-5-20251001".into(),
            display_name: "Claude Haiku 4.5 (Bedrock)".into(),
            provider: "bedrock".into(),
            tier: ModelTier::Fast,
            context_window: 200_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.25,
            output_cost_per_m: 1.25,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "bedrock/amazon.nova-pro-v1:0".into(),
            display_name: "Amazon Nova Pro (Bedrock)".into(),
            provider: "bedrock".into(),
            tier: ModelTier::Smart,
            context_window: 300_000,
            max_output_tokens: 5_120,
            input_cost_per_m: 0.80,
            output_cost_per_m: 3.20,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "bedrock/amazon.nova-lite-v1:0".into(),
            display_name: "Amazon Nova Lite (Bedrock)".into(),
            provider: "bedrock".into(),
            tier: ModelTier::Fast,
            context_window: 300_000,
            max_output_tokens: 5_120,
            input_cost_per_m: 0.06,
            output_cost_per_m: 0.24,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec![],
        },
        ModelCatalogEntry {
            id: "bedrock/meta.llama3-3-70b-instruct-v1:0".into(),
            display_name: "Llama 3.3 70B (Bedrock)".into(),
            provider: "bedrock".into(),
            tier: ModelTier::Balanced,
            context_window: 128_000,
            max_output_tokens: 4_096,
            input_cost_per_m: 0.72,
            output_cost_per_m: 0.72,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec![],
        },
        // ══════════════════════════════════════════════════════════════
        // OpenAI Codex (2) — reuses OpenAI driver
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "codex/gpt-4.1".into(),
            display_name: "GPT-4.1 (Codex)".into(),
            provider: "codex".into(),
            tier: ModelTier::Frontier,
            context_window: 1_047_576,
            max_output_tokens: 32_768,
            input_cost_per_m: 2.00,
            output_cost_per_m: 8.00,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec!["codex".into(), "codex-4.1".into()],
        },
        ModelCatalogEntry {
            id: "codex/o4-mini".into(),
            display_name: "o4-mini (Codex)".into(),
            provider: "codex".into(),
            tier: ModelTier::Smart,
            context_window: 200_000,
            max_output_tokens: 100_000,
            input_cost_per_m: 1.10,
            output_cost_per_m: 4.40,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            aliases: vec!["codex-o4".into()],
        },
        // ══════════════════════════════════════════════════════════════
        // Claude Code CLI (3) — subprocess-based
        // ══════════════════════════════════════════════════════════════
        ModelCatalogEntry {
            id: "claude-code/opus".into(),
            display_name: "Claude Opus (CLI)".into(),
            provider: "claude-code".into(),
            tier: ModelTier::Frontier,
            context_window: 200_000,
            max_output_tokens: 128_000,
            input_cost_per_m: 5.0,
            output_cost_per_m: 25.0,
            supports_tools: false,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec!["claude-code-opus".into()],
        },
        ModelCatalogEntry {
            id: "claude-code/sonnet".into(),
            display_name: "Claude Sonnet (CLI)".into(),
            provider: "claude-code".into(),
            tier: ModelTier::Smart,
            context_window: 200_000,
            max_output_tokens: 64_000,
            input_cost_per_m: 3.0,
            output_cost_per_m: 15.0,
            supports_tools: false,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec!["claude-code".into(), "claude-code-sonnet".into()],
        },
        ModelCatalogEntry {
            id: "claude-code/haiku".into(),
            display_name: "Claude Haiku (CLI)".into(),
            provider: "claude-code".into(),
            tier: ModelTier::Fast,
            context_window: 200_000,
            max_output_tokens: 8_192,
            input_cost_per_m: 0.25,
            output_cost_per_m: 1.25,
            supports_tools: false,
            supports_vision: false,
            supports_streaming: true,
            aliases: vec!["claude-code-haiku".into()],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_has_models() {
        let catalog = ModelCatalog::new();
        assert!(catalog.list_models().len() >= 30);
    }

    #[test]
    fn test_catalog_has_providers() {
        let catalog = ModelCatalog::new();
        assert_eq!(catalog.list_providers().len(), 30);
    }

    #[test]
    fn test_find_model_by_id() {
        let catalog = ModelCatalog::new();
        let entry = catalog.find_model("claude-sonnet-4-20250514").unwrap();
        assert_eq!(entry.display_name, "Claude Sonnet 4");
        assert_eq!(entry.provider, "anthropic");
        assert_eq!(entry.tier, ModelTier::Smart);
    }

    #[test]
    fn test_find_model_by_alias() {
        let catalog = ModelCatalog::new();
        let entry = catalog.find_model("sonnet").unwrap();
        assert_eq!(entry.id, "claude-sonnet-4-6");
    }

    #[test]
    fn test_find_model_case_insensitive() {
        let catalog = ModelCatalog::new();
        assert!(catalog.find_model("Claude-Sonnet-4-20250514").is_some());
        assert!(catalog.find_model("SONNET").is_some());
    }

    #[test]
    fn test_find_model_not_found() {
        let catalog = ModelCatalog::new();
        assert!(catalog.find_model("nonexistent-model").is_none());
    }

    #[test]
    fn test_resolve_alias() {
        let catalog = ModelCatalog::new();
        assert_eq!(
            catalog.resolve_alias("sonnet"),
            Some("claude-sonnet-4-6")
        );
        assert_eq!(
            catalog.resolve_alias("haiku"),
            Some("claude-haiku-4-5-20251001")
        );
        assert!(catalog.resolve_alias("nonexistent").is_none());
    }

    #[test]
    fn test_models_by_provider() {
        let catalog = ModelCatalog::new();
        let anthropic = catalog.models_by_provider("anthropic");
        assert_eq!(anthropic.len(), 7);
        assert!(anthropic.iter().all(|m| m.provider == "anthropic"));
    }

    #[test]
    fn test_models_by_tier() {
        let catalog = ModelCatalog::new();
        let frontier = catalog.models_by_tier(ModelTier::Frontier);
        assert!(frontier.len() >= 3); // At least opus, gpt-4.1, gemini-2.5-pro
        assert!(frontier.iter().all(|m| m.tier == ModelTier::Frontier));
    }

    #[test]
    fn test_pricing_lookup() {
        let catalog = ModelCatalog::new();
        let (input, output) = catalog.pricing("claude-sonnet-4-20250514").unwrap();
        assert!((input - 3.0).abs() < 0.001);
        assert!((output - 15.0).abs() < 0.001);
    }

    #[test]
    fn test_pricing_via_alias() {
        let catalog = ModelCatalog::new();
        let (input, output) = catalog.pricing("sonnet").unwrap();
        assert!((input - 3.0).abs() < 0.001);
        assert!((output - 15.0).abs() < 0.001);
    }

    #[test]
    fn test_pricing_not_found() {
        let catalog = ModelCatalog::new();
        assert!(catalog.pricing("nonexistent").is_none());
    }

    #[test]
    fn test_detect_auth_local_providers() {
        let mut catalog = ModelCatalog::new();
        catalog.detect_auth();
        // Local providers should be NotRequired
        let ollama = catalog.get_provider("ollama").unwrap();
        assert_eq!(ollama.auth_status, AuthStatus::NotRequired);
        let vllm = catalog.get_provider("vllm").unwrap();
        assert_eq!(vllm.auth_status, AuthStatus::NotRequired);
    }

    #[test]
    fn test_available_models_includes_local() {
        let mut catalog = ModelCatalog::new();
        catalog.detect_auth();
        let available = catalog.available_models();
        // Local providers (ollama, vllm, lmstudio) should always be available
        assert!(available.iter().any(|m| m.provider == "ollama"));
    }

    #[test]
    fn test_provider_model_counts() {
        let catalog = ModelCatalog::new();
        let anthropic = catalog.get_provider("anthropic").unwrap();
        assert_eq!(anthropic.model_count, 7);
        let groq = catalog.get_provider("groq").unwrap();
        assert_eq!(groq.model_count, 10);
    }

    #[test]
    fn test_list_aliases() {
        let catalog = ModelCatalog::new();
        let aliases = catalog.list_aliases();
        assert!(aliases.len() >= 20);
        assert_eq!(aliases.get("sonnet").unwrap(), "claude-sonnet-4-6");
        // New aliases
        assert_eq!(aliases.get("grok").unwrap(), "grok-4");
        assert_eq!(aliases.get("jamba").unwrap(), "jamba-1.5-large");
    }

    #[test]
    fn test_find_grok_by_alias() {
        let catalog = ModelCatalog::new();
        let entry = catalog.find_model("grok").unwrap();
        assert_eq!(entry.id, "grok-4");
        assert_eq!(entry.provider, "xai");
    }

    #[test]
    fn test_new_providers_in_catalog() {
        let catalog = ModelCatalog::new();
        assert!(catalog.get_provider("perplexity").is_some());
        assert!(catalog.get_provider("cohere").is_some());
        assert!(catalog.get_provider("ai21").is_some());
        assert!(catalog.get_provider("cerebras").is_some());
        assert!(catalog.get_provider("sambanova").is_some());
        assert!(catalog.get_provider("huggingface").is_some());
        assert!(catalog.get_provider("xai").is_some());
        assert!(catalog.get_provider("replicate").is_some());
    }

    #[test]
    fn test_xai_models() {
        let catalog = ModelCatalog::new();
        let xai = catalog.models_by_provider("xai");
        assert_eq!(xai.len(), 6);
        assert!(xai.iter().any(|m| m.id == "grok-4"));
        assert!(xai.iter().any(|m| m.id == "grok-4.1-fast"));
        assert!(xai.iter().any(|m| m.id == "grok-3"));
        assert!(xai.iter().any(|m| m.id == "grok-3-mini"));
        assert!(xai.iter().any(|m| m.id == "grok-2"));
        assert!(xai.iter().any(|m| m.id == "grok-2-mini"));
    }

    #[test]
    fn test_perplexity_models() {
        let catalog = ModelCatalog::new();
        let pp = catalog.models_by_provider("perplexity");
        assert_eq!(pp.len(), 4);
    }

    #[test]
    fn test_cohere_models() {
        let catalog = ModelCatalog::new();
        let co = catalog.models_by_provider("cohere");
        assert_eq!(co.len(), 4);
    }

    #[test]
    fn test_default_creates_valid_catalog() {
        let catalog = ModelCatalog::default();
        assert!(!catalog.list_models().is_empty());
        assert!(!catalog.list_providers().is_empty());
    }

    #[test]
    fn test_merge_adds_new_models() {
        let mut catalog = ModelCatalog::new();
        let before = catalog.models_by_provider("ollama").len();
        catalog.merge_discovered_models(
            "ollama",
            &["codestral:latest".to_string(), "qwen2:7b".to_string()],
        );
        let after = catalog.models_by_provider("ollama").len();
        assert_eq!(after, before + 2);
        // Verify the new models are Local tier with zero cost
        let qwen = catalog.find_model("qwen2:7b").unwrap();
        assert_eq!(qwen.tier, ModelTier::Local);
        assert!((qwen.input_cost_per_m).abs() < f64::EPSILON);
    }

    #[test]
    fn test_merge_skips_existing() {
        let mut catalog = ModelCatalog::new();
        // "llama3.2" is already a builtin Ollama model
        let before = catalog.list_models().len();
        catalog.merge_discovered_models("ollama", &["llama3.2".to_string()]);
        let after = catalog.list_models().len();
        assert_eq!(after, before); // no new model added
    }

    #[test]
    fn test_merge_updates_model_count() {
        let mut catalog = ModelCatalog::new();
        let before_count = catalog.get_provider("ollama").unwrap().model_count;
        catalog.merge_discovered_models("ollama", &["new-model:latest".to_string()]);
        let after_count = catalog.get_provider("ollama").unwrap().model_count;
        assert_eq!(after_count, before_count + 1);
    }

    #[test]
    fn test_chinese_providers_in_catalog() {
        let catalog = ModelCatalog::new();
        assert!(catalog.get_provider("qwen").is_some());
        assert!(catalog.get_provider("minimax").is_some());
        assert!(catalog.get_provider("zhipu").is_some());
        assert!(catalog.get_provider("zhipu_coding").is_some());
        assert!(catalog.get_provider("moonshot").is_some());
        assert!(catalog.get_provider("qianfan").is_some());
        assert!(catalog.get_provider("bedrock").is_some());
    }

    #[test]
    fn test_chinese_model_aliases() {
        let catalog = ModelCatalog::new();
        assert!(catalog.find_model("kimi").is_some());
        assert!(catalog.find_model("glm").is_some());
        assert!(catalog.find_model("codegeex").is_some());
        assert!(catalog.find_model("ernie").is_some());
        assert!(catalog.find_model("minimax").is_some());
        // MiniMax M2.5 — by exact ID, alias, and case-insensitive
        let m25 = catalog.find_model("MiniMax-M2.5").unwrap();
        assert_eq!(m25.provider, "minimax");
        assert_eq!(m25.tier, ModelTier::Frontier);
        assert!(catalog.find_model("minimax-m2.5").is_some());
        // Default "minimax" alias now points to M2.5
        let default = catalog.find_model("minimax").unwrap();
        assert_eq!(default.id, "MiniMax-M2.5");
    }

    #[test]
    fn test_bedrock_models() {
        let catalog = ModelCatalog::new();
        let bedrock = catalog.models_by_provider("bedrock");
        assert_eq!(bedrock.len(), 8);
    }

    #[test]
    fn test_set_provider_url() {
        let mut catalog = ModelCatalog::new();
        let old_url = catalog.get_provider("ollama").unwrap().base_url.clone();
        assert_eq!(old_url, OLLAMA_BASE_URL);

        let updated = catalog.set_provider_url("ollama", "http://192.168.1.100:11434/v1");
        assert!(updated);
        assert_eq!(
            catalog.get_provider("ollama").unwrap().base_url,
            "http://192.168.1.100:11434/v1"
        );
    }

    #[test]
    fn test_set_provider_url_unknown() {
        let mut catalog = ModelCatalog::new();
        let updated = catalog.set_provider_url("nonexistent", "http://localhost:9999");
        assert!(!updated);
    }

    #[test]
    fn test_apply_url_overrides() {
        let mut catalog = ModelCatalog::new();
        let mut overrides = HashMap::new();
        overrides.insert("ollama".to_string(), "http://10.0.0.5:11434/v1".to_string());
        overrides.insert("vllm".to_string(), "http://10.0.0.6:8000/v1".to_string());
        overrides.insert("nonexistent".to_string(), "http://nowhere".to_string());

        catalog.apply_url_overrides(&overrides);

        assert_eq!(
            catalog.get_provider("ollama").unwrap().base_url,
            "http://10.0.0.5:11434/v1"
        );
        assert_eq!(
            catalog.get_provider("vllm").unwrap().base_url,
            "http://10.0.0.6:8000/v1"
        );
        // lmstudio should be unchanged
        assert_eq!(
            catalog.get_provider("lmstudio").unwrap().base_url,
            LMSTUDIO_BASE_URL
        );
    }

    #[test]
    fn test_codex_provider() {
        let catalog = ModelCatalog::new();
        let codex = catalog.get_provider("codex").unwrap();
        assert_eq!(codex.display_name, "OpenAI Codex");
        assert_eq!(codex.api_key_env, "OPENAI_API_KEY");
        assert!(codex.key_required);
    }

    #[test]
    fn test_codex_models() {
        let catalog = ModelCatalog::new();
        let models = catalog.models_by_provider("codex");
        assert_eq!(models.len(), 2);
        assert!(models.iter().any(|m| m.id == "codex/gpt-4.1"));
        assert!(models.iter().any(|m| m.id == "codex/o4-mini"));
    }

    #[test]
    fn test_codex_aliases() {
        let catalog = ModelCatalog::new();
        let entry = catalog.find_model("codex").unwrap();
        assert_eq!(entry.id, "codex/gpt-4.1");
    }

    #[test]
    fn test_claude_code_provider() {
        let catalog = ModelCatalog::new();
        let cc = catalog.get_provider("claude-code").unwrap();
        assert_eq!(cc.display_name, "Claude Code");
        assert!(!cc.key_required);
    }

    #[test]
    fn test_claude_code_models() {
        let catalog = ModelCatalog::new();
        let models = catalog.models_by_provider("claude-code");
        assert_eq!(models.len(), 3);
        assert!(models.iter().any(|m| m.id == "claude-code/opus"));
        assert!(models.iter().any(|m| m.id == "claude-code/sonnet"));
        assert!(models.iter().any(|m| m.id == "claude-code/haiku"));
    }

    #[test]
    fn test_claude_code_aliases() {
        let catalog = ModelCatalog::new();
        let entry = catalog.find_model("claude-code").unwrap();
        assert_eq!(entry.id, "claude-code/sonnet");
    }
}
