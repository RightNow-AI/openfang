use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub steps: Vec<WorkflowNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowNode {
    Agent(AgentNode),
    Approval(ApprovalNode),
    Route(RouteNode),
}

impl WorkflowNode {
    pub fn step_id(&self) -> &str {
        match self {
            WorkflowNode::Agent(n) => &n.id,
            WorkflowNode::Approval(n) => &n.id,
            WorkflowNode::Route(n) => &n.id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentNode {
    pub id: String,
    pub title: String,
    pub agent: AgentSelector,
    pub prompt: String,
    #[serde(default)]
    pub store_as: Option<String>,
    /// If set, jump to this step id after completion instead of advancing linearly.
    #[serde(default)]
    pub next_step_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum AgentSelector {
    ById { agent_id: String },
    ByName { agent_name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalNode {
    pub id: String,
    pub title: String,
    pub prompt: String,
    pub on_rejected: ApprovalRejection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApprovalRejection {
    FailRun,
    CompleteRun { message: String },
}

/// Keyword-based router. Inspects `last_output` and jumps to the first matching step id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteNode {
    pub id: String,
    pub title: String,
    pub rules: Vec<RouteRule>,
    /// Step id to use when no rule matches.
    pub fallback_step_id: String,
}

/// A single routing rule: if `last_output` contains `when_contains` (case-insensitive), jump to `next_step_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteRule {
    pub when_contains: String,
    pub next_step_id: String,
}
