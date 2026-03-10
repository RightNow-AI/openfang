//! Integration tests for the maestro-algorithm MAESTRO 7-phase algorithm.
//!
//! These tests verify that the ISC generation, validation, and the
//! full MAESTRO phase sequence work correctly end-to-end.

use maestro_algorithm::{
    isc::{generate_criteria, validate_criteria},
    CriterionCategory, Phase,
};
use serde_json::json;

// ─── ISC generation tests ─────────────────────────────────────────────────────

#[test]
fn test_generate_criteria_returns_non_empty_set() {
    // generate_criteria takes a serde_json::Value (plan output)
    let plan = json!({ "task": "Build a REST API for user authentication" });
    let criteria = generate_criteria(&plan);
    assert!(!criteria.is_empty(), "generate_criteria should return at least one criterion");
}

#[test]
fn test_generate_criteria_covers_all_categories() {
    let plan = json!({
        "task": "Design a machine learning pipeline for image classification",
        "deliverables": ["trained model", "evaluation report"],
        "quality_bar": ["accuracy >= 0.90"],
        "inputs": ["image dataset", "labels"],
        "constraints": ["must run on CPU only"]
    });
    let criteria = generate_criteria(&plan);
    let has_functional = criteria.iter().any(|c| matches!(c.category, CriterionCategory::Functional));
    let has_quality = criteria.iter().any(|c| matches!(c.category, CriterionCategory::Quality));
    let has_completeness = criteria.iter().any(|c| matches!(c.category, CriterionCategory::Completeness));
    let has_constraint = criteria.iter().any(|c| matches!(c.category, CriterionCategory::Constraint));
    assert!(has_functional, "Should have at least one Functional criterion");
    assert!(has_quality, "Should have at least one Quality criterion");
    assert!(has_completeness, "Should have at least one Completeness criterion");
    assert!(has_constraint, "Should have at least one Constraint criterion");
}

#[test]
fn test_generate_criteria_weights_sum_to_one() {
    let plan = json!({
        "task": "Write a comprehensive test suite for a Rust web server",
        "deliverables": ["test suite", "coverage report"],
        "quality_bar": ["coverage >= 80%"],
        "inputs": ["source code"],
        "constraints": ["tests must be deterministic"]
    });
    let criteria = generate_criteria(&plan);
    let total_weight: f64 = criteria.iter().map(|c| c.weight).sum();
    assert!(
        (total_weight - 1.0).abs() < 0.01,
        "Criterion weights should sum to 1.0; got {total_weight}"
    );
}

#[test]
fn test_validate_criteria_passes_valid_set() {
    // Use a fully-structured plan so that generate_criteria produces criteria with
    // measurable language ("must contain or produce", "must meet", "must cover",
    // "must not violate") that pass the validate_criteria measurability check.
    let plan = json!({
        "task": "Implement a secure password hashing function",
        "deliverables": ["hashed password output"],
        "quality_bar": ["bcrypt cost >= 12"],
        "inputs": ["plaintext password"],
        "constraints": ["must not store plaintext"]
    });
    let criteria = generate_criteria(&plan);
    let warnings = validate_criteria(&criteria);
    assert!(
        warnings.is_empty(),
        "Valid auto-generated criteria should produce no warnings; got: {:?}",
        warnings
    );
}

#[test]
fn test_generate_criteria_fallback_for_empty_plan() {
    // An empty JSON object should trigger the fallback criteria generation
    let plan = json!({});
    let criteria = generate_criteria(&plan);
    assert!(
        criteria.len() >= 4,
        "Fallback should generate at least 4 criteria; got {}",
        criteria.len()
    );
}

// ─── Phase enum tests ─────────────────────────────────────────────────────────

#[test]
fn test_phase_sequence_is_ordered() {
    let phases = [
        Phase::Observe,
        Phase::Orient,
        Phase::Plan,
        Phase::Execute,
        Phase::Verify,
        Phase::Learn,
        Phase::Adapt,
    ];
    // Verify the phase ordering is deterministic and complete
    assert_eq!(phases.len(), 7, "MAESTRO should have exactly 7 phases");
    // Each phase should have a distinct display name
    let names: Vec<String> = phases.iter().map(|p| p.to_string()).collect();
    let unique_names: std::collections::HashSet<&String> = names.iter().collect();
    assert_eq!(unique_names.len(), 7, "All 7 MAESTRO phases should have distinct names");
}

#[test]
fn test_phase_display_names_are_uppercase() {
    assert_eq!(Phase::Observe.to_string(), "OBSERVE");
    assert_eq!(Phase::Orient.to_string(), "ORIENT");
    assert_eq!(Phase::Plan.to_string(), "PLAN");
    assert_eq!(Phase::Execute.to_string(), "EXECUTE");
    assert_eq!(Phase::Verify.to_string(), "VERIFY");
    assert_eq!(Phase::Learn.to_string(), "LEARN");
    assert_eq!(Phase::Adapt.to_string(), "ADAPT");
}

#[test]
fn test_generate_criteria_descriptions_are_non_trivial() {
    let plan = json!({ "task": "Create a real-time chat application" });
    let criteria = generate_criteria(&plan);
    for criterion in &criteria {
        assert!(
            criterion.description.len() >= 20,
            "Criterion '{}' description is too short: '{}'",
            criterion.id,
            criterion.description
        );
        assert!(
            !criterion.description.is_empty(),
            "Criterion '{}' should have a non-empty description",
            criterion.id
        );
    }
}
