//! Usage tracking data types.

use crate::agent::AgentId;
use serde::{Deserialize, Serialize};

/// A single usage event recording an LLM call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    /// Which agent made the call.
    pub agent_id: AgentId,
    /// Model used.
    pub model: String,
    /// Input tokens consumed.
    pub input_tokens: u64,
    /// Output tokens consumed.
    pub output_tokens: u64,
    /// Estimated cost in USD.
    pub cost_usd: f64,
    /// Number of tool calls in this interaction.
    pub tool_calls: u32,
}

/// Summary of usage over a period.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageSummary {
    /// Total input tokens.
    pub total_input_tokens: u64,
    /// Total output tokens.
    pub total_output_tokens: u64,
    /// Total estimated cost in USD.
    pub total_cost_usd: f64,
    /// Total number of calls.
    pub call_count: u64,
    /// Total tool calls.
    pub total_tool_calls: u64,
}

/// Usage grouped by model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelUsage {
    /// Model name.
    pub model: String,
    /// Total cost for this model.
    pub total_cost_usd: f64,
    /// Total input tokens.
    pub total_input_tokens: u64,
    /// Total output tokens.
    pub total_output_tokens: u64,
    /// Number of calls.
    pub call_count: u64,
}

/// Daily usage breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyBreakdown {
    /// Date string (YYYY-MM-DD).
    pub date: String,
    /// Total cost for this day.
    pub cost_usd: f64,
    /// Total tokens (input + output).
    pub tokens: u64,
    /// Number of API calls.
    pub calls: u64,
}
