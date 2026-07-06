//! Graceful shutdown — ordered subsystem teardown for clean exit.
//!
//! When OMTAE receives a shutdown signal (SIGTERM, Ctrl+C, API call), this
//! module orchestrates an ordered shutdown sequence to prevent data loss and
//! ensure clean resource cleanup.
//!
//! Shutdown sequence (order matters):
//! 1. Stop accepting new requests (mark as draining)
//! 2. Broadcast shutdown to WebSocket clients
//! 3. Wait for in-flight agent loops to complete (with timeout)
//! 4. Close browser sessions
//! 5. Stop MCP connections
//! 6. Stop heartbeat/background tasks
//! 7. Flush audit log
//! 8. Close database connections
//! 9. Exit

use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// Shutdown phase identifiers (in execution order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[repr(u8)]
pub enum ShutdownPhase {
    Running = 0,
    Draining = 1,
    BroadcastingShutdown = 2,
    WaitingForAgents = 3,
    ClosingBrowsers = 4,
    ClosingMcp = 5,
    StoppingBackground = 6,
    FlushingAudit = 7,
    ClosingDatabase = 8,
    Complete = 9,
}

impl std::fmt::Display for ShutdownPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Running => write!(f, "running"),
            Self::Draining => write!(f, "draining"),
            Self::BroadcastingShutdown => write!(f, "broadcasting_shutdown"),
            Self::WaitingForAgents => write!(f, "waiting_for_agents"),
            Self::ClosingBrowsers => write!(f, "closing_browsers"),
            Self::ClosingMcp => write!(f, "closing_mcp"),
            Self::StoppingBackground => write!(f, "stopping_background"),
            Self::FlushingAudit => write!(f, "flushing_audit"),
            Self::ClosingDatabase => write!(f, "closing_database"),
            Self::Complete => write!(f, "complete"),
        }
    }
}

/// Configuration for graceful shutdown.
#[derive(Debug, Clone)]
pub struct ShutdownConfig {
    /// Maximum time to wait for in-flight requests to complete.
    pub drain_timeout: Duration,
    /// Maximum time to wait for agent loops to finish.
    pub agent_timeout: Duration,
    /// Maximum time for the entire shutdown sequence.
    pub total_timeout: Duration,
    /// Whether to broadcast a shutdown message to WS clients.
    pub broadcast_shutdown: bool,
    /// Human-readable reason for shutdown (included in WS broadcast).
    pub shutdown_reason: String,
}

impl Default for ShutdownConfig {
    fn default() -> Self {
        Self {
            drain_timeout: Duration::from_secs(30),
            agent_timeout: Duration::from_secs(60),
            total_timeout: Duration::from_secs(120),
            broadcast_shutdown: true,
            shutdown_reason: "System shutdown".to_string(),
        }
    }
}

/// Tracks the state of a graceful shutdown in progress.
pub struct ShutdownCoordinator {
    /// Whether shutdown has been initiated.
    is_shutting_down: AtomicBool,
    /// Current shutdown phase.
    current_phase: AtomicU8,
    /// When shutdown was initiated.
    started_at: std::sync::Mutex<Option<Instant>>,
    /// Configuration.
    config: ShutdownConfig,
    /// Log of completed phases with timing.
    phase_log: std::sync::Mutex<Vec<PhaseLog>>,
}

/// Log entry for a completed shutdown phase.
#[derive(Debug, Clone, Serialize)]
pub struct PhaseLog {
    pub phase: ShutdownPhase,
    pub duration_ms: u64,
    pub success: bool,
    pub message: Option<String>,
}

/// Shutdown progress snapshot (for API responses / WS broadcast).
#[derive(Debug, Clone, Serialize)]
pub struct ShutdownStatus {
    pub is_shutting_down: bool,
    pub current_phase: String,
    pub elapsed_secs: f64,
    pub reason: String,
    pub phases_completed: Vec<PhaseLog>,
}

impl ShutdownCoordinator {
    /// Create a new shutdown coordinator.
    pub fn new(config: ShutdownConfig) -> Self {
        Self {
            is_shutting_down: AtomicBool::new(false),
            current_phase: AtomicU8::new(ShutdownPhase::Running as u8),
            started_at: std::sync::Mutex::new(None),
            config,
            phase_log: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Check if shutdown is in progress.
    pub fn is_shutting_down(&self) -> bool {
        self.is_shutting_down.load(Ordering::Relaxed)
    }

    /// Initiate shutdown. Returns `false` if already shutting down.
    pub fn initiate(&self) -> bool {
        if self.is_shutting_down.swap(true, Ordering::SeqCst) {
            return false; // Already shutting down.
        }
        *self.started_at.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
        info!(reason = %self.config.shutdown_reason, "Graceful shutdown initiated");
        true
    }

    /// Get the current shutdown phase.
    pub fn current_phase(&self) -> ShutdownPhase {
        let val = self.current_phase.load(Ordering::Relaxed);
        match val {
            0 => ShutdownPhase::Running,
            1 => ShutdownPhase::Draining,
            2 => ShutdownPhase::BroadcastingShutdown,
            3 => ShutdownPhase::WaitingForAgents,
            4 => ShutdownPhase::ClosingBrowsers,
            5 => ShutdownPhase::ClosingMcp,
            6 => ShutdownPhase::StoppingBackground,
            7 => ShutdownPhase::FlushingAudit,
            8 => ShutdownPhase::ClosingDatabase,
            _ => ShutdownPhase::Complete,
        }
    }

    /// Advance to the next phase. Records timing for the completed phase.
    pub fn advance_phase(&self, next: ShutdownPhase, success: bool, message: Option<String>) {
        let current = self.current_phase();
        let elapsed = self
            .started_at
            .lock()
            .unwrap()
            .map(|s| s.elapsed().as_millis() as u64)
            .unwrap_or(0);

        let log = PhaseLog {
            phase: current,
            duration_ms: elapsed,
            success,
            message: message.clone(),
        };

        self.phase_log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(log);
        self.current_phase.store(next as u8, Ordering::SeqCst);

        if success {
            info!(phase = %current, next = %next, elapsed_ms = elapsed, "Shutdown phase complete");
        } else {
            warn!(phase = %current, next = %next, error = ?message, "Shutdown phase failed, continuing");
        }
    }

    /// Get a snapshot of shutdown status (for API/WS).
    pub fn status(&self) -> ShutdownStatus {
        let elapsed = self
            .started_at
            .lock()
            .unwrap()
            .map(|s| s.elapsed().as_secs_f64())
            .unwrap_or(0.0);

        ShutdownStatus {
            is_shutting_down: self.is_shutting_down(),
            current_phase: self.current_phase().to_string(),
            elapsed_secs: elapsed,
            reason: self.config.shutdown_reason.clone(),
            phases_completed: self
                .phase_log
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
        }
    }

    /// Check if the total timeout has been exceeded.
    pub fn is_timeout_exceeded(&self) -> bool {
        self.started_at
            .lock()
            .unwrap()
            .map(|s| s.elapsed() > self.config.total_timeout)
            .unwrap_or(false)
    }

    /// Get the drain timeout duration.
    pub fn drain_timeout(&self) -> Duration {
        self.config.drain_timeout
    }

    /// Get the agent timeout duration.
    pub fn agent_timeout(&self) -> Duration {
        self.config.agent_timeout
    }

    /// Whether to broadcast shutdown to WS clients.
    pub fn should_broadcast(&self) -> bool {
        self.config.broadcast_shutdown
    }

    /// Get the shutdown reason for WS broadcast.
    pub fn shutdown_reason(&self) -> &str {
        &self.config.shutdown_reason
    }

    /// Build a WS-compatible shutdown message (JSON).
    pub fn ws_shutdown_message(&self) -> String {
        let status = self.status();
        serde_json::json!({
            "type": "shutdown",
            "reason": status.reason,
            "phase": status.current_phase,
        })
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shutdown_config_defaults() {
        let config = ShutdownConfig::default();
        assert_eq!(config.drain_timeout, Duration::from_secs(30));
        assert_eq!(config.agent_timeout, Duration::from_secs(60));
        assert_eq!(config.total_timeout, Duration::from_secs(120));
        assert!(config.broadcast_shutdown);
        assert_eq!(config.shutdown_reason, "System shutdown");
    }

    #[test]
    fn test_coordinator_not_shutting_down_initially() {
        let coord = ShutdownCoordinator::new(ShutdownConfig::default());
        assert!(!coord.is_shutting_down());
        assert_eq!(coord.current_phase(), ShutdownPhase::Running);
    }

    #[test]
    fn test_initiate_shutdown() {
        let coord = ShutdownCoordinator::new(ShutdownConfig::default());
        assert!(coord.initiate());
        assert!(coord.is_shutting_down());
    }

    #[test]
    fn test_double_initiate_returns_false() {
        let coord = ShutdownCoordinator::new(ShutdownConfig::default());
        assert!(coord.initiate());
        assert!(!coord.initiate()); // Second call returns false.
        assert!(coord.is_shutting_down()); // Still shutting down.
    }

    #[test]
    fn test_phase_advancement() {
        let coord = ShutdownCoordinator::new(ShutdownConfig::default());
        coord.initiate();
        assert_eq!(coord.current_phase(), ShutdownPhase::Running);

        coord.advance_phase(ShutdownPhase::Draining, true, None);
        assert_eq!(coord.current_phase(), ShutdownPhase::Draining);

        coord.advance_phase(ShutdownPhase::BroadcastingShutdown, true, None);
        assert_eq!(coord.current_phase(), ShutdownPhase::BroadcastingShutdown);

        coord.advance_phase(ShutdownPhase::WaitingForAgents, true, None);
        assert_eq!(coord.current_phase(), ShutdownPhase::WaitingForAgents);

        coord.advance_phase(ShutdownPhase::Complete, true, None);
        assert_eq!(coord.current_phase(), ShutdownPhase::Complete);
    }

    #[test]
    fn test_phase_display_names() {
        assert_eq!(ShutdownPhase::Running.to_string(), "running");
        assert_eq!(ShutdownPhase::Draining.to_string(), "draining");
        assert_eq!(
            ShutdownPhase::BroadcastingShutdown.to_string(),
            "broadcasting_shutdown"
        );
        assert_eq!(
            ShutdownPhase::WaitingForAgents.to_string(),
            "waiting_for_agents"
        );
        assert_eq!(
            ShutdownPhase::ClosingBrowsers.to_string(),
            "closing_browsers"
        );
        assert_eq!(ShutdownPhase::ClosingMcp.to_string(), "closing_mcp");
        assert_eq!(
            ShutdownPhase::StoppingBackground.to_string(),
            "stopping_background"
        );
        assert_eq!(ShutdownPhase::FlushingAudit.to_string(), "flushing_audit");
        assert_eq!(
            ShutdownPhase::ClosingDatabase.to_string(),
            "closing_database"
        );
        assert_eq!(ShutdownPhase::Complete.to_string(), "complete");
    }

    #[test]
    fn test_status_snapshot() {
        let coord = ShutdownCoordinator::new(ShutdownConfig::default());
        let status = coord.status();

        assert!(!status.is_shutting_down);
        assert_eq!(status.current_phase, "running");
        assert_eq!(status.reason, "System shutdown");
        assert!(status.phases_completed.is_empty());
    }

    #[test]
    fn test_timeout_check() {
        let config = ShutdownConfig {
            total_timeout: Duration::from_millis(1), // Very short timeout.
            ..Default::default()
        };
        let coord = ShutdownCoordinator::new(config);

        // Not started yet — no timeout.
        assert!(!coord.is_timeout_exceeded());

        coord.initiate();
        // Sleep briefly to let the 1ms timeout expire.
        std::thread::sleep(Duration::from_millis(10));
        assert!(coord.is_timeout_exceeded());
    }

    #[test]
    fn test_ws_shutdown_message() {
        let coord = ShutdownCoordinator::new(ShutdownConfig::default());
        coord.initiate();
        let msg = coord.ws_shutdown_message();

        let parsed: serde_json::Value = serde_json::from_str(&msg).expect("valid JSON");
        assert_eq!(parsed["type"], "shutdown");
        assert_eq!(parsed["reason"], "System shutdown");
        assert_eq!(parsed["phase"], "running");
    }

    #[test]
    fn test_shutdown_reason() {
        let config = ShutdownConfig {
            shutdown_reason: "Maintenance window".to_string(),
            ..Default::default()
        };
        let coord = ShutdownCoordinator::new(config);
        assert_eq!(coord.shutdown_reason(), "Maintenance window");
    }

    #[test]
    fn test_phase_log_recording() {
        let coord = ShutdownCoordinator::new(ShutdownConfig::default());
        coord.initiate();

        coord.advance_phase(ShutdownPhase::Draining, true, None);
        coord.advance_phase(
            ShutdownPhase::BroadcastingShutdown,
            false,
            Some("WS broadcast failed".to_string()),
        );

        let status = coord.status();
        assert_eq!(status.phases_completed.len(), 2);

        assert_eq!(status.phases_completed[0].phase, ShutdownPhase::Running);
        assert!(status.phases_completed[0].success);
        assert!(status.phases_completed[0].message.is_none());

        assert_eq!(status.phases_completed[1].phase, ShutdownPhase::Draining);
        assert!(!status.phases_completed[1].success);
        assert_eq!(
            status.phases_completed[1].message.as_deref(),
            Some("WS broadcast failed")
        );
    }

    #[test]
    fn test_all_phases_ordered() {
        // Verify repr(u8) values are strictly ascending.
        let phases = [
            ShutdownPhase::Running,
            ShutdownPhase::Draining,
            ShutdownPhase::BroadcastingShutdown,
            ShutdownPhase::WaitingForAgents,
            ShutdownPhase::ClosingBrowsers,
            ShutdownPhase::ClosingMcp,
            ShutdownPhase::StoppingBackground,
            ShutdownPhase::FlushingAudit,
            ShutdownPhase::ClosingDatabase,
            ShutdownPhase::Complete,
        ];

        for i in 1..phases.len() {
            assert!(
                phases[i] > phases[i - 1],
                "{:?} should be > {:?}",
                phases[i],
                phases[i - 1]
            );
        }

        // Verify count.
        assert_eq!(phases.len(), 10);
    }
}
