//! Heartbeat monitor — detects unresponsive agents for 24/7 autonomous operation.
//!
//! The heartbeat monitor runs as a background tokio task, periodically checking
//! each running agent's `last_active` timestamp. If an agent hasn't been active
//! for longer than 2x its heartbeat interval, a `HealthCheckFailed` event is
//! published to the event bus.
//!
//! Crashed agents are tracked for auto-recovery: the heartbeat will attempt to
//! reset crashed agents back to Running up to `max_recovery_attempts` times.
//! After exhausting attempts, agents are marked as Terminated (dead).

use crate::registry::AgentRegistry;
use chrono::Utc;
use dashmap::DashMap;
use openfang_types::agent::{AgentId, AgentState};
use tracing::{debug, warn};

/// Default heartbeat check interval (seconds).
const DEFAULT_CHECK_INTERVAL_SECS: u64 = 30;

/// Multiplier: agent is considered unresponsive if inactive for this many
/// multiples of its heartbeat interval.
const UNRESPONSIVE_MULTIPLIER: u64 = 2;

/// Default maximum recovery attempts before giving up.
const DEFAULT_MAX_RECOVERY_ATTEMPTS: u32 = 3;

/// Default cooldown between recovery attempts (seconds).
const DEFAULT_RECOVERY_COOLDOWN_SECS: u64 = 60;

/// Result of a heartbeat check.
#[derive(Debug, Clone)]
pub struct HeartbeatStatus {
    /// Agent ID.
    pub agent_id: AgentId,
    /// Agent name.
    pub name: String,
    /// Seconds since last activity.
    pub inactive_secs: i64,
    /// Whether the agent is considered unresponsive.
    pub unresponsive: bool,
    /// Current agent state.
    pub state: AgentState,
}

/// Heartbeat monitor configuration.
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// How often to run the heartbeat check (seconds).
    pub check_interval_secs: u64,
    /// Default threshold for unresponsiveness (seconds).
    /// Overridden per-agent by AutonomousConfig.heartbeat_interval_secs.
    pub default_timeout_secs: u64,
    /// Maximum recovery attempts before marking agent as Terminated.
    pub max_recovery_attempts: u32,
    /// Minimum seconds between recovery attempts for the same agent.
    pub recovery_cooldown_secs: u64,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: DEFAULT_CHECK_INTERVAL_SECS,
            // 180s default: browser tasks and complex LLM calls can take 1-3 minutes
            default_timeout_secs: 180,
            max_recovery_attempts: DEFAULT_MAX_RECOVERY_ATTEMPTS,
            recovery_cooldown_secs: DEFAULT_RECOVERY_COOLDOWN_SECS,
        }
    }
}

/// Tracks per-agent recovery state across heartbeat cycles.
#[derive(Debug)]
pub struct RecoveryTracker {
    /// Per-agent recovery state: (consecutive_failures, last_attempt_epoch_secs).
    state: DashMap<AgentId, (u32, u64)>,
}

impl RecoveryTracker {
    /// Create a new recovery tracker.
    pub fn new() -> Self {
        Self {
            state: DashMap::new(),
        }
    }

    /// Record a recovery attempt for an agent.
    /// Returns the current attempt number (1-indexed).
    pub fn record_attempt(&self, agent_id: AgentId) -> u32 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut entry = self.state.entry(agent_id).or_insert((0, 0));
        entry.0 += 1;
        entry.1 = now;
        entry.0
    }

    /// Check if enough time has passed since the last recovery attempt.
    pub fn can_attempt(&self, agent_id: AgentId, cooldown_secs: u64) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        match self.state.get(&agent_id) {
            Some(entry) => now.saturating_sub(entry.1) >= cooldown_secs,
            None => true, // No prior attempts
        }
    }

    /// Get the current failure count for an agent.
    pub fn failure_count(&self, agent_id: AgentId) -> u32 {
        self.state.get(&agent_id).map(|e| e.0).unwrap_or(0)
    }

    /// Reset recovery state for an agent (e.g. after successful recovery).
    pub fn reset(&self, agent_id: AgentId) {
        self.state.remove(&agent_id);
    }
}

impl Default for RecoveryTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Check all running and crashed agents and return their heartbeat status.
///
/// This is a pure function — it doesn't start a background task.
/// The caller (kernel) can run this periodically or in a background task.
pub fn check_agents(registry: &AgentRegistry, config: &HeartbeatConfig) -> Vec<HeartbeatStatus> {
    let now = Utc::now();
    let mut statuses = Vec::new();

    for entry_ref in registry.list() {
        // Check Running agents (for unresponsiveness) and Crashed agents (for recovery)
        match entry_ref.state {
            AgentState::Running | AgentState::Crashed => {}
            _ => continue,
        }

        let inactive_secs = (now - entry_ref.last_active).num_seconds();

        // Determine timeout: use agent's autonomous config if set, else default
        let timeout_secs = entry_ref
            .manifest
            .autonomous
            .as_ref()
            .map(|a| a.heartbeat_interval_secs * UNRESPONSIVE_MULTIPLIER)
            .unwrap_or(config.default_timeout_secs) as i64;

        // Crashed agents are always considered unresponsive
        let unresponsive = entry_ref.state == AgentState::Crashed || inactive_secs > timeout_secs;

        if unresponsive && entry_ref.state == AgentState::Running {
            warn!(
                agent = %entry_ref.name,
                inactive_secs,
                timeout_secs,
                "Agent is unresponsive"
            );
        } else if entry_ref.state == AgentState::Crashed {
            warn!(
                agent = %entry_ref.name,
                inactive_secs,
                "Agent is crashed — eligible for recovery"
            );
        } else {
            debug!(
                agent = %entry_ref.name,
                inactive_secs,
                "Agent heartbeat OK"
            );
        }

        statuses.push(HeartbeatStatus {
            agent_id: entry_ref.id,
            name: entry_ref.name.clone(),
            inactive_secs,
            unresponsive,
            state: entry_ref.state,
        });
    }

    statuses
}

/// Check if an agent is currently within its quiet hours.
///
/// Quiet hours format: "HH:MM-HH:MM" (24-hour format, UTC).
/// Returns true if the current time falls within the quiet period.
pub fn is_quiet_hours(quiet_hours: &str) -> bool {
    let parts: Vec<&str> = quiet_hours.split('-').collect();
    if parts.len() != 2 {
        return false;
    }

    let now = Utc::now();
    let current_minutes = now.format("%H").to_string().parse::<u32>().unwrap_or(0) * 60
        + now.format("%M").to_string().parse::<u32>().unwrap_or(0);

    let parse_time = |s: &str| -> Option<u32> {
        let hm: Vec<&str> = s.trim().split(':').collect();
        if hm.len() != 2 {
            return None;
        }
        let h = hm[0].parse::<u32>().ok()?;
        let m = hm[1].parse::<u32>().ok()?;
        if h > 23 || m > 59 {
            return None;
        }
        Some(h * 60 + m)
    };

    let start = match parse_time(parts[0]) {
        Some(v) => v,
        None => return false,
    };
    let end = match parse_time(parts[1]) {
        Some(v) => v,
        None => return false,
    };

    if start <= end {
        // Same-day range: e.g., 22:00-06:00 would be cross-midnight
        // This is start <= current < end
        current_minutes >= start && current_minutes < end
    } else {
        // Cross-midnight: e.g., 22:00-06:00
        current_minutes >= start || current_minutes < end
    }
}

/// Aggregate heartbeat summary.
#[derive(Debug, Clone, Default)]
pub struct HeartbeatSummary {
    /// Total agents checked.
    pub total_checked: usize,
    /// Number of responsive agents.
    pub responsive: usize,
    /// Number of unresponsive agents.
    pub unresponsive: usize,
    /// Details of unresponsive agents.
    pub unresponsive_agents: Vec<HeartbeatStatus>,
}

/// Produce a summary from heartbeat statuses.
pub fn summarize(statuses: &[HeartbeatStatus]) -> HeartbeatSummary {
    let unresponsive_agents: Vec<HeartbeatStatus> = statuses
        .iter()
        .filter(|s| s.unresponsive)
        .cloned()
        .collect();

    HeartbeatSummary {
        total_checked: statuses.len(),
        responsive: statuses.len() - unresponsive_agents.len(),
        unresponsive: unresponsive_agents.len(),
        unresponsive_agents,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quiet_hours_parsing() {
        // We can't easily test time-dependent logic, but we can test format parsing
        assert!(!is_quiet_hours("invalid"));
        assert!(!is_quiet_hours(""));
        assert!(!is_quiet_hours("25:00-06:00")); // Invalid hours handled gracefully
    }

    #[test]
    fn test_quiet_hours_format_valid() {
        // The function returns true/false based on current time
        // We just verify it doesn't panic on valid input
        let _ = is_quiet_hours("22:00-06:00");
        let _ = is_quiet_hours("00:00-23:59");
        let _ = is_quiet_hours("09:00-17:00");
    }

    #[test]
    fn test_heartbeat_config_default() {
        let config = HeartbeatConfig::default();
        assert_eq!(config.check_interval_secs, 30);
        assert_eq!(config.default_timeout_secs, 180);
    }

    #[test]
    fn test_summarize_empty() {
        let summary = summarize(&[]);
        assert_eq!(summary.total_checked, 0);
        assert_eq!(summary.responsive, 0);
        assert_eq!(summary.unresponsive, 0);
    }

    #[test]
    fn test_summarize_mixed() {
        let statuses = vec![
            HeartbeatStatus {
                agent_id: AgentId::new(),
                name: "agent-1".to_string(),
                inactive_secs: 10,
                unresponsive: false,
                state: AgentState::Running,
            },
            HeartbeatStatus {
                agent_id: AgentId::new(),
                name: "agent-2".to_string(),
                inactive_secs: 120,
                unresponsive: true,
                state: AgentState::Running,
            },
            HeartbeatStatus {
                agent_id: AgentId::new(),
                name: "agent-3".to_string(),
                inactive_secs: 5,
                unresponsive: false,
                state: AgentState::Running,
            },
        ];

        let summary = summarize(&statuses);
        assert_eq!(summary.total_checked, 3);
        assert_eq!(summary.responsive, 2);
        assert_eq!(summary.unresponsive, 1);
        assert_eq!(summary.unresponsive_agents.len(), 1);
        assert_eq!(summary.unresponsive_agents[0].name, "agent-2");
    }

    #[test]
    fn test_recovery_tracker() {
        let tracker = RecoveryTracker::new();
        let agent_id = AgentId::new();

        assert_eq!(tracker.failure_count(agent_id), 0);
        assert!(tracker.can_attempt(agent_id, 60));

        let attempt = tracker.record_attempt(agent_id);
        assert_eq!(attempt, 1);
        assert_eq!(tracker.failure_count(agent_id), 1);

        // Just recorded — cooldown should block (unless cooldown is 0)
        assert!(!tracker.can_attempt(agent_id, 60));
        assert!(tracker.can_attempt(agent_id, 0));

        let attempt = tracker.record_attempt(agent_id);
        assert_eq!(attempt, 2);

        tracker.reset(agent_id);
        assert_eq!(tracker.failure_count(agent_id), 0);
    }
}
