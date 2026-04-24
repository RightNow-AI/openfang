//! Ollama driver — thin wrapper around the OpenAI-compatible driver.
//!
//! Ollama speaks OpenAI's `/v1/chat/completions` dialect well, but emits opaque
//! 404s when the requested model isn't pulled (`model "llama3.2:70b" not found`).
//! This wrapper detects that specific failure and enriches the error with the
//! list of models actually available on the local Ollama instance, fetched
//! on-demand from Ollama's native `/api/tags` endpoint.
//!
//! No cache: the probe only runs on the error path, so there's nothing worth
//! caching. The hot path is pure delegation to the inner OpenAIDriver.

use crate::llm_driver::{CompletionRequest, CompletionResponse, LlmDriver, LlmError, StreamEvent};
use async_trait::async_trait;
use std::time::Duration;
use tokio::sync::mpsc;

/// Ollama's native tags endpoint ships at the root, not under `/v1`.
const TAGS_PATH: &str = "/api/tags";

/// Max time to spend fetching tags during error enrichment.
const TAGS_PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// Strip any trailing `/v1` (with or without a trailing slash) from a base URL
/// so the result addresses the Ollama root — where `/api/tags` lives.
pub(crate) fn ollama_root_from_base(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_string()
}

/// Returns `true` if the error looks like Ollama reporting an unknown model.
pub(crate) fn looks_like_model_not_found(err: &LlmError) -> bool {
    match err {
        LlmError::ModelNotFound(_) => true,
        LlmError::Api { status, message } => {
            let m = message.to_ascii_lowercase();
            (*status == 404 && m.contains("model"))
                || (m.contains("model") && m.contains("not found"))
                || m.contains("try pulling")
        }
        _ => false,
    }
}

/// Build the user-facing error message when we know the available tags.
pub(crate) fn format_model_not_found(requested: &str, available: &[String]) -> String {
    if available.is_empty() {
        return format!(
            "Ollama model '{requested}' not found and no local models are pulled. \
             Run: `ollama pull {requested}`"
        );
    }
    format!(
        "Ollama model '{requested}' not found. Available locally: {}. \
         To pull: `ollama pull {requested}`",
        available.join(", ")
    )
}

/// Wraps an [`OpenAIDriver`](super::openai::OpenAIDriver) with Ollama-specific
/// error enrichment.
pub struct OllamaDriver {
    inner: super::openai::OpenAIDriver,
    root_url: String,
    http: reqwest::Client,
}

impl OllamaDriver {
    /// Wrap a configured OpenAI-compatible driver. `root_url` should be the Ollama
    /// root (no `/v1` suffix); callers typically use [`ollama_root_from_base`].
    pub fn new(inner: super::openai::OpenAIDriver, root_url: String) -> Self {
        Self {
            inner,
            root_url,
            http: reqwest::Client::builder()
                .user_agent(crate::USER_AGENT)
                .build()
                .unwrap_or_default(),
        }
    }

    /// Hit `/api/tags` once and extract model names. Returns `None` on any failure
    /// — we never mask the original error with a probe failure.
    async fn fetch_tags(&self) -> Option<Vec<String>> {
        let url = format!("{}{TAGS_PATH}", self.root_url.trim_end_matches('/'));
        let resp = self
            .http
            .get(&url)
            .timeout(TAGS_PROBE_TIMEOUT)
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let body: serde_json::Value = resp.json().await.ok()?;
        let models = body.get("models")?.as_array()?;
        let names: Vec<String> = models
            .iter()
            .filter_map(|m| m.get("name").and_then(|n| n.as_str()).map(String::from))
            .collect();
        Some(names)
    }

    async fn maybe_enrich(&self, err: LlmError, requested_model: &str) -> LlmError {
        if !looks_like_model_not_found(&err) {
            return err;
        }
        match self.fetch_tags().await {
            Some(tags) => LlmError::ModelNotFound(format_model_not_found(requested_model, &tags)),
            None => err,
        }
    }
}

#[async_trait]
impl LlmDriver for OllamaDriver {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let model = request.model.clone();
        match self.inner.complete(request).await {
            Ok(r) => Ok(r),
            Err(e) => Err(self.maybe_enrich(e, &model).await),
        }
    }

    async fn stream(
        &self,
        request: CompletionRequest,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<CompletionResponse, LlmError> {
        let model = request.model.clone();
        match self.inner.stream(request, tx).await {
            Ok(r) => Ok(r),
            Err(e) => Err(self.maybe_enrich(e, &model).await),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_strips_v1_suffix() {
        assert_eq!(
            ollama_root_from_base("http://127.0.0.1:11434/v1"),
            "http://127.0.0.1:11434"
        );
        assert_eq!(
            ollama_root_from_base("http://localhost:11434/v1/"),
            "http://localhost:11434"
        );
    }

    #[test]
    fn root_unchanged_without_v1() {
        assert_eq!(
            ollama_root_from_base("http://127.0.0.1:11434"),
            "http://127.0.0.1:11434"
        );
        assert_eq!(
            ollama_root_from_base("http://127.0.0.1:11434/"),
            "http://127.0.0.1:11434"
        );
    }

    #[test]
    fn root_preserves_custom_path() {
        // If someone runs ollama behind a reverse proxy at /ollama, we must not
        // accidentally eat that path.
        assert_eq!(
            ollama_root_from_base("http://proxy.local/ollama/v1"),
            "http://proxy.local/ollama"
        );
        assert_eq!(
            ollama_root_from_base("http://proxy.local/ollama"),
            "http://proxy.local/ollama"
        );
    }

    #[test]
    fn classify_model_not_found_explicit_variant() {
        assert!(looks_like_model_not_found(&LlmError::ModelNotFound(
            "x".to_string()
        )));
    }

    #[test]
    fn classify_api_404_with_model_word() {
        assert!(looks_like_model_not_found(&LlmError::Api {
            status: 404,
            message: "model \"llama3.2:70b\" not found, try pulling it first".to_string(),
        }));
    }

    #[test]
    fn classify_ollama_try_pulling_phrasing() {
        // Older Ollama versions return a plain 500 with `try pulling it first`.
        assert!(looks_like_model_not_found(&LlmError::Api {
            status: 500,
            message: "llama runner error: try pulling it first".to_string(),
        }));
    }

    #[test]
    fn classify_rejects_unrelated_errors() {
        assert!(!looks_like_model_not_found(&LlmError::RateLimited {
            retry_after_ms: 1000,
        }));
        assert!(!looks_like_model_not_found(&LlmError::Api {
            status: 500,
            message: "internal server error".to_string(),
        }));
        assert!(!looks_like_model_not_found(&LlmError::Http("timeout".into())));
    }

    #[test]
    fn format_with_available_lists_them() {
        let msg = format_model_not_found(
            "llama3.2:70b",
            &["llama3.2:3b".to_string(), "qwen2.5:7b".to_string()],
        );
        assert!(msg.contains("llama3.2:70b"));
        assert!(msg.contains("llama3.2:3b"));
        assert!(msg.contains("qwen2.5:7b"));
        assert!(msg.contains("ollama pull"));
    }

    #[test]
    fn format_with_empty_hints_first_pull() {
        let msg = format_model_not_found("qwen2.5:7b", &[]);
        assert!(msg.contains("qwen2.5:7b"));
        assert!(msg.contains("no local models"));
        assert!(msg.contains("ollama pull"));
    }
}
