//! End-to-end workflow integration tests.
//!
//! Tests the full pipeline: boot kernel → spawn agents → create workflow →
//! execute workflow → verify outputs flow through the pipeline.
//!
//! LLM tests require GROQ_API_KEY. Non-LLM tests verify the kernel-level
//! workflow wiring without making real API calls.

use openfang_kernel::workflow::{
    ErrorMode, StepAgent, StepMode, Workflow, WorkflowId, WorkflowStep,
};
use openfang_kernel::OpenFangKernel;
use openfang_types::agent::AgentManifest;
use openfang_types::config::{DefaultModelConfig, KernelConfig};
use std::sync::Arc;

fn test_config(provider: &str, model: &str, api_key_env: &str) -> KernelConfig {
    let tmp = tempfile::tempdir().unwrap();
    KernelConfig {
        home_dir: tmp.path().to_path_buf(),
        data_dir: tmp.path().join("data"),
        default_model: DefaultModelConfig {
            provider: provider.to_string(),
            model: model.to_string(),
            api_key_env: api_key_env.to_string(),
            base_url: None,
            subprocess_timeout_secs: None,
        },
        ..KernelConfig::default()
    }
}

fn spawn_test_agent(
    kernel: &OpenFangKernel,
    name: &str,
    system_prompt: &str,
) -> openfang_types::agent::AgentId {
    let manifest_str = format!(
        r#"
name = "{name}"
version = "0.1.0"
description = "Workflow test agent: {name}"
author = "test"
module = "builtin:chat"

[model]
provider = "groq"
model = "llama-3.3-70b-versatile"
system_prompt = "{system_prompt}"

[capabilities]
memory_read = ["*"]
memory_write = ["self.*"]
"#
    );
    let manifest: AgentManifest = toml::from_str(&manifest_str).unwrap();
    kernel.spawn_agent(manifest).expect("Agent should spawn")
}

// ---------------------------------------------------------------------------
// Kernel-level workflow wiring tests (no LLM needed)
// ---------------------------------------------------------------------------

/// Test that workflow registration and agent resolution work at the kernel level.
#[tokio::test]
async fn test_workflow_register_and_resolve() {
    let config = test_config("ollama", "test-model", "OLLAMA_API_KEY");
    let kernel = OpenFangKernel::boot_with_config(config).expect("Kernel should boot");
    let kernel = Arc::new(kernel);

    // Spawn agents
    let manifest: AgentManifest = toml::from_str(
        r#"
name = "agent-alpha"
version = "0.1.0"
description = "Alpha"
author = "test"
module = "builtin:chat"

[model]
provider = "ollama"
model = "test"
system_prompt = "Alpha."

[capabilities]
memory_read = ["*"]
memory_write = ["self.*"]
"#,
    )
    .unwrap();
    let alpha_id = kernel.spawn_agent(manifest).unwrap();

    let manifest2: AgentManifest = toml::from_str(
        r#"
name = "agent-beta"
version = "0.1.0"
description = "Beta"
author = "test"
module = "builtin:chat"

[model]
provider = "ollama"
model = "test"
system_prompt = "Beta."

[capabilities]
memory_read = ["*"]
memory_write = ["self.*"]
"#,
    )
    .unwrap();
    let beta_id = kernel.spawn_agent(manifest2).unwrap();

    // Create a 2-step workflow referencing agents by name
    let workflow = Workflow {
        id: WorkflowId::new(),
        name: "alpha-beta-pipeline".to_string(),
        description: "Tests agent resolution by name".to_string(),
        steps: vec![
            WorkflowStep {
                name: "step-alpha".to_string(),
                agent: StepAgent::ByName {
                    name: "agent-alpha".to_string(),
                },
                prompt_template: "Analyze: {{input}}".to_string(),
                mode: StepMode::Sequential,
                timeout_secs: 30,
                error_mode: ErrorMode::Fail,
                output_var: Some("alpha_out".to_string()),
            },
            WorkflowStep {
                name: "step-beta".to_string(),
                agent: StepAgent::ByName {
                    name: "agent-beta".to_string(),
                },
                prompt_template: "Summarize: {{input}} (alpha said: {{alpha_out}})".to_string(),
                mode: StepMode::Sequential,
                timeout_secs: 30,
                error_mode: ErrorMode::Fail,
                output_var: None,
            },
        ],
        created_at: chrono::Utc::now(),
    };

    let wf_id = kernel.register_workflow(workflow).await;

    // Verify workflow is registered
    let workflows = kernel.workflows.list_workflows().await;
    assert_eq!(workflows.len(), 1);
    assert_eq!(workflows[0].name, "alpha-beta-pipeline");

    // Verify agents can be found by name
    let alpha = kernel.registry.find_by_name("agent-alpha");
    assert!(alpha.is_some());
    assert_eq!(alpha.unwrap().id, alpha_id);

    let beta = kernel.registry.find_by_name("agent-beta");
    assert!(beta.is_some());
    assert_eq!(beta.unwrap().id, beta_id);

    // Verify workflow run can be created
    let run_id = kernel
        .workflows
        .create_run(wf_id, "test input".to_string())
        .await;
    assert!(run_id.is_some());

    let run = kernel.workflows.get_run(run_id.unwrap()).await.unwrap();
    assert_eq!(run.input, "test input");

    kernel.shutdown();
}

/// Test workflow with agent referenced by ID.
#[tokio::test]
async fn test_workflow_agent_by_id() {
    let config = test_config("ollama", "test-model", "OLLAMA_API_KEY");
    let kernel = OpenFangKernel::boot_with_config(config).expect("Kernel should boot");

    let manifest: AgentManifest = toml::from_str(
        r#"
name = "id-agent"
version = "0.1.0"
description = "Test"
author = "test"
module = "builtin:chat"

[model]
provider = "ollama"
model = "test"
system_prompt = "Test."

[capabilities]
memory_read = ["*"]
memory_write = ["self.*"]
"#,
    )
    .unwrap();
    let agent_id = kernel.spawn_agent(manifest).unwrap();

    let workflow = Workflow {
        id: WorkflowId::new(),
        name: "by-id-test".to_string(),
        description: "".to_string(),
        steps: vec![WorkflowStep {
            name: "step1".to_string(),
            agent: StepAgent::ById {
                id: agent_id.to_string(),
            },
            prompt_template: "{{input}}".to_string(),
            mode: StepMode::Sequential,
            timeout_secs: 30,
            error_mode: ErrorMode::Fail,
            output_var: None,
        }],
        created_at: chrono::Utc::now(),
    };

    let wf_id = kernel.register_workflow(workflow).await;

    // Can create run (agent resolution happens at execute time)
    let run_id = kernel
        .workflows
        .create_run(wf_id, "hello".to_string())
        .await;
    assert!(run_id.is_some());

    kernel.shutdown();
}

/// Test trigger registration and listing at kernel level.
#[tokio::test]
async fn test_trigger_registration_with_kernel() {
    use openfang_kernel::triggers::TriggerPattern;

    let config = test_config("ollama", "test-model", "OLLAMA_API_KEY");
    let kernel = OpenFangKernel::boot_with_config(config).expect("Kernel should boot");

    let manifest: AgentManifest = toml::from_str(
        r#"
name = "trigger-agent"
version = "0.1.0"
description = "Trigger test"
author = "test"
module = "builtin:chat"

[model]
provider = "ollama"
model = "test"
system_prompt = "Test."

[capabilities]
memory_read = ["*"]
memory_write = ["self.*"]
"#,
    )
    .unwrap();
    let agent_id = kernel.spawn_agent(manifest).unwrap();

    // Register triggers
    let t1 = kernel
        .register_trigger(
            agent_id,
            TriggerPattern::Lifecycle,
            "Lifecycle event: {{event}}".to_string(),
            0,
        )
        .unwrap();

    let t2 = kernel
        .register_trigger(
            agent_id,
            TriggerPattern::SystemKeyword {
                keyword: "deploy".to_string(),
            },
            "Deploy event: {{event}}".to_string(),
            5,
        )
        .unwrap();

    // List all triggers
    let all = kernel.list_triggers(None);
    assert_eq!(all.len(), 2);

    // List triggers for specific agent
    let agent_triggers = kernel.list_triggers(Some(agent_id));
    assert_eq!(agent_triggers.len(), 2);

    // Remove one
    assert!(kernel.remove_trigger(t1));
    let remaining = kernel.list_triggers(None);
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, t2);

    kernel.shutdown();
}

// ---------------------------------------------------------------------------
// Full E2E with real LLM (skip if no GROQ_API_KEY)
// ---------------------------------------------------------------------------

/// End-to-end: boot kernel → spawn 2 agents → create 2-step workflow →
/// run it through the real Groq LLM → verify output flows from step 1 to step 2.
#[tokio::test]
async fn test_workflow_e2e_with_groq() {
    if std::env::var("GROQ_API_KEY").is_err() {
        eprintln!("GROQ_API_KEY not set, skipping E2E workflow test");
        return;
    }

    let config = test_config("groq", "llama-3.3-70b-versatile", "GROQ_API_KEY");
    let kernel = OpenFangKernel::boot_with_config(config).expect("Kernel should boot");
    let kernel = Arc::new(kernel);
    kernel.set_self_handle();

    // Spawn two agents with distinct roles
    let _analyst_id = spawn_test_agent(
        &kernel,
        "wf-analyst",
        "You are an analyst. When given text, respond with exactly: ANALYSIS: followed by a one-sentence analysis.",
    );
    let _writer_id = spawn_test_agent(
        &kernel,
        "wf-writer",
        "You are a writer. When given text, respond with exactly: SUMMARY: followed by a one-sentence summary.",
    );

    // Create a 2-step pipeline: analyst → writer
    let workflow = Workflow {
        id: WorkflowId::new(),
        name: "analyst-writer-pipeline".to_string(),
        description: "E2E integration test workflow".to_string(),
        steps: vec![
            WorkflowStep {
                name: "analyze".to_string(),
                agent: StepAgent::ByName {
                    name: "wf-analyst".to_string(),
                },
                prompt_template: "Analyze the following: {{input}}".to_string(),
                mode: StepMode::Sequential,
                timeout_secs: 60,
                error_mode: ErrorMode::Fail,
                output_var: None,
            },
            WorkflowStep {
                name: "summarize".to_string(),
                agent: StepAgent::ByName {
                    name: "wf-writer".to_string(),
                },
                prompt_template: "Summarize this analysis: {{input}}".to_string(),
                mode: StepMode::Sequential,
                timeout_secs: 60,
                error_mode: ErrorMode::Fail,
                output_var: None,
            },
        ],
        created_at: chrono::Utc::now(),
    };

    let wf_id = kernel.register_workflow(workflow).await;

    // Run the workflow
    let result = kernel
        .run_workflow(
            wf_id,
            "The Rust programming language is growing rapidly.".to_string(),
        )
        .await;

    assert!(
        result.is_ok(),
        "Workflow should complete: {:?}",
        result.err()
    );
    let (run_id, output) = result.unwrap();

    println!("\n=== WORKFLOW OUTPUT ===");
    println!("{output}");
    println!("======================\n");

    assert!(!output.is_empty(), "Workflow output should not be empty");

    // Verify the workflow run record
    let run = kernel.workflows.get_run(run_id).await.unwrap();
    assert!(matches!(
        run.state,
        openfang_kernel::workflow::WorkflowRunState::Completed
    ));
    assert_eq!(run.step_results.len(), 2);
    assert_eq!(run.step_results[0].step_name, "analyze");
    assert_eq!(run.step_results[1].step_name, "summarize");

    // Both steps should have used tokens
    assert!(run.step_results[0].input_tokens > 0);
    assert!(run.step_results[0].output_tokens > 0);
    assert!(run.step_results[1].input_tokens > 0);
    assert!(run.step_results[1].output_tokens > 0);

    // List runs
    let runs = kernel.workflows.list_runs(None).await;
    assert_eq!(runs.len(), 1);

    kernel.shutdown();
}
