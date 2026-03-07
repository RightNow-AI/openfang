//! Ideal State Criteria (ISC) generation and management.
//!
//! HONEST NOTE: ISC is the strongest concept from Maestro's algorithm.
//! However, the original implementation let the LLM generate criteria
//! freeform, which produced vague, untestable criteria like "ensure quality."
//!
//! This module should enforce structured templates:
//! - FUNCTIONAL: "The output must contain X" (verifiable by string search)
//! - QUALITY: "The output must score >= N on metric M" (verifiable by eval)
//! - COMPLETENESS: "All N items from the input must appear in output"
//! - CONSTRAINT: "The output must not exceed N tokens / contain X"

use crate::IdealStateCriterion;

/// Generate ISC criteria from a plan output.
///
/// TODO: Implement structured ISC generation with templates.
/// The current approach (letting the LLM freeform generate criteria)
/// is the weakest link in the entire pipeline.
pub fn generate_criteria(_plan_output: &serde_json::Value) -> Vec<IdealStateCriterion> {
    todo!("Structured ISC generation")
}

/// Validate that criteria are testable (not vague).
pub fn validate_criteria(criteria: &[IdealStateCriterion]) -> Vec<String> {
    let mut warnings = Vec::new();
    for c in criteria {
        if c.description.len() < 20 {
            warnings.push(format!("{}: Description too short to be testable", c.id));
        }
        // TODO: Add more validation rules
        // - Check for measurable language ("must contain", "must not exceed")
        // - Flag vague language ("ensure", "improve", "good quality")
    }
    warnings
}
