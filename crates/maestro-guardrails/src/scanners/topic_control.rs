//! Topic control scanner — enforces allowed/blocked topics.
//! Inspired by Kore.ai's topic enforcement guardrail.

use async_trait::async_trait;
use crate::{GuardrailAction, ScanDirection, ScanResult, Scanner};

pub struct TopicControlScanner;

#[async_trait]
impl Scanner for TopicControlScanner {
    fn name(&self) -> &str { "topic_control_scanner" }
    fn directions(&self) -> Vec<ScanDirection> { vec![ScanDirection::Input, ScanDirection::Output] }
    async fn scan(&self, content: &str, _direction: ScanDirection) -> ScanResult {
        // TODO: Implement topic classification
        ScanResult {
            scanner_name: self.name().to_string(),
            triggered: false, confidence: 0.0, findings: vec![],
            action: GuardrailAction::Allow, processed_content: content.to_string(),
        }
    }
}
