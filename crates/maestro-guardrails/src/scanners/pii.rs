//! PII (Personally Identifiable Information) scanner.
//!
//! Detects and optionally redacts: SSN, credit cards, emails, phone numbers,
//! IP addresses, and other PII patterns.
//!
//! HONEST NOTE: This is regex-only. For production, integrate a NER model
//! (e.g., via Rig.rs) for context-aware PII detection. Regex will miss
//! PII in natural language ("my social is nine eight seven...").

use async_trait::async_trait;
use crate::{GuardrailAction, ScanDirection, ScanResult, Scanner};

pub struct PiiScanner {
    // TODO: Add compiled regex patterns for each PII type
    // TODO: Add configuration for which PII types to detect
}

impl PiiScanner {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for PiiScanner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Scanner for PiiScanner {
    fn name(&self) -> &str { "pii_scanner" }

    fn directions(&self) -> Vec<ScanDirection> {
        vec![ScanDirection::Input, ScanDirection::Output]
    }

    async fn scan(&self, content: &str, _direction: ScanDirection) -> ScanResult {
        // TODO: Implement PII pattern matching
        ScanResult {
            scanner_name: self.name().to_string(),
            triggered: false,
            confidence: 0.0,
            findings: vec![],
            action: GuardrailAction::Allow,
            processed_content: content.to_string(),
        }
    }
}
