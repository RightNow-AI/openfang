//! GitHub Copilot LLM driver with full OAuth device flow authentication.
//!
//! Implements the complete Copilot auth chain:
//! 1. OAuth device flow (user registers their own GitHub OAuth App)
//! 2. `ghu_` access token with `grt_` refresh token (auto-refresh)
//! 3. Copilot API token exchange via `/copilot_internal/v2/token`
//! 4. OpenAI-compatible completions against the Copilot API
//!
//! Users configure their OAuth App's `client_id` and `client_secret` in
//! `config.toml`. The driver handles the rest — device flow, token persistence,
//! refresh, and Copilot API token exchange — automatically.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use zeroize::Zeroizing;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Copilot token exchange endpoint.
const COPILOT_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";

/// GitHub device code endpoint.
const GITHUB_DEVICE_CODE_URL: &str = "https://github.com/login/device/code";

/// GitHub OAuth access token endpoint.
const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";

/// GitHub Copilot's own OAuth App client ID (VS Code Copilot extension).
/// This is a public, well-known client ID used by all direct Copilot integrations.
/// Only tokens from this app (`ghu_` prefix) are accepted by copilot_internal.
const COPILOT_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";

/// Default Copilot API base URL (overridden by `endpoints.api` from token exchange).
pub const GITHUB_COPILOT_BASE_URL: &str = "https://api.githubcopilot.com";

/// HTTP timeout for token exchange requests.
const TOKEN_EXCHANGE_TIMEOUT: Duration = Duration::from_secs(10);

/// Refresh Copilot API token this many seconds before it expires.
const COPILOT_TOKEN_REFRESH_BUFFER_SECS: u64 = 300; // 5 minutes

/// Refresh `ghu_` access token this many seconds before it expires.
const ACCESS_TOKEN_REFRESH_BUFFER_SECS: u64 = 600; // 10 minutes

/// Scopes requested during device flow.
const OAUTH_SCOPES: &str = "copilot";

/// File name for persisted OAuth tokens (inside ~/.openfang/).
const TOKEN_FILE_NAME: &str = ".copilot-tokens.json";

/// Device flow polling interval (seconds) — GitHub default is 5.
#[allow(dead_code)]
const DEVICE_FLOW_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Maximum time to wait for user to authorize the device flow.
const DEVICE_FLOW_TIMEOUT: Duration = Duration::from_secs(900); // 15 minutes

// ---------------------------------------------------------------------------
// Persisted OAuth tokens (ghu_ + grt_)
// ---------------------------------------------------------------------------

/// OAuth tokens persisted to disk for survival across daemon restarts.
#[derive(Clone, Serialize, Deserialize)]
pub struct PersistedTokens {
    /// `ghu_` user access token.
    pub access_token: String,
    /// Unix timestamp when `access_token` expires.
    pub access_token_expires_at: i64,
    /// `grt_` refresh token (single-use, rotated on each refresh).
    pub refresh_token: String,
}

impl PersistedTokens {
    /// Whether the access token is still usable (with buffer).
    fn access_token_valid(&self) -> bool {
        let now = unix_now();
        self.access_token_expires_at > now + ACCESS_TOKEN_REFRESH_BUFFER_SECS as i64
    }

    /// Load from the OpenFang data directory.
    pub fn load(openfang_dir: &Path) -> Option<Self> {
        let path = openfang_dir.join(TOKEN_FILE_NAME);
        let data = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&data).ok()
    }

    /// Persist to the OpenFang data directory with restricted permissions.
    pub fn save(&self, openfang_dir: &Path) -> Result<(), String> {
        let path = openfang_dir.join(TOKEN_FILE_NAME);
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize tokens: {e}"))?;
        std::fs::write(&path, &json)
            .map_err(|e| format!("Failed to write {}: {e}", path.display()))?;

        // Restrict file permissions on Unix.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            let _ = std::fs::set_permissions(&path, perms);
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Cached Copilot API token (tid=…)
// ---------------------------------------------------------------------------

/// Short-lived Copilot API token from `/copilot_internal/v2/token`.
#[derive(Clone)]
pub struct CachedCopilotToken {
    /// The bearer token for Copilot API requests.
    pub token: Zeroizing<String>,
    /// When this token expires.
    pub expires_at: Instant,
    /// Base URL from `endpoints.api` (plan-specific).
    pub base_url: String,
}

impl CachedCopilotToken {
    fn is_valid(&self) -> bool {
        self.expires_at > Instant::now() + Duration::from_secs(COPILOT_TOKEN_REFRESH_BUFFER_SECS)
    }
}

// ---------------------------------------------------------------------------
// Cached model list
// ---------------------------------------------------------------------------

/// Cached list of available models from the Copilot API.
#[derive(Clone)]
struct CachedModels {
    models: Vec<String>,
    #[allow(dead_code)]
    fetched_at: Instant,
}

// ---------------------------------------------------------------------------
// OAuth device flow
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default = "default_interval")]
    pub interval: u64,
    #[allow(dead_code)]
    pub expires_in: u64,
}

fn default_interval() -> u64 {
    5
}

#[derive(Deserialize)]
struct OAuthTokenResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    #[allow(dead_code)]
    refresh_token_expires_in: Option<i64>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

/// Request a device code from GitHub using the Copilot client ID.
pub async fn request_device_code(client: &reqwest::Client) -> Result<DeviceCodeResponse, String> {
    let resp = client
        .post(GITHUB_DEVICE_CODE_URL)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "client_id": COPILOT_CLIENT_ID,
            "scope": OAUTH_SCOPES,
        }))
        .send()
        .await
        .map_err(|e| format!("Device code request failed: {e}"))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Device code request returned error: {body}"));
    }

    resp.json::<DeviceCodeResponse>()
        .await
        .map_err(|e| format!("Failed to parse device code response: {e}"))
}

/// Poll for the OAuth token after user authorizes.
pub async fn poll_for_token(
    client: &reqwest::Client,
    device_code: &str,
    interval: u64,
) -> Result<PersistedTokens, String> {
    let poll_interval = Duration::from_secs(interval.max(5));
    let deadline = Instant::now() + DEVICE_FLOW_TIMEOUT;

    loop {
        if Instant::now() > deadline {
            return Err("Device flow timed out — user did not authorize in time".to_string());
        }

        tokio::time::sleep(poll_interval).await;

        let resp = client
            .post(GITHUB_TOKEN_URL)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "client_id": COPILOT_CLIENT_ID,
                "device_code": device_code,
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
            }))
            .send()
            .await
            .map_err(|e| format!("Token poll failed: {e}"))?;

        let token_resp: OAuthTokenResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse token poll response: {e}"))?;

        if let Some(ref err) = token_resp.error {
            match err.as_str() {
                "authorization_pending" => continue,
                "slow_down" => {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
                "expired_token" => {
                    return Err("Device code expired — please try again".to_string());
                }
                "access_denied" => {
                    return Err("User denied authorization".to_string());
                }
                _ => {
                    let desc = token_resp.error_description.as_deref().unwrap_or("");
                    return Err(format!("OAuth error: {err} — {desc}"));
                }
            }
        }

        let access_token = token_resp
            .access_token
            .ok_or("Missing access_token in response")?;
        let refresh_token = token_resp.refresh_token.unwrap_or_default(); // Empty if token expiration is disabled on the OAuth App
        let expires_in = token_resp.expires_in.unwrap_or(0); // 0 = non-expiring

        return Ok(PersistedTokens {
            access_token,
            access_token_expires_at: if expires_in > 0 {
                unix_now() + expires_in
            } else {
                // Non-expiring token — set far-future expiry.
                unix_now() + 365 * 24 * 3600
            },
            refresh_token,
        });
    }
}

/// Refresh an expired `ghu_` token using the `grt_` refresh token.
async fn refresh_access_token(
    client: &reqwest::Client,
    refresh_token: &str,
) -> Result<PersistedTokens, String> {
    debug!("Refreshing Copilot access token via refresh_token");

    let resp = client
        .post(GITHUB_TOKEN_URL)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "client_id": COPILOT_CLIENT_ID,
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
        }))
        .send()
        .await
        .map_err(|e| format!("Token refresh request failed: {e}"))?;

    let token_resp: OAuthTokenResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse refresh response: {e}"))?;

    if let Some(ref err) = token_resp.error {
        let desc = token_resp.error_description.as_deref().unwrap_or("");
        return Err(format!("Token refresh failed: {err} — {desc}"));
    }

    let access_token = token_resp
        .access_token
        .ok_or("Missing access_token in refresh response")?;
    let new_refresh_token = token_resp
        .refresh_token
        .ok_or("Missing refresh_token in refresh response")?;
    let expires_in = token_resp.expires_in.unwrap_or(28800);

    Ok(PersistedTokens {
        access_token,
        access_token_expires_at: unix_now() + expires_in,
        refresh_token: new_refresh_token,
    })
}

// ---------------------------------------------------------------------------
// Copilot API token exchange
// ---------------------------------------------------------------------------

/// Exchange a `ghu_` access token for a short-lived Copilot API token.
pub async fn exchange_copilot_token(
    client: &reqwest::Client,
    access_token: &str,
) -> Result<CachedCopilotToken, String> {
    debug!("Exchanging access token for Copilot API token");

    let resp = client
        .get(COPILOT_TOKEN_URL)
        .header("Authorization", format!("token {access_token}"))
        .header("Accept", "application/json")
        .header("User-Agent", "OpenFang/1.0")
        .header("Editor-Version", "vscode/1.96.0")
        .header("Editor-Plugin-Version", "copilot/1.250.0")
        .send()
        .await
        .map_err(|e| format!("Copilot token exchange failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Copilot token exchange returned {status}: {body}"));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Copilot token response: {e}"))?;

    let raw_token = body
        .get("token")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'token' field in Copilot response")?;

    let expires_at_unix = body.get("expires_at").and_then(|v| v.as_i64()).unwrap_or(0);
    let ttl_secs = (expires_at_unix - unix_now()).max(60) as u64;

    // Extract base URL from endpoints.api or proxy-ep in the token.
    let base_url = body
        .pointer("/endpoints/api")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| parse_proxy_ep(raw_token))
        .unwrap_or_else(|| GITHUB_COPILOT_BASE_URL.to_string());

    // Validate HTTPS.
    let base_url = if base_url.starts_with("https://") {
        base_url
    } else if base_url.contains('.') && !base_url.contains("://") {
        // Handle bare hostnames like "proxy.enterprise.githubcopilot.com".
        format!("https://{base_url}")
    } else {
        warn!(url = %base_url, "Copilot endpoint is not HTTPS, using default");
        GITHUB_COPILOT_BASE_URL.to_string()
    };

    Ok(CachedCopilotToken {
        token: Zeroizing::new(raw_token.to_string()),
        expires_at: Instant::now() + Duration::from_secs(ttl_secs),
        base_url,
    })
}

/// Extract `proxy-ep` from the semicolon-delimited Copilot token string.
fn parse_proxy_ep(raw: &str) -> Option<String> {
    for segment in raw.split(';') {
        if let Some(url) = segment.trim().strip_prefix("proxy-ep=") {
            if url.contains('.') {
                return Some(if url.starts_with("https://") {
                    url.to_string()
                } else {
                    format!("https://{url}")
                });
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Model list fetching
// ---------------------------------------------------------------------------

/// Fetch available models from the Copilot API.
pub async fn fetch_models(
    client: &reqwest::Client,
    base_url: &str,
    copilot_token: &str,
) -> Result<Vec<String>, String> {
    debug!(base_url = %base_url, "Fetching available Copilot models");

    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {copilot_token}"))
        .header("User-Agent", "OpenFang/1.0")
        .header("Editor-Version", "vscode/1.96.0")
        .send()
        .await
        .map_err(|e| format!("Model list fetch failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Model list request returned {status}: {body}"));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse model list: {e}"))?;

    let models: Vec<String> = body
        .get("data")
        .or_else(|| body.get("models"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    m.get("id")
                        .and_then(|id| id.as_str())
                        .map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default();

    debug!(count = models.len(), "Loaded Copilot model catalog");
    Ok(models)
}

// ---------------------------------------------------------------------------
// CopilotDriver
// ---------------------------------------------------------------------------

/// LLM driver that authenticates via GitHub OAuth device flow and proxies
/// completions through the Copilot API (OpenAI-compatible).
pub struct CopilotDriver {
    openfang_dir: PathBuf,
    http_client: reqwest::Client,

    /// Persisted OAuth tokens (ghu_ + grt_).
    oauth_tokens: Mutex<Option<PersistedTokens>>,
    /// Cached short-lived Copilot API token.
    copilot_token: Mutex<Option<CachedCopilotToken>>,
    /// Cached model list.
    models: Mutex<Option<CachedModels>>,
}

impl CopilotDriver {
    pub fn new(openfang_dir: PathBuf) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(TOKEN_EXCHANGE_TIMEOUT)
            .build()
            .expect("Failed to build HTTP client");

        // Try to load persisted tokens on construction.
        let persisted = PersistedTokens::load(&openfang_dir);
        if persisted.is_some() {
            debug!("Loaded persisted Copilot OAuth tokens");
        }

        Self {
            openfang_dir,
            http_client,
            oauth_tokens: Mutex::new(persisted),
            copilot_token: Mutex::new(None),
            models: Mutex::new(None),
        }
    }

    /// Ensure we have a valid `ghu_` access token, refreshing or re-authing as needed.
    async fn ensure_access_token(&self) -> Result<String, crate::llm_driver::LlmError> {
        // Check if current token is still valid.
        {
            let lock = self.oauth_tokens.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(ref tokens) = *lock {
                if tokens.access_token_valid() {
                    return Ok(tokens.access_token.clone());
                }
            }
        }

        // Try to refresh using the refresh token (only if one exists).
        let refresh_token = {
            let lock = self.oauth_tokens.lock().unwrap_or_else(|e| e.into_inner());
            lock.as_ref()
                .map(|t| t.refresh_token.clone())
                .filter(|rt| !rt.is_empty())
        };

        if let Some(ref rt) = refresh_token {
            match refresh_access_token(&self.http_client, rt).await {
                Ok(new_tokens) => {
                    info!("Copilot access token refreshed successfully");
                    if let Err(e) = new_tokens.save(&self.openfang_dir) {
                        warn!("Failed to persist refreshed tokens: {e}");
                    }
                    let access_token = new_tokens.access_token.clone();
                    let mut lock = self.oauth_tokens.lock().unwrap_or_else(|e| e.into_inner());
                    *lock = Some(new_tokens);
                    // Invalidate copilot token cache since access token changed.
                    let mut ct_lock = self.copilot_token.lock().unwrap_or_else(|e| e.into_inner());
                    *ct_lock = None;
                    return Ok(access_token);
                }
                Err(e) => {
                    warn!("Token refresh failed, device flow re-auth required: {e}");
                }
            }
        }

        // No valid tokens and refresh failed — need device flow.
        // In daemon mode, we can't do interactive auth. Return a clear error.
        Err(crate::llm_driver::LlmError::AuthenticationFailed(
            "Copilot OAuth tokens expired. Run `openfang config set-key github-copilot` to re-authenticate via device flow.".to_string(),
        ))
    }

    /// Ensure we have a valid Copilot API token (tid=…).
    async fn ensure_copilot_token(
        &self,
    ) -> Result<CachedCopilotToken, crate::llm_driver::LlmError> {
        // Check cache.
        {
            let lock = self.copilot_token.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(ref ct) = *lock {
                if ct.is_valid() {
                    return Ok(ct.clone());
                }
            }
        }

        // Get a valid access token first.
        let access_token = self.ensure_access_token().await?;

        // Exchange for Copilot API token.
        let ct = exchange_copilot_token(&self.http_client, &access_token)
            .await
            .map_err(|e| crate::llm_driver::LlmError::Api {
                status: 401,
                message: format!("Copilot token exchange failed: {e}"),
            })?;

        let mut lock = self.copilot_token.lock().unwrap_or_else(|e| e.into_inner());
        *lock = Some(ct.clone());
        Ok(ct)
    }

    /// Ensure model list is cached; fetch if missing.
    async fn ensure_models(
        &self,
        copilot_token: &CachedCopilotToken,
    ) -> Result<Vec<String>, crate::llm_driver::LlmError> {
        {
            let lock = self.models.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(ref cached) = *lock {
                return Ok(cached.models.clone());
            }
        }

        self.refresh_models(copilot_token).await
    }

    /// Force-refresh the model list from the API.
    async fn refresh_models(
        &self,
        copilot_token: &CachedCopilotToken,
    ) -> Result<Vec<String>, crate::llm_driver::LlmError> {
        let models = fetch_models(
            &self.http_client,
            &copilot_token.base_url,
            &copilot_token.token,
        )
        .await
        .map_err(|e| crate::llm_driver::LlmError::Api {
            status: 500,
            message: format!("Failed to fetch model list: {e}"),
        })?;

        let mut lock = self.models.lock().unwrap_or_else(|e| e.into_inner());
        *lock = Some(CachedModels {
            models: models.clone(),
            fetched_at: Instant::now(),
        });
        Ok(models)
    }

    /// Build an OpenAI driver for the current Copilot token.
    fn make_inner_driver(&self, ct: &CachedCopilotToken) -> super::openai::OpenAIDriver {
        let base_url = if ct.base_url.is_empty() {
            GITHUB_COPILOT_BASE_URL.to_string()
        } else {
            ct.base_url.clone()
        };
        super::openai::OpenAIDriver::new(ct.token.to_string(), base_url).with_extra_headers(vec![
            ("Editor-Version".to_string(), "vscode/1.96.0".to_string()),
            (
                "Editor-Plugin-Version".to_string(),
                "copilot/1.250.0".to_string(),
            ),
            (
                "Copilot-Integration-Id".to_string(),
                "vscode-chat".to_string(),
            ),
        ])
    }

    /// Execute a request, retrying once on model_not_supported after refreshing models.
    async fn execute_with_model_retry<F, Fut>(
        &self,
        request: crate::llm_driver::CompletionRequest,
        execute: F,
    ) -> Result<crate::llm_driver::CompletionResponse, crate::llm_driver::LlmError>
    where
        F: Fn(super::openai::OpenAIDriver, crate::llm_driver::CompletionRequest) -> Fut,
        Fut: std::future::Future<
            Output = Result<crate::llm_driver::CompletionResponse, crate::llm_driver::LlmError>,
        >,
    {
        let ct = self.ensure_copilot_token().await?;
        let _ = self.ensure_models(&ct).await; // best-effort cache
        let driver = self.make_inner_driver(&ct);

        match execute(driver, request.clone()).await {
            Ok(resp) => Ok(resp),
            Err(crate::llm_driver::LlmError::Api {
                status,
                ref message,
            }) if status == 400 && message.contains("model_not_supported") => {
                // Refresh model list so subsequent calls have updated info.
                warn!(
                    model = %request.model,
                    "Model not supported — refreshing model catalog"
                );
                if let Ok(models) = self.refresh_models(&ct).await {
                    let available = models.join(", ");
                    return Err(crate::llm_driver::LlmError::ModelNotFound(format!(
                        "'{}' is not available. Models: {available}",
                        request.model
                    )));
                }
                Err(crate::llm_driver::LlmError::ModelNotFound(format!(
                    "'{}' is not supported by your Copilot plan",
                    request.model
                )))
            }
            Err(e) => Err(e),
        }
    }
}

#[async_trait::async_trait]
impl crate::llm_driver::LlmDriver for CopilotDriver {
    async fn complete(
        &self,
        request: crate::llm_driver::CompletionRequest,
    ) -> Result<crate::llm_driver::CompletionResponse, crate::llm_driver::LlmError> {
        self.execute_with_model_retry(
            request,
            |driver, req| async move { driver.complete(req).await },
        )
        .await
    }

    async fn stream(
        &self,
        request: crate::llm_driver::CompletionRequest,
        tx: tokio::sync::mpsc::Sender<crate::llm_driver::StreamEvent>,
    ) -> Result<crate::llm_driver::CompletionResponse, crate::llm_driver::LlmError> {
        let ct = self.ensure_copilot_token().await?;
        let _ = self.ensure_models(&ct).await;
        let driver = self.make_inner_driver(&ct);

        match driver.stream(request.clone(), tx.clone()).await {
            Ok(resp) => Ok(resp),
            Err(crate::llm_driver::LlmError::Api {
                status,
                ref message,
            }) if status == 400 && message.contains("model_not_supported") => {
                warn!(
                    model = %request.model,
                    "Model not supported — refreshing model catalog"
                );
                if let Ok(models) = self.refresh_models(&ct).await {
                    let available = models.join(", ");
                    return Err(crate::llm_driver::LlmError::ModelNotFound(format!(
                        "'{}' is not available. Models: {available}",
                        request.model
                    )));
                }
                Err(crate::llm_driver::LlmError::ModelNotFound(format!(
                    "'{}' is not supported by your Copilot plan",
                    request.model
                )))
            }
            Err(e) => Err(e),
        }
    }
}

// ---------------------------------------------------------------------------
// Interactive device flow (called from CLI, not from daemon)
// ---------------------------------------------------------------------------

/// Run the interactive Copilot setup: execute the device flow.
///
/// Called from `openfang config set-key github-copilot`, `openfang init`,
/// `openfang onboard`, and `openfang configure`.
pub async fn run_interactive_setup(openfang_dir: &Path) -> Result<PersistedTokens, String> {
    run_device_flow(openfang_dir).await
}

/// Run the OAuth device flow using the Copilot client ID.
///
/// Prints the user code and verification URL, attempts to open the browser,
/// then polls until the user authorizes.
pub async fn run_device_flow(openfang_dir: &Path) -> Result<PersistedTokens, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    // Step 1: Request device code.
    let device = request_device_code(&client).await?;

    // Step 2: Tell the user what to do + try to open browser.
    println!();
    println!("  Open this URL in your browser:");
    println!("    {}", device.verification_uri);
    println!();
    println!("  Enter code: {}", device.user_code);
    println!();

    // Try to open the browser automatically.
    let _ = open_verification_url(&device.verification_uri);

    println!("  Waiting for authorization...");

    // Step 3: Poll for authorization.
    let tokens = poll_for_token(&client, &device.device_code, device.interval).await?;

    // Step 4: Persist.
    tokens.save(openfang_dir)?;
    println!("  Copilot authentication successful.");

    Ok(tokens)
}

/// Read a line from stdin with a prompt. Used during interactive setup.
#[allow(dead_code)]
fn prompt_line(prompt: &str) -> Result<String, String> {
    use std::io::{self, BufRead, Write};
    print!("{prompt}");
    io::stdout().flush().map_err(|e| format!("IO error: {e}"))?;
    let mut line = String::new();
    io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| format!("Failed to read input: {e}"))?;
    Ok(line.trim().to_string())
}

/// Try to open the verification URL in the default browser.
pub fn open_verification_url(url: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn().is_ok()
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .is_ok()
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .is_ok()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = url;
        false
    }
}

/// Check if Copilot OAuth tokens exist on disk.
pub fn copilot_auth_available(openfang_dir: &Path) -> bool {
    openfang_dir.join(TOKEN_FILE_NAME).exists()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_proxy_ep_with_https() {
        let raw = "tid=abc;exp=123;proxy-ep=https://proxy.enterprise.githubcopilot.com";
        assert_eq!(
            parse_proxy_ep(raw),
            Some("https://proxy.enterprise.githubcopilot.com".to_string())
        );
    }

    #[test]
    fn test_parse_proxy_ep_bare_hostname() {
        let raw = "tid=abc;exp=123;proxy-ep=proxy.enterprise.githubcopilot.com";
        assert_eq!(
            parse_proxy_ep(raw),
            Some("https://proxy.enterprise.githubcopilot.com".to_string())
        );
    }

    #[test]
    fn test_parse_proxy_ep_missing() {
        let raw = "tid=abc;exp=123;sku=copilot_enterprise";
        assert_eq!(parse_proxy_ep(raw), None);
    }

    #[test]
    fn test_persisted_tokens_validity() {
        let valid = PersistedTokens {
            access_token: "ghu_test".to_string(),
            access_token_expires_at: unix_now() + 7200, // 2h from now
            refresh_token: "grt_test".to_string(),
        };
        assert!(valid.access_token_valid());

        let expired = PersistedTokens {
            access_token: "ghu_test".to_string(),
            access_token_expires_at: unix_now() + 60, // 1 min — within buffer
            refresh_token: "grt_test".to_string(),
        };
        assert!(!expired.access_token_valid());
    }

    #[test]
    fn test_persisted_tokens_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();

        let tokens = PersistedTokens {
            access_token: "ghu_abc123".to_string(),
            access_token_expires_at: unix_now() + 3600,
            refresh_token: "grt_xyz789".to_string(),
        };

        tokens.save(&path).unwrap();

        let loaded = PersistedTokens::load(&path).unwrap();
        assert_eq!(loaded.access_token, "ghu_abc123");
        assert_eq!(loaded.refresh_token, "grt_xyz789");
    }

    #[test]
    fn test_cached_copilot_token_validity() {
        let valid = CachedCopilotToken {
            token: Zeroizing::new("tid=test".to_string()),
            expires_at: Instant::now() + Duration::from_secs(3600),
            base_url: GITHUB_COPILOT_BASE_URL.to_string(),
        };
        assert!(valid.is_valid());

        let expired = CachedCopilotToken {
            token: Zeroizing::new("tid=test".to_string()),
            expires_at: Instant::now() + Duration::from_secs(60),
            base_url: GITHUB_COPILOT_BASE_URL.to_string(),
        };
        assert!(!expired.is_valid());
    }

    #[test]
    fn test_copilot_base_url() {
        assert_eq!(GITHUB_COPILOT_BASE_URL, "https://api.githubcopilot.com");
    }
}
