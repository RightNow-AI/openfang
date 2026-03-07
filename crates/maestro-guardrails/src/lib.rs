//! # maestro-guardrails
//!
//! Input/output guardrails system inspired by Kore.ai's 6-scanner architecture.
//!
//! ## Architecture (from Kore.ai docs.kore.ai)
//!
//! Kore.ai implements guardrails at THREE levels:
//! 1. **Platform-level** — Global defaults for all agents
//! 2. **Agent-level** — Per-agent overrides
//! 3. **Node-level** — Per-conversation-node overrides
//!
//! Each level has **input scanners** (run before the LLM sees the message)
//! and **output scanners** (run before the response reaches the user).
//!
//! ## Scanner Types (from Kore.ai)
//!
//! 1. **Data Anonymizer** — PII detection and masking
//! 2. **Prompt Injection** — Jailbreak and injection defense
//! 3. **Toxicity** — Hate speech, harassment, profanity
//! 4. **Topic Control** — Allowed/blocked topic enforcement
//! 5. **Hallucination** — Factual grounding checks (output only)
//! 6. **Custom Regex** — User-defined pattern matching
//!
//! ## Integration with OpenFang
//!
//! OpenFang has basic taint tracking (`openfang-types::taint`) but NO
//! content scanning. Maestro's gateway had 34 prompt injection patterns
//! but no PII, toxicity, or topic control.
//!
//! This crate provides a pluggable scanner pipeline that hooks into
//! OpenFang's `hooks.rs` system (pre/post message processing).
//!
//! ## HONEST GAPS
//!
//! - PII detection is regex-based only (no NER model integration yet)
//! - Toxicity detection requires an external API (no local model)
//! - Hallucination detection is not implemented (requires RAG grounding)
//! - No UI for configuring scanners (TOML config only)
//! - Performance impact of running 6 scanners on every message is unknown

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The action to take when a scanner triggers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardrailAction {
    /// Allow the message through (log only).
    Allow,
    /// Redact the matched content and allow.
    Redact,
    /// Block the message entirely.
    Block,
    /// Replace the message with a canned response.
    Replace { response: String },
    /// Escalate to a human reviewer.
    Escalate,
}

/// Result of a single scanner's evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    /// Which scanner produced this result.
    pub scanner_name: String,
    /// Whether the scanner triggered.
    pub triggered: bool,
    /// Confidence score (0.0 - 1.0).
    pub confidence: f64,
    /// What was detected (e.g., "SSN pattern", "injection attempt").
    pub findings: Vec<String>,
    /// The recommended action.
    pub action: GuardrailAction,
    /// The (possibly modified) content after scanning.
    pub processed_content: String,
}

/// Direction of scanning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScanDirection {
    Input,
    Output,
}

/// Trait for implementing a guardrail scanner.
///
/// Each scanner receives the message content and returns a ScanResult.
/// Scanners are composable — they run in sequence, and each scanner
/// receives the output of the previous scanner.
#[async_trait]
pub trait Scanner: Send + Sync {
    /// The scanner's unique name.
    fn name(&self) -> &str;

    /// Which direction(s) this scanner applies to.
    fn directions(&self) -> Vec<ScanDirection>;

    /// Scan the content and return a result.
    async fn scan(&self, content: &str, direction: ScanDirection) -> ScanResult;
}

/// Configuration for the guardrails pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardrailsConfig {
    /// Whether guardrails are enabled globally.
    pub enabled: bool,
    /// Scanner configurations keyed by scanner name.
    pub scanners: HashMap<String, ScannerConfig>,
    /// Default action when a scanner triggers.
    pub default_action: GuardrailAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerConfig {
    pub enabled: bool,
    pub directions: Vec<ScanDirection>,
    pub action: GuardrailAction,
    pub threshold: f64,
    /// Scanner-specific parameters.
    pub params: HashMap<String, serde_json::Value>,
}

/// The guardrails pipeline — runs all configured scanners in sequence.
pub struct GuardrailsPipeline {
    scanners: Vec<Box<dyn Scanner>>,
    config: GuardrailsConfig,
}

impl GuardrailsPipeline {
    pub fn new(config: GuardrailsConfig) -> Self {
        Self {
            scanners: Vec::new(),
            config,
        }
    }

    /// Register a scanner with the pipeline.
    pub fn add_scanner(&mut self, scanner: Box<dyn Scanner>) {
        self.scanners.push(scanner);
    }

    /// Run all scanners on the given content.
    ///
    /// Returns the final processed content and all scan results.
    /// If any scanner returns Block, the pipeline short-circuits.
    pub async fn scan(
        &self,
        content: &str,
        direction: ScanDirection,
    ) -> (String, Vec<ScanResult>) {
        let mut current_content = content.to_string();
        let mut results = Vec::new();

        for scanner in &self.scanners {
            if !scanner.directions().contains(&direction) {
                continue;
            }

            let scanner_name = scanner.name().to_string();
            let scanner_config = self.config.scanners.get(&scanner_name);

            // Skip disabled scanners
            if let Some(cfg) = scanner_config {
                if !cfg.enabled {
                    continue;
                }
            }

            let result = scanner.scan(&current_content, direction).await;

            if result.triggered {
                match &result.action {
                    GuardrailAction::Block => {
                        results.push(result);
                        return ("".to_string(), results); // Short-circuit
                    }
                    GuardrailAction::Redact | GuardrailAction::Replace { .. } => {
                        current_content = result.processed_content.clone();
                    }
                    _ => {}
                }
            }

            results.push(result);
        }

        (current_content, results)
    }
}

// ── Built-in Scanner Implementations ────────────────────────────────────

pub mod scanners {
    //! Built-in scanner implementations.
    //!
    //! HONEST NOTE: These are starter implementations. Production use
    //! requires more comprehensive pattern libraries and potentially
    //! ML-based detection for toxicity and hallucination.

    pub mod pii;
    pub mod prompt_injection;
    pub mod topic_control;
    pub mod custom_regex;
    // TODO: pub mod toxicity; (requires external API)
    // TODO: pub mod hallucination; (requires RAG grounding)
}
