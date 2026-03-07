//! Custom regex scanner — user-defined pattern matching.

use async_trait::async_trait;
use crate::{GuardrailAction, ScanDirection, ScanResult, Scanner};

pub struct CustomRegexScanner;

#[async_trait]
impl Scanner for CustomRegexScanner {
    fn name(&self) -> &str { "custom_regex_scanner" }
    fn directions(&self) -> Vec<ScanDirection> { vec![ScanDirection::Input, ScanDirection::Output] }
    async fn scan(&self, content: &str, _direction: ScanDirection) -> ScanResult {
        // TODO: Implement configurable regex pattern matching
        ScanResult {
            scanner_name: self.name().to_string(),
            triggered: false, confidence: 0.0, findings: vec![],
            action: GuardrailAction::Allow, processed_content: content.to_string(),
        }
    }
}
