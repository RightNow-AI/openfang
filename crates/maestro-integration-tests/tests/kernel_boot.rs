//! Integration tests for the OpenFang kernel boot sequence.
//!
//! These tests verify that the kernel can boot with a minimal in-memory
//! configuration without requiring any external services (no SurrealDB,
//! no Redis, no LLM API key).
//!
//! NOTE: These tests do NOT call any LLM endpoints. They only verify
//! the kernel's initialization, guardrails wiring, observability wiring,
//! and the agent registry's initial state.

use openfang_kernel::OpenFangKernel;
use openfang_types::config::KernelConfig;
use std::sync::Arc;
use tempfile::TempDir;

/// Build a minimal KernelConfig suitable for integration testing.
fn test_config(tmp: &TempDir) -> KernelConfig {
    KernelConfig {
        home_dir: tmp.path().to_path_buf(),
        data_dir: tmp.path().join("data"),
        api_listen: "127.0.0.1:0".to_string(),
        network_enabled: false,
        api_key: String::new(),
        ..Default::default()
    }
}

#[tokio::test]
async fn test_kernel_boots_with_minimal_config() {
    let tmp = TempDir::new().expect("create temp dir");
    let cfg = test_config(&tmp);
    let kernel = OpenFangKernel::boot_with_config(cfg).await;
    assert!(
        kernel.is_ok(),
        "Kernel should boot successfully with minimal config; got: {:?}",
        kernel.err()
    );
}

#[tokio::test]
async fn test_kernel_has_guardrails_wired() {
    let tmp = TempDir::new().expect("create temp dir");
    let cfg = test_config(&tmp);
    let kernel = Arc::new(
        OpenFangKernel::boot_with_config(cfg)
            .await
            .expect("kernel boot"),
    );
    assert!(
        kernel.guardrails.is_some(),
        "Kernel should have guardrails wired in by boot_with_config"
    );
}

#[tokio::test]
async fn test_kernel_has_trace_store_wired() {
    let tmp = TempDir::new().expect("create temp dir");
    let cfg = test_config(&tmp);
    let kernel = Arc::new(
        OpenFangKernel::boot_with_config(cfg)
            .await
            .expect("kernel boot"),
    );
    assert!(
        kernel.trace_store.is_some(),
        "Kernel should have trace store wired in by boot_with_config"
    );
}

#[tokio::test]
async fn test_kernel_agent_registry_has_default_assistant() {
    let tmp = TempDir::new().expect("create temp dir");
    let cfg = test_config(&tmp);
    let kernel = Arc::new(
        OpenFangKernel::boot_with_config(cfg)
            .await
            .expect("kernel boot"),
    );
    kernel.set_self_handle();

    // On a fresh install (no persisted agents), the kernel automatically spawns
    // a default "assistant" agent.  The registry should therefore contain exactly
    // 1 agent after boot.
    let count = kernel.registry.count();
    assert_eq!(
        count, 1,
        "Fresh kernel should have exactly 1 default assistant in the registry; found {count}"
    );
    let agents = kernel.registry.list();
    assert_eq!(
        agents[0].name, "assistant",
        "The default agent should be named 'assistant'"
    );
}

#[tokio::test]
async fn test_kernel_hand_registry_has_bundled_hands() {
    let tmp = TempDir::new().expect("create temp dir");
    let cfg = test_config(&tmp);
    let kernel = Arc::new(
        OpenFangKernel::boot_with_config(cfg)
            .await
            .expect("kernel boot"),
    );

    let hands = kernel.hand_registry.list_definitions();
    assert!(
        hands.len() >= 7,
        "Kernel hand registry should have at least 7 bundled Hands; found {}",
        hands.len()
    );
}

#[tokio::test]
async fn test_kernel_config_api_key_is_preserved() {
    let tmp = TempDir::new().expect("create temp dir");
    let mut cfg = test_config(&tmp);
    cfg.api_key = "test-key-12345".to_string();
    let kernel = Arc::new(
        OpenFangKernel::boot_with_config(cfg)
            .await
            .expect("kernel boot"),
    );

    assert_eq!(
        kernel.config.api_key, "test-key-12345",
        "Kernel config should reflect the api_key that was set"
    );
}
