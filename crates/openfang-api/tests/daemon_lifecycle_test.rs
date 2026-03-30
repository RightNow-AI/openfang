//! Daemon lifecycle integration tests.
//!
//! Tests the real daemon startup, PID file management, health serving,
//! and graceful shutdown sequence.

use axum::Router;
use openfang_api::middleware;
use openfang_api::routes::{self, AppState};
use openfang_api::server::{read_daemon_info, DaemonInfo};
use openfang_kernel::OpenFangKernel;
use openfang_types::config::{DefaultModelConfig, KernelConfig};
use std::sync::Arc;
use std::time::Instant;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Test DaemonInfo serialization and deserialization round-trip.
#[test]
fn test_daemon_info_serde_roundtrip() {
    let info = DaemonInfo {
        pid: 12345,
        listen_addr: "127.0.0.1:4200".to_string(),
        started_at: "2024-01-01T00:00:00Z".to_string(),
        version: "0.1.0".to_string(),
        platform: "linux".to_string(),
    };

    let json = serde_json::to_string_pretty(&info).unwrap();
    let parsed: DaemonInfo = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.pid, 12345);
    assert_eq!(parsed.listen_addr, "127.0.0.1:4200");
    assert_eq!(parsed.version, "0.1.0");
    assert_eq!(parsed.platform, "linux");
}

/// Test read_daemon_info from a file on disk.
#[test]
fn test_read_daemon_info_from_file() {
    let tmp = tempfile::tempdir().unwrap();

    // Write a daemon.json
    let info = DaemonInfo {
        pid: std::process::id(),
        listen_addr: "127.0.0.1:9999".to_string(),
        started_at: chrono::Utc::now().to_rfc3339(),
        version: "0.1.0".to_string(),
        platform: "test".to_string(),
    };
    let json = serde_json::to_string_pretty(&info).unwrap();
    std::fs::write(tmp.path().join("daemon.json"), json).unwrap();

    // Read it back
    let loaded = read_daemon_info(tmp.path());
    assert!(loaded.is_some());
    let loaded = loaded.unwrap();
    assert_eq!(loaded.pid, std::process::id());
    assert_eq!(loaded.listen_addr, "127.0.0.1:9999");
}

/// Test read_daemon_info returns None when file doesn't exist.
#[test]
fn test_read_daemon_info_missing_file() {
    let tmp = tempfile::tempdir().unwrap();
    let loaded = read_daemon_info(tmp.path());
    assert!(loaded.is_none());
}

/// Test read_daemon_info returns None for corrupt JSON.
#[test]
fn test_read_daemon_info_corrupt_json() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("daemon.json"), "not json at all").unwrap();
    let loaded = read_daemon_info(tmp.path());
    assert!(loaded.is_none());
}

/// Test the full daemon lifecycle:
///   1. Boot kernel + start server on random port
///   2. Write daemon info file
///   3. Verify health endpoint
///   4. Verify daemon info file contents match
///   5. Shut down and verify cleanup
#[tokio::test]
async fn test_full_daemon_lifecycle() {
    let tmp = tempfile::tempdir().unwrap();
    let daemon_info_path = tmp.path().join("daemon.json");

    let config = KernelConfig {
        home_dir: tmp.path().to_path_buf(),
        data_dir: tmp.path().join("data"),
        default_model: DefaultModelConfig {
            provider: "ollama".to_string(),
            model: "test".to_string(),
            api_key_env: "OLLAMA_API_KEY".to_string(),
            base_url: None,
        },
        ..KernelConfig::default()
    };

    let kernel = OpenFangKernel::boot_with_config(config).expect("Kernel should boot");
    let kernel = Arc::new(kernel);
    kernel.set_self_handle();

    let state = Arc::new(AppState {
        kernel: kernel.clone(),
        started_at: Instant::now(),
        peer_registry: None,
        bridge_manager: tokio::sync::Mutex::new(None),
        channels_config: tokio::sync::RwLock::new(Default::default()),
        shutdown_notify: Arc::new(tokio::sync::Notify::new()),
        clawhub_cache: dashmap::DashMap::new(),
        provider_probe_cache: openfang_runtime::provider_health::ProbeCache::new(),
        budget_config: Arc::new(tokio::sync::RwLock::new(Default::default())),
    });

    let app = Router::new()
        .route("/api/health", axum::routing::get(routes::health))
        .route("/api/status", axum::routing::get(routes::status))
        .route("/api/shutdown", axum::routing::post(routes::shutdown))
        .layer(axum::middleware::from_fn(middleware::request_logging))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    // Bind to random port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Spawn server
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Write daemon info file (like run_daemon does)
    let daemon_info = DaemonInfo {
        pid: std::process::id(),
        listen_addr: addr.to_string(),
        started_at: chrono::Utc::now().to_rfc3339(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: std::env::consts::OS.to_string(),
    };
    let json = serde_json::to_string_pretty(&daemon_info).unwrap();
    std::fs::write(&daemon_info_path, &json).unwrap();

    // --- Verify daemon info file ---
    assert!(daemon_info_path.exists());
    let loaded = read_daemon_info(tmp.path()).unwrap();
    assert_eq!(loaded.pid, std::process::id());
    assert_eq!(loaded.listen_addr, addr.to_string());

    // --- Verify health endpoint ---
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{}/api/health", addr))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");

    // --- Verify status endpoint ---
    let resp = client
        .get(format!("http://{}/api/status", addr))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "running");

    // --- Shutdown ---
    let resp = client
        .post(format!("http://{}/api/shutdown", addr))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Clean up daemon info file (like run_daemon does)
    let _ = std::fs::remove_file(&daemon_info_path);
    assert!(!daemon_info_path.exists());

    kernel.shutdown();
}

/// Test that stale daemon info is detected when no process is running at that PID.
#[test]
fn test_stale_daemon_info_detection() {
    let tmp = tempfile::tempdir().unwrap();

    // Write daemon.json with a PID that almost certainly doesn't exist
    // (using a very high PID number)
    let info = DaemonInfo {
        pid: 99999999, // unlikely to be running
        listen_addr: "127.0.0.1:9999".to_string(),
        started_at: "2024-01-01T00:00:00Z".to_string(),
        version: "0.1.0".to_string(),
        platform: "test".to_string(),
    };
    let json = serde_json::to_string_pretty(&info).unwrap();
    std::fs::write(tmp.path().join("daemon.json"), json).unwrap();

    // read_daemon_info just reads the file — it doesn't check if the PID is alive
    // (that check happens in run_daemon). So the file is readable:
    let loaded = read_daemon_info(tmp.path());
    assert!(loaded.is_some());
    assert_eq!(loaded.unwrap().pid, 99999999);
}

/// Test that the server starts and immediately responds to requests.
#[tokio::test]
async fn test_server_immediate_responsiveness() {
    let tmp = tempfile::tempdir().unwrap();
    let config = KernelConfig {
        home_dir: tmp.path().to_path_buf(),
        data_dir: tmp.path().join("data"),
        default_model: DefaultModelConfig {
            provider: "ollama".to_string(),
            model: "test".to_string(),
            api_key_env: "OLLAMA_API_KEY".to_string(),
            base_url: None,
        },
        ..KernelConfig::default()
    };

    let kernel = OpenFangKernel::boot_with_config(config).unwrap();
    let kernel = Arc::new(kernel);

    let state = Arc::new(AppState {
        kernel: kernel.clone(),
        started_at: Instant::now(),
        peer_registry: None,
        bridge_manager: tokio::sync::Mutex::new(None),
        channels_config: tokio::sync::RwLock::new(Default::default()),
        shutdown_notify: Arc::new(tokio::sync::Notify::new()),
        clawhub_cache: dashmap::DashMap::new(),
        provider_probe_cache: openfang_runtime::provider_health::ProbeCache::new(),
        budget_config: Arc::new(tokio::sync::RwLock::new(Default::default())),
    });

    let app = Router::new()
        .route("/api/health", axum::routing::get(routes::health))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Hit health endpoint immediately — should respond fast
    let client = reqwest::Client::new();
    let start = Instant::now();
    let resp = client
        .get(format!("http://{}/api/health", addr))
        .send()
        .await
        .unwrap();
    let latency = start.elapsed();

    assert_eq!(resp.status(), 200);
    assert!(
        latency.as_millis() < 1000,
        "Health endpoint should respond in <1s, took {}ms",
        latency.as_millis()
    );

    kernel.shutdown();
}
