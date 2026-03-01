//! OpenFang daemon server — boots the kernel and serves the HTTP API.

use crate::channel_bridge;
use crate::middleware;
use crate::rate_limiter;
use crate::routes::{self, AppState};
use crate::webchat;
use crate::ws;
use axum::Router;
use openfang_kernel::OpenFangKernel;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

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

    let channels_config = kernel.config.channels.clone();
    let state = Arc::new(AppState {
        kernel: kernel.clone(),
        started_at: Instant::now(),
        peer_registry: kernel.peer_registry.as_ref().map(|r| Arc::new(r.clone())),
        bridge_manager: tokio::sync::Mutex::new(bridge),
        channels_config: tokio::sync::RwLock::new(channels_config),
        shutdown_notify: Arc::new(tokio::sync::Notify::new()),
    });

    // CORS: allow localhost origins by default. If API key is set, the API
    // is protected anyway. For development, permissive CORS is convenient.
    let cors = if state.kernel.config.api_key.is_empty() {
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

    let api_key = state.kernel.config.api_key.clone();
    let gcra_limiter = rate_limiter::create_rate_limiter();

    let app = Router::new()
        .route("/", axum::routing::get(webchat::webchat_page))
        .route("/logo.png", axum::routing::get(webchat::logo_png))
        .route("/favicon.ico", axum::routing::get(webchat::favicon_ico))
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
            axum::routing::get(routes::get_agent).delete(routes::kill_agent),
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
            "/api/clawhub/install",
            axum::routing::post(routes::clawhub_install),
        )
        // Hands endpoints
        .route("/api/hands", axum::routing::get(routes::list_hands))
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
        .layer(axum::middleware::from_fn_with_state(
            api_key,
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

    // Write daemon info file
    if let Some(info_path) = daemon_info_path {
        // Check if another daemon is already running with this PID file
        if info_path.exists() {
            if let Ok(existing) = std::fs::read_to_string(info_path) {
                if let Ok(info) = serde_json::from_str::<DaemonInfo>(&existing) {
                    if is_process_alive(info.pid) {
                        return Err(format!(
                            "Another daemon (PID {}) is already running at {}",
                            info.pid, info.listen_addr
                        )
                        .into());
                    }
                }
            }
            // Stale PID file, remove it
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

    let listener = tokio::net::TcpListener::bind(addr).await?;

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
