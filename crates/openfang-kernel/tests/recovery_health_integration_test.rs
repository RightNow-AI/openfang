use openfang_hands::registry::HandRegistry;
use openfang_kernel::OpenFangKernel;
use openfang_types::config::{DefaultModelConfig, KernelConfig};
use std::collections::HashMap;
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

#[test]
fn cron_backup_recovery_marks_restore_health_not_ready() {
    let home = tempdir().unwrap();
    std::fs::write(home.path().join("cron_jobs.json"), "{bad json").unwrap();
    std::fs::write(home.path().join("cron_jobs.json.bak"), "[]").unwrap();

    let kernel = OpenFangKernel::boot_with_config(test_config(home.path().to_path_buf())).unwrap();
    let restore = kernel.restore_health_status();

    assert!(restore
        .cron_warnings
        .iter()
        .any(|warning| warning.contains("recovered from backup")));
    kernel.shutdown();
}

#[tokio::test]
async fn hand_backup_recovery_marks_restore_health_not_ready() {
    let home = tempdir().unwrap();

    let reg = HandRegistry::new();
    reg.load_bundled();
    reg.activate("clip", HashMap::new()).unwrap();

    let state_path = home.path().join("hand_state.json");
    reg.persist_state(&state_path).unwrap();
    std::fs::copy(&state_path, home.path().join("hand_state.json.bak")).unwrap();
    std::fs::write(&state_path, "{bad json").unwrap();

    let kernel =
        Arc::new(OpenFangKernel::boot_with_config(test_config(home.path().to_path_buf())).unwrap());
    kernel.set_self_handle();
    kernel.start_background_agents();

    let restore = kernel.restore_health_status();
    assert!(restore
        .hand_warnings
        .iter()
        .any(|warning| warning.contains("recovered from backup")));

    kernel.shutdown();
}
