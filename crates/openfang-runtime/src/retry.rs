//! Generic retry with exponential backoff and jitter.
//!
//! Provides a configurable, async-aware retry utility that can be used for
//! LLM API calls, network operations, channel message delivery, and any
//! other fallible async operation across the OpenFang codebase.
//!
//! Jitter uses `std::time::SystemTime` UNIX nanos as a seed to avoid
//! requiring the `rand` crate as a dependency.

use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Configuration for retry behavior.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of attempts (including the first try).
    pub max_attempts: u32,
    /// Minimum delay between retries in milliseconds.
    pub min_delay_ms: u64,
    /// Maximum delay between retries in milliseconds.
    pub max_delay_ms: u64,
    /// Jitter factor (0.0 = no jitter, 1.0 = full jitter).
    ///
    /// The actual sleep is `delay * (1 + random_fraction * jitter)`, where
    /// `random_fraction` is in `[0, 1)`.
    pub jitter: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            min_delay_ms: 300,
            max_delay_ms: 30_000,
            jitter: 0.2,
        }
    }
}

/// Result of a retry operation.
#[derive(Debug)]
pub enum RetryOutcome<T, E> {
    /// The operation succeeded.
    Success {
        /// The successful result.
        result: T,
        /// Total number of attempts made (1 = first try succeeded).
        attempts: u32,
    },
    /// All retries exhausted without success.
    Exhausted {
        /// The error from the last attempt.
        last_error: E,
        /// Total number of attempts made.
        attempts: u32,
    },
}

// ---------------------------------------------------------------------------
// Backoff computation
// ---------------------------------------------------------------------------

/// Compute the delay for a given attempt (0-indexed).
///
/// Formula: `min(min_delay * 2^attempt, max_delay) * (1 + random * jitter)`
///
/// Uses `std::time::SystemTime` nanos as a lightweight pseudo-random source
/// instead of requiring the `rand` crate.
pub fn compute_backoff(config: &RetryConfig, attempt: u32) -> u64 {
    // Exponential base: min_delay * 2^attempt, capped at max_delay.
    let base = config
        .min_delay_ms
        .saturating_mul(1u64.checked_shl(attempt).unwrap_or(u64::MAX));
    let capped = base.min(config.max_delay_ms);

    // Jitter: multiply by (1 + random_fraction * jitter).
    if config.jitter <= 0.0 {
        return capped;
    }

    let frac = pseudo_random_fraction();
    let jitter_offset = (capped as f64) * frac * config.jitter;
    let with_jitter = (capped as f64) + jitter_offset;

    // Clamp to max_delay (jitter can push slightly above).
    (with_jitter as u64).min(config.max_delay_ms)
}

/// Return a pseudo-random fraction in `[0, 1)` using the current system time
/// nanos. This is NOT cryptographically secure, but good enough for jitter.
fn pseudo_random_fraction() -> f64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    // Mix the bits a bit to reduce predictability.
    let mixed = nanos.wrapping_mul(2654435761); // Knuth multiplicative hash
    (mixed as f64) / (u32::MAX as f64)
}

// ---------------------------------------------------------------------------
// Core retry function
// ---------------------------------------------------------------------------

/// Execute an async operation with retry.
///
/// # Parameters
///
/// - `config` — retry configuration (attempts, delays, jitter).
/// - `operation` — the async closure to execute. Called once per attempt.
/// - `should_retry` — predicate that inspects the error and returns `true`
///   if the operation should be retried.
/// - `retry_after_hint` — optional hint extractor. If it returns `Some(ms)`,
///   that delay is used instead of the computed backoff (but still capped at
///   `max_delay_ms`).
///
/// # Returns
///
/// A `RetryOutcome` indicating success or exhaustion.
pub async fn retry_async<F, Fut, T, E, P, H>(
    config: &RetryConfig,
    mut operation: F,
    should_retry: P,
    retry_after_hint: H,
) -> RetryOutcome<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    P: Fn(&E) -> bool,
    H: Fn(&E) -> Option<u64>,
    E: std::fmt::Debug,
{
    let max = config.max_attempts.max(1);
    let mut last_error: Option<E> = None;

    for attempt in 0..max {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    debug!(
                        attempt = attempt + 1,
                        "retry succeeded after {} previous failures", attempt
                    );
                }
                return RetryOutcome::Success {
                    result,
                    attempts: attempt + 1,
                };
            }
            Err(err) => {
                let is_last = attempt + 1 >= max;

                if is_last || !should_retry(&err) {
                    if !should_retry(&err) {
                        debug!(
                            attempt = attempt + 1,
                            "error is not retryable, giving up: {:?}", err
                        );
                    } else {
                        warn!(
                            attempt = attempt + 1,
                            max_attempts = max,
                            "all retry attempts exhausted: {:?}",
                            err
                        );
                    }
                    return RetryOutcome::Exhausted {
                        last_error: err,
                        attempts: attempt + 1,
                    };
                }

                // Determine delay.
                let hint = retry_after_hint(&err);
                let delay_ms = if let Some(hinted) = hint {
                    // Respect the hint, but cap it.
                    hinted.min(config.max_delay_ms)
                } else {
                    compute_backoff(config, attempt)
                };

                debug!(
                    attempt = attempt + 1,
                    delay_ms, "retrying after error: {:?}", err
                );

                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;

                last_error = Some(err);
            }
        }
    }

    // Should not be reachable, but handle gracefully.
    RetryOutcome::Exhausted {
        last_error: last_error.expect("at least one attempt should have been made"),
        attempts: max,
    }
}

// ---------------------------------------------------------------------------
// Pre-built configs
// ---------------------------------------------------------------------------

/// Retry config for LLM API calls.
///
/// 3 attempts, 1s initial delay, up to 60s, 20% jitter.
pub fn llm_retry_config() -> RetryConfig {
    RetryConfig {
        max_attempts: 3,
        min_delay_ms: 1_000,
        max_delay_ms: 60_000,
        jitter: 0.2,
    }
}

/// Backoff window used for provider-originated 429/503 retry loops.
///
/// This is intentionally wider than the generic LLM retry helper so multiple
/// agents do not immediately retry in lockstep after a shared provider throttle.
pub fn provider_rate_limit_delay_ms(attempt: u32) -> u64 {
    let config = RetryConfig {
        max_attempts: 4,
        min_delay_ms: 1_000,
        max_delay_ms: 30_000,
        jitter: 1.0,
    };
    compute_backoff(&config, attempt)
}

/// Retry config for network operations (webhooks, fetches).
///
/// 3 attempts, 500ms initial delay, up to 30s, 10% jitter.
pub fn network_retry_config() -> RetryConfig {
    RetryConfig {
        max_attempts: 3,
        min_delay_ms: 500,
        max_delay_ms: 30_000,
        jitter: 0.1,
    }
}

/// Retry config for channel message delivery.
///
/// 3 attempts, 400ms initial delay, up to 15s, 10% jitter.
pub fn channel_retry_config() -> RetryConfig {
    RetryConfig {
        max_attempts: 3,
        min_delay_ms: 400,
        max_delay_ms: 15_000,
        jitter: 0.1,
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_retry_config_defaults() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.min_delay_ms, 300);
        assert_eq!(config.max_delay_ms, 30_000);
        assert!((config.jitter - 0.2).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_backoff_exponential() {
        let config = RetryConfig {
            max_attempts: 5,
            min_delay_ms: 100,
            max_delay_ms: 100_000,
            jitter: 0.0, // no jitter for deterministic test
        };

        // 100 * 2^0 = 100
        assert_eq!(compute_backoff(&config, 0), 100);
        // 100 * 2^1 = 200
        assert_eq!(compute_backoff(&config, 1), 200);
        // 100 * 2^2 = 400
        assert_eq!(compute_backoff(&config, 2), 400);
        // 100 * 2^3 = 800
        assert_eq!(compute_backoff(&config, 3), 800);
    }

    #[test]
    fn test_compute_backoff_capped() {
        let config = RetryConfig {
            max_attempts: 10,
            min_delay_ms: 1_000,
            max_delay_ms: 5_000,
            jitter: 0.0,
        };

        // 1000 * 2^0 = 1000
        assert_eq!(compute_backoff(&config, 0), 1_000);
        // 1000 * 2^1 = 2000
        assert_eq!(compute_backoff(&config, 1), 2_000);
        // 1000 * 2^2 = 4000
        assert_eq!(compute_backoff(&config, 2), 4_000);
        // 1000 * 2^3 = 8000, capped at 5000
        assert_eq!(compute_backoff(&config, 3), 5_000);
        // Further attempts stay capped
        assert_eq!(compute_backoff(&config, 10), 5_000);
    }

    #[tokio::test]
    async fn test_retry_success_first_try() {
        let config = RetryConfig {
            max_attempts: 3,
            min_delay_ms: 10,
            max_delay_ms: 100,
            jitter: 0.0,
        };

        let outcome = retry_async(
            &config,
            || async { Ok::<&str, &str>("hello") },
            |_| true,
            |_: &&str| None,
        )
        .await;

        match outcome {
            RetryOutcome::Success { result, attempts } => {
                assert_eq!(result, "hello");
                assert_eq!(attempts, 1);
            }
            _ => panic!("expected success"),
        }
    }

    #[tokio::test]
    async fn test_retry_success_after_failures() {
        let config = RetryConfig {
            max_attempts: 5,
            min_delay_ms: 1, // tiny delays for test speed
            max_delay_ms: 10,
            jitter: 0.0,
        };

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let outcome = retry_async(
            &config,
            move || {
                let c = counter_clone.clone();
                async move {
                    let n = c.fetch_add(1, Ordering::SeqCst);
                    if n < 2 {
                        Err("not yet")
                    } else {
                        Ok("finally")
                    }
                }
            },
            |_| true,
            |_: &&str| None,
        )
        .await;

        match outcome {
            RetryOutcome::Success { result, attempts } => {
                assert_eq!(result, "finally");
                assert_eq!(attempts, 3); // failed twice, succeeded on 3rd
            }
            _ => panic!("expected success"),
        }
    }

    #[tokio::test]
    async fn test_retry_exhausted() {
        let config = RetryConfig {
            max_attempts: 3,
            min_delay_ms: 1,
            max_delay_ms: 10,
            jitter: 0.0,
        };

        let outcome = retry_async(
            &config,
            || async { Err::<(), &str>("always fails") },
            |_| true,
            |_: &&str| None,
        )
        .await;

        match outcome {
            RetryOutcome::Exhausted {
                last_error,
                attempts,
            } => {
                assert_eq!(last_error, "always fails");
                assert_eq!(attempts, 3);
            }
            _ => panic!("expected exhausted"),
        }
    }

    #[tokio::test]
    async fn test_retry_non_retryable_error() {
        let config = RetryConfig {
            max_attempts: 5,
            min_delay_ms: 1,
            max_delay_ms: 10,
            jitter: 0.0,
        };

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let outcome = retry_async(
            &config,
            move || {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err::<(), &str>("fatal error")
                }
            },
            |_| false, // never retry
            |_: &&str| None,
        )
        .await;

        match outcome {
            RetryOutcome::Exhausted {
                last_error,
                attempts,
            } => {
                assert_eq!(last_error, "fatal error");
                assert_eq!(attempts, 1); // gave up immediately
            }
            _ => panic!("expected exhausted"),
        }

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_with_hint_delay() {
        let config = RetryConfig {
            max_attempts: 3,
            min_delay_ms: 10_000, // large base delay
            max_delay_ms: 60_000,
            jitter: 0.0,
        };

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let start = std::time::Instant::now();

        let outcome = retry_async(
            &config,
            move || {
                let c = counter_clone.clone();
                async move {
                    let n = c.fetch_add(1, Ordering::SeqCst);
                    if n < 1 {
                        Err("transient")
                    } else {
                        Ok("ok")
                    }
                }
            },
            |_| true,
            |_: &&str| Some(1), // hint: 1ms delay (overrides 10s base)
        )
        .await;

        let elapsed = start.elapsed();

        match outcome {
            RetryOutcome::Success { result, attempts } => {
                assert_eq!(result, "ok");
                assert_eq!(attempts, 2);
                // Should complete in well under 1 second (hint was 1ms,
                // not the 10s base delay).
                assert!(
                    elapsed.as_millis() < 5_000,
                    "retry took too long: {:?} — hint should have overridden base delay",
                    elapsed
                );
            }
            _ => panic!("expected success"),
        }
    }

    #[test]
    fn test_llm_retry_config() {
        let config = llm_retry_config();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.min_delay_ms, 1_000);
        assert_eq!(config.max_delay_ms, 60_000);
        assert!((config.jitter - 0.2).abs() < f64::EPSILON);
    }

    #[test]
    fn test_provider_rate_limit_delay_is_bounded() {
        let delay0 = provider_rate_limit_delay_ms(0);
        let delay1 = provider_rate_limit_delay_ms(1);
        let delay10 = provider_rate_limit_delay_ms(10);

        assert!((1_000..=2_000).contains(&delay0));
        assert!((2_000..=4_000).contains(&delay1));
        assert_eq!(delay10, 30_000);
    }

    #[test]
    fn test_channel_retry_config() {
        let config = channel_retry_config();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.min_delay_ms, 400);
        assert_eq!(config.max_delay_ms, 15_000);
        assert!((config.jitter - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn test_network_retry_config() {
        let config = network_retry_config();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.min_delay_ms, 500);
        assert_eq!(config.max_delay_ms, 30_000);
        assert!((config.jitter - 0.1).abs() < f64::EPSILON);
    }
}
