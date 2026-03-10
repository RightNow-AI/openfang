use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum SWEAgentAction {
    ReadFile(String),
    WriteFile(String, String),
    ExecuteCommand(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum SWEAgentEvent {
    FileRead(String, String),
    FileWritten(String),
    CommandExecuted(String, String, i32),
}
