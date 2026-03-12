//! # maestro-eval
//!
//! Evaluation Studio for OpenFang. Provides structured test suites, automated
//! evaluation runs, LLM-as-judge scoring, regression detection, and A/B testing.
//!
//! ## Architecture
//!
//! ```text
//! TestSuite → SuiteRunner → EvalResult[] → RegressionTracker → Report
//!                ↓
//!          ScoringEngine (ExactMatch | Contains | Semantic | LlmAsJudge)
//! ```
//!
//! ## SWE Evaluation
//!
//! The `swe` module provides specialized test types for evaluating SWE (Software
//! Engineering) agents:
//!
//! ```text
//! SWETestSuite → SWETestRunner → SWESuiteReport
//!      ↓
//! SWETestCase (FileRead | FileWrite | CodeGeneration | BugFix | Refactoring)
//! ```

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use thiserror::Error;
use tracing::{info, warn};
use uuid::Uuid;

// SWE evaluation module
pub mod swe;
pub mod swe_runner;
pub mod swe_suites;

pub use swe::{SWEDifficulty, SWESuiteReport, SWETestCase, SWETestResult, SWETestSuite, SWETaskType};
pub use swe_runner::SWETestRunner;
pub use swe_suites::{
    create_advanced_suite, create_basic_suite, create_expert_suite, create_full_suite,
    create_intermediate_suite, get_suite_by_name,
};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum EvalError {
    #[error("Scoring error: {0}")]
    Scoring(String),
    #[error("Runner error: {0}")]
    Runner(String),
    #[error("Regression detected: {0}")]
    Regression(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

pub type EvalResult2 = Result<EvalResult, EvalError>;

// ---------------------------------------------------------------------------
// Core types (from stub, extended)
// ---------------------------------------------------------------------------

/// A test case for evaluating agent behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    pub id: Uuid,
    pub name: String,
    pub input: String,
    pub expected_output: Option<String>,
    pub scoring_method: ScoringMethod,
    pub tags: Vec<String>,
    pub metadata: HashMap<String, String>,
}

impl TestCase {
    pub fn new(
        name: impl Into<String>,
        input: impl Into<String>,
        expected: Option<String>,
        method: ScoringMethod,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            input: input.into(),
            expected_output: expected,
            scoring_method: method,
            tags: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoringMethod {
    ExactMatch,
    ContainsAll { required_strings: Vec<String> },
    ContainsNone { forbidden_strings: Vec<String> },
    SemanticSimilarity { threshold: f64 },
    LlmAsJudge { rubric: String },
    Custom { evaluator_name: String },
    Composite { methods: Vec<ScoringMethod>, weights: Vec<f64> },
}

/// Result of evaluating a single test case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResult {
    pub test_case_id: Uuid,
    pub test_case_name: String,
    pub score: f64,
    pub passed: bool,
    pub actual_output: String,
    pub reasoning: Option<String>,
    pub latency_ms: u64,
    pub tokens_used: u64,
    pub evaluated_at: DateTime<Utc>,
    pub tags: Vec<String>,
}

/// A test suite — a collection of test cases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuite {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub test_cases: Vec<TestCase>,
    pub version: String,
    pub pass_threshold: f64,
    pub created_at: DateTime<Utc>,
}

impl TestSuite {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: description.into(),
            test_cases: Vec::new(),
            version: "1.0.0".to_string(),
            pass_threshold: 0.8,
            created_at: Utc::now(),
        }
    }

    pub fn add_case(&mut self, case: TestCase) -> &mut Self {
        self.test_cases.push(case);
        self
    }
}

/// Summary of a completed suite run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuiteRunReport {
    pub suite_id: Uuid,
    pub suite_name: String,
    pub suite_version: String,
    pub run_id: Uuid,
    pub results: Vec<EvalResult>,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub pass_rate: f64,
    pub avg_score: f64,
    pub avg_latency_ms: f64,
    pub total_tokens: u64,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub regressions: Vec<RegressionItem>,
}

impl SuiteRunReport {
    pub fn is_passing(&self, threshold: f64) -> bool {
        self.pass_rate >= threshold
    }
}

// ---------------------------------------------------------------------------
// Scoring engine
// ---------------------------------------------------------------------------

pub struct ScoringEngine;

impl ScoringEngine {
    /// Score an actual output against a test case. Returns (score 0.0-1.0, reasoning).
    pub fn score(test: &TestCase, actual: &str) -> (f64, Option<String>) {
        match &test.scoring_method {
            ScoringMethod::ExactMatch => {
                let expected = test.expected_output.as_deref().unwrap_or("");
                let passed = actual.trim() == expected.trim();
                (if passed { 1.0 } else { 0.0 }, Some(format!("ExactMatch: expected='{}' actual='{}'", expected, actual)))
            }
            ScoringMethod::ContainsAll { required_strings } => {
                let lower = actual.to_lowercase();
                let found: Vec<&String> = required_strings.iter().filter(|s| lower.contains(s.to_lowercase().as_str())).collect();
                let score = found.len() as f64 / required_strings.len().max(1) as f64;
                let missing: Vec<&String> = required_strings.iter().filter(|s| !lower.contains(s.to_lowercase().as_str())).collect();
                let reasoning = if missing.is_empty() {
                    "All required strings found".to_string()
                } else {
                    format!("Missing: {:?}", missing)
                };
                (score, Some(reasoning))
            }
            ScoringMethod::ContainsNone { forbidden_strings } => {
                let lower = actual.to_lowercase();
                let found: Vec<&String> = forbidden_strings.iter().filter(|s| lower.contains(s.to_lowercase().as_str())).collect();
                let score = if found.is_empty() { 1.0 } else { 0.0 };
                let reasoning = if found.is_empty() {
                    "No forbidden strings found".to_string()
                } else {
                    format!("Found forbidden: {:?}", found)
                };
                (score, Some(reasoning))
            }
            ScoringMethod::SemanticSimilarity { threshold } => {
                // Without an embedding model, fall back to token overlap (Jaccard similarity)
                let expected = test.expected_output.as_deref().unwrap_or("");
                let score = jaccard_similarity(expected, actual);
                let _passed = score >= *threshold;
                (score, Some(format!("Jaccard similarity: {:.3} (threshold: {})", score, threshold)))
            }
            ScoringMethod::LlmAsJudge { rubric: _ } => {
                // Placeholder: in production, this calls an LLM with the rubric.
                // Returns 0.5 as a neutral score when no LLM is configured.
                (0.5, Some("LlmAsJudge: requires LLM configuration (returning neutral 0.5)".to_string()))
            }
            ScoringMethod::Custom { evaluator_name } => {
                warn!("Custom evaluator '{}' not registered, returning 0.0", evaluator_name);
                (0.0, Some(format!("Custom evaluator '{}' not found", evaluator_name)))
            }
            ScoringMethod::Composite { methods, weights } => {
                let mut total_weight = 0.0f64;
                let mut weighted_score = 0.0f64;
                let mut reasonings = Vec::new();
                for (method, weight) in methods.iter().zip(weights.iter()) {
                    let sub_test = TestCase {
                        scoring_method: method.clone(),
                        ..test.clone()
                    };
                    let (s, r) = Self::score(&sub_test, actual);
                    weighted_score += s * weight;
                    total_weight += weight;
                    if let Some(r) = r { reasonings.push(r); }
                }
                let score = if total_weight > 0.0 { weighted_score / total_weight } else { 0.0 };
                (score, Some(reasonings.join(" | ")))
            }
        }
    }
}

fn jaccard_similarity(a: &str, b: &str) -> f64 {
    let tokens_a: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let tokens_b: std::collections::HashSet<&str> = b.split_whitespace().collect();
    if tokens_a.is_empty() && tokens_b.is_empty() { return 1.0; }
    let intersection = tokens_a.intersection(&tokens_b).count();
    let union = tokens_a.union(&tokens_b).count();
    if union == 0 { 0.0 } else { intersection as f64 / union as f64 }
}

// ---------------------------------------------------------------------------
// EvalRunner trait
// ---------------------------------------------------------------------------

/// Trait for evaluation runners.
#[async_trait]
pub trait EvalRunner: Send + Sync {
    /// Run a single test case against an agent.
    async fn run_test(&self, test: &TestCase) -> anyhow::Result<EvalResult>;

    /// Run an entire test suite.
    async fn run_suite(&self, suite: &TestSuite) -> anyhow::Result<Vec<EvalResult>>;
}

// ---------------------------------------------------------------------------
// MockRunner — for testing without a live agent
// ---------------------------------------------------------------------------

pub struct MockRunner {
    pub response_fn: Arc<dyn Fn(&str) -> String + Send + Sync>,
    pub latency_ms: u64,
}

impl MockRunner {
    pub fn new(f: impl Fn(&str) -> String + Send + Sync + 'static) -> Self {
        Self { response_fn: Arc::new(f), latency_ms: 10 }
    }

    pub fn echo() -> Self {
        Self::new(|s| s.to_string())
    }
}

#[async_trait]
impl EvalRunner for MockRunner {
    async fn run_test(&self, test: &TestCase) -> anyhow::Result<EvalResult> {
        let start = Instant::now();
        let actual = (self.response_fn)(&test.input);
        let latency_ms = start.elapsed().as_millis() as u64 + self.latency_ms;
        let (score, reasoning) = ScoringEngine::score(test, &actual);
        Ok(EvalResult {
            test_case_id: test.id,
            test_case_name: test.name.clone(),
            score,
            passed: score >= 0.8,
            actual_output: actual,
            reasoning,
            latency_ms,
            tokens_used: 0,
            evaluated_at: Utc::now(),
            tags: test.tags.clone(),
        })
    }

    async fn run_suite(&self, suite: &TestSuite) -> anyhow::Result<Vec<EvalResult>> {
        let mut results = Vec::new();
        for test in &suite.test_cases {
            results.push(self.run_test(test).await?);
        }
        Ok(results)
    }
}

// ---------------------------------------------------------------------------
// SuiteRunner — full runner with reporting and regression detection
// ---------------------------------------------------------------------------

pub struct SuiteRunner {
    runner: Arc<dyn EvalRunner>,
    regression_tracker: Arc<RegressionTracker>,
}

impl SuiteRunner {
    pub fn new(runner: Arc<dyn EvalRunner>) -> Self {
        Self {
            runner,
            regression_tracker: Arc::new(RegressionTracker::new()),
        }
    }

    pub fn with_tracker(runner: Arc<dyn EvalRunner>, tracker: Arc<RegressionTracker>) -> Self {
        Self { runner, regression_tracker: tracker }
    }

    pub async fn run(&self, suite: &TestSuite) -> anyhow::Result<SuiteRunReport> {
        let started_at = Utc::now();
        let run_id = Uuid::new_v4();
        info!("Starting eval run {} for suite '{}' v{}", run_id, suite.name, suite.version);

        let results = self.runner.run_suite(suite).await?;

        let total = results.len();
        let passed = results.iter().filter(|r| r.passed).count();
        let failed = total - passed;
        let pass_rate = if total > 0 { passed as f64 / total as f64 } else { 0.0 };
        let avg_score = if total > 0 { results.iter().map(|r| r.score).sum::<f64>() / total as f64 } else { 0.0 };
        let avg_latency_ms = if total > 0 { results.iter().map(|r| r.latency_ms as f64).sum::<f64>() / total as f64 } else { 0.0 };
        let total_tokens: u64 = results.iter().map(|r| r.tokens_used).sum();

        let regressions = self.regression_tracker.detect_regressions(suite, &results);
        self.regression_tracker.record_run(suite, &results);

        let report = SuiteRunReport {
            suite_id: suite.id,
            suite_name: suite.name.clone(),
            suite_version: suite.version.clone(),
            run_id,
            results,
            total,
            passed,
            failed,
            pass_rate,
            avg_score,
            avg_latency_ms,
            total_tokens,
            started_at,
            finished_at: Utc::now(),
            regressions: regressions.clone(),
        };

        if regressions.is_empty() {
            info!("Suite '{}': {}/{} passed ({:.1}%), avg score {:.3}", suite.name, passed, total, pass_rate * 100.0, avg_score);
        } else {
            warn!("Suite '{}': {} regressions detected!", suite.name, regressions.len());
        }

        Ok(report)
    }

    pub fn tracker(&self) -> &RegressionTracker { &self.regression_tracker }
}

// ---------------------------------------------------------------------------
// Regression tracker
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionItem {
    pub test_case_id: Uuid,
    pub test_case_name: String,
    pub previous_score: f64,
    pub current_score: f64,
    pub delta: f64,
    pub severity: RegressionSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RegressionSeverity {
    Minor,   // 0.05-0.15 drop
    Major,   // 0.15-0.30 drop
    Critical, // >0.30 drop
}

#[derive(Debug, Default)]
pub struct RegressionTracker {
    history: RwLock<HashMap<Uuid, HashMap<Uuid, f64>>>,
}

impl RegressionTracker {
    pub fn new() -> Self { Self::default() }

    pub fn record_run(&self, suite: &TestSuite, results: &[EvalResult]) {
        let mut history = self.history.write().unwrap();
        let suite_history = history.entry(suite.id).or_default();
        for result in results {
            suite_history.insert(result.test_case_id, result.score);
        }
    }

    pub fn detect_regressions(&self, suite: &TestSuite, results: &[EvalResult]) -> Vec<RegressionItem> {
        let history = self.history.read().unwrap();
        let Some(suite_history) = history.get(&suite.id) else { return Vec::new(); };
        let mut regressions = Vec::new();
        for result in results {
            if let Some(&prev_score) = suite_history.get(&result.test_case_id) {
                let delta = result.score - prev_score;
                if delta < -0.05 {
                    let severity = if delta < -0.30 {
                        RegressionSeverity::Critical
                    } else if delta < -0.15 {
                        RegressionSeverity::Major
                    } else {
                        RegressionSeverity::Minor
                    };
                    regressions.push(RegressionItem {
                        test_case_id: result.test_case_id,
                        test_case_name: result.test_case_name.clone(),
                        previous_score: prev_score,
                        current_score: result.score,
                        delta,
                        severity,
                    });
                }
            }
        }
        regressions
    }

    pub fn get_history(&self, suite_id: Uuid) -> HashMap<Uuid, f64> {
        let history = self.history.read().unwrap();
        history.get(&suite_id).cloned().unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Benchmark runner
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    pub runs: usize,
    pub warmup_runs: usize,
    pub concurrency: usize,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self { runs: 10, warmup_runs: 2, concurrency: 1 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub suite_name: String,
    pub total_runs: usize,
    pub avg_pass_rate: f64,
    pub min_pass_rate: f64,
    pub max_pass_rate: f64,
    pub std_dev_pass_rate: f64,
    pub avg_latency_ms: f64,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub avg_score: f64,
    pub per_run_reports: Vec<SuiteRunReport>,
}

pub struct BenchmarkRunner {
    suite_runner: SuiteRunner,
    config: BenchmarkConfig,
}

impl BenchmarkRunner {
    pub fn new(runner: Arc<dyn EvalRunner>, config: BenchmarkConfig) -> Self {
        Self { suite_runner: SuiteRunner::new(runner), config }
    }

    pub async fn run(&self, suite: &TestSuite) -> anyhow::Result<BenchmarkReport> {
        let total_runs = self.config.warmup_runs + self.config.runs;
        let mut reports = Vec::new();

        info!("Starting benchmark: {} warmup + {} measured runs", self.config.warmup_runs, self.config.runs);

        for i in 0..total_runs {
            let report = self.suite_runner.run(suite).await?;
            if i >= self.config.warmup_runs {
                reports.push(report);
            }
        }

        let n = reports.len() as f64;
        let pass_rates: Vec<f64> = reports.iter().map(|r| r.pass_rate).collect();
        let latencies: Vec<f64> = reports.iter().flat_map(|r| r.results.iter().map(|res| res.latency_ms as f64)).collect();
        let scores: Vec<f64> = reports.iter().flat_map(|r| r.results.iter().map(|res| res.score)).collect();

        let avg_pass_rate = pass_rates.iter().sum::<f64>() / n;
        let min_pass_rate = pass_rates.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_pass_rate = pass_rates.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let variance = pass_rates.iter().map(|&x| (x - avg_pass_rate).powi(2)).sum::<f64>() / n;
        let std_dev_pass_rate = variance.sqrt();

        let avg_latency_ms = if latencies.is_empty() { 0.0 } else { latencies.iter().sum::<f64>() / latencies.len() as f64 };
        let avg_score = if scores.is_empty() { 0.0 } else { scores.iter().sum::<f64>() / scores.len() as f64 };

        let mut sorted_latencies = latencies.clone();
        sorted_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p50 = percentile(&sorted_latencies, 50.0);
        let p95 = percentile(&sorted_latencies, 95.0);
        let p99 = percentile(&sorted_latencies, 99.0);

        Ok(BenchmarkReport {
            suite_name: suite.name.clone(),
            total_runs: reports.len(),
            avg_pass_rate,
            min_pass_rate,
            max_pass_rate,
            std_dev_pass_rate,
            avg_latency_ms,
            p50_latency_ms: p50,
            p95_latency_ms: p95,
            p99_latency_ms: p99,
            avg_score,
            per_run_reports: reports,
        })
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() { return 0.0; }
    let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

// ---------------------------------------------------------------------------
// A/B testing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ABTestReport {
    pub suite_name: String,
    pub variant_a_name: String,
    pub variant_b_name: String,
    pub report_a: SuiteRunReport,
    pub report_b: SuiteRunReport,
    pub winner: Option<String>,
    pub score_delta: f64,
    pub pass_rate_delta: f64,
    pub latency_delta_ms: f64,
    pub per_case_deltas: Vec<CaseDelta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseDelta {
    pub test_case_id: Uuid,
    pub test_case_name: String,
    pub score_a: f64,
    pub score_b: f64,
    pub delta: f64,
}

pub struct ABTester {
    runner_a: Arc<dyn EvalRunner>,
    runner_b: Arc<dyn EvalRunner>,
    name_a: String,
    name_b: String,
}

impl ABTester {
    pub fn new(
        runner_a: Arc<dyn EvalRunner>,
        name_a: impl Into<String>,
        runner_b: Arc<dyn EvalRunner>,
        name_b: impl Into<String>,
    ) -> Self {
        Self { runner_a, runner_b, name_a: name_a.into(), name_b: name_b.into() }
    }

    pub async fn run(&self, suite: &TestSuite) -> anyhow::Result<ABTestReport> {
        let suite_runner_a = SuiteRunner::new(self.runner_a.clone());
        let suite_runner_b = SuiteRunner::new(self.runner_b.clone());

        let report_a = suite_runner_a.run(suite).await?;
        let report_b = suite_runner_b.run(suite).await?;

        let score_delta = report_b.avg_score - report_a.avg_score;
        let pass_rate_delta = report_b.pass_rate - report_a.pass_rate;
        let latency_delta_ms = report_b.avg_latency_ms - report_a.avg_latency_ms;

        let winner = if score_delta.abs() < 0.02 {
            None // Too close to call
        } else if score_delta > 0.0 {
            Some(self.name_b.clone())
        } else {
            Some(self.name_a.clone())
        };

        let scores_a: HashMap<Uuid, f64> = report_a.results.iter().map(|r| (r.test_case_id, r.score)).collect();
        let per_case_deltas = report_b.results.iter().map(|r| {
            let score_a = scores_a.get(&r.test_case_id).copied().unwrap_or(0.0);
            CaseDelta {
                test_case_id: r.test_case_id,
                test_case_name: r.test_case_name.clone(),
                score_a,
                score_b: r.score,
                delta: r.score - score_a,
            }
        }).collect();

        Ok(ABTestReport {
            suite_name: suite.name.clone(),
            variant_a_name: self.name_a.clone(),
            variant_b_name: self.name_b.clone(),
            report_a,
            report_b,
            winner,
            score_delta,
            pass_rate_delta,
            latency_delta_ms,
            per_case_deltas,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_suite() -> TestSuite {
        let mut suite = TestSuite::new("Test Suite", "Unit tests");
        suite.add_case(TestCase::new(
            "exact_match_pass",
            "What is 2+2?",
            Some("4".to_string()),
            ScoringMethod::ExactMatch,
        ));
        suite.add_case(TestCase::new(
            "contains_all_pass",
            "Tell me about Rust",
            None,
            ScoringMethod::ContainsAll { required_strings: vec!["rust".to_string(), "systems".to_string()] },
        ));
        suite.add_case(TestCase::new(
            "contains_none_pass",
            "Is Rust safe?",
            None,
            ScoringMethod::ContainsNone { forbidden_strings: vec!["unsafe".to_string()] },
        ));
        suite
    }

    #[test]
    fn test_exact_match_scoring() {
        let case = TestCase::new("t", "q", Some("hello".to_string()), ScoringMethod::ExactMatch);
        let (score, _) = ScoringEngine::score(&case, "hello");
        assert_eq!(score, 1.0);
        let (score2, _) = ScoringEngine::score(&case, "world");
        assert_eq!(score2, 0.0);
    }

    #[test]
    fn test_contains_all_scoring() {
        let case = TestCase::new("t", "q", None, ScoringMethod::ContainsAll {
            required_strings: vec!["foo".to_string(), "bar".to_string()],
        });
        let (score, _) = ScoringEngine::score(&case, "foo and bar");
        assert_eq!(score, 1.0);
        let (score2, _) = ScoringEngine::score(&case, "only foo");
        assert_eq!(score2, 0.5);
    }

    #[test]
    fn test_contains_none_scoring() {
        let case = TestCase::new("t", "q", None, ScoringMethod::ContainsNone {
            forbidden_strings: vec!["bad".to_string()],
        });
        let (score, _) = ScoringEngine::score(&case, "good content");
        assert_eq!(score, 1.0);
        let (score2, _) = ScoringEngine::score(&case, "this is bad");
        assert_eq!(score2, 0.0);
    }

    #[test]
    fn test_jaccard_similarity() {
        assert_eq!(jaccard_similarity("a b c", "a b c"), 1.0);
        assert_eq!(jaccard_similarity("a b", "c d"), 0.0);
        assert!(jaccard_similarity("a b c", "a b d") > 0.0);
    }

    #[tokio::test]
    async fn test_mock_runner_exact_match() {
        let runner = MockRunner::echo();
        let case = TestCase::new("echo", "hello", Some("hello".to_string()), ScoringMethod::ExactMatch);
        let result = runner.run_test(&case).await.unwrap();
        assert_eq!(result.score, 1.0);
        assert!(result.passed);
    }

    #[tokio::test]
    async fn test_suite_runner_report() {
        let suite = make_suite();
        let runner = Arc::new(MockRunner::new(|input| {
            if input.contains("2+2") { "4".to_string() }
            else if input.contains("Rust") { "rust is a systems language".to_string() }
            else { "Rust is safe and memory efficient".to_string() }
        }));
        let suite_runner = SuiteRunner::new(runner);
        let report = suite_runner.run(&suite).await.unwrap();
        assert_eq!(report.total, 3);
        assert!(report.pass_rate >= 0.0);
    }

    #[tokio::test]
    async fn test_regression_detection() {
        let mut suite = TestSuite::new("Regression Test", "");
        let case = TestCase::new("t1", "q", Some("good".to_string()), ScoringMethod::ExactMatch);
        let case_id = case.id;
        suite.add_case(case);

        let tracker = Arc::new(RegressionTracker::new());

        // First run: score 1.0
        let good_results = vec![EvalResult {
            test_case_id: case_id,
            test_case_name: "t1".to_string(),
            score: 1.0,
            passed: true,
            actual_output: "good".to_string(),
            reasoning: None,
            latency_ms: 10,
            tokens_used: 0,
            evaluated_at: Utc::now(),
            tags: vec![],
        }];
        tracker.record_run(&suite, &good_results);

        // Second run: score 0.5 (regression)
        let bad_results = vec![EvalResult {
            test_case_id: case_id,
            test_case_name: "t1".to_string(),
            score: 0.5,
            passed: false,
            actual_output: "bad".to_string(),
            reasoning: None,
            latency_ms: 10,
            tokens_used: 0,
            evaluated_at: Utc::now(),
            tags: vec![],
        }];
        let regressions = tracker.detect_regressions(&suite, &bad_results);
        assert_eq!(regressions.len(), 1);
        // delta = 0.5 - 1.0 = -0.5, which is < -0.30 => Critical
        assert_eq!(regressions[0].severity, RegressionSeverity::Critical);
    }

    #[tokio::test]
    async fn test_ab_tester() {
        let suite = make_suite();
        let runner_a = Arc::new(MockRunner::new(|input| {
            if input.contains("2+2") { "4".to_string() }
            else if input.contains("Rust") { "rust is a systems language".to_string() }
            else { "Rust is safe".to_string() }
        }));
        let runner_b = Arc::new(MockRunner::new(|input| {
            if input.contains("2+2") { "4".to_string() }
            else if input.contains("Rust") { "rust systems language".to_string() }
            else { "Rust is safe and fast".to_string() }
        }));
        let tester = ABTester::new(runner_a, "v1", runner_b, "v2");
        let report = tester.run(&suite).await.unwrap();
        assert_eq!(report.suite_name, "Test Suite");
        assert_eq!(report.per_case_deltas.len(), 3);
    }

    #[tokio::test]
    async fn test_benchmark_runner() {
        let suite = make_suite();
        let runner = Arc::new(MockRunner::new(|input| {
            if input.contains("2+2") { "4".to_string() }
            else if input.contains("Rust") { "rust is a systems language".to_string() }
            else { "Rust is safe".to_string() }
        }));
        let config = BenchmarkConfig { runs: 3, warmup_runs: 1, concurrency: 1 };
        let bench = BenchmarkRunner::new(runner, config);
        let report = bench.run(&suite).await.unwrap();
        assert_eq!(report.total_runs, 3);
        assert!(report.avg_pass_rate >= 0.0);
    }
}
