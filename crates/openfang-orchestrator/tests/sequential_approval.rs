use openfang_orchestrator::{
    AgentNode, AgentSelector, ApprovalNode, ApprovalRejection, InMemoryWorkflowStore,
    MockWorkflowExecutor, WorkflowDefinition, WorkflowEngine, WorkflowNode, WorkflowRunStatus,
};
use openfang_types::approval::ApprovalDecision;
use std::sync::Arc;

fn approval_workflow() -> WorkflowDefinition {
    WorkflowDefinition {
        id: "approval-flow".to_string(),
        name: "Approval flow".to_string(),
        description: "Test sequential approval behavior".to_string(),
        steps: vec![
            WorkflowNode::Agent(AgentNode {
                id: "step-1".to_string(),
                title: "First agent".to_string(),
                agent: AgentSelector::ByName {
                    agent_name: "researcher".to_string(),
                },
                prompt: "Research: {{input}}".to_string(),
                store_as: Some("research".to_string()),
                next_step_id: None,
            }),
            WorkflowNode::Approval(ApprovalNode {
                id: "approval-1".to_string(),
                title: "Human review".to_string(),
                prompt: "Approve continuation?".to_string(),
                on_rejected: ApprovalRejection::FailRun,
            }),
            WorkflowNode::Agent(AgentNode {
                id: "step-2".to_string(),
                title: "Second agent".to_string(),
                agent: AgentSelector::ByName {
                    agent_name: "writer".to_string(),
                },
                prompt: "Write using {{research}}".to_string(),
                store_as: Some("draft".to_string()),
                next_step_id: None,
            }),
        ],
    }
}

#[tokio::test]
async fn workflow_waits_and_resumes_after_approval() {
    let store = Arc::new(InMemoryWorkflowStore::new());
    let engine = WorkflowEngine::new(store, Arc::new(MockWorkflowExecutor));
    engine.register_definition(approval_workflow()).await.unwrap();

    let run = engine
        .start_workflow("approval-flow", "Investigate the outage".to_string())
        .await
        .unwrap();

    assert_eq!(run.status, WorkflowRunStatus::WaitingApproval);
    assert_eq!(run.steps.len(), 1);
    assert!(run.pending_approval.is_some());

    let pending = run.pending_approval.clone().unwrap();
    let resumed = engine
        .resume_workflow(
            run.id,
            pending.approval_id,
            ApprovalDecision::Approved,
            Some("ops-lead".to_string()),
        )
        .await
        .unwrap();

    assert_eq!(resumed.status, WorkflowRunStatus::Completed);
    assert_eq!(resumed.steps.len(), 3);
    assert!(resumed.last_output.unwrap().contains("writer handled"));
}

#[tokio::test]
async fn workflow_rejects_wrong_approval_id() {
    let store = Arc::new(InMemoryWorkflowStore::new());
    let engine = WorkflowEngine::new(store, Arc::new(MockWorkflowExecutor));
    engine.register_definition(approval_workflow()).await.unwrap();

    let run = engine
        .start_workflow("approval-flow", "Investigate the outage".to_string())
        .await
        .unwrap();

    let error = engine
        .resume_workflow(
            run.id,
            uuid::Uuid::new_v4(),
            ApprovalDecision::Approved,
            None,
        )
        .await
        .unwrap_err();

    assert!(error.to_string().contains("approval id"));
}

#[tokio::test]
async fn workflow_fails_when_approval_is_denied() {
    let store = Arc::new(InMemoryWorkflowStore::new());
    let engine = WorkflowEngine::new(store, Arc::new(MockWorkflowExecutor));
    engine.register_definition(approval_workflow()).await.unwrap();

    let run = engine
        .start_workflow("approval-flow", "Investigate the outage".to_string())
        .await
        .unwrap();

    let pending = run.pending_approval.clone().unwrap();
    let denied = engine
        .resume_workflow(run.id, pending.approval_id, ApprovalDecision::Denied, None)
        .await
        .unwrap();

    assert_eq!(denied.status, WorkflowRunStatus::Failed);
    assert_eq!(denied.steps.len(), 2);
    assert_eq!(denied.error.as_deref(), Some("Approval denied"));
}
