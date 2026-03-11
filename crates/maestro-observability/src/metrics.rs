//! Metrics collection — real-time counters and per-agent dashboards.
//!
//! Uses OpenTelemetry Metrics API for standard instrument types (counter,
//! histogram, gauge) and a `MetricsStore` for in-process aggregation that
//! powers the dashboard without requiring an external metrics backend.

use crate::{Trace, TraceStatus};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use opentelemetry::{global, KeyValue};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

/// Per-agent metrics snapshot for the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetrics {
    pub agent_id: String,
    pub total_traces: u64,
    pub error_count: u64,
    pub timeout_count: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost_usd: f64,
    pub avg_latency_ms: f64,
    pub p99_latency_ms: u64,
    pub last_active_at: Option<DateTime<Utc>>,
}

/// System-wide metrics snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub total_traces: u64,
    pub active_agents: u64,
    pub total_tokens_1h: u64,
    pub total_cost_1h_usd: f64,
    pub error_rate_5m: f64,
    pub p99_latency_ms_5m: u64,
    pub snapshot_at: DateTime<Utc>,
}

/// Accumulated raw data per agent for metric computation.
#[derive(Debug, Default)]
struct AgentAccumulator {
    total_traces: u64,
    error_count: u64,
    timeout_count: u64,
    total_input_tokens: u64,
    total_output_tokens: u64,
    total_cost_usd: f64,
    latencies: Vec<u64>,
    last_active_at: Option<DateTime<Utc>>,
}

/// In-process metrics store that powers the observability dashboard.
#[derive(Clone)]
#[allow(clippy::type_complexity)]
pub struct MetricsStore {
    agents: Arc<DashMap<String, AgentAccumulator>>,
    recent_latencies: Arc<RwLock<Vec<(DateTime<Utc>, u64)>>>,
    recent_errors: Arc<RwLock<Vec<(DateTime<Utc>, bool)>>>,
    recent_tokens: Arc<RwLock<Vec<(DateTime<Utc>, u64)>>>,
    recent_costs: Arc<RwLock<Vec<(DateTime<Utc>, f64)>>>,
}

impl MetricsStore {
    pub fn new() -> Self {
        Self {
            agents: Arc::new(DashMap::new()),
            recent_latencies: Arc::new(RwLock::new(Vec::new())),
            recent_errors: Arc::new(RwLock::new(Vec::new())),
            recent_tokens: Arc::new(RwLock::new(Vec::new())),
            recent_costs: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn ingest(&self, trace: &Trace) {
        let now = Utc::now();
        let is_error = matches!(trace.status, TraceStatus::Error | TraceStatus::Timeout);
        let total_tokens = trace.input_tokens + trace.output_tokens;

        let mut acc = self.agents.entry(trace.agent_id.clone()).or_default();
        acc.total_traces += 1;
        if matches!(trace.status, TraceStatus::Error) { acc.error_count += 1; }
        if matches!(trace.status, TraceStatus::Timeout) { acc.timeout_count += 1; }
        acc.total_input_tokens += trace.input_tokens;
        acc.total_output_tokens += trace.output_tokens;
        acc.total_cost_usd += trace.cost_usd;
        acc.latencies.push(trace.latency_ms);
        if acc.latencies.len() > 10_000 { acc.latencies.drain(0..1000); }
        acc.last_active_at = Some(trace.completed_at);
        drop(acc);

        {
            let cutoff_5m = now - chrono::Duration::minutes(5);
            let cutoff_1h = now - chrono::Duration::hours(1);
            let mut lat = self.recent_latencies.write().await;
            lat.push((now, trace.latency_ms));
            lat.retain(|(ts, _)| *ts >= cutoff_5m);
            let mut errs = self.recent_errors.write().await;
            errs.push((now, is_error));
            errs.retain(|(ts, _)| *ts >= cutoff_5m);
            let mut toks = self.recent_tokens.write().await;
            toks.push((now, total_tokens));
            toks.retain(|(ts, _)| *ts >= cutoff_1h);
            let mut costs = self.recent_costs.write().await;
            costs.push((now, trace.cost_usd));
            costs.retain(|(ts, _)| *ts >= cutoff_1h);
        }

        let meter = global::meter("maestro-observability");
        let agent_label = KeyValue::new("agent_id", trace.agent_id.clone());
        let model_label = KeyValue::new("model", trace.model_used.clone());
        meter.u64_counter("agent.traces.total").build().add(1, &[agent_label.clone(), model_label.clone()]);
        meter.u64_counter("agent.tokens.input").build().add(trace.input_tokens, &[agent_label.clone(), model_label.clone()]);
        meter.u64_counter("agent.tokens.output").build().add(trace.output_tokens, &[agent_label.clone(), model_label.clone()]);
        meter.f64_counter("agent.cost.usd").build().add(trace.cost_usd, &[agent_label.clone(), model_label.clone()]);
        meter.u64_histogram("agent.latency.ms").build().record(trace.latency_ms, &[agent_label.clone(), model_label.clone()]);
        if is_error {
            meter.u64_counter("agent.errors.total").build().add(1, &[agent_label, model_label]);
        }
        debug!(agent_id = %trace.agent_id, latency_ms = trace.latency_ms, tokens = total_tokens, "Metrics ingested");
    }

    pub fn agent_metrics(&self, agent_id: &str) -> Option<AgentMetrics> {
        let acc = self.agents.get(agent_id)?;
        let mut latencies = acc.latencies.clone();
        latencies.sort_unstable();
        let p99 = if latencies.is_empty() { 0 } else {
            let idx = (latencies.len() as f64 * 0.99) as usize;
            latencies[idx.min(latencies.len() - 1)]
        };
        let avg = if latencies.is_empty() { 0.0 } else {
            latencies.iter().sum::<u64>() as f64 / latencies.len() as f64
        };
        Some(AgentMetrics {
            agent_id: agent_id.to_string(),
            total_traces: acc.total_traces,
            error_count: acc.error_count,
            timeout_count: acc.timeout_count,
            total_input_tokens: acc.total_input_tokens,
            total_output_tokens: acc.total_output_tokens,
            total_cost_usd: acc.total_cost_usd,
            avg_latency_ms: avg,
            p99_latency_ms: p99,
            last_active_at: acc.last_active_at,
        })
    }

    pub fn all_agent_metrics(&self) -> Vec<AgentMetrics> {
        self.agents.iter().filter_map(|e| self.agent_metrics(e.key())).collect()
    }

    pub async fn system_metrics(&self) -> SystemMetrics {
        let now = Utc::now();
        let total_traces: u64 = self.agents.iter().map(|e| e.total_traces).sum();
        let active_agents = self.agents.len() as u64;
        let total_tokens_1h: u64 = self.recent_tokens.read().await.iter().map(|(_, t)| t).sum();
        let total_cost_1h_usd: f64 = self.recent_costs.read().await.iter().map(|(_, c)| c).sum();
        let errs = self.recent_errors.read().await;
        let error_rate_5m = if errs.is_empty() { 0.0 } else {
            errs.iter().filter(|(_, e)| *e).count() as f64 / errs.len() as f64
        };
        drop(errs);
        let mut lats: Vec<u64> = self.recent_latencies.read().await.iter().map(|(_, l)| *l).collect();
        lats.sort_unstable();
        let p99_latency_ms_5m = if lats.is_empty() { 0 } else {
            let idx = (lats.len() as f64 * 0.99) as usize;
            lats[idx.min(lats.len() - 1)]
        };
        SystemMetrics { total_traces, active_agents, total_tokens_1h, total_cost_1h_usd, error_rate_5m, p99_latency_ms_5m, snapshot_at: now }
    }

    pub fn reset_agent(&self, agent_id: &str) { self.agents.remove(agent_id); }
}

impl Default for MetricsStore {
    fn default() -> Self { Self::new() }
}

/// Initialize the OpenTelemetry metrics provider with OTLP export.
pub fn init_metrics(otlp_endpoint: Option<&str>) -> crate::Result<()> {
    if let Some(endpoint) = otlp_endpoint {
        use opentelemetry_otlp::WithExportConfig;
        let exporter = opentelemetry_otlp::MetricExporter::builder()
            .with_http()
            .with_endpoint(endpoint)
            .build()
            .map_err(|e: opentelemetry_sdk::metrics::MetricError| crate::ObsError::Otel(e.to_string()))?;
        let provider = opentelemetry_sdk::metrics::SdkMeterProvider::builder()
            .with_periodic_exporter(exporter)
            .build();
        global::set_meter_provider(provider);
        tracing::info!(endpoint = %endpoint, "OpenTelemetry OTLP metrics initialized");
    }
    Ok(())
}
