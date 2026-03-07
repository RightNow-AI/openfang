//! SurrealDB-backed usage store.

use openfang_types::agent::AgentId;
use openfang_types::error::OpenFangResult;
use openfang_types::usage::{DailyBreakdown, ModelUsage, UsageRecord, UsageSummary};

/// SurrealDB-backed usage store — provides the same interface as the SQLite UsageStore.
#[derive(Clone)]
pub struct SurrealUsageStore {
    // TODO: hold a reference to the SurrealDB connection
}

impl SurrealUsageStore {
    /// Create a new SurrealDB usage store.
    pub fn new() -> Self {
        Self {}
    }

    /// Record a usage event.
    pub fn record(&self, _record: &UsageRecord) -> OpenFangResult<()> {
        Ok(()) // TODO: implement SurrealDB usage recording
    }

    /// Query hourly cost for an agent.
    pub fn query_hourly(&self, _agent_id: AgentId) -> OpenFangResult<f64> {
        Ok(0.0)
    }

    /// Query daily cost for an agent.
    pub fn query_daily(&self, _agent_id: AgentId) -> OpenFangResult<f64> {
        Ok(0.0)
    }

    /// Query monthly cost for an agent.
    pub fn query_monthly(&self, _agent_id: AgentId) -> OpenFangResult<f64> {
        Ok(0.0)
    }

    /// Query global hourly cost.
    pub fn query_global_hourly(&self) -> OpenFangResult<f64> {
        Ok(0.0)
    }

    /// Query global monthly cost.
    pub fn query_global_monthly(&self) -> OpenFangResult<f64> {
        Ok(0.0)
    }

    /// Get a usage summary, optionally filtered by agent.
    pub fn query_summary(&self, _agent_id: Option<AgentId>) -> OpenFangResult<UsageSummary> {
        Ok(UsageSummary::default())
    }

    /// Get usage grouped by model.
    pub fn query_by_model(&self) -> OpenFangResult<Vec<ModelUsage>> {
        Ok(Vec::new())
    }

    /// Get daily usage breakdown for the last N days.
    pub fn query_daily_breakdown(&self, _days: u32) -> OpenFangResult<Vec<DailyBreakdown>> {
        Ok(Vec::new())
    }

    /// Get the date of the first usage event.
    pub fn query_first_event_date(&self) -> OpenFangResult<Option<String>> {
        Ok(None)
    }

    /// Get today's total cost.
    pub fn query_today_cost(&self) -> OpenFangResult<f64> {
        Ok(0.0)
    }

    /// Clean up old usage events.
    pub fn cleanup_old(&self, _days: u32) -> OpenFangResult<usize> {
        Ok(0)
    }
}
