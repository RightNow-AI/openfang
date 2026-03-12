//! Supervisor engine API routes.
//!
//! These endpoints expose the MAESTRO supervisor engine to the dashboard
//! frontend and external integrations. All routes are prefixed with
//! `/api/supervisor/`.
//!
//! When the supervisor engine is not initialized, every endpoint returns
//! 503 Service Unavailable.

use crate::routes::AppState;
use crate::types::DelegateTaskRequest;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use openfang_kernel::supervisor_engine::{OrchestrationId, SupervisorEngine, TaskType};
use serde::{Deserialize, Serialize};
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
    (
        status,
        Json(serde_json::json!({
            "error": msg
        })),
    )
}

// ── Request & Response Types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct OrchestrateRequest {
    /// Task description to orchestrate.
    ///
    /// The supervisor will either delegate to a single agent (if complexity <= threshold_sequential)
    /// or run the full MAESTRO pipeline (complexity > threshold_sequential).
    pub task: String,
    /// Capabilities/LLM parameters required for the task.
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Serialize)]  
pub struct OrchestrateResponse {
    /// ID of the created orchestration run.
    pub id: String,
    /// Final output from the orchestrated agent(s).
    pub output: String,
    /// Overall runtime satisfaction score (0.0-1.0).
    pub satisfaction: f64,
    /// Number of agents spawned during orchestration (0 for single-agent pass-through).
    pub agents_spawned: u32,
    /// True if MAESTRO orchestration was used, false if task was passed through.
    pub orchestrated: bool,
}

#[derive(Debug, Deserialize)]
pub struct StatusQuery {
    /// Include full history in response (expensive operation).
    #[serde(default)]
    pub include_history: bool,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    /// Whether the supervisor is currently running an orchestration.
    pub active: bool,
    /// Current/last orchestration ID (if any).
    pub current_run: Option<String>,
    /// Current phase of the active run (if any).
    pub current_phase: Option<String>,
    /// Total orchestrations completed (since boot).
    pub total_runs: u64,
    /// Number of runs that met satisfaction threshold.
    pub successful_runs: u64,
    /// Average satisfaction score over all runs (0.0-1.0).
    pub avg_satisfaction: f64,
    /// Total learnings accumulated (since boot).
    pub total_learnings: u64,
    /// Recent orchestration summaries (last 10).
    pub recent_runs: Vec<RecentRunSummary>,
}

#[derive(Debug, Serialize)]
pub struct RecentRunSummary {
    pub id: String,
    pub task: String,
    pub complexity: u8,
    pub satisfaction: f64,
    pub duration_ms: u64,
    pub agents_spawned: u32,
    pub completed_at: String,
}

#[derive(Debug, Serialize)]
pub struct ConfigResponse {
    /// Algorithm configuration including phase thresholds.
    // This includes all fields from AlgorithmConfig directly due to flattened structure
    #[serde(flatten)]
    pub config: maestro_algorithm::executor::AlgorithmConfig,
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    /// Maximum number of historical entries to return (default 20).
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    20
}

// ── Route Handlers ───────────────────────────────────────────────────────────

/// GET /api/supervisor/status
///
/// Returns the current status of the supervisor engine including active run,
/// statistics, and recent orchestration history.
pub async fn status(
    State(state): State<Arc<AppState>>,
    Query(query): Query<StatusQuery>,
) -> impl IntoResponse {
    let engine = match require_supervisor(&state) {
        Ok(e) => e,
        Err(err) => return err.into_response(),
    };

    let mut status = engine.status().await;
    
    // Conditionally include full history if requested (expensive)
    if !query.include_history {
        status.recent_runs.truncate(10);
    }
    
    (
        StatusCode::OK,
        Json(serde_json::to_value(&status).unwrap_or_default()),
    )
        .into_response()
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
    Json(request): Json<OrchestrateRequest>,
) -> impl IntoResponse {
    let engine = match require_supervisor(&state) {
        Ok(e) => e.clone(),
        Err(err) => return err.into_response(),
    };

    if request.task.trim().is_empty() {
        return err_response(StatusCode::BAD_REQUEST, "Task description is required")
            .into_response();
    }

    // Run orchestration (blocks until completion - consider streaming for long tasks in future)
    match engine.orchestrate(&request.task, &request.capabilities).await {
        Ok(result) => (
            StatusCode::OK,
            Json(serde_json::to_value(&OrchestrateResponse {
                id: result.id.to_string(),
                output: result.output,
                satisfaction: result.satisfaction,
                agents_spawned: result.agents_spawned,
                orchestrated: result.orchestrated,
            })
            .unwrap_or_default()),
        )
            .into_response(),
        Err(e) => {
            err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Orchestration failed: {e}"),
            )
                .into_response()
        }
    }
}

/// POST /api/supervisor/delegate
/// 
/// Explicitly delegate a task to the appropriate agent engine based on content.
/// Unlike orchestrate which uses MAESTRO, this uses smart routing to direct agents
/// like the SWE agent.
pub async fn delegate_to_supervisor(
    State(state): State<Arc<AppState>>,
    Json(request): Json<DelegateTaskRequest>,
) -> impl IntoResponse {
    let engine = match require_supervisor(&state) {
        Ok(e) => e.clone(),
        Err(err) => return err.into_response(),
    };

    if request.description.trim().is_empty() {
        return err_response(StatusCode::BAD_REQUEST, "Description is required")
            .into_response();
    }

    // Classify the task type to determine routing
    if let TaskType::SWE = engine.classify_task(&request.description, &request.capabilities).await {
        // Direct to SWE agent
        let result = engine.delegate_swe_task(&request.description, request.description.clone()).await;
        match result {
            Ok(orch_result) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "task_type": "swe",
                    "task_id": orch_result.id.to_string(),
                    "output": orch_result.output,
                    "status": "delegated_to_swe"
                }))
            ).into_response(),
            Err(e) => {
                err_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("SWE delegation failed: {e}"),
                )
                    .into_response()
            }
        }
    } else {
        // Otherwise route to general MAESTRO orchestration as fallback
        match engine.orchestrate(&request.description, &request.capabilities).await {
            Ok(result) => (
                StatusCode::OK,
                Json(serde_json::to_value(&OrchestrateResponse {
                    id: result.id.to_string(),
                    output: result.output,
                    satisfaction: result.satisfaction,
                    agents_spawned: result.agents_spawned,
                    orchestrated: result.orchestrated,
                })
                .unwrap_or_default()),
            )
                .into_response(),
            Err(e) => {
                err_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("Orchestration failed: {e}"),
                )
                    .into_response()
            }
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

    let id = OrchestrationId(uuid);

    match engine.get_run(id).await {
        Some(result) => (
            StatusCode::OK,
            Json(serde_json::to_value(&result).unwrap_or_default()),
        )
            .into_response(),
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

    (
        StatusCode::OK,
        Json(serde_json::to_value(&limited).unwrap_or_default()),
    )
        .into_response()
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

    (
        StatusCode::OK,
        Json(serde_json::to_value(&serialized).unwrap_or_default()),
    )
        .into_response()
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
    (
        StatusCode::OK,
        Json(serde_json::to_value(&config).unwrap_or_default()),
    )
        .into_response()
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