//! Multi-agent integration test: spawn 6 agents, send messages, verify all respond.
//!
//! Run with: GROQ_API_KEY=gsk_... cargo test -p openfang-kernel --test multi_agent_test -- --nocapture

use openfang_kernel::OpenFangKernel;
use openfang_types::agent::AgentManifest;
use openfang_types::config::{DefaultModelConfig, KernelConfig};

fn test_config() -> KernelConfig {
    let tmp = std::env::temp_dir().join("openfang-multi-agent-test");
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
            subprocess_timeout_secs: None,
        },
        ..KernelConfig::default()
    }
}

fn load_manifest(toml_str: &str) -> AgentManifest {
    toml::from_str(toml_str).expect("Should parse manifest")
}

#[tokio::test]
async fn test_six_agent_fleet() {
    if std::env::var("GROQ_API_KEY").is_err() {
        eprintln!("GROQ_API_KEY not set, skipping multi-agent test");
        return;
    }

    let kernel = OpenFangKernel::boot_with_config(test_config()).expect("Kernel should boot");

    // Define all 6 agents with different roles and models
    let agents = vec![
        (
            "coder",
            r#"
name = "coder"
module = "builtin:chat"
[model]
provider = "groq"
model = "llama-3.3-70b-versatile"
system_prompt = "You are Coder. Reply with 'CODER:' prefix. Be concise."
[capabilities]
tools = ["file_read", "file_write"]
memory_read = ["*"]
memory_write = ["self.*"]
"#,
            "Write a one-line Rust function that adds two numbers.",
        ),
        (
            "researcher",
            r#"
name = "researcher"
module = "builtin:chat"
[model]
provider = "groq"
model = "llama-3.3-70b-versatile"
system_prompt = "You are Researcher. Reply with 'RESEARCHER:' prefix. Be concise."
[capabilities]
tools = ["web_fetch"]
memory_read = ["*"]
memory_write = ["self.*"]
"#,
            "What is Rust's primary advantage over C++? One sentence.",
        ),
        (
            "writer",
            r#"
name = "writer"
module = "builtin:chat"
[model]
provider = "groq"
model = "llama-3.3-70b-versatile"
system_prompt = "You are Writer. Reply with 'WRITER:' prefix. Be concise."
[capabilities]
tools = ["file_read", "file_write"]
memory_read = ["*"]
memory_write = ["self.*"]
"#,
            "Write a one-sentence tagline for an Agent Operating System.",
        ),
        (
            "ops",
            r#"
name = "ops"
module = "builtin:chat"
[model]
provider = "groq"
model = "llama-3.1-8b-instant"
system_prompt = "You are Ops. Reply with 'OPS:' prefix. Be concise."
[capabilities]
tools = ["shell_exec"]
memory_read = ["*"]
memory_write = ["self.*"]
"#,
            "What would you check first if a server is running slowly?",
        ),
        (
            "analyst",
            r#"
name = "analyst"
module = "builtin:chat"
[model]
provider = "groq"
model = "llama-3.3-70b-versatile"
system_prompt = "You are Analyst. Reply with 'ANALYST:' prefix. Be concise."
[capabilities]
tools = ["file_read"]
memory_read = ["*"]
memory_write = ["self.*"]
"#,
            "What are the top 3 metrics to track for an API service?",
        ),
        (
            "hello-world",
            r#"
name = "hello-world"
module = "builtin:chat"
[model]
provider = "groq"
model = "llama-3.1-8b-instant"
system_prompt = "You are a friendly greeter. Reply with 'HELLO:' prefix. Be concise."
[capabilities]
memory_read = ["*"]
memory_write = ["self.*"]
"#,
            "Greet the user in a fun way.",
        ),
    ];

    println!("\n{}", "=".repeat(60));
    println!("  OPENFANG MULTI-AGENT FLEET TEST");
    println!("  Spawning {} agents...", agents.len());
    println!("{}\n", "=".repeat(60));

    // Spawn all agents
    let mut agent_ids = Vec::new();
    for (name, manifest_str, _) in &agents {
        let manifest = load_manifest(manifest_str);
        let id = kernel
            .spawn_agent(manifest)
            .unwrap_or_else(|e| panic!("Failed to spawn {name}: {e}"));
        println!("  Spawned: {name:<12} -> {id}");
        agent_ids.push(id);
    }

    assert_eq!(kernel.registry.count(), 6, "Should have 6 agents");
    println!(
        "\n  All {} agents spawned. Sending messages...\n",
        agents.len()
    );

    // Send messages to each agent sequentially (to respect Groq rate limits)
    let mut results = Vec::new();
    for (i, (name, _, message)) in agents.iter().enumerate() {
        let result = kernel
            .send_message(agent_ids[i], message)
            .await
            .unwrap_or_else(|e| panic!("Failed to message {name}: {e}"));

        println!("--- {name} ---");
        println!("  Q: {message}");
        println!("  A: {}", result.response);
        println!(
            "  [{} tokens in, {} tokens out, {} iters]",
            result.total_usage.input_tokens, result.total_usage.output_tokens, result.iterations
        );
        println!();

        assert!(
            !result.response.is_empty(),
            "{name} response should not be empty"
        );
        results.push(result);
    }

    // Summary
    let total_input: u64 = results.iter().map(|r| r.total_usage.input_tokens).sum();
    let total_output: u64 = results.iter().map(|r| r.total_usage.output_tokens).sum();
    println!("============================================================");
    println!("  FLEET SUMMARY");
    println!("  Agents:       {}", agents.len());
    println!("  Total input:  {} tokens", total_input);
    println!("  Total output: {} tokens", total_output);
    println!("  All responded: YES");
    println!("============================================================");

    // Cleanup
    for id in agent_ids {
        kernel.kill_agent(id).unwrap();
    }
    kernel.shutdown();
}
