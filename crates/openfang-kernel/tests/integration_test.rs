//! Integration test: boot kernel -> spawn agent -> send message via Groq API.
//!
//! Run with: GROQ_API_KEY=gsk_... cargo test -p openfang-kernel --test integration_test -- --nocapture

use openfang_kernel::OpenFangKernel;
use openfang_types::agent::AgentManifest;
use openfang_types::config::{DefaultModelConfig, KernelConfig};

fn test_config() -> KernelConfig {
    let tmp = std::env::temp_dir().join("openfang-integration-test");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    KernelConfig {
        home_dir: tmp.clone(),
        data_dir: tmp.join("data"),
        default_model: DefaultModelConfig {
            provider: "groq".to_string(),
            model: "llama-3.3-70b-versatile".to_string(),
            api_key_env: "GROQ_API_KEY".to_string(),
            base_url: None,
        },
        ..KernelConfig::default()
    }
}

#[tokio::test]
async fn test_full_pipeline_with_groq() {
    if std::env::var("GROQ_API_KEY").is_err() {
        eprintln!("GROQ_API_KEY not set, skipping integration test");
        return;
    }

    // Boot kernel
    let config = test_config();
    let kernel = OpenFangKernel::boot_with_config(config).expect("Kernel should boot");

    // Spawn agent
    let manifest: AgentManifest = toml::from_str(
        r#"
name = "test-agent"
version = "0.1.0"
description = "Integration test agent"
author = "test"
module = "builtin:chat"

[model]
provider = "groq"
model = "llama-3.3-70b-versatile"
system_prompt = "You are a test agent. Reply concisely in one sentence."

[capabilities]
tools = ["file_read"]
memory_read = ["*"]
memory_write = ["self.*"]
"#,
    )
    .unwrap();

    let agent_id = kernel.spawn_agent(manifest).expect("Agent should spawn");

    // Send message
    let result = kernel
        .send_message(agent_id, "Say hello in exactly 5 words.")
        .await
        .expect("Message should get a response");

    println!("\n=== AGENT RESPONSE ===");
    println!("{}", result.response);
    println!(
        "=== USAGE: {} tokens in, {} tokens out, {} iterations ===",
        result.total_usage.input_tokens, result.total_usage.output_tokens, result.iterations
    );

    assert!(!result.response.is_empty(), "Response should not be empty");
    assert!(
        result.total_usage.input_tokens > 0,
        "Should have used tokens"
    );

    // Kill agent
    kernel.kill_agent(agent_id).expect("Agent should be killed");
    kernel.shutdown();
}

#[tokio::test]
async fn test_multiple_agents_different_models() {
    if std::env::var("GROQ_API_KEY").is_err() {
        eprintln!("GROQ_API_KEY not set, skipping integration test");
        return;
    }

    let config = test_config();
    let kernel = OpenFangKernel::boot_with_config(config).expect("Kernel should boot");

    // Spawn agent 1: llama 70b
    let manifest1: AgentManifest = toml::from_str(
        r#"
name = "agent-llama70b"
version = "0.1.0"
description = "Llama 70B agent"
author = "test"
module = "builtin:chat"

[model]
provider = "groq"
model = "llama-3.3-70b-versatile"
system_prompt = "You are Agent A. Always start your reply with 'A:'."

[capabilities]
memory_read = ["*"]
memory_write = ["self.*"]
"#,
    )
    .unwrap();

    // Spawn agent 2: llama 8b (faster, smaller)
    let manifest2: AgentManifest = toml::from_str(
        r#"
name = "agent-llama8b"
version = "0.1.0"
description = "Llama 8B agent"
author = "test"
module = "builtin:chat"

[model]
provider = "groq"
model = "llama-3.1-8b-instant"
system_prompt = "You are Agent B. Always start your reply with 'B:'."

[capabilities]
memory_read = ["*"]
memory_write = ["self.*"]
"#,
    )
    .unwrap();

    let id1 = kernel.spawn_agent(manifest1).expect("Agent 1 should spawn");
    let id2 = kernel.spawn_agent(manifest2).expect("Agent 2 should spawn");

    // Send messages to both
    let r1 = kernel
        .send_message(id1, "What model are you?")
        .await
        .expect("Agent 1 response");
    let r2 = kernel
        .send_message(id2, "What model are you?")
        .await
        .expect("Agent 2 response");

    println!("\n=== AGENT 1 (llama-70b) ===");
    println!("{}", r1.response);
    println!("\n=== AGENT 2 (llama-8b) ===");
    println!("{}", r2.response);

    assert!(!r1.response.is_empty());
    assert!(!r2.response.is_empty());

    // Cleanup
    kernel.kill_agent(id1).unwrap();
    kernel.kill_agent(id2).unwrap();
    kernel.shutdown();
}

#[test]
fn test_agent_manifest_skills_parsing() {
    let toml_str = r#"
name = "skills-test-agent"
version = "0.1.0"
description = "Test agent with skills"
author = "test"
module = "builtin:chat"

skills = ["Productivity", "web-search"]
mcp_servers = ["github"]

[model]
provider = "groq"
model = "llama-3.3-70b-versatile"

[capabilities]
tools = ["file_read"]

[resources]
max_llm_tokens_per_hour = 100000
"#;

    let manifest: AgentManifest = toml::from_str(toml_str).unwrap();
    assert_eq!(
        manifest.skills,
        vec!["Productivity", "web-search"],
        "Skills should be parsed correctly (must be at top level, not after [capabilities])"
    );
    assert_eq!(
        manifest.mcp_servers,
        vec!["github"],
        "MCP servers should be parsed correctly (must be at top level)"
    );
}
