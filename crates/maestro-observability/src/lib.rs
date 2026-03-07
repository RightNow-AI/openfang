//! # maestro-observability
//!
//! Observability suite inspired by Kore.ai's analytics and Maestro's
//! predictive analytics engine.
//!
//! ## What Kore.ai Has (from docs.kore.ai)
//!
//! - **Traces** with parent/child spans for every agent interaction
//! - **Sessions** grouping related traces into conversations
//! - **Dashboard** with real-time metrics (latency, tokens, cost, errors)
//! - **Alerts** with configurable rules and notification channels
//! - **Audit Logs** for compliance and security
//! - **Export** to external systems (OTLP, webhooks)
//!
//! ## What OpenFang Has
//!
//! - `tracing` crate integration for structured logging
//! - Basic metrics in the kernel (agent count, message count)
//! - NO: cost tracking, session analytics, alert rules, audit logs
//!
//! ## What Maestro Had (worth porting)
//!
//! - Predictive analytics engine (~800 LOC) with real statistical modeling
//! - Token usage tracking per model per phase
//! - Cost estimation based on model pricing
//!
//! ## HONEST GAPS
//!
//! - No dashboard UI (this crate provides the data layer only)
//! - Alert rules are evaluated in-process (no external alerting service)
//! - Audit log storage is local SQLite (not suitable for compliance at scale)
//! - Cost tracking depends on accurate pricing data in the model hub
//! - No anomaly detection (the predictive analytics is statistical, not ML)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod alerts;
pub mod audit;
pub mod cost;
pub mod metrics;
pub mod traces;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Trigger when error rate exceeds threshold in window.
    ErrorRate { threshold: f64, window_secs: u64 },
    /// Trigger when latency exceeds threshold.
    LatencyExceeded { threshold_ms: u64 },
    /// Trigger when cost exceeds budget in period.
    CostExceeded { budget_usd: f64, period_secs: u64 },
    /// Trigger when token usage exceeds limit.
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
