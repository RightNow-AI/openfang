//! OpenFang daemon server — boots the kernel and serves the HTTP API.

use crate::channel_bridge;
use crate::api_response::ApiError;
use crate::middleware;
use crate::rate_limiter;
use crate::request_context::RequestId;
use crate::routes::{self, AppState};
use crate::webchat;
use crate::ws;
use axum::{extract::OriginalUri, http::Method, response::IntoResponse, Extension, Router};
use openfang_kernel::OpenFangKernel;
use openfang_orchestrator::{
    support_triage_workflow, InMemoryWorkflowStore, MockWorkflowExecutor, WorkflowEngine,
};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

fn request_id_from_extension(request_id: Option<Extension<RequestId>>) -> Option<String> {
    request_id.map(|Extension(id)| id.0)
}

async fn not_found_handler(
    method: Method,
    uri: OriginalUri,
    request_id: Option<Extension<RequestId>>,
) -> impl axum::response::IntoResponse {
    let request_id = request_id_from_extension(request_id);
    tracing::warn!(
        route = uri.0.path(),
        method = %method,
        request_id = request_id.as_deref().unwrap_or(""),
        "api route not found"
    );
    ApiError::not_found("Route not found", request_id)
}

/// Outer wildcard handler for `/{*path}` registered on the outer router.
///
/// OPTIONS → 204 (CORS preflight; the outer `cors2` layer adds headers).  
/// Any other method on an unregistered path → 404 JSON envelope (ensures
/// unknown paths return 404, not 405, even though `/{*path}` matches their
/// path segment).
async fn outer_catch_all(
    method: Method,
    uri: OriginalUri,
) -> axum::response::Response {
    if method == Method::OPTIONS {
        return axum::http::StatusCode::NO_CONTENT.into_response();
    }
    tracing::debug!(route = uri.0.path(), "outer wildcard: route not found");
    ApiError::not_found("Route not found", None).into_response()
}

/// Daemon info written to `~/.openfang/daemon.json` so the CLI can find us.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct DaemonInfo {
    pub pid: u32,
    pub listen_addr: String,
    pub started_at: String,
    pub version: String,
    pub platform: String,
}

/// Build the full API router with all routes, middleware, and state.
///
/// This is extracted from `run_daemon()` so that embedders (e.g. openfang-desktop)
/// can create the router without starting the full daemon lifecycle.
///
/// Returns `(router, shared_state)`. The caller can use `state.bridge_manager`
/// to shut down the bridge on exit.
pub async fn build_router(
    kernel: Arc<OpenFangKernel>,
    listen_addr: SocketAddr,
) -> (Router<()>, Arc<AppState>) {
    // Start channel bridges (Telegram, etc.)
    let bridge = channel_bridge::start_channel_bridge(kernel.clone()).await;
    let orchestrator = Arc::new(WorkflowEngine::new(
        Arc::new(InMemoryWorkflowStore::new()),
        Arc::new(MockWorkflowExecutor),
    ));
    orchestrator
        .register_definition(support_triage_workflow())
        .await
        .expect("support-triage workflow should register");

    // Seed bundled agency fixture profiles (idempotent — skips existing).
    let _ = kernel.memory.agency_seed_fixtures();

    let channels_config = kernel.config.channels.clone();
    let user_rate_limiter = rate_limiter::create_user_rate_limiter();
    let state = Arc::new(AppState {
        kernel: kernel.clone(),
        orchestrator,
        local_capabilities: tokio::sync::RwLock::new(None),
        local_status: tokio::sync::RwLock::new(crate::local_inference::LocalModelStatus::default()),
        started_at: Instant::now(),
        peer_registry: kernel.peer_registry.as_ref().map(|r| Arc::new(r.clone())),
        bridge_manager: tokio::sync::Mutex::new(bridge),
        channels_config: tokio::sync::RwLock::new(channels_config),
        shutdown_notify: Arc::new(tokio::sync::Notify::new()),
        clawhub_cache: dashmap::DashMap::new(),
        provider_probe_cache: openfang_runtime::provider_health::ProbeCache::new(),
        user_rate_limiter,
        orchestrator_runs: tokio::sync::RwLock::new(Vec::new()),
    });

    // H1: Restrict CORS to explicit headers and methods rather than Any.
    // Allowing Any headers and methods would permit cross-origin requests with
    // credentials, custom auth headers, and arbitrary HTTP methods.
    let allowed_headers = vec![
        axum::http::header::AUTHORIZATION,
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderName::from_static("x-api-key"),
        axum::http::HeaderName::from_static("x-request-id"),
    ];
    let allowed_methods = vec![
        axum::http::Method::GET,
        axum::http::Method::POST,
        axum::http::Method::PUT,
        axum::http::Method::PATCH,
        axum::http::Method::DELETE,
        axum::http::Method::OPTIONS,
    ];
    let cors_origins: Vec<axum::http::HeaderValue> = if state.kernel.config.api_key.is_empty() {
        // No auth → restrict CORS to localhost origins (include both 127.0.0.1 and localhost)
        let port = listen_addr.port();
        let mut origins: Vec<axum::http::HeaderValue> = vec![
            format!("http://{listen_addr}").parse().unwrap(),
            format!("http://localhost:{port}").parse().unwrap(),
        ];
        // Also allow common dev ports (3002 = Next.js primary frontend)
        for p in [3000u16, 3002, 8080] {
            if p != port {
                if let Ok(v) = format!("http://127.0.0.1:{p}").parse() {
                    origins.push(v);
                }
                if let Ok(v) = format!("http://localhost:{p}").parse() {
                    origins.push(v);
                }
            }
        }
        origins
    } else {
        // Auth enabled → restrict CORS to localhost + configured origins.
        let mut origins: Vec<axum::http::HeaderValue> = vec![
            format!("http://{listen_addr}").parse().unwrap(),
            "http://localhost:50051".parse().unwrap(),
            "http://127.0.0.1:50051".parse().unwrap(),
            "http://localhost:8080".parse().unwrap(),
            "http://127.0.0.1:8080".parse().unwrap(),
        ];
        // Merge explicit ENV allowlist (Playbook Rule 11: Production Domains)
        if let Ok(env_cors) = std::env::var("OPENFANG_CORS_ORIGINS") {
            for o in env_cors.split(',') {
                if let Ok(v) = o.trim().parse() {
                    origins.push(v);
                }
            }
        }
        // Add the actual listen address variants
        if listen_addr.port() != 50051 && listen_addr.port() != 8080 {
            if let Ok(v) = format!("http://localhost:{}", listen_addr.port()).parse() {
                origins.push(v);
            }
            if let Ok(v) = format!("http://127.0.0.1:{}", listen_addr.port()).parse() {
                origins.push(v);
            }
        }
        origins
    };
    // Build identical CorsLayer instances — one for the inner router and one for the
    // outer OPTIONS preflight handler (tower-http 0.6 CorsLayer does not impl Clone).
    // max_age lets browsers cache preflight results for up to 24 h, reducing round-trips.
    let make_cors = |origins: Vec<axum::http::HeaderValue>| {
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods(allowed_methods.clone())
            .allow_headers(allowed_headers.clone())
            .max_age(std::time::Duration::from_secs(86400))
    };
    let cors  = make_cors(cors_origins.clone());
    let cors2 = make_cors(cors_origins);

    let gcra_limiter = rate_limiter::create_rate_limiter();

    // OpenAPI spec handler — served unauthenticated (schema is public contract)
    let openapi_json = {
        use utoipa::OpenApi as _;
        crate::openapi::ApiDoc::openapi()
    };

    let app = Router::new()
        .route("/", axum::routing::get(webchat::webchat_page))
        .route("/logo.png", axum::routing::get(webchat::logo_png))
        .route("/favicon.ico", axum::routing::get(webchat::favicon_ico))
        .route(
            "/api-doc/openapi.json",
            axum::routing::get({
                let spec = openapi_json.clone();
                move || async move { axum::Json(spec) }
            }),
        )
        .route(
            "/api/metrics",
            axum::routing::get(routes::prometheus_metrics),
        )
        .route("/api/health", axum::routing::get(routes::health))
        .route(
            "/api/health/detail",
            axum::routing::get(routes::health_detail),
        )
        .route("/api/status", axum::routing::get(routes::status))
        .route("/api/version", axum::routing::get(routes::version))
        .route(
            "/api/agents",
            axum::routing::get(routes::list_agents).post(routes::spawn_agent),
        )
        .route(
            "/api/agents/catalog",
            axum::routing::get(routes::list_agent_catalog),
        )
        .route(
            "/api/agents/catalog/{id}/enabled",
            axum::routing::put(routes::update_agent_catalog),
        )
        .route(
            "/api/agents/{id}",
            axum::routing::get(routes::get_agent).delete(routes::kill_agent).patch(routes::patch_agent),
        )
        .route(
            "/api/agents/{id}/mode",
            axum::routing::put(routes::set_agent_mode),
        )
        .route("/api/profiles", axum::routing::get(routes::list_profiles))
        .route(
            "/api/agents/{id}/message",
            axum::routing::post(routes::send_message),
        )
        .route(
            "/api/agents/{id}/message/stream",
            axum::routing::post(routes::send_message_stream),
        )
        .route(
            "/api/agents/{id}/session",
            axum::routing::get(routes::get_agent_session),
        )
        .route(
            "/api/agents/{id}/sessions",
            axum::routing::get(routes::list_agent_sessions).post(routes::create_agent_session),
        )
        .route(
            "/api/agents/{id}/sessions/{session_id}/switch",
            axum::routing::post(routes::switch_agent_session),
        )
        .route(
            "/api/planner/inbox",
            axum::routing::get(routes::list_planner_inbox)
                .post(routes::create_planner_inbox_item),
        )
        .route(
            "/api/planner/clarify",
            axum::routing::post(routes::clarify_planner_inbox_item),
        )
        .route(
            "/api/planner/today",
            axum::routing::get(routes::get_planner_today),
        )
        .route(
            "/api/planner/today/rebuild",
            axum::routing::post(routes::rebuild_planner_today),
        )
        .route(
            "/api/planner/agents",
            axum::routing::get(routes::list_planner_agents),
        )
        .route(
            "/api/planner/agents/{id}",
            axum::routing::put(routes::update_planner_agent),
        )
        .route(
            "/api/agency/import",
            axum::routing::post(routes::import_agency_profile),
        )
        .route(
            "/api/agency/fixtures",
            axum::routing::post(routes::seed_agency_fixtures),
        )
        .route(
            "/api/agency/profiles",
            axum::routing::get(routes::list_agency_profiles),
        )
        .route(
            "/api/agency/profiles/{id}",
            axum::routing::get(routes::get_agency_profile),
        )
        .route(
            "/api/agency/profiles/{id}/enabled",
            axum::routing::put(routes::update_agency_profile_enabled),
        )
        .route(
            "/api/agency/roster",
            axum::routing::get(routes::get_agency_roster),
        )
        .route(
            "/api/agency/route",
            axum::routing::post(routes::route_agency_task),
        )
        .route(
            "/api/local/policy",
            axum::routing::get(routes::get_local_policy),
        )
        .route(
            "/api/local/status",
            axum::routing::get(routes::get_local_status),
        )
        .route(
            "/api/local/capabilities",
            axum::routing::post(routes::post_local_capabilities),
        )
        .route(
            "/api/local/planner/recommend",
            axum::routing::post(routes::post_local_planner_recommend),
        )
        .route(
            "/api/local/planner/split",
            axum::routing::post(routes::post_local_planner_split),
        )
        .route(
            "/api/local/planner/translate",
            axum::routing::post(routes::post_local_planner_translate),
        )
        .route(
            "/api/local/harness",
            axum::routing::get(routes::get_local_comparison_harness),
        )
        .route(
            "/api/workflows/start",
            axum::routing::post(routes::start_orchestrated_workflow),
        )
        .route(
            "/api/workflows/{run_id}/resume",
            axum::routing::post(routes::resume_orchestrated_workflow),
        )
        .route(
            "/api/workflows/{run_id}",
            axum::routing::get(routes::get_orchestrated_workflow_run),
        )
        .route(
            "/api/agents/{id}/session/reset",
            axum::routing::post(routes::reset_session),
        )
        .route(
            "/api/agents/{id}/history",
            axum::routing::delete(routes::clear_agent_history),
        )
        .route(
            "/api/agents/{id}/session/compact",
            axum::routing::post(routes::compact_session),
        )
        .route(
            "/api/agents/{id}/stop",
            axum::routing::post(routes::stop_agent),
        )
        .route(
            "/api/agents/{id}/model",
            axum::routing::put(routes::set_model),
        )
        .route(
            "/api/agents/{id}/tools",
            axum::routing::get(routes::get_agent_tools).put(routes::set_agent_tools),
        )
        .route(
            "/api/agents/{id}/skills",
            axum::routing::get(routes::get_agent_skills).put(routes::set_agent_skills),
        )
        .route(
            "/api/agents/{id}/mcp_servers",
            axum::routing::get(routes::get_agent_mcp_servers).put(routes::set_agent_mcp_servers),
        )
        .route(
            "/api/agents/{id}/identity",
            axum::routing::patch(routes::update_agent_identity),
        )
        .route(
            "/api/agents/{id}/config",
            axum::routing::patch(routes::patch_agent_config),
        )
        .route(
            "/api/agents/{id}/clone",
            axum::routing::post(routes::clone_agent),
        )
        .route(
            "/api/agents/{id}/files",
            axum::routing::get(routes::list_agent_files),
        )
        .route(
            "/api/agents/{id}/files/{filename}",
            axum::routing::get(routes::get_agent_file).put(routes::set_agent_file),
        )
        .route(
            "/api/agents/{id}/deliveries",
            axum::routing::get(routes::get_agent_deliveries),
        )
        .route(
            "/api/agents/{id}/upload",
            axum::routing::post(routes::upload_file),
        )
        .route("/api/agents/{id}/ws", axum::routing::get(ws::agent_ws))
        // Upload serving
        .route(
            "/api/uploads/{file_id}",
            axum::routing::get(routes::serve_upload),
        )
        // Channel endpoints
        .route("/api/channels", axum::routing::get(routes::list_channels))
        .route(
            "/api/channels/{name}/configure",
            axum::routing::post(routes::configure_channel).delete(routes::remove_channel),
        )
        .route(
            "/api/channels/{name}/test",
            axum::routing::post(routes::test_channel),
        )
        .route(
            "/api/channels/reload",
            axum::routing::post(routes::reload_channels),
        )
        // WhatsApp QR login flow
        .route(
            "/api/channels/whatsapp/qr/start",
            axum::routing::post(routes::whatsapp_qr_start),
        )
        .route(
            "/api/channels/whatsapp/qr/status",
            axum::routing::get(routes::whatsapp_qr_status),
        )
        // Template endpoints
        .route("/api/templates", axum::routing::get(routes::list_templates))
        .route(
            "/api/templates/{name}",
            axum::routing::get(routes::get_template),
        )
        // Memory endpoints
        .route(
            "/api/memory/agents/{id}/kv",
            axum::routing::get(routes::get_agent_kv),
        )
        .route(
            "/api/memory/agents/{id}/kv/{key}",
            axum::routing::get(routes::get_agent_kv_key)
                .put(routes::set_agent_kv_key)
                .delete(routes::delete_agent_kv_key),
        )
        // Trigger endpoints
        .route(
            "/api/triggers",
            axum::routing::get(routes::list_triggers).post(routes::create_trigger),
        )
        .route(
            "/api/triggers/{id}",
            axum::routing::delete(routes::delete_trigger).put(routes::update_trigger),
        )
        // Schedule (cron job) endpoints
        .route(
            "/api/schedules",
            axum::routing::get(routes::list_schedules).post(routes::create_schedule),
        )
        .route(
            "/api/schedules/{id}",
            axum::routing::delete(routes::delete_schedule).put(routes::update_schedule),
        )
        .route(
            "/api/schedules/{id}/run",
            axum::routing::post(routes::run_schedule),
        )
        // Workflow endpoints
        .route(
            "/api/workflows",
            axum::routing::get(routes::list_workflows).post(routes::create_workflow),
        )
        .route(
            "/api/workflows/{id}/run",
            axum::routing::post(routes::run_workflow),
        )
        .route(
            "/api/workflows/{id}/runs",
            axum::routing::get(routes::list_workflow_runs),
        )
        // Skills endpoints
        .route("/api/skills", axum::routing::get(routes::list_skills))
        .route(
            "/api/skills/install",
            axum::routing::post(routes::install_skill),
        )
        .route(
            "/api/skills/uninstall",
            axum::routing::post(routes::uninstall_skill),
        )
        .route(
            "/api/marketplace/search",
            axum::routing::get(routes::marketplace_search),
        )
        // ClawHub (OpenClaw ecosystem) endpoints
        .route(
            "/api/clawhub/search",
            axum::routing::get(routes::clawhub_search),
        )
        .route(
            "/api/clawhub/browse",
            axum::routing::get(routes::clawhub_browse),
        )
        .route(
            "/api/clawhub/skill/{slug}",
            axum::routing::get(routes::clawhub_skill_detail),
        )
        .route(
            "/api/clawhub/skill/{slug}/code",
            axum::routing::get(routes::clawhub_skill_code),
        )
        .route(
            "/api/clawhub/install",
            axum::routing::post(routes::clawhub_install),
        )
        // Hands endpoints
        .route("/api/hands", axum::routing::get(routes::list_hands))
        .route(
            "/api/hands/install",
            axum::routing::post(routes::install_hand),
        )
        .route(
            "/api/hands/active",
            axum::routing::get(routes::list_active_hands),
        )
        .route("/api/hands/{hand_id}", axum::routing::get(routes::get_hand))
        .route(
            "/api/hands/{hand_id}/activate",
            axum::routing::post(routes::activate_hand),
        )
        .route(
            "/api/hands/{hand_id}/check-deps",
            axum::routing::post(routes::check_hand_deps),
        )
        .route(
            "/api/hands/{hand_id}/install-deps",
            axum::routing::post(routes::install_hand_deps),
        )
        .route(
            "/api/hands/{hand_id}/settings",
            axum::routing::get(routes::get_hand_settings)
                .put(routes::update_hand_settings),
        )
        .route(
            "/api/hands/instances/{id}/pause",
            axum::routing::post(routes::pause_hand),
        )
        .route(
            "/api/hands/instances/{id}/resume",
            axum::routing::post(routes::resume_hand),
        )
        .route(
            "/api/hands/instances/{id}",
            axum::routing::delete(routes::deactivate_hand),
        )
        .route(
            "/api/hands/instances/{id}/stats",
            axum::routing::get(routes::hand_stats),
        )
        .route(
            "/api/hands/instances/{id}/browser",
            axum::routing::get(routes::hand_instance_browser),
        )
        // MCP server endpoints
        .route(
            "/api/mcp/servers",
            axum::routing::get(routes::list_mcp_servers),
        )
        // Audit endpoints
        .route(
            "/api/audit/recent",
            axum::routing::get(routes::audit_recent),
        )
        .route(
            "/api/audit/verify",
            axum::routing::get(routes::audit_verify),
        )
        // Live log streaming (SSE)
        .route("/api/logs/stream", axum::routing::get(routes::logs_stream))
        // Peer/Network endpoints
        .route("/api/peers", axum::routing::get(routes::list_peers))
        .route(
            "/api/network/status",
            axum::routing::get(routes::network_status),
        )
        // Agent communication (Comms) endpoints
        .route(
            "/api/comms/topology",
            axum::routing::get(routes::comms_topology),
        )
        .route(
            "/api/comms/events",
            axum::routing::get(routes::comms_events),
        )
        .route(
            "/api/comms/events/stream",
            axum::routing::get(routes::comms_events_stream),
        )
        .route(
            "/api/comms/send",
            axum::routing::post(routes::comms_send),
        )
        .route(
            "/api/comms/task",
            axum::routing::post(routes::comms_task),
        )
        // Tools endpoint
        .route("/api/tools", axum::routing::get(routes::list_tools))
        // Config endpoints
        .route("/api/config", axum::routing::get(routes::get_config))
        .route(
            "/api/config/schema",
            axum::routing::get(routes::config_schema),
        )
        .route("/api/config/set", axum::routing::post(routes::config_set))
        // Approval endpoints
        .route(
            "/api/approvals",
            axum::routing::get(routes::list_approvals).post(routes::create_approval),
        )
        .route(
            "/api/approvals/{id}/approve",
            axum::routing::post(routes::approve_request),
        )
        .route(
            "/api/approvals/{id}/reject",
            axum::routing::post(routes::reject_request),
        )
        // Usage endpoints
        .route("/api/usage", axum::routing::get(routes::usage_stats))
        .route(
            "/api/usage/summary",
            axum::routing::get(routes::usage_summary),
        )
        .route(
            "/api/usage/by-model",
            axum::routing::get(routes::usage_by_model),
        )
        .route("/api/usage/daily", axum::routing::get(routes::usage_daily))
        // Budget endpoints
        .route(
            "/api/budget",
            axum::routing::get(routes::budget_status).put(routes::update_budget),
        )
        .route(
            "/api/budget/agents",
            axum::routing::get(routes::agent_budget_ranking),
        )
        .route(
            "/api/budget/agents/{id}",
            axum::routing::get(routes::agent_budget_status)
                .put(routes::update_agent_budget),
        )
        // Session endpoints
        .route("/api/sessions", axum::routing::get(routes::list_sessions))
        .route(
            "/api/sessions/{id}",
            axum::routing::delete(routes::delete_session),
        )
        .route(
            "/api/sessions/{id}/label",
            axum::routing::put(routes::set_session_label),
        )
        .route(
            "/api/agents/{id}/sessions/by-label/{label}",
            axum::routing::get(routes::find_session_by_label),
        )
        // Agent update
        .route(
            "/api/agents/{id}/update",
            axum::routing::put(routes::update_agent),
        )
        // Security dashboard endpoint
        .route("/api/security", axum::routing::get(routes::security_status))
        // Model catalog endpoints
        .route("/api/models", axum::routing::get(routes::list_models))
        .route(
            "/api/models/aliases",
            axum::routing::get(routes::list_aliases),
        )
        .route(
            "/api/models/custom",
            axum::routing::post(routes::add_custom_model),
        )
        .route(
            "/api/models/custom/{*id}",
            axum::routing::delete(routes::remove_custom_model),
        )
        .route("/api/models/{*id}", axum::routing::get(routes::get_model))
        .route("/api/providers", axum::routing::get(routes::list_providers))
        .route("/api/auth/status", axum::routing::get(routes::auth_status))
        .route("/api/auth/me", axum::routing::get(routes::auth_me))
        .route("/api/auth/logout", axum::routing::post(routes::auth_logout))
        .route(
            "/api/auth/github/start",
            axum::routing::post(routes::auth_github_start),
        )
        .route(
            "/api/auth/github/poll/{poll_id}",
            axum::routing::get(routes::auth_github_poll),
        )
        // Copilot OAuth (must be before parametric {name} routes)
        .route(
            "/api/providers/github-copilot/oauth/start",
            axum::routing::post(routes::copilot_oauth_start),
        )
        .route(
            "/api/providers/github-copilot/oauth/poll/{poll_id}",
            axum::routing::get(routes::copilot_oauth_poll),
        )
        .route(
            "/api/providers/{name}/key",
            axum::routing::post(routes::set_provider_key).delete(routes::delete_provider_key),
        )
        .route(
            "/api/providers/{name}/test",
            axum::routing::post(routes::test_provider),
        )
        .route(
            "/api/providers/{name}/url",
            axum::routing::put(routes::set_provider_url),
        )
        .route(
            "/api/settings/providers/current",
            axum::routing::get(routes::get_current_provider)
                .put(routes::save_provider_settings),
        )
        .route(
            "/api/skills/create",
            axum::routing::post(routes::create_skill),
        )
        // Migration endpoints
        .route(
            "/api/migrate/detect",
            axum::routing::get(routes::migrate_detect),
        )
        .route(
            "/api/migrate/scan",
            axum::routing::post(routes::migrate_scan),
        )
        .route("/api/migrate", axum::routing::post(routes::run_migrate))
        // Cron job management endpoints
        .route(
            "/api/cron/jobs",
            axum::routing::get(routes::list_cron_jobs).post(routes::create_cron_job),
        )
        .route(
            "/api/cron/jobs/{id}",
            axum::routing::delete(routes::delete_cron_job),
        )
        .route(
            "/api/cron/jobs/{id}/enable",
            axum::routing::put(routes::toggle_cron_job),
        )
        .route(
            "/api/cron/jobs/{id}/status",
            axum::routing::get(routes::cron_job_status),
        )
        // Webhook trigger endpoints (external event injection)
        .route("/hooks/wake", axum::routing::post(routes::webhook_wake))
        .route("/hooks/agent", axum::routing::post(routes::webhook_agent))
        .route("/api/shutdown", axum::routing::post(routes::shutdown))
        // Chat commands endpoint (dynamic slash menu)
        .route("/api/commands", axum::routing::get(routes::list_commands))
        // Config reload endpoint
        .route(
            "/api/config/reload",
            axum::routing::post(routes::config_reload),
        )
        // Agent binding routes
        .route(
            "/api/bindings",
            axum::routing::get(routes::list_bindings).post(routes::add_binding),
        )
        .route(
            "/api/bindings/{index}",
            axum::routing::delete(routes::remove_binding),
        )
        // A2A (Agent-to-Agent) Protocol endpoints
        .route(
            "/.well-known/agent.json",
            axum::routing::get(routes::a2a_agent_card),
        )
        .route("/a2a/agents", axum::routing::get(routes::a2a_list_agents))
        .route(
            "/a2a/tasks/send",
            axum::routing::post(routes::a2a_send_task),
        )
        .route("/a2a/tasks/{id}", axum::routing::get(routes::a2a_get_task))
        .route(
            "/a2a/tasks/{id}/cancel",
            axum::routing::post(routes::a2a_cancel_task),
        )
        // A2A management (outbound) endpoints
        .route(
            "/api/a2a/agents",
            axum::routing::get(routes::a2a_list_external_agents),
        )
        .route(
            "/api/a2a/discover",
            axum::routing::post(routes::a2a_discover_external),
        )
        .route(
            "/api/a2a/send",
            axum::routing::post(routes::a2a_send_external),
        )
        .route(
            "/api/a2a/tasks/{id}/status",
            axum::routing::get(routes::a2a_external_task_status),
        )
        // Integration management endpoints
        .route(
            "/api/integrations",
            axum::routing::get(routes::list_integrations),
        )
        .route(
            "/api/integrations/available",
            axum::routing::get(routes::list_available_integrations),
        )
        .route(
            "/api/integrations/add",
            axum::routing::post(routes::add_integration),
        )
        .route(
            "/api/integrations/{id}",
            axum::routing::delete(routes::remove_integration),
        )
        .route(
            "/api/integrations/{id}/reconnect",
            axum::routing::post(routes::reconnect_integration),
        )
        .route(
            "/api/integrations/health",
            axum::routing::get(routes::integrations_health),
        )
        .route(
            "/api/integrations/reload",
            axum::routing::post(routes::reload_integrations),
        )
        // Device pairing endpoints
        .route(
            "/api/pairing/request",
            axum::routing::post(routes::pairing_request),
        )
        .route(
            "/api/pairing/complete",
            axum::routing::post(routes::pairing_complete),
        )
        .route(
            "/api/pairing/devices",
            axum::routing::get(routes::pairing_devices),
        )
        .route(
            "/api/pairing/devices/{id}",
            axum::routing::delete(routes::pairing_remove_device),
        )
        .route(
            "/api/pairing/notify",
            axum::routing::post(routes::pairing_notify),
        )
        // WorkItem endpoints
        .route(
            "/api/work",
            axum::routing::get(routes::list_work_items).post(routes::create_work_item),
        )
        // summary must be before /{id} to avoid "summary" matching as an ID
        .route(
            "/api/work/summary",
            axum::routing::get(routes::get_work_summary),
        )
        .route(
            "/api/work/{id}",
            axum::routing::get(routes::get_work_item),
        )
        .route(
            "/api/work/{id}/run",
            axum::routing::post(routes::run_work_item),
        )
        .route(
            "/api/work/{id}/approve",
            axum::routing::post(routes::approve_work_item),
        )
        .route(
            "/api/work/{id}/reject",
            axum::routing::post(routes::reject_work_item),
        )
        .route(
            "/api/work/{id}/cancel",
            axum::routing::post(routes::cancel_work_item),
        )
        .route(
            "/api/work/{id}/retry",
            axum::routing::post(routes::retry_work_item),
        )
        .route(
            "/api/work/{id}/events",
            axum::routing::get(routes::list_work_events),
        )
        .route(
            "/api/work/{id}/delegate",
            axum::routing::post(routes::delegate_work_item),
        )
        // Swarm manifest and planning endpoints
        .route(
            "/api/agents/{id}/manifest",
            axum::routing::get(routes::get_agent_swarm_manifest),
        )
        .route(
            "/api/agents/{id}/manifest/validate",
            axum::routing::get(routes::validate_agent_swarm_manifest),
        )
        .route(
            "/api/swarm/plan/{work_id}",
            axum::routing::get(routes::get_swarm_plan),
        )
        .route(
            "/api/work/{id}/swarm",
            axum::routing::get(routes::get_work_swarm),
        )
        .route(
            "/api/swarm/agents",
            axum::routing::get(routes::list_swarm_agents),
        )
        // Orchestrator endpoints
        .route(
            "/api/orchestrator/status",
            axum::routing::get(routes::get_orchestrator_status),
        )
        .route(
            "/api/orchestrator/runs",
            axum::routing::get(routes::get_orchestrator_runs),
        )
        .route(
            "/api/orchestrator/heartbeat",
            axum::routing::post(routes::post_orchestrator_heartbeat),
        )
        // Research / Autoresearch inspection endpoints
        .route(
            "/api/research/control-plane",
            axum::routing::get(routes::get_research_control_plane)
                .post(routes::put_research_control_plane),
        )
        .route(
            "/api/research/experiments",
            axum::routing::get(routes::list_research_experiments)
                .post(routes::run_research_experiment),
        )
        .route(
            "/api/research/experiments/{id}",
            axum::routing::get(routes::get_research_experiment),
        )
        .route(
            "/api/research/patterns",
            axum::routing::get(routes::list_research_patterns),
        )
        // MCP HTTP endpoint (exposes MCP protocol over HTTP)
        .route("/mcp", axum::routing::post(routes::mcp_http))
        // OpenAI-compatible API
        .route(
            "/v1/chat/completions",
            axum::routing::post(crate::openai_compat::chat_completions),
        )
        .route(
            "/v1/models",
            axum::routing::get(crate::openai_compat::list_models),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::auth,
        ))
        .layer(axum::middleware::from_fn_with_state(
            gcra_limiter,
            rate_limiter::gcra_rate_limit,
        ))
        // C5: Per-user rate limit applied after auth so we have the AuthPrincipal.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::user_rate_limit,
        ))
        .layer(axum::middleware::from_fn(
            middleware::normalize_empty_error_responses,
        ))
        .layer(axum::middleware::from_fn(middleware::security_headers))
        .layer(axum::middleware::from_fn(middleware::request_logging))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        // No custom method_not_allowed_fallback: axum's default 405 includes the
        // Allow header, which normalize_empty_error_responses then preserves when
        // it wraps the response in the standard JSON error envelope.
        .fallback(not_found_handler)
        .with_state(state.clone());

    // Outer router: intercepts OPTIONS preflights for ALL channel + API paths
    // before axum routing can fire. Routes registered with `any(outer_catch_all)`
    // on `/{*path}`; axum prefers more-specific inner_app routes, so real paths
    // continue to use their full middleware stack (normalize, cors, auth, etc.).
    // Only unknown paths hit outer_catch_all (OPTIONS → 204, other → 404 JSON).
    // A separate explicit `OPTIONS /` handler covers the root path since
    // `/{*path}` requires at least one non-empty segment.
    let app = axum::Router::new()
        .route("/{*path}", axum::routing::any(outer_catch_all))
        .route("/", axum::routing::options(|| async { axum::http::StatusCode::NO_CONTENT }))
        .merge(app)
        .layer(cors2);

    (app, state)
}

/// Start the OpenFang daemon: boot kernel + HTTP API server.
///
/// This function blocks until Ctrl+C or a shutdown request.
pub async fn run_daemon(
    kernel: OpenFangKernel,
    listen_addr: &str,
    daemon_info_path: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let addr: SocketAddr = listen_addr.parse()?;

    let kernel = Arc::new(kernel);
    kernel.set_self_handle();
    kernel.start_background_agents();

    // Config file hot-reload watcher (polls every 30 seconds)
    {
        let k = kernel.clone();
        let config_path = kernel.config.home_dir.join("config.toml");
        tokio::spawn(async move {
            let mut last_modified = std::fs::metadata(&config_path)
                .and_then(|m| m.modified())
                .ok();
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                let current = std::fs::metadata(&config_path)
                    .and_then(|m| m.modified())
                    .ok();
                if current != last_modified && current.is_some() {
                    last_modified = current;
                    tracing::info!("Config file changed, reloading...");
                    match k.reload_config() {
                        Ok(plan) => {
                            if plan.has_changes() {
                                tracing::info!("Config hot-reload applied: {:?}", plan.hot_actions);
                            } else {
                                tracing::debug!("Config hot-reload: no actionable changes");
                            }
                        }
                        Err(e) => tracing::warn!("Config hot-reload failed: {e}"),
                    }
                }
            }
        });
    }

    let (app, state) = build_router(kernel.clone(), addr).await;

    // C1: Warn loudly when the daemon is listening on a non-loopback interface
    // without any authentication. This means any machine on the network can
    // query and control the AI agents without credentials.
    if state.kernel.config.api_key.is_empty() && !addr.ip().is_loopback() {
        tracing::error!(
            listen_addr = %addr,
            "SECURITY WARNING: OpenFang is listening on a non-loopback interface \
             with NO authentication configured. Any host on the network can access \
             your agents, memories, and configuration. Set `api_key` in \
             ~/.openfang/config.toml or bind to 127.0.0.1 only."
        );
    }
    if let Some(info_path) = daemon_info_path {
        // Check if another daemon is already running with this PID file
        if info_path.exists() {
            if let Ok(existing) = std::fs::read_to_string(info_path) {
                if let Ok(info) = serde_json::from_str::<DaemonInfo>(&existing) {
                    // PID alive AND the health endpoint responds → truly running
                    if is_process_alive(info.pid) && is_daemon_responding(&info.listen_addr) {
                        return Err(format!(
                            "Another daemon (PID {}) is already running at {}",
                            info.pid, info.listen_addr
                        )
                        .into());
                    }
                }
            }
            // Stale PID file (process dead or different process reused PID), remove it
            info!("Removing stale daemon info file");
            let _ = std::fs::remove_file(info_path);
        }

        let daemon_info = DaemonInfo {
            pid: std::process::id(),
            listen_addr: addr.to_string(),
            started_at: chrono::Utc::now().to_rfc3339(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            platform: std::env::consts::OS.to_string(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&daemon_info) {
            let _ = std::fs::write(info_path, json);
            // SECURITY: Restrict daemon info file permissions (contains PID and port).
            restrict_permissions(info_path);
        }
    }

    info!("OpenFang API server listening on http://{addr}");
    info!("WebChat UI available at http://{addr}/",);
    info!("WebSocket endpoint: ws://{addr}/api/agents/{{id}}/ws",);

    // Use SO_REUSEADDR to allow binding immediately after reboot (avoids TIME_WAIT).
    let socket = socket2::Socket::new(
        if addr.is_ipv4() {
            socket2::Domain::IPV4
        } else {
            socket2::Domain::IPV6
        },
        socket2::Type::STREAM,
        None,
    )?;
    socket.set_reuse_address(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(1024)?;
    let listener =
        tokio::net::TcpListener::from_std(std::net::TcpListener::from(socket))?;

    // Run server with graceful shutdown.
    // SECURITY: `into_make_service_with_connect_info` injects the peer
    // SocketAddr so the auth middleware can check for loopback connections.
    let api_shutdown = state.shutdown_notify.clone();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(api_shutdown))
    .await?;

    // Clean up daemon info file
    if let Some(info_path) = daemon_info_path {
        let _ = std::fs::remove_file(info_path);
    }

    // Stop channel bridges
    if let Some(ref mut b) = *state.bridge_manager.lock().await {
        b.stop().await;
    }

    // Shutdown kernel
    kernel.shutdown();

    info!("OpenFang daemon stopped");
    Ok(())
}

/// SECURITY: Restrict file permissions to owner-only (0600) on Unix.
/// On non-Unix platforms this is a no-op.
#[cfg(unix)]
fn restrict_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) {}

/// Read daemon info from the standard location.
pub fn read_daemon_info(home_dir: &Path) -> Option<DaemonInfo> {
    let info_path = home_dir.join("daemon.json");
    let contents = std::fs::read_to_string(info_path).ok()?;
    serde_json::from_str(&contents).ok()
}

/// Wait for an OS termination signal OR an API shutdown request.
///
/// On Unix: listens for SIGINT, SIGTERM, and API notify.
/// On Windows: listens for Ctrl+C and API notify.
async fn shutdown_signal(api_shutdown: Arc<tokio::sync::Notify>) {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigint = signal(SignalKind::interrupt()).expect("Failed to listen for SIGINT");
        let mut sigterm = signal(SignalKind::terminate()).expect("Failed to listen for SIGTERM");

        tokio::select! {
            _ = sigint.recv() => {
                info!("Received SIGINT (Ctrl+C), shutting down...");
            }
            _ = sigterm.recv() => {
                info!("Received SIGTERM, shutting down...");
            }
            _ = api_shutdown.notified() => {
                info!("Shutdown requested via API, shutting down...");
            }
        }
    }

    #[cfg(not(unix))]
    {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Ctrl+C received, shutting down...");
            }
            _ = api_shutdown.notified() => {
                info!("Shutdown requested via API, shutting down...");
            }
        }
    }
}

/// Check if a process with the given PID is still alive.
fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // Use kill -0 to check if process exists without sending a signal
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[cfg(windows)]
    {
        // tasklist /FI "PID eq N" returns "INFO: No tasks..." when no match,
        // or a table row with the PID when found. Check exit code and that
        // "INFO:" is NOT in the output to confirm the process exists.
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .map(|o| {
                o.status.success() && {
                    let out = String::from_utf8_lossy(&o.stdout);
                    !out.contains("INFO:") && out.contains(&pid.to_string())
                }
            })
            .unwrap_or(false)
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

/// Check if an OpenFang daemon is actually responding at the given address.
/// This avoids false positives where a different process reused the same PID
/// after a system reboot.
fn is_daemon_responding(addr: &str) -> bool {
    // Quick TCP connect check — don't make a full HTTP request to avoid delays
    let addr_only = addr
        .strip_prefix("http://")
        .or_else(|| addr.strip_prefix("https://"))
        .unwrap_or(addr);
    if let Ok(sock_addr) = addr_only.parse::<std::net::SocketAddr>() {
        std::net::TcpStream::connect_timeout(
            &sock_addr,
            std::time::Duration::from_millis(500),
        )
        .is_ok()
    } else {
        // Fallback: try connecting to hostname
        std::net::TcpStream::connect(addr_only)
            .map(|_| true)
            .unwrap_or(false)
    }
}
