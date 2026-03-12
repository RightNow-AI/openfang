//! SWE (Software Engineering) Agent task API routes.
//!
//! These endpoints expose task management for the SWE agent executor.
//! All routes are prefixed with `/api/swe/`.
//!
//! Features:
//! - Create and auto-start SWE tasks
//! - List, get, and cancel tasks
//! - In-memory storage with RwLock for thread safety

use crate::routes::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use maestro_swe::{SWEAgentAction, SWEAgentEvent, SWEAgentExecutor};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

// ── Types ───────────────────────────────────────────────────────────────────

/// Status of a SWE task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SWETaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// A SWE task with metadata and execution state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SWETask {
    pub id: String,
    pub description: String,
    pub actions: Vec<SWEAgentAction>,
    pub status: SWETaskStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub events: Vec<SWEAgentEvent>,
    pub error: Option<String>,
}

/// In-memory store for SWE tasks.
pub type SWETaskStore = Arc<RwLock<HashMap<String, SWETask>>>;

/// Request to create a new SWE task.
#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    /// Human-readable description of the task.
    pub description: String,
    /// List of actions to execute.
    pub actions: Vec<SWEAgentActionRequest>,
}

/// Request action format (simplified for API).
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum SWEAgentActionRequest {
    ReadFile { path: String },
    WriteFile { path: String, content: String },
    ExecuteCommand { command: String },
}

impl SWEAgentActionRequest {
    fn into_action(self) -> SWEAgentAction {
        match self {
            SWEAgentActionRequest::ReadFile { path } => SWEAgentAction::ReadFile(path),
            SWEAgentActionRequest::WriteFile { path, content } => {
                SWEAgentAction::WriteFile(path, content)
            }
            SWEAgentActionRequest::ExecuteCommand { command } => {
                SWEAgentAction::ExecuteCommand(command)
            }
        }
    }
}

/// Response after creating a task.
#[derive(Debug, Serialize)]
pub struct CreateTaskResponse {
    pub task_id: String,
    pub status: SWETaskStatus,
    pub created_at: DateTime<Utc>,
}

/// Task summary for list responses.
#[derive(Debug, Serialize)]
pub struct TaskSummary {
    pub id: String,
    pub description: String,
    pub status: SWETaskStatus,
    pub created_at: DateTime<Utc>,
    pub action_count: usize,
    pub event_count: usize,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Standard JSON error response.
fn err_response(status: StatusCode, msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({ "error": msg })))
}

/// Spawn a background task to execute SWE actions.
fn spawn_task_executor(
    task_id: String,
    actions: Vec<SWEAgentAction>,
    store: SWETaskStore,
) {
    tokio::spawn(async move {
        let executor = SWEAgentExecutor::new();
        let mut events = Vec::new();

        // Mark as running
        {
            let mut store = store.write().await;
            if let Some(task) = store.get_mut(&task_id) {
                task.status = SWETaskStatus::Running;
                task.started_at = Some(Utc::now());
            }
        }

        // Execute actions
        for action in actions {
            // Check if cancelled
            {
                let store = store.read().await;
                if let Some(task) = store.get(&task_id) {
                    if matches!(task.status, SWETaskStatus::Cancelled) {
                        return;
                    }
                }
            }

            let event = executor.execute(action);
            events.push(event);

            // Update events in store
            {
                let mut store = store.write().await;
                if let Some(task) = store.get_mut(&task_id) {
                    task.events.clone_from(&events);
                }
            }
        }

        // Mark as completed
        {
            let mut store = store.write().await;
            if let Some(task) = store.get_mut(&task_id) {
                task.status = SWETaskStatus::Completed;
                task.completed_at = Some(Utc::now());
                task.events = events;
            }
        }
    });
}

// ── Route Handlers ───────────────────────────────────────────────────────────

/// POST /api/swe/tasks
///
/// Create a new SWE task and auto-start execution.
///
/// Request body:
/// ```json
/// {
///   "description": "Read and modify config file",
///   "actions": [
///     { "type": "ReadFile", "path": "/path/to/config" },
///     { "type": "WriteFile", "path": "/path/to/config", "content": "new content" }
///   ]
/// }
/// ```
pub async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(request): Json<CreateTaskRequest>,
) -> impl IntoResponse {
    if request.description.trim().is_empty() {
        return err_response(StatusCode::BAD_REQUEST, "Task description is required")
            .into_response();
    }

    if request.actions.is_empty() {
        return err_response(StatusCode::BAD_REQUEST, "At least one action is required")
            .into_response();
    }

    let task_id = Uuid::new_v4().to_string();
    let actions: Vec<SWEAgentAction> = request
        .actions
        .into_iter()
        .map(|a| a.into_action())
        .collect();

    let task = SWETask {
        id: task_id.clone(),
        description: request.description,
        actions: actions.clone(),
        status: SWETaskStatus::Pending,
        created_at: Utc::now(),
        started_at: None,
        completed_at: None,
        events: Vec::new(),
        error: None,
    };

    // Store task
    {
        let mut store = state.swe_tasks.write().await;
        store.insert(task_id.clone(), task);
    }

    // Auto-start execution
    spawn_task_executor(task_id.clone(), actions, state.swe_tasks.clone());

    let response = CreateTaskResponse {
        task_id,
        status: SWETaskStatus::Pending,
        created_at: Utc::now(),
    };

    (StatusCode::CREATED, Json(response)).into_response()
}

/// GET /api/swe/tasks
///
/// List all SWE tasks with summaries.
pub async fn list_tasks(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let store = state.swe_tasks.read().await;

    let summaries: Vec<TaskSummary> = store
        .values()
        .map(|task| TaskSummary {
            id: task.id.clone(),
            description: task.description.clone(),
            status: task.status.clone(),
            created_at: task.created_at,
            action_count: task.actions.len(),
            event_count: task.events.len(),
        })
        .collect();

    (StatusCode::OK, Json(summaries)).into_response()
}

/// GET /api/swe/tasks/{id}
///
/// Get detailed information about a specific task.
pub async fn get_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    let store = state.swe_tasks.read().await;

    match store.get(&task_id) {
        Some(task) => (StatusCode::OK, Json(task.clone())).into_response(),
        None => {
            err_response(StatusCode::NOT_FOUND, "Task not found").into_response()
        }
    }
}

/// DELETE /api/swe/tasks/{id}
///
/// Cancel and delete a task. Running tasks are marked for cancellation.
pub async fn delete_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    let mut store = state.swe_tasks.write().await;

    match store.get_mut(&task_id) {
        Some(task) => {
            match task.status {
                SWETaskStatus::Pending | SWETaskStatus::Running => {
                    task.status = SWETaskStatus::Cancelled;
                    task.completed_at = Some(Utc::now());
                }
                _ => {}
            }
            store.remove(&task_id);
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "deleted",
                    "task_id": task_id
                })),
            )
                .into_response()
        }
        None => err_response(StatusCode::NOT_FOUND, "Task not found").into_response(),
    }
}

/// POST /api/swe/tasks/{id}/cancel
///
/// Cancel a running or pending task without deleting it.
pub async fn cancel_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    let mut store = state.swe_tasks.write().await;

    match store.get_mut(&task_id) {
        Some(task) => {
            match task.status {
                SWETaskStatus::Pending | SWETaskStatus::Running => {
                    task.status = SWETaskStatus::Cancelled;
                    task.completed_at = Some(Utc::now());
                    (
                        StatusCode::OK,
                        Json(serde_json::json!({
                            "status": "cancelled",
                            "task_id": task_id
                        })),
                    )
                        .into_response()
                }
                _ => err_response(
                    StatusCode::CONFLICT,
                    &format!("Task is already {:?}", task.status),
                )
                .into_response(),
            }
        }
        None => err_response(StatusCode::NOT_FOUND, "Task not found").into_response(),
    }
}

/// GET /api/swe/tasks/{id}/events
///
/// Get all events for a specific task.
pub async fn get_task_events(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    let store = state.swe_tasks.read().await;

    match store.get(&task_id) {
        Some(task) => {
            let events: Vec<serde_json::Value> = task
                .events
                .iter()
                .map(|e| match e {
                    SWEAgentEvent::FileRead(path, content) => {
                        serde_json::json!({
                            "type": "FileRead",
                            "path": path,
                            "content_preview": content.chars().take(200).collect::<String>(),
                        })
                    }
                    SWEAgentEvent::FileWritten(path) => {
                        serde_json::json!({
                            "type": "FileWritten",
                            "path": path,
                        })
                    }
                    SWEAgentEvent::CommandExecuted(command, output, exit_code) => {
                        serde_json::json!({
                            "type": "CommandExecuted",
                            "command": command,
                            "output_preview": output.chars().take(500).collect::<String>(),
                            "exit_code": exit_code,
                        })
                    }
                })
                .collect();
            (StatusCode::OK, Json(events)).into_response()
        }
        None => err_response(StatusCode::NOT_FOUND, "Task not found").into_response(),
    }
}
