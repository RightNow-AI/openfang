//! Integration tests for the `/api/skills/{id}/config` surface.
//!
//! These boot a real kernel, start a real axum server on a random port, plant
//! a synthetic skill on disk whose SKILL.md declares a `config:` section, and
//! exercise GET / PUT / DELETE end to end. Bundled skills currently declare
//! no runtime config, so the synthetic skill fixture is what lets us prove
//! the wire contract.
//!
//! Run: cargo test -p openfang-api --test skill_config_api_test -- --nocapture

use axum::Router;
use openfang_api::middleware;
use openfang_api::routes::{self, AppState};
use openfang_kernel::OpenFangKernel;
use openfang_types::config::{DefaultModelConfig, KernelConfig};
use std::sync::Arc;
use std::time::Instant;
use tower_http::cors::CorsLayer;

// ---------------------------------------------------------------------------
// Test server harness
// ---------------------------------------------------------------------------

struct TestServer {
    base_url: String,
    home_dir: std::path::PathBuf,
    #[allow(dead_code)]
    state: Arc<AppState>,
    _tmp: tempfile::TempDir,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.state.kernel.shutdown();
    }
}

/// Write a skill fixture under `<home>/skills/<name>/SKILL.md` that declares
/// a `config:` section. This matches the on-disk format that OpenClaw skills
/// use, so the loader's real `parse_skillmd_str` path is exercised.
fn plant_skill_with_config(home: &std::path::Path, skill_name: &str) {
    let skill_dir = home.join("skills").join(skill_name);
    std::fs::create_dir_all(&skill_dir).unwrap();
    // Leading four spaces inside YAML lists matter — keep them.
    let skillmd = format!(
        "---
name: {skill_name}
description: Synthetic skill for config endpoint tests
config:
  github_token:
    description: GitHub personal access token
    env: OPENFANG_TEST_SKILLCFG_GH_TOKEN
    required: true
  default_branch:
    description: Default branch name
    default: main
    required: false
---
# Test Skill

Placeholder body so the parser accepts this as a valid prompt-only skill.
"
    );
    std::fs::write(skill_dir.join("SKILL.md"), skillmd).unwrap();
}

async fn start_test_server() -> TestServer {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().to_path_buf();

    let config = KernelConfig {
        home_dir: home.clone(),
        data_dir: home.join("data"),
        default_model: DefaultModelConfig {
            provider: "ollama".to_string(),
            model: "test-model".to_string(),
            api_key_env: "OLLAMA_API_KEY".to_string(),
            base_url: None,
        },
        ..KernelConfig::default()
    };

    // Plant synthetic skill BEFORE booting so the initial skill load picks it up.
    plant_skill_with_config(&home, "test-config-skill");

    let kernel = OpenFangKernel::boot_with_config(config).expect("kernel boot");
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
        budget_config: Arc::new(tokio::sync::RwLock::new(Default::default())),
    });

    let app = Router::new()
        .route("/api/skills", axum::routing::get(routes::list_skills))
        .route(
            "/api/skills/{id}/config",
            axum::routing::get(routes::get_skill_config).put(routes::put_skill_config),
        )
        .route(
            "/api/skills/{id}/config/{var_name}",
            axum::routing::delete(routes::delete_skill_config_var),
        )
        .layer(axum::middleware::from_fn(middleware::request_logging))
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test port");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    TestServer {
        base_url: format!("http://{}", addr),
        home_dir: home,
        state,
        _tmp: tmp,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_config_returns_declared_and_resolved() {
    // Make sure no host leak from prior tests interferes.
    // SAFETY: single-threaded test, env var is unique to this test suite.
    unsafe { std::env::remove_var("OPENFANG_TEST_SKILLCFG_GH_TOKEN") };

    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!(
            "{}/api/skills/test-config-skill/config",
            server.base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();

    assert_eq!(body["skill"], "test-config-skill");
    // Both vars declared
    assert!(body["declared"]["github_token"].is_object());
    assert!(body["declared"]["default_branch"].is_object());
    assert_eq!(body["declared"]["github_token"]["required"], true);
    assert_eq!(body["declared"]["default_branch"]["required"], false);

    // github_token has no user override, no env, no default -> unresolved.
    assert_eq!(
        body["resolved"]["github_token"]["source"], "unresolved",
        "github_token should be unresolved without env"
    );
    assert!(body["resolved"]["github_token"]["is_secret"].as_bool().unwrap());

    // default_branch falls back to default "main".
    assert_eq!(body["resolved"]["default_branch"]["source"], "default");
    assert_eq!(body["resolved"]["default_branch"]["value"], "main");
}

#[tokio::test]
async fn get_config_redacts_secret_values_after_put() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    // Write a real-looking token via PUT.
    let payload = serde_json::json!({
        "values": {
            "github_token": "ghp_realsecretvalue_DO_NOT_LEAK",
            "default_branch": "develop"
        }
    });
    let resp = client
        .put(format!(
            "{}/api/skills/test-config-skill/config",
            server.base_url
        ))
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "PUT should succeed");

    // GET back and confirm the secret is redacted and non-secret is visible.
    let resp = client
        .get(format!(
            "{}/api/skills/test-config-skill/config",
            server.base_url
        ))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();

    let returned_token = body["resolved"]["github_token"]["value"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        !returned_token.contains("realsecretvalue"),
        "secret leaked on the wire: {returned_token}"
    );
    assert!(
        returned_token.contains("redacted"),
        "expected redaction marker, got: {returned_token}"
    );
    assert_eq!(body["resolved"]["github_token"]["source"], "user");

    // Non-secret var kept as-is.
    assert_eq!(body["resolved"]["default_branch"]["value"], "develop");
    assert_eq!(body["resolved"]["default_branch"]["source"], "user");

    // config.toml persisted the change, including the full secret value
    // (redaction is only on the wire — disk is the source of truth).
    let cfg = std::fs::read_to_string(server.home_dir.join("config.toml")).unwrap();
    assert!(
        cfg.contains("[skills.test-config-skill]"),
        "skills section missing: {cfg}"
    );
    assert!(cfg.contains("realsecretvalue"));
    assert!(cfg.contains("develop"));
}

#[tokio::test]
async fn put_rejects_unknown_variable() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    let payload = serde_json::json!({
        "values": { "nonexistent_var": "value" }
    });
    let resp = client
        .put(format!(
            "{}/api/skills/test-config-skill/config",
            server.base_url
        ))
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("nonexistent_var"));
}

#[tokio::test]
async fn delete_override_reverts_to_default() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    // Set override first.
    client
        .put(format!(
            "{}/api/skills/test-config-skill/config",
            server.base_url
        ))
        .json(&serde_json::json!({
            "values": { "default_branch": "develop" }
        }))
        .send()
        .await
        .unwrap();

    // Remove it.
    let resp = client
        .delete(format!(
            "{}/api/skills/test-config-skill/config/default_branch",
            server.base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Now source should be "default" again with value "main".
    let body: serde_json::Value = client
        .get(format!(
            "{}/api/skills/test-config-skill/config",
            server.base_url
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["resolved"]["default_branch"]["source"], "default");
    assert_eq!(body["resolved"]["default_branch"]["value"], "main");
}

#[tokio::test]
async fn delete_refuses_to_strand_required_var() {
    // github_token is required, has no default, and no env — so removing an
    // override would leave it unresolvable. The endpoint must refuse.
    // SAFETY: single-threaded test.
    unsafe { std::env::remove_var("OPENFANG_TEST_SKILLCFG_GH_TOKEN") };

    let server = start_test_server().await;
    let client = reqwest::Client::new();

    // Set an override.
    client
        .put(format!(
            "{}/api/skills/test-config-skill/config",
            server.base_url
        ))
        .json(&serde_json::json!({
            "values": { "github_token": "ghp_value" }
        }))
        .send()
        .await
        .unwrap();

    // Try to delete — should 409.
    let resp = client
        .delete(format!(
            "{}/api/skills/test-config-skill/config/github_token",
            server.base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
}

#[tokio::test]
async fn get_unknown_skill_returns_404() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{}/api/skills/this-skill-does-not-exist/config",
            server.base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn put_reloads_registry_so_agents_see_change() {
    let server = start_test_server().await;
    let client = reqwest::Client::new();

    client
        .put(format!(
            "{}/api/skills/test-config-skill/config",
            server.base_url
        ))
        .json(&serde_json::json!({
            "values": {
                "github_token": "ghp_new",
                "default_branch": "release"
            }
        }))
        .send()
        .await
        .unwrap();

    // The kernel's live override map must now hold the new values.
    let guard = server
        .state
        .kernel
        .skill_config_overrides
        .read()
        .unwrap();
    let overrides = guard.as_ref().expect("override map set after PUT");
    let skill_cfg = overrides.get("test-config-skill").expect("skill present");
    assert_eq!(skill_cfg.get("github_token").unwrap(), "ghp_new");
    assert_eq!(skill_cfg.get("default_branch").unwrap(), "release");
}
