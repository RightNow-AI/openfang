//! Supervised listener infrastructure for channel adapters.
//!
//! Extracts the common reconnection loop, exponential backoff, and shutdown
//! handling that every adapter reimplements. Individual adapters just need to
//! implement a `listen_once()` method that runs until it encounters an error
//! or the connection drops — the supervisor handles the rest.

use std::time::Duration;
use tracing::{info, warn};

/// Default initial backoff duration on failures.
pub const DEFAULT_INITIAL_BACKOFF: Duration = Duration::from_secs(1);

/// Default maximum backoff duration on failures (cap for exponential backoff).
pub const DEFAULT_MAX_BACKOFF: Duration = Duration::from_secs(60);

/// Default channel buffer size for the mpsc channel between the listener
/// spawn and the returned stream.
pub const DEFAULT_CHANNEL_BUFFER: usize = 256;

/// Configuration for the supervised reconnection loop.
#[derive(Debug, Clone)]
pub struct SupervisorConfig {
    /// Initial delay before retrying after a failure.
    pub initial_backoff: Duration,
    /// Maximum delay between retries (exponential backoff cap).
    pub max_backoff: Duration,
    /// Human-readable component name for log messages.
    pub component_name: String,
}

impl SupervisorConfig {
    /// Create a new config with the given component name and default backoff values.
    pub fn new(component_name: impl Into<String>) -> Self {
        Self {
            initial_backoff: DEFAULT_INITIAL_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
            component_name: component_name.into(),
        }
    }

    /// Create a config with custom backoff values.
    pub fn with_backoff(
        component_name: impl Into<String>,
        initial: Duration,
        max: Duration,
    ) -> Self {
        Self {
            initial_backoff: initial,
            max_backoff: max,
            component_name: component_name.into(),
        }
    }
}

/// Calculate the next backoff duration using exponential backoff with a cap.
///
/// Doubles the current backoff, clamping at `max_backoff`.
#[inline]
pub fn next_backoff(current: Duration, max_backoff: Duration) -> Duration {
    (current * 2).min(max_backoff)
}

/// Run a supervised reconnection loop.
///
/// This is the core loop that replaces the duplicated reconnection logic across
/// all adapters. It calls `connect_and_run` repeatedly, applying exponential
/// backoff on failures and resetting backoff on success.
///
/// # Arguments
/// * `config` - Supervisor configuration (backoff, name).
/// * `shutdown_rx` - Watch receiver for shutdown signal.
/// * `connect_and_run` - Async closure that establishes a connection and processes
///   messages. Returns `Ok(true)` to reconnect, `Ok(false)` to stop permanently,
///   or `Err(msg)` on failure (will retry with backoff).
///
/// # Example
/// ```ignore
/// supervisor::run_supervised_loop(
///     SupervisorConfig::new("my-adapter"),
///     shutdown_rx,
///     || async {
///         let conn = connect().await.map_err(|e| e.to_string())?;
///         process_messages(conn).await;
///         Ok(true) // reconnect
///     },
/// ).await;
/// ```
pub async fn run_supervised_loop<F, Fut>(
    config: SupervisorConfig,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    connect_and_run: F,
) where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<bool, String>>,
{
    let mut backoff = config.initial_backoff;
    let name = &config.component_name;

    loop {
        // Check shutdown before attempting connection
        if *shutdown_rx.borrow() {
            info!("{name}: shutdown requested, exiting supervisor loop");
            break;
        }

        match connect_and_run().await {
            Ok(true) => {
                // Adapter wants to reconnect (e.g., server disconnect, stream ended)
                if *shutdown_rx.borrow() {
                    info!("{name}: shutdown requested after disconnect");
                    break;
                }
                warn!("{name}: reconnecting in {backoff:?}");
                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {}
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            info!("{name}: shutdown during backoff");
                            break;
                        }
                    }
                }
                backoff = next_backoff(backoff, config.max_backoff);
            }
            Ok(false) => {
                // Adapter signaled permanent stop (e.g., 409 Conflict, fatal error)
                info!("{name}: adapter requested permanent stop");
                break;
            }
            Err(e) => {
                // Connection or setup failure — retry with backoff
                if *shutdown_rx.borrow() {
                    break;
                }
                warn!("{name}: error: {e}, retrying in {backoff:?}");
                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {}
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            info!("{name}: shutdown during error backoff");
                            break;
                        }
                    }
                }
                backoff = next_backoff(backoff, config.max_backoff);
            }
        }
    }

    info!("{name}: supervisor loop stopped");
}

/// Run a supervised reconnection loop, resetting backoff on success.
///
/// Like `run_supervised_loop`, but resets the backoff to initial whenever
/// the `connect_and_run` closure succeeds (returns `Ok`). This is the more
/// common pattern for adapters where a successful connection means the
/// backoff should be reset.
pub async fn run_supervised_loop_reset_on_connect<F, Fut>(
    config: SupervisorConfig,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    connect_and_run: F,
) where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<bool, String>>,
{
    let mut backoff = config.initial_backoff;
    let name = &config.component_name;

    loop {
        if *shutdown_rx.borrow() {
            info!("{name}: shutdown requested, exiting supervisor loop");
            break;
        }

        match connect_and_run().await {
            Ok(should_reconnect) => {
                // Successfully connected at some point — reset backoff
                backoff = config.initial_backoff;

                if !should_reconnect || *shutdown_rx.borrow() {
                    info!("{name}: adapter loop finished (reconnect={should_reconnect})");
                    break;
                }
                warn!("{name}: reconnecting in {backoff:?}");
                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {}
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            break;
                        }
                    }
                }
                backoff = next_backoff(backoff, config.max_backoff);
            }
            Err(e) => {
                if *shutdown_rx.borrow() {
                    break;
                }
                warn!("{name}: error: {e}, retrying in {backoff:?}");
                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {}
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            break;
                        }
                    }
                }
                backoff = next_backoff(backoff, config.max_backoff);
            }
        }
    }

    info!("{name}: supervisor loop stopped");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_next_backoff() {
        assert_eq!(next_backoff(Duration::from_secs(1), Duration::from_secs(60)), Duration::from_secs(2));
        assert_eq!(next_backoff(Duration::from_secs(2), Duration::from_secs(60)), Duration::from_secs(4));
        assert_eq!(next_backoff(Duration::from_secs(32), Duration::from_secs(60)), Duration::from_secs(60));
        assert_eq!(next_backoff(Duration::from_secs(60), Duration::from_secs(60)), Duration::from_secs(60));
    }

    #[test]
    fn test_supervisor_config_new() {
        let config = SupervisorConfig::new("test");
        assert_eq!(config.initial_backoff, DEFAULT_INITIAL_BACKOFF);
        assert_eq!(config.max_backoff, DEFAULT_MAX_BACKOFF);
        assert_eq!(config.component_name, "test");
    }

    #[test]
    fn test_supervisor_config_with_backoff() {
        let config = SupervisorConfig::with_backoff(
            "custom",
            Duration::from_millis(500),
            Duration::from_secs(30),
        );
        assert_eq!(config.initial_backoff, Duration::from_millis(500));
        assert_eq!(config.max_backoff, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_supervised_loop_immediate_shutdown() {
        let (tx, rx) = tokio::sync::watch::channel(true); // already shut down
        let _ = tx;
        let call_count = Arc::new(AtomicU32::new(0));
        let cc = call_count.clone();

        run_supervised_loop(
            SupervisorConfig::new("test"),
            rx,
            || {
                let cc = cc.clone();
                async move {
                    cc.fetch_add(1, Ordering::SeqCst);
                    Ok(true)
                }
            },
        )
        .await;

        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_supervised_loop_stops_on_false() {
        let (_tx, rx) = tokio::sync::watch::channel(false);
        let call_count = Arc::new(AtomicU32::new(0));
        let cc = call_count.clone();

        run_supervised_loop(
            SupervisorConfig::new("test"),
            rx,
            || {
                let cc = cc.clone();
                async move {
                    cc.fetch_add(1, Ordering::SeqCst);
                    Ok(false) // stop
                }
            },
        )
        .await;

        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_supervised_loop_shutdown_during_run() {
        let (tx, rx) = tokio::sync::watch::channel(false);
        let call_count = Arc::new(AtomicU32::new(0));
        let cc = call_count.clone();

        run_supervised_loop(
            SupervisorConfig::new("test"),
            rx,
            || {
                let cc = cc.clone();
                let tx = tx.clone();
                async move {
                    let count = cc.fetch_add(1, Ordering::SeqCst);
                    if count == 0 {
                        // First call: signal shutdown and return reconnect
                        let _ = tx.send(true);
                        Ok(true)
                    } else {
                        Ok(false)
                    }
                }
            },
        )
        .await;

        // Should have run exactly once (shutdown before reconnect)
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }
}
