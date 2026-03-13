use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub definition: WorkflowDefinitionSpec,
    pub default_agents: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowDefinitionSpec {
    pub start_node: String,
    pub nodes: Vec<WorkflowNodeSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowNodeSpec {
    Intake(IntakeNodeSpec),
    AgentCall(AgentCallNodeSpec),
    ApprovalGate(ApprovalGateNodeSpec),
    Route(RouteNodeSpec),
    Complete(CompleteNodeSpec),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntakeNodeSpec {
    pub id: String,
    pub next: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentCallNodeSpec {
    pub id: String,
    pub agent_profile_id: String,
    pub purpose: String,
    pub expected_deliverable_ids: Vec<String>,
    pub next: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalGateNodeSpec {
    pub id: String,
    pub approval_key: String,
    pub next_on_approve: String,
    pub next_on_reject: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteNodeSpec {
    pub id: String,
    pub rules: Vec<RouteRuleSpec>,
    pub fallback_next: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteRuleSpec {
    pub when: String,
    pub next: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompleteNodeSpec {
    pub id: String,
}
