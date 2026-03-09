//! Analytics dashboard API routes.
//!
//! These endpoints expose the FalkorDB graph analytics engine to the
//! dashboard frontend. All routes are prefixed with `/api/analytics/`.
//! When analytics is not configured, every endpoint returns 503 Service Unavailable.

use crate::routes::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns a reference to the analytics engine, or a 503 error if not configured.
fn require_analytics(
    state: &AppState,
) -> Result<&maestro_falkor_analytics::FalkorAnalytics, (StatusCode, Json<serde_json::Value>)> {
    state.kernel.analytics.as_deref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "Analytics engine is not configured. Set [analytics] in config.toml."
            })),
        )
    })
}

/// Standard JSON error response.
fn err_response(status: StatusCode, msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({ "error": msg })))
}

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct EntityIdQuery {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize)]
pub struct AgentMemoriesQuery {
    pub agent_id: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize)]
pub struct ShortestPathQuery {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Deserialize)]
pub struct TopHubsQuery {
    #[serde(default = "default_hub_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize)]
pub struct ConfidenceQuery {
    #[serde(default = "default_confidence")]
    pub min_confidence: f64,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    50
}
fn default_hub_limit() -> usize {
    20
}
fn default_confidence() -> f64 {
    0.8
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

/// `GET /api/analytics/health` — Check FalkorDB connectivity.
pub async fn analytics_health(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let analytics = match require_analytics(&state) {
        Ok(a) => a,
        Err(e) => return e.into_response(),
    };
    match analytics.health_check().await {
        Ok(true) => Json(serde_json::json!({ "status": "ok" })).into_response(),
        Ok(false) => err_response(StatusCode::SERVICE_UNAVAILABLE, "FalkorDB health check returned no results").into_response(),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, &format!("Health check failed: {e}")).into_response(),
    }
}

/// `GET /api/analytics/stats` — Overall graph statistics.
pub async fn graph_stats(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let analytics = match require_analytics(&state) {
        Ok(a) => a,
        Err(e) => return e.into_response(),
    };
    match analytics.graph_stats().await {
        Ok(stats) => Json(serde_json::json!({ "stats": stats })).into_response(),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, &format!("Graph stats failed: {e}")).into_response(),
    }
}

/// `GET /api/analytics/entities/:id` — Get a single entity by ID.
pub async fn get_entity(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let analytics = match require_analytics(&state) {
        Ok(a) => a,
        Err(e) => return e.into_response(),
    };
    match analytics.get_entity(&id).await {
        Ok(Some(node)) => Json(serde_json::json!({ "entity": node })).into_response(),
        Ok(None) => err_response(StatusCode::NOT_FOUND, "Entity not found").into_response(),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, &format!("Get entity failed: {e}")).into_response(),
    }
}

/// `GET /api/analytics/entities/:id/neighbors` — Get neighbors of an entity.
pub async fn entity_neighbors(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let analytics = match require_analytics(&state) {
        Ok(a) => a,
        Err(e) => return e.into_response(),
    };
    match analytics.entity_neighbors(&id).await {
        Ok(neighbors) => Json(serde_json::json!({ "neighbors": neighbors })).into_response(),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, &format!("Neighbor query failed: {e}")).into_response(),
    }
}

/// `GET /api/analytics/entities/search?q=...&limit=50` — Search entities by name.
pub async fn search_entities(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchQuery>,
) -> impl IntoResponse {
    let analytics = match require_analytics(&state) {
        Ok(a) => a,
        Err(e) => return e.into_response(),
    };
    match analytics.search_entities(&params.q, params.limit).await {
        Ok(entities) => Json(serde_json::json!({ "entities": entities })).into_response(),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, &format!("Search failed: {e}")).into_response(),
    }
}

/// `GET /api/analytics/path?source=...&target=...` — Shortest path between two entities.
pub async fn shortest_path(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ShortestPathQuery>,
) -> impl IntoResponse {
    let analytics = match require_analytics(&state) {
        Ok(a) => a,
        Err(e) => return e.into_response(),
    };
    match analytics.shortest_path(&params.source, &params.target).await {
        Ok(path) => Json(serde_json::json!({ "path": path })).into_response(),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, &format!("Shortest path failed: {e}")).into_response(),
    }
}

/// `GET /api/analytics/distribution/entities` — Entity type distribution.
pub async fn entity_type_distribution(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let analytics = match require_analytics(&state) {
        Ok(a) => a,
        Err(e) => return e.into_response(),
    };
    match analytics.entity_type_distribution().await {
        Ok(dist) => Json(serde_json::json!({ "distribution": dist })).into_response(),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, &format!("Distribution query failed: {e}")).into_response(),
    }
}

/// `GET /api/analytics/distribution/relations` — Relation type distribution.
pub async fn relation_type_distribution(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let analytics = match require_analytics(&state) {
        Ok(a) => a,
        Err(e) => return e.into_response(),
    };
    match analytics.relation_type_distribution().await {
        Ok(dist) => Json(serde_json::json!({ "distribution": dist })).into_response(),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, &format!("Distribution query failed: {e}")).into_response(),
    }
}

/// `GET /api/analytics/hubs?limit=20` — Top hub entities by connection count.
pub async fn top_hubs(
    State(state): State<Arc<AppState>>,
    Query(params): Query<TopHubsQuery>,
) -> impl IntoResponse {
    let analytics = match require_analytics(&state) {
        Ok(a) => a,
        Err(e) => return e.into_response(),
    };
    match analytics.top_hubs(params.limit).await {
        Ok(hubs) => Json(serde_json::json!({ "hubs": hubs })).into_response(),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, &format!("Hub query failed: {e}")).into_response(),
    }
}

/// `GET /api/analytics/memories?agent_id=...&limit=50` — Agent memories in the graph.
pub async fn agent_memories(
    State(state): State<Arc<AppState>>,
    Query(params): Query<AgentMemoriesQuery>,
) -> impl IntoResponse {
    let analytics = match require_analytics(&state) {
        Ok(a) => a,
        Err(e) => return e.into_response(),
    };
    match analytics.agent_memories(&params.agent_id, params.limit).await {
        Ok(memories) => Json(serde_json::json!({ "memories": memories })).into_response(),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, &format!("Memory query failed: {e}")).into_response(),
    }
}

/// `GET /api/analytics/memories/stats` — Per-agent memory statistics.
pub async fn agent_memory_stats(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let analytics = match require_analytics(&state) {
        Ok(a) => a,
        Err(e) => return e.into_response(),
    };
    match analytics.agent_memory_stats().await {
        Ok(stats) => Json(serde_json::json!({ "stats": stats })).into_response(),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, &format!("Memory stats failed: {e}")).into_response(),
    }
}

/// `GET /api/analytics/relations/high-confidence?min_confidence=0.8&limit=50` — High-confidence relations.
pub async fn high_confidence_relations(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ConfidenceQuery>,
) -> impl IntoResponse {
    let analytics = match require_analytics(&state) {
        Ok(a) => a,
        Err(e) => return e.into_response(),
    };
    match analytics.high_confidence_relations(params.min_confidence, params.limit).await {
        Ok(edges) => Json(serde_json::json!({ "relations": edges })).into_response(),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, &format!("Relation query failed: {e}")).into_response(),
    }
}

/// `POST /api/analytics/etl/run` — Trigger an ETL run from SurrealDB → FalkorDB.
pub async fn trigger_etl(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let analytics = match require_analytics(&state) {
        Ok(a) => a,
        Err(e) => return e.into_response(),
    };
    let mem: Arc<dyn openfang_types::memory::Memory> = state.kernel.memory.clone();
    let fa = analytics.clone();
    let handle = maestro_falkor_analytics::spawn_etl(mem, fa);
    // Don't await the handle — return immediately, ETL runs in background
    drop(handle);
    Json(serde_json::json!({ "status": "etl_started" })).into_response()
}
