//! End-to-end integration tests for multi-session isolation and workflow resume.
//!
//! These tests avoid real LLM calls while still exercising the public kernel /
//! workflow APIs used by the multi-agent foundation features.

use openfang_kernel::workflow::{
    ErrorMode, StepAgent, StepMode, Workflow, WorkflowEngine, WorkflowId, WorkflowRunState,
    WorkflowStep,
};
use openfang_kernel::OpenFangKernel;
use openfang_memory::session::Session;
use openfang_types::agent::{AgentId, AgentManifest, ManifestCapabilities, ModelConfig, SessionId};
use openfang_types::config::{DefaultModelConfig, KernelConfig};
use openfang_types::message::Message;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

fn test_config(tmp: &tempfile::TempDir) -> KernelConfig {
    KernelConfig {
        home_dir: tmp.path().to_path_buf(),
        data_dir: tmp.path().join("data"),
        default_model: DefaultModelConfig {
            provider: "ollama".to_string(),
            model: "test-model".to_string(),
            api_key_env: "OLLAMA_API_KEY".to_string(),
            base_url: None,
        },
        ..KernelConfig::default()
    }
}

fn session_test_manifest(workspace: PathBuf) -> AgentManifest {
    AgentManifest {
        name: "session-e2e-agent".to_string(),
        description: "Session isolation e2e test agent".to_string(),
        author: "test".to_string(),
        module: "builtin:chat".to_string(),
        model: ModelConfig {
            provider: "ollama".to_string(),
            model: "test-model".to_string(),
            system_prompt: "Test agent".to_string(),
            api_key_env: Some("OLLAMA_API_KEY".to_string()),
            ..ModelConfig::default()
        },
        capabilities: ManifestCapabilities {
            memory_read: vec!["*".to_string()],
            memory_write: vec!["self.*".to_string()],
            ..ManifestCapabilities::default()
        },
        workspace: Some(workspace),
        ..AgentManifest::default()
    }
}

fn write_session_messages(
    kernel: &OpenFangKernel,
    session_id: SessionId,
    agent_id: AgentId,
    user_text: &str,
    assistant_text: &str,
) {
    let session = Session {
        id: session_id,
        agent_id,
        messages: vec![Message::user(user_text), Message::assistant(assistant_text)],
        context_window_tokens: 0,
        label: None,
    };
    kernel.memory.save_session(&session).unwrap();
}

fn only_markdown_file(dir: &Path) -> PathBuf {
    let entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .collect();
    assert_eq!(
        entries.len(),
        1,
        "expected one markdown file in {}",
        dir.display()
    );
    entries[0].clone()
}

#[test]
fn test_multi_session_e2e_session_summaries_stay_scoped() {
    let tmp = tempfile::tempdir().unwrap();
    let workspace = tmp.path().join("workspaces").join("session-e2e-agent");
    let config = test_config(&tmp);
    let kernel = OpenFangKernel::boot_with_config(config).expect("Kernel should boot");

    let agent_id = kernel
        .spawn_agent(session_test_manifest(workspace.clone()))
        .expect("Agent should spawn");

    let session_a = kernel.registry.get(agent_id).unwrap().session_id;
    let session_b = kernel
        .create_agent_session(agent_id, Some("session-b"))
        .unwrap()["session_id"]
        .as_str()
        .map(|raw| SessionId(Uuid::parse_str(raw).unwrap()))
        .unwrap();

    write_session_messages(
        &kernel,
        session_a,
        agent_id,
        "alpha confidential thread",
        "alpha assistant reply",
    );
    write_session_messages(
        &kernel,
        session_b,
        agent_id,
        "bravo isolated topic",
        "bravo assistant reply",
    );

    kernel.switch_agent_session(agent_id, session_a).unwrap();
    kernel.reset_session(agent_id).unwrap();

    kernel.switch_agent_session(agent_id, session_b).unwrap();
    kernel.reset_session(agent_id).unwrap();

    let session_a_memory_dir = workspace
        .join(".session-workspaces")
        .join(session_a.to_string())
        .join("memory");
    let session_b_memory_dir = workspace
        .join(".session-workspaces")
        .join(session_b.to_string())
        .join("memory");

    let session_a_summary =
        std::fs::read_to_string(only_markdown_file(&session_a_memory_dir)).unwrap();
    let session_b_summary =
        std::fs::read_to_string(only_markdown_file(&session_b_memory_dir)).unwrap();

    assert!(session_a_summary.contains("alpha confidential thread"));
    assert!(!session_a_summary.contains("bravo isolated topic"));
    assert!(session_b_summary.contains("bravo isolated topic"));
    assert!(!session_b_summary.contains("alpha confidential thread"));

    let base_memory_entries: Vec<_> = std::fs::read_dir(workspace.join("memory"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert!(
        base_memory_entries.is_empty(),
        "base workspace memory should stay empty when concurrent sessions are isolated"
    );

    kernel.shutdown();
}

#[tokio::test]
async fn test_workflow_e2e_resume_after_interrupted_snapshot_without_llm() {
    let engine = Arc::new(WorkflowEngine::new());
    let workflow = Workflow {
        id: WorkflowId::new(),
        name: "e2e-session-resume".to_string(),
        description: "integration e2e for interrupted snapshot resume".to_string(),
        steps: vec![
            WorkflowStep {
                name: "analyze".to_string(),
                agent: StepAgent::ByName {
                    name: "planner".to_string(),
                },
                prompt_template: "Analyze this: {{input}}".to_string(),
                mode: StepMode::Sequential,
                timeout_secs: 30,
                error_mode: ErrorMode::Fail,
                output_var: Some("analysis".to_string()),
            },
            WorkflowStep {
                name: "summarize".to_string(),
                agent: StepAgent::ByName {
                    name: "writer".to_string(),
                },
                prompt_template: "Summarize this analysis: {{input}}".to_string(),
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
        .create_run(workflow_id, "interrupted raw input".to_string())
        .await
        .unwrap();

    let resolver = |_agent: &StepAgent| Some((AgentId::new(), "mock-agent".to_string()));
    let summarize_started = Arc::new(tokio::sync::Notify::new());
    let allow_summary_finish = Arc::new(tokio::sync::Notify::new());
    let summarize_started_ref = summarize_started.clone();
    let allow_summary_finish_ref = allow_summary_finish.clone();

    let engine_for_run = engine.clone();
    let run_handle = tokio::spawn(async move {
        let sender = move |_agent_id: AgentId, message: String| {
            let summarize_started_ref = summarize_started_ref.clone();
            let allow_summary_finish_ref = allow_summary_finish_ref.clone();
            async move {
                if message.starts_with("Analyze this:") {
                    Ok(("analysis-ready".to_string(), 10u64, 5u64))
                } else {
                    summarize_started_ref.notify_one();
                    allow_summary_finish_ref.notified().await;
                    Ok(("summary-ready".to_string(), 10u64, 5u64))
                }
            }
        };

        engine_for_run.execute_run(run_id, resolver, sender).await
    });

    summarize_started.notified().await;

    let tempdir = tempfile::tempdir().unwrap();
    let snapshot_path = tempdir.path().join("interrupted-workflow.json");
    engine.save_recovery_snapshot(&snapshot_path).await.unwrap();

    run_handle.abort();
    let _ = run_handle.await;

    let recovered_engine = WorkflowEngine::load_recovery_snapshot(&snapshot_path)
        .await
        .unwrap();
    let recovered_run = recovered_engine.get_run(run_id).await.unwrap();
    assert!(matches!(recovered_run.state, WorkflowRunState::Blocked));
    assert_eq!(recovered_run.step_results.len(), 1);
    assert_eq!(recovered_run.step_results[0].output, "analysis-ready");

    let resumed_prompts = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let resumed_prompts_ref = resumed_prompts.clone();
    let resumed_sender = move |_agent_id: AgentId, message: String| {
        let resumed_prompts_ref = resumed_prompts_ref.clone();
        async move {
            resumed_prompts_ref.lock().unwrap().push(message.clone());
            if message.starts_with("Analyze this:") {
                Err("analyze step should not rerun after interrupted recovery".to_string())
            } else {
                Ok(("summary-ready".to_string(), 10u64, 5u64))
            }
        }
    };

    let resumed_output = recovered_engine
        .execute_run(run_id, resolver, resumed_sender)
        .await
        .unwrap();
    assert_eq!(resumed_output, "summary-ready");

    let resumed_run = recovered_engine.get_run(run_id).await.unwrap();
    assert!(matches!(resumed_run.state, WorkflowRunState::Completed));
    assert_eq!(resumed_run.step_results.len(), 2);

    let prompts = resumed_prompts.lock().unwrap();
    assert_eq!(prompts.len(), 1);
    assert!(prompts[0].starts_with("Summarize this analysis:"));
}
