use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SWEAgentAction {
    ReadFile(String),
    WriteFile(String, String),
    ExecuteCommand(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SWEAgentEvent {
    FileRead(String, String),
    FileWritten(String),
    CommandExecuted(String, String, i32),
}
