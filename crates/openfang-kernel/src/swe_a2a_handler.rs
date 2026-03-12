//! SWE Agent A2A Handler for executing software engineering tasks.
//!
//! This handler processes SWE task requests via A2A protocol,
//! executing file operations and commands using SWEAgentExecutor.

use crate::a2a_registry::A2AHandler;
use openfang_a2a::protocol::{
    A2AMessage, A2APayload, SWEActionRequest, SWEAgentEvent, SWETaskStatus,
};
use maestro_swe::{SWEAgentAction, SWEAgentExecutor};

/// Handler for SWE agent A2A messages.
pub struct SWEA2AHandler {
    executor: SWEAgentExecutor,
}

impl SWEA2AHandler {
    /// Create a new SWE A2A handler.
    pub fn new() -> Self {
        Self {
            executor: SWEAgentExecutor::default(),
        }
    }
}

impl Default for SWEA2AHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl A2AHandler for SWEA2AHandler {
    async fn handle_message(&self, message: A2AMessage) -> Option<A2AMessage> {
        match message.payload {
            A2APayload::SWETaskRequest {
                task_id,
                description,
                actions,
            } => {
                let response = self.handle_swe_task(task_id, description, actions).await;
                Some(A2AMessage {
                    version: "1.0".to_string(),
                    message_id: uuid::Uuid::new_v4().to_string(),
                    sender_id: "swe".to_string(),
                    receiver_id: message.sender_id,
                    payload: response,
                })
            }
            _ => None,
        }
    }
}

impl SWEA2AHandler {
    /// Handle a SWE task request.
    async fn handle_swe_task(
        &self,
        task_id: String,
        _description: String,
        actions: Vec<SWEActionRequest>,
    ) -> A2APayload {
        let mut events = Vec::new();
        let mut result = String::new();
        let mut has_error = false;

        for action_req in actions {
            let action = convert_action(action_req);
            let event = self.executor.execute(action).await;

            // Accumulate results
            match &event {
                maestro_swe::SWEAgentEvent::FileRead(path, content) => {
                    events.push(SWEAgentEvent::FileRead(path.clone(), content.clone()));
                    result.push_str(&format!("Read file {} ({} chars)\n", path, content.len()));
                }
                maestro_swe::SWEAgentEvent::FileWritten(path) => {
                    events.push(SWEAgentEvent::FileWritten(path.clone()));
                    result.push_str(&format!("Wrote file {}\n", path));
                }
                maestro_swe::SWEAgentEvent::FileReadFailed(path, error) => {
                    events.push(SWEAgentEvent::FileReadFailed(path.clone(), error.clone()));
                    result.push_str(&format!("Failed to read file {}: {}\n", path, error));
                    has_error = true;
                }
                maestro_swe::SWEAgentEvent::FileWriteFailed(path, error) => {
                    events.push(SWEAgentEvent::FileWriteFailed(path.clone(), error.clone()));
                    result.push_str(&format!("Failed to write file {}: {}\n", path, error));
                    has_error = true;
                }
                maestro_swe::SWEAgentEvent::CommandExecuted(cmd, output, code) => {
                    events.push(SWEAgentEvent::CommandExecuted(
                        cmd.clone(),
                        output.clone(),
                        *code,
                    ));
                    result.push_str(&format!(
                        "Executed '{}': exit_code={}\n",
                        cmd, code
                    ));
                }
                maestro_swe::SWEAgentEvent::CommandBlocked(cmd, reason) => {
                    events.push(SWEAgentEvent::CommandBlocked(cmd.clone(), reason.clone()));
                    result.push_str(&format!("Command '{}' blocked: {}\n", cmd, reason));
                    has_error = true;
                }
                maestro_swe::SWEAgentEvent::CommandTimedOut(cmd, timeout_secs) => {
                    events.push(SWEAgentEvent::CommandTimedOut(cmd.clone(), *timeout_secs));
                    result.push_str(&format!("Command '{}' timed out after {}s\n", cmd, timeout_secs));
                    has_error = true;
                }
                maestro_swe::SWEAgentEvent::PathBlocked(path, reason) => {
                    events.push(SWEAgentEvent::PathBlocked(path.clone(), reason.clone()));
                    result.push_str(&format!("Path '{}' blocked: {}\n", path, reason));
                    has_error = true;
                }
            }
        }

        let status = if has_error {
            SWETaskStatus::Failed
        } else {
            SWETaskStatus::Completed
        };

        A2APayload::SWETaskResponse {
            task_id,
            status,
            events,
            result: Some(result),
            error: if has_error { Some("One or more actions failed".to_string()) } else { None },
        }
    }
}

/// Convert A2A action request to maestro-swe action.
fn convert_action(req: SWEActionRequest) -> SWEAgentAction {
    match req {
        SWEActionRequest::ReadFile(path) => SWEAgentAction::ReadFile(path),
        SWEActionRequest::WriteFile(path, content) => {
            SWEAgentAction::WriteFile(path, content)
        }
        SWEActionRequest::ExecuteCommand(cmd) => SWEAgentAction::ExecuteCommand(cmd),
    }
}