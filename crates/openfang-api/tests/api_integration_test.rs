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
use openfang_api::ws;
use openfang_kernel::OpenFangKernel;
use openfang_memory::MemorySubstrate;
use openfang_types::config::{ChannelsConfig, DefaultModelConfig, KernelConfig, TelegramConfig};
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

    assert_eq!(resp.status(), 200);
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

    assert_eq!(resp.status(), 200);
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

    assert_eq!(resp.status(), 200);
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

    assert_eq!(resp.status(), 200);
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

    assert_eq!(resp.status(), 200);
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

    assert_eq!(resp.status(), 200);
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
async fn test_metrics_expose_readiness_gauges() {
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
    assert!(body.contains("openfang_config_warnings"));
    assert!(body.contains("openfang_restore_warnings"));
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
        .route("/api/commands", axum::routing::get(|| async { "ok" }))
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
async fn test_auth_health_detail_requires_auth() {
    let server = start_test_server_with_auth("secret-key-123").await;
    let client = reqwest::Client::new();

    let unauthenticated = client
        .get(format!("{}/api/health/detail", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), 401);

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
