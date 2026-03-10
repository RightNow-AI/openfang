//! Integration tests for the maestro-eval scoring and regression tracking.
//!
//! These tests verify that the ScoringEngine, RegressionTracker, and
//! BenchmarkRunner work correctly end-to-end without requiring an LLM.

use maestro_eval::{
    ScoringEngine, ScoringMethod, TestCase,
    RegressionTracker, RegressionSeverity,
    SuiteRunner, TestSuite, MockRunner, BenchmarkRunner, BenchmarkConfig,
};
use std::sync::Arc;

// ─── ScoringEngine tests ──────────────────────────────────────────────────────

#[test]
fn test_exact_match_scoring_pass() {
    let tc = TestCase::new(
        "exact-match",
        "What is the capital of France?",
        Some("Paris".to_string()),
        ScoringMethod::ExactMatch,
    );
    let (score, _) = ScoringEngine::score(&tc, "Paris");
    assert!(
        (score - 1.0).abs() < 0.001,
        "Exact match should score 1.0; got {score}"
    );
}

#[test]
fn test_exact_match_scoring_fail() {
    let tc = TestCase::new(
        "exact-match-fail",
        "What is the capital of France?",
        Some("Paris".to_string()),
        ScoringMethod::ExactMatch,
    );
    let (score, _) = ScoringEngine::score(&tc, "London");
    assert!(
        (score - 0.0).abs() < 0.001,
        "Non-match should score 0.0; got {score}"
    );
}

#[test]
fn test_contains_all_scoring_pass() {
    let tc = TestCase::new(
        "contains-all",
        "Describe Paris",
        None,
        ScoringMethod::ContainsAll {
            required_strings: vec!["Paris".to_string(), "France".to_string()],
        },
    );
    let (score, _) = ScoringEngine::score(&tc, "Paris is the capital of France.");
    assert!(
        (score - 1.0).abs() < 0.001,
        "ContainsAll should score 1.0 when all strings present; got {score}"
    );
}

#[test]
fn test_contains_all_scoring_partial() {
    let tc = TestCase::new(
        "contains-all-partial",
        "Describe Paris",
        None,
        ScoringMethod::ContainsAll {
            required_strings: vec!["Paris".to_string(), "France".to_string()],
        },
    );
    let (score, _) = ScoringEngine::score(&tc, "Paris is a great city.");
    assert!(
        score > 0.0 && score < 1.0,
        "ContainsAll should score between 0 and 1 when only some strings present; got {score}"
    );
}

#[test]
fn test_contains_none_scoring_pass() {
    let tc = TestCase::new(
        "contains-none",
        "Describe Paris",
        None,
        ScoringMethod::ContainsNone {
            forbidden_strings: vec!["Berlin".to_string(), "London".to_string()],
        },
    );
    let (score, _) = ScoringEngine::score(&tc, "Paris is the capital of France.");
    assert!(
        (score - 1.0).abs() < 0.001,
        "ContainsNone should score 1.0 when no forbidden strings present; got {score}"
    );
}

#[test]
fn test_semantic_similarity_scoring() {
    let tc = TestCase::new(
        "semantic",
        "Describe the Eiffel Tower",
        Some("The Eiffel Tower is in Paris France".to_string()),
        ScoringMethod::SemanticSimilarity { threshold: 0.5 },
    );
    let (score, _) = ScoringEngine::score(&tc, "The Eiffel Tower is in Paris, France");
    assert!(
        score > 0.5,
        "Similar strings should score > 0.5 with Jaccard similarity; got {score}"
    );
}

// ─── RegressionTracker tests ──────────────────────────────────────────────────

#[tokio::test]
async fn test_regression_tracker_no_regression() {
    let runner = Arc::new(MockRunner::echo());
    let tracker = Arc::new(RegressionTracker::new());
    let suite_runner = SuiteRunner::with_tracker(runner, tracker.clone());

    let mut suite = TestSuite::new("stable-suite", "A stable test suite");
    suite.add_case(TestCase::new(
        "echo-test",
        "hello",
        Some("hello".to_string()),
        ScoringMethod::ExactMatch,
    ));

    // Run twice to establish baseline and current
    let results1 = suite_runner.run(&suite).await.expect("run 1");
    tracker.record_run(&suite, &results1.results);
    let results2 = suite_runner.run(&suite).await.expect("run 2");
    let regressions = tracker.detect_regressions(&suite, &results2.results);
    assert!(
        regressions.is_empty(),
        "No regression should be detected for stable scores; got: {regressions:?}"
    );
}

#[tokio::test]
async fn test_regression_tracker_detects_major_regression() {
    let tracker = Arc::new(RegressionTracker::new());

    // Simulate a previous run with high scores by recording a suite run
    let runner_good = Arc::new(MockRunner::new(|_| "Paris".to_string()));
    let runner_bad = Arc::new(MockRunner::new(|_| "wrong answer".to_string()));

    let mut suite = TestSuite::new("regression-suite", "A suite that regresses");
    suite.add_case(TestCase::new(
        "capital-test",
        "What is the capital of France?",
        Some("Paris".to_string()),
        ScoringMethod::ExactMatch,
    ));

    let suite_runner_good = SuiteRunner::with_tracker(runner_good, tracker.clone());
    let results_good = suite_runner_good.run(&suite).await.expect("run good");
    tracker.record_run(&suite, &results_good.results);

    let suite_runner_bad = SuiteRunner::new(runner_bad);
    let results_bad = suite_runner_bad.run(&suite).await.expect("run bad");
    let regressions = tracker.detect_regressions(&suite, &results_bad.results);

    assert!(
        !regressions.is_empty(),
        "Major regression should be detected when score drops from 1.0 to 0.0"
    );
    let r = &regressions[0];
    assert!(
        matches!(r.severity, RegressionSeverity::Major | RegressionSeverity::Critical),
        "Severity should be Major or Critical for a 1.0 drop; got {:?}",
        r.severity
    );
}

// ─── BenchmarkRunner tests ────────────────────────────────────────────────────

#[tokio::test]
async fn test_benchmark_runner_runs_suite() {
    let runner = Arc::new(MockRunner::echo());
    let config = BenchmarkConfig {
        runs: 3,
        warmup_runs: 1,
        concurrency: 1,
    };
    let bench = BenchmarkRunner::new(runner, config);

    let mut suite = TestSuite::new("bench-suite", "Benchmark test suite");
    suite.add_case(TestCase::new(
        "echo-bench",
        "hello",
        Some("hello".to_string()),
        ScoringMethod::ExactMatch,
    ));

    let report = bench.run(&suite).await.expect("benchmark run");
    assert_eq!(report.total_runs, 3, "Should have 3 measured runs (1 warmup excluded)");
    assert!(
        report.avg_pass_rate >= 0.0 && report.avg_pass_rate <= 1.0,
        "avg_pass_rate should be in [0, 1]; got {}",
        report.avg_pass_rate
    );
}

#[tokio::test]
async fn test_benchmark_runner_echo_scores_perfect() {
    let runner = Arc::new(MockRunner::echo());
    let config = BenchmarkConfig {
        runs: 2,
        warmup_runs: 0,
        concurrency: 1,
    };
    let bench = BenchmarkRunner::new(runner, config);

    let mut suite = TestSuite::new("perfect-bench", "Perfect scoring benchmark");
    suite.add_case(TestCase::new(
        "echo-perfect",
        "exact",
        Some("exact".to_string()),
        ScoringMethod::ExactMatch,
    ));

    let report = bench.run(&suite).await.expect("benchmark run");
    assert!(
        (report.avg_pass_rate - 1.0).abs() < 0.001,
        "Echo runner with exact match should have 100% pass rate; got {}",
        report.avg_pass_rate
    );
    assert!(
        (report.avg_score - 1.0).abs() < 0.001,
        "Echo runner with exact match should have avg_score 1.0; got {}",
        report.avg_score
    );
}
