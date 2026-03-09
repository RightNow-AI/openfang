//! Custom regex scanner — user-defined pattern matching.
//!
//! Allows operators to define their own regex patterns for detecting
//! domain-specific sensitive content, compliance violations, or
//! business-specific rules.

use async_trait::async_trait;
use crate::{GuardrailAction, ScanDirection, ScanResult, Scanner};
use regex::Regex;

/// A single custom regex rule.
#[derive(Debug)]
pub struct CustomRule {
    pub name: String,
    pub regex: Regex,
    pub directions: Vec<ScanDirection>,
    pub action: GuardrailAction,
    pub confidence: f64,
    pub replacement: Option<String>,
}

impl CustomRule {
    pub fn new(
        name: impl Into<String>,
        pattern: &str,
        directions: Vec<ScanDirection>,
        action: GuardrailAction,
        confidence: f64,
    ) -> Result<Self, regex::Error> {
        Ok(Self {
            name: name.into(),
            regex: Regex::new(pattern)?,
            directions, action, confidence, replacement: None,
        })
    }

    pub fn redact(name: impl Into<String>, pattern: &str, replacement: impl Into<String>) -> Result<Self, regex::Error> {
        Ok(Self {
            name: name.into(),
            regex: Regex::new(pattern)?,
            directions: vec![ScanDirection::Input, ScanDirection::Output],
            action: GuardrailAction::Redact,
            confidence: 0.90,
            replacement: Some(replacement.into()),
        })
    }

    pub fn block(name: impl Into<String>, pattern: &str) -> Result<Self, regex::Error> {
        Ok(Self {
            name: name.into(),
            regex: Regex::new(pattern)?,
            directions: vec![ScanDirection::Input, ScanDirection::Output],
            action: GuardrailAction::Block,
            confidence: 0.90,
            replacement: None,
        })
    }
}

/// Custom regex scanner with user-defined rules.
pub struct CustomRegexScanner {
    rules: Vec<CustomRule>,
}

impl CustomRegexScanner {
    pub fn new() -> Self { Self { rules: Vec::new() } }

    pub fn with_rules(rules: Vec<CustomRule>) -> Self { Self { rules } }

    pub fn add_rule(&mut self, rule: CustomRule) { self.rules.push(rule); }
}

impl Default for CustomRegexScanner {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Scanner for CustomRegexScanner {
    fn name(&self) -> &str { "custom_regex_scanner" }

    fn directions(&self) -> Vec<ScanDirection> {
        vec![ScanDirection::Input, ScanDirection::Output]
    }

    async fn scan(&self, content: &str, direction: ScanDirection) -> ScanResult {
        let mut findings = Vec::new();
        let mut processed = content.to_string();
        let mut triggered = false;
        let mut max_confidence = 0.0_f64;
        let mut final_action = GuardrailAction::Allow;

        for rule in &self.rules {
            if !rule.directions.contains(&direction) { continue; }

            if rule.regex.is_match(&processed) {
                let count = rule.regex.find_iter(&processed).count();
                findings.push(format!("{} ({} match{})", rule.name, count, if count == 1 { "" } else { "es" }));
                triggered = true;
                if rule.confidence > max_confidence { max_confidence = rule.confidence; }

                match &rule.action {
                    GuardrailAction::Block => {
                        return ScanResult {
                            scanner_name: self.name().to_string(),
                            triggered: true, confidence: max_confidence,
                            findings, action: GuardrailAction::Block,
                            processed_content: processed,
                        };
                    }
                    GuardrailAction::Redact => {
                        let replacement = rule.replacement.as_deref().unwrap_or("[REDACTED]");
                        processed = rule.regex.replace_all(&processed, replacement).to_string();
                        final_action = GuardrailAction::Redact;
                    }
                    other => { final_action = other.clone(); }
                }
            }
        }

        ScanResult {
            scanner_name: self.name().to_string(),
            triggered, confidence: max_confidence,
            findings, action: final_action,
            processed_content: processed,
        }
    }
}
