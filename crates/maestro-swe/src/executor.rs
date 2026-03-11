use crate::protocol::{SWEAgentAction, SWEAgentEvent};
use std::process::Command;

pub struct SWEAgentExecutor;

impl Default for SWEAgentExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl SWEAgentExecutor {
    pub fn new() -> Self {
        Self
    }

    pub fn execute(&self, action: SWEAgentAction) -> SWEAgentEvent {
        match action {
            SWEAgentAction::ReadFile(path) => match std::fs::read_to_string(path.clone()) {
                Ok(content) => SWEAgentEvent::FileRead(path, content),
                Err(e) => SWEAgentEvent::FileRead(path, e.to_string()),
            },
            SWEAgentAction::WriteFile(path, content) => {
                match std::fs::write(path.clone(), content) {
                    Ok(_) => SWEAgentEvent::FileWritten(path),
                    Err(_e) => SWEAgentEvent::FileWritten(path),
                }
            }
            SWEAgentAction::ExecuteCommand(command) => {
                let output = match Command::new("sh").arg("-c").arg(command.clone()).output() {
                    Ok(output) => output,
                    Err(e) => {
                        return SWEAgentEvent::CommandExecuted(
                            command,
                            format!("Failed to execute: {}", e),
                            -1,
                        );
                    }
                };

                SWEAgentEvent::CommandExecuted(
                    command,
                    String::from_utf8_lossy(&output.stdout).to_string(),
                    output.status.code().unwrap_or(-1),
                )
            }
        }
    }
}
