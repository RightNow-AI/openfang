use chrono::{DateTime, Utc};
use openfang_types::approval::ApprovalDecision;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowRunStatus {
    Running,
    WaitingApproval,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    Agent,
    Approval,
    Route,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingApproval {
    pub approval_id: Uuid,
    pub step_id: String,
    pub title: String,
    pub prompt: String,
    pub requested_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartWorkflowRequest {
    pub workflow_id: Option<String>,
    pub input: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeWorkflowRequest {
    pub approval_id: Uuid,
    pub decision: ApprovalDecision,
    pub decided_by: Option<String>,
}
