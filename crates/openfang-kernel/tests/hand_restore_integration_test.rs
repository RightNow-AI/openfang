use openfang_kernel::OpenFangKernel;
use openfang_types::config::{DefaultModelConfig, KernelConfig};
use std::sync::Arc;
use tempfile::tempdir;

fn test_config(home_dir: std::path::PathBuf) -> KernelConfig {
    KernelConfig {
        home_dir: home_dir.clone(),
        data_dir: home_dir.join("data"),
        default_model: DefaultModelConfig {
            provider: "ollama".to_string(),
            model: "test-model".to_string(),
            api_key_env: "OLLAMA_API_KEY".to_string(),
            base_url: None,
        },
        ..KernelConfig::default()
    }
}

#[tokio::test]
async fn external_hand_restores_after_restart_from_persisted_content() {
    let home = tempdir().unwrap();
    let hand_dir = tempdir().unwrap();

    std::fs::write(
        hand_dir.path().join("HAND.toml"),
        r#"
id = "external-restart-test"
name = "External Restart Test"
description = "Restores across daemon restarts"
category = "development"
tools = []

[agent]
name = "external-restart-hand"
description = "External restart test agent"
system_prompt = "You are a restart test hand."

[dashboard]
metrics = []
"#,
    )
    .unwrap();
    std::fs::write(
        hand_dir.path().join("SKILL.md"),
        "This external hand should survive restarts.",
    )
    .unwrap();

    let kernel = OpenFangKernel::boot_with_config(test_config(home.path().to_path_buf())).unwrap();
    kernel
        .hand_registry
        .install_from_path(hand_dir.path())
        .unwrap();
    let instance = kernel
        .activate_hand("external-restart-test", Default::default())
        .unwrap();
    assert_eq!(instance.hand_id, "external-restart-test");
    kernel.shutdown();

    std::fs::remove_dir_all(hand_dir.path()).unwrap();

    let kernel =
        Arc::new(OpenFangKernel::boot_with_config(test_config(home.path().to_path_buf())).unwrap());
    kernel.set_self_handle();
    kernel.start_background_agents();

    assert!(kernel
        .hand_registry
        .get_definition("external-restart-test")
        .is_some());

    let instances = kernel.hand_registry.list_instances();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].hand_id, "external-restart-test");
    assert!(instances[0].agent_id.is_some());

    kernel.shutdown();
}

#[tokio::test]
async fn invalid_hand_state_marks_restore_health_not_ready() {
    let home = tempdir().unwrap();
    std::fs::write(home.path().join("hand_state.json"), "{bad json").unwrap();

    let kernel =
        Arc::new(OpenFangKernel::boot_with_config(test_config(home.path().to_path_buf())).unwrap());
    kernel.set_self_handle();
    kernel.start_background_agents();

    let restore = kernel.restore_health_status();
    assert!(!restore.hand_warnings.is_empty());
    assert!(restore
        .hand_warnings
        .iter()
        .any(|warning| warning.contains("hand state restore failed")));

    kernel.shutdown();
}
