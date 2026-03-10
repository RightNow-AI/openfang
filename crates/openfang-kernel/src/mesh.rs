//! Mesh routing types for the Multi-Agent Mesh (Phase 12).
//!
//! The kernel maintains a bounded ring buffer of recent routing decisions
//! so the dashboard can display a live task routing log.

use chrono::{DateTime, Utc};

/// A single entry in the mesh task routing log.
#[derive(Debug, Clone)]
pub struct MeshRouteEntry {
    /// Unique entry ID (monotonically increasing).
    pub id: u64,
    /// Timestamp when the routing decision was made.
    pub ts: DateTime<Utc>,
    /// Short summary of the task that was routed.
    pub task_summary: String,
    /// Human-readable description of where the task was routed.
    /// Examples: "local:agent:abc123", "hand:github-copilot", "peer:node-xyz:agent-456"
    pub target: String,
    /// How long the routing decision took in milliseconds (0 if not yet complete).
    pub duration_ms: u64,
    /// Outcome: "success", "failed", "pending", or "cancelled".
    pub status: String,
}

impl MeshRouteEntry {
    /// Create a new routing log entry.
    pub fn new(id: u64, task_summary: impl Into<String>, target: impl Into<String>) -> Self {
        Self {
            id,
            ts: Utc::now(),
            task_summary: task_summary.into(),
            target: target.into(),
            duration_ms: 0,
            status: "pending".to_string(),
        }
    }
}
