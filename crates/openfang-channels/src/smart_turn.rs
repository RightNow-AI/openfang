//! Smart Turn end-of-utterance detection.
//!
//! Runs the Smart Turn v3.2 ONNX model (8M params, ~12ms CPU inference) to
//! predict whether the user has finished their turn based on audio prosody
//! (intonation, rhythm, energy) rather than a simple silence timer.
//!
//! The model expects Whisper-style mel spectrogram features computed from the
//! last 8 seconds of audio at 16 kHz mono.  A probability > threshold means
//! the user is done speaking.

use ort::{session::builder::GraphOptimizationLevel, session::Session, value::Tensor};
use rustfft::{num_complex::Complex, FftPlanner};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

// ── Constants (match Whisper feature extractor) ──────────────────────────────

const SAMPLE_RATE: usize = 16_000;
const MAX_SECONDS: usize = 8;
const MAX_SAMPLES: usize = MAX_SECONDS * SAMPLE_RATE;

// Whisper STFT parameters
const N_FFT: usize = 400;
const HOP_LENGTH: usize = 160;
const N_MELS: usize = 80;

// ── SmartTurnDetector ────────────────────────────────────────────────────────

/// Loaded Smart Turn model ready for inference.
pub struct SmartTurnDetector {
    session: Arc<Mutex<Session>>,
    threshold: f32,
}

impl SmartTurnDetector {
    /// Load the ONNX model from `model_path`.
    pub fn load(model_path: &str, threshold: f32) -> Result<Self, Box<dyn std::error::Error>> {
        let path = Path::new(model_path);
        if !path.exists() {
            return Err(format!("Smart Turn model not found: {model_path}").into());
        }

        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::All)?
            .with_intra_threads(1)?
            .with_inter_threads(1)?
            .commit_from_file(model_path)?;

        let size_mb = std::fs::metadata(model_path)
            .map(|m| m.len() as f32 / 1024.0 / 1024.0)
            .unwrap_or(0.0);
        info!(
            "Smart Turn model loaded from {} ({:.1} MB)",
            model_path, size_mb
        );

        Ok(Self {
            session: Arc::new(Mutex::new(session)),
            threshold,
        })
    }

    /// Predict end-of-turn from raw 16-bit PCM at 16 kHz mono.
    ///
    /// Returns `(complete, probability)`.  `complete` is true when the model
    /// thinks the user has finished speaking.
    pub fn predict(&self, pcm: &[i16]) -> (bool, f32) {
        // Convert i16 → f32 in [-1, 1]
        let mut audio: Vec<f32> = pcm.iter().map(|&s| s as f32 / 32768.0).collect();

        // Pad or truncate to MAX_SAMPLES (last 8 seconds)
        if audio.len() > MAX_SAMPLES {
            audio = audio[audio.len() - MAX_SAMPLES..].to_vec();
        } else {
            let pad = MAX_SAMPLES - audio.len();
            let mut padded = vec![0.0f32; pad];
            padded.extend_from_slice(&audio);
            audio = padded;
        }

        // Compute log-mel spectrogram
        let mel = log_mel_spectrogram(&audio);

        // mel shape: [N_MELS, time_frames]
        // Model expects [1, N_MELS, time_frames]
        let time_frames = mel.len() / N_MELS;
        // Build [1, N_MELS, time_frames] tensor
        let tensor = match Tensor::from_array(([1usize, N_MELS, time_frames], mel)) {
            Ok(t) => t,
            Err(e) => {
                warn!("Smart Turn: failed to build input tensor: {e}");
                return (true, 1.0);
            }
        };

        let mut session = match self.session.lock() {
            Ok(s) => s,
            Err(e) => {
                warn!("Smart Turn session lock poisoned: {e}");
                return (true, 1.0);
            }
        };
        let outputs = match session.run(ort::inputs!["input_features" => tensor]) {
            Ok(o) => o,
            Err(e) => {
                warn!("Smart Turn inference error: {e}");
                return (true, 1.0); // fail-open: treat as complete
            }
        };

        let probability = outputs[0]
            .try_extract_scalar::<f32>()
            .unwrap_or(1.0);

        (probability > self.threshold, probability)
    }
}

// ── Mel Spectrogram ──────────────────────────────────────────────────────────

/// Compute a Whisper-compatible log-mel spectrogram.
///
/// Returns a flat `[N_MELS * time_frames]` vector in row-major order
/// (i.e. `mel[mel_bin * time_frames + t]`).
fn log_mel_spectrogram(audio: &[f32]) -> Vec<f32> {
    let window = hann_window(N_FFT);
    let mel_filters = mel_filterbank(SAMPLE_RATE, N_FFT, N_MELS);
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(N_FFT);

    let n_frames = (audio.len().saturating_sub(N_FFT)) / HOP_LENGTH + 1;
    // power spectrogram: [n_frames, N_FFT/2+1]
    let n_freqs = N_FFT / 2 + 1;
    let mut power = vec![0.0f32; n_frames * n_freqs];

    for frame_idx in 0..n_frames {
        let start = frame_idx * HOP_LENGTH;
        let end = (start + N_FFT).min(audio.len());

        let mut buf: Vec<Complex<f32>> = (0..N_FFT)
            .map(|i| {
                let s = if start + i < end {
                    audio[start + i] * window[i]
                } else {
                    0.0
                };
                Complex::new(s, 0.0)
            })
            .collect();

        fft.process(&mut buf);

        for k in 0..n_freqs {
            let re = buf[k].re;
            let im = buf[k].im;
            power[frame_idx * n_freqs + k] = re * re + im * im;
        }
    }

    // Apply mel filterbank: [N_MELS, n_frames]
    let mut mel_spec = vec![0.0f32; N_MELS * n_frames];
    for m in 0..N_MELS {
        for t in 0..n_frames {
            let mut val = 0.0f32;
            for k in 0..n_freqs {
                val += mel_filters[m * n_freqs + k] * power[t * n_freqs + k];
            }
            // log(max(val, 1e-10))
            mel_spec[m * n_frames + t] = val.max(1e-10).ln();
        }
    }

    // Normalize: clip to max-8, scale to [-1,1]
    let max_val = mel_spec.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let floor = max_val - 8.0;
    for v in mel_spec.iter_mut() {
        *v = ((*v).max(floor) - (floor + max_val) / 2.0) / 4.0;
    }

    mel_spec
}

/// Hann window of length `n`.
fn hann_window(n: usize) -> Vec<f32> {
    use std::f32::consts::PI;
    (0..n)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / n as f32).cos()))
        .collect()
}

/// Triangular mel filterbank.
///
/// Returns a flat `[n_mels * n_freqs]` matrix where `n_freqs = n_fft/2 + 1`.
fn mel_filterbank(sample_rate: usize, n_fft: usize, n_mels: usize) -> Vec<f32> {
    let n_freqs = n_fft / 2 + 1;
    let fmin = 0.0f32;
    let fmax = sample_rate as f32 / 2.0;

    let mel_min = hz_to_mel(fmin);
    let mel_max = hz_to_mel(fmax);

    // n_mels + 2 equally spaced mel points
    let mel_points: Vec<f32> = (0..=n_mels + 1)
        .map(|i| mel_min + (mel_max - mel_min) * i as f32 / (n_mels + 1) as f32)
        .collect();
    let hz_points: Vec<f32> = mel_points.iter().map(|&m| mel_to_hz(m)).collect();

    // Convert hz_points to FFT bin indices
    let bins: Vec<f32> = hz_points
        .iter()
        .map(|&hz| (n_fft as f32 + 1.0) * hz / sample_rate as f32)
        .collect();

    let mut filters = vec![0.0f32; n_mels * n_freqs];
    for m in 0..n_mels {
        let start = bins[m];
        let center = bins[m + 1];
        let end = bins[m + 2];
        for k in 0..n_freqs {
            let k_f = k as f32;
            if k_f >= start && k_f < center {
                filters[m * n_freqs + k] = (k_f - start) / (center - start);
            } else if k_f >= center && k_f <= end {
                filters[m * n_freqs + k] = (end - k_f) / (end - center);
            }
        }
    }

    filters
}

fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10.0f32.powf(mel / 2595.0) - 1.0)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hann_window_endpoints() {
        let w = hann_window(400);
        assert_eq!(w.len(), 400);
        assert!(w[0].abs() < 1e-6, "Hann window should start near 0");
    }

    #[test]
    fn test_mel_filterbank_shape() {
        let filters = mel_filterbank(16_000, 400, 80);
        assert_eq!(filters.len(), 80 * 201);
    }

    #[test]
    fn test_mel_filterbank_non_negative() {
        let filters = mel_filterbank(16_000, 400, 80);
        for &v in &filters {
            assert!(v >= 0.0, "Filter weights must be non-negative");
        }
    }

    #[test]
    fn test_log_mel_spectrogram_shape() {
        let audio = vec![0.0f32; MAX_SAMPLES];
        let mel = log_mel_spectrogram(&audio);
        let n_freqs = N_FFT / 2 + 1;
        let n_frames = (MAX_SAMPLES.saturating_sub(N_FFT)) / HOP_LENGTH + 1;
        assert_eq!(mel.len(), N_MELS * n_frames);
        let _ = n_freqs; // used above in calculation
    }

    #[test]
    fn test_log_mel_spectrogram_silence() {
        // Silence should produce a valid (all-identical) mel spectrogram without panics
        let audio = vec![0.0f32; MAX_SAMPLES];
        let mel = log_mel_spectrogram(&audio);
        assert!(!mel.is_empty());
        assert!(mel.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn test_hz_mel_roundtrip() {
        for hz in [0.0f32, 100.0, 500.0, 1000.0, 4000.0, 8000.0] {
            let roundtrip = mel_to_hz(hz_to_mel(hz));
            assert!((roundtrip - hz).abs() < 0.01, "hz={hz} roundtrip={roundtrip}");
        }
    }
}
