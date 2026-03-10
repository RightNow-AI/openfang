use crate::{GuardrailAction, ScanDirection, ScanResult, Scanner};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToxicityScanner {
    pub threshold: f64,
}

#[async_trait]
impl Scanner for ToxicityScanner {
    fn name(&self) -> &str {
        "toxicity"
    }

    fn directions(&self) -> Vec<ScanDirection> {
        vec![ScanDirection::Input, ScanDirection::Output]
    }

    async fn scan(&self, content: &str, _direction: ScanDirection) -> ScanResult {
        // This is a placeholder for a real toxicity detection model.
        // In a real implementation, this would call an external API
        // or a local model to get a toxicity score.
        let is_toxic = content.to_lowercase().contains("toxic");
        let score = if is_toxic { 1.0 } else { 0.0 };

        let triggered = score >= self.threshold;

        ScanResult {
            scanner_name: self.name().to_string(),
            triggered,
            confidence: score,
            findings: if triggered {
                vec!["Toxic content detected".to_string()]
            } else {
                Vec::new()
            },
            action: if triggered {
                GuardrailAction::Block
            } else {
                GuardrailAction::Allow
            },
            processed_content: content.to_string(),
        }
    }
}
