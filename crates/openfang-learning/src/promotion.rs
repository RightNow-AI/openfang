//! Promotion policy — controls when and where learnings are promoted to core memory.

use crate::event::LearningType;
use serde::{Deserialize, Serialize};

/// Policy controlling automatic promotion of learnings to core memory files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionPolicy {
    /// Composite score threshold (0–100) above which a learning is auto-promoted.
    /// Default: 60.0
    pub promotion_threshold: f32,

    /// Whether to auto-promote security observations immediately (ignores threshold).
    /// Default: true
    pub always_promote_security: bool,

    /// Whether to auto-promote errors with resolutions immediately.
    /// Default: false
    pub always_promote_resolved_errors: bool,
}

impl Default for PromotionPolicy {
    fn default() -> Self {
        Self {
            promotion_threshold: 60.0,
            always_promote_security: true,
            always_promote_resolved_errors: false,
        }
    }
}

impl PromotionPolicy {
    /// Returns the target core memory filename for a given learning type.
    pub fn target_file(&self, learning_type: &LearningType) -> String {
        match learning_type {
            LearningType::SecurityObservation => "SECURITY.md".to_string(),
            LearningType::Error => "SOUL.md".to_string(),
            LearningType::BestPractice => "TOOLS.md".to_string(),
            LearningType::Correction | LearningType::KnowledgeGap => "AGENTS.md".to_string(),
            LearningType::FeatureRequest => "FEATURE_REQUESTS.md".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_file_maps_every_learning_type() {
        let policy = PromotionPolicy::default();
        assert_eq!(
            policy.target_file(&LearningType::SecurityObservation),
            "SECURITY.md"
        );
        assert_eq!(policy.target_file(&LearningType::Error), "SOUL.md");
        assert_eq!(
            policy.target_file(&LearningType::BestPractice),
            "TOOLS.md"
        );
        assert_eq!(
            policy.target_file(&LearningType::Correction),
            "AGENTS.md"
        );
        assert_eq!(
            policy.target_file(&LearningType::KnowledgeGap),
            "AGENTS.md"
        );
        assert_eq!(
            policy.target_file(&LearningType::FeatureRequest),
            "FEATURE_REQUESTS.md"
        );
    }
}
