//! Telegram Local Bot API Server — embedded process management.
//!
//! Manages a local instance of telegram-bot-api server to support large file downloads (>20MB).
//! The official Telegram Bot API has a 20MB limit on getFile, but the local server supports up to 2GB.
//!
//! Architecture:
//! - Uses a preinstalled telegram-bot-api binary from `~/.openfang/bin`, system PATH, or
//!   `~/.openfang/telegram-local-api/`
//! - Spawns as a managed child process with auto-restart on crash
//! - Lifecycle tied to OpenFang kernel (starts/stops with daemon)

use crate::config::openfang_home;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use tracing::{error, info, warn};

/// Maximum restart attempts before giving up.
const MAX_RESTARTS: u32 = 3;

/// Restart backoff delays in seconds.
const RESTART_DELAYS: [u64; 3] = [5, 10, 20];

/// Get the local API server installation directory.
fn local_api_dir() -> PathBuf {
    openfang_home().join("telegram-local-api")
}

#[cfg(windows)]
fn telegram_bot_api_binary_name() -> &'static str {
    "telegram-bot-api.exe"
}

#[cfg(not(windows))]
fn telegram_bot_api_binary_name() -> &'static str {
    "telegram-bot-api"
}

/// Configuration for Local Bot API Server.
#[derive(Debug, Clone)]
pub struct LocalApiConfig {
    /// Telegram API ID (from https://my.telegram.org/apps)
    pub api_id: String,
    /// Telegram API Hash
    pub api_hash: String,
    /// Server listen port
    pub port: u16,
    /// Working directory for file storage
    pub work_dir: PathBuf,
}

/// Check if telegram-bot-api binary exists in system PATH or local installation.
async fn find_telegram_bot_api_binary() -> Option<PathBuf> {
    // Check OpenFang bin directory first
    let openfang_bin = openfang_home()
        .join("bin")
        .join(telegram_bot_api_binary_name());
    if openfang_bin.exists() {
        info!(
            "Found telegram-bot-api in OpenFang bin: {}",
            openfang_bin.display()
        );
        return Some(openfang_bin);
    }

    #[cfg(windows)]
    let locator = "where";
    #[cfg(not(windows))]
    let locator = "which";

    // Check system PATH first
    if let Ok(output) = tokio::process::Command::new(locator)
        .arg("telegram-bot-api")
        .output()
        .await
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .unwrap_or("")
                .to_string();
            if !path.is_empty() {
                info!("Found telegram-bot-api in system PATH: {}", path);
                return Some(PathBuf::from(path));
            }
        }
    }

    // Check local installation
    let local_bin = local_api_dir().join(telegram_bot_api_binary_name());
    if local_bin.exists() {
        info!(
            "Found telegram-bot-api in local installation: {}",
            local_bin.display()
        );
        return Some(local_bin);
    }

    None
}

/// Download and install telegram-bot-api binary.
///
/// For now, we'll guide users to install it manually or via Docker.
/// Future: auto-download prebuilt binaries from GitHub releases.
async fn ensure_telegram_bot_api_installed() -> Result<PathBuf, String> {
    if let Some(binary) = find_telegram_bot_api_binary().await {
        return Ok(binary);
    }

    Err("telegram-bot-api binary not found. Please install it:\n\
         \n\
         Option 1 - Docker (recommended):\n\
         docker run -d --name telegram-bot-api -p 8081:8081 \\\n\
           -e TELEGRAM_API_ID=your_api_id \\\n\
           -e TELEGRAM_API_HASH=your_api_hash \\\n\
           aiogram/telegram-bot-api:latest\n\
         \n\
         Option 2 - Build from source:\n\
         git clone --recursive https://github.com/tdlib/telegram-bot-api.git\n\
         cd telegram-bot-api && mkdir build && cd build\n\
         cmake -DCMAKE_BUILD_TYPE=Release ..\n\
         cmake --build . --target install\n\
         \n\
         Option 3 - Place the binary where OpenFang can find it:\n\
         cp telegram-bot-api ~/.openfang/bin/\n\
         \n\
         Option 4 - System package manager:\n\
         # Arch Linux\n\
         yay -S telegram-bot-api\n\
         \n\
         # macOS\n\
         install manually under ~/.openfang/bin or use Docker"
        .to_string())
}

/// Start the Local Bot API Server as a managed child process.
///
/// Returns after the initial process accepts TCP connections on the configured port.
pub async fn start_local_api_server(
    config: LocalApiConfig,
    pid_storage: Arc<std::sync::Mutex<Option<u32>>>,
    shutdown_requested: Arc<AtomicBool>,
) -> Result<(), String> {
    let binary = ensure_telegram_bot_api_installed().await?;

    std::fs::create_dir_all(&config.work_dir)
        .map_err(|e| format!("Failed to create work dir: {}", e))?;

    if pid_storage.lock().unwrap().is_some() {
        info!("Telegram Local Bot API Server already running, skip duplicate start");
        return Ok(());
    }

    // Reset stop flag for (re)start.
    shutdown_requested.store(false, Ordering::SeqCst);

    info!(
        "Starting Telegram Local Bot API Server on port {}",
        config.port
    );

    let (ready_tx, ready_rx) = oneshot::channel();
    let binary_clone = binary.clone();
    let config_clone = config.clone();
    let pid_storage_clone = pid_storage.clone();
    let shutdown_clone = shutdown_requested.clone();

    tokio::spawn(async move {
        let mut restart_count = 0;
        let mut ready_tx = Some(ready_tx);

        loop {
            if shutdown_clone.load(Ordering::SeqCst) {
                if let Some(tx) = ready_tx.take() {
                    let _ = tx.send(Err(
                        "Telegram Local Bot API Server stop requested before startup completed"
                            .to_string(),
                    ));
                }
                info!("Telegram Local Bot API Server stop requested, exiting manager loop");
                return;
            }

            info!(
                "Spawning telegram-bot-api process (attempt {})",
                restart_count + 1
            );

            let mut child = match tokio::process::Command::new(&binary_clone)
                .arg("--api-id")
                .arg(&config_clone.api_id)
                .arg("--api-hash")
                .arg(&config_clone.api_hash)
                .arg("--local")
                .arg("--http-port")
                .arg(config_clone.port.to_string())
                .arg("--dir")
                .arg(&config_clone.work_dir)
                // Use inherited stdio to avoid child process blocking on a full pipe buffer.
                .stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::inherit())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to spawn telegram-bot-api: {}", e);
                    return;
                }
            };

            // Store PID
            if let Some(pid) = child.id() {
                *pid_storage_clone.lock().unwrap() = Some(pid);
                info!("Telegram Local Bot API Server started with PID {}", pid);
            }

            match wait_for_local_api_ready(config_clone.port, &shutdown_clone).await {
                Ok(()) => {
                    info!(
                        "Telegram Local Bot API Server is accepting connections on port {}",
                        config_clone.port
                    );
                    if let Some(tx) = ready_tx.take() {
                        let _ = tx.send(Ok(()));
                    }
                }
                Err(e) => {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    *pid_storage_clone.lock().unwrap() = None;

                    if let Some(tx) = ready_tx.take() {
                        let _ = tx.send(Err(e.clone()));
                        error!(
                            "Telegram Local Bot API Server failed readiness check: {}",
                            e
                        );
                        return;
                    }

                    error!(
                        "Telegram Local Bot API Server failed readiness check: {}",
                        e
                    );
                }
            }

            // Wait for process to exit
            match child.wait().await {
                Ok(status) => {
                    warn!(
                        "Telegram Local Bot API Server exited with status: {}",
                        status
                    );
                }
                Err(e) => {
                    error!("Error waiting for telegram-bot-api process: {}", e);
                }
            }

            // Clear PID
            *pid_storage_clone.lock().unwrap() = None;

            if shutdown_clone.load(Ordering::SeqCst) {
                info!("Telegram Local Bot API Server stopped by shutdown request");
                return;
            }

            // Check restart limit
            if restart_count >= MAX_RESTARTS {
                error!(
                    "Telegram Local Bot API Server crashed {} times, giving up",
                    MAX_RESTARTS
                );
                return;
            }

            // Backoff before restart
            let delay = RESTART_DELAYS[restart_count as usize];
            warn!(
                "Telegram Local Bot API Server will restart in {} seconds...",
                delay
            );
            tokio::time::sleep(tokio::time::Duration::from_secs(delay)).await;

            restart_count += 1;
        }
    });

    match ready_rx.await {
        Ok(result) => result,
        Err(_) => Err(
            "Telegram Local Bot API Server manager exited before reporting readiness".to_string(),
        ),
    }
}

/// Stop the Local Bot API Server by killing its process.
pub fn stop_local_api_server(
    pid_storage: Arc<std::sync::Mutex<Option<u32>>>,
    shutdown_requested: Arc<AtomicBool>,
) {
    shutdown_requested.store(true, Ordering::SeqCst);

    if let Some(pid) = *pid_storage.lock().unwrap() {
        info!("Stopping Telegram Local Bot API Server (PID {})", pid);

        #[cfg(unix)]
        {
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
        }

        #[cfg(windows)]
        {
            use std::process::Command;
            let _ = Command::new("taskkill")
                .args(&["/PID", &pid.to_string(), "/F"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }

        *pid_storage.lock().unwrap() = None;
    }
}

async fn wait_for_local_api_ready(
    port: u16,
    shutdown_requested: &AtomicBool,
) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(10);
    let addr = format!("127.0.0.1:{port}");

    loop {
        if shutdown_requested.load(Ordering::SeqCst) {
            return Err("Telegram Local Bot API Server startup cancelled".to_string());
        }

        match TcpStream::connect(&addr).await {
            Ok(stream) => {
                drop(stream);
                return Ok(());
            }
            Err(_) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
            }
            Err(e) => {
                return Err(format!(
                    "telegram-bot-api did not become ready on {addr} within 10 seconds: {e}"
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_find_binary() {
        // This test will pass if telegram-bot-api is installed, otherwise skip
        let result = find_telegram_bot_api_binary().await;
        println!("Binary search result: {:?}", result);
    }

    #[test]
    fn test_binary_name_matches_platform() {
        #[cfg(windows)]
        assert_eq!(telegram_bot_api_binary_name(), "telegram-bot-api.exe");

        #[cfg(not(windows))]
        assert_eq!(telegram_bot_api_binary_name(), "telegram-bot-api");
    }

    #[test]
    fn test_local_api_dir_uses_openfang_home() {
        let temp = tempdir().unwrap();
        let original = std::env::var_os("OPENFANG_HOME");

        std::env::set_var("OPENFANG_HOME", temp.path());
        assert_eq!(local_api_dir(), temp.path().join("telegram-local-api"));

        match original {
            Some(value) => std::env::set_var("OPENFANG_HOME", value),
            None => std::env::remove_var("OPENFANG_HOME"),
        }
    }
}
