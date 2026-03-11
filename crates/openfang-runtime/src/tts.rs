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
}

impl TtsEngine {
    pub fn new(config: TtsConfig) -> Self {
        Self { config }
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

        let client = reqwest::Client::new();
        let response = client
            .post("https://api.openai.com/v1/audio/speech")
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
            let truncated = if err.len() > 500 { let mut end = 500; while end > 0 && !err.is_char_boundary(end) { end -= 1; } &err[..end] } else { &err };
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
        let url = format!("https://api.elevenlabs.io/v1/text-to-speech/{}", voice_id);

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
            let truncated = if err.len() > 500 { let mut end = 500; while end > 0 && !err.is_char_boundary(end) { end -= 1; } &err[..end] } else { &err };
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
}
