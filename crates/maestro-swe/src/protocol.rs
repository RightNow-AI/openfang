//! SWE Agent Protocol Types
//!
//! Defines the action/event types for the Software Engineering Agent executor.
//! Actions are operations the agent can perform; Events are the results.

use serde::{Deserialize, Serialize};

/// Actions that can be performed by the SWE agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SWEAgentAction {
    /// Read the contents of a file at the given path.
    /// The path will be validated against the working directory sandbox.
    ReadFile(String),

    /// Write content to a file at the given path.
    /// The path will be validated against the working directory sandbox.
    WriteFile(String, String),

    /// Execute a shell command.
    /// Only whitelisted commands are allowed (e.g., ls, cat, grep, cargo, git).
    /// Commands are executed with a configurable timeout.
    ExecuteCommand(String),
}

/// Events produced by SWE agent action execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SWEAgentEvent {
    /// File was successfully read. Contains (path, content).
    FileRead(String, String),

    /// File was successfully written. Contains path.
    FileWritten(String),

    /// File read failed. Contains (path, error_message).
    FileReadFailed(String, String),

    /// File write failed. Contains (path, error_message).
    FileWriteFailed(String, String),

    /// Command was executed. Contains (command, stdout, exit_code).
    CommandExecuted(String, String, i32),

    /// Command was blocked due to security policy. Contains (command, reason).
    CommandBlocked(String, String),

    /// Command execution timed out. Contains (command, timeout_seconds).
    CommandTimedOut(String, u64),

    /// Path traversal attempt was blocked. Contains (path, reason).
    PathBlocked(String, String),
}

/// Configuration for the SWE agent executor.
#[derive(Debug, Clone)]
pub struct SWEExecutorConfig {
    /// Working directory for file operations. All paths are sandboxed to this directory.
    pub working_dir: std::path::PathBuf,

    /// Timeout for command execution in seconds.
    pub command_timeout_secs: u64,

    /// Whether to allow all commands (dangerous - for testing only).
    pub allow_all_commands: bool,
}

impl Default for SWEExecutorConfig {
    fn default() -> Self {
        let working_dir = std::env::current_dir()
            .and_then(|p| p.canonicalize())
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        Self {
            working_dir,
            command_timeout_secs: 30,
            allow_all_commands: false,
        }
    }
}

impl SWEExecutorConfig {
    /// Create a new config with the specified working directory.
    /// The working directory will be canonicalized to resolve symlinks.
    pub fn new(working_dir: impl Into<std::path::PathBuf>) -> Self {
        let working_dir = working_dir.into();
        let canonicalized = working_dir
            .canonicalize()
            .unwrap_or(working_dir);
        Self {
            working_dir: canonicalized,
            ..Default::default()
        }
    }

    /// Set the command timeout.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.command_timeout_secs = secs;
        self
    }

    /// Allow all commands (dangerous - for testing only).
    #[cfg(test)]
    pub fn with_allow_all_commands(mut self) -> Self {
        self.allow_all_commands = true;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_serialization() {
        let action = SWEAgentAction::ReadFile("test.txt".to_string());
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("ReadFile"));
    }

    #[test]
    fn test_event_serialization() {
        let event = SWEAgentEvent::FileWritten("test.txt".to_string());
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("FileWritten"));
    }
}