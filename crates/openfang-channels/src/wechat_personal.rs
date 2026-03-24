//! WeChat Personal (Web) channel adapter.
//!
//! Uses the WeChat Web API (wx.qq.com) for personal account integration.
//! This adapter simulates the WeChat Web client to receive and send messages.
//! Requires scanning a QR code to authenticate.
//!
//! **Security Note**: This adapter uses the unofficial WeChat Web API which
//! may violate WeChat's Terms of Service. Use at your own risk.

use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use zeroize::Zeroizing;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::Stream;
use serde::{Deserialize, Serialize};

use crate::types::{ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser};

/// WeChat adapter error types.
#[derive(Debug, Clone)]
pub enum WeChatError {
    /// Network-level error (connection failed, timeout, etc.).
    NetworkError(String),
    /// Session expired, requires re-authentication.
    SessionExpired,
    /// Rate limited by WeChat API.
    RateLimited { retry_after_ms: u64 },
    /// WeChat API returned an error.
    ApiError { code: i32, message: String },
    /// Invalid or unexpected response from API.
    InvalidResponse(String),
}

impl std::fmt::Display for WeChatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WeChatError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            WeChatError::SessionExpired => write!(f, "Session expired, re-authentication required"),
            WeChatError::RateLimited { retry_after_ms } => {
                write!(f, "Rate limited, retry after {} ms", retry_after_ms)
            }
            WeChatError::ApiError { code, message } => {
                write!(f, "API error (code {}): {}", code, message)
            }
            WeChatError::InvalidResponse(msg) => write!(f, "Invalid response: {}", msg),
        }
    }
}

impl std::error::Error for WeChatError {}

impl WeChatError {
    /// Returns true if the error is retryable.
    ///
    /// Retryable errors:
    /// - NetworkError: transient network issues
    /// - RateLimited: should retry after the specified delay
    /// - ApiError: only certain error codes are retryable (e.g., 500, 502, 503, 504)
    ///
    /// Non-retryable errors:
    /// - SessionExpired: requires re-authentication
    /// - InvalidResponse: indicates a protocol mismatch or bug
    pub fn is_retryable(&self) -> bool {
        match self {
            WeChatError::NetworkError(_) => true,
            WeChatError::SessionExpired => false,
            WeChatError::RateLimited { .. } => true,
            WeChatError::ApiError { code, .. } => matches!(code, 500 | 502 | 503 | 504),
            WeChatError::InvalidResponse(_) => false,
        }
    }
}

/// Calculate exponential backoff delay for retries.
///
/// # Arguments
/// * `attempt` - The current retry attempt (0-indexed)
/// * `base_delay_ms` - The base delay in milliseconds
/// * `max_delay_ms` - The maximum delay cap in milliseconds
///
/// # Returns
/// The calculated delay in milliseconds
///
/// # Example
/// ```
/// let delay = calculate_backoff(0, 1000, 30000); // Returns 1000
/// let delay = calculate_backoff(1, 1000, 30000); // Returns 2000
/// let delay = calculate_backoff(2, 1000, 30000); // Returns 4000
/// ```
pub fn calculate_backoff(attempt: u32, base_delay_ms: u64, max_delay_ms: u64) -> u64 {
    let delay = base_delay_ms.saturating_mul(2_u64.saturating_pow(attempt));
    delay.min(max_delay_ms)
}

/// Default WeChat Web API base URL.
const DEFAULT_BASE_URL: &str = "https://wx.qq.com/cgi-bin/mmwebwx-bin";
/// Long-polling timeout for message sync (35 seconds).
const DEFAULT_LONG_POLL_TIMEOUT_MS: u64 = 35_000;
/// Maximum consecutive failures before backing off.
const MAX_CONSECUTIVE_FAILURES: u32 = 3;
/// Backoff delay after max failures (30 seconds).
const BACKOFF_DELAY_MS: u64 = 30_000;
/// Retry delay between failed requests (2 seconds).
const RETRY_DELAY_MS: u64 = 2_000;

/// WeChat message content item types.
#[derive(Debug, Clone)]
pub enum WeChatMessageItem {
    /// Plain text message.
    Text { content: String },
    /// Image message with URL.
    Image {
        url: String,
        thumb_url: Option<String>,
    },
    /// Voice message with URL and duration.
    Voice { url: String, duration_seconds: u32 },
    /// Video message with URL and thumbnail.
    Video {
        url: String,
        thumb_url: Option<String>,
    },
    /// File attachment.
    File {
        url: String,
        filename: String,
        size: u64,
    },
    /// Location sharing.
    Location {
        lat: f64,
        lon: f64,
        label: Option<String>,
    },
    /// System/notification message.
    System { content: String },
    /// Unknown/unhandled message type.
    Unknown {
        type_code: i32,
        raw: serde_json::Value,
    },
}

/// QR login result structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrLoginResult {
    /// QR code as a data URL (base64 encoded PNG).
    pub qr_data_url: String,
    /// Session key for tracking the login process.
    pub session_key: String,
    /// Status message for the login attempt.
    pub message: String,
}

/// Login wait result structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginWaitResult {
    /// Whether the login was successful.
    pub connected: bool,
    /// The account ID if login succeeded.
    pub account_id: Option<String>,
    /// Status message.
    pub message: String,
}

/// Logout result structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogoutResult {
    /// Whether the logout was successful.
    pub cleared: bool,
    /// Status message.
    pub message: String,
}

/// WeChat message structure from the Web API.
#[derive(Debug, Clone)]
pub struct WeChatMessage {
    /// Unique message ID from WeChat.
    pub message_id: u64,
    /// Sender's WeChat ID (username).
    pub from_user_id: String,
    /// Recipient's WeChat ID (username).
    pub to_user_id: String,
    /// Message creation timestamp in milliseconds.
    pub create_time_ms: i64,
    /// Context token for conversation threading.
    pub context_token: Option<String>,
    /// List of message content items (for composite messages).
    pub item_list: Vec<WeChatMessageItem>,
    /// Raw message type from WeChat API.
    pub msg_type: i32,
    /// Additional metadata from the raw message.
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Configuration for the WeChat Personal adapter.
#[derive(Debug, Clone)]
pub struct WeChatPersonalConfig {
    /// WeChat account identifier (display name or phone).
    pub account_id: String,
    /// Optional base URL override for the WeChat Web API.
    pub base_url: Option<String>,
    /// Optional CDN base URL for media files.
    pub cdn_base_url: Option<String>,
    /// Long-polling timeout in milliseconds.
    pub long_poll_timeout_ms: u64,
    /// Maximum consecutive failures before backing off.
    pub max_consecutive_failures: u32,
    /// Backoff delay in milliseconds.
    pub backoff_delay_ms: u64,
    /// Retry delay in milliseconds.
    pub retry_delay_ms: u64,
}

impl Default for WeChatPersonalConfig {
    fn default() -> Self {
        Self {
            account_id: String::new(),
            base_url: None,
            cdn_base_url: None,
            long_poll_timeout_ms: DEFAULT_LONG_POLL_TIMEOUT_MS,
            max_consecutive_failures: MAX_CONSECUTIVE_FAILURES,
            backoff_delay_ms: BACKOFF_DELAY_MS,
            retry_delay_ms: RETRY_DELAY_MS,
        }
    }
}

/// Session data structure for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionData {
    account_id: String,
    token: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl SessionData {
    fn new(account_id: String, token: String) -> Self {
        let now = Utc::now();
        Self {
            account_id,
            token,
            created_at: now,
            updated_at: now,
        }
    }
}

/// WeChat Personal adapter using the Web API.
///
/// This adapter connects to WeChat's web interface to send and receive
/// messages from a personal WeChat account. It requires QR code authentication
/// and maintains a persistent session.
pub struct WeChatPersonalAdapter {
    /// Account identifier for this WeChat connection.
    account_id: String,
    /// HTTP client for API requests.
    client: reqwest::Client,
    /// Session token (zeroized on drop for security).
    token: Option<Zeroizing<String>>,
    /// Shutdown signal sender.
    shutdown_tx: Arc<watch::Sender<bool>>,
    /// Shutdown signal receiver.
    shutdown_rx: watch::Receiver<bool>,
    /// Base URL for WeChat Web API.
    base_url: String,
    /// Optional CDN base URL for media files.
    cdn_base_url: Option<String>,
    /// Consecutive failure counter for backoff logic.
    consecutive_failures: Arc<std::sync::atomic::AtomicU32>,
}

impl WeChatPersonalAdapter {
    /// Create a new WeChat Personal adapter with the given configuration.
    pub fn new(config: WeChatPersonalConfig) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let base_url = config
            .base_url
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

        Self {
            account_id: config.account_id,
            client: reqwest::Client::new(),
            token: None,
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            base_url,
            cdn_base_url: config.cdn_base_url,
            consecutive_failures: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        }
    }

    /// Create a new adapter with explicit account ID (uses default config otherwise).
    pub fn with_account_id(account_id: String) -> Self {
        let mut config = WeChatPersonalConfig::default();
        config.account_id = account_id;
        Self::new(config)
    }

    /// Check if the adapter has a valid session token.
    pub fn is_authenticated(&self) -> bool {
        self.token.is_some()
    }

    /// Get the account ID for this adapter.
    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    /// Set the session token for this adapter.
    /// The token is wrapped in Zeroizing<String> for secure memory handling.
    pub fn set_token(&mut self, token: String) {
        self.token = Some(Zeroizing::new(token));
    }

    /// Get the current session token if one is set.
    /// Returns None if no token has been set.
    pub fn get_token(&self) -> Option<String> {
        self.token.as_ref().map(|t| t.to_string())
    }

    /// Clear the session token from this adapter.
    /// The token will be zeroized from memory automatically.
    pub fn clear_token(&mut self) {
        self.token = None;
    }

    /// Start the QR code login process.
    ///
    /// Returns a QR code data URL and session key for tracking the login.
    /// This is a placeholder implementation - actual WeChat API integration is TODO.
    pub async fn login_start(&self) -> Result<QrLoginResult, WeChatError> {
        // TODO: Implement actual WeChat QR code generation API call
        // This would:
        // 1. Call WeChat API to get UUID for QR code
        // 2. Generate QR code image
        // 3. Return data URL and session key

        let session_key = format!("wx_session_{}", uuid::Uuid::new_v4());

        // Placeholder QR code (a simple base64 PNG)
        let qr_data_url = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==".to_string();

        tracing::info!(session_key = %session_key, "QR login started (placeholder)");

        Ok(QrLoginResult {
            qr_data_url,
            session_key,
            message: "Please scan the QR code with WeChat".to_string(),
        })
    }

    /// Wait for the user to scan the QR code and confirm login.
    ///
    /// Polls the WeChat API for login status until:
    /// - User scans and confirms (returns connected=true)
    /// - Timeout expires (returns connected=false)
    /// - Error occurs
    ///
    /// This is a placeholder implementation - actual WeChat API integration is TODO.
    pub async fn login_wait(&self, session_key: &str, timeout_ms: u64) -> Result<LoginWaitResult, WeChatError> {
        // TODO: Implement actual WeChat login status polling
        // This would:
        // 1. Poll WeChat API for login status using session_key
        // 2. Check if user scanned QR code
        // 3. Wait for confirmation
        // 4. Extract account info and token on success

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(timeout_ms);

        tracing::info!(session_key = %session_key, timeout_ms = timeout_ms, "Waiting for QR scan confirmation (placeholder)");

        // Placeholder: simulate polling with timeout
        loop {
            if start.elapsed() > timeout {
                return Ok(LoginWaitResult {
                    connected: false,
                    account_id: None,
                    message: "Login timeout - QR code expired".to_string(),
                });
            }

            // TODO: Replace with actual API polling
            // For now, return timeout to simulate unimplemented behavior
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            return Ok(LoginWaitResult {
                connected: false,
                account_id: None,
                message: "Login timeout - QR code expired".to_string(),
            });
        }
    }

    /// Logout and clear the session.
    ///
    /// Clears the local token and optionally notifies the WeChat API.
    /// Returns a result indicating whether cleanup was successful.
    pub async fn logout(&mut self) -> LogoutResult {
        let had_token = self.token.is_some();

        // Clear local token
        self.clear_token();

        // TODO: Implement actual WeChat logout API call
        // This would notify WeChat servers to invalidate the session
        tracing::info!(account_id = %self.account_id, had_token = had_token, "Logout completed (placeholder)");

        LogoutResult {
            cleared: had_token,
            message: if had_token {
                "Successfully logged out".to_string()
            } else {
                "No active session to clear".to_string()
            },
        }
    }

    /// Get the session file path for this adapter.
    /// Path format: `{data_dir}/wechat/{account_id}.json`
    pub fn get_session_path(&self, data_dir: &std::path::Path) -> PathBuf {
        data_dir.join("wechat").join(format!("{}.json", self.account_id))
    }

    /// Save the current session to a file.
    /// Returns the path to the saved session file.
    pub async fn save_session(&self, data_dir: &std::path::Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let session_path = self.get_session_path(data_dir);
        
        // Create parent directory if it doesn't exist
        if let Some(parent) = session_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        
        // Get token if available
        let token = match &self.token {
            Some(t) => t.to_string(),
            None => return Err("No session token to save".into()),
        };
        
        // Create session data
        let session_data = SessionData::new(self.account_id.clone(), token);
        
        // Serialize to JSON
        let json = serde_json::to_string_pretty(&session_data)?;
        
        // Write to file
        tokio::fs::write(&session_path, json).await?;
        
        Ok(session_path)
    }

    /// Load a session from a file.
    /// Returns true if a session was loaded successfully.
    pub async fn load_session(&mut self, data_dir: &std::path::Path) -> Result<bool, Box<dyn std::error::Error>> {
        let session_path = self.get_session_path(data_dir);
        
        // Check if file exists
        if !tokio::fs::try_exists(&session_path).await? {
            return Ok(false);
        }
        
        // Read file
        let json = tokio::fs::read_to_string(&session_path).await?;
        
        // Parse session data
        let session_data: SessionData = serde_json::from_str(&json)?;
        
        // Verify account_id matches
        if session_data.account_id != self.account_id {
            return Err(format!(
                "Session file account_id mismatch: expected {}, found {}",
                self.account_id, session_data.account_id
            ).into());
        }
        
        // Set token
        self.token = Some(Zeroizing::new(session_data.token));
        
        Ok(true)
    }
}

#[async_trait]
impl ChannelAdapter for WeChatPersonalAdapter {
    fn name(&self) -> &str {
        "wechat-personal"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::WeChatPersonal
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let shutdown_rx = self.shutdown_rx.clone();
        let client = self.client.clone();
        let base_url = self.base_url.clone();
        let account_id = self.account_id.clone();
        let consecutive_failures = self.consecutive_failures.clone();

        // Spawn the polling loop
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        tracing::info!("WeChat adapter shutting down");
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)) => {}
                }

                // TODO: Implement actual WeChat API polling
                // For now, this is a placeholder that doesn't make real API calls
                // The actual implementation would:
                // 1. Call getUpdates endpoint
                // 2. Parse response for new messages
                // 3. Convert WeChatMessage to ChannelMessage
                // 4. Send through tx channel
                // 5. Handle errors with exponential backoff

                tracing::debug!(
                    account_id = %account_id,
                    "WeChat polling loop iteration (placeholder)"
                );
            }
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn send(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match content {
            ChannelContent::Text(text) => {
                // TODO: Implement actual WeChat message sending
                // For now, this is a placeholder
                tracing::info!(
                    account_id = %self.account_id,
                    to = %user.platform_id,
                    text_len = text.len(),
                    "WeChat send message (placeholder)"
                );
                Ok(())
            }
            ChannelContent::Image { url, caption } => {
                tracing::warn!(
                    account_id = %self.account_id,
                    to = %user.platform_id,
                    url = %url,
                    "WeChat image sending not yet implemented"
                );
                Err("Image sending not implemented for WeChat Personal".into())
            }
            ChannelContent::File { url, filename } => {
                tracing::warn!(
                    account_id = %self.account_id,
                    to = %user.platform_id,
                    filename = %filename,
                    "WeChat file sending not yet implemented"
                );
                Err("File sending not implemented for WeChat Personal".into())
            }
            _ => {
                Err(format!("Unsupported content type for WeChat Personal: {:?}", content).into())
            }
        }
    }

    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.shutdown_tx.send(true);
        tracing::info!(account_id = %self.account_id, "WeChat adapter stopped");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wechat_personal_config_default() {
        let config = WeChatPersonalConfig::default();
        assert_eq!(config.account_id, "");
        assert_eq!(config.base_url, None);
        assert_eq!(config.cdn_base_url, None);
        assert_eq!(config.long_poll_timeout_ms, DEFAULT_LONG_POLL_TIMEOUT_MS);
        assert_eq!(config.max_consecutive_failures, MAX_CONSECUTIVE_FAILURES);
        assert_eq!(config.backoff_delay_ms, BACKOFF_DELAY_MS);
        assert_eq!(config.retry_delay_ms, RETRY_DELAY_MS);
    }

    #[test]
    fn test_wechat_personal_adapter_creation() {
        let config = WeChatPersonalConfig {
            account_id: "test_account".to_string(),
            ..Default::default()
        };
        let adapter = WeChatPersonalAdapter::new(config);
        assert_eq!(adapter.account_id, "test_account");
        assert_eq!(adapter.base_url, DEFAULT_BASE_URL);
        assert!(adapter.token.is_none());
    }

    #[test]
    fn test_wechat_personal_adapter_with_account_id() {
        let adapter = WeChatPersonalAdapter::with_account_id("my_account".to_string());
        assert_eq!(adapter.account_id, "my_account");
        assert_eq!(adapter.base_url, DEFAULT_BASE_URL);
    }

    #[test]
    fn test_wechat_message_item_variants() {
        let text_item = WeChatMessageItem::Text {
            content: "Hello".to_string(),
        };
        assert!(matches!(text_item, WeChatMessageItem::Text { .. }));

        let image_item = WeChatMessageItem::Image {
            url: "https://example.com/img.jpg".to_string(),
            thumb_url: None,
        };
        assert!(matches!(image_item, WeChatMessageItem::Image { .. }));
    }

    #[test]
    fn test_wechat_message_creation() {
        let msg = WeChatMessage {
            message_id: 12345,
            from_user_id: "user1".to_string(),
            to_user_id: "user2".to_string(),
            create_time_ms: 1700000000000,
            context_token: Some("token123".to_string()),
            item_list: vec![WeChatMessageItem::Text {
                content: "Test".to_string(),
            }],
            msg_type: 1,
            metadata: HashMap::new(),
        };
        assert_eq!(msg.message_id, 12345);
        assert_eq!(msg.from_user_id, "user1");
        assert_eq!(msg.to_user_id, "user2");
    }

    #[test]
    fn test_constants() {
        assert_eq!(DEFAULT_BASE_URL, "https://wx.qq.com/cgi-bin/mmwebwx-bin");
        assert_eq!(DEFAULT_LONG_POLL_TIMEOUT_MS, 35_000);
        assert_eq!(MAX_CONSECUTIVE_FAILURES, 3);
        assert_eq!(BACKOFF_DELAY_MS, 30_000);
        assert_eq!(RETRY_DELAY_MS, 2_000);
    }

    #[test]
    fn test_token_lifecycle() {
        let mut adapter = WeChatPersonalAdapter::with_account_id("test_account".to_string());

        // Initially no token
        assert!(adapter.get_token().is_none());
        assert!(!adapter.is_authenticated());

        // Set token
        adapter.set_token("session_token_123".to_string());
        assert_eq!(adapter.get_token(), Some("session_token_123".to_string()));
        assert!(adapter.is_authenticated());

        // Clear token
        adapter.clear_token();
        assert!(adapter.get_token().is_none());
        assert!(!adapter.is_authenticated());
    }

    #[test]
    fn test_token_overwrite() {
        let mut adapter = WeChatPersonalAdapter::with_account_id("test_account".to_string());

        // Set initial token
        adapter.set_token("token_old".to_string());
        assert_eq!(adapter.get_token(), Some("token_old".to_string()));

        // Overwrite with new token
        adapter.set_token("token_new".to_string());
        assert_eq!(adapter.get_token(), Some("token_new".to_string()));
    }

    #[test]
    fn test_token_empty_string() {
        let mut adapter = WeChatPersonalAdapter::with_account_id("test_account".to_string());

        // Set empty token (edge case)
        adapter.set_token("".to_string());
        assert_eq!(adapter.get_token(), Some("".to_string()));
        assert!(adapter.is_authenticated());
    }

    #[test]
    fn test_wechat_error_display() {
        let err = WeChatError::NetworkError("connection refused".to_string());
        assert_eq!(err.to_string(), "Network error: connection refused");

        let err = WeChatError::SessionExpired;
        assert_eq!(err.to_string(), "Session expired, re-authentication required");

        let err = WeChatError::RateLimited { retry_after_ms: 5000 };
        assert_eq!(err.to_string(), "Rate limited, retry after 5000 ms");

        let err = WeChatError::ApiError {
            code: 123,
            message: "invalid token".to_string(),
        };
        assert_eq!(err.to_string(), "API error (code 123): invalid token");

        let err = WeChatError::InvalidResponse("unexpected json".to_string());
        assert_eq!(err.to_string(), "Invalid response: unexpected json");
    }

    #[test]
    fn test_wechat_error_is_retryable() {
        // NetworkError is retryable
        let err = WeChatError::NetworkError("timeout".to_string());
        assert!(err.is_retryable());

        // SessionExpired is NOT retryable
        let err = WeChatError::SessionExpired;
        assert!(!err.is_retryable());

        // RateLimited is retryable
        let err = WeChatError::RateLimited { retry_after_ms: 1000 };
        assert!(err.is_retryable());

        // ApiError with retryable codes
        assert!(WeChatError::ApiError { code: 500, message: "".to_string() }.is_retryable());
        assert!(WeChatError::ApiError { code: 502, message: "".to_string() }.is_retryable());
        assert!(WeChatError::ApiError { code: 503, message: "".to_string() }.is_retryable());
        assert!(WeChatError::ApiError { code: 504, message: "".to_string() }.is_retryable());

        // ApiError with non-retryable codes
        assert!(!WeChatError::ApiError { code: 400, message: "".to_string() }.is_retryable());
        assert!(!WeChatError::ApiError { code: 401, message: "".to_string() }.is_retryable());
        assert!(!WeChatError::ApiError { code: 403, message: "".to_string() }.is_retryable());
        assert!(!WeChatError::ApiError { code: 404, message: "".to_string() }.is_retryable());

        // InvalidResponse is NOT retryable
        let err = WeChatError::InvalidResponse("parse error".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_calculate_backoff() {
        // First attempt: base delay
        assert_eq!(calculate_backoff(0, 1000, 30000), 1000);

        // Second attempt: 2x base delay
        assert_eq!(calculate_backoff(1, 1000, 30000), 2000);

        // Third attempt: 4x base delay
        assert_eq!(calculate_backoff(2, 1000, 30000), 4000);

        // Fourth attempt: 8x base delay
        assert_eq!(calculate_backoff(3, 1000, 30000), 8000);

        // Should cap at max_delay_ms
        assert_eq!(calculate_backoff(10, 1000, 30000), 30000);

        // Different base delays
        assert_eq!(calculate_backoff(0, 2000, 60000), 2000);
        assert_eq!(calculate_backoff(1, 2000, 60000), 4000);
        assert_eq!(calculate_backoff(2, 2000, 60000), 8000);

        // Edge case: max_delay_ms is 0
        assert_eq!(calculate_backoff(0, 1000, 0), 0);
    }

    #[tokio::test]
    async fn test_get_session_path() {
        let adapter = WeChatPersonalAdapter::with_account_id("test_account".to_string());
        let data_dir = std::path::Path::new("/tmp/data");
        let path = adapter.get_session_path(data_dir);
        assert_eq!(path, std::path::PathBuf::from("/tmp/data/wechat/test_account.json"));
    }

    #[tokio::test]
    async fn test_save_and_load_session() {
        let temp_dir = tempfile::tempdir().unwrap();
        let data_dir = temp_dir.path();
        
        // Create adapter and set token
        let mut adapter = WeChatPersonalAdapter::with_account_id("test_account".to_string());
        adapter.set_token("encrypted_token_123".to_string());
        
        // Save session
        let session_path = adapter.save_session(data_dir).await.unwrap();
        assert!(session_path.exists());
        
        // Verify file contents
        let json = tokio::fs::read_to_string(&session_path).await.unwrap();
        let session_data: SessionData = serde_json::from_str(&json).unwrap();
        assert_eq!(session_data.account_id, "test_account");
        assert_eq!(session_data.token, "encrypted_token_123");
        assert!(session_data.created_at <= Utc::now());
        assert!(session_data.updated_at <= Utc::now());
        
        // Create new adapter and load session
        let mut new_adapter = WeChatPersonalAdapter::with_account_id("test_account".to_string());
        assert!(!new_adapter.is_authenticated());
        
        let loaded = new_adapter.load_session(data_dir).await.unwrap();
        assert!(loaded);
        assert!(new_adapter.is_authenticated());
        assert_eq!(new_adapter.get_token(), Some("encrypted_token_123".to_string()));
    }

    #[tokio::test]
    async fn test_load_session_file_not_exists() {
        let temp_dir = tempfile::tempdir().unwrap();
        let data_dir = temp_dir.path();
        
        let mut adapter = WeChatPersonalAdapter::with_account_id("nonexistent_account".to_string());
        let loaded = adapter.load_session(data_dir).await.unwrap();
        assert!(!loaded);
    }

    #[tokio::test]
    async fn test_load_session_wrong_account_id() {
        let temp_dir = tempfile::tempdir().unwrap();
        let data_dir = temp_dir.path();
        
        // Create session file with wrong account_id
        let session_path = data_dir.join("wechat").join("wrong_account.json");
        tokio::fs::create_dir_all(session_path.parent().unwrap()).await.unwrap();
        
        let session_data = SessionData::new("different_account".to_string(), "token".to_string());
        let json = serde_json::to_string_pretty(&session_data).unwrap();
        tokio::fs::write(&session_path, json).await.unwrap();
        
        // Try to load with different account_id
        let mut adapter = WeChatPersonalAdapter::with_account_id("wrong_account".to_string());
        let result = adapter.load_session(data_dir).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("account_id mismatch"));
    }

    #[tokio::test]
    async fn test_save_session_no_token() {
        let temp_dir = tempfile::tempdir().unwrap();
        let data_dir = temp_dir.path();
        
        let adapter = WeChatPersonalAdapter::with_account_id("test_account".to_string());
        let result = adapter.save_session(data_dir).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No session token"));
    }

    #[tokio::test]
    async fn test_session_data_serialization() {
        let session_data = SessionData::new("account_123".to_string(), "token_abc".to_string());

        let json = serde_json::to_string_pretty(&session_data).unwrap();
        let parsed: SessionData = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.account_id, "account_123");
        assert_eq!(parsed.token, "token_abc");
    }

    #[tokio::test]
    async fn test_login_start_returns_qr_data() {
        let adapter = WeChatPersonalAdapter::with_account_id("test_account".to_string());

        let result = adapter.login_start().await;
        assert!(result.is_ok());

        let login_result = result.unwrap();
        assert!(login_result.qr_data_url.starts_with("data:image/png;base64,"));
        assert!(!login_result.session_key.is_empty());
        assert!(login_result.session_key.starts_with("wx_session_"));
        assert_eq!(login_result.message, "Please scan the QR code with WeChat");
    }

    #[tokio::test]
    async fn test_login_wait_timeout() {
        let adapter = WeChatPersonalAdapter::with_account_id("test_account".to_string());

        // Start login to get session key
        let login_start = adapter.login_start().await.unwrap();
        let session_key = login_start.session_key;

        // Wait with very short timeout (100ms)
        let result = adapter.login_wait(&session_key, 100).await;
        assert!(result.is_ok());

        let wait_result = result.unwrap();
        assert!(!wait_result.connected);
        assert!(wait_result.account_id.is_none());
        assert_eq!(wait_result.message, "Login timeout - QR code expired");
    }

    #[tokio::test]
    async fn test_logout_clears_token() {
        let mut adapter = WeChatPersonalAdapter::with_account_id("test_account".to_string());

        // Set a token first
        adapter.set_token("test_token".to_string());
        assert!(adapter.is_authenticated());

        // Logout
        let result = adapter.logout().await;
        assert!(result.cleared);
        assert_eq!(result.message, "Successfully logged out");

        // Verify token is cleared
        assert!(!adapter.is_authenticated());
        assert!(adapter.get_token().is_none());
    }

    #[tokio::test]
    async fn test_logout_without_token() {
        let mut adapter = WeChatPersonalAdapter::with_account_id("test_account".to_string());

        // Ensure no token
        assert!(!adapter.is_authenticated());

        // Logout should report no session
        let result = adapter.logout().await;
        assert!(!result.cleared);
        assert_eq!(result.message, "No active session to clear");
    }

    #[tokio::test]
    async fn test_login_flow_integration() {
        // Test the complete login flow
        let mut adapter = WeChatPersonalAdapter::with_account_id("test_account".to_string());

        // Step 1: Start login
        let login_result = adapter.login_start().await.unwrap();
        assert!(!login_result.session_key.is_empty());

        // Step 2: Wait for login (will timeout in placeholder)
        let wait_result = adapter.login_wait(&login_result.session_key, 50).await.unwrap();
        assert!(!wait_result.connected); // Placeholder always returns timeout

        // Step 3: Simulate successful login by setting token manually
        adapter.set_token("simulated_token".to_string());
        assert!(adapter.is_authenticated());

        // Step 4: Logout
        let logout_result = adapter.logout().await;
        assert!(logout_result.cleared);
        assert!(!adapter.is_authenticated());
    }

    #[test]
    fn test_qr_login_result_serialization() {
        let result = QrLoginResult {
            qr_data_url: "data:image/png;base64,abc123".to_string(),
            session_key: "wx_session_test".to_string(),
            message: "Scan me".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: QrLoginResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.qr_data_url, "data:image/png;base64,abc123");
        assert_eq!(parsed.session_key, "wx_session_test");
        assert_eq!(parsed.message, "Scan me");
    }

    #[test]
    fn test_login_wait_result_serialization() {
        let result = LoginWaitResult {
            connected: true,
            account_id: Some("user123".to_string()),
            message: "Connected".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: LoginWaitResult = serde_json::from_str(&json).unwrap();

        assert!(parsed.connected);
        assert_eq!(parsed.account_id, Some("user123".to_string()));
        assert_eq!(parsed.message, "Connected");
    }

    #[test]
    fn test_logout_result_serialization() {
        let result = LogoutResult {
            cleared: true,
            message: "Logged out".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: LogoutResult = serde_json::from_str(&json).unwrap();

        assert!(parsed.cleared);
        assert_eq!(parsed.message, "Logged out");
    }
}
