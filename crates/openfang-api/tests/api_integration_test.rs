//! Real HTTP integration tests for the OpenFang API.
//!
//! These tests boot a real kernel, start a real axum HTTP server on a random
//! port, and hit actual endpoints with reqwest.  No mocking.
//!
//! Tests that require an LLM API call are gated behind GROQ_API_KEY.
//!
//! Run: cargo test -p openfang-api --test api_integration_test -- --nocapture

use axum::Router;
use futures::Stream;
use openfang_api::middleware;
use openfang_api::routes::{self, AppState};
use openfang_api::ws;
use openfang_channels::types::{
    ChannelAdapter, ChannelContent, ChannelMessage, ChannelStatus, ChannelType, ChannelUser,
};
use openfang_kernel::OpenFangKernel;
use openfang_memory::usage::UsageRecord;
use openfang_types::config::{DefaultModelConfig, KernelConfig};
use reqwest::StatusCode;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

// ---------------------------------------------------------------------------
// Test infrastructure
// ---------------------------------------------------------------------------

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

    let kernel = OpenFangKernel::boot_with_config(config).expect("Kernel should boot");
    let kernel = Arc::new(kernel);
    kernel.set_self_handle();

    let state = Arc::new(AppState {
        kernel,
        started_at: Instant::now(),
        peer_registry: None,
        bridge_manager: tokio::sync::Mutex::new(None),
        channels_config: tokio::sync::RwLock::new(Default::default()),
        shutdown_notify: Arc::new(tokio::sync::Notify::new()),
        clawhub_cache: dashmap::DashMap::new(),
        provider_probe_cache: openfang_runtime::provider_health::ProbeCache::new(),
    });

    let app = Router::new()
        .route("/api/health", axum::routing::get(routes::health))
        .route("/api/status", axum::routing::get(routes::status))
        .route("/api/usage", axum::routing::get(routes::usage_stats))
        .route(
            "/api/mcp/servers",
            axum::routing::get(routes::list_mcp_servers).post(routes::add_mcp_server),
        )
        .route(
            "/api/mcp/servers/{name}",
            axum::routing::put(routes::update_mcp_server).delete(routes::delete_mcp_server),
        )
        .route("/api/profiles", axum::routing::get(routes::list_profiles))
        .route(
            "/api/profiles/{name}",
            axum::routing::get(routes::get_profile),
        )
        .route("/api/tools", axum::routing::get(routes::list_tools))
        .route("/api/tools/{name}", axum::routing::get(routes::get_tool))
        .route(
            "/api/agents",
            axum::routing::get(routes::list_agents).post(routes::spawn_agent),
        )
        .route(
            "/api/routing/capabilities",
            axum::routing::get(routes::list_routing_capabilities),
        )
        .route(
            "/api/routing/proposals",
            axum::routing::post(routes::create_routing_proposal),
        )
        .route(
            "/api/routing/proposals/apply",
            axum::routing::post(routes::apply_routing_proposal),
        )
        .route(
            "/api/routing/proposals/jobs",
            axum::routing::get(routes::list_routing_proposal_jobs),
        )
        .route(
            "/api/routing/proposals/jobs/{id}",
            axum::routing::get(routes::get_routing_proposal_job),
        )
        .route(
            "/api/agents/{id}/message",
            axum::routing::post(routes::send_message),
        )
        .route(
            "/api/agents/{id}/upload",
            axum::routing::post(routes::upload_file),
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
        .route("/api/comms/send", axum::routing::post(routes::comms_send))
        .route(
            "/api/cron/jobs",
            axum::routing::get(routes::list_cron_jobs).post(routes::create_cron_job),
        )
        .route(
            "/api/cron/jobs/{id}",
            axum::routing::delete(routes::delete_cron_job).put(routes::update_cron_job),
        )
        .route("/api/backup", axum::routing::post(routes::create_backup))
        .route("/api/backups", axum::routing::get(routes::list_backups))
        .route(
            "/api/backups/{filename}",
            axum::routing::delete(routes::delete_backup),
        )
        .route("/api/restore", axum::routing::post(routes::restore_backup))
        .route(
            "/api/a2a/agents",
            axum::routing::get(routes::a2a_list_external_agents),
        )
        .route(
            "/api/a2a/agents/{*id}",
            axum::routing::get(routes::a2a_get_external_agent),
        )
        .route(
            "/api/routing/decisions",
            axum::routing::get(routes::list_decision_traces),
        )
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
            "/api/integrations/health",
            axum::routing::get(routes::integrations_health),
        )
        .route(
            "/api/integrations/reload",
            axum::routing::post(routes::reload_integrations),
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
        axum::serve(listener, app).await.unwrap();
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

#[derive(Clone, Debug)]
struct SentRecord {
    user: ChannelUser,
    content: ChannelContent,
    thread_id: Option<String>,
}

struct MockChannelAdapter {
    sent: Arc<std::sync::Mutex<Vec<SentRecord>>>,
}

#[async_trait::async_trait]
impl ChannelAdapter for MockChannelAdapter {
    fn name(&self) -> &str {
        "mock"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("mock".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        Ok(Box::pin(futures::stream::empty()))
    }

    async fn send(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.sent.lock().unwrap().push(SentRecord {
            user: user.clone(),
            content,
            thread_id: None,
        });
        Ok(())
    }

    async fn send_in_thread(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
        thread_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.sent.lock().unwrap().push(SentRecord {
            user: user.clone(),
            content,
            thread_id: Some(thread_id.to_string()),
        });
        Ok(())
    }

    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    fn status(&self) -> ChannelStatus {
        ChannelStatus::default()
    }
}

async fn spawn_test_agent(server: &TestServer, client: &reqwest::Client) -> String {
    let response = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": TEST_MANIFEST}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);

    let body: serde_json::Value = response.json().await.unwrap();
    body["agent_id"].as_str().unwrap().to_string()
}

async fn create_cron_job_for_agent(
    server: &TestServer,
    client: &reqwest::Client,
    agent_id: &str,
    name: &str,
) -> String {
    let response = client
        .post(format!("{}/api/cron/jobs", server.base_url))
        .json(&serde_json::json!({
            "agent_id": agent_id,
            "name": name,
            "schedule": { "kind": "cron", "expr": "0 * * * *" },
            "action": { "kind": "agent_turn", "message": "Check status" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);

    let list_response = client
        .get(format!("{}/api/cron/jobs", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(list_response.status(), 200);

    let body: serde_json::Value = list_response.json().await.unwrap();
    body["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|job| job["name"] == name)
        .and_then(|job| job["id"].as_str())
        .unwrap()
        .to_string()
}

async fn upload_test_attachment(
    server: &TestServer,
    client: &reqwest::Client,
    agent_id: &str,
    filename: &str,
    content_type: &str,
    data: Vec<u8>,
) -> serde_json::Value {
    let response = client
        .post(format!(
            "{}/api/agents/{}/upload",
            server.base_url, agent_id
        ))
        .header(reqwest::header::CONTENT_TYPE, content_type)
        .header("X-Filename", filename)
        .body(data)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    response.json().await.unwrap()
}

fn install_mock_channel_adapter(server: &TestServer) -> Arc<std::sync::Mutex<Vec<SentRecord>>> {
    let sent = Arc::new(std::sync::Mutex::new(Vec::new()));
    server.state.kernel.channel_adapters.insert(
        "mock".to_string(),
        Arc::new(MockChannelAdapter { sent: sent.clone() }),
    );
    sent
}

fn write_test_config(server: &TestServer) {
    let config_path = server.state.kernel.config.home_dir.join("config.toml");
    let content = toml::to_string_pretty(&server.state.kernel.config)
        .expect("test kernel config should serialize");
    std::fs::write(config_path, content).expect("test config.toml should be written");
}

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
async fn test_get_profile_endpoint() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/api/profiles/research", server.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["name"], "research");
    assert!(body["tools"].is_array());
}

#[tokio::test]
async fn test_get_tool_endpoint() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/api/tools/file_read", server.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["name"], "file_read");
    assert_eq!(body["source"], "builtin");
    assert!(body["input_schema"].is_object());
}

#[tokio::test]
async fn test_list_routing_capabilities_endpoint() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    server
        .state
        .kernel
        .spawn_agent(openfang_types::agent::AgentManifest {
            name: "orchestrator".to_string(),
            module: "builtin:router".to_string(),
            ..Default::default()
        })
        .unwrap();
    server
        .state
        .kernel
        .spawn_agent(openfang_types::agent::AgentManifest {
            name: "coder".to_string(),
            description: "Implements patches and code changes".to_string(),
            tags: vec!["coding".to_string()],
            ..Default::default()
        })
        .unwrap();

    server
        .state
        .kernel
        .register_workflow(openfang_kernel::workflow::Workflow {
            id: openfang_kernel::workflow::WorkflowId::new(),
            name: "daily report".to_string(),
            description: "Generate a daily report".to_string(),
            steps: vec![openfang_kernel::workflow::WorkflowStep {
                name: "summarize".to_string(),
                agent: openfang_kernel::workflow::StepAgent::ByName {
                    name: "coder".to_string(),
                },
                prompt_template: "{{input}}".to_string(),
                mode: openfang_kernel::workflow::StepMode::Sequential,
                timeout_secs: 30,
                error_mode: openfang_kernel::workflow::ErrorMode::Fail,
                output_var: None,
            }],
            created_at: chrono::Utc::now(),
        })
        .await;

    let response = client
        .get(format!("{}/api/routing/capabilities", server.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let caps = body["capabilities"].as_array().unwrap();
    assert!(caps
        .iter()
        .any(|cap| cap["kind"] == "hand" && cap["id"] == "browser"));
    assert!(caps
        .iter()
        .any(|cap| cap["kind"] == "workflow" && cap["name"] == "daily report"));
    assert!(caps
        .iter()
        .any(|cap| cap["kind"] == "agent" && cap["name"] == "coder"));
}

#[tokio::test]
async fn test_usage_stats_includes_agent_costs() {
    let server = start_test_server().await;
    let agent = server
        .state
        .kernel
        .registry
        .list()
        .into_iter()
        .next()
        .expect("expected bundled agent");
    let agent_id = agent.id;
    let agent_id_str = agent_id.to_string();

    server
        .state
        .kernel
        .memory
        .usage()
        .record(&UsageRecord {
            agent_id,
            model: "test-model".to_string(),
            input_tokens: 120,
            output_tokens: 30,
            cost_usd: 0.0125,
            tool_calls: 1,
        })
        .expect("usage record should be stored");

    let body: serde_json::Value = reqwest::get(format!("{}/api/usage", server.base_url))
        .await
        .expect("request should succeed")
        .json()
        .await
        .expect("response should be json");

    let entry = body["agents"]
        .as_array()
        .and_then(|agents| {
            agents
                .iter()
                .find(|candidate| candidate["agent_id"].as_str() == Some(agent_id_str.as_str()))
        })
        .expect("agent usage entry should exist");

    assert_eq!(entry["cost_usd"].as_f64().unwrap_or_default(), 0.0125);
}

#[tokio::test]
async fn test_create_routing_proposal_endpoint() {
    let server = start_test_server().await;

    server
        .state
        .kernel
        .spawn_agent(openfang_types::agent::AgentManifest {
            name: "orchestrator".to_string(),
            description: "Native router".to_string(),
            module: "builtin:router".to_string(),
            ..Default::default()
        })
        .expect("router should spawn");

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/api/routing/proposals", server.base_url))
        .json(&serde_json::json!({
            "message": "automate contract renewal approval escalation weekly"
        }))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("json body");
    assert_eq!(body["gap_detected"], true);
    assert_eq!(body["proposal"]["kind"], "workflow");
    assert_eq!(body["proposal"]["approval_required"], true);
}

#[tokio::test]
async fn test_apply_routing_proposal_creates_workflow_after_approval() {
    let server = start_test_server().await;

    server
        .state
        .kernel
        .spawn_agent(openfang_types::agent::AgentManifest {
            name: "orchestrator".to_string(),
            description: "Native router".to_string(),
            module: "builtin:router".to_string(),
            ..Default::default()
        })
        .expect("router should spawn");

    let client = reqwest::Client::new();
    let apply_response = client
        .post(format!("{}/api/routing/proposals/apply", server.base_url))
        .json(&serde_json::json!({
            "proposal": {
                "kind": "workflow",
                "name": "automate-contract-renewal-approval",
                "description": "Workflow for automate contract renewal approval escalation weekly",
                "rationale": "test duplicate",
                "approval_required": true,
                "tags": ["builder:draft", "workflow"],
                "keywords": ["automate", "contract", "renewal", "approval"],
                "suggested_tools": [],
                "workflow": {
                    "trigger": "user-invoked goal",
                    "steps": [
                        {
                            "name": "execute",
                            "purpose": "perform the work",
                            "agent_name_hint": "assistant",
                            "prompt_template": "{{input}}"
                        }
                    ]
                }
            },
            "activate_after_create": false
        }))
        .send()
        .await
        .expect("apply request should succeed");
    assert_eq!(apply_response.status(), StatusCode::CREATED);
    let apply_body: serde_json::Value = apply_response.json().await.expect("apply json");
    let approval_id = apply_body["approval_id"]
        .as_str()
        .expect("approval id")
        .to_string();
    let job_id = apply_body["job_id"].as_str().expect("job id").to_string();

    let approve_response = client
        .post(format!(
            "{}/api/approvals/{}/approve",
            server.base_url, approval_id
        ))
        .send()
        .await
        .expect("approve request should succeed");
    assert_eq!(approve_response.status(), StatusCode::OK);

    let mut final_job = None;
    for _ in 0..20 {
        let response = client
            .get(format!(
                "{}/api/routing/proposals/jobs/{}",
                server.base_url, job_id
            ))
            .send()
            .await
            .expect("job status request should succeed");
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = response.json().await.expect("job json");
        if body["status"] == "applied" {
            final_job = Some(body);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    let final_job = final_job.expect("job should complete");
    assert_eq!(final_job["status"], "applied");
    assert_eq!(final_job["outcome"]["kind"], "workflow");
    assert_eq!(
        server.state.kernel.workflows.list_workflows().await.len(),
        1
    );

    let jobs_response = client
        .get(format!("{}/api/routing/proposals/jobs", server.base_url))
        .send()
        .await
        .expect("jobs list request should succeed");
    assert_eq!(jobs_response.status(), StatusCode::OK);
    let jobs_body: serde_json::Value = jobs_response.json().await.expect("jobs list json");
    assert!(jobs_body["total"].as_u64().unwrap_or(0) >= 1);
}

#[tokio::test]
async fn test_apply_routing_proposal_fails_for_duplicate_workflow_name() {
    let server = start_test_server().await;

    server
        .state
        .kernel
        .spawn_agent(openfang_types::agent::AgentManifest {
            name: "orchestrator".to_string(),
            description: "Native router".to_string(),
            module: "builtin:router".to_string(),
            ..Default::default()
        })
        .expect("router should spawn");

    server
        .state
        .kernel
        .register_workflow(openfang_kernel::workflow::Workflow {
            id: openfang_kernel::workflow::WorkflowId(uuid::Uuid::new_v4()),
            name: "automate-contract-renewal-approval".to_string(),
            description: "Existing workflow".to_string(),
            steps: vec![],
            created_at: chrono::Utc::now(),
        })
        .await;

    let client = reqwest::Client::new();
    let apply_response = client
        .post(format!("{}/api/routing/proposals/apply", server.base_url))
        .json(&serde_json::json!({
            "proposal": {
                "kind": "workflow",
                "name": "automate-contract-renewal-approval",
                "description": "Workflow for automate contract renewal approval escalation weekly",
                "rationale": "duplicate test",
                "approval_required": true,
                "tags": ["builder:draft", "workflow"],
                "keywords": ["automate", "contract", "renewal", "approval"],
                "suggested_tools": [],
                "workflow": {
                    "trigger": "user-invoked goal",
                    "steps": [
                        {
                            "name": "execute",
                            "purpose": "perform the work",
                            "agent_name_hint": "assistant",
                            "prompt_template": "{{input}}"
                        }
                    ]
                }
            },
            "activate_after_create": false
        }))
        .send()
        .await
        .expect("apply request should succeed");
    assert_eq!(apply_response.status(), StatusCode::CREATED);
    let apply_body: serde_json::Value = apply_response.json().await.expect("apply json");
    let approval_id = apply_body["approval_id"]
        .as_str()
        .expect("approval id")
        .to_string();
    let job_id = apply_body["job_id"].as_str().expect("job id").to_string();

    let approve_response = client
        .post(format!(
            "{}/api/approvals/{}/approve",
            server.base_url, approval_id
        ))
        .send()
        .await
        .expect("approve request should succeed");
    assert_eq!(approve_response.status(), StatusCode::OK);

    let mut final_job = None;
    for _ in 0..20 {
        let response = client
            .get(format!(
                "{}/api/routing/proposals/jobs/{}",
                server.base_url, job_id
            ))
            .send()
            .await
            .expect("job status request should succeed");
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = response.json().await.expect("job json");
        if body["status"] == "failed" {
            final_job = Some(body);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    let final_job = final_job.expect("job should fail");
    assert_eq!(final_job["status"], "failed");
    assert!(final_job["error"]
        .as_str()
        .unwrap_or("")
        .contains("already exists"));
}

#[tokio::test]
async fn test_get_external_a2a_agent_endpoint() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    {
        let mut agents = server
            .state
            .kernel
            .a2a_external_agents
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        agents.push((
            "https://example.com".to_string(),
            openfang_runtime::a2a::AgentCard {
                name: "demo-agent".to_string(),
                description: "Demo external agent".to_string(),
                url: "https://example.com".to_string(),
                version: "1.0.0".to_string(),
                capabilities: openfang_runtime::a2a::AgentCapabilities::default(),
                skills: vec![openfang_runtime::a2a::AgentSkill {
                    id: "research".to_string(),
                    name: "Research".to_string(),
                    description: "Research capability".to_string(),
                    tags: vec![],
                    examples: vec![],
                }],
                default_input_modes: vec![],
                default_output_modes: vec![],
            },
        ));
    }

    let response = client
        .get(format!("{}/api/a2a/agents/demo-agent", server.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["name"], "demo-agent");
    assert_eq!(body["url"], "https://example.com");
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
async fn test_update_cron_job_endpoint() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let agent_id = spawn_test_agent(&server, &client).await;
    let job_id = create_cron_job_for_agent(&server, &client, &agent_id, "hourly-check").await;

    let response = client
        .put(format!("{}/api/cron/jobs/{}", server.base_url, job_id))
        .json(&serde_json::json!({
            "name": "heartbeat-job",
            "enabled": false,
            "schedule": { "kind": "every", "every_secs": 900 },
            "action": { "kind": "system_event", "text": "heartbeat" },
            "delivery": { "kind": "webhook", "url": "https://example.com/hook" }
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["id"], job_id);
    assert_eq!(body["name"], "heartbeat-job");
    assert_eq!(body["enabled"], false);
    assert_eq!(body["schedule"]["kind"], "every");
    assert_eq!(body["schedule"]["every_secs"], 900);
    assert_eq!(body["action"]["kind"], "system_event");
    assert_eq!(body["delivery"]["kind"], "webhook");

    let list_response = client
        .get(format!("{}/api/cron/jobs", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(list_response.status(), 200);
    let list_body: serde_json::Value = list_response.json().await.unwrap();
    assert_eq!(list_body["total"], 1);
    let jobs = list_body["jobs"].as_array().unwrap();
    assert_eq!(jobs[0]["name"], "heartbeat-job");
    assert_eq!(jobs[0]["enabled"], false);
}

#[tokio::test]
async fn test_update_cron_job_invalid_input_returns_400() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let agent_id = spawn_test_agent(&server, &client).await;
    let job_id = create_cron_job_for_agent(&server, &client, &agent_id, "invalid-update").await;

    let response = client
        .put(format!("{}/api/cron/jobs/{}", server.base_url, job_id))
        .json(&serde_json::json!({
            "schedule": { "kind": "every", "every_secs": 30 }
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 400);
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("every_secs too small"));
}

#[tokio::test]
async fn test_update_cron_job_not_found_returns_404() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let response = client
        .put(format!(
            "{}/api/cron/jobs/{}",
            server.base_url,
            uuid::Uuid::new_v4()
        ))
        .json(&serde_json::json!({
            "name": "missing-job"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("not found"));
}

#[tokio::test]
async fn test_backup_restore_endpoints_round_trip() {
    let server = start_test_server().await;
    write_test_config(&server);
    let client = reqwest::Client::new();

    let home = &server.state.kernel.config.home_dir;
    std::fs::write(home.join("custom_models.json"), "{\"models\":[]}").unwrap();
    std::fs::create_dir_all(home.join("workspaces").join("backup-agent")).unwrap();
    std::fs::write(
        home.join("workspaces").join("backup-agent").join("SOUL.md"),
        "workspace before backup",
    )
    .unwrap();

    let create = client
        .post(format!("{}/api/backup", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(create.status(), 200);
    let create_body: serde_json::Value = create.json().await.unwrap();
    let filename = create_body["filename"].as_str().unwrap().to_string();
    assert!(create_body["components"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "config"));
    assert!(home.join("backups").join(&filename).exists());

    let list = client
        .get(format!("{}/api/backups", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(list.status(), 200);
    let list_body: serde_json::Value = list.json().await.unwrap();
    assert_eq!(list_body["total"], 1);
    assert_eq!(list_body["backups"][0]["filename"], filename);

    std::fs::write(home.join("config.toml"), "log_level = \"error\"\n").unwrap();
    std::fs::write(
        home.join("workspaces").join("backup-agent").join("SOUL.md"),
        "workspace after mutation",
    )
    .unwrap();

    let restore = client
        .post(format!("{}/api/restore", server.base_url))
        .json(&serde_json::json!({ "filename": filename }))
        .send()
        .await
        .unwrap();
    assert_eq!(restore.status(), 200);
    let restore_body: serde_json::Value = restore.json().await.unwrap();
    assert_eq!(restore_body["restart_required"], true);
    assert_eq!(
        std::fs::read_to_string(home.join("workspaces").join("backup-agent").join("SOUL.md"))
            .unwrap(),
        "workspace before backup"
    );
    assert!(std::fs::read_to_string(home.join("config.toml"))
        .unwrap()
        .contains("home_dir"));

    let delete = client
        .delete(format!("{}/api/backups/{}", server.base_url, filename))
        .send()
        .await
        .unwrap();
    assert_eq!(delete.status(), 200);

    let list_after_delete = client
        .get(format!("{}/api/backups", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(list_after_delete.status(), 200);
    let list_after_delete_body: serde_json::Value = list_after_delete.json().await.unwrap();
    assert_eq!(list_after_delete_body["total"], 0);
}

#[tokio::test]
async fn test_restore_partial_failure_returns_500() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();
    let backups_dir = server.state.kernel.config.home_dir.join("backups");
    std::fs::create_dir_all(&backups_dir).unwrap();
    let backup_path = backups_dir.join("partial_restore.zip");

    let file = std::fs::File::create(&backup_path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let manifest = openfang_types::backup::BackupManifest {
        format_version: 1,
        product: "openfang".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        hostname: "test-host".to_string(),
        openfang_version: env!("CARGO_PKG_VERSION").to_string(),
        components: vec!["config".to_string()],
        omitted_components: Vec::new(),
        archive_files: vec!["config.toml".to_string()],
        archive_directories: Vec::new(),
    };
    use std::io::Write as _;
    zip.start_file("manifest.json", options).unwrap();
    zip.write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
        .unwrap();
    zip.start_file("config.toml", options).unwrap();
    zip.write_all(b"log_level = \"debug\"\n").unwrap();
    zip.start_file("rogue.txt", options).unwrap();
    zip.write_all(b"should trigger partial restore").unwrap();
    zip.finish().unwrap();

    let response = client
        .post(format!("{}/api/restore", server.base_url))
        .json(&serde_json::json!({ "filename": "partial_restore.zip" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 500);
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("Restore completed with 1 file error"));
    assert_eq!(body["restored_files"], 1);
    assert_eq!(body["errors"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_mcp_server_crud_persists_and_lists() {
    let server = start_test_server().await;
    write_test_config(&server);
    let client = reqwest::Client::new();

    let create = client
        .post(format!("{}/api/mcp/servers", server.base_url))
        .json(&serde_json::json!({
            "name": "github-sync",
            "transport": {
                "type": "stdio",
                "command": "/bin/echo",
                "args": ["not-a-real-mcp-server"]
            },
            "timeout_secs": 45,
            "env": ["GITHUB_TOKEN"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create.status(), 201);
    let create_body: serde_json::Value = create.json().await.unwrap();
    assert_eq!(create_body["status"], "added");
    assert_eq!(create_body["reload"], "applied");

    let list = client
        .get(format!("{}/api/mcp/servers", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(list.status(), 200);
    let list_body: serde_json::Value = list.json().await.unwrap();
    assert_eq!(list_body["total_configured"], 1);
    assert_eq!(list_body["configured"][0]["name"], "github-sync");
    assert_eq!(list_body["configured"][0]["timeout_secs"], 45);
    assert_eq!(list_body["configured"][0]["transport"]["type"], "stdio");
    assert_eq!(
        list_body["configured"][0]["transport"]["command"],
        "/bin/echo"
    );
    assert_eq!(list_body["total_connected"], 0);

    let effective = server
        .state
        .kernel
        .effective_mcp_servers
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    assert_eq!(effective.len(), 1);
    assert_eq!(effective[0].name, "github-sync");

    let config_path = server.state.kernel.config.home_dir.join("config.toml");
    let config_text = std::fs::read_to_string(&config_path).unwrap();
    assert!(config_text.contains("github-sync"));
    assert!(config_text.contains("GITHUB_TOKEN"));

    let update = client
        .put(format!("{}/api/mcp/servers/github-sync", server.base_url))
        .json(&serde_json::json!({
            "transport": {
                "type": "sse",
                "url": "http://127.0.0.1:59999/sse"
            },
            "timeout_secs": 60,
            "env": ["GITHUB_TOKEN", "EXTRA_TOKEN"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(update.status(), 200);
    let update_body: serde_json::Value = update.json().await.unwrap();
    assert_eq!(update_body["status"], "updated");

    let list_after_update = client
        .get(format!("{}/api/mcp/servers", server.base_url))
        .send()
        .await
        .unwrap();
    let update_list_body: serde_json::Value = list_after_update.json().await.unwrap();
    assert_eq!(
        update_list_body["configured"][0]["transport"]["type"],
        "sse"
    );
    assert_eq!(
        update_list_body["configured"][0]["transport"]["url"],
        "http://127.0.0.1:59999/sse"
    );
    assert_eq!(update_list_body["configured"][0]["timeout_secs"], 60);
    assert_eq!(update_list_body["configured"][0]["env"][1], "EXTRA_TOKEN");

    let delete = client
        .delete(format!("{}/api/mcp/servers/github-sync", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(delete.status(), 200);
    let delete_body: serde_json::Value = delete.json().await.unwrap();
    assert_eq!(delete_body["status"], "removed");

    let final_list = client
        .get(format!("{}/api/mcp/servers", server.base_url))
        .send()
        .await
        .unwrap();
    let final_body: serde_json::Value = final_list.json().await.unwrap();
    assert_eq!(final_body["total_configured"], 0);

    let effective_after_delete = server
        .state
        .kernel
        .effective_mcp_servers
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    assert!(effective_after_delete.is_empty());
}

#[tokio::test]
async fn test_mcp_server_crud_validation_errors() {
    let server = start_test_server().await;
    write_test_config(&server);
    let client = reqwest::Client::new();

    let missing_transport = client
        .post(format!("{}/api/mcp/servers", server.base_url))
        .json(&serde_json::json!({
            "name": "broken"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(missing_transport.status(), 400);

    let create = client
        .post(format!("{}/api/mcp/servers", server.base_url))
        .json(&serde_json::json!({
            "name": "filesystem",
            "transport": {
                "type": "stdio",
                "command": "/bin/echo",
                "args": ["still-not-mcp"]
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create.status(), 201);

    let duplicate = client
        .post(format!("{}/api/mcp/servers", server.base_url))
        .json(&serde_json::json!({
            "name": "filesystem",
            "transport": {
                "type": "stdio",
                "command": "/bin/echo",
                "args": []
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(duplicate.status(), 409);

    let update_missing = client
        .put(format!(
            "{}/api/mcp/servers/does-not-exist",
            server.base_url
        ))
        .json(&serde_json::json!({
            "transport": {
                "type": "sse",
                "url": "http://127.0.0.1:1/sse"
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(update_missing.status(), 404);

    let delete_missing = client
        .delete(format!(
            "{}/api/mcp/servers/does-not-exist",
            server.base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(delete_missing.status(), 404);
}

#[tokio::test]
async fn test_comms_send_channel_with_thread_and_attachments() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let source_agent_id = spawn_test_agent(&server, &client).await;
    let sent = install_mock_channel_adapter(&server);
    let upload = upload_test_attachment(
        &server,
        &client,
        &source_agent_id,
        "notes.txt",
        "text/plain",
        b"hello attachment".to_vec(),
    )
    .await;

    let response = client
        .post(format!("{}/api/comms/send", server.base_url))
        .json(&serde_json::json!({
            "from_agent_id": source_agent_id,
            "channel": "mock",
            "recipient": "user-123",
            "message": "hello channel",
            "thread_id": "thread-42",
            "attachments": [{
                "file_id": upload["file_id"],
                "filename": upload["filename"],
                "content_type": upload["content_type"]
            }]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["ok"], true);
    assert_eq!(body["mode"], "channel");
    assert_eq!(body["thread_id"], "thread-42");
    assert_eq!(body["attachments_sent"], 1);
    assert_eq!(body["message_sent"], true);

    let records = sent.lock().unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].user.platform_id, "user-123");
    assert_eq!(records[0].thread_id.as_deref(), Some("thread-42"));
    assert!(matches!(
        &records[0].content,
        ChannelContent::Text(text) if text == "hello channel"
    ));
    assert_eq!(records[1].thread_id.as_deref(), Some("thread-42"));
    assert!(matches!(
        &records[1].content,
        ChannelContent::FileData { filename, mime_type, .. }
            if filename == "notes.txt" && mime_type == "text/plain"
    ));
}

#[tokio::test]
async fn test_comms_send_requires_exactly_one_target() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let source_agent_id = spawn_test_agent(&server, &client).await;

    let response = client
        .post(format!("{}/api/comms/send", server.base_url))
        .json(&serde_json::json!({
            "from_agent_id": source_agent_id,
            "message": "hello"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 400);
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("exactly one delivery target"));
}

#[tokio::test]
async fn test_comms_send_agent_rejects_non_image_attachments() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let source_agent_id = spawn_test_agent(&server, &client).await;
    let target_manifest =
        TEST_MANIFEST.replacen("name = \"test-agent\"", "name = \"target-agent\"", 1);
    let target_response = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": target_manifest}))
        .send()
        .await
        .unwrap();
    assert_eq!(target_response.status(), 201);
    let target_body: serde_json::Value = target_response.json().await.unwrap();
    let target_agent_id = target_body["agent_id"].as_str().unwrap().to_string();
    let upload = upload_test_attachment(
        &server,
        &client,
        &source_agent_id,
        "notes.txt",
        "text/plain",
        b"hello attachment".to_vec(),
    )
    .await;

    let response = client
        .post(format!("{}/api/comms/send", server.base_url))
        .json(&serde_json::json!({
            "from_agent_id": source_agent_id,
            "to_agent_id": target_agent_id,
            "message": "hello agent",
            "attachments": [{
                "file_id": upload["file_id"],
                "filename": upload["filename"],
                "content_type": upload["content_type"]
            }]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 400);
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("image attachments only"));
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
        peer_registry: None,
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
        .route(
            "/api/cron/jobs",
            axum::routing::get(routes::list_cron_jobs).post(routes::create_cron_job),
        )
        .route(
            "/api/cron/jobs/{id}",
            axum::routing::delete(routes::delete_cron_job).put(routes::update_cron_job),
        )
        .route("/api/backup", axum::routing::post(routes::create_backup))
        .route("/api/backups", axum::routing::get(routes::list_backups))
        .route(
            "/api/backups/{filename}",
            axum::routing::delete(routes::delete_backup),
        )
        .route("/api/restore", axum::routing::post(routes::restore_backup))
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
        axum::serve(listener, app).await.unwrap();
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
    // Empty API key = auth disabled
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    // Protected endpoint accessible without auth when no key is configured
    let resp = client
        .get(format!("{}/api/status", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

// ---------------------------------------------------------------------------
// Phase 10-12: Integration management tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_available_integrations_returns_bundled_templates() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/integrations/available", server.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let integrations = body["integrations"].as_array().unwrap();
    assert!(
        integrations.len() >= 20,
        "Expected at least 20 bundled integration templates, got {}",
        integrations.len()
    );
    assert_eq!(body["count"].as_u64().unwrap(), integrations.len() as u64);

    // Each entry must have the required fields
    for entry in integrations {
        assert!(
            entry["id"].is_string(),
            "Integration entry missing 'id': {:?}",
            entry
        );
        assert!(
            entry["name"].is_string(),
            "Integration entry missing 'name': {:?}",
            entry
        );
        assert!(
            entry["category"].is_string(),
            "Integration entry missing 'category': {:?}",
            entry
        );
        assert!(
            entry["description"].is_string(),
            "Integration entry missing 'description': {:?}",
            entry
        );
    }
}

#[tokio::test]
async fn test_install_and_remove_integration() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    // Install brave-search integration
    let install_resp = client
        .post(format!("{}/api/integrations/add", server.base_url))
        .json(&serde_json::json!({"id": "brave-search"}))
        .send()
        .await
        .unwrap();
    assert_eq!(install_resp.status(), StatusCode::CREATED);
    let install_body: serde_json::Value = install_resp.json().await.unwrap();
    assert_eq!(install_body["id"], "brave-search");
    assert_eq!(install_body["status"], "installed");

    // Verify it appears in the installed list
    let list_resp = client
        .get(format!("{}/api/integrations", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(list_resp.status(), 200);
    let list_body: serde_json::Value = list_resp.json().await.unwrap();
    let installed = list_body["installed"].as_array().unwrap();
    assert!(
        installed.iter().any(|i| i["id"] == "brave-search"),
        "brave-search should appear in installed integrations"
    );

    // Remove the integration
    let remove_resp = client
        .delete(format!("{}/api/integrations/brave-search", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(remove_resp.status(), 200);
    let remove_body: serde_json::Value = remove_resp.json().await.unwrap();
    assert_eq!(remove_body["id"], "brave-search");
    assert_eq!(remove_body["status"], "removed");

    // Verify it's gone from the installed list
    let list_after_resp = client
        .get(format!("{}/api/integrations", server.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(list_after_resp.status(), 200);
    let list_after_body: serde_json::Value = list_after_resp.json().await.unwrap();
    let installed_after = list_after_body["installed"].as_array().unwrap();
    assert!(
        !installed_after.iter().any(|i| i["id"] == "brave-search"),
        "brave-search should no longer appear in installed integrations"
    );
}

#[tokio::test]
async fn test_install_unknown_integration_returns_404() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/integrations/add", server.base_url))
        .json(&serde_json::json!({"id": "nonexistent-integration"}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("nonexistent-integration"));
}

#[tokio::test]
async fn test_install_duplicate_integration_returns_409() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    // First install should succeed
    let first_resp = client
        .post(format!("{}/api/integrations/add", server.base_url))
        .json(&serde_json::json!({"id": "brave-search"}))
        .send()
        .await
        .unwrap();
    assert_eq!(first_resp.status(), StatusCode::CREATED);

    // Second install of same ID should conflict
    let second_resp = client
        .post(format!("{}/api/integrations/add", server.base_url))
        .json(&serde_json::json!({"id": "brave-search"}))
        .send()
        .await
        .unwrap();
    assert_eq!(second_resp.status(), StatusCode::CONFLICT);
    let body: serde_json::Value = second_resp.json().await.unwrap();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("already installed"));
}

#[tokio::test]
async fn test_integrations_health_endpoint() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/integrations/health", server.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(
        body["health"].is_array(),
        "Response should have 'health' array"
    );
    assert!(
        body["count"].is_number(),
        "Response should have 'count' field"
    );
    assert_eq!(
        body["count"].as_u64().unwrap(),
        body["health"].as_array().unwrap().len() as u64
    );
}

#[tokio::test]
async fn test_reload_integrations_endpoint() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/integrations/reload", server.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "reloaded");
}

// ---------------------------------------------------------------------------
// Phase 10-12: Decision trace tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_decision_traces_empty_by_default() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/routing/decisions", server.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(
        body["decisions"].is_array(),
        "Response should have 'decisions' array"
    );
    assert!(
        body["count"].is_number(),
        "Response should have 'count' field"
    );
}

#[tokio::test]
async fn test_decision_traces_with_limit_param() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/routing/decisions?limit=5", server.base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["decisions"].is_array());
    assert!(body["count"].as_u64().unwrap() <= 5);
}
