//! Heartbeat monitor — detects unresponsive agents for 24/7 autonomous operation.
//!
//! The heartbeat monitor runs as a background tokio task, periodically checking
//! each autonomous running agent's `last_active` timestamp. If an autonomous
//! agent hasn't been active for longer than 2x its heartbeat interval, a
//! `HealthCheckFailed` event is published to the event bus.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAction {
    None,
    Cooldown,
    Attempt { attempt: u32 },
    Terminate { attempts: u32 },
}

pub fn recovery_action(
    tracker: &RecoveryTracker,
    agent_id: AgentId,
    config: &HeartbeatConfig,
) -> RecoveryAction {
    let failures = tracker.failure_count(agent_id);
    if failures >= config.max_recovery_attempts {
        return RecoveryAction::Terminate { attempts: failures };
    }
    if !tracker.can_attempt(agent_id, config.recovery_cooldown_secs) {
        return RecoveryAction::Cooldown;
    }
    let attempt = tracker.record_attempt(agent_id);
    if attempt > config.max_recovery_attempts {
        RecoveryAction::Terminate { attempts: attempt }
    } else {
        RecoveryAction::Attempt { attempt }
    }
}

/// Grace period (seconds): if an agent's `last_active` is within this window
/// of `created_at`, it has never genuinely processed a message and should not
/// be flagged as unresponsive.  This covers the small gap between registration
/// and the initial `set_state(Running)` call.
const IDLE_GRACE_SECS: i64 = 10;

/// Check all autonomous running agents and crashed agents and return heartbeat status.
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

        // Crashed agents are always considered unresponsive.
        // For Running agents, heartbeat liveness is only enforced for autonomous agents.
        // Non-autonomous chat/hands can stay idle for long periods and should not be
        // treated as unresponsive noise.
        let timeout_secs = entry_ref
            .manifest
            .autonomous
            .as_ref()
            .map(|a| a.heartbeat_interval_secs * UNRESPONSIVE_MULTIPLIER)
            .unwrap_or(config.default_timeout_secs) as i64;
        let unresponsive = if entry_ref.state == AgentState::Crashed {
            true
        } else if entry_ref.manifest.autonomous.is_some() {
            let never_active =
                (entry_ref.last_active - entry_ref.created_at).num_seconds() <= IDLE_GRACE_SECS;

            if never_active {
                debug!(
                    agent = %entry_ref.name,
                    inactive_secs,
                    "Skipping idle autonomous agent that has never processed a message"
                );
                continue;
            }

            inactive_secs > timeout_secs
        } else {
            false
        };

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
    use chrono::Duration;
    use openfang_types::agent::*;
    use std::collections::HashMap;

    /// Helper: build a minimal AgentEntry for heartbeat tests.
    fn make_entry(
        name: &str,
        state: AgentState,
        created_at: chrono::DateTime<Utc>,
        last_active: chrono::DateTime<Utc>,
    ) -> AgentEntry {
        AgentEntry {
            id: AgentId::new(),
            name: name.to_string(),
            manifest: AgentManifest {
                name: name.to_string(),
                version: "0.1.0".to_string(),
                description: "test".to_string(),
                author: "test".to_string(),
                module: "test".to_string(),
                schedule: ScheduleMode::default(),
                model: ModelConfig::default(),
                fallback_models: vec![],
                resources: ResourceQuota::default(),
                priority: Priority::default(),
                capabilities: ManifestCapabilities::default(),
                profile: None,
                tools: HashMap::new(),
                skills: vec![],
                mcp_servers: vec![],
                metadata: HashMap::new(),
                tags: vec![],
                routing: None,
                autonomous: None,
                pinned_model: None,
                workspace: None,
                generate_identity_files: true,
                exec_policy: None,
                tool_allowlist: vec![],
                tool_blocklist: vec![],
            },
            state,
            mode: AgentMode::default(),
            created_at,
            last_active,
            parent: None,
            children: vec![],
            session_id: SessionId::new(),
            tags: vec![],
            identity: Default::default(),
            onboarding_completed: false,
            onboarding_completed_at: None,
        }
    }

    #[test]
    fn test_idle_agent_skipped_by_heartbeat() {
        // An agent spawned 5 minutes ago that has never processed a message
        // (last_active == created_at). It should NOT appear in heartbeat
        // statuses because it was never genuinely active.
        let registry = crate::registry::AgentRegistry::new();
        let five_min_ago = Utc::now() - Duration::seconds(300);
        let mut idle_agent = make_entry(
            "idle-agent",
            AgentState::Running,
            five_min_ago,
            five_min_ago,
        );
        idle_agent.manifest.autonomous = Some(openfang_types::agent::AutonomousConfig::default());
        registry.register(idle_agent).unwrap();

        let config = HeartbeatConfig::default(); // timeout = 180s
        let statuses = check_agents(&registry, &config);

        // The idle agent should be skipped entirely
        assert!(
            statuses.is_empty(),
            "idle agent should be skipped by heartbeat"
        );
    }

    #[test]
    fn test_active_agent_detected_unresponsive() {
        // An agent that WAS active (last_active >> created_at) but has gone
        // silent for longer than the timeout — should be flagged unresponsive.
        let registry = crate::registry::AgentRegistry::new();
        let ten_min_ago = Utc::now() - Duration::seconds(600);
        let five_min_ago = Utc::now() - Duration::seconds(300);
        let mut active_agent = make_entry(
            "active-agent",
            AgentState::Running,
            ten_min_ago,
            five_min_ago,
        );
        active_agent.manifest.autonomous =
            Some(openfang_types::agent::AutonomousConfig::default());
        registry.register(active_agent).unwrap();

        let config = HeartbeatConfig::default(); // timeout = 180s, inactive = ~300s
        let statuses = check_agents(&registry, &config);

        assert_eq!(statuses.len(), 1);
        assert!(
            statuses[0].unresponsive,
            "active agent past timeout should be unresponsive"
        );
    }

    #[test]
    fn test_active_agent_within_timeout_is_ok() {
        // An agent that has been active recently (within timeout).
        let registry = crate::registry::AgentRegistry::new();
        let ten_min_ago = Utc::now() - Duration::seconds(600);
        let just_now = Utc::now() - Duration::seconds(10);
        let healthy_agent = make_entry("healthy-agent", AgentState::Running, ten_min_ago, just_now);
        registry.register(healthy_agent).unwrap();

        let config = HeartbeatConfig::default(); // timeout = 180s
        let statuses = check_agents(&registry, &config);

        assert_eq!(statuses.len(), 1);
        assert!(
            !statuses[0].unresponsive,
            "recently active agent should not be unresponsive"
        );
    }

    #[test]
    fn test_crashed_agent_not_skipped_even_if_idle() {
        // A crashed agent should still appear in statuses for recovery,
        // even if it was never genuinely active.
        let registry = crate::registry::AgentRegistry::new();
        let five_min_ago = Utc::now() - Duration::seconds(300);
        let crashed_agent = make_entry(
            "crashed-idle",
            AgentState::Crashed,
            five_min_ago,
            five_min_ago,
        );
        registry.register(crashed_agent).unwrap();

        let config = HeartbeatConfig::default();
        let statuses = check_agents(&registry, &config);

        assert_eq!(statuses.len(), 1);
        assert!(
            statuses[0].unresponsive,
            "crashed agent should be marked unresponsive"
        );
    }

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
    fn test_non_autonomous_running_agent_is_not_marked_unresponsive() {
        let reg = AgentRegistry::new();
        let manifest = openfang_types::agent::AgentManifest {
            name: "manual-chat-agent".to_string(),
            autonomous: None,
            ..Default::default()
        };

        let mut entry = openfang_types::agent::AgentEntry {
            id: openfang_types::agent::AgentId::new(),
            name: manifest.name.clone(),
            manifest,
            state: AgentState::Suspended,
            mode: openfang_types::agent::AgentMode::Full,
            created_at: Utc::now(),
            last_active: Utc::now(),
            parent: None,
            children: Vec::new(),
            session_id: openfang_types::agent::SessionId::new(),
            tags: Vec::new(),
            identity: Default::default(),
            onboarding_completed: false,
            onboarding_completed_at: None,
        };
        entry.state = AgentState::Running;
        // Simulate long idle.
        entry.last_active = Utc::now() - chrono::Duration::seconds(10_000);
        let agent_id = entry.id;
        reg.register(entry).unwrap();

        let statuses = check_agents(&reg, &HeartbeatConfig::default());
        let status = statuses
            .iter()
            .find(|s| s.agent_id == agent_id)
            .expect("status should exist");
        assert_eq!(status.state, AgentState::Running);
        assert!(!status.unresponsive);
    }

    #[test]
    fn test_never_active_autonomous_agent_is_skipped() {
        let reg = AgentRegistry::new();
        let manifest = openfang_types::agent::AgentManifest {
            name: "idle-autonomous".to_string(),
            autonomous: Some(openfang_types::agent::AutonomousConfig {
                quiet_hours: None,
                max_iterations: 50,
                max_restarts: 10,
                heartbeat_interval_secs: 30,
                heartbeat_channel: None,
            }),
            ..Default::default()
        };

        let created_at = Utc::now() - chrono::Duration::hours(4);
        let entry = openfang_types::agent::AgentEntry {
            id: openfang_types::agent::AgentId::new(),
            name: manifest.name.clone(),
            manifest,
            state: AgentState::Running,
            mode: openfang_types::agent::AgentMode::Full,
            created_at,
            last_active: created_at + chrono::Duration::seconds(1),
            parent: None,
            children: Vec::new(),
            session_id: openfang_types::agent::SessionId::new(),
            tags: Vec::new(),
            identity: Default::default(),
            onboarding_completed: false,
            onboarding_completed_at: None,
        };
        reg.register(entry).unwrap();

        let statuses = check_agents(&reg, &HeartbeatConfig::default());
        assert!(statuses.is_empty());
    }

    #[test]
    fn test_heartbeat_config_custom_timeout() {
        let config = HeartbeatConfig {
            default_timeout_secs: 600,
            ..HeartbeatConfig::default()
        };
        assert_eq!(config.default_timeout_secs, 600);
        assert_eq!(config.check_interval_secs, DEFAULT_CHECK_INTERVAL_SECS);
        assert_eq!(config.max_recovery_attempts, DEFAULT_MAX_RECOVERY_ATTEMPTS);
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

    #[test]
    fn test_recovery_action_attempt_then_cooldown_then_terminate() {
        let tracker = RecoveryTracker::new();
        let agent_id = AgentId::new();
        let config = HeartbeatConfig {
            max_recovery_attempts: 2,
            recovery_cooldown_secs: 60,
            ..HeartbeatConfig::default()
        };

        assert_eq!(
            recovery_action(&tracker, agent_id, &config),
            RecoveryAction::Attempt { attempt: 1 }
        );
        assert_eq!(
            recovery_action(&tracker, agent_id, &config),
            RecoveryAction::Cooldown
        );

        tracker.reset(agent_id);
        assert_eq!(
            recovery_action(&tracker, agent_id, &config),
            RecoveryAction::Attempt { attempt: 1 }
        );
        tracker.reset(agent_id);
        tracker.record_attempt(agent_id);
        tracker.record_attempt(agent_id);
        assert_eq!(
            recovery_action(&tracker, agent_id, &config),
            RecoveryAction::Terminate { attempts: 2 }
        );
    }
}
