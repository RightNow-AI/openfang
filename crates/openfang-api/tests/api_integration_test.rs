//! Real HTTP integration tests for the OpenFang API.
//!
//! These tests boot a real kernel, start a real axum HTTP server on a random
//! port, and hit actual endpoints with reqwest.  No mocking.
//!
//! Tests that require an LLM API call are gated behind GROQ_API_KEY.
//!
//! Run: cargo test -p openfang-api --test api_integration_test -- --nocapture

use axum::Router;
use openfang_api::middleware;
use openfang_api::routes::{self, AppState};
use openfang_api::server;
use openfang_api::webchat;
use openfang_api::ws;
use openfang_kernel::OpenFangKernel;
use openfang_memory::usage::UsageRecord;
use openfang_memory::MemorySubstrate;
use openfang_types::config::{
    BudgetConfig, ChannelsConfig, DefaultModelConfig, KernelConfig, MemoryConfig, TelegramConfig,
};
use openfang_types::message::TokenUsage;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

// ---------------------------------------------------------------------------
// Test infrastructure
// ---------------------------------------------------------------------------

struct EnvVarGuard {
    key: String,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn remove(key: &str) -> Self {
        let previous = std::env::var(key).ok();
        std::env::remove_var(key);
        Self {
            key: key.to_string(),
            previous,
        }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(ref value) = self.previous {
            std::env::set_var(&self.key, value);
        } else {
            std::env::remove_var(&self.key);
        }
    }
}

struct TestServer {
    base_url: String,
    state: Arc<AppState>,
    _tmp: tempfile::TempDir,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.state.kernel.shutdown();
    }
}

/// Start a test server using ollama as default provider (no API key needed).
/// This lets the kernel boot without any real LLM credentials.
/// Tests that need actual LLM calls should use `start_test_server_with_llm()`.
async fn start_test_server() -> TestServer {
    start_test_server_with_provider("ollama", "test-model", "OLLAMA_API_KEY").await
}

/// Start a test server with Groq as the LLM provider (requires GROQ_API_KEY).
async fn start_test_server_with_llm() -> TestServer {
    start_test_server_with_provider("groq", "llama-3.3-70b-versatile", "GROQ_API_KEY").await
}

async fn start_test_server_with_provider(
    provider: &str,
    model: &str,
    api_key_env: &str,
) -> TestServer {
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");

    let config = KernelConfig {
        home_dir: tmp.path().to_path_buf(),
        data_dir: tmp.path().join("data"),
        default_model: DefaultModelConfig {
            provider: provider.to_string(),
            model: model.to_string(),
            api_key_env: api_key_env.to_string(),
            base_url: None,
        },
        ..KernelConfig::default()
    };

    start_test_server_with_config(config, tmp).await
}

async fn start_test_server_with_config(config: KernelConfig, tmp: tempfile::TempDir) -> TestServer {
    let kernel = OpenFangKernel::boot_with_config(config).expect("Kernel should boot");
    let kernel = Arc::new(kernel);
    kernel.set_self_handle();

    let state = Arc::new(AppState {
        kernel,
        started_at: Instant::now(),
        bridge_manager: tokio::sync::Mutex::new(None),
        channels_config: tokio::sync::RwLock::new(Default::default()),
        shutdown_notify: Arc::new(tokio::sync::Notify::new()),
        clawhub_cache: dashmap::DashMap::new(),
        provider_probe_cache: openfang_runtime::provider_health::ProbeCache::new(),
    });

    let app = Router::new()
        .route("/", axum::routing::get(webchat::webchat_page))
        .route("/api/health", axum::routing::get(routes::health))
        .route(
            "/api/health/detail",
            axum::routing::get(routes::health_detail),
        )
        .route(
            "/api/metrics",
            axum::routing::get(routes::prometheus_metrics),
        )
        .route("/api/status", axum::routing::get(routes::status))
        .route("/api/commands", axum::routing::get(routes::list_commands))
        .route(
            "/api/providers/{name}/key",
            axum::routing::post(routes::set_provider_key).delete(routes::delete_provider_key),
        )
        .route(
            "/api/agents",
            axum::routing::get(routes::list_agents).post(routes::spawn_agent),
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
            "/api/agents/{id}/restart",
            axum::routing::post(routes::restart_agent),
        )
        .route(
            "/api/agents/{id}",
            axum::routing::get(routes::get_agent).delete(routes::kill_agent),
        )
        .route(
            "/api/agents/{id}/session",
            axum::routing::get(routes::get_agent_session),
        )
        .route(
            "/api/agents/{id}/sessions",
            axum::routing::post(routes::create_agent_session),
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
        .route("/api/agents/{id}/ws", axum::routing::get(ws::agent_ws))
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
        .route(
            "/api/triggers",
            axum::routing::get(routes::list_triggers).post(routes::create_trigger),
        )
        .route(
            "/api/triggers/{id}",
            axum::routing::delete(routes::delete_trigger),
        )
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
        .route(
            "/api/workflows/{id}",
            axum::routing::get(routes::get_workflow)
                .put(routes::update_workflow)
                .delete(routes::delete_workflow),
        )
        .route("/api/shutdown", axum::routing::post(routes::shutdown))
        .layer(axum::middleware::from_fn(middleware::request_logging))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind test server");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    TestServer {
        base_url: format!("http://{}", addr),
        state,
        _tmp: tmp,
    }
}

async fn start_full_test_server_with_config(
    config: KernelConfig,
    tmp: tempfile::TempDir,
) -> TestServer {
    let kernel = Arc::new(OpenFangKernel::boot_with_config(config).expect("Kernel should boot"));
    kernel.set_self_handle();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind test server");
    let addr = listener.local_addr().unwrap();
    let (app, state) = server::build_router(kernel, addr).await;

    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    TestServer {
        base_url: format!("http://{}", addr),
        state,
        _tmp: tmp,
    }
}

/// Manifest that uses ollama (no API key required, won't make real LLM calls).
const TEST_MANIFEST: &str = r#"
name = "test-agent"
version = "0.1.0"
description = "Integration test agent"
author = "test"
module = "builtin:chat"

[model]
provider = "ollama"
model = "test-model"
system_prompt = "You are a test agent. Reply concisely."

[capabilities]
tools = ["file_read"]
memory_read = ["*"]
memory_write = ["self.*"]
"#;

/// Manifest that uses Groq for real LLM tests.
const LLM_MANIFEST: &str = r#"
name = "test-agent"
version = "0.1.0"
description = "Integration test agent"
author = "test"
module = "builtin:chat"

[model]
provider = "groq"
model = "llama-3.3-70b-versatile"
system_prompt = "You are a test agent. Reply concisely."

[capabilities]
tools = ["file_read"]
memory_read = ["*"]
memory_write = ["self.*"]
"#;

/// Manifest that inherits the kernel default provider/model labels.
const DEFAULT_MODEL_MANIFEST: &str = r#"
name = "metrics-default-agent"
version = "0.1.0"
description = "Integration test agent"
author = "test"
module = "builtin:chat"

[model]
provider = "default"
model = "default"
system_prompt = "You are a test agent. Reply concisely."

[capabilities]
tools = ["file_read"]
memory_read = ["*"]
memory_write = ["self.*"]
"#;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_health_endpoint() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/health", server.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    // Middleware injects x-request-id
    assert!(resp.headers().contains_key("x-request-id"));

    let body: serde_json::Value = resp.json().await.unwrap();
    // Public health endpoint returns minimal info (redacted for security)
    assert_eq!(body["status"], "ok");
    assert!(body["version"].is_string());
    // Detailed fields should NOT appear in public health endpoint
    assert!(body["database"].is_null());
    assert!(body["agent_count"].is_null());
}

#[tokio::test]
async fn test_health_detail_degrades_when_default_provider_auth_is_missing() {
    let _groq_guard = EnvVarGuard::remove("GROQ_API_KEY");
    let server =
        start_test_server_with_provider("groq", "llama-3.3-70b-versatile", "GROQ_API_KEY").await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/health/detail", server.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 503);
    assert!(resp.headers().contains_key("x-request-id"));
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "degraded");
    assert_eq!(body["readiness"]["ready"], false);
    assert_eq!(body["readiness"]["default_provider_auth"], "missing");
    assert!(body["readiness"]["failing_checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "default_provider_auth"));
}

#[tokio::test]
async fn test_health_detail_degrades_when_explicit_embedding_provider_is_unusable() {
    let _embedding_guard = EnvVarGuard::remove("OPENFANG_TEST_EMBEDDING_KEY");
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");
    let config = KernelConfig {
        home_dir: tmp.path().to_path_buf(),
        data_dir: tmp.path().join("data"),
        default_model: DefaultModelConfig {
            provider: "ollama".to_string(),
            model: "test-model".to_string(),
            api_key_env: "OLLAMA_API_KEY".to_string(),
            base_url: None,
        },
        memory: MemoryConfig {
            embedding_provider: Some("openai".to_string()),
            embedding_api_key_env: Some("OPENFANG_TEST_EMBEDDING_KEY".to_string()),
            ..Default::default()
        },
        ..KernelConfig::default()
    };

    let server = start_test_server_with_config(config, tmp).await;
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/api/health/detail", server.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 503);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "degraded");
    assert_eq!(body["readiness"]["ready"], false);
    assert!(body["readiness"]["failing_checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "embedding"));
    assert_eq!(body["embedding"]["mode"], "explicit");
    assert_eq!(body["embedding"]["provider"], "openai");
    assert_eq!(
        body["embedding"]["api_key_env"],
        "OPENFANG_TEST_EMBEDDING_KEY"
    );
    assert_eq!(body["embedding"]["api_key_configured"], false);
    assert_eq!(body["embedding"]["driver_active"], false);
    assert!(body["embedding"]["warning"]
        .as_str()
        .unwrap()
        .contains("OPENFANG_TEST_EMBEDDING_KEY"));
}

#[tokio::test]
async fn test_health_detail_uses_effective_default_model_override() {
    let _groq_guard = EnvVarGuard::remove("GROQ_API_KEY");
    let server = start_test_server().await;
    {
        let mut override_guard = server.state.kernel.default_model_override.write().unwrap();
        *override_guard = Some(DefaultModelConfig {
            provider: "groq".to_string(),
            model: "llama-3.3-70b-versatile".to_string(),
            api_key_env: "GROQ_API_KEY".to_string(),
            base_url: None,
        });
    }

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/api/health/detail", server.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 503);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "degraded");
    assert_eq!(body["readiness"]["ready"], false);
    assert_eq!(body["readiness"]["default_provider_auth"], "missing");
    assert!(body["readiness"]["failing_checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "default_provider_auth"));
}

#[tokio::test]
async fn test_runtime_surfaces_use_effective_default_model_override() {
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");
    let config = KernelConfig {
        home_dir: tmp.path().to_path_buf(),
        data_dir: tmp.path().join("data"),
        default_model: DefaultModelConfig {
            provider: "ollama".to_string(),
            model: "test-model".to_string(),
            api_key_env: "OLLAMA_API_KEY".to_string(),
            base_url: None,
        },
        ..KernelConfig::default()
    };
    let server = start_full_test_server_with_config(config, tmp).await;
    {
        let mut override_guard = server.state.kernel.default_model_override.write().unwrap();
        *override_guard = Some(DefaultModelConfig {
            provider: "groq".to_string(),
            model: "llama-3.3-70b-versatile".to_string(),
            api_key_env: "GROQ_API_KEY".to_string(),
            base_url: None,
        });
    }

    let client = reqwest::Client::new();

    let spawn = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": DEFAULT_MODEL_MANIFEST}))
        .send()
        .await
        .unwrap();
    assert_eq!(spawn.status(), 201);
    let spawn_body: serde_json::Value = spawn.json().await.unwrap();
    let agent_id: openfang_types::agent::AgentId =
        spawn_body["agent_id"].as_str().unwrap().parse().unwrap();
    server.state.kernel.scheduler.record_usage(
        agent_id,
        &TokenUsage {
            input_tokens: 2,
            output_tokens: 3,
        },
    );

    let agents = client
        .get(format!("{}/api/agents", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(agents.status(), 200);
    let agents_body: Vec<serde_json::Value> = agents.json().await.unwrap();
    let agent = agents_body
        .iter()
        .find(|item| item["id"] == agent_id.to_string())
        .unwrap();
    assert_eq!(agent["model_provider"], "groq");
    assert_eq!(agent["model_name"], "llama-3.3-70b-versatile");

    let metrics = client
        .get(format!("{}/api/metrics", server.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(metrics.contains(
        "openfang_tokens_total{agent=\"metrics-default-agent\",provider=\"groq\",model=\"llama-3.3-70b-versatile\"} 5"
    ));

    let hand = client
        .get(format!("{}/api/hands/lead", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(hand.status(), 200);
    let hand_body: serde_json::Value = hand.json().await.unwrap();
    assert_eq!(hand_body["agent"]["provider"], "groq");
    assert_eq!(hand_body["agent"]["model"], "llama-3.3-70b-versatile");

    let activate = client
        .post(format!("{}/api/hands/lead/activate", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(activate.status(), 200);
    let activate_body: serde_json::Value = activate.json().await.unwrap();
    let hand_agent_id: openfang_types::agent::AgentId =
        activate_body["agent_id"].as_str().unwrap().parse().unwrap();
    let hand_agent = server.state.kernel.registry.get(hand_agent_id).unwrap();
    assert_eq!(hand_agent.manifest.model.provider, "groq");
    assert_eq!(hand_agent.manifest.model.model, "llama-3.3-70b-versatile");

    let config = client
        .get(format!("{}/api/config", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(config.status(), 200);
    let config_body: serde_json::Value = config.json().await.unwrap();
    assert_eq!(config_body["default_model"]["provider"], "groq");
    assert_eq!(
        config_body["default_model"]["model"],
        "llama-3.3-70b-versatile"
    );
    assert_eq!(config_body["default_model"]["api_key_env"], "GROQ_API_KEY");
}

#[tokio::test]
async fn test_health_detail_accepts_runtime_env_file_credentials() {
    let _telegram_guard = EnvVarGuard::remove("TELEGRAM_BOT_TOKEN");
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");
    std::fs::write(
        tmp.path().join(".env"),
        "TELEGRAM_BOT_TOKEN=from-runtime-dotenv\n",
    )
    .unwrap();

    let channels = ChannelsConfig {
        telegram: Some(TelegramConfig {
            bot_token_env: "TELEGRAM_BOT_TOKEN".to_string(),
            ..Default::default()
        }),
        ..Default::default()
    };

    let config = KernelConfig {
        home_dir: tmp.path().to_path_buf(),
        data_dir: tmp.path().join("data"),
        channels,
        default_model: DefaultModelConfig {
            provider: "ollama".to_string(),
            model: "test-model".to_string(),
            api_key_env: "OLLAMA_API_KEY".to_string(),
            base_url: None,
        },
        ..KernelConfig::default()
    };

    let server = start_test_server_with_config(config, tmp).await;
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/api/health/detail", server.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert!(body["config_warnings"]
        .as_array()
        .unwrap()
        .iter()
        .all(|warning| warning.as_str()
            != Some("Telegram configured but TELEGRAM_BOT_TOKEN is not set")));
}

#[tokio::test]
async fn test_health_detail_degrades_when_agent_restore_fails() {
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");
    let db_path = tmp.path().join("data").join("openfang.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let memory = MemorySubstrate::open(&db_path, 0.05).unwrap();
    {
        let conn = memory.usage_conn();
        let conn = conn.lock().unwrap();
        let session_id = openfang_types::agent::SessionId::new().0.to_string();
        conn.execute_batch(&format!(
            "ALTER TABLE agents ADD COLUMN session_id TEXT DEFAULT '';
             ALTER TABLE agents ADD COLUMN identity TEXT DEFAULT '{{}}';
             INSERT INTO agents (id, name, manifest, state, created_at, updated_at, session_id, identity)
             VALUES ('not-a-uuid', 'broken-agent', X'00', '\"Running\"', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', '{session_id}', '{{}}');"
        ))
        .unwrap();
    }

    let config = KernelConfig {
        home_dir: tmp.path().to_path_buf(),
        data_dir: tmp.path().join("data"),
        default_model: DefaultModelConfig {
            provider: "ollama".to_string(),
            model: "test-model".to_string(),
            api_key_env: "OLLAMA_API_KEY".to_string(),
            base_url: None,
        },
        ..KernelConfig::default()
    };

    let server = start_test_server_with_config(config, tmp).await;
    let client = reqwest::Client::new();

    let status_resp = client
        .get(format!("{}/api/status", server.base_url))
        .send()
        .await
        .unwrap();
    let status_body: serde_json::Value = status_resp.json().await.unwrap();
    assert_eq!(status_body["agent_count"], 0);

    let resp = client
        .get(format!("{}/api/health/detail", server.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 503);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "degraded");
    assert_eq!(body["readiness"]["ready"], false);
    assert!(body["readiness"]["failing_checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "agent_restore"));
    assert_eq!(body["restore_warnings"]["persisted_agent_rows"], 1);
    assert_eq!(body["restore_warnings"]["restored_agent_rows"], 0);
    assert!(body["restore_warnings"]["agent"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning
            .as_str()
            .unwrap_or_default()
            .contains("invalid UUID")));
}

#[tokio::test]
async fn test_health_detail_degrades_when_cron_restore_fails() {
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");
    std::fs::write(tmp.path().join("cron_jobs.json"), "{bad json").unwrap();

    let config = KernelConfig {
        home_dir: tmp.path().to_path_buf(),
        data_dir: tmp.path().join("data"),
        default_model: DefaultModelConfig {
            provider: "ollama".to_string(),
            model: "test-model".to_string(),
            api_key_env: "OLLAMA_API_KEY".to_string(),
            base_url: None,
        },
        ..KernelConfig::default()
    };

    let server = start_test_server_with_config(config, tmp).await;
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/api/health/detail", server.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 503);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "degraded");
    assert!(body["readiness"]["failing_checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "cron_restore"));
    assert!(body["restore_warnings"]["cron"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning
            .as_str()
            .unwrap_or_default()
            .contains("cron restore failed")));
}

#[tokio::test]
async fn test_health_detail_degrades_when_hand_restore_fails() {
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");
    std::fs::write(tmp.path().join("hand_state.json"), "{bad json").unwrap();

    let config = KernelConfig {
        home_dir: tmp.path().to_path_buf(),
        data_dir: tmp.path().join("data"),
        default_model: DefaultModelConfig {
            provider: "ollama".to_string(),
            model: "test-model".to_string(),
            api_key_env: "OLLAMA_API_KEY".to_string(),
            base_url: None,
        },
        ..KernelConfig::default()
    };

    let server = start_test_server_with_config(config, tmp).await;
    server.state.kernel.start_background_agents();
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/api/health/detail", server.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 503);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "degraded");
    assert!(body["readiness"]["failing_checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "hand_restore"));
    assert!(body["restore_warnings"]["hand"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning
            .as_str()
            .unwrap_or_default()
            .contains("hand state restore failed")));
}

#[tokio::test]
async fn test_health_detail_degrades_when_default_provider_is_unknown() {
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");
    let config = KernelConfig {
        home_dir: tmp.path().to_path_buf(),
        data_dir: tmp.path().join("data"),
        default_model: DefaultModelConfig {
            provider: "missing-provider".to_string(),
            model: "test-model".to_string(),
            api_key_env: "MISSING_PROVIDER_API_KEY".to_string(),
            base_url: None,
        },
        ..KernelConfig::default()
    };

    let server = start_test_server_with_config(config, tmp).await;
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/api/health/detail", server.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 503);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "degraded");
    assert_eq!(body["readiness"]["ready"], false);
    assert_eq!(body["readiness"]["default_provider_auth"], "unknown");
    assert!(body["readiness"]["failing_checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "default_provider_auth"));
}

#[tokio::test]
async fn test_health_detail_keeps_ready_after_recorded_panic() {
    let server = start_test_server().await;
    server.state.kernel.supervisor.record_panic();

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/api/health/detail", server.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["readiness"]["ready"], true);
    assert_eq!(body["panic_count"], 1);
    assert!(body["readiness"]["failing_checks"]
        .as_array()
        .unwrap()
        .iter()
        .all(|value| value != "supervisor_panics"));
}

#[tokio::test]
async fn test_health_detail_degrades_when_agent_runtime_is_unhealthy() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let spawn = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({
            "manifest_toml": r#"
name = "heartbeat-agent"
version = "0.1.0"
description = "Autonomous test agent"
author = "openfang"
module = "builtin:chat"

[model]
provider = "default"
model = "default"
system_prompt = "You are a helper."

[autonomous]
heartbeat_interval_secs = 30
"#
        }))
        .send()
        .await
        .unwrap();
    let spawn_body: serde_json::Value = spawn.json().await.unwrap();
    let agent_id: openfang_types::agent::AgentId =
        spawn_body["agent_id"].as_str().unwrap().parse().unwrap();

    server
        .state
        .kernel
        .registry
        .set_state(agent_id, openfang_types::agent::AgentState::Crashed)
        .unwrap();

    let resp = client
        .get(format!("{}/api/health/detail", server.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 503);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "degraded");
    assert_eq!(body["agent_runtime"]["unhealthy_count"], 1);
    assert!(body["agent_runtime"]["agents"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value["agent_id"] == agent_id.to_string()));
    assert!(body["readiness"]["failing_checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "agent_runtime"));
}

#[tokio::test]
async fn test_metrics_expose_operational_metric_families() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let body = client
        .get(format!("{}/api/metrics", server.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert!(body.contains("openfang_readiness_ready"));
    assert!(body.contains("openfang_database_ok"));
    assert!(body.contains("openfang_usage_store_ok"));
    assert!(body.contains("openfang_shutdown_requested"));
    assert!(body.contains("openfang_default_provider_auth_missing"));
    assert!(body.contains("openfang_config_warnings"));
    assert!(body.contains("openfang_restore_warnings"));
    assert!(body.contains("openfang_agent_runtime_issues"));
    assert!(body.contains("openfang_panics_total"));
    assert!(body.contains("openfang_restarts_total"));
    assert!(body.contains("openfang_info"));
}

#[tokio::test]
async fn test_health_detail_degrades_when_usage_store_is_unusable() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    {
        let conn = server.state.kernel.memory.usage_conn();
        let conn = conn.lock().unwrap();
        conn.execute("DROP TABLE usage_events", []).unwrap();
    }

    let resp = client
        .get(format!("{}/api/health/detail", server.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 503);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "degraded");
    assert_eq!(body["usage_store"], "error");
    assert!(body["readiness"]["failing_checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "usage_store"));
}

#[tokio::test]
async fn test_budget_endpoints_fail_when_usage_store_is_unusable() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();
    let agent_id = server.state.kernel.registry.list()[0].id.to_string();

    {
        let conn = server.state.kernel.memory.usage_conn();
        let conn = conn.lock().unwrap();
        conn.execute("DROP TABLE usage_events", []).unwrap();
    }

    let global = client
        .get(format!("{}/api/budget", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(global.status(), 500);

    let ranking = client
        .get(format!("{}/api/budget/agents", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(ranking.status(), 500);

    let per_agent = client
        .get(format!(
            "{}/api/budget/agents/{}",
            server.base_url, agent_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(per_agent.status(), 500);
}

#[tokio::test]
async fn test_update_budget_rejects_negative_values() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();
    let original_daily_limit = server.state.kernel.effective_budget_config().max_daily_usd;

    let resp = client
        .put(format!("{}/api/budget", server.base_url))
        .json(&serde_json::json!({
            "max_daily_usd": -1.0
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("max_daily_usd cannot be negative"));
    assert_eq!(
        server.state.kernel.effective_budget_config().max_daily_usd,
        original_daily_limit
    );
}

#[tokio::test]
async fn test_update_budget_rejects_negative_hourly_limit() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .put(format!("{}/api/budget", server.base_url))
        .json(&serde_json::json!({
            "max_hourly_usd": -2.5,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("max_hourly_usd cannot be negative"));
}

#[tokio::test]
async fn test_update_budget_returns_update_status_when_usage_store_is_unusable() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    {
        let conn = server.state.kernel.memory.usage_conn();
        let conn = conn.lock().unwrap();
        conn.execute("DROP TABLE usage_events", []).unwrap();
    }

    let resp = client
        .put(format!("{}/api/budget", server.base_url))
        .json(&serde_json::json!({
            "max_daily_usd": 1.25
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "warning");
    assert_eq!(body["update_status"], "applied");
    assert_eq!(body["budget"]["max_daily_usd"], 1.25);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("budget status unavailable after update"));
    assert_eq!(
        server.state.kernel.effective_budget_config().max_daily_usd,
        1.25
    );
}

#[tokio::test]
async fn test_send_message_rejects_when_global_budget_is_exhausted() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let spawn = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": TEST_MANIFEST}))
        .send()
        .await
        .unwrap();
    let spawn_body: serde_json::Value = spawn.json().await.unwrap();
    let agent_id: openfang_types::agent::AgentId =
        spawn_body["agent_id"].as_str().unwrap().parse().unwrap();

    server
        .state
        .kernel
        .set_effective_budget_config(BudgetConfig {
            max_daily_usd: 0.01,
            ..BudgetConfig::default()
        });
    server
        .state
        .kernel
        .metering
        .record(&UsageRecord {
            agent_id,
            model: "test-model".to_string(),
            input_tokens: 1,
            output_tokens: 1,
            cost_usd: 0.02,
            tool_calls: 0,
        })
        .unwrap();

    let resp = client
        .post(format!(
            "{}/api/agents/{}/message",
            server.base_url, agent_id
        ))
        .json(&serde_json::json!({"message": "hello"}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 429);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("quota"));
}

#[tokio::test]
async fn test_send_message_stream_rejects_when_global_budget_is_exhausted() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let spawn = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": TEST_MANIFEST}))
        .send()
        .await
        .unwrap();
    let spawn_body: serde_json::Value = spawn.json().await.unwrap();
    let agent_id: openfang_types::agent::AgentId =
        spawn_body["agent_id"].as_str().unwrap().parse().unwrap();

    server
        .state
        .kernel
        .set_effective_budget_config(BudgetConfig {
            max_daily_usd: 0.01,
            ..BudgetConfig::default()
        });
    server
        .state
        .kernel
        .metering
        .record(&UsageRecord {
            agent_id,
            model: "test-model".to_string(),
            input_tokens: 1,
            output_tokens: 1,
            cost_usd: 0.02,
            tool_calls: 0,
        })
        .unwrap();

    let resp = client
        .post(format!(
            "{}/api/agents/{}/message/stream",
            server.base_url, agent_id
        ))
        .json(&serde_json::json!({"message": "hello"}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 429);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("quota"));
}

#[tokio::test]
async fn test_send_message_stream_emits_chunk_and_done_events() {
    if std::env::var("GROQ_API_KEY").is_err() {
        eprintln!("Skipping streaming success test: GROQ_API_KEY not set");
        return;
    }

    let server = start_test_server_with_llm().await;
    let client = reqwest::Client::new();

    let spawn = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": LLM_MANIFEST}))
        .send()
        .await
        .unwrap();
    let spawn_body: serde_json::Value = spawn.json().await.unwrap();
    let agent_id = spawn_body["agent_id"].as_str().unwrap().to_string();

    let resp = client
        .post(format!(
            "{}/api/agents/{}/message/stream",
            server.base_url, agent_id
        ))
        .json(&serde_json::json!({"message": "Say hello in exactly two words."}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert!(body.contains("event: chunk"), "missing chunk event: {body}");
    assert!(body.contains("event: done"), "missing done event: {body}");
    assert!(
        body.contains("event: timing"),
        "missing timing event: {body}"
    );
    assert!(
        body.contains("\"response\""),
        "missing final response payload: {body}"
    );
    assert!(
        body.find("event: timing").unwrap_or(usize::MAX)
            < body.rfind("event: done").unwrap_or_default(),
        "timing should arrive before the final done event: {body}"
    );
}

#[tokio::test]
async fn test_restart_agent_keeps_same_id_and_session() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let spawn = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": TEST_MANIFEST}))
        .send()
        .await
        .unwrap();
    let spawn_body: serde_json::Value = spawn.json().await.unwrap();
    let agent_id = spawn_body["agent_id"].as_str().unwrap().to_string();

    let before = server
        .state
        .kernel
        .registry
        .get(agent_id.parse().unwrap())
        .unwrap();
    let session_id = before.session_id.0.to_string();

    let resp = client
        .post(format!(
            "{}/api/agents/{}/restart",
            server.base_url, agent_id
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "restarted");
    assert_eq!(body["agent_id"], agent_id);

    let after = server
        .state
        .kernel
        .registry
        .get(agent_id.parse().unwrap())
        .unwrap();
    assert_eq!(after.id.0.to_string(), agent_id);
    assert_eq!(after.session_id.0.to_string(), session_id);
}

#[tokio::test]
async fn test_restart_agent_failure_keeps_agent_registered() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let spawn = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": TEST_MANIFEST}))
        .send()
        .await
        .unwrap();
    let spawn_body: serde_json::Value = spawn.json().await.unwrap();
    let agent_id: openfang_types::agent::AgentId =
        spawn_body["agent_id"].as_str().unwrap().parse().unwrap();

    {
        let conn = server.state.kernel.memory.usage_conn();
        let conn = conn.lock().unwrap();
        conn.execute("DROP TABLE agents", []).unwrap();
    }

    let resp = client
        .post(format!(
            "{}/api/agents/{}/restart",
            server.base_url, agent_id
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 500);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("Persistent state update failed"));
    assert!(server.state.kernel.registry.get(agent_id).is_some());
}

#[tokio::test]
async fn test_update_agent_budget_rejects_negative_values() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();
    let agent_id = server.state.kernel.registry.list()[0].id.to_string();

    let resp = client
        .put(format!(
            "{}/api/budget/agents/{}",
            server.base_url, agent_id
        ))
        .json(&serde_json::json!({
            "max_cost_per_day_usd": -1.0
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("cannot be negative"));
}

#[tokio::test]
async fn test_update_agent_budget_rolls_back_when_persistence_fails() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();
    let agent_id = server.state.kernel.registry.list()[0].id;
    let original_daily_limit = server
        .state
        .kernel
        .registry
        .get(agent_id)
        .unwrap()
        .manifest
        .resources
        .max_cost_per_day_usd;

    {
        let conn = server.state.kernel.memory.usage_conn();
        let conn = conn.lock().unwrap();
        conn.execute("DROP TABLE agents", []).unwrap();
    }

    let resp = client
        .put(format!(
            "{}/api/budget/agents/{}",
            server.base_url, agent_id
        ))
        .json(&serde_json::json!({
            "max_cost_per_day_usd": 12.5
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 500);
    let current_daily_limit = server
        .state
        .kernel
        .registry
        .get(agent_id)
        .unwrap()
        .manifest
        .resources
        .max_cost_per_day_usd;
    assert_eq!(current_daily_limit, original_daily_limit);
}

#[tokio::test]
async fn test_metrics_resolve_default_provider_labels() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": DEFAULT_MODEL_MANIFEST}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let body: serde_json::Value = resp.json().await.unwrap();
    let agent_id = body["agent_id"].as_str().unwrap().parse().unwrap();
    server.state.kernel.scheduler.record_usage(
        agent_id,
        &TokenUsage {
            input_tokens: 2,
            output_tokens: 3,
        },
    );

    let metrics = client
        .get(format!("{}/api/metrics", server.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert!(metrics.contains(
        "openfang_tokens_total{agent=\"metrics-default-agent\",provider=\"ollama\",model=\"test-model\"} 5"
    ));
    assert!(!metrics.contains(
        "openfang_tokens_total{agent=\"metrics-default-agent\",provider=\"default\",model=\"default\"}"
    ));
}

#[tokio::test]
async fn test_status_endpoint() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/status", server.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "running");
    assert_eq!(body["agent_count"], 1); // default assistant auto-spawned
    assert!(body["uptime_seconds"].is_number());
    assert_eq!(body["default_provider"], "ollama");
    assert_eq!(body["agents"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_spawn_list_kill_agent() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    // --- Spawn ---
    let resp = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": TEST_MANIFEST}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["name"], "test-agent");
    let agent_id = body["agent_id"].as_str().unwrap().to_string();
    let parsed_agent_id: openfang_types::agent::AgentId = agent_id.parse().unwrap();
    assert!(!agent_id.is_empty());

    // --- List (2 agents: default assistant + test-agent) ---
    let resp = client
        .get(format!("{}/api/agents", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let agents: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(agents.len(), 2);
    let test_agent = agents.iter().find(|a| a["name"] == "test-agent").unwrap();
    assert_eq!(test_agent["id"], agent_id);
    assert_eq!(test_agent["model_provider"], "ollama");

    // --- Kill ---
    let resp = client
        .delete(format!("{}/api/agents/{}", server.base_url, agent_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "killed");
    assert!(server
        .state
        .kernel
        .memory
        .load_agent(parsed_agent_id)
        .unwrap()
        .is_none());

    // --- List (only default assistant remains) ---
    let resp = client
        .get(format!("{}/api/agents", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let agents: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0]["name"], "assistant");
}

#[tokio::test]
async fn test_session_operations_persist_active_session_pointer() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let spawn = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": TEST_MANIFEST}))
        .send()
        .await
        .unwrap();
    let spawn_body: serde_json::Value = spawn.json().await.unwrap();
    let agent_id_str = spawn_body["agent_id"].as_str().unwrap().to_string();
    let agent_id: openfang_types::agent::AgentId = agent_id_str.parse().unwrap();

    let create = client
        .post(format!(
            "{}/api/agents/{}/sessions",
            server.base_url, agent_id_str
        ))
        .json(&serde_json::json!({"label": "smoke"}))
        .send()
        .await
        .unwrap();
    assert_eq!(create.status(), 200);
    let create_body: serde_json::Value = create.json().await.unwrap();
    let new_session_id = create_body["session_id"].as_str().unwrap().to_string();

    let persisted = server
        .state
        .kernel
        .memory
        .load_agent(agent_id)
        .unwrap()
        .unwrap();
    assert_eq!(persisted.session_id.0.to_string(), new_session_id);

    let switch = client
        .post(format!(
            "{}/api/agents/{}/sessions/{}/switch",
            server.base_url, agent_id_str, new_session_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(switch.status(), 200);

    let persisted = server
        .state
        .kernel
        .memory
        .load_agent(agent_id)
        .unwrap()
        .unwrap();
    assert_eq!(persisted.session_id.0.to_string(), new_session_id);
}

#[tokio::test]
async fn test_reset_and_clear_history_persist_new_session_pointer() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let spawn = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": TEST_MANIFEST}))
        .send()
        .await
        .unwrap();
    let spawn_body: serde_json::Value = spawn.json().await.unwrap();
    let agent_id_str = spawn_body["agent_id"].as_str().unwrap().to_string();
    let agent_id: openfang_types::agent::AgentId = agent_id_str.parse().unwrap();

    let detail = client
        .get(format!("{}/api/agents/{}", server.base_url, agent_id_str))
        .send()
        .await
        .unwrap();
    let detail_body: serde_json::Value = detail.json().await.unwrap();
    let original_session_id = detail_body["session_id"].as_str().unwrap().to_string();

    let reset = client
        .post(format!(
            "{}/api/agents/{}/session/reset",
            server.base_url, agent_id_str
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(reset.status(), 200);

    let after_reset = server
        .state
        .kernel
        .memory
        .load_agent(agent_id)
        .unwrap()
        .unwrap();
    let reset_session_id = after_reset.session_id.0.to_string();
    assert_ne!(reset_session_id, original_session_id);

    let clear = client
        .delete(format!(
            "{}/api/agents/{}/history",
            server.base_url, agent_id_str
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(clear.status(), 200);

    let after_clear = server
        .state
        .kernel
        .memory
        .load_agent(agent_id)
        .unwrap()
        .unwrap();
    assert_ne!(after_clear.session_id.0.to_string(), reset_session_id);
}

#[tokio::test]
async fn test_agent_session_empty() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    // Spawn agent
    let resp = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": TEST_MANIFEST}))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let agent_id = body["agent_id"].as_str().unwrap();

    // Session should be empty — no messages sent yet
    let resp = client
        .get(format!(
            "{}/api/agents/{}/session",
            server.base_url, agent_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["message_count"], 0);
    assert_eq!(body["messages"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_send_message_with_llm() {
    if std::env::var("GROQ_API_KEY").is_err() {
        eprintln!("GROQ_API_KEY not set, skipping LLM integration test");
        return;
    }

    let server = start_test_server_with_llm().await;
    let client = reqwest::Client::new();

    // Spawn
    let resp = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": LLM_MANIFEST}))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let agent_id = body["agent_id"].as_str().unwrap().to_string();

    // Send message through the real HTTP endpoint → kernel → Groq LLM
    let resp = client
        .post(format!(
            "{}/api/agents/{}/message",
            server.base_url, agent_id
        ))
        .json(&serde_json::json!({"message": "Say hello in exactly 3 words."}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let response_text = body["response"].as_str().unwrap();
    assert!(
        !response_text.is_empty(),
        "LLM response should not be empty"
    );
    assert!(body["input_tokens"].as_u64().unwrap() > 0);
    assert!(body["output_tokens"].as_u64().unwrap() > 0);

    // Session should now have messages
    let resp = client
        .get(format!(
            "{}/api/agents/{}/session",
            server.base_url, agent_id
        ))
        .send()
        .await
        .unwrap();
    let session: serde_json::Value = resp.json().await.unwrap();
    assert!(session["message_count"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn test_workflow_crud() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    // Spawn agent for workflow
    let resp = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": TEST_MANIFEST}))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let agent_name = body["name"].as_str().unwrap().to_string();

    // Create workflow
    let resp = client
        .post(format!("{}/api/workflows", server.base_url))
        .json(&serde_json::json!({
            "name": "test-workflow",
            "description": "Integration test workflow",
            "steps": [
                {
                    "name": "step1",
                    "agent_name": agent_name,
                    "prompt": "Echo: {{input}}",
                    "mode": "sequential",
                    "timeout_secs": 30
                }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    let workflow_id = body["workflow_id"].as_str().unwrap().to_string();
    assert!(!workflow_id.is_empty());
    let workflow_path = server
        ._tmp
        .path()
        .join("workflows")
        .join(format!("{workflow_id}.json"));
    assert!(workflow_path.exists());

    // List workflows
    let resp = client
        .get(format!("{}/api/workflows", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let workflows: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(workflows.len(), 1);
    assert_eq!(workflows[0]["name"], "test-workflow");
    assert_eq!(workflows[0]["steps"], 1);

    // Update workflow
    let resp = client
        .put(format!("{}/api/workflows/{}", server.base_url, workflow_id))
        .json(&serde_json::json!({
            "name": "test-workflow-updated",
            "description": "Updated integration workflow",
            "steps": [
                {
                    "name": "step1",
                    "agent_name": agent_name,
                    "prompt": "Echo updated: {{input}}",
                    "mode": "sequential",
                    "timeout_secs": 45
                }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let persisted: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&workflow_path).unwrap()).unwrap();
    assert_eq!(persisted["name"], "test-workflow-updated");
    assert_eq!(persisted["description"], "Updated integration workflow");

    // Delete workflow
    let resp = client
        .delete(format!("{}/api/workflows/{}", server.base_url, workflow_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert!(!workflow_path.exists());
}

#[tokio::test]
async fn test_trigger_crud() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    // Spawn agent for trigger
    let resp = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": TEST_MANIFEST}))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let agent_id = body["agent_id"].as_str().unwrap().to_string();

    // Create trigger (Lifecycle pattern — simplest variant)
    let resp = client
        .post(format!("{}/api/triggers", server.base_url))
        .json(&serde_json::json!({
            "agent_id": agent_id,
            "pattern": "lifecycle",
            "prompt_template": "Handle: {{event}}",
            "max_fires": 5
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.unwrap();
    let trigger_id = body["trigger_id"].as_str().unwrap().to_string();
    assert_eq!(body["agent_id"], agent_id);

    // List triggers (unfiltered)
    let resp = client
        .get(format!("{}/api/triggers", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggers: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(triggers.len(), 1);
    assert_eq!(triggers[0]["agent_id"], agent_id);
    assert_eq!(triggers[0]["enabled"], true);
    assert_eq!(triggers[0]["max_fires"], 5);

    // List triggers (filtered by agent_id)
    let resp = client
        .get(format!(
            "{}/api/triggers?agent_id={}",
            server.base_url, agent_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggers: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(triggers.len(), 1);

    // Delete trigger
    let resp = client
        .delete(format!("{}/api/triggers/{}", server.base_url, trigger_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // List triggers (should be empty)
    let resp = client
        .get(format!("{}/api/triggers", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let triggers: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(triggers.len(), 0);
}

#[tokio::test]
async fn test_invalid_agent_id_returns_400() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    // Send message to invalid ID
    let resp = client
        .post(format!("{}/api/agents/not-a-uuid/message", server.base_url))
        .json(&serde_json::json!({"message": "hello"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("Invalid"));

    // Kill invalid ID
    let resp = client
        .delete(format!("{}/api/agents/not-a-uuid", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    // Session for invalid ID
    let resp = client
        .get(format!("{}/api/agents/not-a-uuid/session", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_kill_nonexistent_agent_returns_404() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let resp = client
        .delete(format!("{}/api/agents/{}", server.base_url, fake_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_spawn_invalid_manifest_returns_400() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": "this is {{ not valid toml"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("Invalid manifest"));
}

#[tokio::test]
async fn test_request_id_header_is_uuid() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/health", server.base_url))
        .send()
        .await
        .unwrap();

    let request_id = resp
        .headers()
        .get("x-request-id")
        .expect("x-request-id header should be present");
    let id_str = request_id.to_str().unwrap();
    assert!(
        uuid::Uuid::parse_str(id_str).is_ok(),
        "x-request-id should be a valid UUID, got: {}",
        id_str
    );
}

#[tokio::test]
async fn test_multiple_agents_lifecycle() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    // Spawn 3 agents
    let mut ids = Vec::new();
    for i in 0..3 {
        let manifest = format!(
            r#"
name = "agent-{i}"
version = "0.1.0"
description = "Multi-agent test {i}"
author = "test"
module = "builtin:chat"

[model]
provider = "ollama"
model = "test-model"
system_prompt = "Agent {i}."

[capabilities]
memory_read = ["*"]
memory_write = ["self.*"]
"#
        );

        let resp = client
            .post(format!("{}/api/agents", server.base_url))
            .json(&serde_json::json!({"manifest_toml": manifest}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201);
        let body: serde_json::Value = resp.json().await.unwrap();
        ids.push(body["agent_id"].as_str().unwrap().to_string());
    }

    // List should show 4 (3 spawned + default assistant)
    let resp = client
        .get(format!("{}/api/agents", server.base_url))
        .send()
        .await
        .unwrap();
    let agents: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(agents.len(), 4);

    // Status should agree
    let resp = client
        .get(format!("{}/api/status", server.base_url))
        .send()
        .await
        .unwrap();
    let status: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(status["agent_count"], 4);

    // Kill one
    let resp = client
        .delete(format!("{}/api/agents/{}", server.base_url, ids[1]))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // List should show 3 (2 spawned + default assistant)
    let resp = client
        .get(format!("{}/api/agents", server.base_url))
        .send()
        .await
        .unwrap();
    let agents: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(agents.len(), 3);

    // Kill the rest
    for id in [&ids[0], &ids[2]] {
        client
            .delete(format!("{}/api/agents/{}", server.base_url, id))
            .send()
            .await
            .unwrap();
    }

    // List should have only default assistant
    let resp = client
        .get(format!("{}/api/agents", server.base_url))
        .send()
        .await
        .unwrap();
    let agents: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(agents.len(), 1);
}

// ---------------------------------------------------------------------------
// Auth integration tests
// ---------------------------------------------------------------------------

/// Start a test server with Bearer-token authentication enabled.
async fn start_test_server_with_auth(api_key: &str) -> TestServer {
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");

    let config = KernelConfig {
        home_dir: tmp.path().to_path_buf(),
        data_dir: tmp.path().join("data"),
        api_key: api_key.to_string(),
        default_model: DefaultModelConfig {
            provider: "ollama".to_string(),
            model: "test-model".to_string(),
            api_key_env: "OLLAMA_API_KEY".to_string(),
            base_url: None,
        },
        ..KernelConfig::default()
    };

    let kernel = OpenFangKernel::boot_with_config(config).expect("Kernel should boot");
    let kernel = Arc::new(kernel);
    kernel.set_self_handle();

    let state = Arc::new(AppState {
        kernel,
        started_at: Instant::now(),
        bridge_manager: tokio::sync::Mutex::new(None),
        channels_config: tokio::sync::RwLock::new(Default::default()),
        shutdown_notify: Arc::new(tokio::sync::Notify::new()),
        clawhub_cache: dashmap::DashMap::new(),
        provider_probe_cache: openfang_runtime::provider_health::ProbeCache::new(),
    });

    let api_key = state.kernel.config.api_key.trim().to_string();
    let auth_state = middleware::AuthState {
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

    let app = Router::new()
        .route("/", axum::routing::get(webchat::webchat_page))
        .route("/api/health", axum::routing::get(routes::health))
        .route(
            "/api/health/detail",
            axum::routing::get(routes::health_detail),
        )
        .route(
            "/api/metrics",
            axum::routing::get(routes::prometheus_metrics),
        )
        .route("/api/status", axum::routing::get(routes::status))
        .route("/api/commands", axum::routing::get(routes::list_commands))
        .route(
            "/api/providers/{name}/key",
            axum::routing::post(routes::set_provider_key).delete(routes::delete_provider_key),
        )
        .route(
            "/api/agents",
            axum::routing::get(routes::list_agents).post(routes::spawn_agent),
        )
        .route(
            "/api/agents/{id}/message",
            axum::routing::post(routes::send_message),
        )
        .route(
            "/api/agents/{id}/session",
            axum::routing::get(routes::get_agent_session),
        )
        .route("/api/agents/{id}/ws", axum::routing::get(ws::agent_ws))
        .route(
            "/api/agents/{id}",
            axum::routing::delete(routes::kill_agent),
        )
        .route(
            "/api/triggers",
            axum::routing::get(routes::list_triggers).post(routes::create_trigger),
        )
        .route(
            "/api/triggers/{id}",
            axum::routing::delete(routes::delete_trigger),
        )
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
        .route(
            "/api/workflows/{id}",
            axum::routing::get(routes::get_workflow)
                .put(routes::update_workflow)
                .delete(routes::delete_workflow),
        )
        .route("/api/shutdown", axum::routing::post(routes::shutdown))
        .layer(axum::middleware::from_fn_with_state(
            auth_state,
            middleware::auth,
        ))
        .layer(axum::middleware::from_fn(middleware::request_logging))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind test server");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    TestServer {
        base_url: format!("http://{}", addr),
        state,
        _tmp: tmp,
    }
}

/// Start a test server with session-based dashboard authentication enabled.
async fn start_test_server_with_session_auth(username: &str, password: &str) -> TestServer {
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");

    let config = KernelConfig {
        home_dir: tmp.path().to_path_buf(),
        data_dir: tmp.path().join("data"),
        auth: openfang_types::config::AuthConfig {
            enabled: true,
            username: username.to_string(),
            password_hash: openfang_api::session_auth::hash_password(password).unwrap(),
            session_ttl_hours: 24,
        },
        default_model: DefaultModelConfig {
            provider: "ollama".to_string(),
            model: "test-model".to_string(),
            api_key_env: "OLLAMA_API_KEY".to_string(),
            base_url: None,
        },
        ..KernelConfig::default()
    };

    let kernel = OpenFangKernel::boot_with_config(config).expect("Kernel should boot");
    let kernel = Arc::new(kernel);
    kernel.set_self_handle();

    let state = Arc::new(AppState {
        kernel,
        started_at: Instant::now(),
        bridge_manager: tokio::sync::Mutex::new(None),
        channels_config: tokio::sync::RwLock::new(Default::default()),
        shutdown_notify: Arc::new(tokio::sync::Notify::new()),
        clawhub_cache: dashmap::DashMap::new(),
        provider_probe_cache: openfang_runtime::provider_health::ProbeCache::new(),
    });

    let api_key = state.kernel.config.api_key.trim().to_string();
    let auth_state = middleware::AuthState {
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

    let app = Router::new()
        .route("/", axum::routing::get(webchat::webchat_page))
        .route("/api/health", axum::routing::get(routes::health))
        .route(
            "/api/health/detail",
            axum::routing::get(routes::health_detail),
        )
        .route("/api/status", axum::routing::get(routes::status))
        .route("/api/auth/login", axum::routing::post(routes::auth_login))
        .route("/api/auth/logout", axum::routing::post(routes::auth_logout))
        .route("/api/auth/check", axum::routing::get(routes::auth_check))
        .route(
            "/api/triggers",
            axum::routing::get(routes::list_triggers).post(routes::create_trigger),
        )
        .layer(axum::middleware::from_fn_with_state(
            auth_state,
            middleware::auth,
        ))
        .layer(axum::middleware::from_fn(middleware::request_logging))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind test server");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    TestServer {
        base_url: format!("http://{}", addr),
        state,
        _tmp: tmp,
    }
}

#[tokio::test]
async fn test_auth_health_is_public() {
    let server = start_test_server_with_auth("secret-key-123").await;
    let client = reqwest::Client::new();

    // /api/health should be accessible without auth
    let resp = client
        .get(format!("{}/api/health", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_dashboard_shell_is_public_and_contains_expected_markers() {
    let server = start_test_server_with_auth("secret-key-123").await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert!(resp.headers().contains_key("x-request-id"));
    assert!(resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/html")));

    let body = resp.text().await.unwrap();
    assert!(body.contains("<title>OpenFang Dashboard</title>"));
    assert!(body.contains("<body x-data=\"app\""));
}

#[tokio::test]
async fn test_auth_health_detail_requires_auth() {
    let server = start_test_server_with_auth("secret-key-123").await;
    let client = reqwest::Client::new();

    let unauthenticated = client
        .get(format!("{}/api/health/detail", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), 401);
    assert!(unauthenticated.headers().contains_key("x-request-id"));

    let authenticated = client
        .get(format!("{}/api/health/detail", server.base_url))
        .header("authorization", "Bearer secret-key-123")
        .send()
        .await
        .unwrap();
    assert_eq!(authenticated.status(), 200);
}

#[tokio::test]
async fn test_auth_metrics_requires_auth() {
    let server = start_test_server_with_auth("secret-key-123").await;
    let client = reqwest::Client::new();

    let unauthenticated = client
        .get(format!("{}/api/metrics", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), 401);

    let authenticated = client
        .get(format!("{}/api/metrics", server.base_url))
        .header("authorization", "Bearer secret-key-123")
        .send()
        .await
        .unwrap();
    assert_eq!(authenticated.status(), 200);
}

#[tokio::test]
async fn test_auth_rejects_no_token() {
    let server = start_test_server_with_auth("secret-key-123").await;
    let client = reqwest::Client::new();

    // Protected endpoint without auth header → 401
    // Note: /api/status is public (dashboard needs it), so use a protected endpoint
    let resp = client
        .get(format!("{}/api/commands", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("Missing"));
}

#[tokio::test]
async fn test_auth_rejects_wrong_token() {
    let server = start_test_server_with_auth("secret-key-123").await;
    let client = reqwest::Client::new();

    // Wrong bearer token → 401
    // Note: /api/status is public (dashboard needs it), so use a protected endpoint
    let resp = client
        .get(format!("{}/api/commands", server.base_url))
        .header("authorization", "Bearer wrong-key")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("Invalid"));
}

#[tokio::test]
async fn test_commands_endpoint_describes_new_session_command() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/commands", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    let commands = body["commands"].as_array().expect("commands array missing");
    let new_command = commands
        .iter()
        .find(|command| command["cmd"].as_str() == Some("/new"))
        .expect("/new command missing");

    assert_eq!(
        new_command["desc"].as_str(),
        Some("Start a new conversation (clear history)")
    );
}

#[tokio::test]
async fn test_custom_provider_key_rejects_invalid_provider_name() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/providers/my.provider/key", server.base_url))
        .json(&serde_json::json!({"key": "secret-value"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"]
        .as_str()
        .unwrap_or_default()
        .contains("Invalid custom provider name"));
}

#[tokio::test]
async fn test_custom_provider_key_accepts_numeric_provider_name() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/providers/111/key", server.base_url))
        .json(&serde_json::json!({"key": "secret-value"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "saved");
    assert_eq!(body["provider"], "111");
}

#[tokio::test]
async fn test_auth_accepts_correct_token() {
    let server = start_test_server_with_auth("secret-key-123").await;
    let client = reqwest::Client::new();

    // Correct bearer token → 200
    let resp = client
        .get(format!("{}/api/status", server.base_url))
        .header("authorization", "Bearer secret-key-123")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "running");
}

#[tokio::test]
async fn test_auth_disabled_when_no_key() {
    // Empty API key still permits loopback access to protected routes.
    let server = start_test_server_with_auth("").await;
    let client = reqwest::Client::new();

    // Protected endpoint remains accessible from loopback when no key is configured.
    let resp = client
        .get(format!("{}/api/status", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_session_login_allows_access_to_protected_endpoint() {
    let server = start_test_server_with_session_auth("admin", "secret123").await;
    let client = reqwest::Client::new();

    let login = client
        .post(format!("{}/api/auth/login", server.base_url))
        .json(&serde_json::json!({
            "username": "admin",
            "password": "secret123",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(login.status(), 200);
    let cookie = login
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap()
        .to_string();
    let login_body: serde_json::Value = login.json().await.unwrap();
    assert_eq!(login_body["status"], "ok");
    assert!(login_body.get("token").is_none());
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Strict"));

    let protected = client
        .get(format!("{}/api/triggers", server.base_url))
        .header(reqwest::header::COOKIE, cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(protected.status(), 200);
}

#[tokio::test]
async fn test_session_login_rejects_invalid_password() {
    let server = start_test_server_with_session_auth("admin", "secret123").await;
    let client = reqwest::Client::new();

    let login = client
        .post(format!("{}/api/auth/login", server.base_url))
        .json(&serde_json::json!({
            "username": "admin",
            "password": "wrong-password",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(login.status(), 401);
}

#[tokio::test]
async fn test_session_auth_rejects_empty_api_key_header_when_no_api_key_is_configured() {
    let server = start_test_server_with_session_auth("admin", "secret123").await;
    let client = reqwest::Client::new();

    let protected = client
        .get(format!("{}/api/triggers", server.base_url))
        .header("x-api-key", "")
        .send()
        .await
        .unwrap();

    assert_eq!(protected.status(), 401);
}

#[tokio::test]
async fn test_session_login_sets_secure_cookie_for_https_proxy() {
    let server = start_test_server_with_session_auth("admin", "secret123").await;
    let client = reqwest::Client::new();

    let login = client
        .post(format!("{}/api/auth/login", server.base_url))
        .header("x-forwarded-proto", "https")
        .json(&serde_json::json!({
            "username": "admin",
            "password": "secret123",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(login.status(), 200);
    let cookie = login
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap();
    assert!(cookie.contains("Secure"));
}

#[tokio::test]
async fn test_build_router_auth_and_metrics_wiring() {
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");
    let config = KernelConfig {
        home_dir: tmp.path().to_path_buf(),
        data_dir: tmp.path().join("data"),
        auth: openfang_types::config::AuthConfig {
            enabled: true,
            username: "admin".to_string(),
            password_hash: openfang_api::session_auth::hash_password("secret123").unwrap(),
            session_ttl_hours: 24,
        },
        default_model: DefaultModelConfig {
            provider: "ollama".to_string(),
            model: "test-model".to_string(),
            api_key_env: "OLLAMA_API_KEY".to_string(),
            base_url: None,
        },
        ..KernelConfig::default()
    };
    let kernel = Arc::new(OpenFangKernel::boot_with_config(config).unwrap());
    kernel.set_self_handle();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind test server");
    let addr = listener.local_addr().unwrap();
    let (app, state) = server::build_router(kernel, addr).await;

    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    let server = TestServer {
        base_url: format!("http://{}", addr),
        state,
        _tmp: tmp,
    };
    let client = reqwest::Client::new();

    let unauthenticated = client
        .get(format!("{}/api/metrics", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), 401);

    let login = client
        .post(format!("{}/api/auth/login", server.base_url))
        .json(&serde_json::json!({
            "username": "admin",
            "password": "secret123",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), 200);
    let cookie = login
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap()
        .to_string();

    let auth_check = client
        .get(format!("{}/api/auth/check", server.base_url))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(auth_check.status(), 200);

    let metrics = client
        .get(format!("{}/api/metrics", server.base_url))
        .header(reqwest::header::COOKIE, cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(metrics.status(), 200);
}
