use crate::agent_profile::RiskLevel;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EscalationRule {
    pub id: String,
    pub trigger: EscalationTrigger,
    pub action: EscalationAction,
    pub reason_template: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EscalationTrigger {
    KeywordMatch { keywords: Vec<String> },
    RiskLevelAtLeast(RiskLevel),
    MissingRequiredInput,
    UserSentimentHighRisk,
    ConfidenceBelow(f32),
    RequiresSpecialist(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EscalationAction {
    RouteToAgent { agent_id: String },
    RequireApproval { approval_key: String },
    MarkBlocked,
    EmitReviewFlag,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowStepSpec {
    pub order: u32,
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SuccessMetric {
    pub label: String,
    pub target: String,
}
