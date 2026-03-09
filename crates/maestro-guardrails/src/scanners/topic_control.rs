//! Topic control scanner — enforces allowed/blocked topics.
//! Inspired by Kore.ai's topic enforcement guardrail.
//!
//! Uses keyword matching with configurable allowed and blocked topic lists.
//! Supports both whitelist mode (only allow listed topics) and blacklist mode
//! (block listed topics, allow everything else).

use async_trait::async_trait;
use crate::{GuardrailAction, ScanDirection, ScanResult, Scanner};

/// A topic definition with keywords and policy.
#[derive(Debug, Clone)]
pub struct TopicDefinition {
    pub name: String,
    pub keywords: Vec<String>,
    pub allowed: bool,
    pub blocked_response: Option<String>,
}

impl TopicDefinition {
    pub fn allowed(name: impl Into<String>, keywords: Vec<impl Into<String>>) -> Self {
        Self {
            name: name.into(),
            keywords: keywords.into_iter().map(|k| k.into()).collect(),
            allowed: true,
            blocked_response: None,
        }
    }

    pub fn blocked(name: impl Into<String>, keywords: Vec<impl Into<String>>, response: Option<String>) -> Self {
        Self {
            name: name.into(),
            keywords: keywords.into_iter().map(|k| k.into()).collect(),
            allowed: false,
            blocked_response: response,
        }
    }

    pub fn matches(&self, content: &str) -> bool {
        let lower = content.to_lowercase();
        self.keywords.iter().any(|kw| lower.contains(&kw.to_lowercase()))
    }
}

/// Topic control scanner configuration.
#[derive(Debug, Clone, Default)]
pub struct TopicControlConfig {
    pub topics: Vec<TopicDefinition>,
    pub whitelist_mode: bool,
    pub default_blocked_response: Option<String>,
}

impl TopicControlConfig {
    pub fn blacklist(blocked_topics: Vec<TopicDefinition>) -> Self {
        Self { topics: blocked_topics, whitelist_mode: false, default_blocked_response: None }
    }

    pub fn whitelist(allowed_topics: Vec<TopicDefinition>, default_response: Option<String>) -> Self {
        Self { topics: allowed_topics, whitelist_mode: true, default_blocked_response: default_response }
    }
}

/// Topic control scanner.
pub struct TopicControlScanner {
    config: TopicControlConfig,
}

impl TopicControlScanner {
    pub fn new() -> Self {
        Self { config: TopicControlConfig::default() }
    }

    pub fn with_config(config: TopicControlConfig) -> Self {
        Self { config }
    }

    pub fn with_defaults() -> Self {
        let blocked = vec![
            TopicDefinition::blocked(
                "violence",
                vec!["how to kill", "how to murder", "how to assault"],
                Some("I'm not able to discuss topics related to violence.".to_string()),
            ),
            TopicDefinition::blocked(
                "illegal_activities",
                vec!["how to steal", "how to hack", "money laundering", "drug trafficking"],
                Some("I'm not able to assist with illegal activities.".to_string()),
            ),
        ];
        Self::with_config(TopicControlConfig::blacklist(blocked))
    }
}

impl Default for TopicControlScanner {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Scanner for TopicControlScanner {
    fn name(&self) -> &str { "topic_control_scanner" }

    fn directions(&self) -> Vec<ScanDirection> {
        vec![ScanDirection::Input, ScanDirection::Output]
    }

    async fn scan(&self, content: &str, _direction: ScanDirection) -> ScanResult {
        if self.config.topics.is_empty() {
            return ScanResult {
                scanner_name: self.name().to_string(),
                triggered: false, confidence: 0.0, findings: vec![],
                action: GuardrailAction::Allow, processed_content: content.to_string(),
            };
        }

        if self.config.whitelist_mode {
            let matched_allowed = self.config.topics.iter()
                .filter(|t| t.allowed && t.matches(content))
                .count();
            if matched_allowed == 0 {
                let response = self.config.default_blocked_response.clone()
                    .unwrap_or_else(|| "This topic is outside my allowed scope.".to_string());
                return ScanResult {
                    scanner_name: self.name().to_string(),
                    triggered: true, confidence: 0.85,
                    findings: vec!["No allowed topic matched (whitelist mode)".to_string()],
                    action: GuardrailAction::Replace { response },
                    processed_content: content.to_string(),
                };
            }
        } else {
            let matched_blocked: Vec<&TopicDefinition> = self.config.topics.iter()
                .filter(|t| !t.allowed && t.matches(content))
                .collect();
            if !matched_blocked.is_empty() {
                let findings: Vec<String> = matched_blocked.iter().map(|t| t.name.clone()).collect();
                let response = matched_blocked[0].blocked_response.clone()
                    .unwrap_or_else(|| "This topic is not allowed.".to_string());
                return ScanResult {
                    scanner_name: self.name().to_string(),
                    triggered: true, confidence: 0.85,
                    findings,
                    action: GuardrailAction::Replace { response },
                    processed_content: content.to_string(),
                };
            }
        }

        ScanResult {
            scanner_name: self.name().to_string(),
            triggered: false, confidence: 0.0, findings: vec![],
            action: GuardrailAction::Allow, processed_content: content.to_string(),
        }
    }
}
