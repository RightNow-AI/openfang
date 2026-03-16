use openfang_migrate::{openclaw, MigrateOptions, MigrateSource};
use tempfile::TempDir;

fn assert_default_model_mapping(
    model_ref: &str,
    expected_provider: &str,
    expected_api_key_env: &str,
) {
    let source = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();

    let openclaw_json = format!(
        r#"{{
  agents: {{
    defaults: {{ model: "{model_ref}" }},
    list: [
      {{ id: "tester" }}
    ]
  }}
}}"#
    );
    std::fs::write(source.path().join("openclaw.json"), openclaw_json).unwrap();

    let options = MigrateOptions {
        source: MigrateSource::OpenClaw,
        source_dir: source.path().to_path_buf(),
        target_dir: target.path().to_path_buf(),
        dry_run: false,
    };
    openclaw::migrate(&options).unwrap();

    let config_toml = std::fs::read_to_string(target.path().join("config.toml")).unwrap();
    let parsed: toml::Value = toml::from_str(&config_toml).unwrap();
    let default_model = parsed
        .get("default_model")
        .and_then(toml::Value::as_table)
        .expect("config.toml should contain [default_model]");

    assert_eq!(
        default_model.get("provider").and_then(toml::Value::as_str),
        Some(expected_provider)
    );
    assert_eq!(
        default_model
            .get("api_key_env")
            .and_then(toml::Value::as_str),
        Some(expected_api_key_env)
    );
}

#[test]
fn json5_default_model_qwencode_maps_to_qwen_and_dashscope_key() {
    assert_default_model_mapping("qwencode/glm-5", "qwen", "DASHSCOPE_API_KEY");
}

#[test]
fn json5_default_model_kimicode_maps_to_moonshot_and_moonshot_key() {
    assert_default_model_mapping("kimicode/kimi-k2.5", "moonshot", "MOONSHOT_API_KEY");
}

#[test]
fn json5_default_model_copilot_maps_to_github_copilot_and_github_token() {
    assert_default_model_mapping("copilot/gpt-4.1", "github-copilot", "GITHUB_TOKEN");
}
