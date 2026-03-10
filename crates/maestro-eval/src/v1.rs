use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum EvaluationError {
    #[error("Failed to run test case: {0}")]
    TestCaseRun(String),
}

pub type EvaluationResult<T> = Result<T, EvaluationError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    pub id: Uuid,
    pub name: String,
    pub prompt: String,
    pub expected_output: Option<String>,
    pub scoring_method: ScoringMethod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScoringMethod {
    ExactMatch,
    Contains,
    SemanticSimilarity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub test_case_id: Uuid,
    pub score: f64,
    pub passed: bool,
    pub actual_output: String,
    pub evaluated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuite {
    pub id: Uuid,
    pub name: String,
    pub test_cases: Vec<TestCase>,
}

#[async_trait]
pub trait Evaluator: Send + Sync {
    async fn evaluate(&self, test_case: &TestCase) -> EvaluationResult<TestResult>;
}

pub struct MockEvaluator;

#[async_trait]
impl Evaluator for MockEvaluator {
    async fn evaluate(&self, test_case: &TestCase) -> EvaluationResult<TestResult> {
        let actual_output = "mock output".to_string();
        let score = match test_case.scoring_method {
            ScoringMethod::ExactMatch => {
                if let Some(expected) = &test_case.expected_output {
                    if &actual_output == expected {
                        1.0
                    } else {
                        0.0
                    }
                } else {
                    0.0
                }
            }
            ScoringMethod::Contains => {
                if let Some(expected) = &test_case.expected_output {
                    if actual_output.contains(expected) {
                        1.0
                    } else {
                        0.0
                    }
                } else {
                    0.0
                }
            }
            ScoringMethod::SemanticSimilarity => 0.5, // Placeholder
        };

        Ok(TestResult {
            test_case_id: test_case.id,
            score,
            passed: score >= 0.5,
            actual_output,
            evaluated_at: Utc::now(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestReport {
    pub suite_id: Uuid,
    pub suite_name: String,
    pub results: Vec<TestResult>,
    pub total_tests: usize,
    pub passed_tests: usize,
    pub failed_tests: usize,
    pub pass_rate: f64,
    pub average_score: f64,
    pub run_at: DateTime<Utc>,
}

pub struct TestSuiteRunner<E: Evaluator> {
    evaluator: E,
}

impl<E: Evaluator> TestSuiteRunner<E> {
    pub fn new(evaluator: E) -> Self {
        Self { evaluator }
    }

    pub async fn run_suite(&self, suite: &TestSuite) -> TestReport {
        let mut results = Vec::new();
        for test_case in &suite.test_cases {
            if let Ok(result) = self.evaluator.evaluate(test_case).await {
                results.push(result);
            }
        }

        let total_tests = results.len();
        let passed_tests = results.iter().filter(|r| r.passed).count();
        let failed_tests = total_tests - passed_tests;
        let pass_rate = if total_tests > 0 {
            (passed_tests as f64 / total_tests as f64) * 100.0
        } else {
            0.0
        };
        let average_score = if total_tests > 0 {
            results.iter().map(|r| r.score).sum::<f64>() / total_tests as f64
        } else {
            0.0
        };

        TestReport {
            suite_id: suite.id,
            suite_name: suite.name.clone(),
            results,
            total_tests,
            passed_tests,
            failed_tests,
            pass_rate,
            average_score,
            run_at: Utc::now(),
        }
    }
}
