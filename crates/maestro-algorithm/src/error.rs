//! Error types for the algorithm executor.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AlgorithmError {
    #[error("Model completion failed: {0}")]
    ModelError(String),

    #[error("JSON extraction failed: {0}")]
    ExtractionError(String),

    #[error("Phase {phase} failed after {retries} retries: {reason}")]
    PhaseFailure {
        phase: String,
        retries: u32,
        reason: String,
    },

    #[error("ISC verification failed: satisfaction {satisfaction:.1}% below threshold {threshold:.1}%")]
    VerificationFailure {
        satisfaction: f64,
        threshold: f64,
    },

    #[error("Agent delegation failed: {0}")]
    DelegationError(String),

    #[error("Timeout after {duration_ms}ms in phase {phase}")]
    Timeout {
        phase: String,
        duration_ms: u64,
    },

    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}
