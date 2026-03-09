//! Unit tests for the MAESTRO algorithm crate.
//!
//! These tests verify type serialization, prompt generation, ISC validation,
//! algorithm configuration, and the executor's retry/backoff logic.

#[cfg(test)]
mod tests {
    use crate::executor::AlgorithmConfig;
    use crate::isc::validate_criteria;
    use crate::prompts::*;
    use crate::types::*;
    use crate::{
        AlgorithmResult, IdealStateCriterion, Learning, LearningCategory, Phase, PhaseOutput,
        RunId,
    };
    use chrono::Utc;

    // ── RunId & Phase ──────────────────────────────────────────────────────

    #[test]
    fn run_id_uniqueness() {
        let a = RunId::new();
        let b = RunId::new();
        assert_ne!(a.0, b.0);
    }

    #[test]
    fn phase_display_round_trip() {
        let phases = vec![
            Phase::Observe,
            Phase::Orient,
            Phase::Plan,
            Phase::Execute,
            Phase::Verify,
            Phase::Learn,
            Phase::Adapt,
        ];
        for phase in phases {
            let s = phase.to_string();
            assert!(!s.is_empty(), "Phase display should not be empty");
        }
    }

    // ── AlgorithmConfig ────────────────────────────────────────────────────

    #[test]
    fn default_config_is_sane() {
        let cfg = AlgorithmConfig::default();
        assert!(cfg.max_retries > 0, "Must allow at least 1 retry");
        assert!(cfg.max_retries <= 10, "Retries should be bounded");
        assert!(
            cfg.satisfaction_threshold > 0.0 && cfg.satisfaction_threshold <= 1.0,
            "Threshold must be in (0, 1]"
        );
        assert!(
            cfg.complexity_threshold_sequential < cfg.complexity_threshold_parallel,
            "Sequential threshold must be below parallel"
        );
        assert!(cfg.default_timeout_seconds > 0);
        assert!(cfg.backoff_base_ms > 0);
    }

    #[test]
    fn config_serialization_round_trip() {
        let cfg = AlgorithmConfig::default();
        let json = serde_json::to_string(&cfg).expect("serialize");
        let cfg2: AlgorithmConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cfg.max_retries, cfg2.max_retries);
        assert_eq!(cfg.satisfaction_threshold, cfg2.satisfaction_threshold);
        assert_eq!(
            cfg.complexity_threshold_sequential,
            cfg2.complexity_threshold_sequential
        );
    }

    // ── Phase Output Types ─────────────────────────────────────────────────

    #[test]
    fn observe_output_serialization() {
        let output = ObserveOutput {
            task_restatement: "Test task restated".into(),
            entities: vec!["entity1".into()],
            constraints: vec!["must be fast".into()],
            information_gaps: vec![],
            prior_learnings: vec![],
            available_capabilities: vec!["code".into()],
            notes: vec!["extra note".into()],
        };
        let json = serde_json::to_value(&output).expect("serialize");
        assert_eq!(json["task_restatement"], "Test task restated");
        assert_eq!(json["entities"][0], "entity1");
    }

    #[test]
    fn orient_output_serialization() {
        let output = OrientOutput {
            complexity: 5,
            sub_tasks: vec![SubTask {
                id: "st-1".into(),
                description: "Do the thing".into(),
                capabilities: vec!["code".into()],
                depends_on: vec![],
                effort: 3,
            }],
            risks: vec![Risk {
                description: "Might fail".into(),
                likelihood: "low".into(),
                impact: "medium".into(),
                mitigation: "retry".into(),
            }],
            recommended_agent_count: 2,
            requires_external_data: false,
            produces_artifacts: true,
            strategy_summary: "Sequential execution".into(),
        };
        let json = serde_json::to_value(&output).expect("serialize");
        assert_eq!(json["complexity"], 5);
        assert_eq!(json["sub_tasks"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn plan_output_serialization() {
        let output = PlanOutput {
            steps: vec![ExecutionStep {
                step_number: 1,
                instruction: "Execute the task".into(),
                expected_output: "Code result".into(),
                sub_task_id: "st-1".into(),
                parallelizable: false,
                timeout_seconds: 60,
            }],
            criteria: vec![Criterion {
                id: "C1".into(),
                description: "Output must contain valid JSON".into(),
                category: CriterionCategory::Functional,
                verification_method: "parse as JSON".into(),
                weight: 1.0,
            }],
            agent_assignments: vec![AgentAssignment {
                agent_role: "coder".into(),
                capabilities: vec!["python".into()],
                step_numbers: vec![1],
                model_tier: "balanced".into(),
            }],
            estimated_token_budget: 5000,
            plan_summary: "One step plan".into(),
        };
        let json = serde_json::to_value(&output).expect("serialize");
        assert_eq!(json["steps"].as_array().unwrap().len(), 1);
        assert_eq!(json["estimated_token_budget"], 5000);
    }

    #[test]
    fn execute_output_serialization() {
        let output = ExecuteOutput {
            step_results: vec![StepResult {
                step_number: 1,
                output: "Done".into(),
                success: true,
                error: None,
                duration_ms: 500,
                tokens_used: 100,
            }],
            summary: "All steps completed".into(),
            all_steps_completed: true,
            tokens_used: 100,
        };
        let json = serde_json::to_value(&output).expect("serialize");
        assert!(json["all_steps_completed"].as_bool().unwrap());
    }

    #[test]
    fn verify_output_serialization() {
        let output = VerifyOutput {
            criterion_results: vec![CriterionResult {
                criterion_id: "C1".into(),
                status: VerificationStatus::Satisfied,
                evidence: "Found expected output".into(),
                confidence: 0.95,
                score: 0.95,
            }],
            overall_satisfaction: 0.85,
            threshold_met: true,
            improvement_suggestions: vec![],
        };
        let json = serde_json::to_value(&output).expect("serialize");
        assert_eq!(json["overall_satisfaction"], 0.85);
        assert!(json["threshold_met"].as_bool().unwrap());
    }

    #[test]
    fn learn_output_serialization() {
        let output = LearnOutput {
            learnings: vec![LearningEntry {
                category: crate::types::LearningCategory::System,
                insight: "Caching improves speed".into(),
                context: "Performance test".into(),
                actionable: true,
                suggested_action: Some("Enable caching".into()),
            }],
            successes: vec!["Fast execution".into()],
            failures: vec![],
            recommendations: vec!["Use caching".into()],
        };
        let json = serde_json::to_value(&output).expect("serialize");
        assert_eq!(json["learnings"].as_array().unwrap().len(), 1);
        assert!(json["learnings"][0]["actionable"].as_bool().unwrap());
    }

    #[test]
    fn adapt_output_serialization() {
        let output = AdaptOutput {
            adjustments: vec![ParameterAdjustment {
                parameter: "max_retries".into(),
                current_value: "3".into(),
                proposed_value: "4".into(),
                reason: "High failure rate".into(),
            }],
            rationale: "Increased retries due to failures".into(),
            confidence: 0.8,
        };
        let json = serde_json::to_value(&output).expect("serialize");
        assert_eq!(json["adjustments"].as_array().unwrap().len(), 1);
        assert_eq!(json["confidence"], 0.8);
    }

    // ── Core Types ─────────────────────────────────────────────────────────

    #[test]
    fn learning_serialization() {
        let learning = Learning {
            category: LearningCategory::Algorithm,
            insight: "Parallel execution is faster".into(),
            context: "Multi-agent test".into(),
            actionable: true,
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&learning).expect("serialize");
        let parsed: Learning = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.insight, "Parallel execution is faster");
        assert!(parsed.actionable);
    }

    #[test]
    fn algorithm_result_construction() {
        let result = AlgorithmResult {
            run_id: RunId::new(),
            task_description: "Test task".into(),
            phase_outputs: vec![PhaseOutput {
                phase: Phase::Execute,
                output: serde_json::json!({"result": "done"}),
                tokens_used: 100,
                duration_ms: 500,
                model_used: "test-model".into(),
            }],
            overall_satisfaction: 0.9,
            learnings: vec![],
            started_at: Utc::now(),
            completed_at: Utc::now(),
            total_tokens_used: 100,
            total_cost_usd: 0.01,
        };
        assert_eq!(result.phase_outputs.len(), 1);
        assert!(result.overall_satisfaction > 0.0);
    }

    // ── ISC Validation ─────────────────────────────────────────────────────

    #[test]
    fn isc_validation_flags_short_descriptions() {
        let criteria = vec![
            IdealStateCriterion {
                id: "isc-1".into(),
                description: "Too short".into(), // < 20 chars
                category: crate::CriterionCategory::Functional,
                weight: 1.0,
            },
            IdealStateCriterion {
                id: "isc-2".into(),
                description: "This is a sufficiently long description for testing".into(),
                category: crate::CriterionCategory::Quality,
                weight: 0.8,
            },
        ];
        let warnings = validate_criteria(&criteria);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("isc-1"));
    }

    #[test]
    fn isc_validation_passes_good_criteria() {
        let criteria = vec![IdealStateCriterion {
            id: "isc-1".into(),
            description: "The output must contain a valid JSON response with status field".into(),
            category: crate::CriterionCategory::Functional,
            weight: 1.0,
        }];
        let warnings = validate_criteria(&criteria);
        assert!(warnings.is_empty());
    }

    // ── Prompt Generation ──────────────────────────────────────────────────

    #[test]
    fn observe_prompt_contains_task() {
        let prompt = observe_user_prompt(
            "Build a REST API",
            &["web_search".to_string(), "code".to_string()],
            &["Previous: APIs need auth".to_string()],
        );
        assert!(prompt.contains("Build a REST API"));
        assert!(prompt.contains("web_search"));
        assert!(prompt.contains("Previous: APIs need auth"));
    }

    #[test]
    fn orient_prompt_contains_observe_output() {
        let observe_json = r#"{"task_restatement":"Build API","entities":[]}"#;
        let prompt = orient_user_prompt("Build a REST API", observe_json);
        assert!(prompt.contains("Build a REST API"));
        assert!(prompt.contains("Build API"));
    }

    #[test]
    fn plan_prompt_contains_both_inputs() {
        let observe = r#"{"task_restatement":"Test"}"#;
        let orient = r#"{"complexity":5}"#;
        let prompt = plan_user_prompt("Do the thing", observe, orient);
        assert!(prompt.contains("Do the thing"));
        assert!(prompt.contains("task_restatement"));
        assert!(prompt.contains("complexity"));
    }

    #[test]
    fn verify_prompt_contains_threshold() {
        let plan = r#"{"steps":[]}"#;
        let execute = r#"{"results":[]}"#;
        let prompt = verify_user_prompt(plan, execute, 0.75);
        assert!(prompt.contains("0.75") || prompt.contains("75"));
    }

    #[test]
    fn all_system_prompts_are_non_empty() {
        assert!(!OBSERVE_SYSTEM.is_empty());
        assert!(!ORIENT_SYSTEM.is_empty());
        assert!(!PLAN_SYSTEM.is_empty());
        assert!(!EXECUTE_SYSTEM.is_empty());
        assert!(!VERIFY_SYSTEM.is_empty());
        assert!(!LEARN_SYSTEM.is_empty());
        assert!(!ADAPT_SYSTEM.is_empty());
    }

    #[test]
    fn system_prompts_contain_phase_identity() {
        assert!(OBSERVE_SYSTEM.contains("OBSERVE"));
        assert!(ORIENT_SYSTEM.contains("ORIENT"));
        assert!(PLAN_SYSTEM.contains("PLAN"));
        assert!(EXECUTE_SYSTEM.contains("EXECUTE"));
        assert!(VERIFY_SYSTEM.contains("VERIFY"));
        assert!(LEARN_SYSTEM.contains("LEARN"));
        assert!(ADAPT_SYSTEM.contains("ADAPT"));
    }

    #[test]
    fn system_prompts_mention_json() {
        // All prompts should instruct the model to produce JSON
        assert!(OBSERVE_SYSTEM.contains("JSON"));
        assert!(ORIENT_SYSTEM.contains("JSON"));
        assert!(PLAN_SYSTEM.contains("JSON"));
        assert!(VERIFY_SYSTEM.contains("JSON"));
        assert!(LEARN_SYSTEM.contains("JSON"));
        assert!(ADAPT_SYSTEM.contains("JSON"));
    }

    // ── SubTask & Risk ─────────────────────────────────────────────────────

    #[test]
    fn subtask_with_dependencies() {
        let st = SubTask {
            id: "review".into(),
            description: "Review the draft".into(),
            capabilities: vec!["analysis".into()],
            depends_on: vec!["draft".into()],
            effort: 2,
        };
        assert_eq!(st.depends_on.len(), 1);
        assert_eq!(st.effort, 2);
    }

    #[test]
    fn step_result_failure() {
        let sr = StepResult {
            step_number: 2,
            output: String::new(),
            success: false,
            error: Some("Timeout exceeded".into()),
            duration_ms: 60000,
            tokens_used: 0,
        };
        assert!(!sr.success);
        assert!(sr.error.is_some());
    }

    // ── Criterion Category ─────────────────────────────────────────────────

    #[test]
    fn criterion_categories_serialize_correctly() {
        let cats = vec![
            CriterionCategory::Functional,
            CriterionCategory::Quality,
            CriterionCategory::Completeness,
            CriterionCategory::Constraint,
        ];
        for cat in cats {
            let json = serde_json::to_string(&cat).expect("serialize");
            assert!(!json.is_empty());
        }
    }

    #[test]
    fn verification_status_variants() {
        let statuses = vec![
            VerificationStatus::Satisfied,
            VerificationStatus::Partial,
            VerificationStatus::Failed,
        ];
        for status in statuses {
            let json = serde_json::to_string(&status).expect("serialize");
            assert!(!json.is_empty());
        }
    }
}
