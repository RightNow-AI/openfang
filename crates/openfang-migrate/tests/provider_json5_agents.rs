use openfang_migrate::{run_migration, MigrateOptions, MigrateSource};
use tempfile::TempDir;

fn migrate_with_json5(json5_content: &str) -> (TempDir, TempDir) {
    let source = TempDir::new().expect("create source tempdir");
    let target = TempDir::new().expect("create target tempdir");

    std::fs::write(source.path().join("openclaw.json"), json5_content)
        .expect("write openclaw.json");

    let options = MigrateOptions {
        source: MigrateSource::OpenClaw,
        source_dir: source.path().to_path_buf(),
        target_dir: target.path().to_path_buf(),
        dry_run: false,
    };

    run_migration(&options).expect("run migration");
    (source, target)
}

#[test]
fn json5_agent_provider_and_fallback_api_key_env_mapping() {
    let json5_content = r#"{
  agents: {
    list: [
      {
        id: "provider-case",
        model: {
          primary: "qwencode/glm-5",
          fallbacks: [
            "kimicode/kimi-k2.5",
            "copilot/gpt-4.1",
            "vllm/llama3"
          ]
        }
      }
    ]
  }
}"#;

    let (_source, target) = migrate_with_json5(json5_content);

    let agent_toml = std::fs::read_to_string(
        target
            .path()
            .join("agents")
            .join("provider-case")
            .join("agent.toml"),
    )
    .expect("read migrated agent.toml");

    let parsed: toml::Value = toml::from_str(&agent_toml).expect("parse agent.toml");

    let model = parsed
        .get("model")
        .and_then(toml::Value::as_table)
        .expect("[model] exists");

    assert_eq!(
        model.get("provider").and_then(toml::Value::as_str),
        Some("qwen")
    );
    assert_eq!(
        model.get("api_key_env").and_then(toml::Value::as_str),
        Some("DASHSCOPE_API_KEY")
    );

    let fallback_models = parsed
        .get("fallback_models")
        .and_then(toml::Value::as_array)
        .expect("[[fallback_models]] exists");

    assert_eq!(fallback_models.len(), 3);

    let expected = [
        ("moonshot", "MOONSHOT_API_KEY"),
        ("github-copilot", "GITHUB_TOKEN"),
        ("vllm", "VLLM_API_KEY"),
    ];

    for (index, (expected_provider, expected_api_key_env)) in expected.iter().enumerate() {
        let fallback = fallback_models[index]
            .as_table()
            .expect("fallback entry is table");

        assert_eq!(
            fallback.get("provider").and_then(toml::Value::as_str),
            Some(*expected_provider)
        );
        assert_eq!(
            fallback.get("api_key_env").and_then(toml::Value::as_str),
            Some(*expected_api_key_env)
        );
    }
}

#[test]
fn json5_lmstudio_provider_api_key_env_mapping() {
    let json5_content = r#"{
  agents: {
    list: [
      {
        id: "lmstudio-case",
        model: "lmstudio/llama3"
      }
    ]
  }
}"#;

    let (_source, target) = migrate_with_json5(json5_content);

    let agent_toml = std::fs::read_to_string(
        target
            .path()
            .join("agents")
            .join("lmstudio-case")
            .join("agent.toml"),
    )
    .expect("read migrated lmstudio agent.toml");

    let parsed: toml::Value = toml::from_str(&agent_toml).expect("parse agent.toml");

    let model = parsed
        .get("model")
        .and_then(toml::Value::as_table)
        .expect("[model] exists");

    assert_eq!(
        model.get("provider").and_then(toml::Value::as_str),
        Some("lmstudio")
    );
    assert_eq!(
        model.get("api_key_env").and_then(toml::Value::as_str),
        Some("LMSTUDIO_API_KEY")
    );
}
