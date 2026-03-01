//! Provider health probing — lightweight HTTP checks for local LLM providers.
//!
//! Probes local providers (Ollama, vLLM, LM Studio) for reachability and
//! dynamically discovers which models they currently serve. For Ollama,
//! enriches discovered models with real metadata from `/api/show`.

use std::time::Instant;

/// Metadata for a model discovered from a local provider's listing endpoint.
///
/// For Ollama, enriched with data from `POST /api/show`. For other providers
/// (vLLM, LM Studio), only `name` is populated and all other fields are `None`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DiscoveredModel {
    /// Model name/ID as returned by the listing endpoint.
    pub name: String,
    /// Context window size in tokens.
    pub context_window: Option<u64>,
    /// Maximum output tokens.
    pub max_output_tokens: Option<u64>,
    /// Whether the model supports vision/image inputs.
    pub supports_vision: Option<bool>,
    /// Whether the model supports tool/function calling.
    pub supports_tools: Option<bool>,
    /// Model family (e.g., "llama", "gemma", "qwen2").
    pub family: Option<String>,
    /// Model families list (e.g., \["llama", "clip"\] for multimodal).
    pub families: Option<Vec<String>>,
    /// Parameter size string (e.g., "8B", "70B").
    pub parameter_size: Option<String>,
    /// Quantization level (e.g., "Q4_K_M", "Q8_0").
    pub quantization_level: Option<String>,
}

/// Result of probing a provider endpoint.
#[derive(Debug, Clone, Default)]
pub struct ProbeResult {
    /// Whether the provider responded successfully.
    pub reachable: bool,
    /// Round-trip latency in milliseconds.
    pub latency_ms: u64,
    /// Models discovered from the provider's listing endpoint.
    pub discovered_models: Vec<DiscoveredModel>,
    /// Error message if the probe failed.
    pub error: Option<String>,
}

/// Check if a provider is a local provider (no key required, localhost URL).
///
/// Returns true for `"ollama"`, `"vllm"`, `"lmstudio"`.
pub fn is_local_provider(provider: &str) -> bool {
    matches!(
        provider.to_lowercase().as_str(),
        "ollama" | "vllm" | "lmstudio"
    )
}

/// Probe timeout for local provider health checks.
const PROBE_TIMEOUT_SECS: u64 = 5;

/// Timeout for individual `/api/show` calls (per model).
const SHOW_TIMEOUT_SECS: u64 = 10;

/// Maximum concurrent `/api/show` requests to avoid overwhelming Ollama.
const OLLAMA_SHOW_CONCURRENCY: usize = 4;

/// Probe a provider's health by hitting its model listing endpoint.
///
/// - **Ollama**: `GET {base_url_root}/api/tags` → parses `.models[].name`,
///   then enriches each model via `POST /api/show` for real metadata.
/// - **OpenAI-compat** (vLLM, LM Studio): `GET {base_url}/models` → parses `.data[].id`
///
/// `base_url` should be the provider's base URL from the catalog (e.g.,
/// `http://localhost:11434/v1` for Ollama, `http://localhost:8000/v1` for vLLM).
pub async fn probe_provider(provider: &str, base_url: &str) -> ProbeResult {
    let start = Instant::now();

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(PROBE_TIMEOUT_SECS))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return ProbeResult {
                error: Some(format!("Failed to build HTTP client: {e}")),
                ..Default::default()
            };
        }
    };

    let lower = provider.to_lowercase();

    // Ollama uses a non-OpenAI endpoint for model listing
    let ollama_root = if lower == "ollama" {
        Some(
            base_url
                .trim_end_matches('/')
                .trim_end_matches("/v1")
                .trim_end_matches("/v1/")
                .to_string(),
        )
    } else {
        None
    };
    let is_ollama = ollama_root.is_some();

    let url = if let Some(ref root) = ollama_root {
        format!("{root}/api/tags")
    } else {
        let trimmed = base_url.trim_end_matches('/');
        format!("{trimmed}/models")
    };

    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            return ProbeResult {
                latency_ms: start.elapsed().as_millis() as u64,
                error: Some(format!("{e}")),
                ..Default::default()
            };
        }
    };

    if !resp.status().is_success() {
        return ProbeResult {
            latency_ms: start.elapsed().as_millis() as u64,
            error: Some(format!("HTTP {}", resp.status())),
            ..Default::default()
        };
    }

    let body: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return ProbeResult {
                reachable: true, // server responded, just bad JSON
                latency_ms: start.elapsed().as_millis() as u64,
                error: Some(format!("Invalid JSON: {e}")),
                ..Default::default()
            };
        }
    };

    let latency_ms = start.elapsed().as_millis() as u64;

    // Parse model names
    let model_names: Vec<String> = if is_ollama {
        // Ollama: { "models": [ { "name": "llama3.2:latest", ... }, ... ] }
        body.get("models")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        m.get("name")
                            .and_then(|n| n.as_str())
                            .map(|s| s.to_string())
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        // OpenAI-compatible: { "data": [ { "id": "model-name", ... }, ... ] }
        body.get("data")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("id").and_then(|n| n.as_str()).map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    };

    // For Ollama, enrich models with /api/show metadata
    let discovered_models = if is_ollama && !model_names.is_empty() {
        enrich_ollama_models(ollama_root.as_deref().unwrap(), model_names).await
    } else {
        model_names
            .into_iter()
            .map(|name| DiscoveredModel {
                name,
                ..Default::default()
            })
            .collect()
    };

    ProbeResult {
        reachable: true,
        latency_ms,
        discovered_models,
        error: None,
    }
}

/// Enrich a list of model names with metadata from Ollama's `/api/show` endpoint.
///
/// Calls `/api/show` for each model in parallel (up to [`OLLAMA_SHOW_CONCURRENCY`]
/// concurrent requests). Models that fail enrichment retain only their name.
async fn enrich_ollama_models(root_url: &str, model_names: Vec<String>) -> Vec<DiscoveredModel> {
    use futures::stream::StreamExt;

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(SHOW_TIMEOUT_SECS))
        .build()
    {
        Ok(c) => c,
        Err(_) => {
            return model_names
                .into_iter()
                .map(|name| DiscoveredModel {
                    name,
                    ..Default::default()
                })
                .collect();
        }
    };

    let root = root_url.to_string();
    futures::stream::iter(model_names)
        .map(|name| {
            let client = client.clone();
            let root = root.clone();
            async move { fetch_ollama_model_info(&client, &root, &name).await }
        })
        .buffer_unordered(OLLAMA_SHOW_CONCURRENCY)
        .collect()
        .await
}

/// Fetch detailed model info from Ollama's `POST /api/show` endpoint.
///
/// Extracts context_window, vision/tool support, model family, parameter size,
/// and quantization level. On failure, returns a `DiscoveredModel` with only
/// the `name` set (all metadata fields `None`).
async fn fetch_ollama_model_info(
    client: &reqwest::Client,
    root_url: &str,
    model_name: &str,
) -> DiscoveredModel {
    let url = format!("{}/api/show", root_url.trim_end_matches('/'));
    let body = serde_json::json!({ "name": model_name });

    let resp = match client.post(&url).json(&body).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => {
            return DiscoveredModel {
                name: model_name.to_string(),
                ..Default::default()
            };
        }
    };

    let json: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(_) => {
            return DiscoveredModel {
                name: model_name.to_string(),
                ..Default::default()
            };
        }
    };

    // Extract context_window from model_info — keys are "{arch}.context_length"
    let context_window = json
        .get("model_info")
        .and_then(|info| {
            info.as_object().and_then(|map| {
                map.iter()
                    .find(|(k, _)| k.ends_with(".context_length"))
                    .and_then(|(_, v)| v.as_u64())
            })
        })
        .or_else(|| parse_num_ctx_from_parameters(json.get("parameters")));

    // Extract vision support — primary: capabilities array
    let supports_vision_from_caps = json
        .get("capabilities")
        .and_then(|c| c.as_array())
        .map(|arr| arr.iter().any(|v| v.as_str() == Some("vision")));

    // Extract families list from details
    let families = json
        .get("details")
        .and_then(|d| d.get("families"))
        .and_then(|f| f.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<String>>()
        });

    // Vision: capabilities > family heuristic > model name heuristic
    let supports_vision = supports_vision_from_caps.or_else(|| {
        families
            .as_ref()
            .map(|fams| fams.iter().any(|f| is_vision_family(f)))
    });

    // Extract tool support from capabilities
    let supports_tools = json
        .get("capabilities")
        .and_then(|c| c.as_array())
        .map(|arr| arr.iter().any(|v| v.as_str() == Some("tools")));

    // Extract details
    let family = json
        .get("details")
        .and_then(|d| d.get("family"))
        .and_then(|f| f.as_str())
        .map(|s| s.to_string());

    let parameter_size = json
        .get("details")
        .and_then(|d| d.get("parameter_size"))
        .and_then(|p| p.as_str())
        .map(|s| s.to_string());

    let quantization_level = json
        .get("details")
        .and_then(|d| d.get("quantization_level"))
        .and_then(|q| q.as_str())
        .map(|s| s.to_string());

    DiscoveredModel {
        name: model_name.to_string(),
        context_window,
        max_output_tokens: None, // Ollama doesn't expose this separately
        supports_vision,
        supports_tools,
        family,
        families,
        parameter_size,
        quantization_level,
    }
}

/// Parse `num_ctx` from Ollama's parameters text field.
///
/// The parameters field is a newline-separated key-value text like:
/// ```text
/// temperature 0.7
/// num_ctx 2048
/// ```
fn parse_num_ctx_from_parameters(params: Option<&serde_json::Value>) -> Option<u64> {
    let text = params?.as_str()?;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        if parts.next() == Some("num_ctx") {
            return parts.next().and_then(|v| v.parse().ok());
        }
    }
    None
}

/// Known vision model families from Ollama.
fn is_vision_family(family: &str) -> bool {
    matches!(
        family.to_lowercase().as_str(),
        "llava" | "bakllava" | "moondream" | "clip" | "mllama"
    )
}

/// Lightweight model probe -- sends a minimal completion request to verify a model is responsive.
///
/// Unlike `probe_provider` which checks the listing endpoint, this actually sends
/// a tiny prompt ("Hi") to verify the model can generate completions. Used by the
/// circuit breaker to re-test a provider during cooldown.
///
/// Returns `Ok(latency_ms)` if the model responds, or `Err(error_message)` if it fails.
pub async fn probe_model(
    provider: &str,
    base_url: &str,
    model: &str,
    api_key: Option<&str>,
) -> Result<u64, String> {
    let start = Instant::now();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "Hi"}],
        "max_tokens": 1,
        "temperature": 0.0
    });

    let mut req = client.post(&url).json(&body);
    if let Some(key) = api_key {
        // Detect provider to set correct auth header
        let lower = provider.to_lowercase();
        if lower == "gemini" {
            req = req.header("x-goog-api-key", key);
        } else {
            req = req.header("Authorization", format!("Bearer {key}"));
        }
    }

    let resp = req.send().await.map_err(|e| format!("{e}"))?;
    let latency = start.elapsed().as_millis() as u64;

    if resp.status().is_success() {
        Ok(latency)
    } else {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        Err(format!("HTTP {status}: {}", &body[..body.len().min(200)]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_local_provider_true_for_ollama() {
        assert!(is_local_provider("ollama"));
        assert!(is_local_provider("Ollama"));
        assert!(is_local_provider("OLLAMA"));
        assert!(is_local_provider("vllm"));
        assert!(is_local_provider("lmstudio"));
    }

    #[test]
    fn test_is_local_provider_false_for_openai() {
        assert!(!is_local_provider("openai"));
        assert!(!is_local_provider("anthropic"));
        assert!(!is_local_provider("gemini"));
        assert!(!is_local_provider("groq"));
    }

    #[test]
    fn test_probe_result_default() {
        let result = ProbeResult::default();
        assert!(!result.reachable);
        assert_eq!(result.latency_ms, 0);
        assert!(result.discovered_models.is_empty());
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn test_probe_unreachable_returns_error() {
        // Probe a port that's almost certainly not running a server
        let result = probe_provider("ollama", "http://127.0.0.1:19999").await;
        assert!(!result.reachable);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_probe_timeout_value() {
        assert_eq!(PROBE_TIMEOUT_SECS, 5);
    }

    #[test]
    fn test_probe_model_url_construction() {
        // Verify the URL format logic used inside probe_model.
        let url = format!(
            "{}/chat/completions",
            "http://localhost:8000/v1".trim_end_matches('/')
        );
        assert_eq!(url, "http://localhost:8000/v1/chat/completions");

        let url2 = format!(
            "{}/chat/completions",
            "http://localhost:8000/v1/".trim_end_matches('/')
        );
        assert_eq!(url2, "http://localhost:8000/v1/chat/completions");
    }

    #[tokio::test]
    async fn test_probe_model_unreachable() {
        let result = probe_model("test", "http://127.0.0.1:19998/v1", "test-model", None).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_num_ctx_from_parameters() {
        let params = serde_json::json!("temperature 0.7\nnum_ctx 4096\ntop_p 0.9");
        assert_eq!(parse_num_ctx_from_parameters(Some(&params)), Some(4096));

        let no_ctx = serde_json::json!("temperature 0.7\ntop_p 0.9");
        assert_eq!(parse_num_ctx_from_parameters(Some(&no_ctx)), None);

        assert_eq!(parse_num_ctx_from_parameters(None), None);
    }

    #[test]
    fn test_is_vision_family() {
        assert!(is_vision_family("llava"));
        assert!(is_vision_family("clip"));
        assert!(is_vision_family("mllama"));
        assert!(is_vision_family("LLAVA"));
        assert!(!is_vision_family("llama"));
        assert!(!is_vision_family("qwen2"));
    }

    #[test]
    fn test_discovered_model_default() {
        let model = DiscoveredModel::default();
        assert!(model.name.is_empty());
        assert!(model.context_window.is_none());
        assert!(model.supports_vision.is_none());
        assert!(model.supports_tools.is_none());
        assert!(model.family.is_none());
    }
}
