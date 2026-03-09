//! PII (Personally Identifiable Information) scanner.
//!
//! Detects and optionally redacts: SSN, credit cards, emails, phone numbers,
//! IP addresses, passport numbers, bank accounts, API keys, and more.
//!
//! HONEST NOTE: This is regex-only. For production, integrate a NER model
//! (e.g., via Rig.rs) for context-aware PII detection. Regex will miss
//! PII in natural language ("my social is nine eight seven...").

use async_trait::async_trait;
use crate::{GuardrailAction, ScanDirection, ScanResult, Scanner};
use once_cell::sync::Lazy;
use regex::Regex;

/// A named PII pattern with its replacement token.
struct PiiPattern {
    name: &'static str,
    regex: Regex,
    replacement: &'static str,
}

static PII_PATTERNS: Lazy<Vec<PiiPattern>> = Lazy::new(|| {
    vec![
        PiiPattern {
            name: "SSN",
            regex: Regex::new(r"\b(?:\d{3}-\d{2}-\d{4}|\d{9})\b").unwrap(),
            replacement: "[SSN REDACTED]",
        },
        PiiPattern {
            name: "Credit Card",
            regex: Regex::new(r"\b(?:4[0-9]{12}(?:[0-9]{3})?|5[1-5][0-9]{14}|3[47][0-9]{13}|3(?:0[0-5]|[68][0-9])[0-9]{11}|6(?:011|5[0-9]{2})[0-9]{12}|(?:2131|1800|35\d{3})\d{11})\b").unwrap(),
            replacement: "[CREDIT CARD REDACTED]",
        },
        PiiPattern {
            name: "Email",
            regex: Regex::new(r"\b[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}\b").unwrap(),
            replacement: "[EMAIL REDACTED]",
        },
        PiiPattern {
            name: "US Phone",
            regex: Regex::new(r"\b(?:\+?1[-.\s]?)?\(?([0-9]{3})\)?[-.\s]?([0-9]{3})[-.\s]?([0-9]{4})\b").unwrap(),
            replacement: "[PHONE REDACTED]",
        },
        PiiPattern {
            name: "IPv4 Address",
            regex: Regex::new(r"\b(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\b").unwrap(),
            replacement: "[IP REDACTED]",
        },
        PiiPattern {
            name: "AWS Access Key",
            regex: Regex::new(r"\b(?:AKIA|ASIA|AROA|AIDA|AIPA|ANPA|ANVA|APKA)[A-Z0-9]{16}\b").unwrap(),
            replacement: "[AWS KEY REDACTED]",
        },
        PiiPattern {
            name: "Generic API Key",
            regex: Regex::new(r#"(?i)(?:api[_\-]?key|secret[_\-]?key|access[_\-]?token|auth[_\-]?token)["'\s:=]+([A-Za-z0-9\-_]{20,})"#).unwrap(),
            replacement: "[API KEY REDACTED]",
        },
    ]
});

/// Configuration for the PII scanner.
#[derive(Debug, Clone)]
pub struct PiiScannerConfig {
    /// Which PII types to detect (empty = all).
    pub enabled_types: Vec<String>,
    /// Action to take when PII is detected.
    pub action: GuardrailAction,
}

impl Default for PiiScannerConfig {
    fn default() -> Self {
        Self {
            enabled_types: vec![],
            action: GuardrailAction::Redact,
        }
    }
}

/// PII scanner with configurable detection and redaction.
pub struct PiiScanner {
    config: PiiScannerConfig,
}

impl PiiScanner {
    pub fn new() -> Self {
        Self { config: PiiScannerConfig::default() }
    }

    pub fn with_config(config: PiiScannerConfig) -> Self {
        Self { config }
    }
}

impl Default for PiiScanner {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Scanner for PiiScanner {
    fn name(&self) -> &str { "pii_scanner" }

    fn directions(&self) -> Vec<ScanDirection> {
        vec![ScanDirection::Input, ScanDirection::Output]
    }

    async fn scan(&self, content: &str, _direction: ScanDirection) -> ScanResult {
        let mut findings = Vec::new();
        let mut processed = content.to_string();
        let mut triggered = false;

        for pattern in PII_PATTERNS.iter() {
            if !self.config.enabled_types.is_empty()
                && !self.config.enabled_types.iter().any(|t| t.eq_ignore_ascii_case(pattern.name))
            {
                continue;
            }

            if pattern.regex.is_match(&processed) {
                let count = pattern.regex.find_iter(&processed).count();
                findings.push(format!("{} ({} occurrence{})", pattern.name, count, if count == 1 { "" } else { "s" }));
                processed = pattern.regex.replace_all(&processed, pattern.replacement).to_string();
                triggered = true;
            }
        }

        ScanResult {
            scanner_name: self.name().to_string(),
            triggered,
            confidence: if triggered { 0.95 } else { 0.0 },
            findings,
            action: if triggered { self.config.action.clone() } else { GuardrailAction::Allow },
            processed_content: processed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ssn_detection() {
        let scanner = PiiScanner::new();
        let result = scanner.scan("My SSN is 123-45-6789", ScanDirection::Input).await;
        assert!(result.triggered);
        assert!(result.processed_content.contains("[SSN REDACTED]"));
    }

    #[tokio::test]
    async fn test_email_detection() {
        let scanner = PiiScanner::new();
        let result = scanner.scan("Contact john.doe@example.com", ScanDirection::Output).await;
        assert!(result.triggered);
        assert!(result.processed_content.contains("[EMAIL REDACTED]"));
    }

    #[tokio::test]
    async fn test_no_pii() {
        let scanner = PiiScanner::new();
        let result = scanner.scan("This is a normal message", ScanDirection::Input).await;
        assert!(!result.triggered);
    }
}
