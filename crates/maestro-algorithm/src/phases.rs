//! Individual phase runner functions.
//!
//! Each function encapsulates the logic for one phase of the MAESTRO algorithm.
//! They accept a `ModelProvider`, build the appropriate prompt, call the model,
//! and return a typed `PhaseOutput`.

use std::time::Instant;

use serde_json::json;
use tracing::{info, warn};

use crate::{
    Phase, PhaseOutput, RunId,
    error::AlgorithmError,
    executor::{ExecutionHooks, ModelProvider},
    prompts,
    types::*,
};

/// Run the OBSERVE phase: gather raw observations about the task.
pub async fn run_observe<M: ModelProvider>(
    model: &M,
    _run_id: RunId,
    task: &str,
    capabilities: &[String],
    prior_learnings: &[String],
) -> Result<PhaseOutput, AlgorithmError> {
    let start = Instant::now();
    info!("OBSERVE: Gathering observations for task");

    let user_prompt = prompts::observe_user_prompt(task, capabilities, prior_learnings);

    let output: ObserveOutput = model
        .extract(&user_prompt, prompts::OBSERVE_SYSTEM)
        .await
        .map_err(|e| AlgorithmError::PhaseFailure {
            phase: "OBSERVE".to_string(),
            retries: 0,
            reason: e.to_string(),
        })?;

    info!(
        entities = output.entities.len(),
        constraints = output.constraints.len(),
        gaps = output.information_gaps.len(),
        "OBSERVE complete"
    );

    Ok(PhaseOutput {
        phase: Phase::Observe,
        output: serde_json::to_value(&output).unwrap_or_default(),
        tokens_used: 0, // Tracked externally by model provider
        duration_ms: start.elapsed().as_millis() as u64,
        model_used: model.model_id().to_string(),
    })
}

/// Run the ORIENT phase: analyze, decompose, and assess complexity.
pub async fn run_orient<M: ModelProvider>(
    model: &M,
    _run_id: RunId,
    task: &str,
    observe_output: &serde_json::Value,
) -> Result<PhaseOutput, AlgorithmError> {
    let start = Instant::now();
    info!("ORIENT: Analyzing task complexity and decomposition");

    let observe_json = serde_json::to_string_pretty(observe_output).unwrap_or_default();
    let user_prompt = prompts::orient_user_prompt(task, &observe_json);

    let output: OrientOutput = model
        .extract(&user_prompt, prompts::ORIENT_SYSTEM)
        .await
        .map_err(|e| AlgorithmError::PhaseFailure {
            phase: "ORIENT".to_string(),
            retries: 0,
            reason: e.to_string(),
        })?;

    info!(
        complexity = output.complexity,
        sub_tasks = output.sub_tasks.len(),
        recommended_agents = output.recommended_agent_count,
        "ORIENT complete"
    );

    Ok(PhaseOutput {
        phase: Phase::Orient,
        output: serde_json::to_value(&output).unwrap_or_default(),
        tokens_used: 0,
        duration_ms: start.elapsed().as_millis() as u64,
        model_used: model.model_id().to_string(),
    })
}

/// Run the PLAN phase: create execution plan with ISC criteria.
pub async fn run_plan<M: ModelProvider>(
    model: &M,
    _run_id: RunId,
    task: &str,
    observe_output: &serde_json::Value,
    orient_output: &serde_json::Value,
) -> Result<PhaseOutput, AlgorithmError> {
    let start = Instant::now();
    info!("PLAN: Creating execution plan with ISC criteria");

    let observe_json = serde_json::to_string_pretty(observe_output).unwrap_or_default();
    let orient_json = serde_json::to_string_pretty(orient_output).unwrap_or_default();
    let user_prompt = prompts::plan_user_prompt(task, &observe_json, &orient_json);

    let output: PlanOutput = model
        .extract(&user_prompt, prompts::PLAN_SYSTEM)
        .await
        .map_err(|e| AlgorithmError::PhaseFailure {
            phase: "PLAN".to_string(),
            retries: 0,
            reason: e.to_string(),
        })?;

    // Validate ISC criteria weights sum to ~1.0
    let weight_sum: f64 = output.criteria.iter().map(|c| c.weight).sum();
    if (weight_sum - 1.0).abs() > 0.05 {
        warn!(
            weight_sum,
            "ISC criteria weights do not sum to 1.0 — normalizing"
        );
    }

    info!(
        steps = output.steps.len(),
        criteria = output.criteria.len(),
        agents = output.agent_assignments.len(),
        "PLAN complete"
    );

    Ok(PhaseOutput {
        phase: Phase::Plan,
        output: serde_json::to_value(&output).unwrap_or_default(),
        tokens_used: 0,
        duration_ms: start.elapsed().as_millis() as u64,
        model_used: model.model_id().to_string(),
    })
}

/// Run the EXECUTE phase: delegate steps to agents and collect results.
///
/// This is the only phase that interacts with the external world via
/// `ExecutionHooks::delegate_to_agent()`. The model is used only to
/// synthesize the final ExecuteOutput from raw agent results.
pub async fn run_execute<M: ModelProvider, H: ExecutionHooks>(
    model: &M,
    hooks: &H,
    _run_id: RunId,
    task: &str,
    plan_output: &serde_json::Value,
) -> Result<PhaseOutput, AlgorithmError> {
    let start = Instant::now();
    info!("EXECUTE: Delegating steps to agents");

    // Parse the plan to get execution steps
    let plan: PlanOutput = serde_json::from_value(plan_output.clone()).map_err(|e| {
        AlgorithmError::PhaseFailure {
            phase: "EXECUTE".to_string(),
            retries: 0,
            reason: format!("Failed to parse plan output: {e}"),
        }
    })?;

    // Execute each step by delegating to agents
    let mut raw_results = Vec::new();

    // Group steps by parallelizability
    // For now, execute sequentially (parallel execution is a future enhancement)
    for step in &plan.steps {
        let step_start = Instant::now();

        // Find the agent assignment for this step
        let assignment = plan
            .agent_assignments
            .iter()
            .find(|a| a.step_numbers.contains(&step.step_number));

        let capabilities: Vec<String> = assignment
            .map(|a| a.capabilities.clone())
            .unwrap_or_default();

        info!(
            step = step.step_number,
            instruction = %step.instruction.chars().take(80).collect::<String>(),
            "Delegating step to agent"
        );

        let result = hooks
            .delegate_to_agent(&step.instruction, &capabilities)
            .await;

        let step_duration = step_start.elapsed().as_millis() as u64;

        match result {
            Ok(output) => {
                raw_results.push(json!({
                    "step_number": step.step_number,
                    "output": output,
                    "success": true,
                    "duration_ms": step_duration,
                }));
            }
            Err(e) => {
                warn!(step = step.step_number, error = %e, "Step delegation failed");
                raw_results.push(json!({
                    "step_number": step.step_number,
                    "error": e.to_string(),
                    "success": false,
                    "duration_ms": step_duration,
                }));
            }
        }
    }

    // Use the model to synthesize raw results into a structured ExecuteOutput
    let plan_json = serde_json::to_string_pretty(plan_output).unwrap_or_default();
    let results_text = raw_results
        .iter()
        .map(|r| serde_json::to_string_pretty(r).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n\n");

    let user_prompt = prompts::execute_user_prompt(&plan_json, &results_text);

    let output: ExecuteOutput = model
        .extract(&user_prompt, prompts::EXECUTE_SYSTEM)
        .await
        .map_err(|e| AlgorithmError::PhaseFailure {
            phase: "EXECUTE".to_string(),
            retries: 0,
            reason: format!("Failed to synthesize execution results: {e}"),
        })?;

    info!(
        steps_completed = output.step_results.iter().filter(|r| r.success).count(),
        total_steps = output.step_results.len(),
        all_completed = output.all_steps_completed,
        "EXECUTE complete"
    );

    // Include the raw task in the output for downstream phases
    let mut output_value = serde_json::to_value(&output).unwrap_or_default();
    if let Some(obj) = output_value.as_object_mut() {
        obj.insert("_task".to_string(), json!(task));
    }

    Ok(PhaseOutput {
        phase: Phase::Execute,
        output: output_value,
        tokens_used: output.tokens_used,
        duration_ms: start.elapsed().as_millis() as u64,
        model_used: model.model_id().to_string(),
    })
}

/// Run the VERIFY phase: check ISC criteria against execution output.
pub async fn run_verify<M: ModelProvider>(
    model: &M,
    _run_id: RunId,
    plan_output: &serde_json::Value,
    execute_output: &serde_json::Value,
    threshold: f64,
) -> Result<PhaseOutput, AlgorithmError> {
    let start = Instant::now();
    info!("VERIFY: Checking ISC criteria against execution output");

    let plan_json = serde_json::to_string_pretty(plan_output).unwrap_or_default();
    let execute_json = serde_json::to_string_pretty(execute_output).unwrap_or_default();
    let user_prompt = prompts::verify_user_prompt(&plan_json, &execute_json, threshold);

    let output: VerifyOutput = model
        .extract(&user_prompt, prompts::VERIFY_SYSTEM)
        .await
        .map_err(|e| AlgorithmError::PhaseFailure {
            phase: "VERIFY".to_string(),
            retries: 0,
            reason: e.to_string(),
        })?;

    info!(
        satisfaction = format!("{:.1}%", output.overall_satisfaction),
        threshold_met = output.threshold_met,
        criteria_checked = output.criterion_results.len(),
        "VERIFY complete"
    );

    Ok(PhaseOutput {
        phase: Phase::Verify,
        output: serde_json::to_value(&output).unwrap_or_default(),
        tokens_used: 0,
        duration_ms: start.elapsed().as_millis() as u64,
        model_used: model.model_id().to_string(),
    })
}

/// Run the LEARN phase: extract structured learnings from the execution.
pub async fn run_learn<M: ModelProvider>(
    model: &M,
    _run_id: RunId,
    task: &str,
    satisfaction: f64,
    all_phase_outputs: &[PhaseOutput],
) -> Result<PhaseOutput, AlgorithmError> {
    let start = Instant::now();
    info!("LEARN: Extracting learnings from execution");

    // Build a condensed summary of all phases for the learning prompt
    let phases_summary: Vec<serde_json::Value> = all_phase_outputs
        .iter()
        .map(|p| {
            json!({
                "phase": p.phase.to_string(),
                "duration_ms": p.duration_ms,
                "tokens_used": p.tokens_used,
                "output_summary": truncate_json(&p.output, 2000),
            })
        })
        .collect();

    let all_phases_json = serde_json::to_string_pretty(&phases_summary).unwrap_or_default();
    let user_prompt = prompts::learn_user_prompt(task, satisfaction, &all_phases_json);

    let output: LearnOutput = model
        .extract(&user_prompt, prompts::LEARN_SYSTEM)
        .await
        .map_err(|e| AlgorithmError::PhaseFailure {
            phase: "LEARN".to_string(),
            retries: 0,
            reason: e.to_string(),
        })?;

    info!(
        learnings = output.learnings.len(),
        successes = output.successes.len(),
        failures = output.failures.len(),
        "LEARN complete"
    );

    Ok(PhaseOutput {
        phase: Phase::Learn,
        output: serde_json::to_value(&output).unwrap_or_default(),
        tokens_used: 0,
        duration_ms: start.elapsed().as_millis() as u64,
        model_used: model.model_id().to_string(),
    })
}

/// Run the ADAPT phase: propose parameter adjustments.
pub async fn run_adapt<M: ModelProvider>(
    model: &M,
    _run_id: RunId,
    learn_output: &serde_json::Value,
    current_config: &serde_json::Value,
    past_learnings: &[String],
) -> Result<PhaseOutput, AlgorithmError> {
    let start = Instant::now();
    info!("ADAPT: Proposing parameter adjustments");

    let learnings_json = serde_json::to_string_pretty(learn_output).unwrap_or_default();
    let config_json = serde_json::to_string_pretty(current_config).unwrap_or_default();
    let user_prompt = prompts::adapt_user_prompt(&learnings_json, &config_json, past_learnings);

    let output: AdaptOutput = model
        .extract(&user_prompt, prompts::ADAPT_SYSTEM)
        .await
        .map_err(|e| AlgorithmError::PhaseFailure {
            phase: "ADAPT".to_string(),
            retries: 0,
            reason: e.to_string(),
        })?;

    info!(
        adjustments = output.adjustments.len(),
        confidence = format!("{:.2}", output.confidence),
        "ADAPT complete"
    );

    Ok(PhaseOutput {
        phase: Phase::Adapt,
        output: serde_json::to_value(&output).unwrap_or_default(),
        tokens_used: 0,
        duration_ms: start.elapsed().as_millis() as u64,
        model_used: model.model_id().to_string(),
    })
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Truncate a JSON value to approximately `max_chars` for inclusion in prompts.
fn truncate_json(value: &serde_json::Value, max_chars: usize) -> serde_json::Value {
    let s = serde_json::to_string(value).unwrap_or_default();
    if s.len() <= max_chars {
        value.clone()
    } else {
        json!(format!("{}... [truncated]", &s[..max_chars]))
    }
}
