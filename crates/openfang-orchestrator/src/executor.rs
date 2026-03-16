use crate::definition::{AgentNode, AgentSelector};
use crate::errors::{WorkflowError, WorkflowResult};
use async_trait::async_trait;
use openfang_kernel::OpenFangKernel;
use openfang_types::agent::AgentId;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AgentExecutionInput {
    pub run_id: Uuid,
    pub workflow_id: String,
    pub workflow_name: String,
    pub original_input: String,
    pub rendered_prompt: String,
    pub last_output: Option<String>,
    pub outputs: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct AgentExecutionResult {
    pub agent_label: String,
    pub output: String,
}

#[async_trait]
pub trait WorkflowExecutor: Send + Sync {
    async fn execute_agent(
        &self,
        node: &AgentNode,
        input: AgentExecutionInput,
    ) -> WorkflowResult<AgentExecutionResult>;
}

#[derive(Debug, Default)]
pub struct MockWorkflowExecutor;

#[async_trait]
impl WorkflowExecutor for MockWorkflowExecutor {
    async fn execute_agent(
        &self,
        node: &AgentNode,
        input: AgentExecutionInput,
    ) -> WorkflowResult<AgentExecutionResult> {
        let agent_label = match &node.agent {
            AgentSelector::ById { agent_id } => agent_id.clone(),
            AgentSelector::ByName { agent_name } => agent_name.clone(),
        };

        let output = if agent_label == "support-responder" || agent_label == "support-triage" {
            format!(
                "Category: frontend, Urgency: normal, \
                 Summary: Login page errors after latest frontend release. \
                 Route to: frontend-specialist. \
                 (triage from '{}')",
                input.original_input
            )
        } else if agent_label == "frontend-developer" || agent_label == "frontend-specialist" {
            format!(
                "Frontend analysis: The login page regression is likely caused by a CSS/JS bundle \
                 change in the latest release. Recommend: revert the layout component change and \
                 rerun the auth flow test suite. (based on triage: {})",
                input.outputs
                    .get("triage")
                    .cloned()
                    .unwrap_or_else(|| input.original_input.clone())
            )
        } else if agent_label == "security-auditor" || agent_label == "security-specialist" {
            format!(
                "Security assessment: No credential exposure detected. Issue appears to be a \
                 frontend rendering bug, not a security vulnerability. Risk: low. \
                 Recommend routing to frontend specialist. (based on: {})",
                input.original_input
            )
        } else if agent_label == "reviewer" {
            format!(
                "Review: Analysis is thorough and actionable. The root cause identification is \
                 correct. Recommended resolution is appropriate. Approved for customer response. \
                 (reviewed analysis: {})",
                input.outputs
                    .get("specialist_analysis")
                    .cloned()
                    .unwrap_or_else(|| input.rendered_prompt.clone())
            )
        } else if agent_label == "support-writer" {
            format!(
                "Draft reply: Thanks for the report. Based on '{}' we are preparing the next step.",
                input.outputs
                    .get("triage_summary")
                    .cloned()
                    .or(input.last_output)
                    .unwrap_or_else(|| input.rendered_prompt.clone())
            )
        } else {
            format!("{} handled: {}", agent_label, input.rendered_prompt)
        };

        Ok(AgentExecutionResult { agent_label, output })
    }
}

pub struct OpenFangAgentExecutor {
    kernel: Arc<OpenFangKernel>,
}

impl OpenFangAgentExecutor {
    pub fn new(kernel: Arc<OpenFangKernel>) -> Self {
        Self { kernel }
    }
}

#[async_trait]
impl WorkflowExecutor for OpenFangAgentExecutor {
    async fn execute_agent(
        &self,
        node: &AgentNode,
        input: AgentExecutionInput,
    ) -> WorkflowResult<AgentExecutionResult> {
        let (agent_id, agent_label) = match &node.agent {
            AgentSelector::ById { agent_id } => {
                let parsed = agent_id
                    .parse::<AgentId>()
                    .map_err(|_| WorkflowError::AgentNotFound(agent_id.clone()))?;
                (parsed, agent_id.clone())
            }
            AgentSelector::ByName { agent_name } => {
                let entry = self
                    .kernel
                    .registry
                    .find_by_name(agent_name)
                    .ok_or_else(|| WorkflowError::AgentNotFound(agent_name.clone()))?;
                (entry.id, entry.name)
            }
        };

        let result = self
            .kernel
            .send_message(agent_id, &input.rendered_prompt)
            .await
            .map_err(|err| WorkflowError::ExecutionFailed(err.to_string()))?;

        Ok(AgentExecutionResult {
            agent_label,
            output: result.response,
        })
    }
}
