//! Text-to-speech providers for the voice pipeline.
//!
//! Accepts a text string and returns raw 16-bit PCM at 16 kHz mono.
//! All providers stream or return audio that is converted to a common
//! PCM format before being sent back to the WebSocket client.
//!
//! # Supported providers
//! - **Cartesia** — sonic-2 model, very low latency streaming
//! - **ElevenLabs** — high quality, streaming MP3 decoded to PCM
//! - **OpenAI TTS** — tts-1 / tts-1-hd, MP3 decoded to PCM

use openfang_types::config::{VoiceTtsConfig, VoiceTtsProvider};
use reqwest::Client;
use serde::Serialize;
use std::time::Duration;
use tracing::{debug, warn};

const TTS_TIMEOUT_SECS: u64 = 30;

// ── Public interface ─────────────────────────────────────────────────────────

/// Synthesize `text` to raw 16-bit PCM at 16 kHz mono.
pub async fn synthesize(
    text: &str,
    config: &VoiceTtsConfig,
) -> Result<Vec<i16>, Box<dyn std::error::Error + Send + Sync>> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(TTS_TIMEOUT_SECS))
        .build()?;

    match config.provider {
        VoiceTtsProvider::Cartesia => synthesize_cartesia(text, config, &client).await,
        VoiceTtsProvider::ElevenLabs => synthesize_elevenlabs(text, config, &client).await,
        VoiceTtsProvider::OpenAi => synthesize_openai(text, config, &client).await,
    }
}

// ── Cartesia ─────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct CartesiaRequest<'a> {
    model_id: &'a str,
    transcript: &'a str,
    voice: CartesiaVoice<'a>,
    output_format: CartesiaOutputFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    speed: Option<f32>,
}

#[derive(Serialize)]
struct CartesiaVoice<'a> {
    mode: &'a str,
    id: &'a str,
}

#[derive(Serialize)]
struct CartesiaOutputFormat {
    container: &'static str,
    encoding: &'static str,
    sample_rate: u32,
}

async fn synthesize_cartesia(
    text: &str,
    config: &VoiceTtsConfig,
    client: &Client,
) -> Result<Vec<i16>, Box<dyn std::error::Error + Send + Sync>> {
    let model = config.model.as_deref().unwrap_or("sonic-2");

    let body = CartesiaRequest {
        model_id: model,
        transcript: text,
        voice: CartesiaVoice {
            mode: "id",
            id: &config.voice_id,
        },
        output_format: CartesiaOutputFormat {
            container: "raw",
            encoding: "pcm_s16le",
            sample_rate: 16_000,
        },
        speed: config.speed,
    };

    let resp = client
        .post("https://api.cartesia.ai/tts/bytes")
        .header(
            "X-API-Key",
            std::env::var(&config.api_key_env).unwrap_or_default(),
        )
        .header("Cartesia-Version", "2024-06-10")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        warn!("Cartesia TTS error {status}: {body_text}");
        return Err(format!("Cartesia TTS error {status}").into());
    }

    let raw_bytes = resp.bytes().await?;
    debug!("Cartesia TTS: {} bytes PCM", raw_bytes.len());

    // Raw bytes are already pcm_s16le — convert to Vec<i16>
    Ok(bytes_to_i16_le(&raw_bytes))
}

// ── ElevenLabs ────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ElevenLabsRequest<'a> {
    text: &'a str,
    model_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    voice_settings: Option<ElevenLabsVoiceSettings>,
}

#[derive(Serialize)]
struct ElevenLabsVoiceSettings {
    stability: f32,
    similarity_boost: f32,
    speed: f32,
}

async fn synthesize_elevenlabs(
    text: &str,
    config: &VoiceTtsConfig,
    client: &Client,
) -> Result<Vec<i16>, Box<dyn std::error::Error + Send + Sync>> {
    let model = config.model.as_deref().unwrap_or("eleven_turbo_v2_5");

    let voice_settings = config.speed.map(|spd| ElevenLabsVoiceSettings {
        stability: 0.5,
        similarity_boost: 0.75,
        speed: spd,
    });

    let body = ElevenLabsRequest {
        text,
        model_id: model,
        voice_settings,
    };

    // Request PCM output directly
    let url = format!(
        "https://api.elevenlabs.io/v1/text-to-speech/{}/stream?output_format=pcm_16000",
        config.voice_id
    );

    let resp = client
        .post(&url)
        .header(
            "xi-api-key",
            std::env::var(&config.api_key_env).unwrap_or_default(),
        )
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        warn!("ElevenLabs TTS error {status}: {body_text}");
        return Err(format!("ElevenLabs TTS error {status}").into());
    }

    let raw_bytes = resp.bytes().await?;
    debug!("ElevenLabs TTS: {} bytes PCM", raw_bytes.len());

    Ok(bytes_to_i16_le(&raw_bytes))
}

// ── OpenAI TTS ────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct OpenAiTtsRequest<'a> {
    model: &'a str,
    input: &'a str,
    voice: &'a str,
    response_format: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    speed: Option<f32>,
}

async fn synthesize_openai(
    text: &str,
    config: &VoiceTtsConfig,
    client: &Client,
) -> Result<Vec<i16>, Box<dyn std::error::Error + Send + Sync>> {
    let model = config.model.as_deref().unwrap_or("tts-1");

    let body = OpenAiTtsRequest {
        model,
        input: text,
        voice: &config.voice_id,
        response_format: "pcm",
        speed: config.speed,
    };

    let resp = client
        .post("https://api.openai.com/v1/audio/speech")
        .header(
            "Authorization",
            format!(
                "Bearer {}",
                std::env::var(&config.api_key_env).unwrap_or_default()
            ),
        )
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        warn!("OpenAI TTS error {status}: {body_text}");
        return Err(format!("OpenAI TTS error {status}").into());
    }

    // OpenAI returns 24kHz PCM — resample to 16kHz
    let raw_bytes = resp.bytes().await?;
    debug!("OpenAI TTS: {} bytes PCM (24kHz)", raw_bytes.len());
    let pcm_24k = bytes_to_i16_le(&raw_bytes);
    Ok(resample_24k_to_16k(&pcm_24k))
}

// ── PCM helpers ───────────────────────────────────────────────────────────────

/// Convert a byte slice of little-endian int16 samples to Vec<i16>.
pub(crate) fn bytes_to_i16_le(bytes: &[u8]) -> Vec<i16> {
    bytes
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect()
}

/// Encode i16 PCM samples as a minimal WAV file (16kHz mono) for STT upload.
pub fn pcm_to_wav(pcm: &[i16]) -> Vec<u8> {
    let num_samples = pcm.len() as u32;
    let byte_rate: u32 = 16_000 * 2;
    let data_size = num_samples * 2;
    let file_size = 36 + data_size;

    let mut wav = Vec::with_capacity(44 + data_size as usize);
    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    // fmt chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&16_000u32.to_le_bytes()); // sample rate
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes()); // block align
    wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
                                                 // data chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    for sample in pcm {
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    wav
}

/// Convert i16 PCM samples to raw little-endian bytes.
pub fn i16_to_bytes(pcm: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pcm.len() * 2);
    for s in pcm {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Simple linear interpolation resample from 24kHz to 16kHz (ratio 2:3).
fn resample_24k_to_16k(src: &[i16]) -> Vec<i16> {
    if src.is_empty() {
        return Vec::new();
    }
    let src_rate = 24_000f64;
    let dst_rate = 16_000f64;
    let ratio = src_rate / dst_rate;
    let out_len = (src.len() as f64 / ratio).ceil() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f64 * ratio;
        let idx = pos as usize;
        let frac = pos - idx as f64;
        let s0 = src.get(idx).copied().unwrap_or(0) as f64;
        let s1 = src.get(idx + 1).copied().unwrap_or(0) as f64;
        out.push((s0 + frac * (s1 - s0)).round() as i16);
    }
    out
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_types::config::VoiceTtsProvider;

    #[test]
    fn test_pcm_to_wav_header() {
        let pcm = vec![0i16; 16_000]; // 1 second silence
        let wav = pcm_to_wav(&pcm);
        assert!(wav.len() > 44);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
    }

    #[test]
    fn test_pcm_to_wav_roundtrip() {
        let pcm: Vec<i16> = (0..100).map(|i| i as i16).collect();
        let wav = pcm_to_wav(&pcm);
        let data = &wav[44..];
        let recovered = bytes_to_i16_le(data);
        assert_eq!(recovered, pcm);
    }

    #[test]
    fn test_i16_to_bytes_roundtrip() {
        let pcm: Vec<i16> = vec![0, 1000, -1000, i16::MAX, i16::MIN];
        let bytes = i16_to_bytes(&pcm);
        let recovered = bytes_to_i16_le(&bytes);
        assert_eq!(recovered, pcm);
    }

    #[test]
    fn test_resample_24k_to_16k_length() {
        let src = vec![0i16; 24_000]; // 1 second at 24kHz
        let dst = resample_24k_to_16k(&src);
        // Should be ~16000 samples
        assert!((dst.len() as i64 - 16_000).abs() <= 2);
    }

    #[test]
    fn test_resample_empty() {
        let result = resample_24k_to_16k(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_tts_config_cartesia() {
        let cfg = VoiceTtsConfig {
            provider: VoiceTtsProvider::Cartesia,
            api_key_env: "TTS_API_KEY".to_string(),
            voice_id: "voice-123".to_string(),
            model: Some("sonic-2".to_string()),
            speed: None,
        };
        assert_eq!(cfg.provider, VoiceTtsProvider::Cartesia);
        assert_eq!(cfg.voice_id, "voice-123");
    }

    #[test]
    fn test_tts_config_elevenlabs() {
        let cfg = VoiceTtsConfig {
            provider: VoiceTtsProvider::ElevenLabs,
            api_key_env: "TTS_API_KEY".to_string(),
            voice_id: "el-voice".to_string(),
            model: None,
            speed: Some(1.2),
        };
        assert_eq!(cfg.provider, VoiceTtsProvider::ElevenLabs);
        assert_eq!(cfg.speed, Some(1.2));
    }
}
