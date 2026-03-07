//! # maestro-eval
//!
//! Evaluation Studio inspired by Kore.ai's Evaluation Studio.
//!
//! ## What Kore.ai Has
//!
//! - Test suite management with versioned test cases
//! - Automated evaluation runs against agent deployments
//! - Multiple scoring methods (exact match, semantic similarity, LLM-as-judge)
//! - A/B testing between agent versions
//! - Regression detection
//!
//! ## What OpenFang Has
//!
//! - `openfang-eval` crate (exists but minimal)
//! - Basic test harness for agent responses
//! - BUT: No structured test suites, no scoring, no regression detection
//!
//! ## HONEST GAPS
//!
//! - LLM-as-judge scoring requires additional LLM calls (cost)
//! - Semantic similarity requires embedding model access
//! - No UI for test suite management
//! - No integration with CI/CD pipelines
//! - No statistical significance testing for A/B comparisons

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A test case for evaluating agent behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    pub id: Uuid,
    pub name: String,
    pub input: String,
    pub expected_output: Option<String>,
    pub scoring_method: ScoringMethod,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoringMethod {
    ExactMatch,
    ContainsAll { required_strings: Vec<String> },
    SemanticSimilarity { threshold: f64 },
    LlmAsJudge { rubric: String },
    Custom { evaluator_name: String },
}

/// Result of evaluating a single test case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResult {
    pub test_case_id: Uuid,
    pub score: f64,
    pub passed: bool,
    pub actual_output: String,
    pub reasoning: Option<String>,
    pub latency_ms: u64,
    pub tokens_used: u64,
    pub evaluated_at: DateTime<Utc>,
}

/// A test suite — a collection of test cases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuite {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub test_cases: Vec<TestCase>,
    pub version: String,
}

/// Trait for evaluation runners.
#[async_trait::async_trait]
pub trait EvalRunner: Send + Sync {
    /// Run a single test case against an agent.
    async fn run_test(&self, test: &TestCase) -> anyhow::Result<EvalResult>;

    /// Run an entire test suite.
    async fn run_suite(&self, suite: &TestSuite) -> anyhow::Result<Vec<EvalResult>>;
}
