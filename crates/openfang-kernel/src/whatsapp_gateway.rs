//! WhatsApp Web gateway — embedded Node.js process management.
//!
//! Embeds the gateway JS at compile time, extracts it to `~/.openfang/whatsapp-gateway/`,
//! runs `npm install` if needed, and spawns `node index.js` as a managed child process
//! that auto-restarts on crash.

use crate::config::openfang_home;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

/// Gateway source files embedded at compile time.
const GATEWAY_INDEX_JS: &str = include_str!("../../../packages/whatsapp-gateway/index.js");
const GATEWAY_PACKAGE_JSON: &str = include_str!("../../../packages/whatsapp-gateway/package.json");

/// Default port for the WhatsApp Web gateway.
const DEFAULT_GATEWAY_PORT: u16 = 3009;

/// Maximum restart attempts before giving up.
const MAX_RESTARTS: u32 = 3;

/// Restart backoff delays in seconds: 5s, 10s, 20s.
const RESTART_DELAYS: [u64; 3] = [5, 10, 20];

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
    let index_changed = write_if_changed(&index_path, GATEWAY_INDEX_JS)
        .map_err(|e| format!("Write index.js: {e}"))?;
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

    // Auto-set the env var so the rest of the system finds the gateway
    std::env::set_var(
        "WHATSAPP_WEB_GATEWAY_URL",
        format!("http://127.0.0.1:{port}"),
    );
    info!("WHATSAPP_WEB_GATEWAY_URL set to http://127.0.0.1:{port}");

    // Spawn with crash monitoring
    let kernel_weak = Arc::downgrade(kernel);
    let gateway_pid = Arc::clone(&kernel.whatsapp_gateway_pid);

    tokio::spawn(async move {
        let mut restarts = 0u32;

        loop {
            let node_cmd = if cfg!(windows) { "node.exe" } else { "node" };

            info!("Starting WhatsApp Web gateway (attempt {})", restarts + 1);

            let child = tokio::process::Command::new(node_cmd)
                .arg("index.js")
                .current_dir(&gateway_path)
                .env("WHATSAPP_GATEWAY_PORT", port.to_string())
                .env("OPENFANG_URL", &openfang_url)
                .env("OPENFANG_DEFAULT_AGENT", &default_agent)
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

            restarts += 1;
            if restarts >= MAX_RESTARTS {
                warn!("WhatsApp gateway exceeded max restarts ({MAX_RESTARTS}), giving up");
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
        assert_eq!(RESTART_DELAYS, [5, 10, 20]);
        assert_eq!(MAX_RESTARTS, 3);
    }
}
