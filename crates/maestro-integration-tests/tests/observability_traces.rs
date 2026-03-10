//! Integration tests for the maestro-observability trace store.
//!
//! These tests verify that traces are recorded, indexed, and queryable
//! without requiring any external services (OpenTelemetry collector, etc.).

use maestro_observability::{Trace, TraceStatus, traces::TraceStore};
use uuid::Uuid;
use chrono::Utc;

fn make_trace(agent_id: &str, model: &str, latency_ms: u64, cost_usd: f64, status: TraceStatus) -> Trace {
    let now = Utc::now();
    Trace {
        trace_id: Uuid::new_v4(),
        parent_trace_id: None,
        session_id: Uuid::new_v4(),
        agent_id: agent_id.to_string(),
        model_used: model.to_string(),
        input_tokens: 100,
        output_tokens: 50,
        latency_ms,
        cost_usd,
        status,
        metadata: serde_json::json!({"test": true}),
        started_at: now - chrono::Duration::milliseconds(latency_ms as i64),
        completed_at: now,
    }
}

#[tokio::test]
async fn test_trace_store_records_and_retrieves() {
    let store = TraceStore::new();
    let trace = make_trace("agent-1", "gpt-4.1-mini", 250, 0.001, TraceStatus::Success);
    let trace_id = trace.trace_id;
    store.record(trace).await;

    let recent = store.recent(10).await;
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].trace_id, trace_id);
}

#[tokio::test]
async fn test_trace_store_filters_by_agent() {
    let store = TraceStore::new();
    store.record(make_trace("agent-alpha", "gpt-4.1-mini", 100, 0.001, TraceStatus::Success)).await;
    store.record(make_trace("agent-beta", "gpt-4.1-mini", 200, 0.002, TraceStatus::Success)).await;
    store.record(make_trace("agent-alpha", "gpt-4.1-mini", 150, 0.001, TraceStatus::Success)).await;

    let alpha_traces = store.for_agent("agent-alpha", 10).await;
    assert_eq!(alpha_traces.len(), 2, "Should return 2 traces for agent-alpha");

    let beta_traces = store.for_agent("agent-beta", 10).await;
    assert_eq!(beta_traces.len(), 1, "Should return 1 trace for agent-beta");
}

#[tokio::test]
async fn test_trace_store_error_rate() {
    let store = TraceStore::new();
    store.record(make_trace("agent-1", "gpt-4.1-mini", 100, 0.001, TraceStatus::Success)).await;
    store.record(make_trace("agent-1", "gpt-4.1-mini", 100, 0.001, TraceStatus::Success)).await;
    store.record(make_trace("agent-1", "gpt-4.1-mini", 100, 0.001, TraceStatus::Error)).await;
    store.record(make_trace("agent-1", "gpt-4.1-mini", 100, 0.001, TraceStatus::Error)).await;

    let error_rate = store.error_rate(3600).await;
    assert!(
        (error_rate - 0.5).abs() < 0.01,
        "Error rate should be 0.5 (2 errors out of 4 traces); got {error_rate}"
    );
}

#[tokio::test]
async fn test_trace_store_total_cost() {
    let store = TraceStore::new();
    store.record(make_trace("agent-1", "gpt-4.1-mini", 100, 0.005, TraceStatus::Success)).await;
    store.record(make_trace("agent-1", "gpt-4.1-mini", 100, 0.003, TraceStatus::Success)).await;
    store.record(make_trace("agent-1", "gpt-4.1-mini", 100, 0.002, TraceStatus::Success)).await;

    let total_cost = store.total_cost_usd(3600).await;
    assert!(
        (total_cost - 0.010).abs() < 0.0001,
        "Total cost should be $0.010; got {total_cost}"
    );
}

#[tokio::test]
async fn test_trace_store_p99_latency() {
    let store = TraceStore::new();
    // Record 100 traces with latencies 1ms to 100ms
    for i in 1u64..=100 {
        store.record(make_trace("agent-1", "gpt-4.1-mini", i, 0.001, TraceStatus::Success)).await;
    }

    let p99 = store.p99_latency_ms(3600).await;
    assert!(
        p99 >= 99,
        "p99 latency should be >= 99ms for traces 1-100ms; got {p99}ms"
    );
}

#[tokio::test]
async fn test_trace_store_count() {
    let store = TraceStore::new();
    assert_eq!(store.count().await, 0, "Empty store should have count 0");

    for _ in 0..5 {
        store.record(make_trace("agent-1", "gpt-4.1-mini", 100, 0.001, TraceStatus::Success)).await;
    }
    assert_eq!(store.count().await, 5, "Store should have count 5 after 5 records");
}
