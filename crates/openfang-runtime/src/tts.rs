//! Text-to-speech engine — synthesize text to audio.
//!
//! Auto-cascades through available providers based on configured API keys.

use openfang_types::config::TtsConfig;

/// Maximum audio response size (10MB).
const MAX_AUDIO_RESPONSE_BYTES: usize = 10 * 1024 * 1024;

/// Result of TTS synthesis.
#[derive(Debug)]
pub struct TtsResult {
    pub audio_data: Vec<u8>,
    pub format: String,
    pub provider: String,
    pub duration_estimate_ms: u64,
}

/// Text-to-speech engine.
pub struct TtsEngine {
    config: TtsConfig,
    /// Optional override for OpenAI TTS base URL. When set, the engine POSTs
    /// to `<openai_base_url>/v1/audio/speech` instead of the hardcoded
    /// `https://api.openai.com/v1/audio/speech`. Sourced from
    /// `MediaConfig.tts_openai_base_url`. Closes #1051.
    openai_base_url: Option<String>,
    /// Optional override for ElevenLabs TTS base URL. When set, the engine
    /// POSTs to `<elevenlabs_base_url>/v1/text-to-speech/{voice_id}` instead
    /// of the hardcoded `https://api.elevenlabs.io/...`. Sourced from
    /// `MediaConfig.tts_elevenlabs_base_url`. Closes #1051.
    elevenlabs_base_url: Option<String>,
}

impl TtsEngine {
    pub fn new(config: TtsConfig) -> Self {
        Self {
            config,
            openai_base_url: None,
            elevenlabs_base_url: None,
        }
    }

    /// Attach optional base-URL overrides from `MediaConfig`. Use this to
    /// route TTS calls at a local OpenAI-compatible service (e.g.
    /// Lemonade/Kokoro, LM Studio) or an ElevenLabs proxy. Closes #1051.
    pub fn with_base_urls(
        mut self,
        openai_base_url: Option<String>,
        elevenlabs_base_url: Option<String>,
    ) -> Self {
        self.openai_base_url = openai_base_url;
        self.elevenlabs_base_url = elevenlabs_base_url;
        self
    }

    /// Detect which TTS provider is available based on environment variables.
    fn detect_provider() -> Option<&'static str> {
        if std::env::var("OPENAI_API_KEY").is_ok() {
            return Some("openai");
        }
        if std::env::var("ELEVENLABS_API_KEY").is_ok() {
            return Some("elevenlabs");
        }
        None
    }

    /// Synthesize text to audio bytes.
    /// Auto-cascade: configured provider -> OpenAI -> ElevenLabs.
    /// Optional overrides for voice and format (per-request, from tool input).
    pub async fn synthesize(
        &self,
        text: &str,
        voice_override: Option<&str>,
        format_override: Option<&str>,
    ) -> Result<TtsResult, String> {
        if !self.config.enabled {
            return Err("TTS is disabled in configuration".into());
        }

        // Validate text length
        if text.is_empty() {
            return Err("Text cannot be empty".into());
        }
        if text.len() > self.config.max_text_length {
            return Err(format!(
                "Text too long: {} chars (max {})",
                text.len(),
                self.config.max_text_length
            ));
        }

        let provider = self
            .config
            .provider
            .as_deref()
            .or_else(|| Self::detect_provider())
            .ok_or("No TTS provider configured. Set OPENAI_API_KEY or ELEVENLABS_API_KEY")?;

        match provider {
            "openai" => {
                self.synthesize_openai(text, voice_override, format_override)
                    .await
            }
            "elevenlabs" => self.synthesize_elevenlabs(text, voice_override).await,
            other => Err(format!("Unknown TTS provider: {other}")),
        }
    }

    /// Synthesize via OpenAI TTS API.
    async fn synthesize_openai(
        &self,
        text: &str,
        voice_override: Option<&str>,
        format_override: Option<&str>,
    ) -> Result<TtsResult, String> {
        let api_key = std::env::var("OPENAI_API_KEY").map_err(|_| "OPENAI_API_KEY not set")?;

        // Apply per-request overrides or fall back to config defaults
        let voice = voice_override.unwrap_or(&self.config.openai.voice);
        let format = format_override.unwrap_or(&self.config.openai.format);

        let body = serde_json::json!({
            "model": self.config.openai.model,
            "input": text,
            "voice": voice,
            "response_format": format,
            "speed": self.config.openai.speed,
        });

        // `tts_openai_base_url` (config.media.tts_openai_base_url) overrides
        // the hardcoded provider URL when set, allowing the same OpenAI-compat
        // JSON wire format to be sent to a local TTS service (Lemonade/Kokoro,
        // LM Studio, etc.) instead of the cloud provider. The Authorization
        // header is still built from `OPENAI_API_KEY`; local services typically
        // accept any non-empty bearer token. Closes #1051.
        let url = self
            .openai_base_url
            .as_deref()
            .map(|base| format!("{}/v1/audio/speech", base.trim_end_matches('/')))
            .unwrap_or_else(|| "https://api.openai.com/v1/audio/speech".to_string());

        let client = reqwest::Client::new();
        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .timeout(std::time::Duration::from_secs(self.config.timeout_secs))
            .send()
            .await
            .map_err(|e| format!("OpenAI TTS request failed: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let err = response.text().await.unwrap_or_default();
            let truncated = crate::str_utils::safe_truncate_str(&err, 500);
            return Err(format!("OpenAI TTS failed (HTTP {status}): {truncated}"));
        }

        // Check content length before downloading
        if let Some(len) = response.content_length() {
            if len as usize > MAX_AUDIO_RESPONSE_BYTES {
                return Err(format!(
                    "Audio response too large: {len} bytes (max {MAX_AUDIO_RESPONSE_BYTES})"
                ));
            }
        }

        let audio_data = response
            .bytes()
            .await
            .map_err(|e| format!("Failed to read audio response: {e}"))?;

        if audio_data.len() > MAX_AUDIO_RESPONSE_BYTES {
            return Err(format!(
                "Audio data exceeds {}MB limit",
                MAX_AUDIO_RESPONSE_BYTES / 1024 / 1024
            ));
        }

        // Rough duration estimate: ~150 words/min at ~12 bytes/ms for MP3
        let word_count = text.split_whitespace().count();
        let duration_ms = (word_count as u64 * 400).max(500); // ~400ms per word, min 500ms

        Ok(TtsResult {
            audio_data: audio_data.to_vec(),
            format: format.to_string(),
            provider: "openai".to_string(),
            duration_estimate_ms: duration_ms,
        })
    }

    /// Synthesize via ElevenLabs TTS API.
    async fn synthesize_elevenlabs(
        &self,
        text: &str,
        voice_override: Option<&str>,
    ) -> Result<TtsResult, String> {
        let api_key =
            std::env::var("ELEVENLABS_API_KEY").map_err(|_| "ELEVENLABS_API_KEY not set")?;

        let voice_id = voice_override.unwrap_or(&self.config.elevenlabs.voice_id);
        // `tts_elevenlabs_base_url` (config.media.tts_elevenlabs_base_url)
        // overrides the hardcoded provider URL when set, allowing the same
        // ElevenLabs JSON wire format to be routed through a proxy or
        // self-hosted ElevenLabs-compatible gateway. The `xi-api-key` header
        // still comes from `ELEVENLABS_API_KEY`. Closes #1051.
        let base = self
            .elevenlabs_base_url
            .as_deref()
            .map(|b| b.trim_end_matches('/').to_string())
            .unwrap_or_else(|| "https://api.elevenlabs.io".to_string());
        let url = format!("{}/v1/text-to-speech/{}", base, voice_id);

        let body = serde_json::json!({
            "text": text,
            "model_id": self.config.elevenlabs.model_id,
            "voice_settings": {
                "stability": self.config.elevenlabs.stability,
                "similarity_boost": self.config.elevenlabs.similarity_boost,
            }
        });

        let client = reqwest::Client::new();
        let response = client
            .post(&url)
            .header("xi-api-key", &api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .timeout(std::time::Duration::from_secs(self.config.timeout_secs))
            .send()
            .await
            .map_err(|e| format!("ElevenLabs TTS request failed: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let err = response.text().await.unwrap_or_default();
            let truncated = crate::str_utils::safe_truncate_str(&err, 500);
            return Err(format!(
                "ElevenLabs TTS failed (HTTP {status}): {truncated}"
            ));
        }

        if let Some(len) = response.content_length() {
            if len as usize > MAX_AUDIO_RESPONSE_BYTES {
                return Err(format!(
                    "Audio response too large: {len} bytes (max {MAX_AUDIO_RESPONSE_BYTES})"
                ));
            }
        }

        let audio_data = response
            .bytes()
            .await
            .map_err(|e| format!("Failed to read audio response: {e}"))?;

        if audio_data.len() > MAX_AUDIO_RESPONSE_BYTES {
            return Err(format!(
                "Audio data exceeds {}MB limit",
                MAX_AUDIO_RESPONSE_BYTES / 1024 / 1024
            ));
        }

        let word_count = text.split_whitespace().count();
        let duration_ms = (word_count as u64 * 400).max(500);

        Ok(TtsResult {
            audio_data: audio_data.to_vec(),
            format: "mp3".to_string(),
            provider: "elevenlabs".to_string(),
            duration_estimate_ms: duration_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> TtsConfig {
        TtsConfig::default()
    }

    #[test]
    fn test_engine_creation() {
        let engine = TtsEngine::new(default_config());
        assert!(!engine.config.enabled);
    }

    #[test]
    fn test_config_defaults() {
        let config = TtsConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.max_text_length, 4096);
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.openai.voice, "alloy");
        assert_eq!(config.openai.model, "tts-1");
        assert_eq!(config.openai.format, "mp3");
        assert_eq!(config.openai.speed, 1.0);
        assert_eq!(config.elevenlabs.voice_id, "21m00Tcm4TlvDq8ikWAM");
        assert_eq!(config.elevenlabs.model_id, "eleven_monolingual_v1");
    }

    #[tokio::test]
    async fn test_synthesize_disabled() {
        let engine = TtsEngine::new(default_config());
        let result = engine.synthesize("Hello", None, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("disabled"));
    }

    #[tokio::test]
    async fn test_synthesize_empty_text() {
        let mut config = default_config();
        config.enabled = true;
        let engine = TtsEngine::new(config);
        let result = engine.synthesize("", None, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[tokio::test]
    async fn test_synthesize_text_too_long() {
        let mut config = default_config();
        config.enabled = true;
        config.max_text_length = 10;
        let engine = TtsEngine::new(config);
        let result = engine
            .synthesize("This text is definitely longer than ten chars", None, None)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too long"));
    }

    #[test]
    fn test_detect_provider_none() {
        // In test env, likely no API keys set
        let _ = TtsEngine::detect_provider(); // Just verify no panic
    }

    #[tokio::test]
    async fn test_synthesize_no_provider() {
        let mut config = default_config();
        config.enabled = true;
        let engine = TtsEngine::new(config);
        // This may or may not error depending on env vars
        let result = engine.synthesize("Hello world", None, None).await;
        // If no API keys are set, should error
        if let Err(err) = result {
            assert!(err.contains("No TTS provider") || err.contains("not set"));
        }
    }

    #[test]
    fn test_max_audio_constant() {
        assert_eq!(MAX_AUDIO_RESPONSE_BYTES, 10 * 1024 * 1024);
    }

    #[test]
    fn test_with_base_urls_sets_overrides() {
        let engine = TtsEngine::new(default_config()).with_base_urls(
            Some("http://127.0.0.1:8000".to_string()),
            Some("http://127.0.0.1:9000".to_string()),
        );
        assert_eq!(
            engine.openai_base_url.as_deref(),
            Some("http://127.0.0.1:8000")
        );
        assert_eq!(
            engine.elevenlabs_base_url.as_deref(),
            Some("http://127.0.0.1:9000")
        );
    }

    /// Closes #1051: when the OpenAI TTS base URL is overridden, the URL
    /// building logic must append `/v1/audio/speech` and strip any trailing
    /// slash. When unset, the hardcoded provider URL is used.
    #[test]
    fn test_tts_openai_base_url_override_logic() {
        // Helper mirroring the URL construction in `synthesize_openai`.
        fn build(base: Option<&str>) -> String {
            base.map(|b| format!("{}/v1/audio/speech", b.trim_end_matches('/')))
                .unwrap_or_else(|| "https://api.openai.com/v1/audio/speech".to_string())
        }

        // Default: hardcoded URL preserved (backward compatibility).
        assert_eq!(build(None), "https://api.openai.com/v1/audio/speech");

        // Override applied.
        assert_eq!(
            build(Some("http://127.0.0.1:8000")),
            "http://127.0.0.1:8000/v1/audio/speech"
        );

        // Trailing slash on the user-supplied base is stripped.
        assert_eq!(
            build(Some("http://127.0.0.1:8000/")),
            "http://127.0.0.1:8000/v1/audio/speech"
        );
        assert_eq!(
            build(Some("https://tts.example.com/")),
            "https://tts.example.com/v1/audio/speech"
        );
    }

    /// Closes #1051: when the ElevenLabs TTS base URL is overridden, the URL
    /// building logic must append `/v1/text-to-speech/{voice_id}` and strip
    /// any trailing slash. When unset, the hardcoded provider URL is used.
    #[test]
    fn test_tts_elevenlabs_base_url_override_logic() {
        fn build(base: Option<&str>, voice_id: &str) -> String {
            let b = base
                .map(|b| b.trim_end_matches('/').to_string())
                .unwrap_or_else(|| "https://api.elevenlabs.io".to_string());
            format!("{}/v1/text-to-speech/{}", b, voice_id)
        }

        let voice = "21m00Tcm4TlvDq8ikWAM";

        // Default: hardcoded URL preserved.
        assert_eq!(
            build(None, voice),
            format!("https://api.elevenlabs.io/v1/text-to-speech/{voice}")
        );

        // Override applied.
        assert_eq!(
            build(Some("http://127.0.0.1:9000"), voice),
            format!("http://127.0.0.1:9000/v1/text-to-speech/{voice}")
        );

        // Trailing slash stripped.
        assert_eq!(
            build(Some("http://127.0.0.1:9000/"), voice),
            format!("http://127.0.0.1:9000/v1/text-to-speech/{voice}")
        );
        assert_eq!(
            build(Some("https://eleven.example.com/"), voice),
            format!("https://eleven.example.com/v1/text-to-speech/{voice}")
        );
    }
}
