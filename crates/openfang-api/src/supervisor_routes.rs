//! Supervisor engine API routes.
//!
//! These endpoints expose the MAESTRO supervisor engine to the dashboard
//! frontend and external integrations. All routes are prefixed with
//! `/api/supervisor/`.
//!
//! When the supervisor engine is not initialized, every endpoint returns
//! 503 Service Unavailable.

use crate::routes::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use openfang_kernel::supervisor_engine::{OrchestrationId, SupervisorEngine};
use serde::Deserialize;
use std::sync::Arc;

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Returns a reference to the supervisor engine, or a 503 error.
fn require_supervisor(
    state: &AppState,
) -> Result<&Arc<SupervisorEngine>, (StatusCode, Json<serde_json::Value>)> {
    state
        .kernel
        .supervisor_engine
        .as_ref()
        .ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Supervisor engine is not initialized."
                })),
            )
        })
}

/// Standard JSON error response.
fn err_response(status: StatusCode, msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({ "error": msg })))
}

// ── Query Parameters ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct OrchestrationRequest {
    /// The task to orchestrate.
    pub task: String,
    /// Optional capabilities the task requires.
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct RunIdPath {
    pub run_id: String,
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    /// Maximum number of results to return.
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    20
}

// ── Route Handlers ──────────────────────────────────────────────────────────

/// GET /api/supervisor/status
///
/// Returns the current status of the supervisor engine including active run,
/// statistics, and recent orchestration history.
pub async fn supervisor_status(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let engine = match require_supervisor(&state) {
        Ok(e) => e,
        Err(err) => return err.into_response(),
    };

    let status = engine.status().await;
    (StatusCode::OK, Json(serde_json::to_value(&status).unwrap_or_default())).into_response()
}

/// POST /api/supervisor/orchestrate
///
/// Submit a task for supervisor orchestration. The supervisor will assess
/// complexity and either delegate to a single agent or run the full MAESTRO
/// pipeline.
///
/// Request body:
/// ```json
/// {
///   "task": "Research and summarize the latest AI safety papers",
///   "capabilities": ["web_search", "file_write"]
/// }
/// ```
pub async fn orchestrate(
    State(state): State<Arc<AppState>>,
    Json(request): Json<OrchestrationRequest>,
) -> impl IntoResponse {
    let engine = match require_supervisor(&state) {
        Ok(e) => e,
        Err(err) => return err.into_response(),
    };

    if request.task.trim().is_empty() {
        return err_response(StatusCode::BAD_REQUEST, "Task description is required")
            .into_response();
    }

    match engine.orchestrate(&request.task, &request.capabilities).await {
        Ok(result) => {
            (StatusCode::OK, Json(serde_json::to_value(&result).unwrap_or_default()))
                .into_response()
        }
        Err(e) => {
            err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Orchestration failed: {e}"),
            )
            .into_response()
        }
    }
}

/// GET /api/supervisor/runs/{run_id}
///
/// Get the details of a specific orchestration run.
pub async fn get_run(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<String>,
) -> impl IntoResponse {
    let engine = match require_supervisor(&state) {
        Ok(e) => e,
        Err(err) => return err.into_response(),
    };

    let uuid = match run_id.parse::<uuid::Uuid>() {
        Ok(u) => u,
        Err(_) => {
            return err_response(StatusCode::BAD_REQUEST, "Invalid run ID format").into_response()
        }
    };

    let orch_id = OrchestrationId(uuid);

    match engine.get_run(orch_id).await {
        Some(result) => {
            (StatusCode::OK, Json(serde_json::to_value(&result).unwrap_or_default()))
                .into_response()
        }
        None => err_response(StatusCode::NOT_FOUND, "Orchestration run not found").into_response(),
    }
}

/// GET /api/supervisor/history
///
/// Get the orchestration history. Supports `?limit=N` query parameter.
pub async fn history(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HistoryQuery>,
) -> impl IntoResponse {
    let engine = match require_supervisor(&state) {
        Ok(e) => e,
        Err(err) => return err.into_response(),
    };

    let all_history = engine.history().await;
    let limited: Vec<_> = all_history.into_iter().rev().take(query.limit).collect();

    (StatusCode::OK, Json(serde_json::to_value(&limited).unwrap_or_default())).into_response()
}

/// GET /api/supervisor/learnings
///
/// Get the accumulated learnings from all orchestration runs.
pub async fn learnings(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let engine = match require_supervisor(&state) {
        Ok(e) => e,
        Err(err) => return err.into_response(),
    };

    let all_learnings = engine.learnings().await;
    let serialized: Vec<serde_json::Value> = all_learnings
        .iter()
        .map(|l| {
            serde_json::json!({
                "category": format!("{:?}", l.category),
                "insight": l.insight,
                "context": l.context,
                "actionable": l.actionable,
                "timestamp": l.timestamp.to_rfc3339(),
            })
        })
        .collect();

    (StatusCode::OK, Json(serde_json::to_value(&serialized).unwrap_or_default())).into_response()
}

/// GET /api/supervisor/config
///
/// Get the current algorithm configuration.
pub async fn get_config(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let engine = match require_supervisor(&state) {
        Ok(e) => e,
        Err(err) => return err.into_response(),
    };

    let config = engine.algorithm_config().await;
    (StatusCode::OK, Json(serde_json::to_value(&config).unwrap_or_default())).into_response()
}

/// PUT /api/supervisor/config
///
/// Update the algorithm configuration.
///
/// Request body: AlgorithmConfig JSON object.
pub async fn update_config(
    State(state): State<Arc<AppState>>,
    Json(config): Json<maestro_algorithm::executor::AlgorithmConfig>,
) -> impl IntoResponse {
    let engine = match require_supervisor(&state) {
        Ok(e) => e,
        Err(err) => return err.into_response(),
    };

    engine.update_config(config.clone()).await;

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "updated",
            "config": serde_json::to_value(&config).unwrap_or_default(),
        })),
    )
        .into_response()
}
