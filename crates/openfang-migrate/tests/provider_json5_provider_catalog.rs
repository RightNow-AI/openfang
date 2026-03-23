use openfang_migrate::{run_migration, MigrateOptions, MigrateSource};
use tempfile::TempDir;

#[test]
fn json5_catalog_preserves_custom_openai_provider_and_base_url() {
    let source = TempDir::new().expect("create source tempdir");
    let target = TempDir::new().expect("create target tempdir");

    let json5 = r#"{
  models: {
    providers: {
      qwencode: {
        baseUrl: "https://coding.dashscope.aliyuncs.com/v1",
        api: "openai-completions"
      }
    }
  },
  agents: {
    defaults: {
      model: "qwencode/glm-5"
    },
    list: [
      { id: "coder" }
    ]
  }
}"#;

    std::fs::write(source.path().join("openclaw.json"), json5).expect("write openclaw.json");

    let options = MigrateOptions {
        source: MigrateSource::OpenClaw,
        source_dir: source.path().to_path_buf(),
        target_dir: target.path().to_path_buf(),
        dry_run: false,
    };
    run_migration(&options).expect("migration succeeds");

    let cfg = std::fs::read_to_string(target.path().join("config.toml")).expect("read config.toml");
    let cfg_val: toml::Value = toml::from_str(&cfg).expect("parse config.toml");
    let dm = cfg_val
        .get("default_model")
        .and_then(toml::Value::as_table)
        .expect("default_model table");

    assert_eq!(
        dm.get("provider").and_then(toml::Value::as_str),
        Some("qwencode")
    );
    assert_eq!(dm.get("model").and_then(toml::Value::as_str), Some("glm-5"));
    assert_eq!(
        dm.get("api_key_env").and_then(toml::Value::as_str),
        Some("QWENCODE_API_KEY")
    );
    assert_eq!(
        dm.get("base_url").and_then(toml::Value::as_str),
        Some("https://coding.dashscope.aliyuncs.com/v1")
    );

    let agent = std::fs::read_to_string(target.path().join("agents/coder/agent.toml"))
        .expect("read migrated agent");
    let agent_val: toml::Value = toml::from_str(&agent).expect("parse agent.toml");
    let model = agent_val
        .get("model")
        .and_then(toml::Value::as_table)
        .expect("model table");

    assert_eq!(
        model.get("provider").and_then(toml::Value::as_str),
        Some("qwencode")
    );
    assert_eq!(
        model.get("base_url").and_then(toml::Value::as_str),
        Some("https://coding.dashscope.aliyuncs.com/v1")
    );
}

#[test]
fn json5_catalog_api_hint_maps_custom_provider_to_anthropic_driver() {
    let source = TempDir::new().expect("create source tempdir");
    let target = TempDir::new().expect("create target tempdir");

    let json5 = r#"{
  models: {
    providers: {
      kimicode: {
        baseUrl: "https://api.kimi.com/coding",
        api: "anthropic-messages"
      }
    }
  },
  agents: {
    list: [
      {
        id: "writer",
        model: {
          primary: "kimicode/kimi-k2.5",
          fallbacks: ["kimicode/kimi-k2.5"]
        }
      }
    ]
  }
}"#;

    std::fs::write(source.path().join("openclaw.json"), json5).expect("write openclaw.json");

    let options = MigrateOptions {
        source: MigrateSource::OpenClaw,
        source_dir: source.path().to_path_buf(),
        target_dir: target.path().to_path_buf(),
        dry_run: false,
    };
    run_migration(&options).expect("migration succeeds");

    let agent = std::fs::read_to_string(target.path().join("agents/writer/agent.toml"))
        .expect("read migrated agent");
    let agent_val: toml::Value = toml::from_str(&agent).expect("parse agent.toml");

    let model = agent_val
        .get("model")
        .and_then(toml::Value::as_table)
        .expect("model table");
    assert_eq!(
        model.get("provider").and_then(toml::Value::as_str),
        Some("anthropic")
    );
    assert_eq!(
        model.get("api_key_env").and_then(toml::Value::as_str),
        Some("ANTHROPIC_API_KEY")
    );
    assert_eq!(
        model.get("base_url").and_then(toml::Value::as_str),
        Some("https://api.kimi.com/coding")
    );

    let fallback_models = agent_val
        .get("fallback_models")
        .and_then(toml::Value::as_array)
        .expect("fallback models exist");
    let fb = fallback_models
        .first()
        .and_then(toml::Value::as_table)
        .expect("first fallback table");

    assert_eq!(
        fb.get("provider").and_then(toml::Value::as_str),
        Some("anthropic")
    );
    assert_eq!(
        fb.get("base_url").and_then(toml::Value::as_str),
        Some("https://api.kimi.com/coding")
    );
}

#[test]
fn json5_catalog_unknown_openai_without_base_url_falls_back_to_openai() {
    let source = TempDir::new().expect("create source tempdir");
    let target = TempDir::new().expect("create target tempdir");

    let json5 = r#"{
  models: {
    providers: {
      mygateway: {
        api: "openai-completions"
      }
    }
  },
  agents: {
    defaults: {
      model: "mygateway/gpt-like"
    },
    list: [
      { id: "fallback-check" }
    ]
  }
}"#;

    std::fs::write(source.path().join("openclaw.json"), json5).expect("write openclaw.json");

    let options = MigrateOptions {
        source: MigrateSource::OpenClaw,
        source_dir: source.path().to_path_buf(),
        target_dir: target.path().to_path_buf(),
        dry_run: false,
    };
    run_migration(&options).expect("migration succeeds");

    let cfg = std::fs::read_to_string(target.path().join("config.toml")).expect("read config.toml");
    let cfg_val: toml::Value = toml::from_str(&cfg).expect("parse config.toml");
    let dm = cfg_val
        .get("default_model")
        .and_then(toml::Value::as_table)
        .expect("default_model table");

    assert_eq!(
        dm.get("provider").and_then(toml::Value::as_str),
        Some("openai")
    );
    assert_eq!(
        dm.get("model").and_then(toml::Value::as_str),
        Some("gpt-like")
    );
    assert!(dm.get("base_url").is_none());

    let agent = std::fs::read_to_string(target.path().join("agents/fallback-check/agent.toml"))
        .expect("read migrated agent");
    let agent_val: toml::Value = toml::from_str(&agent).expect("parse agent.toml");
    let model = agent_val
        .get("model")
        .and_then(toml::Value::as_table)
        .expect("model table");

    assert_eq!(
        model.get("provider").and_then(toml::Value::as_str),
        Some("openai")
    );
    assert_eq!(
        model.get("api_key_env").and_then(toml::Value::as_str),
        Some("OPENAI_API_KEY")
    );
    assert!(model.get("base_url").is_none());
}

#[test]
fn json5_catalog_alias_key_mismatch_still_uses_catalog_metadata() {
    let source = TempDir::new().expect("create source tempdir");
    let target = TempDir::new().expect("create target tempdir");

    let json5 = r#"{
  models: {
    providers: {
      openai: {
        baseUrl: "https://proxy.example/v1",
        api: "openai-completions"
      }
    }
  },
  agents: {
    list: [
      {
        id: "alias-check",
        model: "gpt/gpt-4.1-mini"
      }
    ]
  }
}"#;

    std::fs::write(source.path().join("openclaw.json"), json5).expect("write openclaw.json");

    let options = MigrateOptions {
        source: MigrateSource::OpenClaw,
        source_dir: source.path().to_path_buf(),
        target_dir: target.path().to_path_buf(),
        dry_run: false,
    };
    run_migration(&options).expect("migration succeeds");

    let agent = std::fs::read_to_string(target.path().join("agents/alias-check/agent.toml"))
        .expect("read migrated agent");
    let agent_val: toml::Value = toml::from_str(&agent).expect("parse agent.toml");
    let model = agent_val
        .get("model")
        .and_then(toml::Value::as_table)
        .expect("model table");

    assert_eq!(
        model.get("provider").and_then(toml::Value::as_str),
        Some("openai")
    );
    assert_eq!(
        model.get("api_key_env").and_then(toml::Value::as_str),
        Some("OPENAI_API_KEY")
    );
    assert_eq!(
        model.get("base_url").and_then(toml::Value::as_str),
        Some("https://proxy.example/v1")
    );
}
