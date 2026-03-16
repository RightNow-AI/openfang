//! ResearchPlanner — drives the 3-phase autoresearch loop.
//!
//! # Phases
//!
//! 1. **Planner** — receives the hypothesis and produces a bounded execution
//!    specification: what to do, which adapter to use, how to verify success.
//!    May inject reusable `ValidatedPattern`s from the pattern store.
//!
//! 2. **Executor** — follows the spec exactly.  No ad-hoc mutations outside
//!    the plan.  Its raw output feeds into the Reviewer.
//!
//! 3. **Reviewer** — scores the executor output on four dimensions
//!    (relevance, correctness, efficiency, safety), computes a weighted
//!    composite, and gates promotion via `PromotionStatus::from_score`.
//!    If the experiment is `Promoted`, a `ValidatedPattern` is persisted for
//!    future Planner runs.
//!
//! Each phase appends a `SelectionTrace` to the experiment record so the
//! full reasoning chain is auditable.

use std::str::FromStr;
use std::sync::Arc;

use chrono::Utc;
use openfang_types::agent::AgentId;
use openfang_types::research::{
    ControlPlaneConfig, ExperimentScore, ExperimentStatus, PromotionStatus, ResearchExperiment,
    ResearchRole, ScoreWeights, SelectionTrace, ValidatedPattern,
};

use crate::kernel::OpenFangKernel;

// ────────────────────────────────────────────────────────────────────────────
// ResearchPlanner
// ────────────────────────────────────────────────────────────────────────────

/// Drives a `ResearchExperiment` through the Plan → Execute → Review cycle.
pub struct ResearchPlanner {
    kernel: Arc<OpenFangKernel>,
}

impl ResearchPlanner {
    pub fn new(kernel: Arc<OpenFangKernel>) -> Self {
        Self { kernel }
    }

    /// Run a full autoresearch experiment from hypothesis to promotion decision.
    ///
    /// All phases are recorded in the `ResearchExperiment`; failures are noted
    /// in the trace rather than propagated, so the method always returns a
    /// complete (if partial) experiment record.
    pub async fn run_experiment(
        &self,
        hypothesis: &str,
        planner_id: &str,
        executor_id: &str,
        reviewer_id: &str,
    ) -> ResearchExperiment {
        let exp_id = uuid::Uuid::new_v4().to_string();

        // Load control plane for scoring config + policy.
        let config = self
            .kernel
            .memory
            .research()
            .load_control_plane()
            .unwrap_or_default();

        // Fetch top validated patterns to inject into the planner prompt.
        let patterns = self
            .kernel
            .memory
            .research()
            .list_patterns()
            .unwrap_or_default();

        // Persist the initial experiment record.
        let seed = ResearchExperiment::new(exp_id.clone(), hypothesis.to_string(), planner_id.to_string());
        let mut experiment = self
            .kernel
            .memory
            .research()
            .create_experiment(&seed)
            .unwrap_or(seed);

        // ── Phase 1: Planner ─────────────────────────────────────────────────
        let planner_prompt = build_planner_prompt(hypothesis, &patterns, &config);
        let spec = self.call_agent(planner_id, &planner_prompt, "planner").await;

        let planner_trace = SelectionTrace::new(
            ResearchRole::Planner,
            planner_id.to_string(),
            format!("produced execution spec ({} chars)", spec.len()),
        );
        let _ = self.kernel.memory.research().append_trace(&exp_id, &planner_trace);
        experiment.selection_trace.push(planner_trace);

        // Transition to Running; record which agents fill executor + reviewer.
        let _ = self.kernel.memory.research()
            .update_experiment_status(&exp_id, ExperimentStatus::Running);
        experiment.status = ExperimentStatus::Running;
        experiment.executor_id = Some(executor_id.to_string());
        experiment.reviewer_id = Some(reviewer_id.to_string());

        // ── Phase 2: Executor ────────────────────────────────────────────────
        let executor_prompt = build_executor_prompt(hypothesis, &spec);
        let raw_output = self.call_agent(executor_id, &executor_prompt, "executor").await;

        let executor_trace = SelectionTrace::new(
            ResearchRole::Executor,
            executor_id.to_string(),
            format!("produced raw output ({} chars)", raw_output.len()),
        );
        let _ = self.kernel.memory.research().append_trace(&exp_id, &executor_trace);
        experiment.selection_trace.push(executor_trace);

        // Transition to AwaitingReview.
        let _ = self.kernel.memory.research()
            .update_experiment_status(&exp_id, ExperimentStatus::AwaitingReview);
        experiment.status = ExperimentStatus::AwaitingReview;

        // ── Phase 3: Reviewer ────────────────────────────────────────────────
        let reviewer_prompt = build_reviewer_prompt(hypothesis, &spec, &raw_output);
        let review_output = self.call_agent(reviewer_id, &reviewer_prompt, "reviewer").await;

        let reviewer_trace = SelectionTrace::new(
            ResearchRole::Reviewer,
            reviewer_id.to_string(),
            format!("scored result ({} chars output)", review_output.len()),
        );
        let _ = self.kernel.memory.research().append_trace(&exp_id, &reviewer_trace);
        experiment.selection_trace.push(reviewer_trace);

        // Parse numeric scores from the reviewer's formatted response.
        let score = extract_score(&review_output, &config.scoring_rules.weights);
        let promotion = PromotionStatus::from_score(
            score.composite,
            config.scoring_rules.min_score_to_promote,
        );

        // Persist score, promotion, and transition to Reviewed.
        let _ = self.kernel.memory.research().record_score(
            &exp_id,
            &score,
            &promotion,
            Some(review_output.as_str()),
        );
        experiment.score = Some(score);
        experiment.promotion_status = Some(promotion.clone());
        experiment.status = ExperimentStatus::Reviewed;
        experiment.result_summary = Some(review_output);
        experiment.finished_at = Some(Utc::now());

        // ── Promotion gate ───────────────────────────────────────────────────
        // Persist a ValidatedPattern only for Promoted experiments.
        if promotion == PromotionStatus::Promoted {
            let pat_id = uuid::Uuid::new_v4().to_string();
            let description = format!("[{exp_id}] {hypothesis}");
            let pattern_type = infer_pattern_type(hypothesis);
            let mut pat = ValidatedPattern::new(pat_id, description, pattern_type);
            if let Some(wid) = &experiment.work_item_id {
                pat.example_work_item_ids.push(wid.clone());
            }
            let _ = self.kernel.memory.research().create_pattern(&pat);
        }

        experiment
    }

    /// Call an agent and return its text response.
    ///
    /// On error the error message is returned as the output string so the
    /// experiment trace still records what happened.
    async fn call_agent(&self, agent_id_str: &str, prompt: &str, role: &str) -> String {
        let id = match AgentId::from_str(agent_id_str) {
            Ok(id) => id,
            Err(_) => return format!("[{role}] invalid agent_id: {agent_id_str}"),
        };
        match self.kernel.send_message_with_handle(id, prompt, None).await {
            Ok(result) => result.response,
            Err(e) => format!("[{role}] agent call failed: {e}"),
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Prompt builders
// ────────────────────────────────────────────────────────────────────────────

fn build_planner_prompt(
    hypothesis: &str,
    patterns: &[ValidatedPattern],
    config: &ControlPlaneConfig,
) -> String {
    let mut s = format!(
        "You are a research planner.\n\
         Hypothesis: {hypothesis}\n\n\
         Allowed mutation surfaces: {surfaces}\n\
         Forbidden actions: {forbidden}\n\n",
        hypothesis = hypothesis,
        surfaces = if config.mutation_surfaces.is_empty() {
            "unrestricted".to_string()
        } else {
            config.mutation_surfaces.join(", ")
        },
        forbidden = if config.forbidden_actions.is_empty() {
            "none".to_string()
        } else {
            config.forbidden_actions.join(", ")
        },
    );

    let usable: Vec<&ValidatedPattern> = patterns.iter().take(5).collect();
    if !usable.is_empty() {
        s.push_str("Validated patterns available for reuse:\n");
        for p in &usable {
            s.push_str(&format!(
                "  - [{}] {} (type: {}, success_rate: {:.0}%)\n",
                p.id,
                p.description,
                p.pattern_type,
                p.success_rate * 100.0,
            ));
        }
        s.push('\n');
    }

    s.push_str(
        "Produce a concise execution specification for the executor.\n\
         State: (1) what action to perform, (2) which adapter to use \
         (api/cli/browser), (3) how success will be verified.\n",
    );
    s
}

fn build_executor_prompt(hypothesis: &str, spec: &str) -> String {
    format!(
        "You are a research executor.\n\
         Original hypothesis: {hypothesis}\n\n\
         Execution specification from the planner:\n{spec}\n\n\
         Follow the specification exactly. Do not deviate. \
         Report your complete raw output below.\n",
        hypothesis = hypothesis,
        spec = spec,
    )
}

fn build_reviewer_prompt(hypothesis: &str, spec: &str, result: &str) -> String {
    format!(
        "You are a research reviewer.\n\
         Hypothesis: {hypothesis}\n\n\
         Planner specification:\n{spec}\n\n\
         Executor result:\n{result}\n\n\
         Score the result on four dimensions (0.0–1.0 each):\n\
           - relevance:   how well the result addresses the hypothesis\n\
           - correctness: factual/operational accuracy\n\
           - efficiency:  token, time, and cost efficiency\n\
           - safety:      absence of unsafe mutations or policy violations\n\n\
         Respond in EXACTLY this format (no extra lines before the scores):\n\
         RELEVANCE: <score>\n\
         CORRECTNESS: <score>\n\
         EFFICIENCY: <score>\n\
         SAFETY: <score>\n\
         NOTES: <free-form evaluation>\n",
        hypothesis = hypothesis,
        spec = spec,
        result = result,
    )
}

// ────────────────────────────────────────────────────────────────────────────
// Score extraction
// ────────────────────────────────────────────────────────────────────────────

/// Parse the reviewer's structured response into an `ExperimentScore`.
///
/// Falls back to neutral `0.5` for any dimension that cannot be parsed so
/// the experiment always produces a score even when the LLM returns
/// unexpected formatting.
fn extract_score(output: &str, weights: &ScoreWeights) -> ExperimentScore {
    let parse_field = |label: &str| -> f32 {
        output
            .lines()
            .find(|l| l.to_uppercase().starts_with(label))
            .and_then(|l| l.split_once(':').map(|x| x.1))
            .and_then(|v| v.trim().parse::<f32>().ok())
            .unwrap_or(0.5)
            .clamp(0.0, 1.0)
    };

    let notes = output
        .lines()
        .find(|l| l.to_uppercase().starts_with("NOTES"))
        .and_then(|l| l.split_once(':').map(|x| x.1))
        .map(|n| n.trim().to_string());

    ExperimentScore::compute(
        parse_field("RELEVANCE"),
        parse_field("CORRECTNESS"),
        parse_field("EFFICIENCY"),
        parse_field("SAFETY"),
        weights,
        notes,
    )
}

// ────────────────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────────────────

/// Infer a coarse `pattern_type` category from the hypothesis text.
fn infer_pattern_type(hypothesis: &str) -> String {
    let h = hypothesis.to_lowercase();
    if h.contains("adapter") || h.contains("api") || h.contains("cli") {
        "adapter_selection".to_string()
    } else if h.contains("verif") || h.contains("check") || h.contains("test") {
        "verification_strategy".to_string()
    } else if h.contains("retry") || h.contains("fail") || h.contains("error") {
        "retry_policy".to_string()
    } else if h.contains("delegat") || h.contains("swarm") || h.contains("subagent") {
        "delegation_strategy".to_string()
    } else {
        "general".to_string()
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_types::research::ScoreWeights;

    #[test]
    fn extract_score_parses_well_formed_response() {
        let output = "RELEVANCE: 0.9\nCORRECTNESS: 0.8\nEFFICIENCY: 0.7\nSAFETY: 1.0\nNOTES: looks good";
        let weights = ScoreWeights::default();
        let score = extract_score(output, &weights);
        assert!((score.relevance - 0.9).abs() < 0.001);
        assert!((score.correctness - 0.8).abs() < 0.001);
        assert!((score.efficiency - 0.7).abs() < 0.001);
        assert!((score.safety - 1.0).abs() < 0.001);
        assert!(score.composite > 0.0 && score.composite <= 1.0);
        assert_eq!(score.reviewer_notes.as_deref(), Some("looks good"));
    }

    #[test]
    fn extract_score_falls_back_to_neutral_on_garbled_response() {
        let output = "nothing parseable here";
        let weights = ScoreWeights::default();
        let score = extract_score(output, &weights);
        // All dimensions default to 0.5 → composite ≈ 0.5
        assert!((score.composite - 0.5).abs() < 0.01);
    }

    #[test]
    fn infer_pattern_type_classifies_correctly() {
        assert_eq!(infer_pattern_type("test the API adapter"), "adapter_selection");
        assert_eq!(infer_pattern_type("verify the output"), "verification_strategy");
        assert_eq!(infer_pattern_type("retry on failure"), "retry_policy");
        assert_eq!(infer_pattern_type("delegate to subagent"), "delegation_strategy");
        assert_eq!(infer_pattern_type("general hypothesis"), "general");
    }

    #[test]
    fn planner_prompt_injects_validated_patterns() {
        let config = ControlPlaneConfig::default();
        let patterns = vec![ValidatedPattern::new(
            "pat-1".into(),
            "use api for low-risk ops".into(),
            "adapter_selection".into(),
        )];
        let prompt = build_planner_prompt("should we use API?", &patterns, &config);
        assert!(prompt.contains("pat-1"));
        assert!(prompt.contains("adapter_selection"));
    }

    #[test]
    fn planner_prompt_no_patterns_omits_pattern_section() {
        let config = ControlPlaneConfig::default();
        let prompt = build_planner_prompt("hypothesis", &[], &config);
        assert!(!prompt.contains("Validated patterns"));
    }
}
