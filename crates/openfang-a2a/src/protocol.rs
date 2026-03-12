use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct A2AMessage {
    pub version: String,
    pub message_id: String,
    pub sender_id: String,
    pub receiver_id: String,
    pub payload: A2APayload,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum A2APayload {
    TaskRequest(TaskRequest),
    TaskResponse(TaskResponse),
    /// Request to execute a software engineering task
    SWETaskRequest {
        task_id: String,
        description: String,
        actions: Vec<SWEActionRequest>,
    },
    /// Response with SWE task result
    SWETaskResponse {
        task_id: String,
        status: SWETaskStatus,
        events: Vec<SWEAgentEvent>,
        result: Option<String>,
        error: Option<String>,
    },
    Heartbeat,
}

/// Request action for SWE tasks.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SWEActionRequest {
    ReadFile(String),
    WriteFile(String, String),
    ExecuteCommand(String),
}

/// SWE task execution status.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SWETaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// SWE agent execution event.
#[derive(Serialize, Deserialize, Debug, Clone)]
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TaskRequest {
    pub task_id: String,
    pub task_name: String,
    pub task_input: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TaskResponse {
    pub task_id: String,
    pub task_output: String,
    pub task_status: TaskStatus,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TaskStatus {
    Success,
    Failure,
}
