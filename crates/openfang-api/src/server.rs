//! OpenFang daemon server — boots the kernel and serves the HTTP API.

use crate::channel_bridge;
use crate::middleware;
use crate::rate_limiter;
use crate::routes::{self, AppState};
use crate::webchat;
use crate::ws;
use axum::extract::DefaultBodyLimit;
use axum::Router;
use openfang_kernel::OpenFangKernel;
use openfang_types::config::is_placeholder_api_key;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

/// Default max request body for regular API routes.
const DEFAULT_API_BODY_LIMIT_BYTES: usize = 2 * 1024 * 1024;
/// Upload endpoint body limit override (matches routes::MAX_UPLOAD_SIZE).
const UPLOAD_BODY_LIMIT_BYTES: usize = 10 * 1024 * 1024;

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
    crate::routes::start_upload_janitor();

    // Start channel bridges (Telegram, etc.)
    let bridge = channel_bridge::start_channel_bridge(kernel.clone()).await;

    let channels_config = kernel.config.channels.clone();
    let state = Arc::new(AppState {
        kernel: kernel.clone(),
        started_at: Instant::now(),
        bridge_manager: tokio::sync::Mutex::new(bridge),
        channels_config: tokio::sync::RwLock::new(channels_config),
        shutdown_notify: Arc::new(tokio::sync::Notify::new()),
        clawhub_cache: dashmap::DashMap::new(),
        provider_probe_cache: openfang_runtime::provider_health::ProbeCache::new(),
        budget_config: Arc::new(tokio::sync::RwLock::new(kernel.config.budget.clone())),
    });

    // CORS: allow localhost origins by default. If API key is set, the API
    // is protected anyway. For development, permissive CORS is convenient.
    let cors = if state.kernel.config.api_key.trim().is_empty() {
        // No auth → restrict CORS to localhost origins (include both 127.0.0.1 and localhost)
        let port = listen_addr.port();
        let mut origins: Vec<axum::http::HeaderValue> = vec![
            format!("http://{listen_addr}").parse().unwrap(),
            format!("http://localhost:{port}").parse().unwrap(),
        ];
        // Also allow common dev ports
        for p in [3000u16, 8080] {
            if p != port {
                if let Ok(v) = format!("http://127.0.0.1:{p}").parse() {
                    origins.push(v);
                }
                if let Ok(v) = format!("http://localhost:{p}").parse() {
                    origins.push(v);
                }
            }
        }
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods(tower_http::cors::Any)
            .allow_headers(tower_http::cors::Any)
    } else {
        // Auth enabled → restrict CORS to localhost + configured origins.
        // SECURITY: CorsLayer::permissive() is dangerous — any website could
        // make cross-origin requests. Restrict to known origins instead.
        let mut origins: Vec<axum::http::HeaderValue> = vec![
            format!("http://{listen_addr}").parse().unwrap(),
            "http://localhost:4200".parse().unwrap(),
            "http://127.0.0.1:4200".parse().unwrap(),
            "http://localhost:8080".parse().unwrap(),
            "http://127.0.0.1:8080".parse().unwrap(),
        ];
        // Add the actual listen address variants
        if listen_addr.port() != 4200 && listen_addr.port() != 8080 {
            if let Ok(v) = format!("http://localhost:{}", listen_addr.port()).parse() {
                origins.push(v);
            }
            if let Ok(v) = format!("http://127.0.0.1:{}", listen_addr.port()).parse() {
                origins.push(v);
            }
        }
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods(tower_http::cors::Any)
            .allow_headers(tower_http::cors::Any)
    };

    // Trim whitespace so `api_key = ""` or `api_key = "  "` both disable auth.
    let api_key = state.kernel.config.api_key.trim().to_string();
    let auth_state = crate::middleware::AuthState {
        api_key: api_key.clone(),
        auth_enabled: state.kernel.config.auth.enabled,
        session_secret: if !api_key.is_empty() {
            api_key.clone()
        } else if state.kernel.config.auth.enabled {
            state.kernel.config.auth.password_hash.clone()
        } else {
            String::new()
        },
    };
    let gcra_limiter = rate_limiter::create_rate_limiter();

    let app = Router::new()
        .route("/", axum::routing::get(webchat::webchat_page))
        .route("/logo.png", axum::routing::get(webchat::logo_png))
        .route("/favicon.ico", axum::routing::get(webchat::favicon_ico))
        .route("/manifest.json", axum::routing::get(webchat::manifest_json))
        .route("/sw.js", axum::routing::get(webchat::sw_js))
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
            "/api/agents/{id}",
            axum::routing::get(routes::get_agent)
                .delete(routes::kill_agent)
                .patch(routes::patch_agent),
        )
        .route(
            "/api/agents/{id}/mode",
            axum::routing::put(routes::set_agent_mode),
        )
        .route("/api/profiles", axum::routing::get(routes::list_profiles))
        .route(
            "/api/agents/{id}/restart",
            axum::routing::post(routes::restart_agent),
        )
        .route(
            "/api/agents/{id}/start",
            axum::routing::post(routes::restart_agent),
        )
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
            axum::routing::post(routes::upload_file)
                .layer(DefaultBodyLimit::max(UPLOAD_BODY_LIMIT_BYTES)),
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
            "/api/workflows/{id}",
            axum::routing::get(routes::get_workflow)
                .put(routes::update_workflow)
                .delete(routes::delete_workflow),
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
            "/api/hands/upsert",
            axum::routing::post(routes::upsert_hand),
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
            axum::routing::get(routes::get_hand_settings).put(routes::update_hand_settings),
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
        .route("/api/comms/send", axum::routing::post(routes::comms_send))
        .route("/api/comms/task", axum::routing::post(routes::comms_task));

    // Split into a second router chunk to stay within axum's type nesting limit.
    let app = app
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
            axum::routing::get(routes::agent_budget_status).put(routes::update_agent_budget),
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
        // Dashboard authentication endpoints
        .route("/api/auth/login", axum::routing::post(routes::auth_login))
        .route("/api/auth/logout", axum::routing::post(routes::auth_logout))
        .route("/api/auth/check", axum::routing::get(routes::auth_check))
        .layer(axum::middleware::from_fn_with_state(
            auth_state,
            middleware::auth,
        ))
        .layer(axum::middleware::from_fn_with_state(
            gcra_limiter,
            rate_limiter::gcra_rate_limit,
        ))
        .layer(axum::middleware::from_fn(middleware::security_headers))
        .layer(axum::middleware::from_fn(middleware::request_logging))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .layer(DefaultBodyLimit::max(DEFAULT_API_BODY_LIMIT_BYTES))
        .with_state(state.clone());

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
    validate_auth_exposure(&kernel, addr)?;

    // Check daemon info before we bind so we can reject a genuinely running daemon,
    // but do not write a fresh file until the listener is successfully created.
    if let Some(info_path) = daemon_info_path {
        if info_path.exists() {
            if let Ok(existing) = std::fs::read_to_string(info_path) {
                if let Ok(info) = serde_json::from_str::<DaemonInfo>(&existing) {
                    if is_process_alive(info.pid) && is_daemon_responding(&info.listen_addr) {
                        return Err(format!(
                            "Another daemon (PID {}) is already running at {}",
                            info.pid, info.listen_addr
                        )
                        .into());
                    }
                }
            }
            info!("Removing stale daemon info file");
            let _ = std::fs::remove_file(info_path);
        }
    }

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
    let listener = tokio::net::TcpListener::from_std(std::net::TcpListener::from(socket))?;

    let kernel = Arc::new(kernel);
    kernel.set_self_handle();
    kernel.start_background_agents();

    // Config file hot-reload watcher (polls every 30 seconds)
    {
        let k = kernel.clone();
        let config_path = kernel.config_path().to_path_buf();
        tokio::spawn(async move {
            let mut last_snapshot = snapshot_config_dependencies(&config_path);
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                let current_snapshot = snapshot_config_dependencies(&config_path);
                if current_snapshot != last_snapshot {
                    last_snapshot = current_snapshot;
                    tracing::info!("Config file changed, reloading...");
                    match k.reload_config() {
                        Ok(outcome) => {
                            if outcome.plan.has_changes() {
                                let pending: Vec<String> = outcome
                                    .plan
                                    .hot_actions
                                    .iter()
                                    .filter(|action| !outcome.hot_actions_applied.contains(action))
                                    .map(|action| format!("{action:?}"))
                                    .collect();
                                if outcome.plan.restart_required || !pending.is_empty() {
                                    tracing::warn!(
                                        restart_required = outcome.plan.restart_required,
                                        restart_reasons = ?outcome.plan.restart_reasons,
                                        pending_follow_up = ?pending,
                                        applied = ?outcome.hot_actions_applied,
                                        "Config hot-reload left the runtime partially applied"
                                    );
                                } else {
                                    tracing::info!(
                                        detected = ?outcome.plan.hot_actions,
                                        applied = ?outcome.hot_actions_applied,
                                        "Config hot-reload evaluated"
                                    );
                                }
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

    if let Some(info_path) = daemon_info_path {
        let daemon_info = DaemonInfo {
            pid: std::process::id(),
            listen_addr: addr.to_string(),
            started_at: chrono::Utc::now().to_rfc3339(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            platform: std::env::consts::OS.to_string(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&daemon_info) {
            let _ = std::fs::write(info_path, json);
            restrict_permissions(info_path);
        }
    }

    info!("OpenFang API server listening on http://{addr}");
    info!("WebChat UI available at http://{addr}/",);
    info!("WebSocket endpoint: ws://{addr}/api/agents/{{id}}/ws",);

    // Run server with graceful shutdown.
    // SECURITY: `into_make_service_with_connect_info` injects the peer
    // SocketAddr so the auth middleware can check for loopback connections.
    let api_shutdown = state.shutdown_notify.clone();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(kernel.clone(), api_shutdown))
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
async fn shutdown_signal(kernel: Arc<OpenFangKernel>, api_shutdown: Arc<tokio::sync::Notify>) {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigint = signal(SignalKind::interrupt()).expect("Failed to listen for SIGINT");
        let mut sigterm = signal(SignalKind::terminate()).expect("Failed to listen for SIGTERM");

        tokio::select! {
            _ = sigint.recv() => {
                kernel.supervisor.shutdown();
                info!("Received SIGINT (Ctrl+C), shutting down...");
            }
            _ = sigterm.recv() => {
                kernel.supervisor.shutdown();
                info!("Received SIGTERM, shutting down...");
            }
            _ = api_shutdown.notified() => {
                kernel.supervisor.shutdown();
                info!("Shutdown requested via API, shutting down...");
            }
        }
    }

    #[cfg(not(unix))]
    {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                kernel.supervisor.shutdown();
                info!("Ctrl+C received, shutting down...");
            }
            _ = api_shutdown.notified() => {
                kernel.supervisor.shutdown();
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
    use std::io::{Read, Write};

    let addr_only = addr
        .strip_prefix("http://")
        .or_else(|| addr.strip_prefix("https://"))
        .unwrap_or(addr);
    let connect_timeout = std::time::Duration::from_millis(500);
    let io_timeout = Some(std::time::Duration::from_millis(750));
    let mut stream = if let Ok(sock_addr) = addr_only.parse::<std::net::SocketAddr>() {
        match std::net::TcpStream::connect_timeout(&sock_addr, connect_timeout) {
            Ok(stream) => stream,
            Err(_) => return false,
        }
    } else {
        match std::net::TcpStream::connect(addr_only) {
            Ok(stream) => stream,
            Err(_) => return false,
        }
    };

    let _ = stream.set_read_timeout(io_timeout);
    let _ = stream.set_write_timeout(io_timeout);

    let host_header = addr_only
        .parse::<std::net::SocketAddr>()
        .map(|sock| sock.to_string())
        .unwrap_or_else(|_| addr_only.to_string());
    let request =
        format!("GET /api/health HTTP/1.1\r\nHost: {host_header}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }

    let mut response = String::new();
    if stream.read_to_string(&mut response).is_err() {
        return false;
    }

    let mut parts = response.splitn(2, "\r\n\r\n");
    let headers = parts.next().unwrap_or_default();
    let body = parts.next().unwrap_or_default();

    if !(headers.starts_with("HTTP/1.1 200") || headers.starts_with("HTTP/1.0 200")) {
        return false;
    }

    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .map(|payload| {
            payload
                .get("status")
                .and_then(|value| value.as_str())
                .is_some()
                && payload
                    .get("version")
                    .and_then(|value| value.as_str())
                    .is_some()
        })
        .unwrap_or(false)
}

fn snapshot_config_dependencies(
    config_path: &Path,
) -> Vec<(std::path::PathBuf, Option<std::time::SystemTime>)> {
    let mut paths = openfang_kernel::config::collect_config_dependency_paths(config_path)
        .unwrap_or_else(|e| {
            tracing::debug!(
                error = %e,
                path = %config_path.display(),
                "Falling back to root config watcher snapshot"
            );
            vec![config_path.to_path_buf()]
        });
    paths.sort();
    paths.dedup();
    paths
        .into_iter()
        .map(|path| {
            let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            (path, modified)
        })
        .collect()
}

fn validate_auth_exposure(
    kernel: &OpenFangKernel,
    addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error>> {
    let api_key = kernel.config.api_key.trim();
    let api_key_enabled = !api_key.is_empty();
    let dashboard_auth_enabled = kernel.config.auth.enabled;
    let password_hash = kernel.config.auth.password_hash.trim();
    let dashboard_auth_ready = dashboard_auth_enabled
        && crate::session_auth::is_supported_password_hash_format(password_hash);
    if dashboard_auth_enabled && password_hash.is_empty() {
        return Err(
            "Dashboard auth is enabled but auth.password_hash is empty. Generate a password hash with `openfang security hash-password` or disable [auth].enabled before starting the daemon."
                .into(),
        );
    }
    if dashboard_auth_enabled && !dashboard_auth_ready {
        return Err(
            "Dashboard auth is enabled but auth.password_hash is not a supported hash format. Use `openfang security hash-password` for Argon2id or provide a 64-character legacy SHA-256 hex digest."
                .into(),
        );
    }
    if !api_key_enabled && !dashboard_auth_ready && !addr.ip().is_loopback() {
        return Err(format!(
            "Refusing to expose the API on {addr} without authentication. Set OPENFANG_API_KEY or bind to 127.0.0.1."
        )
        .into());
    }
    if !addr.ip().is_loopback() && api_key_enabled && is_placeholder_api_key(api_key) {
        return Err(format!(
            "Refusing to expose the API on {addr} with placeholder API key '{api_key}'. Set a strong OPENFANG_API_KEY or bind to 127.0.0.1."
        )
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Method, Request, StatusCode};
    use openfang_types::config::KernelConfig;
    use tower::ServiceExt;

    fn test_kernel() -> OpenFangKernel {
        let tmp = tempfile::tempdir().unwrap();
        let config = KernelConfig {
            home_dir: tmp.path().to_path_buf(),
            data_dir: tmp.path().join("data"),
            ..KernelConfig::default()
        };
        OpenFangKernel::boot_with_config(config).unwrap()
    }

    #[test]
    fn validate_auth_exposure_rejects_remote_bind_without_auth() {
        let kernel = test_kernel();
        let err = validate_auth_exposure(&kernel, "0.0.0.0:4200".parse().unwrap()).unwrap_err();
        assert!(err
            .to_string()
            .contains("Refusing to expose the API on 0.0.0.0:4200 without authentication"));
    }

    #[test]
    fn validate_auth_exposure_allows_loopback_without_auth() {
        let kernel = test_kernel();
        assert!(validate_auth_exposure(&kernel, "127.0.0.1:4200".parse().unwrap()).is_ok());
    }

    #[test]
    fn validate_auth_exposure_rejects_placeholder_api_key_on_public_bind() {
        let tmp = tempfile::tempdir().unwrap();
        let config = KernelConfig {
            home_dir: tmp.path().to_path_buf(),
            data_dir: tmp.path().join("data"),
            api_key: "change-me".to_string(),
            ..KernelConfig::default()
        };
        let kernel = OpenFangKernel::boot_with_config(config).unwrap();

        let err = validate_auth_exposure(&kernel, "0.0.0.0:4200".parse().unwrap()).unwrap_err();

        assert!(err.to_string().contains("placeholder API key"));
    }

    #[test]
    fn boot_rejects_dashboard_auth_without_password_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let config = KernelConfig {
            home_dir: tmp.path().to_path_buf(),
            data_dir: tmp.path().join("data"),
            auth: openfang_types::config::AuthConfig {
                enabled: true,
                password_hash: String::new(),
                ..Default::default()
            },
            ..KernelConfig::default()
        };
        let err = OpenFangKernel::boot_with_config(config)
            .err()
            .expect("boot should reject missing dashboard auth password hash");

        assert!(err.to_string().contains("auth.password_hash is empty"));
    }

    #[test]
    fn boot_rejects_invalid_dashboard_auth_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let config = KernelConfig {
            home_dir: tmp.path().to_path_buf(),
            data_dir: tmp.path().join("data"),
            auth: openfang_types::config::AuthConfig {
                enabled: true,
                password_hash: "not-a-real-hash".to_string(),
                ..Default::default()
            },
            ..KernelConfig::default()
        };
        let err = OpenFangKernel::boot_with_config(config)
            .err()
            .expect("boot should reject invalid dashboard auth hash");

        let err_text = err.to_string();
        assert!(err_text.contains("auth.password_hash"));
        assert!(err_text.contains("supported"));
    }

    #[tokio::test]
    async fn run_daemon_does_not_write_pid_file_when_bind_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let daemon_info_path = tmp.path().join("daemon.json");
        let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = occupied.local_addr().unwrap();

        let config = KernelConfig {
            home_dir: tmp.path().to_path_buf(),
            data_dir: tmp.path().join("data"),
            ..KernelConfig::default()
        };
        let kernel = OpenFangKernel::boot_with_config(config).unwrap();

        let result = run_daemon(kernel, &addr.to_string(), Some(&daemon_info_path)).await;
        assert!(result.is_err());
        assert!(
            !daemon_info_path.exists(),
            "daemon.json should not be created when the listener fails to bind"
        );
    }

    #[test]
    fn snapshot_config_dependencies_tracks_include_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("config.toml");
        let include = dir.path().join("providers.toml");

        std::fs::write(&include, "api_listen = \"127.0.0.1:4200\"\n").unwrap();
        std::fs::write(&root, "include = [\"providers.toml\"]\n").unwrap();

        let snapshot = snapshot_config_dependencies(&root);
        let paths: Vec<_> = snapshot.into_iter().map(|(path, _)| path).collect();

        assert!(paths.contains(&root.canonicalize().unwrap()));
        assert!(paths.contains(&include.canonicalize().unwrap()));
    }

    #[test]
    fn is_daemon_responding_requires_openfang_health_payload() {
        use std::io::{Read, Write};
        use std::sync::mpsc;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (ready_tx, ready_rx) = mpsc::channel();

        std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            if let Ok((mut stream, _)) = listener.accept() {
                let mut request = [0u8; 512];
                let _ = stream.read(&mut request);
                let body = r#"{"status":"ok","version":"test"}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });

        ready_rx.recv().unwrap();
        assert!(is_daemon_responding(&addr.to_string()));
    }

    #[test]
    fn is_daemon_responding_rejects_non_openfang_tcp_listener() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        assert!(!is_daemon_responding(&addr.to_string()));
    }

    #[tokio::test]
    async fn build_router_wires_auth_and_metrics_routes() {
        let tmp = tempfile::tempdir().unwrap();
        let config = KernelConfig {
            home_dir: tmp.path().to_path_buf(),
            data_dir: tmp.path().join("data"),
            auth: openfang_types::config::AuthConfig {
                enabled: true,
                username: "admin".to_string(),
                password_hash: crate::session_auth::hash_password("secret123").unwrap(),
                session_ttl_hours: 24,
            },
            ..KernelConfig::default()
        };
        let kernel = Arc::new(OpenFangKernel::boot_with_config(config).unwrap());
        kernel.set_self_handle();

        let (app, _state) = build_router(kernel, "127.0.0.1:4200".parse().unwrap()).await;

        let unauthenticated = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

        let login = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/auth/login")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"username":"admin","password":"secret123"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(login.status(), StatusCode::OK);
        let cookie = login
            .headers()
            .get(axum::http::header::SET_COOKIE)
            .and_then(|value| value.to_str().ok())
            .unwrap()
            .to_string();

        let metrics = app
            .oneshot(
                Request::builder()
                    .uri("/api/metrics")
                    .header(axum::http::header::COOKIE, cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(metrics.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn build_router_rejects_oversized_login_body_at_http_boundary() {
        let kernel = Arc::new(test_kernel());
        kernel.set_self_handle();
        let (app, _state) = build_router(kernel, "127.0.0.1:4200".parse().unwrap()).await;

        let oversized = vec![b'a'; DEFAULT_API_BODY_LIMIT_BYTES + 1];
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/auth/login")
                    .header("content-type", "application/json")
                    .body(Body::from(oversized))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn build_router_allows_upload_body_above_default_limit() {
        let kernel = Arc::new(test_kernel());
        kernel.set_self_handle();
        let (app, _state) = build_router(kernel, "127.0.0.1:4200".parse().unwrap()).await;

        let valid_uuid = uuid::Uuid::new_v4().to_string();
        let upload_body = vec![b'x'; DEFAULT_API_BODY_LIMIT_BYTES + 1];
        let mut request = Request::builder()
            .method(Method::POST)
            .uri(format!("/api/agents/{valid_uuid}/upload"))
            .header("content-type", "application/pdf")
            .header("x-filename", "big.pdf")
            .body(Body::from(upload_body))
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 4242))));

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
