use std::fs;
use std::path::Path;

use openfang_migrate::{run_migration, MigrateOptions, MigrateSource};
use tempfile::TempDir;

fn create_legacy_workspace(
    source_dir: &Path,
    config_provider: &str,
    config_model: &str,
    agent_id: &str,
    agent_provider: &str,
    agent_model: &str,
) {
    fs::write(
        source_dir.join("config.yaml"),
        format!("provider: {config_provider}\nmodel: {config_model}\n"),
    )
    .expect("write config.yaml");

    let agent_dir = source_dir.join("agents").join(agent_id);
    fs::create_dir_all(&agent_dir).expect("create agent dir");
    fs::write(
        agent_dir.join("agent.yaml"),
        format!(
            "name: {agent_id}\n\
             description: provider alias mapping test\n\
             provider: {agent_provider}\n\
             model: {agent_model}\n"
        ),
    )
    .expect("write agent.yaml");
}

fn migrate_legacy_workspace(source_dir: &Path, target_dir: &Path) {
    let options = MigrateOptions {
        source: MigrateSource::OpenClaw,
        source_dir: source_dir.to_path_buf(),
        target_dir: target_dir.to_path_buf(),
        dry_run: false,
    };
    run_migration(&options).expect("legacy YAML migration should succeed");
}

fn assert_config_model_mapping(
    target_dir: &Path,
    expected_provider: &str,
    expected_api_key_env: &str,
) {
    let config_toml = fs::read_to_string(target_dir.join("config.toml")).expect("read config.toml");
    let config: toml::Value = toml::from_str(&config_toml).expect("parse config.toml");
    let model = config
        .get("default_model")
        .expect("config.toml should have [default_model]");

    assert_eq!(
        model.get("provider").and_then(toml::Value::as_str),
        Some(expected_provider)
    );
    assert_eq!(
        model.get("api_key_env").and_then(toml::Value::as_str),
        Some(expected_api_key_env)
    );
}

fn assert_agent_model_mapping(
    target_dir: &Path,
    agent_id: &str,
    expected_provider: &str,
    expected_api_key_env: &str,
) {
    let agent_toml =
        fs::read_to_string(target_dir.join("agents").join(agent_id).join("agent.toml"))
            .expect("read agent.toml");
    let agent: toml::Value = toml::from_str(&agent_toml).expect("parse agent.toml");
    let model = agent.get("model").expect("agent.toml should have [model]");

    assert_eq!(
        model.get("provider").and_then(toml::Value::as_str),
        Some(expected_provider)
    );
    assert_eq!(
        model.get("api_key_env").and_then(toml::Value::as_str),
        Some(expected_api_key_env)
    );
}

#[test]
fn legacy_yaml_provider_alias_mapping_kimicode_and_copilot() {
    let source = TempDir::new().expect("create source tempdir");
    let target = TempDir::new().expect("create target tempdir");
    let agent_id = "coder";

    create_legacy_workspace(
        source.path(),
        "kimicode",
        "kimi-k2",
        agent_id,
        "copilot",
        "gpt-4.1",
    );
    migrate_legacy_workspace(source.path(), target.path());

    assert_config_model_mapping(target.path(), "moonshot", "MOONSHOT_API_KEY");
    assert_agent_model_mapping(target.path(), agent_id, "github-copilot", "GITHUB_TOKEN");
}

#[test]
fn legacy_yaml_provider_alias_mapping_qwencode_and_lmstudio() {
    let source = TempDir::new().expect("create source tempdir");
    let target = TempDir::new().expect("create target tempdir");
    let agent_id = "assistant";

    create_legacy_workspace(
        source.path(),
        "qwencode",
        "qwen-plus",
        agent_id,
        "lmstudio",
        "local-model",
    );
    migrate_legacy_workspace(source.path(), target.path());

    assert_config_model_mapping(target.path(), "qwen", "DASHSCOPE_API_KEY");
    assert_agent_model_mapping(target.path(), agent_id, "lmstudio", "LMSTUDIO_API_KEY");
}
