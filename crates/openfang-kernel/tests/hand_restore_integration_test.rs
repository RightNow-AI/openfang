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
async fn external_hand_workspace_scaffold_seeds_workspace_files() {
    let home = tempdir().unwrap();
    let hand_dir = tempdir().unwrap();
    let scaffold_dir = hand_dir.path().join("workspace-scaffold");
    std::fs::create_dir_all(&scaffold_dir).unwrap();
    for filename in [
        "SOUL.md",
        "IDENTITY.md",
        "USER.md",
        "TOOLS.md",
        "MEMORY.md",
        "AGENTS.md",
        "BOOTSTRAP.md",
        "HEARTBEAT.md",
    ] {
        std::fs::write(scaffold_dir.join(filename), format!("{filename} default\n")).unwrap();
    }

    std::fs::write(
        hand_dir.path().join("HAND.toml"),
        r#"
id = "external-scaffold-test"
name = "External Scaffold Test"
description = "Seeds workspace identity files from the hand source"
category = "development"
tools = []

[agent]
name = "external-scaffold-hand"
description = "External scaffold test agent"
system_prompt = "You are a scaffold test hand."

[dashboard]
metrics = []
"#,
    )
    .unwrap();
    std::fs::write(hand_dir.path().join("SKILL.md"), "Scaffold test skill.").unwrap();
    std::fs::write(
        scaffold_dir.join("AGENTS.md"),
        "# Agent Behavioral Guidelines\n\nCustom scaffold guidance.\n",
    )
    .unwrap();
    std::fs::write(
        scaffold_dir.join("TOOLS.md"),
        "# Tools & Environment\n\nCustom tool notes.\n",
    )
    .unwrap();
    std::fs::create_dir_all(scaffold_dir.join("prompts")).unwrap();
    std::fs::write(
        scaffold_dir.join("prompts").join("publish.txt"),
        "Nested scaffold prompt.\n",
    )
    .unwrap();

    let kernel = OpenFangKernel::boot_with_config(test_config(home.path().to_path_buf())).unwrap();
    kernel
        .hand_registry
        .install_from_path(hand_dir.path())
        .unwrap();
    let instance = kernel
        .activate_hand("external-scaffold-test", Default::default())
        .unwrap();
    let agent_id = instance.agent_id.unwrap();
    let entry = kernel.registry.get(agent_id).unwrap();
    let workspace = entry.manifest.workspace.clone().unwrap();

    assert_eq!(
        std::fs::read_to_string(workspace.join("AGENTS.md")).unwrap(),
        "# Agent Behavioral Guidelines\n\nCustom scaffold guidance.\n"
    );
    assert_eq!(
        std::fs::read_to_string(workspace.join("TOOLS.md")).unwrap(),
        "# Tools & Environment\n\nCustom tool notes.\n"
    );
    assert_eq!(
        std::fs::read_to_string(workspace.join("prompts").join("publish.txt")).unwrap(),
        "Nested scaffold prompt.\n"
    );
    assert!(workspace.join("SOUL.md").is_file());

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
