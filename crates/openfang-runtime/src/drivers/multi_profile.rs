//! Multi-profile wrapper for CLI-based LLM drivers (Claude Code, Qwen Code).
//!
//! When a user has multiple subscriptions, each with its own OAuth token stored
//! in a separate config directory, this wrapper automatically rotates between
//! them on rate-limit errors.  Cooldown timestamps are derived from the
//! Anthropic usage API (`/api/oauth/usage`) so profiles are re-enabled at
//! exactly the right time.

use crate::llm_driver::{CompletionRequest, CompletionResponse, LlmDriver, LlmError, StreamEvent};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

// ── Anthropic usage API ──────────────────────────────────────────────────

const ANTHROPIC_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const ANTHROPIC_BETA_HEADER: &str = "oauth-2025-04-20";

/// Utilisation data for a single rate-limit window.
#[derive(Debug, Deserialize)]
struct UsageWindow {
    /// Percentage of the window consumed (0.0 – 100.0).
    #[serde(default)]
    utilization: f64,
    /// ISO-8601 timestamp when the window resets.
    #[serde(default)]
    resets_at: Option<String>,
}

/// Response from `GET /api/oauth/usage`.
#[derive(Debug, Deserialize)]
struct UsageResponse {
    #[serde(default)]
    five_hour: Option<UsageWindow>,
    #[serde(default)]
    seven_day: Option<UsageWindow>,
}

/// Parsed usage info we care about.
#[derive(Debug, Clone)]
pub struct ProfileUsage {
    pub five_hour_utilization: f64,
    pub five_hour_resets_at: Option<chrono::DateTime<chrono::Utc>>,
    pub seven_day_utilization: f64,
}

// ── Profile state ────────────────────────────────────────────────────────

/// Runtime state for a single credential profile.
struct ProfileState {
    /// Display name (directory basename).
    name: String,
    /// Absolute path to the config directory containing `.credentials.json`.
    config_dir: PathBuf,
    /// When `Some`, the profile is in cooldown until this instant.
    cooldown_until: Option<std::time::Instant>,
    /// Cached OAuth access token (read once from disk).
    access_token: Option<String>,
}

impl ProfileState {
    fn new(config_dir: PathBuf) -> Self {
        let name = config_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "default".to_string());

        Self {
            name,
            config_dir,
            cooldown_until: None,
            access_token: None,
        }
    }

    /// Read the OAuth access token from the credentials file on disk.
    fn load_token(&mut self) -> Option<&str> {
        if self.access_token.is_some() {
            return self.access_token.as_deref();
        }

        let cred_paths = [
            self.config_dir.join(".credentials.json"),
            self.config_dir.join("credentials.json"),
        ];

        for path in &cred_paths {
            if let Ok(contents) = std::fs::read_to_string(path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&contents) {
                    if let Some(token) = json
                        .get("claudeAiOauth")
                        .and_then(|o| o.get("accessToken"))
                        .and_then(|t| t.as_str())
                    {
                        self.access_token = Some(token.to_string());
                        debug!(profile = %self.name, path = %path.display(), "Loaded OAuth token");
                        return self.access_token.as_deref();
                    }
                }
            }
        }

        warn!(profile = %self.name, "No valid credentials found");
        None
    }

    /// Check if this profile is currently in cooldown.
    fn is_available(&self) -> bool {
        match self.cooldown_until {
            Some(until) => std::time::Instant::now() >= until,
            None => true,
        }
    }

    /// Put this profile in cooldown until the given instant.
    fn set_cooldown(&mut self, until: std::time::Instant) {
        info!(
            profile = %self.name,
            cooldown_secs = until.duration_since(std::time::Instant::now()).as_secs(),
            "Profile entering cooldown"
        );
        self.cooldown_until = Some(until);
    }

    /// Clear cooldown (e.g. after a successful request).
    fn clear_cooldown(&mut self) {
        if self.cooldown_until.is_some() {
            info!(profile = %self.name, "Profile cooldown cleared");
            self.cooldown_until = None;
        }
    }
}

// ── Usage API client ─────────────────────────────────────────────────────

/// Fetch usage from the Anthropic API for a given OAuth token.
async fn fetch_usage(access_token: &str) -> Result<ProfileUsage, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get(ANTHROPIC_USAGE_URL)
        .header("Authorization", format!("Bearer {access_token}"))
        .header("anthropic-beta", ANTHROPIC_BETA_HEADER)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if resp.status() == 401 {
        return Err("OAuth token expired — re-authenticate this profile".to_string());
    }

    let data: UsageResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse usage response: {e}"))?;

    let (five_util, five_resets) = match data.five_hour {
        Some(w) => {
            let resets = w.resets_at.and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|dt| dt.with_timezone(&chrono::Utc))
            });
            (w.utilization, resets)
        }
        None => (0.0, None),
    };

    let seven_util = data.seven_day.map(|w| w.utilization).unwrap_or(0.0);

    Ok(ProfileUsage {
        five_hour_utilization: five_util,
        five_hour_resets_at: five_resets,
        seven_day_utilization: seven_util,
    })
}

/// Convert a future UTC reset time to a `std::time::Instant`.
fn utc_to_instant(dt: chrono::DateTime<chrono::Utc>) -> std::time::Instant {
    let now_utc = chrono::Utc::now();
    if dt <= now_utc {
        return std::time::Instant::now();
    }
    let delta = (dt - now_utc).to_std().unwrap_or(std::time::Duration::from_secs(0));
    // Add a small buffer (30s) to avoid racing the reset
    std::time::Instant::now() + delta + std::time::Duration::from_secs(30)
}

/// Default cooldown when we can't determine the reset time (30 minutes).
const FALLBACK_COOLDOWN_SECS: u64 = 30 * 60;

// ── Multi-profile driver ─────────────────────────────────────────────────

/// A wrapper driver that manages multiple credential profiles for a
/// CLI-based LLM provider, rotating on rate-limit errors.
pub struct MultiProfileDriver {
    /// The underlying driver factory.  For each request we pick a profile,
    /// set `CLAUDE_CONFIG_DIR`, and delegate to a fresh driver instance.
    cli_path: String,
    skip_permissions: bool,
    /// Mutable profile state, protected by a mutex.
    profiles: Arc<Mutex<Vec<ProfileState>>>,
    /// Index of the currently active profile.
    current: Arc<Mutex<usize>>,
}

impl MultiProfileDriver {
    /// Create a new multi-profile driver.
    ///
    /// `profile_dirs` must contain at least one path.  Each path should be
    /// a directory containing Claude OAuth credentials.  If only one path
    /// is given, the driver behaves identically to a plain `ClaudeCodeDriver`
    /// but with rate-limit detection and reporting.
    pub fn new(
        cli_path: Option<String>,
        skip_permissions: bool,
        profile_dirs: Vec<String>,
    ) -> Self {
        let profiles: Vec<ProfileState> = profile_dirs
            .into_iter()
            .map(|dir| {
                let expanded = expand_tilde(&dir);
                ProfileState::new(PathBuf::from(expanded))
            })
            .collect();

        let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
        info!(profiles = ?names, "Multi-profile Claude driver initialized");

        Self {
            cli_path: cli_path
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "claude".to_string()),
            skip_permissions,
            profiles: Arc::new(Mutex::new(profiles)),
            current: Arc::new(Mutex::new(0)),
        }
    }

    /// Pick the next available (non-cooldown) profile index.
    /// Returns `None` if all profiles are in cooldown.
    async fn next_available(&self) -> Option<(usize, PathBuf)> {
        let profiles = self.profiles.lock().await;
        let count = profiles.len();
        let current = *self.current.lock().await;

        // Try from current, then wrap around
        for offset in 0..count {
            let idx = (current + offset) % count;
            if profiles[idx].is_available() {
                return Some((idx, profiles[idx].config_dir.clone()));
            }
        }
        None
    }

    /// Mark a profile as rate-limited, querying the API for the exact
    /// cooldown duration.
    async fn handle_rate_limit(&self, profile_idx: usize) {
        let mut profiles = self.profiles.lock().await;
        let profile = &mut profiles[profile_idx];

        // Try to get exact reset time from API
        let cooldown_until = if let Some(token) = profile.load_token() {
            let token = token.to_string();
            // Drop lock before the async call
            drop(profiles);

            match fetch_usage(&token).await {
                Ok(usage) => {
                    info!(
                        profile_idx,
                        five_hour_pct = usage.five_hour_utilization,
                        seven_day_pct = usage.seven_day_utilization,
                        resets_at = ?usage.five_hour_resets_at,
                        "Usage API response"
                    );
                    usage
                        .five_hour_resets_at
                        .map(utc_to_instant)
                        .unwrap_or_else(|| {
                            std::time::Instant::now()
                                + std::time::Duration::from_secs(FALLBACK_COOLDOWN_SECS)
                        })
                }
                Err(e) => {
                    warn!(error = %e, "Failed to fetch usage — using fallback cooldown");
                    std::time::Instant::now()
                        + std::time::Duration::from_secs(FALLBACK_COOLDOWN_SECS)
                }
            }
        } else {
            drop(profiles);
            std::time::Instant::now()
                + std::time::Duration::from_secs(FALLBACK_COOLDOWN_SECS)
        };

        // Re-acquire lock and set cooldown
        let mut profiles = self.profiles.lock().await;
        profiles[profile_idx].set_cooldown(cooldown_until);

        // Advance current to next available
        let count = profiles.len();
        for offset in 1..count {
            let next = (profile_idx + offset) % count;
            if profiles[next].is_available() {
                *self.current.lock().await = next;
                info!(
                    from = profiles[profile_idx].name,
                    to = profiles[next].name,
                    "Rotated to next profile"
                );
                return;
            }
        }
        warn!("All profiles are in cooldown — requests will fail until a window resets");
    }

    /// Check if an error is a rate-limit error.
    fn is_rate_limit_error(err: &LlmError) -> bool {
        match err {
            LlmError::Api { status, message } => {
                *status == 429
                    || message.to_lowercase().contains("rate limit")
                    || message.to_lowercase().contains("quota")
                    || message.to_lowercase().contains("too many requests")
                    || message.to_lowercase().contains("usage limit")
                    || message.to_lowercase().contains("overloaded")
            }
            LlmError::Http(msg) => {
                let lower = msg.to_lowercase();
                lower.contains("rate limit")
                    || lower.contains("429")
                    || lower.contains("quota")
                    || lower.contains("usage limit")
            }
            _ => false,
        }
    }

}

#[async_trait]
impl LlmDriver for MultiProfileDriver {
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, LlmError> {
        let profiles_count = self.profiles.lock().await.len();

        // Try each available profile
        for _attempt in 0..profiles_count {
            let (idx, config_dir) = match self.next_available().await {
                Some(pair) => pair,
                None => {
                    return Err(LlmError::Api {
                        status: 429,
                        message: "All Claude Code profiles are rate-limited. \
                                  Requests will resume when a rate-limit window resets."
                            .to_string(),
                    });
                }
            };

            let profile_name = {
                let profiles = self.profiles.lock().await;
                profiles[idx].name.clone()
            };

            debug!(profile = %profile_name, config_dir = %config_dir.display(), "Using profile for completion");

            let result = complete_with_config_dir(
                &self.cli_path,
                self.skip_permissions,
                &config_dir,
                request.clone(),
            )
            .await;

            match &result {
                Ok(_) => {
                    // Success — clear any stale cooldown
                    let mut profiles = self.profiles.lock().await;
                    profiles[idx].clear_cooldown();
                    *self.current.lock().await = idx;
                    return result;
                }
                Err(e) if Self::is_rate_limit_error(e) => {
                    warn!(profile = %profile_name, error = %e, "Rate limit hit — rotating");
                    self.handle_rate_limit(idx).await;
                    continue;
                }
                Err(_) => {
                    // Non-rate-limit error — don't rotate, just return
                    return result;
                }
            }
        }

        Err(LlmError::Api {
            status: 429,
            message: "All Claude Code profiles exhausted after rotation attempts".to_string(),
        })
    }

    async fn stream(
        &self,
        request: CompletionRequest,
        tx: tokio::sync::mpsc::Sender<StreamEvent>,
    ) -> Result<CompletionResponse, LlmError> {
        let profiles_count = self.profiles.lock().await.len();

        for _attempt in 0..profiles_count {
            let (idx, config_dir) = match self.next_available().await {
                Some(pair) => pair,
                None => {
                    return Err(LlmError::Api {
                        status: 429,
                        message: "All Claude Code profiles are rate-limited. \
                                  Requests will resume when a rate-limit window resets."
                            .to_string(),
                    });
                }
            };

            let profile_name = {
                let profiles = self.profiles.lock().await;
                profiles[idx].name.clone()
            };

            debug!(profile = %profile_name, "Using profile for streaming");

            let result = stream_with_config_dir(
                &self.cli_path,
                self.skip_permissions,
                &config_dir,
                request.clone(),
                tx.clone(),
            )
            .await;

            match &result {
                Ok(_) => {
                    let mut profiles = self.profiles.lock().await;
                    profiles[idx].clear_cooldown();
                    *self.current.lock().await = idx;
                    return result;
                }
                Err(e) if Self::is_rate_limit_error(e) => {
                    warn!(profile = %profile_name, error = %e, "Rate limit hit — rotating");
                    self.handle_rate_limit(idx).await;
                    continue;
                }
                Err(_) => return result,
            }
        }

        Err(LlmError::Api {
            status: 429,
            message: "All Claude Code profiles exhausted after rotation attempts".to_string(),
        })
    }
}

// ── Helpers: spawn claude CLI with CLAUDE_CONFIG_DIR ──────────────────────

/// Run a non-streaming completion using the Claude CLI with a specific config dir.
async fn complete_with_config_dir(
    cli_path: &str,
    skip_permissions: bool,
    config_dir: &Path,
    request: CompletionRequest,
) -> Result<CompletionResponse, LlmError> {
    use super::claude_code::ClaudeCodeDriver;
    use openfang_types::message::{ContentBlock, StopReason, TokenUsage};

    let driver = ClaudeCodeDriver::new(Some(cli_path.to_string()), skip_permissions);
    let image_files = ClaudeCodeDriver::extract_images_to_temp(&request).await;
    let prompt = ClaudeCodeDriver::build_prompt(&request, &image_files);
    let model_flag = ClaudeCodeDriver::model_flag(&request.model);
    let args = driver.build_args(&prompt, model_flag.as_deref(), false);

    let mut cmd = tokio::process::Command::new(cli_path);
    cmd.args(&args);
    cmd.env("CLAUDE_CONFIG_DIR", config_dir);
    ClaudeCodeDriver::apply_env_filter(&mut cmd);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    debug!(config_dir = %config_dir.display(), "Spawning Claude CLI with profile");

    let output = cmd.output().await.map_err(|e| {
        LlmError::Http(format!("Claude Code CLI failed to start: {e}"))
    })?;

    ClaudeCodeDriver::cleanup_temp_images(&image_files);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { &stderr } else { &stdout };
        let code = output.status.code().unwrap_or(1);

        return Err(LlmError::Api {
            status: code as u16,
            message: format!("Claude Code CLI exited with code {code}: {detail}"),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Try JSON parse
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&stdout) {
        let text = parsed
            .get("result")
            .or_else(|| parsed.get("content"))
            .or_else(|| parsed.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        let (input_tokens, output_tokens) = parsed
            .get("usage")
            .map(|u| {
                (
                    u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                )
            })
            .unwrap_or((0, 0));

        return Ok(CompletionResponse {
            content: vec![ContentBlock::Text {
                text,
                provider_metadata: None,
            }],
            stop_reason: StopReason::EndTurn,
            tool_calls: Vec::new(),
            usage: TokenUsage {
                input_tokens,
                output_tokens,
            },
        });
    }

    // Fallback: plain text
    Ok(CompletionResponse {
        content: vec![ContentBlock::Text {
            text: stdout.trim().to_string(),
            provider_metadata: None,
        }],
        stop_reason: StopReason::EndTurn,
        tool_calls: Vec::new(),
        usage: TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
        },
    })
}

/// Run a streaming completion using the Claude CLI with a specific config dir.
async fn stream_with_config_dir(
    cli_path: &str,
    skip_permissions: bool,
    config_dir: &Path,
    request: CompletionRequest,
    tx: tokio::sync::mpsc::Sender<StreamEvent>,
) -> Result<CompletionResponse, LlmError> {
    use super::claude_code::ClaudeCodeDriver;
    use openfang_types::message::{ContentBlock, StopReason, TokenUsage};
    use tokio::io::AsyncBufReadExt;

    let driver = ClaudeCodeDriver::new(Some(cli_path.to_string()), skip_permissions);
    let image_files = ClaudeCodeDriver::extract_images_to_temp(&request).await;
    let prompt = ClaudeCodeDriver::build_prompt(&request, &image_files);
    let model_flag = ClaudeCodeDriver::model_flag(&request.model);
    let args = driver.build_args(&prompt, model_flag.as_deref(), true);

    let mut cmd = tokio::process::Command::new(cli_path);
    cmd.args(&args);
    cmd.env("CLAUDE_CONFIG_DIR", config_dir);
    ClaudeCodeDriver::apply_env_filter(&mut cmd);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| {
        LlmError::Http(format!("Claude Code CLI failed to start: {e}"))
    })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| LlmError::Http("No stdout from claude CLI".to_string()))?;

    let reader = tokio::io::BufReader::new(stdout);
    let mut lines = reader.lines();

    let mut full_text = String::new();
    let mut final_usage = TokenUsage {
        input_tokens: 0,
        output_tokens: 0,
    };

    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) {
            let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");

            match event_type {
                "content" | "text" | "assistant" | "content_block_delta" => {
                    if let Some(content) = event.get("content").and_then(|v| v.as_str()) {
                        full_text.push_str(content);
                        let _ = tx.send(StreamEvent::TextDelta { text: content.to_string() }).await;
                    }
                }
                "result" | "done" | "complete" => {
                    if let Some(result) = event.get("result").and_then(|v| v.as_str()) {
                        if full_text.is_empty() {
                            full_text = result.to_string();
                            let _ = tx.send(StreamEvent::TextDelta { text: result.to_string() }).await;
                        }
                    }
                    if let Some(usage) = event.get("usage") {
                        final_usage = TokenUsage {
                            input_tokens: usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                            output_tokens: usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                        };
                    }
                }
                _ => {
                    if let Some(content) = event.get("content").and_then(|v| v.as_str()) {
                        full_text.push_str(content);
                        let _ = tx.send(StreamEvent::TextDelta { text: content.to_string() }).await;
                    }
                }
            }
        } else {
            full_text.push_str(&line);
            let _ = tx.send(StreamEvent::TextDelta { text: line }).await;
        }
    }

    let status = child.wait().await.map_err(|e| {
        LlmError::Http(format!("Claude CLI wait failed: {e}"))
    })?;

    ClaudeCodeDriver::cleanup_temp_images(&image_files);

    if !status.success() {
        let code = status.code().unwrap_or(1);
        // Check if the text we got so far indicates rate limiting
        let lower = full_text.to_lowercase();
        if lower.contains("rate limit") || lower.contains("quota") || lower.contains("usage limit") {
            return Err(LlmError::Api {
                status: 429,
                message: full_text,
            });
        }
        if full_text.is_empty() {
            return Err(LlmError::Api {
                status: code as u16,
                message: format!("Claude Code CLI exited with code {code}"),
            });
        }
    }

    let _ = tx
        .send(StreamEvent::ContentComplete {
            stop_reason: StopReason::EndTurn,
            usage: final_usage,
        })
        .await;

    Ok(CompletionResponse {
        content: vec![ContentBlock::Text {
            text: full_text,
            provider_metadata: None,
        }],
        stop_reason: StopReason::EndTurn,
        tool_calls: Vec::new(),
        usage: final_usage,
    })
}

/// Expand a leading `~` to the user's home directory.
fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    } else if path == "~" {
        if let Ok(home) = std::env::var("HOME") {
            return home;
        }
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_state_new() {
        let p = ProfileState::new(PathBuf::from("/home/user/.claude"));
        assert_eq!(p.name, ".claude");
        assert!(p.is_available());
        assert!(p.access_token.is_none());
    }

    #[test]
    fn test_profile_cooldown() {
        let mut p = ProfileState::new(PathBuf::from("/tmp/test-profile"));
        assert!(p.is_available());

        // Set cooldown 1 hour from now
        let future = std::time::Instant::now() + std::time::Duration::from_secs(3600);
        p.set_cooldown(future);
        assert!(!p.is_available());

        // Set cooldown in the past
        let past = std::time::Instant::now() - std::time::Duration::from_secs(1);
        p.cooldown_until = Some(past);
        assert!(p.is_available());

        // Clear cooldown
        p.set_cooldown(future);
        p.clear_cooldown();
        assert!(p.is_available());
    }

    #[test]
    fn test_utc_to_instant_future() {
        let future = chrono::Utc::now() + chrono::Duration::hours(3);
        let instant = utc_to_instant(future);
        // Should be roughly 3 hours + 30s buffer from now
        let expected_secs = 3 * 3600 + 30;
        let actual_secs = instant.duration_since(std::time::Instant::now()).as_secs();
        assert!(actual_secs >= expected_secs - 5 && actual_secs <= expected_secs + 5);
    }

    #[test]
    fn test_utc_to_instant_past() {
        let past = chrono::Utc::now() - chrono::Duration::hours(1);
        let instant = utc_to_instant(past);
        // Should be approximately now
        let diff = instant
            .saturating_duration_since(std::time::Instant::now())
            .as_secs();
        assert!(diff <= 1);
    }

    #[test]
    fn test_is_rate_limit_error() {
        assert!(MultiProfileDriver::is_rate_limit_error(&LlmError::Api {
            status: 429,
            message: "Too many requests".to_string(),
        }));

        assert!(MultiProfileDriver::is_rate_limit_error(&LlmError::Api {
            status: 1,
            message: "You have exceeded your rate limit".to_string(),
        }));

        assert!(MultiProfileDriver::is_rate_limit_error(
            &LlmError::Http("Error 429: quota exceeded".to_string())
        ));

        assert!(!MultiProfileDriver::is_rate_limit_error(&LlmError::Api {
            status: 500,
            message: "Internal server error".to_string(),
        }));

        assert!(!MultiProfileDriver::is_rate_limit_error(
            &LlmError::Http("Connection refused".to_string())
        ));
    }

    #[test]
    fn test_multi_profile_driver_new() {
        let driver = MultiProfileDriver::new(
            None,
            true,
            vec![
                "~/.claude".to_string(),
                "~/.claude-profiles/account-2".to_string(),
            ],
        );
        assert_eq!(driver.cli_path, "claude");
        assert!(driver.skip_permissions);
    }

    #[test]
    fn test_load_token_from_credentials() {
        // Create a temp credentials file
        let dir = std::env::temp_dir().join("openfang-test-profile");
        std::fs::create_dir_all(&dir).unwrap();
        let cred_path = dir.join(".credentials.json");
        std::fs::write(
            &cred_path,
            r#"{"claudeAiOauth":{"accessToken":"test-token-123","refreshToken":"rt","expiresAt":9999999999}}"#,
        )
        .unwrap();

        let mut profile = ProfileState::new(dir.clone());
        let token = profile.load_token();
        assert_eq!(token, Some("test-token-123"));

        // Cleanup
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn test_next_available_all_in_cooldown() {
        let driver = MultiProfileDriver::new(
            None,
            true,
            vec!["/tmp/p1".to_string(), "/tmp/p2".to_string()],
        );

        // Put all in cooldown
        {
            let mut profiles = driver.profiles.lock().await;
            let future = std::time::Instant::now() + std::time::Duration::from_secs(3600);
            profiles[0].set_cooldown(future);
            profiles[1].set_cooldown(future);
        }

        assert!(driver.next_available().await.is_none());
    }

    #[tokio::test]
    async fn test_next_available_skips_cooldown() {
        let driver = MultiProfileDriver::new(
            None,
            true,
            vec![
                "/tmp/pa".to_string(),
                "/tmp/pb".to_string(),
                "/tmp/pc".to_string(),
            ],
        );

        // Put first profile in cooldown
        {
            let mut profiles = driver.profiles.lock().await;
            let future = std::time::Instant::now() + std::time::Duration::from_secs(3600);
            profiles[0].set_cooldown(future);
        }

        let (idx, _) = driver.next_available().await.unwrap();
        assert_eq!(idx, 1); // Should skip index 0
    }

    // ── expand_tilde tests ───────────────────────────────────────────

    #[test]
    fn test_expand_tilde_home() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(expand_tilde("~/.claude"), format!("{home}/.claude"));
        assert_eq!(
            expand_tilde("~/.claude-profiles/acct2"),
            format!("{home}/.claude-profiles/acct2")
        );
    }

    #[test]
    fn test_expand_tilde_bare() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(expand_tilde("~"), home);
    }

    #[test]
    fn test_expand_tilde_absolute_passthrough() {
        assert_eq!(expand_tilde("/etc/claude"), "/etc/claude");
        assert_eq!(expand_tilde("/tmp/profile"), "/tmp/profile");
    }

    #[test]
    fn test_expand_tilde_no_prefix() {
        assert_eq!(expand_tilde("relative/path"), "relative/path");
        assert_eq!(expand_tilde(""), "");
    }

    // ── fetch_usage parsing tests ────────────────────────────────────

    #[test]
    fn test_parse_usage_response() {
        let json = r#"{
            "five_hour": {
                "utilization": 66.0,
                "resets_at": "2026-03-14T05:00:00Z"
            },
            "seven_day": {
                "utilization": 14.0,
                "resets_at": "2026-03-20T05:00:00Z"
            }
        }"#;

        let resp: UsageResponse = serde_json::from_str(json).unwrap();
        let five = resp.five_hour.unwrap();
        assert!((five.utilization - 66.0).abs() < 0.1);
        assert_eq!(five.resets_at.as_deref(), Some("2026-03-14T05:00:00Z"));

        let seven = resp.seven_day.unwrap();
        assert!((seven.utilization - 14.0).abs() < 0.1);
    }

    #[test]
    fn test_parse_usage_response_empty_windows() {
        let json = r#"{}"#;
        let resp: UsageResponse = serde_json::from_str(json).unwrap();
        assert!(resp.five_hour.is_none());
        assert!(resp.seven_day.is_none());
    }

    #[test]
    fn test_parse_usage_response_partial() {
        let json = r#"{"five_hour": {"utilization": 99.5}}"#;
        let resp: UsageResponse = serde_json::from_str(json).unwrap();
        let five = resp.five_hour.unwrap();
        assert!((five.utilization - 99.5).abs() < 0.1);
        assert!(five.resets_at.is_none()); // no resets_at field
        assert!(resp.seven_day.is_none());
    }

    // ── Profile rotation logic tests ─────────────────────────────────

    #[tokio::test]
    async fn test_next_available_wraps_around() {
        // If current=2 and profile 2 is in cooldown, should wrap to 0
        let driver = MultiProfileDriver::new(
            None,
            true,
            vec![
                "/tmp/x0".to_string(),
                "/tmp/x1".to_string(),
                "/tmp/x2".to_string(),
            ],
        );

        // Set current to 2, put profile 2 in cooldown
        {
            *driver.current.lock().await = 2;
            let mut profiles = driver.profiles.lock().await;
            let future = std::time::Instant::now() + std::time::Duration::from_secs(3600);
            profiles[2].set_cooldown(future);
        }

        let (idx, _) = driver.next_available().await.unwrap();
        assert_eq!(idx, 0); // Wrapped around to 0
    }

    #[tokio::test]
    async fn test_next_available_prefers_current() {
        // If current profile is available, it should be returned
        let driver = MultiProfileDriver::new(
            None,
            true,
            vec!["/tmp/y0".to_string(), "/tmp/y1".to_string()],
        );

        *driver.current.lock().await = 1;

        let (idx, _) = driver.next_available().await.unwrap();
        assert_eq!(idx, 1); // Should prefer current
    }

    #[tokio::test]
    async fn test_expired_cooldown_becomes_available() {
        let driver = MultiProfileDriver::new(
            None,
            true,
            vec!["/tmp/z0".to_string(), "/tmp/z1".to_string()],
        );

        // Put profile 0 in cooldown that has already expired
        {
            let mut profiles = driver.profiles.lock().await;
            profiles[0].cooldown_until =
                Some(std::time::Instant::now() - std::time::Duration::from_secs(1));
        }

        let (idx, _) = driver.next_available().await.unwrap();
        assert_eq!(idx, 0); // Expired cooldown = available
    }

    // ── Credentials file tests ───────────────────────────────────────

    #[test]
    fn test_load_token_alternate_filename() {
        // Test credentials.json (without leading dot)
        let dir = std::env::temp_dir().join("openfang-test-profile-alt");
        std::fs::create_dir_all(&dir).unwrap();
        let cred_path = dir.join("credentials.json");
        std::fs::write(
            &cred_path,
            r#"{"claudeAiOauth":{"accessToken":"alt-token-456"}}"#,
        )
        .unwrap();

        let mut profile = ProfileState::new(dir.clone());
        let token = profile.load_token();
        assert_eq!(token, Some("alt-token-456"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_load_token_missing_dir() {
        let mut profile = ProfileState::new(PathBuf::from("/nonexistent/path"));
        assert!(profile.load_token().is_none());
    }

    #[test]
    fn test_load_token_malformed_json() {
        let dir = std::env::temp_dir().join("openfang-test-profile-bad");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".credentials.json"), "not json").unwrap();

        let mut profile = ProfileState::new(dir.clone());
        assert!(profile.load_token().is_none());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_load_token_missing_oauth_field() {
        let dir = std::env::temp_dir().join("openfang-test-profile-nooauth");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(".credentials.json"),
            r#"{"someOtherField": "value"}"#,
        )
        .unwrap();

        let mut profile = ProfileState::new(dir.clone());
        assert!(profile.load_token().is_none());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_load_token_cached_on_second_call() {
        let dir = std::env::temp_dir().join("openfang-test-profile-cache");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(".credentials.json"),
            r#"{"claudeAiOauth":{"accessToken":"cached-tok"}}"#,
        )
        .unwrap();

        let mut profile = ProfileState::new(dir.clone());
        assert_eq!(profile.load_token(), Some("cached-tok"));

        // Remove the file — second call should still return cached token
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(profile.load_token(), Some("cached-tok"));
    }

    // ── Rate-limit error detection edge cases ────────────────────────

    #[test]
    fn test_is_rate_limit_overloaded() {
        assert!(MultiProfileDriver::is_rate_limit_error(&LlmError::Api {
            status: 529,
            message: "API is overloaded".to_string(),
        }));
    }

    #[test]
    fn test_is_rate_limit_usage_limit_in_http() {
        assert!(MultiProfileDriver::is_rate_limit_error(
            &LlmError::Http("Your usage limit has been reached".to_string())
        ));
    }

    #[test]
    fn test_is_not_rate_limit_auth_error() {
        assert!(!MultiProfileDriver::is_rate_limit_error(&LlmError::Api {
            status: 401,
            message: "Unauthorized".to_string(),
        }));
    }

    // ── DriverConfig profiles field ──────────────────────────────────

    #[test]
    fn test_driver_config_profiles_default_empty() {
        let json = r#"{"provider":"claude-code"}"#;
        let config: crate::llm_driver::DriverConfig = serde_json::from_str(json).unwrap();
        assert!(config.profiles.is_empty());
    }

    #[test]
    fn test_driver_config_profiles_deserialized() {
        let json = r#"{
            "provider": "claude-code",
            "profiles": ["~/.claude", "~/.claude-profiles/acct2"]
        }"#;
        let config: crate::llm_driver::DriverConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.profiles.len(), 2);
        assert_eq!(config.profiles[0], "~/.claude");
        assert_eq!(config.profiles[1], "~/.claude-profiles/acct2");
    }
}
