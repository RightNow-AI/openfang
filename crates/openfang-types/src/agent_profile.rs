use crate::deliverable::{DeliverableContract, DeliverableTemplate};
use crate::escalation::{EscalationRule, SuccessMetric, WorkflowStepSpec};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentProfile {
    pub id: String,
    pub source: AgentProfileSource,
    pub display_name: String,
    pub division: AgentDivision,
    pub role: String,
    pub summary: String,
    pub personality_traits: Vec<String>,
    pub memory_notes: Vec<String>,
    pub core_missions: Vec<String>,
    pub critical_rules: Vec<String>,
    pub workflow_steps: Vec<WorkflowStepSpec>,
    pub deliverables: Vec<DeliverableContract>,
    pub deliverable_templates: Vec<DeliverableTemplate>,
    pub success_metrics: Vec<SuccessMetric>,
    pub escalation_rules: Vec<EscalationRule>,
    pub communication_style: Vec<String>,
    pub best_for: Vec<String>,
    pub avoid_for: Vec<String>,
    pub tags: Vec<String>,
    pub risk_level: RiskLevel,
    pub approval_policy: ApprovalPolicy,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentProfileSource {
    NativeToml,
    ImportedAgencyMarkdown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentDivision {
    Engineering,
    Design,
    Marketing,
    Product,
    ProjectManagement,
    Support,
    Testing,
    Strategy,
    Specialized,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalPolicy {
    pub required_for_external_send: bool,
    pub required_for_sensitive_actions: bool,
    pub required_for_policy_or_legal_language: bool,
}
