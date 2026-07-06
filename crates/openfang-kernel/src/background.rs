//! Background agent executor — runs agents autonomously on schedules, timers, and conditions.
//!
//! Supports three autonomous modes:
//! - **Continuous**: Agent self-prompts on a fixed interval.
//! - **Periodic**: Agent wakes on a simplified cron schedule (e.g. "every 5m").
//! - **Proactive**: Agent wakes when matching events fire (via the trigger engine).

use crate::triggers::TriggerPattern;
use dashmap::DashMap;
use omtae_types::agent::{AgentId, ScheduleMode};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

/// Maximum number of concurrent background LLM calls across all agents.
const MAX_CONCURRENT_BG_LLM: usize = 5;

/// Manages background task loops for autonomous agents.
pub struct BackgroundExecutor {
    /// Running background task handles, keyed by agent ID.
    tasks: DashMap<AgentId, JoinHandle<()>>,
    /// Agents whose background loop is paused (user clicked Stop or `/stop`).
    paused: Arc<DashMap<AgentId, ()>>,
    /// Shutdown signal receiver (from Supervisor).
    shutdown_rx: watch::Receiver<bool>,
    /// SECURITY: Global semaphore to limit concurrent background LLM calls.
    llm_semaphore: Arc<tokio::sync::Semaphore>,
}

impl BackgroundExecutor {
    /// Create a new executor bound to the supervisor's shutdown signal.
    pub fn new(shutdown_rx: watch::Receiver<bool>) -> Self {
        Self {
            tasks: DashMap::new(),
            paused: Arc::new(DashMap::new()),
            shutdown_rx,
            llm_semaphore: Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_BG_LLM)),
        }
    }

    /// Returns true when continuous/periodic ticks are paused for this agent.
    pub fn is_paused(&self, agent_id: AgentId) -> bool {
        self.paused.contains_key(&agent_id)
    }

    /// Pause background ticks and abort any running loop task.
    pub fn pause_agent(&self, agent_id: AgentId) {
        self.paused.insert(agent_id, ());
        self.stop_agent(agent_id);
        info!(id = %agent_id, "Background loop paused");
    }

    /// Resume background ticks (clears pause flag; caller must restart the loop).
    pub fn resume_agent(&self, agent_id: AgentId) {
        self.paused.remove(&agent_id);
        info!(id = %agent_id, "Background loop pause cleared");
    }

    /// Start a background loop for an agent based on its schedule mode.
    ///
    /// For `Continuous` and `Periodic` modes, spawns a tokio task that
    /// periodically sends a self-prompt message to the agent.
    /// For `Proactive` mode, registers triggers — no dedicated task needed.
    ///
    /// `send_message` is a closure that sends a message to the given agent
    /// and returns a result. It captures an `Arc<OMTAEKernel>` from the caller.
    pub fn start_agent<F>(
        &self,
        agent_id: AgentId,
        agent_name: &str,
        schedule: &ScheduleMode,
        send_message: F,
    ) where
        F: Fn(AgentId, String) -> tokio::task::JoinHandle<()> + Send + Sync + 'static,
    {
        match schedule {
            ScheduleMode::Reactive => {} // nothing to do
            ScheduleMode::Continuous {
                check_interval_secs,
            } => {
                let interval = std::time::Duration::from_secs(*check_interval_secs);
                let name = agent_name.to_string();
                let mut shutdown = self.shutdown_rx.clone();
                let busy = Arc::new(AtomicBool::new(false));
                let semaphore = self.llm_semaphore.clone();
                let pause_map = Arc::clone(&self.paused);

                info!(
                    agent = %name, id = %agent_id,
                    interval_secs = check_interval_secs,
                    "Starting continuous background loop"
                );

                let handle = tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            _ = tokio::time::sleep(interval) => {}
                            _ = shutdown.changed() => {
                                info!(agent = %name, "Continuous loop: shutdown signal received");
                                break;
                            }
                        }

                        if pause_map.contains_key(&agent_id) {
                            debug!(agent = %name, "Continuous loop: paused — skipping tick");
                            continue;
                        }

                        // Skip if previous tick is still running
                        if busy
                            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                            .is_err()
                        {
                            debug!(agent = %name, "Continuous loop: skipping tick (busy)");
                            continue;
                        }

                        // SECURITY: Acquire global LLM concurrency permit
                        let permit = match semaphore.clone().acquire_owned().await {
                            Ok(p) => p,
                            Err(_) => {
                                busy.store(false, Ordering::SeqCst);
                                break; // Semaphore closed
                            }
                        };

                        let prompt = format!(
                            "[AUTONOMOUS TICK] You are running in continuous mode. \
                             Check your goals and shared memory for pending tasks. \
                             If nothing needs action, respond with exactly NO_REPLY. \
                             Do not spawn agents or delegate unless a task is explicitly pending. \
                             Agent: {name}"
                        );
                        debug!(agent = %name, "Continuous loop: sending self-prompt");
                        let busy_clone = busy.clone();
                        let jh = (send_message)(agent_id, prompt);
                        // Spawn a watcher that clears the busy flag and drops permit when done
                        tokio::spawn(async move {
                            let _ = jh.await;
                            drop(permit);
                            busy_clone.store(false, Ordering::SeqCst);
                        });
                    }
                });

                self.tasks.insert(agent_id, handle);
            }
            ScheduleMode::Periodic { cron } => {
                let interval_secs = parse_cron_to_secs(cron);
                let interval = std::time::Duration::from_secs(interval_secs);
                let name = agent_name.to_string();
                let cron_owned = cron.clone();
                let mut shutdown = self.shutdown_rx.clone();
                let busy = Arc::new(AtomicBool::new(false));
                let semaphore = self.llm_semaphore.clone();
                let pause_map = Arc::clone(&self.paused);

                info!(
                    agent = %name, id = %agent_id,
                    cron = %cron, interval_secs = interval_secs,
                    "Starting periodic background loop"
                );

                let handle = tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            _ = tokio::time::sleep(interval) => {}
                            _ = shutdown.changed() => {
                                info!(agent = %name, "Periodic loop: shutdown signal received");
                                break;
                            }
                        }

                        if pause_map.contains_key(&agent_id) {
                            debug!(agent = %name, "Periodic loop: paused — skipping tick");
                            continue;
                        }

                        if busy
                            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                            .is_err()
                        {
                            debug!(agent = %name, "Periodic loop: skipping tick (busy)");
                            continue;
                        }

                        // SECURITY: Acquire global LLM concurrency permit
                        let permit = match semaphore.clone().acquire_owned().await {
                            Ok(p) => p,
                            Err(_) => {
                                busy.store(false, Ordering::SeqCst);
                                break; // Semaphore closed
                            }
                        };

                        let prompt = format!(
                            "[SCHEDULED TICK] You are running on a periodic schedule ({cron_owned}). \
                             Perform your routine duties. Agent: {name}"
                        );
                        debug!(agent = %name, "Periodic loop: sending scheduled prompt");
                        let busy_clone = busy.clone();
                        let jh = (send_message)(agent_id, prompt);
                        tokio::spawn(async move {
                            let _ = jh.await;
                            drop(permit);
                            busy_clone.store(false, Ordering::SeqCst);
                        });
                    }
                });

                self.tasks.insert(agent_id, handle);
            }
            ScheduleMode::Proactive { .. } => {
                // Proactive agents rely on triggers, not a dedicated loop.
                // Triggers are registered by the kernel during spawn_agent / start_background_agents.
                debug!(agent = %agent_name, "Proactive agent — triggers handle activation");
            }
        }
    }

    /// Stop the background loop for an agent, if one is running.
    pub fn stop_agent(&self, agent_id: AgentId) {
        if let Some((_, handle)) = self.tasks.remove(&agent_id) {
            handle.abort();
            info!(id = %agent_id, "Background loop stopped");
        }
    }

    /// Number of actively running background loops.
    pub fn active_count(&self) -> usize {
        self.tasks.len()
    }
}

/// Parse a proactive condition string into a `TriggerPattern`.
///
/// Supported formats:
/// - `"event:agent_spawned"` → `TriggerPattern::AgentSpawned { name_pattern: "*" }`
/// - `"event:agent_terminated"` → `TriggerPattern::AgentTerminated`
/// - `"event:lifecycle"` → `TriggerPattern::Lifecycle`
/// - `"event:system"` → `TriggerPattern::System`
/// - `"memory:some_key"` → `TriggerPattern::MemoryKeyPattern { key_pattern: "some_key" }`
/// - `"all"` → `TriggerPattern::All`
pub fn parse_condition(condition: &str) -> Option<TriggerPattern> {
    let condition = condition.trim();

    if condition.eq_ignore_ascii_case("all") {
        return Some(TriggerPattern::All);
    }

    if let Some(event_kind) = condition.strip_prefix("event:") {
        let kind = event_kind.trim().to_lowercase();
        return match kind.as_str() {
            "agent_spawned" => Some(TriggerPattern::AgentSpawned {
                name_pattern: "*".to_string(),
            }),
            "agent_terminated" => Some(TriggerPattern::AgentTerminated),
            "lifecycle" => Some(TriggerPattern::Lifecycle),
            "system" => Some(TriggerPattern::System),
            "memory_update" => Some(TriggerPattern::MemoryUpdate),
            other => {
                warn!(condition = %condition, "Unknown event condition: {other}");
                None
            }
        };
    }

    if let Some(key) = condition.strip_prefix("memory:") {
        return Some(TriggerPattern::MemoryKeyPattern {
            key_pattern: key.trim().to_string(),
        });
    }

    warn!(condition = %condition, "Unrecognized proactive condition format");
    None
}

/// Parse a simplified cron expression into seconds.
///
/// Supported formats:
/// - `"every 30s"` → 30
/// - `"every 5m"` → 300
/// - `"every 1h"` → 3600
/// - `"every 2d"` → 172800
///
/// Falls back to 300 seconds (5 minutes) for unparseable expressions.
pub fn parse_cron_to_secs(cron: &str) -> u64 {
    let cron = cron.trim().to_lowercase();

    // Try "every <N><unit>" format
    if let Some(rest) = cron.strip_prefix("every ") {
        let rest = rest.trim();
        if let Some(num_str) = rest.strip_suffix('s') {
            if let Ok(n) = num_str.trim().parse::<u64>() {
                return n;
            }
        }
        if let Some(num_str) = rest.strip_suffix('m') {
            if let Ok(n) = num_str.trim().parse::<u64>() {
                return n * 60;
            }
        }
        if let Some(num_str) = rest.strip_suffix('h') {
            if let Ok(n) = num_str.trim().parse::<u64>() {
                return n * 3600;
            }
        }
        if let Some(num_str) = rest.strip_suffix('d') {
            if let Ok(n) = num_str.trim().parse::<u64>() {
                return n * 86400;
            }
        }
    }

    warn!(cron = %cron, "Unparseable cron expression, defaulting to 300s");
    300
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cron_seconds() {
        assert_eq!(parse_cron_to_secs("every 30s"), 30);
        assert_eq!(parse_cron_to_secs("every 1s"), 1);
    }

    #[test]
    fn test_parse_cron_minutes() {
        assert_eq!(parse_cron_to_secs("every 5m"), 300);
        assert_eq!(parse_cron_to_secs("every 1m"), 60);
    }

    #[test]
    fn test_parse_cron_hours() {
        assert_eq!(parse_cron_to_secs("every 1h"), 3600);
        assert_eq!(parse_cron_to_secs("every 2h"), 7200);
    }

    #[test]
    fn test_parse_cron_days() {
        assert_eq!(parse_cron_to_secs("every 1d"), 86400);
    }

    #[test]
    fn test_parse_cron_fallback() {
        // Unparseable → 300
        assert_eq!(parse_cron_to_secs("*/5 * * * *"), 300);
        assert_eq!(parse_cron_to_secs("gibberish"), 300);
    }

    #[test]
    fn test_parse_condition_events() {
        assert!(matches!(
            parse_condition("event:agent_spawned"),
            Some(TriggerPattern::AgentSpawned { .. })
        ));
        assert!(matches!(
            parse_condition("event:agent_terminated"),
            Some(TriggerPattern::AgentTerminated)
        ));
        assert!(matches!(
            parse_condition("event:lifecycle"),
            Some(TriggerPattern::Lifecycle)
        ));
        assert!(matches!(
            parse_condition("event:system"),
            Some(TriggerPattern::System)
        ));
        assert!(matches!(
            parse_condition("event:memory_update"),
            Some(TriggerPattern::MemoryUpdate)
        ));
    }

    #[test]
    fn test_parse_condition_memory() {
        match parse_condition("memory:agent.*.status") {
            Some(TriggerPattern::MemoryKeyPattern { key_pattern }) => {
                assert_eq!(key_pattern, "agent.*.status");
            }
            other => panic!("Expected MemoryKeyPattern, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_condition_all() {
        assert!(matches!(parse_condition("all"), Some(TriggerPattern::All)));
    }

    #[test]
    fn test_parse_condition_unknown() {
        assert!(parse_condition("event:unknown_thing").is_none());
        assert!(parse_condition("badprefix:foo").is_none());
    }

    #[tokio::test]
    async fn test_continuous_shutdown() {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let executor = BackgroundExecutor::new(shutdown_rx);
        let agent_id = AgentId::new();

        let tick_count = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let tick_clone = tick_count.clone();

        let schedule = ScheduleMode::Continuous {
            check_interval_secs: 1, // 1 second for fast test
        };

        executor.start_agent(agent_id, "test-agent", &schedule, move |_id, _msg| {
            let tc = tick_clone.clone();
            tokio::spawn(async move {
                tc.fetch_add(1, Ordering::SeqCst);
            })
        });

        assert_eq!(executor.active_count(), 1);

        // Wait for at least 1 tick
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        assert!(tick_count.load(Ordering::SeqCst) >= 1);

        // Shutdown
        let _ = shutdown_tx.send(true);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // The loop should have exited (handle finished)
        // Active count still shows the entry until stop_agent is called
        executor.stop_agent(agent_id);
        assert_eq!(executor.active_count(), 0);
    }

    #[tokio::test]
    async fn test_skip_if_busy() {
        let (_shutdown_tx, shutdown_rx) = watch::channel(false);
        let executor = BackgroundExecutor::new(shutdown_rx);
        let agent_id = AgentId::new();

        let tick_count = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let tick_clone = tick_count.clone();

        let schedule = ScheduleMode::Continuous {
            check_interval_secs: 1,
        };

        // Each tick takes 3 seconds — should cause subsequent ticks to be skipped
        executor.start_agent(agent_id, "slow-agent", &schedule, move |_id, _msg| {
            let tc = tick_clone.clone();
            tokio::spawn(async move {
                tc.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            })
        });

        // Wait 2.5 seconds: 1 tick should fire at t=1s, second at t=2s should be skipped (busy)
        tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
        let ticks = tick_count.load(Ordering::SeqCst);
        // Should be exactly 1 because the first tick is still "busy" when the second arrives
        assert_eq!(ticks, 1, "Expected 1 tick (skip-if-busy), got {ticks}");

        executor.stop_agent(agent_id);
    }

    #[test]
    fn test_pause_and_resume() {
        let (_tx, rx) = watch::channel(false);
        let executor = BackgroundExecutor::new(rx);
        let id = AgentId::new();
        assert!(!executor.is_paused(id));
        executor.pause_agent(id);
        assert!(executor.is_paused(id));
        executor.resume_agent(id);
        assert!(!executor.is_paused(id));
    }

    #[test]
    fn test_executor_active_count() {
        let (_tx, rx) = watch::channel(false);
        let executor = BackgroundExecutor::new(rx);
        assert_eq!(executor.active_count(), 0);

        // Reactive mode → no background task
        let id = AgentId::new();
        executor.start_agent(id, "reactive", &ScheduleMode::Reactive, |_id, _msg| {
            tokio::spawn(async {})
        });
        assert_eq!(executor.active_count(), 0);

        // Proactive mode → no dedicated task
        let id2 = AgentId::new();
        executor.start_agent(
            id2,
            "proactive",
            &ScheduleMode::Proactive {
                conditions: vec!["event:agent_spawned".to_string()],
            },
            |_id, _msg| tokio::spawn(async {}),
        );
        assert_eq!(executor.active_count(), 0);
    }
}
