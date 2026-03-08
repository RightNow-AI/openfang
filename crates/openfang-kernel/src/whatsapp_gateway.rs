//! WhatsApp Web gateway — embedded Node.js process management and health monitoring.
//!
//! Embeds the gateway JS at compile time, extracts it to `~/.openfang/whatsapp-gateway/`,
//! runs `npm install` if needed, and spawns `node index.js` as a managed child process
//! that auto-restarts on crash. Includes a health monitor loop that polls the gateway
//! and triggers reconnection if the Baileys WebSocket dies (e.g. after system sleep/wake).

use crate::config::openfang_home;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{info, warn};

/// Gateway source files embedded at compile time.
const GATEWAY_INDEX_JS: &str =
    include_str!("../../../packages/whatsapp-gateway/index.js");
const GATEWAY_PACKAGE_JSON: &str =
    include_str!("../../../packages/whatsapp-gateway/package.json");

/// Default port for the WhatsApp Web gateway.
const DEFAULT_GATEWAY_PORT: u16 = 3009;

/// Maximum restart attempts before giving up.
const MAX_RESTARTS: u32 = 20;

/// If the gateway ran for this long without crashing, reset the restart counter.
const RESTART_RESET_WINDOW_SECS: u64 = 300;

/// Restart backoff delays in seconds (wraps at last value).
const RESTART_DELAYS: [u64; 5] = [5, 10, 20, 30, 60];

/// Get the gateway installation directory.
fn gateway_dir() -> PathBuf {
    openfang_home().join("whatsapp-gateway")
}

/// Compute a simple hash of content for change detection.
fn content_hash(content: &str) -> String {
    // Use a simple FNV-style hash — no crypto needed, just change detection.
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in content.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

/// Write a file only if its content hash differs from the existing file.
/// Returns `true` if the file was written (content changed).
fn write_if_changed(path: &std::path::Path, content: &str) -> std::io::Result<bool> {
    let hash_path = path.with_extension("hash");
    let new_hash = content_hash(content);

    // Check existing hash
    if let Ok(existing_hash) = std::fs::read_to_string(&hash_path) {
        if existing_hash.trim() == new_hash {
            return Ok(false); // No change
        }
    }

    std::fs::write(path, content)?;
    std::fs::write(&hash_path, &new_hash)?;
    Ok(true)
}

/// Ensure the gateway files are extracted and npm dependencies installed.
///
/// Returns the gateway directory path on success, or an error message.
async fn ensure_gateway_installed() -> Result<PathBuf, String> {
    let dir = gateway_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create gateway dir: {e}"))?;

    let index_path = dir.join("index.js");
    let package_path = dir.join("package.json");

    // Write files only if content changed (avoids unnecessary npm install)
    let index_changed =
        write_if_changed(&index_path, GATEWAY_INDEX_JS).map_err(|e| format!("Write index.js: {e}"))?;
    let package_changed = write_if_changed(&package_path, GATEWAY_PACKAGE_JSON)
        .map_err(|e| format!("Write package.json: {e}"))?;

    let node_modules = dir.join("node_modules");
    let needs_install = !node_modules.exists() || package_changed;

    if needs_install {
        info!("Installing WhatsApp gateway npm dependencies...");

        // Determine npm command (npm.cmd on Windows, npm elsewhere)
        let npm_cmd = if cfg!(windows) { "npm.cmd" } else { "npm" };

        let output = tokio::process::Command::new(npm_cmd)
            .arg("install")
            .arg("--production")
            .current_dir(&dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .await
            .map_err(|e| format!("npm install failed to start: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("npm install failed: {stderr}"));
        }

        info!("WhatsApp gateway npm dependencies installed");
    } else if index_changed {
        info!("WhatsApp gateway index.js updated (binary upgrade)");
    }

    Ok(dir)
}

/// Check if Node.js is available on the system.
async fn node_available() -> bool {
    let node_cmd = if cfg!(windows) { "node.exe" } else { "node" };
    tokio::process::Command::new(node_cmd)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Start the WhatsApp Web gateway as a managed child process.
///
/// This function:
/// 1. Checks if Node.js is available
/// 2. Extracts and installs the gateway files
/// 3. Spawns `node index.js` with appropriate env vars
/// 4. Sets `WHATSAPP_WEB_GATEWAY_URL` so the daemon finds it
/// 5. Monitors the process and restarts on crash (up to 3 times)
///
/// The PID is stored in the kernel's `whatsapp_gateway_pid` for shutdown cleanup.
pub async fn start_whatsapp_gateway(kernel: &Arc<super::kernel::OpenFangKernel>) {
    // Only start if WhatsApp is configured
    let wa_config = match &kernel.config.channels.whatsapp {
        Some(cfg) => cfg.clone(),
        None => return,
    };

    // Check for Node.js
    if !node_available().await {
        warn!(
            "WhatsApp Web gateway requires Node.js >= 18 but `node` was not found. \
             Install Node.js to enable WhatsApp Web integration."
        );
        return;
    }

    // Extract and install
    let gateway_path = match ensure_gateway_installed().await {
        Ok(p) => p,
        Err(e) => {
            warn!("WhatsApp Web gateway setup failed: {e}");
            return;
        }
    };

    let port = DEFAULT_GATEWAY_PORT;
    let api_listen = &kernel.config.api_listen;
    let openfang_url = format!("http://{api_listen}");
    let default_agent = wa_config
        .default_agent
        .as_deref()
        .unwrap_or("assistant")
        .to_string();
    let allowed_users = wa_config.allowed_users.join(",");

    // Auto-set the env var so the rest of the system finds the gateway
    std::env::set_var("WHATSAPP_WEB_GATEWAY_URL", format!("http://127.0.0.1:{port}"));
    info!("WHATSAPP_WEB_GATEWAY_URL set to http://127.0.0.1:{port}");

    // Spawn with crash monitoring
    let kernel_weak = Arc::downgrade(kernel);
    let gateway_pid = Arc::clone(&kernel.whatsapp_gateway_pid);

    tokio::spawn(async move {
        let mut restarts = 0u32;
        let mut last_crash_at = std::time::Instant::now();

        loop {
            let node_cmd = if cfg!(windows) { "node.exe" } else { "node" };

            info!("Starting WhatsApp Web gateway (attempt {})", restarts + 1);

            let child = tokio::process::Command::new(node_cmd)
                .arg("index.js")
                .current_dir(&gateway_path)
                .env("WHATSAPP_GATEWAY_PORT", port.to_string())
                .env("OPENFANG_URL", &openfang_url)
                .env("OPENFANG_DEFAULT_AGENT", &default_agent)
                .env("WHATSAPP_ALLOWED_USERS", &allowed_users)
                .stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::inherit())
                .spawn();

            let mut child = match child {
                Ok(c) => c,
                Err(e) => {
                    warn!("Failed to spawn WhatsApp gateway: {e}");
                    return;
                }
            };

            // Store PID for shutdown cleanup
            if let Some(pid) = child.id() {
                if let Ok(mut guard) = gateway_pid.lock() {
                    *guard = Some(pid);
                }
                info!("WhatsApp Web gateway started (PID {pid})");
            }

            // Wait for process exit
            match child.wait().await {
                Ok(status) => {
                    // Clear stored PID
                    if let Ok(mut guard) = gateway_pid.lock() {
                        *guard = None;
                    }

                    // Check if kernel is still alive (not shutting down)
                    let kernel = match kernel_weak.upgrade() {
                        Some(k) => k,
                        None => {
                            info!("WhatsApp gateway exited (kernel dropped)");
                            return;
                        }
                    };

                    if kernel.supervisor.is_shutting_down() {
                        info!("WhatsApp gateway stopped (daemon shutting down)");
                        return;
                    }

                    if status.success() {
                        info!("WhatsApp gateway exited cleanly");
                        return;
                    }

                    warn!(
                        "WhatsApp gateway crashed (exit: {status}), restart {}/{MAX_RESTARTS}",
                        restarts + 1
                    );
                }
                Err(e) => {
                    if let Ok(mut guard) = gateway_pid.lock() {
                        *guard = None;
                    }
                    warn!("WhatsApp gateway wait error: {e}");
                }
            }

            // Reset restart budget if the gateway was stable for long enough
            let elapsed = last_crash_at.elapsed().as_secs();
            if elapsed >= RESTART_RESET_WINDOW_SECS && restarts > 0 {
                info!(
                    elapsed_secs = elapsed,
                    old_count = restarts,
                    "WhatsApp gateway restart counter reset (was stable)"
                );
                restarts = 0;
            }
            last_crash_at = std::time::Instant::now();

            restarts += 1;
            if restarts >= MAX_RESTARTS {
                warn!(
                    "WhatsApp gateway exceeded max restarts ({MAX_RESTARTS}), giving up"
                );
                return;
            }

            // Backoff before restart
            let delay = RESTART_DELAYS
                .get(restarts as usize - 1)
                .copied()
                .unwrap_or(20);
            info!("Restarting WhatsApp gateway in {delay}s...");
            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
        }
    });
}

// ---------------------------------------------------------------------------
// Health monitoring — polls gateway /health and triggers reconnect on failure
// ---------------------------------------------------------------------------

/// Health status of the WhatsApp gateway (updated by the kernel health loop).
#[derive(Debug, Clone, Serialize)]
pub struct WhatsAppGatewayHealth {
    /// Whether the gateway HTTP process is reachable.
    pub process_alive: bool,
    /// Whether the Baileys WebSocket is connected.
    pub ws_connected: bool,
    /// Last successful health check timestamp (RFC 3339).
    pub last_ok: Option<String>,
    /// Last error message from a failed health check.
    pub last_error: Option<String>,
    /// Number of auto-reconnect attempts triggered by the kernel.
    pub reconnect_attempts: u32,
}

/// Health check interval for the gateway monitor loop.
const HEALTH_CHECK_INTERVAL_SECS: u64 = 30;

/// Number of consecutive disconnected checks before triggering a reconnect.
const RECONNECT_AFTER_CHECKS: u32 = 2;

/// Check the WhatsApp gateway health by hitting its `/health` endpoint.
async fn check_gateway_health(port: u16) -> Result<serde_json::Value, String> {
    let addr = format!("127.0.0.1:{port}");
    let mut stream = tokio::net::TcpStream::connect(&addr)
        .await
        .map_err(|e| format!("Connect failed: {e}"))?;

    let req = format!(
        "GET /health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(req.as_bytes())
        .await
        .map_err(|e| format!("Write: {e}"))?;

    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .await
        .map_err(|e| format!("Read: {e}"))?;
    let response = String::from_utf8_lossy(&buf);

    if let Some(idx) = response.find("\r\n\r\n") {
        let body_str = &response[idx + 4..];
        serde_json::from_str(body_str.trim()).map_err(|e| format!("Parse: {e}"))
    } else {
        Err("No HTTP body in response".to_string())
    }
}

/// Trigger a reconnect via the gateway's `POST /health/reconnect` endpoint.
async fn trigger_gateway_reconnect(port: u16) -> Result<(), String> {
    let addr = format!("127.0.0.1:{port}");
    let mut stream = tokio::net::TcpStream::connect(&addr)
        .await
        .map_err(|e| format!("Connect failed: {e}"))?;

    let req = format!(
        "POST /health/reconnect HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(req.as_bytes())
        .await
        .map_err(|e| format!("Write: {e}"))?;

    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .await
        .map_err(|e| format!("Read: {e}"))?;
    Ok(())
}

/// Run a periodic health check loop for the WhatsApp gateway.
///
/// Polls `/health` every 30 seconds. If the Baileys WebSocket is disconnected
/// for 2 consecutive checks (~60s), triggers `/health/reconnect` to auto-heal.
/// This handles the case where system sleep/wake kills the WebSocket silently.
pub async fn run_whatsapp_health_loop(kernel: &Arc<super::kernel::OpenFangKernel>) {
    let port = DEFAULT_GATEWAY_PORT;
    let health_state = Arc::clone(&kernel.whatsapp_gateway_health);

    // Wait for gateway process to boot up
    tokio::time::sleep(std::time::Duration::from_secs(15)).await;

    let mut interval =
        tokio::time::interval(std::time::Duration::from_secs(HEALTH_CHECK_INTERVAL_SECS));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    interval.tick().await; // skip first immediate tick

    let mut consecutive_disconnects = 0u32;
    let mut total_reconnects = 0u32;
    let mut last_reconnect_trigger: Option<std::time::Instant> = None;

    // Cooldown period after triggering a reconnect — don't trigger another one
    // for at least 90 seconds to let the gateway finish its own reconnect cycle.
    const RECONNECT_COOLDOWN_SECS: u64 = 90;

    loop {
        interval.tick().await;

        if kernel.supervisor.is_shutting_down() {
            break;
        }

        if kernel.config.channels.whatsapp.is_none() {
            break;
        }

        match check_gateway_health(port).await {
            Ok(body) => {
                let connected = body
                    .get("connected")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                // Check if gateway is already handling its own reconnect
                let conn_status = body
                    .get("conn_status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let is_reconnecting = conn_status == "reconnecting";

                if connected {
                    consecutive_disconnects = 0;
                    if let Ok(mut guard) = health_state.write() {
                        *guard = Some(WhatsAppGatewayHealth {
                            process_alive: true,
                            ws_connected: true,
                            last_ok: Some(chrono::Utc::now().to_rfc3339()),
                            last_error: None,
                            reconnect_attempts: total_reconnects,
                        });
                    }
                } else if is_reconnecting {
                    // Gateway is already reconnecting on its own — don't interfere
                    if let Ok(mut guard) = health_state.write() {
                        *guard = Some(WhatsAppGatewayHealth {
                            process_alive: true,
                            ws_connected: false,
                            last_ok: guard.as_ref().and_then(|h| h.last_ok.clone()),
                            last_error: Some("Gateway is reconnecting".to_string()),
                            reconnect_attempts: total_reconnects,
                        });
                    }
                } else {
                    consecutive_disconnects += 1;
                    warn!(
                        "WhatsApp gateway: WebSocket disconnected ({consecutive_disconnects} consecutive checks)"
                    );

                    if let Ok(mut guard) = health_state.write() {
                        *guard = Some(WhatsAppGatewayHealth {
                            process_alive: true,
                            ws_connected: false,
                            last_ok: guard.as_ref().and_then(|h| h.last_ok.clone()),
                            last_error: Some(format!(
                                "Disconnected for {consecutive_disconnects} consecutive checks"
                            )),
                            reconnect_attempts: total_reconnects,
                        });
                    }

                    // After N consecutive failures, trigger reconnect — but respect cooldown
                    if consecutive_disconnects >= RECONNECT_AFTER_CHECKS {
                        let in_cooldown = last_reconnect_trigger
                            .map(|t| t.elapsed().as_secs() < RECONNECT_COOLDOWN_SECS)
                            .unwrap_or(false);

                        if in_cooldown {
                            // Still in cooldown — don't pile on
                            continue;
                        }

                        info!("WhatsApp gateway: triggering auto-reconnect");
                        total_reconnects += 1;
                        last_reconnect_trigger = Some(std::time::Instant::now());
                        match trigger_gateway_reconnect(port).await {
                            Ok(()) => {
                                info!("WhatsApp gateway: reconnect triggered successfully");
                                consecutive_disconnects = 0;
                            }
                            Err(e) => {
                                warn!("WhatsApp gateway: reconnect trigger failed: {e}");
                            }
                        }
                    }
                }
            }
            Err(e) => {
                // Process might be down or restarting
                if let Ok(mut guard) = health_state.write() {
                    *guard = Some(WhatsAppGatewayHealth {
                        process_alive: false,
                        ws_connected: false,
                        last_ok: guard.as_ref().and_then(|h| h.last_ok.clone()),
                        last_error: Some(e),
                        reconnect_attempts: total_reconnects,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_files_not_empty() {
        assert!(!GATEWAY_INDEX_JS.is_empty());
        assert!(!GATEWAY_PACKAGE_JSON.is_empty());
        assert!(GATEWAY_INDEX_JS.contains("WhatsApp"));
        assert!(GATEWAY_PACKAGE_JSON.contains("@openfang/whatsapp-gateway"));
    }

    #[test]
    fn test_content_hash_deterministic() {
        let h1 = content_hash("hello world");
        let h2 = content_hash("hello world");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_content_hash_changes_on_different_input() {
        let h1 = content_hash("version 1");
        let h2 = content_hash("version 2");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_gateway_dir_under_openfang_home() {
        let dir = gateway_dir();
        assert!(dir.ends_with("whatsapp-gateway"));
        assert!(dir
            .parent()
            .unwrap()
            .to_string_lossy()
            .contains(".openfang"));
    }

    #[test]
    fn test_write_if_changed_creates_new_file() {
        let tmp = std::env::temp_dir().join("openfang_test_gateway");
        let _ = std::fs::create_dir_all(&tmp);
        let path = tmp.join("test_write.js");
        let hash_path = path.with_extension("hash");

        // Clean up any previous runs
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&hash_path);

        // First write should return true (new file)
        let changed = write_if_changed(&path, "console.log('v1')").unwrap();
        assert!(changed);
        assert!(path.exists());
        assert!(hash_path.exists());

        // Same content should return false
        let changed = write_if_changed(&path, "console.log('v1')").unwrap();
        assert!(!changed);

        // Different content should return true
        let changed = write_if_changed(&path, "console.log('v2')").unwrap();
        assert!(changed);

        // Clean up
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&hash_path);
        let _ = std::fs::remove_dir(&tmp);
    }

    #[test]
    fn test_default_gateway_port() {
        assert_eq!(DEFAULT_GATEWAY_PORT, 3009);
    }

    #[test]
    fn test_restart_backoff_delays() {
        assert_eq!(RESTART_DELAYS, [5, 10, 20, 30, 60]);
        assert_eq!(MAX_RESTARTS, 20);
        assert_eq!(RESTART_RESET_WINDOW_SECS, 300);
    }
}
