//! Trace management — OpenTelemetry span creation and in-memory trace store.

use crate::{ObsError, Result, Trace, TraceStatus};
use chrono::Utc;
use dashmap::DashMap;
use opentelemetry::{
    global,
    trace::{Span, SpanKind, Tracer},
    KeyValue,
};
use opentelemetry_otlp::WithExportConfig;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

const MAX_TRACES: usize = 10_000;

/// In-memory store for completed traces with ring-buffer eviction.
#[derive(Clone)]
pub struct TraceStore {
    traces: Arc<RwLock<VecDeque<Trace>>>,
    agent_index: Arc<DashMap<String, Vec<Uuid>>>,
    session_index: Arc<DashMap<Uuid, Vec<Uuid>>>,
}

impl TraceStore {
    pub fn new() -> Self {
        Self {
            traces: Arc::new(RwLock::new(VecDeque::with_capacity(MAX_TRACES))),
            agent_index: Arc::new(DashMap::new()),
            session_index: Arc::new(DashMap::new()),
        }
    }

    pub async fn record(&self, trace: Trace) {
        let trace_id = trace.trace_id;
        let agent_id = trace.agent_id.clone();
        let session_id = trace.session_id;

        self.agent_index.entry(agent_id).or_default().push(trace_id);
        self.session_index.entry(session_id).or_default().push(trace_id);

        let mut buf = self.traces.write().await;
        if buf.len() >= MAX_TRACES {
            if let Some(evicted) = buf.pop_front() {
                if let Some(mut ids) = self.agent_index.get_mut(&evicted.agent_id) {
                    ids.retain(|id| *id != evicted.trace_id);
                }
                if let Some(mut ids) = self.session_index.get_mut(&evicted.session_id) {
                    ids.retain(|id| *id != evicted.trace_id);
                }
            }
        }
        buf.push_back(trace);
    }

    pub async fn for_agent(&self, agent_id: &str, limit: usize) -> Vec<Trace> {
        let buf = self.traces.read().await;
        buf.iter().rev().filter(|t| t.agent_id == agent_id).take(limit).cloned().collect()
    }

    pub async fn for_session(&self, session_id: Uuid) -> Vec<Trace> {
        let buf = self.traces.read().await;
        buf.iter().filter(|t| t.session_id == session_id).cloned().collect()
    }

    pub async fn recent(&self, limit: usize) -> Vec<Trace> {
        let buf = self.traces.read().await;
        buf.iter().rev().take(limit).cloned().collect()
    }

    pub async fn in_window(&self, window_secs: u64) -> Vec<Trace> {
        let cutoff = Utc::now() - chrono::Duration::seconds(window_secs as i64);
        let buf = self.traces.read().await;
        buf.iter().filter(|t| t.started_at >= cutoff).cloned().collect()
    }

    pub async fn count(&self) -> usize {
        self.traces.read().await.len()
    }

    pub async fn error_rate(&self, window_secs: u64) -> f64 {
        let traces = self.in_window(window_secs).await;
        if traces.is_empty() { return 0.0; }
        let errors = traces.iter().filter(|t| matches!(t.status, TraceStatus::Error | TraceStatus::Timeout)).count();
        errors as f64 / traces.len() as f64
    }

    pub async fn p99_latency_ms(&self, window_secs: u64) -> u64 {
        let mut latencies: Vec<u64> = self.in_window(window_secs).await.iter().map(|t| t.latency_ms).collect();
        if latencies.is_empty() { return 0; }
        latencies.sort_unstable();
        let idx = (latencies.len() as f64 * 0.99) as usize;
        latencies[idx.min(latencies.len() - 1)]
    }

    pub async fn total_cost_usd(&self, window_secs: u64) -> f64 {
        self.in_window(window_secs).await.iter().map(|t| t.cost_usd).sum()
    }

    pub async fn total_tokens(&self, window_secs: u64) -> u64 {
        self.in_window(window_secs).await.iter().map(|t| t.input_tokens + t.output_tokens).sum()
    }
}

impl Default for TraceStore {
    fn default() -> Self { Self::new() }
}

/// Builder for creating and completing traces with OpenTelemetry integration.
pub struct TraceBuilder {
    trace_id: Uuid,
    session_id: Uuid,
    agent_id: String,
    model_used: String,
    started_at: chrono::DateTime<Utc>,
    store: TraceStore,
}

impl TraceBuilder {
    pub fn start(agent_id: &str, model: &str, session_id: Uuid, store: TraceStore) -> Self {
        let trace_id = Uuid::new_v4();
        let started_at = Utc::now();

        let tracer = global::tracer("maestro-observability");
        let mut span = tracer
            .span_builder(format!("agent.interaction/{}", agent_id))
            .with_kind(SpanKind::Server)
            .start(&tracer);
        span.set_attribute(KeyValue::new("agent.id", agent_id.to_string()));
        span.set_attribute(KeyValue::new("agent.model", model.to_string()));
        span.set_attribute(KeyValue::new("session.id", session_id.to_string()));
        span.set_attribute(KeyValue::new("trace.id", trace_id.to_string()));
        span.end();

        info!(trace_id = %trace_id, agent_id = %agent_id, model = %model, "Trace started");

        Self {
            trace_id,
            session_id,
            agent_id: agent_id.to_string(),
            model_used: model.to_string(),
            started_at,
            store,
        }
    }

    pub async fn complete(
        self,
        input_tokens: u64,
        output_tokens: u64,
        cost_usd: f64,
        status: TraceStatus,
        metadata: serde_json::Value,
    ) -> Result<Trace> {
        let completed_at = Utc::now();
        let latency_ms = (completed_at - self.started_at).num_milliseconds().max(0) as u64;

        let trace = Trace {
            trace_id: self.trace_id,
            parent_trace_id: None,
            session_id: self.session_id,
            agent_id: self.agent_id.clone(),
            model_used: self.model_used.clone(),
            input_tokens,
            output_tokens,
            latency_ms,
            cost_usd,
            status,
            metadata,
            started_at: self.started_at,
            completed_at,
        };

        info!(
            trace_id = %trace.trace_id,
            agent_id = %trace.agent_id,
            latency_ms = trace.latency_ms,
            cost_usd = trace.cost_usd,
            "Trace completed"
        );

        self.store.record(trace.clone()).await;
        Ok(trace)
    }

    pub async fn fail(self, error: &str) -> Result<Trace> {
        self.complete(0, 0, 0.0, TraceStatus::Error, serde_json::json!({ "error": error })).await
    }

    pub fn trace_id(&self) -> Uuid { self.trace_id }
}

/// Initialize the OpenTelemetry tracer provider with OTLP export (HTTP/proto).
pub fn init_tracer(otlp_endpoint: Option<&str>) -> Result<()> {
    if let Some(endpoint) = otlp_endpoint {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(endpoint)
            .build()
            .map_err(|e: opentelemetry::trace::TraceError| ObsError::Otel(e.to_string()))?;

        let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .build();

        global::set_tracer_provider(provider);
        info!(endpoint = %endpoint, "OpenTelemetry OTLP tracer initialized");
    }
    Ok(())
}

/// Shut down the OpenTelemetry tracer provider.
pub fn shutdown_tracer() {
    // In opentelemetry 0.28, the provider shuts down when dropped.
    // We replace it with a no-op provider to trigger shutdown.
    let _ = global::set_tracer_provider(opentelemetry_sdk::trace::SdkTracerProvider::default());
}
