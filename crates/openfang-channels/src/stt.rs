//! Speech-to-text providers for the voice pipeline.
//!
//! Accepts raw 16-bit PCM at 16 kHz mono and returns a transcription string.
//! Uses batch REST APIs — audio is fully buffered before transcription (Smart
//! Turn handles end-of-utterance detection upstream).
//!
//! # Supported providers
//! - **Deepgram** — nova-3 model by default, extremely fast (~300ms)
//! - **OpenAI Whisper** — whisper-1, higher accuracy, slower (~1s)

use crate::tts::pcm_to_wav;
use openfang_types::config::{VoiceSttConfig, VoiceSttProvider};
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;
use tracing::{debug, warn};

const STT_TIMEOUT_SECS: u64 = 30;

// ── Public interface ─────────────────────────────────────────────────────────

/// Transcribe raw 16-bit PCM (16 kHz mono) to text.
///
/// Returns the transcription string, or an error if the provider call fails.
pub async fn transcribe(
    pcm: &[i16],
    config: &VoiceSttConfig,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let client = Client::builder()
        .timeout(Duration::from_secs(STT_TIMEOUT_SECS))
        .build()?;

    match config.provider {
        VoiceSttProvider::Deepgram => transcribe_deepgram(pcm, config, &client).await,
        VoiceSttProvider::OpenAi => transcribe_openai(pcm, config, &client).await,
    }
}

// ── Deepgram ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct DeepgramResponse {
    results: Option<DeepgramResults>,
}

#[derive(Deserialize)]
struct DeepgramResults {
    channels: Vec<DeepgramChannel>,
}

#[derive(Deserialize)]
struct DeepgramChannel {
    alternatives: Vec<DeepgramAlternative>,
}

#[derive(Deserialize)]
struct DeepgramAlternative {
    transcript: String,
}

async fn transcribe_deepgram(
    pcm: &[i16],
    config: &VoiceSttConfig,
    client: &Client,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let wav = pcm_to_wav(pcm);

    let model = config
        .model
        .as_deref()
        .unwrap_or("nova-3");

    let mut url = format!(
        "https://api.deepgram.com/v1/listen?model={model}&smart_format=true"
    );
    if let Some(ref lang) = config.language {
        url.push_str(&format!("&language={lang}"));
    }

    let resp = client
        .post(&url)
        .header("Authorization", format!("Token {}", config.api_key))
        .header("Content-Type", "audio/wav")
        .body(wav)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        warn!("Deepgram STT error {status}: {body}");
        return Err(format!("Deepgram error {status}").into());
    }

    let data: DeepgramResponse = resp.json().await?;
    let transcript = data
        .results
        .and_then(|r| r.channels.into_iter().next())
        .and_then(|c| c.alternatives.into_iter().next())
        .map(|a| a.transcript)
        .unwrap_or_default();

    debug!("Deepgram transcript: {:?}", transcript);
    Ok(transcript)
}

// ── OpenAI Whisper ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct OpenAiTranscriptionResponse {
    text: String,
}

async fn transcribe_openai(
    pcm: &[i16],
    config: &VoiceSttConfig,
    client: &Client,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let wav = pcm_to_wav(pcm);

    let model = config
        .model
        .as_deref()
        .unwrap_or("whisper-1")
        .to_string();

    // multipart/form-data
    let file_part = reqwest::multipart::Part::bytes(wav)
        .file_name("audio.wav")
        .mime_str("audio/wav")?;

    let mut form = reqwest::multipart::Form::new()
        .part("file", file_part)
        .text("model", model);

    if let Some(ref lang) = config.language {
        form = form.text("language", lang.clone());
    }

    let resp = client
        .post("https://api.openai.com/v1/audio/transcriptions")
        .header("Authorization", format!("Bearer {}", config.api_key))
        .multipart(form)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        warn!("OpenAI Whisper STT error {status}: {body}");
        return Err(format!("OpenAI STT error {status}").into());
    }

    let data: OpenAiTranscriptionResponse = resp.json().await?;
    debug!("OpenAI Whisper transcript: {:?}", data.text);
    Ok(data.text)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_types::config::VoiceSttProvider;

    #[test]
    fn test_stt_config_deepgram() {
        let cfg = VoiceSttConfig {
            provider: VoiceSttProvider::Deepgram,
            api_key: "test-key".to_string(),
            language: Some("en".to_string()),
            model: None,
        };
        assert_eq!(cfg.provider, VoiceSttProvider::Deepgram);
        assert_eq!(cfg.language.as_deref(), Some("en"));
    }

    #[test]
    fn test_stt_config_openai() {
        let cfg = VoiceSttConfig {
            provider: VoiceSttProvider::OpenAi,
            api_key: "sk-test".to_string(),
            language: None,
            model: Some("whisper-1".to_string()),
        };
        assert_eq!(cfg.provider, VoiceSttProvider::OpenAi);
        assert_eq!(cfg.model.as_deref(), Some("whisper-1"));
    }
}
