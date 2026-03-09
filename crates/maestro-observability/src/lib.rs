//! # maestro-observability
//!
//! Observability suite for the OpenFang/Maestro platform.
//! Provides OpenTelemetry-based tracing, real-time metrics, cost tracking,
//! alert rules, and an append-only audit log.
//!
//! ## Architecture
//!
//! ```text
//! ObservabilityEngine
//!   ├── TraceStore      — in-memory ring buffer of completed traces
//!   ├── MetricsStore    — per-agent and system-wide metric aggregation
//!   ├── CostTracker     — model pricing catalog and cost calculation
//!   ├── AlertEngine     — rule evaluation and notification dispatch
//!   └── AuditLog        — append-only compliance log
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

pub mod alerts;
pub mod audit;
pub mod cost;
pub mod metrics;
pub mod traces;

pub use alerts::{AlertEngine, AlertEvent};
pub use audit::AuditLog;
pub use cost::{CostTracker, ModelPricing, ModelTier};
pub use metrics::{AgentMetrics, MetricsStore, SystemMetrics};
pub use traces::{TraceBuilder, TraceStore};

/// Observability errors.
#[derive(Debug, Error)]
pub enum ObsError {
    #[error("OpenTelemetry error: {0}")]
    Otel(String),
    #[error("Alert rule not found: {0}")]
    RuleNotFound(String),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

pub type Result<T> = std::result::Result<T, ObsError>;

/// A trace representing a single agent interaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    pub trace_id: Uuid,
    pub parent_trace_id: Option<Uuid>,
    pub session_id: Uuid,
    pub agent_id: String,
    pub model_used: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub latency_ms: u64,
    pub cost_usd: f64,
    pub status: TraceStatus,
    pub metadata: serde_json::Value,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TraceStatus {
    Success,
    Error,
    Timeout,
    Cancelled,
}

/// A session grouping related traces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub session_id: Uuid,
    pub user_id: Option<String>,
    pub trace_count: u64,
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    pub started_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
}

/// An alert rule definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    pub id: String,
    pub name: String,
    pub condition: AlertCondition,
    pub action: AlertAction,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertCondition {
    ErrorRate { threshold: f64, window_secs: u64 },
    LatencyExceeded { threshold_ms: u64 },
    CostExceeded { budget_usd: f64, period_secs: u64 },
    TokenLimit { limit: u64, period_secs: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertAction {
    Log,
    Webhook { url: String },
    ThrottleAgent { agent_id: String },
    PauseAgent { agent_id: String },
}

/// An audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub actor: String,
    pub action: String,
    pub resource: String,
    pub details: serde_json::Value,
    pub ip_address: Option<String>,
}

/// Configuration for the observability engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObsConfig {
    /// OTLP endpoint for trace/metric export (e.g., "http://localhost:4317").
    /// If None, runs in local-only mode.
    pub otlp_endpoint: Option<String>,
    /// Alert evaluation interval in seconds.
    pub alert_interval_secs: u64,
    /// Alert cooldown period in seconds.
    pub alert_cooldown_secs: u64,
    /// Maximum traces to retain in memory.
    pub max_traces: usize,
    /// Maximum audit log entries to retain.
    pub max_audit_entries: usize,
}

impl Default for ObsConfig {
    fn default() -> Self {
        Self {
            otlp_endpoint: None,
            alert_interval_secs: 60,
            alert_cooldown_secs: 300,
            max_traces: 10_000,
            max_audit_entries: 50_000,
        }
    }
}

/// The top-level observability engine — the single entry point for all
/// observability operations.
#[derive(Clone)]
pub struct ObservabilityEngine {
    pub traces: TraceStore,
    pub metrics: MetricsStore,
    pub costs: CostTracker,
    pub alerts: Arc<AlertEngine>,
    pub audit: AuditLog,
    config: ObsConfig,
}

impl ObservabilityEngine {
    /// Create and initialize a new observability engine.
    pub fn new(config: ObsConfig) -> Result<Self> {
        // Initialize OpenTelemetry providers
        traces::init_tracer(config.otlp_endpoint.as_deref())?;
        metrics::init_metrics(config.otlp_endpoint.as_deref())?;

        Ok(Self {
            traces: TraceStore::new(),
            metrics: MetricsStore::new(),
            costs: CostTracker::new(),
            alerts: Arc::new(AlertEngine::new(config.alert_cooldown_secs)),
            audit: AuditLog::new(config.max_audit_entries),
            config,
        })
    }

    /// Create with default config (no OTLP export).
    pub fn default_local() -> Self {
        Self::new(ObsConfig::default()).expect("Default ObsConfig should never fail")
    }

    /// Record a completed trace, updating all downstream stores.
    pub async fn record_trace(&self, trace: Trace) {
        // Calculate cost if not already set
        let trace = if trace.cost_usd == 0.0 {
            let cost = self.costs.calculate(
                &trace.model_used,
                trace.input_tokens,
                trace.output_tokens,
            );
            Trace { cost_usd: cost, ..trace }
        } else {
            trace
        };

        // Ingest into metrics
        self.metrics.ingest(&trace).await;
        // Store trace
        self.traces.record(trace).await;
    }

    /// Start a new trace builder for an agent interaction.
    pub fn start_trace(
        &self,
        agent_id: &str,
        model: &str,
        session_id: Uuid,
    ) -> TraceBuilder {
        TraceBuilder::start(agent_id, model, session_id, self.traces.clone())
    }

    /// Start the background alert evaluation loop.
    pub fn start_alert_loop(&self) -> tokio::task::JoinHandle<()> {
        Arc::clone(&self.alerts)
            .start_evaluation_loop(self.traces.clone(), self.config.alert_interval_secs)
    }

    /// Shut down OpenTelemetry providers, flushing pending data.
    pub fn shutdown(&self) {
        traces::shutdown_tracer();
    }
}
