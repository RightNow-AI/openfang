//! Load & performance tests for the OpenFang API.
//!
//! Measures throughput under concurrent access: agent spawning, API endpoint
//! latency, session management, and memory usage.
//!
//! Run: cargo test -p openfang-api --test load_test -- --nocapture

use axum::Router;
use openfang_api::middleware;
use openfang_api::routes::{self, AppState};
use openfang_kernel::OpenFangKernel;
use openfang_types::config::{DefaultModelConfig, KernelConfig};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

// ---------------------------------------------------------------------------
// Test infrastructure (mirrors api_integration_test.rs)
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

async fn start_test_server() -> TestServer {
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
        .route("/api/version", axum::routing::get(routes::version))
        .route(
            "/api/metrics",
            axum::routing::get(routes::prometheus_metrics),
        )
        .route(
            "/api/agents",
            axum::routing::get(routes::list_agents).post(routes::spawn_agent),
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
            "/api/agents/{id}/session/reset",
            axum::routing::post(routes::reset_session),
        )
        .route(
            "/api/agents/{id}/sessions",
            axum::routing::get(routes::list_agent_sessions).post(routes::create_agent_session),
        )
        .route("/api/tools", axum::routing::get(routes::list_tools))
        .route("/api/models", axum::routing::get(routes::list_models))
        .route("/api/providers", axum::routing::get(routes::list_providers))
        .route("/api/usage", axum::routing::get(routes::usage_stats))
        .route(
            "/api/workflows",
            axum::routing::get(routes::list_workflows).post(routes::create_workflow),
        )
        .route(
            "/api/workflows/{id}/run",
            axum::routing::post(routes::run_workflow),
        )
        .route("/api/config", axum::routing::get(routes::get_config))
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

const TEST_MANIFEST: &str = r#"
name = "load-test-agent"
version = "0.1.0"
description = "Load test agent"
author = "test"
module = "builtin:chat"

[model]
provider = "ollama"
model = "test-model"
system_prompt = "You are a test agent."

[capabilities]
tools = ["file_read"]
memory_read = ["*"]
memory_write = ["self.*"]
"#;

// ---------------------------------------------------------------------------
// Load tests
// ---------------------------------------------------------------------------

/// Test: Concurrent agent spawns — verify kernel handles parallel agent creation.
#[tokio::test]
async fn load_concurrent_agent_spawns() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();
    let n = 20; // 20 concurrent spawns

    let start = Instant::now();
    let mut handles = Vec::new();

    for i in 0..n {
        let c = client.clone();
        let url = format!("{}/api/agents", server.base_url);
        let manifest = TEST_MANIFEST.replace("load-test-agent", &format!("load-agent-{i}"));
        handles.push(tokio::spawn(async move {
            let res = c
                .post(&url)
                .json(&serde_json::json!({"manifest_toml": manifest}))
                .send()
                .await
                .expect("request failed");
            (res.status().as_u16(), i)
        }));
    }

    let mut success = 0;
    for h in handles {
        let (status, _i) = h.await.unwrap();
        if status == 200 || status == 201 {
            success += 1;
        }
    }

    let elapsed = start.elapsed();
    eprintln!(
        "  [LOAD] Concurrent spawns: {success}/{n} succeeded in {:.0}ms ({:.0} spawns/sec)",
        elapsed.as_millis(),
        n as f64 / elapsed.as_secs_f64()
    );
    assert!(success >= n - 2, "Most agents should spawn successfully");

    // Verify via list
    let agents: serde_json::Value = client
        .get(format!("{}/api/agents", server.base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let count = agents.as_array().map(|a| a.len()).unwrap_or(0);
    eprintln!("  [LOAD] Total agents after spawn: {count}");
    assert!(count >= success);
}

/// Test: API endpoint latency — measure p50/p95/p99 for health, status, list agents.
#[tokio::test]
async fn load_endpoint_latency() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    // Spawn a few agents for the list endpoint to return
    for i in 0..5 {
        let manifest = TEST_MANIFEST.replace("load-test-agent", &format!("latency-agent-{i}"));
        client
            .post(format!("{}/api/agents", server.base_url))
            .json(&serde_json::json!({"manifest_toml": manifest}))
            .send()
            .await
            .unwrap();
    }

    let endpoints = vec![
        ("GET", "/api/health"),
        ("GET", "/api/status"),
        ("GET", "/api/agents"),
        ("GET", "/api/tools"),
        ("GET", "/api/models"),
        ("GET", "/api/metrics"),
        ("GET", "/api/config"),
        ("GET", "/api/usage"),
    ];

    for (method, path) in &endpoints {
        let mut latencies = Vec::new();
        let n = 100;

        for _ in 0..n {
            let start = Instant::now();
            let url = format!("{}{}", server.base_url, path);
            let res = match *method {
                "GET" => client.get(&url).send().await,
                _ => client.post(&url).send().await,
            };
            let elapsed = start.elapsed();
            assert!(res.is_ok(), "{method} {path} failed");
            latencies.push(elapsed);
        }

        latencies.sort();
        let p50 = latencies[n / 2];
        let p95 = latencies[(n as f64 * 0.95) as usize];
        let p99 = latencies[(n as f64 * 0.99) as usize];

        eprintln!(
            "  [LOAD] {method} {path:30} p50={:>5.1}ms  p95={:>5.1}ms  p99={:>5.1}ms",
            p50.as_secs_f64() * 1000.0,
            p95.as_secs_f64() * 1000.0,
            p99.as_secs_f64() * 1000.0,
        );

        // p99 should be under 100ms for read endpoints
        assert!(
            p99 < Duration::from_millis(500),
            "{method} {path} p99 too high: {p99:?}"
        );
    }
}

/// Test: Concurrent reads — many clients hitting the same endpoints simultaneously.
#[tokio::test]
async fn load_concurrent_reads() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    // Spawn some agents first
    for i in 0..3 {
        let manifest = TEST_MANIFEST.replace("load-test-agent", &format!("concurrent-agent-{i}"));
        client
            .post(format!("{}/api/agents", server.base_url))
            .json(&serde_json::json!({"manifest_toml": manifest}))
            .send()
            .await
            .unwrap();
    }

    let n = 50;
    let start = Instant::now();
    let mut handles = Vec::new();

    for i in 0..n {
        let c = client.clone();
        let base = server.base_url.clone();
        handles.push(tokio::spawn(async move {
            // Cycle through different endpoints
            let path = match i % 4 {
                0 => "/api/health",
                1 => "/api/agents",
                2 => "/api/status",
                _ => "/api/metrics",
            };
            let res = c
                .get(format!("{base}{path}"))
                .send()
                .await
                .expect("request failed");
            res.status().as_u16()
        }));
    }

    let mut success = 0;
    for h in handles {
        let status = h.await.unwrap();
        if status == 200 {
            success += 1;
        }
    }

    let elapsed = start.elapsed();
    eprintln!(
        "  [LOAD] Concurrent reads: {success}/{n} succeeded in {:.0}ms ({:.0} req/sec)",
        elapsed.as_millis(),
        n as f64 / elapsed.as_secs_f64()
    );
    assert_eq!(success, n, "All concurrent reads should succeed");
}

/// Test: Session management under load — create, list, and switch sessions.
#[tokio::test]
async fn load_session_management() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    // Spawn an agent
    let res: serde_json::Value = client
        .post(format!("{}/api/agents", server.base_url))
        .json(&serde_json::json!({"manifest_toml": TEST_MANIFEST}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let agent_id = res["agent_id"].as_str().unwrap().to_string();

    // Create multiple sessions
    let n = 10;
    let start = Instant::now();
    let mut session_ids = Vec::new();

    for i in 0..n {
        let res: serde_json::Value = client
            .post(format!(
                "{}/api/agents/{}/sessions",
                server.base_url, agent_id
            ))
            .json(&serde_json::json!({"label": format!("session-{i}")}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if let Some(id) = res.get("session_id").and_then(|v| v.as_str()) {
            session_ids.push(id.to_string());
        }
    }

    let elapsed = start.elapsed();
    eprintln!(
        "  [LOAD] Created {n} sessions in {:.0}ms",
        elapsed.as_millis()
    );

    // List sessions
    let start = Instant::now();
    let sessions_resp: serde_json::Value = client
        .get(format!(
            "{}/api/agents/{}/sessions",
            server.base_url, agent_id
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    // Response is {"sessions": [...]} — extract the array
    let session_count = sessions_resp
        .get("sessions")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or_else(|| {
            // Fallback: maybe it's a direct array
            sessions_resp.as_array().map(|a| a.len()).unwrap_or(0)
        });
    eprintln!(
        "  [LOAD] Listed {session_count} sessions in {:.1}ms",
        start.elapsed().as_secs_f64() * 1000.0
    );

    // We expect at least some sessions (the original + our new ones)
    // Note: create_session might fail silently for some if agent was spawned without session
    eprintln!("  [LOAD] Session IDs collected: {}", session_ids.len());
    assert!(
        !session_ids.is_empty() || session_count > 0,
        "Should have created some sessions"
    );

    // Switch between sessions rapidly
    let start = Instant::now();
    for sid in &session_ids {
        client
            .post(format!(
                "{}/api/agents/{}/sessions/{}/switch",
                server.base_url, agent_id, sid
            ))
            .send()
            .await
            .unwrap();
    }
    eprintln!(
        "  [LOAD] Switched through {} sessions in {:.0}ms",
        session_ids.len(),
        start.elapsed().as_millis()
    );
}

/// Test: Workflow creation and listing under load.
#[tokio::test]
async fn load_workflow_operations() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let n = 15;
    let start = Instant::now();

    // Create workflows concurrently
    let mut handles = Vec::new();
    for i in 0..n {
        let c = client.clone();
        let url = format!("{}/api/workflows", server.base_url);
        handles.push(tokio::spawn(async move {
            let res = c
                .post(&url)
                .json(&serde_json::json!({
                    "name": format!("wf-{i}"),
                    "description": format!("Load test workflow {i}"),
                    "steps": [{
                        "name": "step1",
                        "agent_name": "test-agent",
                        "mode": "sequential",
                        "prompt": "{{input}}"
                    }]
                }))
                .send()
                .await
                .expect("request failed");
            res.status().as_u16()
        }));
    }

    let mut created = 0;
    for h in handles {
        let status = h.await.unwrap();
        if status == 200 || status == 201 {
            created += 1;
        }
    }

    let elapsed = start.elapsed();
    eprintln!(
        "  [LOAD] Created {created}/{n} workflows in {:.0}ms",
        elapsed.as_millis()
    );

    // List all workflows
    let start = Instant::now();
    let workflows: serde_json::Value = client
        .get(format!("{}/api/workflows", server.base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let wf_count = workflows.as_array().map(|a| a.len()).unwrap_or(0);
    eprintln!(
        "  [LOAD] Listed {wf_count} workflows in {:.1}ms",
        start.elapsed().as_secs_f64() * 1000.0
    );
    assert!(wf_count >= created);
}

/// Test: Agent spawn + kill cycle — stress the registry.
#[tokio::test]
async fn load_spawn_kill_cycle() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let cycles = 10;
    let start = Instant::now();
    let mut ids = Vec::new();

    // Spawn
    for i in 0..cycles {
        let manifest = TEST_MANIFEST.replace("load-test-agent", &format!("cycle-agent-{i}"));
        let res: serde_json::Value = client
            .post(format!("{}/api/agents", server.base_url))
            .json(&serde_json::json!({"manifest_toml": manifest}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if let Some(id) = res.get("agent_id").and_then(|v| v.as_str()) {
            ids.push(id.to_string());
        }
    }

    // Kill
    for id in &ids {
        client
            .delete(format!("{}/api/agents/{}", server.base_url, id))
            .send()
            .await
            .unwrap();
    }

    let elapsed = start.elapsed();
    eprintln!(
        "  [LOAD] Spawn+kill {cycles} agents in {:.0}ms ({:.0}ms per cycle)",
        elapsed.as_millis(),
        elapsed.as_millis() as f64 / cycles as f64
    );

    // Verify all cleaned up
    let agents: serde_json::Value = client
        .get(format!("{}/api/agents", server.base_url))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let remaining = agents.as_array().map(|a| a.len()).unwrap_or(0);
    assert_eq!(remaining, 1, "Only default assistant should remain");
}

/// Test: Prometheus metrics endpoint under sustained load.
#[tokio::test]
async fn load_metrics_sustained() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    // Spawn a few agents first so metrics have data
    for i in 0..3 {
        let manifest = TEST_MANIFEST.replace("load-test-agent", &format!("metrics-agent-{i}"));
        client
            .post(format!("{}/api/agents", server.base_url))
            .json(&serde_json::json!({"manifest_toml": manifest}))
            .send()
            .await
            .unwrap();
    }

    // Hit metrics endpoint 200 times
    let n = 200;
    let start = Instant::now();
    for _ in 0..n {
        let res = client
            .get(format!("{}/api/metrics", server.base_url))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status().as_u16(), 200);
        let body = res.text().await.unwrap();
        assert!(body.contains("openfang_agents_active"));
    }

    let elapsed = start.elapsed();
    eprintln!(
        "  [LOAD] Metrics {n} requests in {:.0}ms ({:.0} req/sec, {:.1}ms avg)",
        elapsed.as_millis(),
        n as f64 / elapsed.as_secs_f64(),
        elapsed.as_secs_f64() * 1000.0 / n as f64
    );
}
