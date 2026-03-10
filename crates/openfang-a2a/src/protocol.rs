use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct A2AMessage {
    pub version: String,
    pub message_id: String,
    pub sender_id: String,
    pub receiver_id: String,
    pub payload: A2APayload,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum A2APayload {
    TaskRequest(TaskRequest),
    TaskResponse(TaskResponse),
    Heartbeat,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TaskRequest {
    pub task_id: String,
    pub task_name: String,
    pub task_input: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TaskResponse {
    pub task_id: String,
    pub task_output: String,
    pub task_status: TaskStatus,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum TaskStatus {
    Success,
    Failure,
}
