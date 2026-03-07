//! End-to-end workflow integration tests.
//!
//! Tests the full pipeline: boot kernel → spawn agents → create workflow →
//! execute workflow → verify outputs flow through the pipeline.
//!
//! LLM tests require GROQ_API_KEY. Non-LLM tests verify the kernel-level
//! workflow wiring without making real API calls.

use openfang_kernel::workflow::{
    ErrorMode, StepAgent, StepMode, Workflow, WorkflowEngine, WorkflowId, WorkflowRunState,
    WorkflowStep,
};
use openfang_kernel::OpenFangKernel;
use openfang_types::agent::{AgentId, AgentManifest};
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

/// End-to-end workflow engine test (no LLM): review rejection returns to planning,
/// then approved output proceeds to dispatch.
#[tokio::test]
async fn test_workflow_e2e_reject_return_without_llm() {
    let engine = WorkflowEngine::new();
    let workflow = Workflow {
        id: WorkflowId::new(),
        name: "e2e-review-return".to_string(),
        description: "integration e2e for reject-return".to_string(),
        steps: vec![
            WorkflowStep {
                name: "planning".to_string(),
                agent: StepAgent::ByName {
                    name: "planner".to_string(),
                },
                prompt_template: "Plan: {{input}}".to_string(),
                mode: StepMode::Sequential,
                timeout_secs: 30,
                error_mode: ErrorMode::Fail,
                output_var: None,
            },
            WorkflowStep {
                name: "review".to_string(),
                agent: StepAgent::ByName {
                    name: "reviewer".to_string(),
                },
                prompt_template: "Review: {{input}}".to_string(),
                mode: StepMode::Review {
                    reject_if_contains: "reject".to_string(),
                    return_to_step: "planning".to_string(),
                    max_rejects: 2,
                },
                timeout_secs: 30,
                error_mode: ErrorMode::Fail,
                output_var: None,
            },
            WorkflowStep {
                name: "dispatch".to_string(),
                agent: StepAgent::ByName {
                    name: "dispatcher".to_string(),
                },
                prompt_template: "Dispatch: {{input}}".to_string(),
                mode: StepMode::Sequential,
                timeout_secs: 30,
                error_mode: ErrorMode::Fail,
                output_var: None,
            },
        ],
        created_at: chrono::Utc::now(),
    };

    let workflow_id = engine.register(workflow).await;
    let run_id = engine
        .create_run(workflow_id, "initial request".to_string())
        .await
        .unwrap();

    let prompts = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let plan_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let review_count = Arc::new(std::sync::atomic::AtomicU32::new(0));

    let prompts_ref = prompts.clone();
    let plan_ref = plan_count.clone();
    let review_ref = review_count.clone();

    let resolver = |_agent: &StepAgent| Some((AgentId::new(), "mock-agent".to_string()));
    let sender = move |_agent_id: AgentId, message: String| {
        let prompts_ref = prompts_ref.clone();
        let plan_ref = plan_ref.clone();
        let review_ref = review_ref.clone();
        async move {
            prompts_ref.lock().unwrap().push(message.clone());
            if message.starts_with("Plan:") {
                let n = plan_ref.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n == 0 {
                    Ok(("plan-v1".to_string(), 10u64, 5u64))
                } else {
                    Ok(("plan-v2".to_string(), 10u64, 5u64))
                }
            } else if message.starts_with("Review:") {
                let n = review_ref.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n == 0 {
                    Ok(("REJECT: incomplete".to_string(), 10u64, 5u64))
                } else {
                    Ok(("APPROVED: good".to_string(), 10u64, 5u64))
                }
            } else {
                Ok((format!("dispatch-final: {message}"), 10u64, 5u64))
            }
        }
    };

    let result = engine.execute_run(run_id, resolver, sender).await;
    assert!(
        result.is_ok(),
        "workflow should complete: {:?}",
        result.err()
    );
    let output = result.unwrap();
    assert!(output.contains("dispatch-final: Dispatch: APPROVED"));

    let run = engine.get_run(run_id).await.unwrap();
    assert!(matches!(run.state, WorkflowRunState::Completed));
    assert_eq!(run.step_results.len(), 5); // planning x2 + review x2 + dispatch x1
    assert_eq!(plan_count.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert_eq!(review_count.load(std::sync::atomic::Ordering::SeqCst), 2);

    let prompts = prompts.lock().unwrap();
    let planning_prompts: Vec<&String> =
        prompts.iter().filter(|p| p.starts_with("Plan:")).collect();
    assert_eq!(planning_prompts.len(), 2);
    assert!(planning_prompts[1].contains("REJECT: incomplete"));
}

/// End-to-end workflow engine test (no LLM): fan-out branches aggregate into a
/// single collected payload consumed by downstream step.
#[tokio::test]
async fn test_workflow_e2e_parallel_fanout_aggregation_without_llm() {
    let engine = WorkflowEngine::new();
    let workflow = Workflow {
        id: WorkflowId::new(),
        name: "e2e-fanout-collect".to_string(),
        description: "integration e2e for fan-out/fan-in".to_string(),
        steps: vec![
            WorkflowStep {
                name: "prepare".to_string(),
                agent: StepAgent::ByName {
                    name: "planner".to_string(),
                },
                prompt_template: "Prepare: {{input}}".to_string(),
                mode: StepMode::Sequential,
                timeout_secs: 30,
                error_mode: ErrorMode::Fail,
                output_var: None,
            },
            WorkflowStep {
                name: "branch-a".to_string(),
                agent: StepAgent::ByName {
                    name: "worker-a".to_string(),
                },
                prompt_template: "Branch A: {{input}}".to_string(),
                mode: StepMode::FanOut,
                timeout_secs: 30,
                error_mode: ErrorMode::Fail,
                output_var: None,
            },
            WorkflowStep {
                name: "branch-b".to_string(),
                agent: StepAgent::ByName {
                    name: "worker-b".to_string(),
                },
                prompt_template: "Branch B: {{input}}".to_string(),
                mode: StepMode::FanOut,
                timeout_secs: 30,
                error_mode: ErrorMode::Fail,
                output_var: None,
            },
            WorkflowStep {
                name: "collect".to_string(),
                agent: StepAgent::ByName {
                    name: "collector".to_string(),
                },
                prompt_template: "unused".to_string(),
                mode: StepMode::Collect,
                timeout_secs: 30,
                error_mode: ErrorMode::Fail,
                output_var: None,
            },
            WorkflowStep {
                name: "finalize".to_string(),
                agent: StepAgent::ByName {
                    name: "finalizer".to_string(),
                },
                prompt_template: "Finalize: {{input}}".to_string(),
                mode: StepMode::Sequential,
                timeout_secs: 30,
                error_mode: ErrorMode::Fail,
                output_var: None,
            },
        ],
        created_at: chrono::Utc::now(),
    };

    let workflow_id = engine.register(workflow).await;
    let run_id = engine
        .create_run(workflow_id, "raw-task".to_string())
        .await
        .unwrap();

    let resolver = |_agent: &StepAgent| Some((AgentId::new(), "mock-agent".to_string()));
    let sender = |_agent_id: AgentId, message: String| async move {
        let output = if message.starts_with("Prepare:") {
            "prepared".to_string()
        } else if message.starts_with("Branch A:") {
            "branch-a-result".to_string()
        } else if message.starts_with("Branch B:") {
            "branch-b-result".to_string()
        } else if message.starts_with("Finalize:") {
            format!("finalized: {message}")
        } else {
            format!("unexpected: {message}")
        };
        Ok((output, 10u64, 5u64))
    };

    let result = engine.execute_run(run_id, resolver, sender).await;
    assert!(
        result.is_ok(),
        "workflow should complete: {:?}",
        result.err()
    );
    let output = result.unwrap();

    assert!(output.contains("branch-a-result"));
    assert!(output.contains("branch-b-result"));
    assert!(!output.contains("prepared"));

    let run = engine.get_run(run_id).await.unwrap();
    assert!(matches!(run.state, WorkflowRunState::Completed));
    assert_eq!(run.step_results.len(), 4); // prepare + 2 fanout branches + finalize
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
