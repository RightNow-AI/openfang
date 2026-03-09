//! Ideal State Criteria (ISC) generation and management.
//!
//! ISC is the core quality-gate mechanism in the MAESTRO algorithm. After the
//! PLAN phase, the algorithm generates a set of verifiable criteria that the
//! final output must satisfy. These criteria are checked in the EVALUATE phase
//! to determine whether the task is complete or needs another iteration.
//!
//! ## Criterion Categories
//!
//! | Category | Template | Verification Method |
//! |---|---|---|
//! | `Functional` | "The output must contain X" | String search / regex |
//! | `Quality` | "The output must score >= N on metric M" | Eval metric |
//! | `Completeness` | "All N items from the input must appear" | Set membership |
//! | `Constraint` | "The output must not exceed N tokens / contain X" | Negation check |
//!
//! ## Design Note
//!
//! The original MAESTRO implementation let the LLM generate criteria freeform,
//! which produced vague, untestable criteria like "ensure quality" or "be
//! comprehensive". This module enforces structured templates so that every
//! criterion is machine-verifiable, not just human-readable.

use crate::{CriterionCategory, IdealStateCriterion};

/// Generate ISC criteria from a plan output.
///
/// Extracts structured criteria from the plan's JSON representation. The
/// function looks for four well-known keys in the plan:
///
/// - `"deliverables"` → one `Functional` criterion per deliverable item
/// - `"quality_bar"` → one `Quality` criterion per quality requirement
/// - `"inputs"` → one `Completeness` criterion covering all input items
/// - `"constraints"` → one `Constraint` criterion per constraint
///
/// If none of these keys are present, a set of sensible default criteria
/// is generated based on the plan's `"task"` description (if present) or
/// a generic fallback.
///
/// ## Weights
///
/// Weights are assigned by category priority:
/// - `Functional`: 0.40 (primary deliverable — highest weight)
/// - `Quality`: 0.25 (quality bar — second priority)
/// - `Completeness`: 0.20 (coverage — third priority)
/// - `Constraint`: 0.15 (guard rails — lowest weight)
///
/// When multiple criteria of the same category exist, the total weight for
/// that category is divided equally among them.
pub fn generate_criteria(plan_output: &serde_json::Value) -> Vec<IdealStateCriterion> {
    let mut criteria: Vec<IdealStateCriterion> = Vec::new();
    let mut counter = 1usize;

    // --- Functional criteria from "deliverables" ---
    if let Some(deliverables) = plan_output.get("deliverables").and_then(|v| v.as_array()) {
        let n = deliverables.len().max(1);
        let weight = 0.40 / n as f64;
        for item in deliverables {
            let label = item
                .as_str()
                .map(|s| s.to_string())
                .or_else(|| item.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()))
                .unwrap_or_else(|| format!("deliverable-{counter}"));
            criteria.push(IdealStateCriterion {
                id: format!("C{counter}"),
                description: format!("The output must contain or produce: {label}"),
                category: CriterionCategory::Functional,
                weight,
            });
            counter += 1;
        }
    }

    // --- Quality criteria from "quality_bar" ---
    if let Some(quality) = plan_output.get("quality_bar").and_then(|v| v.as_array()) {
        let n = quality.len().max(1);
        let weight = 0.25 / n as f64;
        for item in quality {
            let label = item
                .as_str()
                .map(|s| s.to_string())
                .or_else(|| item.get("metric").and_then(|v| v.as_str()).map(|s| s.to_string()))
                .unwrap_or_else(|| format!("quality-metric-{counter}"));
            criteria.push(IdealStateCriterion {
                id: format!("C{counter}"),
                description: format!("The output must meet quality requirement: {label}"),
                category: CriterionCategory::Quality,
                weight,
            });
            counter += 1;
        }
    }

    // --- Completeness criterion from "inputs" ---
    if let Some(inputs) = plan_output.get("inputs").and_then(|v| v.as_array()) {
        let n = inputs.len();
        if n > 0 {
            criteria.push(IdealStateCriterion {
                id: format!("C{counter}"),
                description: format!(
                    "All {n} input items must be addressed in the output (completeness check)"
                ),
                category: CriterionCategory::Completeness,
                weight: 0.20,
            });
            counter += 1;
        }
    }

    // --- Constraint criteria from "constraints" ---
    if let Some(constraints) = plan_output.get("constraints").and_then(|v| v.as_array()) {
        let n = constraints.len().max(1);
        let weight = 0.15 / n as f64;
        for item in constraints {
            let label = item
                .as_str()
                .map(|s| s.to_string())
                .or_else(|| item.get("rule").and_then(|v| v.as_str()).map(|s| s.to_string()))
                .unwrap_or_else(|| format!("constraint-{counter}"));
            criteria.push(IdealStateCriterion {
                id: format!("C{counter}"),
                description: format!("The output must satisfy constraint: {label}"),
                category: CriterionCategory::Constraint,
                weight,
            });
            counter += 1;
        }
    }

    // --- Fallback: generate defaults if no structured keys found ---
    if criteria.is_empty() {
        let task_desc = plan_output
            .get("task")
            .and_then(|v| v.as_str())
            .unwrap_or("the assigned task");

        criteria.push(IdealStateCriterion {
            id: "C1".to_string(),
            description: format!("The output must directly address and complete: {task_desc}"),
            category: CriterionCategory::Functional,
            weight: 0.40,
        });
        criteria.push(IdealStateCriterion {
            id: "C2".to_string(),
            description: "The output must be coherent, accurate, and free of contradictions"
                .to_string(),
            category: CriterionCategory::Quality,
            weight: 0.25,
        });
        criteria.push(IdealStateCriterion {
            id: "C3".to_string(),
            description: "All explicit requirements stated in the task must be covered".to_string(),
            category: CriterionCategory::Completeness,
            weight: 0.20,
        });
        criteria.push(IdealStateCriterion {
            id: "C4".to_string(),
            description: "The output must not introduce information not present in the task scope"
                .to_string(),
            category: CriterionCategory::Constraint,
            weight: 0.15,
        });
    }

    criteria
}

/// Validate that criteria are testable (not vague).
///
/// Returns a list of warning strings for criteria that fail validation.
/// An empty return value means all criteria passed.
pub fn validate_criteria(criteria: &[IdealStateCriterion]) -> Vec<String> {
    let mut warnings = Vec::new();

    // Vague words that indicate an untestable criterion.
    let vague_words = [
        "ensure", "improve", "good", "better", "appropriate", "sufficient",
        "adequate", "reasonable", "proper", "effective",
    ];

    // Measurable signal words that indicate a testable criterion.
    let measurable_words = [
        "must contain", "must not", "must include", "must produce", "must meet",
        "must satisfy", "must address", "must cover", "must be",
        "all ", "every ", ">= ", "<= ", "exactly ",
    ];

    for c in criteria {
        // Check minimum description length.
        if c.description.len() < 20 {
            warnings.push(format!(
                "{}: Description too short to be testable (< 20 chars)",
                c.id
            ));
        }

        // Check for vague language using whole-word matching to avoid false
        // positives like "sufficiently" matching "sufficient".
        let desc_lower = c.description.to_lowercase();
        let found_vague = vague_words.iter().find(|&&word| {
            // Check that the match is at a word boundary (preceded/followed
            // by a non-alphabetic character or start/end of string).
            let mut found = false;
            let mut start = 0;
            while let Some(pos) = desc_lower[start..].find(word) {
                let abs_pos = start + pos;
                let before_ok = abs_pos == 0
                    || !desc_lower.as_bytes()[abs_pos - 1].is_ascii_alphabetic();
                let after_pos = abs_pos + word.len();
                let after_ok = after_pos >= desc_lower.len()
                    || !desc_lower.as_bytes()[after_pos].is_ascii_alphabetic();
                if before_ok && after_ok {
                    found = true;
                    break;
                }
                start = abs_pos + 1;
                if start >= desc_lower.len() { break; }
            }
            found
        });
        if let Some(word) = found_vague {
            warnings.push(format!(
                "{}: Contains vague language '{}' — replace with measurable language",
                c.id, word
            ));
        }

        // Check for measurable language — only on Functional and Constraint
        // criteria where "must contain / must not" language is required.
        // Quality and Completeness criteria often use scoring or counting
        // language that is harder to pattern-match.
        // Skip this check if the description is already flagged as too short
        // (< 20 chars) to avoid double-warning on the same criterion.
        let requires_measurable = matches!(
            c.category,
            CriterionCategory::Functional | CriterionCategory::Constraint
        );
        let has_measurable = measurable_words.iter().any(|w| desc_lower.contains(w));
        if requires_measurable && !has_measurable && c.description.len() >= 20 {
            warnings.push(format!(
                "{}: No measurable language found — add 'must contain', 'must not', etc.",
                c.id
            ));
        }

        // Check weight is in valid range.
        if !(0.0..=1.0).contains(&c.weight) {
            warnings.push(format!(
                "{}: Weight {:.2} is outside valid range [0.0, 1.0]",
                c.id, c.weight
            ));
        }
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_generate_criteria_from_deliverables() {
        let plan = json!({
            "deliverables": ["a working API", "unit tests", "documentation"]
        });
        let criteria = generate_criteria(&plan);
        assert_eq!(criteria.len(), 3);
        assert!(matches!(criteria[0].category, CriterionCategory::Functional));
        assert!(criteria[0].description.contains("working API"));
    }

    #[test]
    fn test_generate_criteria_fallback() {
        let plan = json!({ "task": "write a sorting algorithm" });
        let criteria = generate_criteria(&plan);
        assert_eq!(criteria.len(), 4);
        assert!(criteria[0].description.contains("sorting algorithm"));
        // Weights should sum to 1.0
        let total: f64 = criteria.iter().map(|c| c.weight).sum();
        assert!((total - 1.0).abs() < 1e-9, "Weights must sum to 1.0, got {total}");
    }

    #[test]
    fn test_generate_criteria_all_categories() {
        let plan = json!({
            "deliverables": ["report"],
            "quality_bar": ["score >= 0.8"],
            "inputs": ["doc1", "doc2"],
            "constraints": ["no PII"]
        });
        let criteria = generate_criteria(&plan);
        assert_eq!(criteria.len(), 4);
        let categories: Vec<_> = criteria.iter().map(|c| &c.category).collect();
        assert!(categories.iter().any(|c| matches!(c, CriterionCategory::Functional)));
        assert!(categories.iter().any(|c| matches!(c, CriterionCategory::Quality)));
        assert!(categories.iter().any(|c| matches!(c, CriterionCategory::Completeness)));
        assert!(categories.iter().any(|c| matches!(c, CriterionCategory::Constraint)));
    }

    #[test]
    fn test_validate_criteria_passes_good_criteria() {
        let criteria = vec![
            IdealStateCriterion {
                id: "C1".to_string(),
                description: "The output must contain a valid JSON response".to_string(),
                category: CriterionCategory::Functional,
                weight: 0.5,
            },
            IdealStateCriterion {
                id: "C2".to_string(),
                description: "The output must not exceed 1000 tokens".to_string(),
                category: CriterionCategory::Constraint,
                weight: 0.5,
            },
        ];
        let warnings = validate_criteria(&criteria);
        assert!(warnings.is_empty(), "Expected no warnings, got: {warnings:?}");
    }

    #[test]
    fn test_validate_criteria_flags_vague_language() {
        let criteria = vec![IdealStateCriterion {
            id: "C1".to_string(),
            description: "Ensure the output is good quality and appropriate".to_string(),
            category: CriterionCategory::Quality,
            weight: 1.0,
        }];
        let warnings = validate_criteria(&criteria);
        assert!(!warnings.is_empty(), "Expected vague language warning");
    }

    #[test]
    fn test_criterion_ids_are_sequential() {
        let plan = json!({
            "deliverables": ["item1", "item2"],
            "constraints": ["no profanity"]
        });
        let criteria = generate_criteria(&plan);
        let ids: Vec<&str> = criteria.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["C1", "C2", "C3"]);
    }
}
