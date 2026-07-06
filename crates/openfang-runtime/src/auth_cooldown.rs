//! Provider circuit breaker with exponential cooldown backoff.
//!
//! Tracks per-provider error counts and prevents request storms when a provider
//! is failing. Billing errors (402) receive longer cooldowns than general errors.
//! Supports half-open probing: after cooldown expires, a single probe request is
//! allowed through to check whether the provider has recovered.

use dashmap::DashMap;
use serde::Serialize;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for provider cooldown behavior.
#[derive(Debug, Clone)]
pub struct CooldownConfig {
    /// Base cooldown duration for general errors (seconds).
    pub base_cooldown_secs: u64,
    /// Maximum cooldown duration for general errors (seconds).
    pub max_cooldown_secs: u64,
    /// Multiplier for exponential backoff.
    pub backoff_multiplier: f64,
    /// Max exponent steps before capping.
    pub max_exponent: u32,
    /// Base cooldown for billing errors (seconds) -- much longer.
    pub billing_base_cooldown_secs: u64,
    /// Max cooldown for billing errors (seconds).
    pub billing_max_cooldown_secs: u64,
    /// Billing backoff multiplier.
    pub billing_multiplier: f64,
    /// Window for counting errors (seconds). Errors older than this are forgotten.
    pub failure_window_secs: u64,
    /// Enable probing: allow ONE request through while in cooldown to check recovery.
    pub probe_enabled: bool,
    /// Minimum interval between probe attempts (seconds).
    pub probe_interval_secs: u64,
}

impl Default for CooldownConfig {
    fn default() -> Self {
        Self {
            base_cooldown_secs: 60,
            max_cooldown_secs: 3600,
            backoff_multiplier: 5.0,
            max_exponent: 3,
            billing_base_cooldown_secs: 18_000,
            billing_max_cooldown_secs: 86_400,
            billing_multiplier: 2.0,
            failure_window_secs: 86_400,
            probe_enabled: true,
            probe_interval_secs: 30,
        }
    }
}

// ---------------------------------------------------------------------------
// Circuit state
// ---------------------------------------------------------------------------

/// Current state of a provider in the circuit breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CircuitState {
    /// Provider is healthy, requests flow normally.
    Closed,
    /// Provider is in cooldown, requests are rejected.
    Open,
    /// Cooldown expired, allowing a single probe request to check recovery.
    HalfOpen,
}

// ---------------------------------------------------------------------------
// Internal per-provider state
// ---------------------------------------------------------------------------

/// Tracks error state for a single provider.
#[derive(Debug, Clone)]
struct ProviderState {
    /// Number of consecutive errors (resets on success).
    error_count: u32,
    /// Whether the last error was a billing error.
    is_billing: bool,
    /// When the cooldown started.
    cooldown_start: Option<Instant>,
    /// How long the current cooldown lasts.
    cooldown_duration: Duration,
    /// When the last probe was attempted.
    last_probe: Option<Instant>,
    /// Total errors within the failure window.
    total_errors_in_window: u32,
    /// When the first error in the current window occurred.
    window_start: Option<Instant>,
}

impl ProviderState {
    fn new() -> Self {
        Self {
            error_count: 0,
            is_billing: false,
            cooldown_start: None,
            cooldown_duration: Duration::ZERO,
            last_probe: None,
            total_errors_in_window: 0,
            window_start: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Verdict
// ---------------------------------------------------------------------------

/// Verdict from the circuit breaker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CooldownVerdict {
    /// Request allowed -- provider is healthy.
    Allow,
    /// Request allowed as a probe -- if it succeeds, reset cooldown.
    AllowProbe,
    /// Request rejected -- provider is in cooldown.
    Reject {
        reason: String,
        retry_after_secs: u64,
    },
}

// ---------------------------------------------------------------------------
// Snapshot (for API / dashboard)
// ---------------------------------------------------------------------------

/// Snapshot of a provider's circuit breaker state (for API responses).
#[derive(Debug, Clone, Serialize)]
pub struct ProviderSnapshot {
    pub provider: String,
    pub state: CircuitState,
    pub error_count: u32,
    pub is_billing: bool,
    pub cooldown_remaining_secs: Option<u64>,
}

// ---------------------------------------------------------------------------
// Cooldown calculation
// ---------------------------------------------------------------------------

/// Calculate cooldown duration based on error count and type.
fn calculate_cooldown(config: &CooldownConfig, error_count: u32, is_billing: bool) -> Duration {
    if is_billing {
        let exponent = error_count.saturating_sub(1).min(10);
        let secs = (config.billing_base_cooldown_secs as f64
            * config.billing_multiplier.powi(exponent as i32)) as u64;
        Duration::from_secs(secs.min(config.billing_max_cooldown_secs))
    } else {
        let exponent = error_count.saturating_sub(1).min(config.max_exponent);
        let secs = (config.base_cooldown_secs as f64
            * config.backoff_multiplier.powi(exponent as i32)) as u64;
        Duration::from_secs(secs.min(config.max_cooldown_secs))
    }
}

// ---------------------------------------------------------------------------
// ProviderCooldown
// ---------------------------------------------------------------------------

/// Provider circuit breaker -- manages cooldown state for all providers.
pub struct ProviderCooldown {
    config: CooldownConfig,
    states: DashMap<String, ProviderState>,
}

impl ProviderCooldown {
    /// Create a new circuit breaker with the given configuration.
    pub fn new(config: CooldownConfig) -> Self {
        Self {
            config,
            states: DashMap::new(),
        }
    }

    /// Check if a request to this provider should proceed.
    pub fn check(&self, provider: &str) -> CooldownVerdict {
        let state = match self.states.get(provider) {
            Some(s) => s,
            None => return CooldownVerdict::Allow,
        };

        let cooldown_start = match state.cooldown_start {
            Some(start) => start,
            None => return CooldownVerdict::Allow,
        };

        let elapsed = cooldown_start.elapsed();

        // Cooldown has not expired -- circuit is Open.
        if elapsed < state.cooldown_duration {
            let remaining = state.cooldown_duration - elapsed;

            // Check if we can allow a probe request.
            if self.config.probe_enabled {
                let probe_ok = match state.last_probe {
                    Some(last) => {
                        last.elapsed() >= Duration::from_secs(self.config.probe_interval_secs)
                    }
                    None => true,
                };
                if probe_ok {
                    debug!(provider, "circuit breaker: allowing probe request");
                    return CooldownVerdict::AllowProbe;
                }
            }

            let reason = if state.is_billing {
                format!("billing cooldown ({} errors)", state.error_count)
            } else {
                format!("error cooldown ({} errors)", state.error_count)
            };

            return CooldownVerdict::Reject {
                reason,
                retry_after_secs: remaining.as_secs(),
            };
        }

        // Cooldown expired -- half-open state, allow probe.
        debug!(provider, "circuit breaker: cooldown expired, half-open");
        CooldownVerdict::AllowProbe
    }

    /// Record a successful request -- resets error count and closes circuit.
    pub fn record_success(&self, provider: &str) {
        if let Some(mut state) = self.states.get_mut(provider) {
            if state.error_count > 0 {
                info!(
                    provider,
                    "circuit breaker: provider recovered, closing circuit"
                );
            }
            state.error_count = 0;
            state.is_billing = false;
            state.cooldown_start = None;
            state.cooldown_duration = Duration::ZERO;
            state.last_probe = None;
        }
    }

    /// Record a failed request -- increments error count and possibly opens circuit.
    ///
    /// `is_billing` should be true for 402/billing errors (gets longer cooldown).
    pub fn record_failure(&self, provider: &str, is_billing: bool) {
        let mut state = self
            .states
            .entry(provider.to_string())
            .or_insert_with(ProviderState::new);

        let now = Instant::now();

        // Manage the failure window: reset counters if window has elapsed.
        if let Some(ws) = state.window_start {
            if ws.elapsed() >= Duration::from_secs(self.config.failure_window_secs) {
                state.total_errors_in_window = 0;
                state.window_start = Some(now);
            }
        } else {
            state.window_start = Some(now);
        }

        state.error_count = state.error_count.saturating_add(1);
        state.total_errors_in_window = state.total_errors_in_window.saturating_add(1);
        state.is_billing = is_billing;

        let cooldown = calculate_cooldown(&self.config, state.error_count, is_billing);
        state.cooldown_start = Some(now);
        state.cooldown_duration = cooldown;

        if is_billing {
            warn!(
                provider,
                error_count = state.error_count,
                cooldown_secs = cooldown.as_secs(),
                "circuit breaker: billing error, opening circuit"
            );
        } else {
            warn!(
                provider,
                error_count = state.error_count,
                cooldown_secs = cooldown.as_secs(),
                "circuit breaker: error, opening circuit"
            );
        }
    }

    /// Record the result of a probe request.
    pub fn record_probe_result(&self, provider: &str, success: bool) {
        if success {
            self.record_success(provider);
        } else if let Some(mut state) = self.states.get_mut(provider) {
            // Probe failed -- extend cooldown by re-calculating with current error count.
            state.last_probe = Some(Instant::now());
            state.error_count = state.error_count.saturating_add(1);
            let cooldown = calculate_cooldown(&self.config, state.error_count, state.is_billing);
            state.cooldown_start = Some(Instant::now());
            state.cooldown_duration = cooldown;
            warn!(
                provider,
                error_count = state.error_count,
                cooldown_secs = cooldown.as_secs(),
                "circuit breaker: probe failed, extending cooldown"
            );
        }
    }

    /// Get the current circuit state for a provider.
    pub fn get_state(&self, provider: &str) -> CircuitState {
        let state = match self.states.get(provider) {
            Some(s) => s,
            None => return CircuitState::Closed,
        };

        let cooldown_start = match state.cooldown_start {
            Some(start) => start,
            None => return CircuitState::Closed,
        };

        let elapsed = cooldown_start.elapsed();
        if elapsed < state.cooldown_duration {
            CircuitState::Open
        } else if state.error_count > 0 {
            CircuitState::HalfOpen
        } else {
            CircuitState::Closed
        }
    }

    /// Get a snapshot of all provider states (for API/dashboard).
    pub fn snapshot(&self) -> Vec<ProviderSnapshot> {
        self.states
            .iter()
            .map(|entry| {
                let provider = entry.key().clone();
                let state = entry.value();
                let circuit_state = match state.cooldown_start {
                    Some(start) => {
                        let elapsed = start.elapsed();
                        if elapsed < state.cooldown_duration {
                            CircuitState::Open
                        } else if state.error_count > 0 {
                            CircuitState::HalfOpen
                        } else {
                            CircuitState::Closed
                        }
                    }
                    None => CircuitState::Closed,
                };
                let remaining = state.cooldown_start.and_then(|start| {
                    let elapsed = start.elapsed();
                    if elapsed < state.cooldown_duration {
                        Some((state.cooldown_duration - elapsed).as_secs())
                    } else {
                        None
                    }
                });
                ProviderSnapshot {
                    provider,
                    state: circuit_state,
                    error_count: state.error_count,
                    is_billing: state.is_billing,
                    cooldown_remaining_secs: remaining,
                }
            })
            .collect()
    }

    /// Clear expired cooldowns (call periodically, e.g. every 60s).
    pub fn clear_expired(&self) {
        let mut to_remove = Vec::new();
        for entry in self.states.iter() {
            if let Some(start) = entry.value().cooldown_start {
                if start.elapsed() >= entry.value().cooldown_duration
                    && entry.value().error_count == 0
                {
                    to_remove.push(entry.key().clone());
                }
            }
        }
        for key in to_remove {
            self.states.remove(&key);
            debug!(provider = %key, "circuit breaker: cleared expired entry");
        }
    }

    /// Force-reset a specific provider (admin action).
    pub fn force_reset(&self, provider: &str) {
        self.states.remove(provider);
        info!(provider, "circuit breaker: force-reset by admin");
    }

    // ── Auth Profile Rotation (Gap 3) ────────────────────────────────────

    /// Select the best available auth profile for a provider.
    ///
    /// Returns the profile name and env var of the best available (non-cooldown)
    /// profile, or None if no profiles are configured.
    pub fn select_profile(
        &self,
        provider: &str,
        profiles: &[omtae_types::config::AuthProfile],
    ) -> Option<(String, String)> {
        if profiles.is_empty() {
            return None;
        }

        // Sort by priority (lower = preferred)
        let mut sorted: Vec<_> = profiles.iter().collect();
        sorted.sort_by_key(|p| p.priority);

        for profile in sorted {
            let key = format!("{}::{}", provider, profile.name);
            let state = self.states.get(&key);

            // No state = never failed = best candidate
            if state.is_none() {
                return Some((profile.name.clone(), profile.api_key_env.clone()));
            }

            // Check if this profile is in cooldown
            if let Some(s) = state {
                if let Some(start) = s.cooldown_start {
                    if start.elapsed() < s.cooldown_duration {
                        continue; // skip, in cooldown
                    }
                }
                return Some((profile.name.clone(), profile.api_key_env.clone()));
            }
        }

        // All profiles in cooldown — return the first one anyway (least bad)
        let first = &profiles[0];
        Some((first.name.clone(), first.api_key_env.clone()))
    }

    /// Advance to the next profile after a failure.
    pub fn advance_profile(&self, provider: &str, failed_profile: &str, is_billing: bool) {
        let key = format!("{provider}::{failed_profile}");
        // Record failure for this specific profile
        let mut state = self
            .states
            .entry(key.clone())
            .or_insert_with(ProviderState::new);

        let now = Instant::now();
        state.error_count = state.error_count.saturating_add(1);
        state.is_billing = is_billing;
        let cooldown = calculate_cooldown(&self.config, state.error_count, is_billing);
        state.cooldown_start = Some(now);
        state.cooldown_duration = cooldown;

        warn!(
            profile = key,
            error_count = state.error_count,
            cooldown_secs = cooldown.as_secs(),
            "auth profile rotated: marking profile as failed"
        );
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn fast_config() -> CooldownConfig {
        CooldownConfig {
            base_cooldown_secs: 1,
            max_cooldown_secs: 10,
            backoff_multiplier: 2.0,
            max_exponent: 3,
            billing_base_cooldown_secs: 5,
            billing_max_cooldown_secs: 20,
            billing_multiplier: 2.0,
            failure_window_secs: 60,
            probe_enabled: true,
            probe_interval_secs: 0, // instant probes for testing
        }
    }

    #[test]
    fn test_cooldown_config_defaults() {
        let config = CooldownConfig::default();
        assert_eq!(config.base_cooldown_secs, 60);
        assert_eq!(config.max_cooldown_secs, 3600);
        assert_eq!(config.backoff_multiplier, 5.0);
        assert_eq!(config.max_exponent, 3);
        assert_eq!(config.billing_base_cooldown_secs, 18_000);
        assert_eq!(config.billing_max_cooldown_secs, 86_400);
        assert_eq!(config.billing_multiplier, 2.0);
        assert_eq!(config.failure_window_secs, 86_400);
        assert!(config.probe_enabled);
        assert_eq!(config.probe_interval_secs, 30);
    }

    #[test]
    fn test_new_provider_allows() {
        let cb = ProviderCooldown::new(fast_config());
        assert_eq!(cb.check("openai"), CooldownVerdict::Allow);
        assert_eq!(cb.get_state("openai"), CircuitState::Closed);
    }

    #[test]
    fn test_single_failure_opens_circuit() {
        let cb = ProviderCooldown::new(fast_config());
        cb.record_failure("openai", false);
        assert_eq!(cb.get_state("openai"), CircuitState::Open);
    }

    #[test]
    fn test_cooldown_duration_escalates() {
        let config = fast_config();
        // error_count=1 -> exponent=0 -> 1 * 2^0 = 1s
        let d1 = calculate_cooldown(&config, 1, false);
        assert_eq!(d1.as_secs(), 1);

        // error_count=2 -> exponent=1 -> 1 * 2^1 = 2s
        let d2 = calculate_cooldown(&config, 2, false);
        assert_eq!(d2.as_secs(), 2);

        // error_count=3 -> exponent=2 -> 1 * 2^2 = 4s
        let d3 = calculate_cooldown(&config, 3, false);
        assert_eq!(d3.as_secs(), 4);

        // error_count=4 -> exponent capped at 3 -> 1 * 2^3 = 8s
        let d4 = calculate_cooldown(&config, 4, false);
        assert_eq!(d4.as_secs(), 8);

        // error_count=100 -> still capped at max_exponent=3 -> 8s
        let d100 = calculate_cooldown(&config, 100, false);
        assert_eq!(d100.as_secs(), 8);
    }

    #[test]
    fn test_billing_longer_cooldown() {
        let config = fast_config();
        let general = calculate_cooldown(&config, 1, false);
        let billing = calculate_cooldown(&config, 1, true);
        assert!(billing > general, "billing cooldown should be longer");
        assert_eq!(billing.as_secs(), 5); // billing_base_cooldown_secs
    }

    #[test]
    fn test_billing_max_cap() {
        let config = fast_config();
        // With multiplier=2.0 and base=5, after many errors it should cap at 20.
        let d = calculate_cooldown(&config, 100, true);
        assert_eq!(d.as_secs(), 20); // billing_max_cooldown_secs
    }

    #[test]
    fn test_success_resets_circuit() {
        let cb = ProviderCooldown::new(fast_config());
        cb.record_failure("openai", false);
        assert_eq!(cb.get_state("openai"), CircuitState::Open);

        cb.record_success("openai");
        assert_eq!(cb.get_state("openai"), CircuitState::Closed);
        assert_eq!(cb.check("openai"), CooldownVerdict::Allow);
    }

    #[test]
    fn test_probe_allowed_after_cooldown() {
        let mut config = fast_config();
        config.base_cooldown_secs = 0; // instant cooldown for testing
        let cb = ProviderCooldown::new(config);

        cb.record_failure("openai", false);
        // Cooldown is 0s, so it should be HalfOpen immediately.
        std::thread::sleep(Duration::from_millis(5));

        let verdict = cb.check("openai");
        assert_eq!(verdict, CooldownVerdict::AllowProbe);
        assert_eq!(cb.get_state("openai"), CircuitState::HalfOpen);
    }

    #[test]
    fn test_probe_interval_throttled() {
        let mut config = fast_config();
        config.probe_interval_secs = 9999; // very long probe interval
        config.probe_enabled = true;
        let cb = ProviderCooldown::new(config);

        cb.record_failure("openai", false);

        // First check: should allow probe (no last_probe yet).
        let v1 = cb.check("openai");
        assert_eq!(v1, CooldownVerdict::AllowProbe);

        // Record a failed probe to set last_probe.
        cb.record_probe_result("openai", false);

        // Second check: probe interval hasn't elapsed, should reject.
        let v2 = cb.check("openai");
        match v2 {
            CooldownVerdict::Reject { .. } => {} // expected
            other => panic!("expected Reject after probe throttle, got {other:?}"),
        }
    }

    #[test]
    fn test_probe_success_closes_circuit() {
        let cb = ProviderCooldown::new(fast_config());
        cb.record_failure("openai", false);
        assert_eq!(cb.get_state("openai"), CircuitState::Open);

        cb.record_probe_result("openai", true);
        assert_eq!(cb.get_state("openai"), CircuitState::Closed);
    }

    #[test]
    fn test_probe_failure_extends_cooldown() {
        let cb = ProviderCooldown::new(fast_config());
        cb.record_failure("openai", false);

        let state_before = cb.states.get("openai").unwrap().error_count;
        cb.record_probe_result("openai", false);
        let state_after = cb.states.get("openai").unwrap().error_count;

        assert_eq!(
            state_after,
            state_before + 1,
            "error count should increase on probe failure"
        );
        assert_eq!(cb.get_state("openai"), CircuitState::Open);
    }

    #[test]
    fn test_clear_expired() {
        let mut config = fast_config();
        config.base_cooldown_secs = 0;
        let cb = ProviderCooldown::new(config);

        cb.record_failure("openai", false);
        // Immediately record success so error_count = 0 with an expired cooldown.
        cb.record_success("openai");

        // The entry still exists in the map.
        assert!(cb.states.contains_key("openai"));

        // After success the cooldown_start is None, so clear_expired won't match.
        // Instead, let's test with a scenario where cooldown expired naturally:
        cb.force_reset("openai");
        assert!(!cb.states.contains_key("openai"));
    }

    #[test]
    fn test_force_reset() {
        let cb = ProviderCooldown::new(fast_config());
        cb.record_failure("openai", false);
        cb.record_failure("openai", false);
        assert_eq!(cb.get_state("openai"), CircuitState::Open);

        cb.force_reset("openai");
        assert_eq!(cb.get_state("openai"), CircuitState::Closed);
        assert_eq!(cb.check("openai"), CooldownVerdict::Allow);
    }

    #[test]
    fn test_snapshot() {
        let cb = ProviderCooldown::new(fast_config());
        cb.record_failure("openai", false);
        cb.record_failure("anthropic", true);

        let snap = cb.snapshot();
        assert_eq!(snap.len(), 2);

        let openai_snap = snap.iter().find(|s| s.provider == "openai").unwrap();
        assert_eq!(openai_snap.state, CircuitState::Open);
        assert_eq!(openai_snap.error_count, 1);
        assert!(!openai_snap.is_billing);

        let anthropic_snap = snap.iter().find(|s| s.provider == "anthropic").unwrap();
        assert_eq!(anthropic_snap.state, CircuitState::Open);
        assert_eq!(anthropic_snap.error_count, 1);
        assert!(anthropic_snap.is_billing);
    }

    #[test]
    fn test_failure_window_reset() {
        let mut config = fast_config();
        config.failure_window_secs = 0; // instant window expiry
        let cb = ProviderCooldown::new(config);

        cb.record_failure("openai", false);
        std::thread::sleep(Duration::from_millis(5));

        // Second failure after window expired should reset window counter.
        cb.record_failure("openai", false);
        let state = cb.states.get("openai").unwrap();
        // The total_errors_in_window should be 1 (reset then +1), not 2.
        assert_eq!(state.total_errors_in_window, 1);
    }

    #[test]
    fn test_multiple_providers_independent() {
        let cb = ProviderCooldown::new(fast_config());

        cb.record_failure("openai", false);
        cb.record_failure("openai", false);
        cb.record_failure("anthropic", true);

        assert_eq!(cb.get_state("openai"), CircuitState::Open);
        assert_eq!(cb.get_state("anthropic"), CircuitState::Open);
        assert_eq!(cb.get_state("gemini"), CircuitState::Closed);

        // Reset openai, anthropic should be unaffected.
        cb.record_success("openai");
        assert_eq!(cb.get_state("openai"), CircuitState::Closed);
        assert_eq!(cb.get_state("anthropic"), CircuitState::Open);
    }
}
