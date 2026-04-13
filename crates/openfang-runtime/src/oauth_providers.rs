//! OAuth providers for LLM services — OpenAI Codex, Gemini, Qwen, MiniMax.
//!
//! This module implements OAuth2 authentication flows for:
//! - OpenAI Codex (ChatGPT subscription) — device code flow + PKCE
//! - Gemini (Google OAuth) — PKCE + device code
//! - Qwen (Alibaba) — **file-based token import** from ~/.qwen/oauth_creds.json
//!   (not a true OAuth flow — reads pre-existing tokens from Qwen CLI)
//! - MiniMax — refresh token based (requires stored refresh token in vault)
//!
//! All tokens are stored in the credential vault.

use base64::Engine;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

// ─── Constants ───────────────────────────────────────────────────────────────────

// OpenAI Codex OAuth
pub const OPENAI_CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const OPENAI_CODEX_AUTH_URL: &str = "https://auth.openai.com/oauth/authorize";
pub const OPENAI_CODEX_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
pub const OPENAI_CODEX_DEVICE_URL: &str = "https://auth.openai.com/oauth/device/code";
pub const OPENAI_CODEX_CALLBACK_URI: &str = "http://localhost:1455/auth/callback";
pub const OPENAI_CODEX_RESPONSES_URL: &str = "https://chatgpt.com/backend-api/codex/responses";

// Gemini OAuth (requires GEMINI_OAUTH_CLIENT_ID and GEMINI_OAUTH_CLIENT_SECRET env vars)
pub const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const GOOGLE_DEVICE_URL: &str = "https://oauth2.googleapis.com/device/code";
pub const GOOGLE_CALLBACK_URI: &str = "http://localhost:1456/auth/callback";
pub const GEMINI_SCOPES: &str =
    "openid profile email https://www.googleapis.com/auth/cloud-platform";

// Qwen OAuth
pub const QWEN_OAUTH_TOKEN_ENDPOINT: &str = "https://chat.qwen.ai/api/v1/oauth2/token";
pub const QWEN_OAUTH_CREDENTIAL_FILE: &str = ".qwen/oauth_creds.json";
pub const QWEN_OAUTH_CLIENT_ID: &str = "f0304373b74a44d2b584a3fb70ca9e56";

// MiniMax OAuth
pub const MINIMAX_OAUTH_TOKEN_ENDPOINT: &str = "https://api.minimax.io/oauth/token";
pub const MINIMAX_OAUTH_CLIENT_ID: &str = "78257093-7e40-4613-99e0-527b14b39113";

// ─── Token Storage ────────────────────────────────────────────────────────────────

/// OAuth tokens stored in vault.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub provider: String,
}

impl OAuthTokenSet {
    /// Check if token is expired (with 60-second buffer).
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            expires_at < Utc::now() + chrono::Duration::seconds(60)
        } else {
            false
        }
    }

    /// Create from token response.
    pub fn from_response(resp: TokenResponse, provider: &str) -> Self {
        let expires_at = resp
            .expires_in
            .map(|secs| Utc::now() + chrono::Duration::seconds(secs));
        Self {
            access_token: resp.access_token,
            refresh_token: resp.refresh_token,
            expires_at,
            provider: provider.to_string(),
        }
    }
}

/// Token response from OAuth provider.
#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub id_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<i64>,
    #[serde(default)]
    pub token_type: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
}

/// Device code start response.
#[derive(Debug, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    #[serde(default)]
    pub interval: Option<u64>,
}

/// Device code flow status.
pub enum DeviceFlowStatus {
    Pending,
    Complete { tokens: OAuthTokenSet },
    SlowDown { new_interval: u64 },
    Expired,
    AccessDenied,
    Error(String),
}

// ─── OpenAI Codex OAuth ───────────────────────────────────────────────────────

/// Start OpenAI Codex device code flow.
pub async fn openai_codex_start_device_flow() -> Result<DeviceCodeResponse, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let resp = client
        .post(OPENAI_CODEX_DEVICE_URL)
        .header("Accept", "application/json")
        .form(&[
            ("client_id", OPENAI_CODEX_CLIENT_ID),
            ("scope", "openid profile email offline_access"),
        ])
        .send()
        .await
        .map_err(|e| format!("Device code request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Device code request returned {status}: {body}"));
    }

    resp.json::<DeviceCodeResponse>()
        .await
        .map_err(|e| format!("Failed to parse device code response: {e}"))
}

/// Poll OpenAI Codex device flow.
pub async fn openai_codex_poll_device_flow(device_code: &str) -> DeviceFlowStatus {
    let client = match Client::builder().timeout(Duration::from_secs(15)).build() {
        Ok(c) => c,
        Err(e) => return DeviceFlowStatus::Error(format!("HTTP client error: {e}")),
    };

    let resp = match client
        .post(OPENAI_CODEX_TOKEN_URL)
        .header("Accept", "application/json")
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", device_code),
            ("client_id", OPENAI_CODEX_CLIENT_ID),
        ])
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return DeviceFlowStatus::Error(format!("Token poll failed: {e}")),
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if let Ok(err) = serde_json::from_str::<serde_json::Value>(&body) {
            if let Some(error) = err.get("error").and_then(|v| v.as_str()) {
                return match error {
                    "authorization_pending" => DeviceFlowStatus::Pending,
                    "slow_down" => {
                        let interval = err.get("interval").and_then(|v| v.as_u64()).unwrap_or(10);
                        DeviceFlowStatus::SlowDown {
                            new_interval: interval,
                        }
                    }
                    "expired_token" => DeviceFlowStatus::Expired,
                    "access_denied" => DeviceFlowStatus::AccessDenied,
                    _ => DeviceFlowStatus::Error(error.to_string()),
                };
            }
        }
        return DeviceFlowStatus::Error(format!("HTTP {status}: {body}"));
    }

    match resp.json::<TokenResponse>().await {
        Ok(tokens) => DeviceFlowStatus::Complete {
            tokens: OAuthTokenSet::from_response(tokens, "openai-codex"),
        },
        Err(e) => DeviceFlowStatus::Error(format!("Failed to parse token response: {e}")),
    }
}

/// Build OpenAI Codex authorization URL for PKCE flow.
pub fn openai_codex_build_authorize_url(state: &str, code_challenge: &str) -> String {
    let params = [
        ("response_type", "code"),
        ("client_id", OPENAI_CODEX_CLIENT_ID),
        ("redirect_uri", OPENAI_CODEX_CALLBACK_URI),
        ("scope", "openid profile email offline_access"),
        ("code_challenge", code_challenge),
        ("code_challenge_method", "S256"),
        ("state", state),
        ("codex_cli_simplified_flow", "true"),
        ("id_token_add_organizations", "true"),
    ];

    let encoded: Vec<String> = params
        .iter()
        .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
        .collect();

    format!("{}?{}", OPENAI_CODEX_AUTH_URL, encoded.join("&"))
}

/// Exchange authorization code for tokens.
pub async fn openai_codex_exchange_code(
    code: &str,
    code_verifier: &str,
) -> Result<OAuthTokenSet, String> {
    let client = Client::new();

    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("client_id", OPENAI_CODEX_CLIENT_ID),
        ("redirect_uri", OPENAI_CODEX_CALLBACK_URI),
        ("code_verifier", code_verifier),
    ];

    let resp = client
        .post(OPENAI_CODEX_TOKEN_URL)
        .form(&form)
        .send()
        .await
        .map_err(|e| format!("Token exchange failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Token exchange failed ({status}): {body}"));
    }

    let tokens: TokenResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse token response: {e}"))?;

    Ok(OAuthTokenSet::from_response(tokens, "openai-codex"))
}

/// Refresh OpenAI Codex access token.
pub async fn openai_codex_refresh_token(refresh_token: &str) -> Result<OAuthTokenSet, String> {
    let client = Client::new();

    let form = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", OPENAI_CODEX_CLIENT_ID),
    ];

    let resp = client
        .post(OPENAI_CODEX_TOKEN_URL)
        .form(&form)
        .send()
        .await
        .map_err(|e| format!("Token refresh failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Token refresh failed ({status}): {body}"));
    }

    let tokens: TokenResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse token response: {e}"))?;

    Ok(OAuthTokenSet::from_response(tokens, "openai-codex"))
}

// ─── Gemini OAuth ───────────────────────────────────────────────────────────────

/// Get Gemini OAuth credentials from environment.
pub fn gemini_oauth_credentials() -> Option<(String, String)> {
    let client_id = std::env::var("GEMINI_OAUTH_CLIENT_ID").ok()?;
    let client_secret = std::env::var("GEMINI_OAUTH_CLIENT_SECRET").ok()?;
    if client_id.is_empty() || client_secret.is_empty() {
        return None;
    }
    Some((client_id, client_secret))
}

/// Start Gemini device code flow.
pub async fn gemini_start_device_flow() -> Result<DeviceCodeResponse, String> {
    let (client_id, _client_secret) = gemini_oauth_credentials()
        .ok_or("GEMINI_OAUTH_CLIENT_ID and GEMINI_OAUTH_CLIENT_SECRET required")?;

    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let scope_str =
        "openid profile email https://www.googleapis.com/auth/cloud-platform".to_string();
    let resp = client
        .post(GOOGLE_DEVICE_URL)
        .header("Accept", "application/json")
        .form(&[("client_id", &client_id), ("scope", &scope_str)])
        .send()
        .await
        .map_err(|e| format!("Device code request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Device code request returned {status}: {body}"));
    }

    #[derive(Deserialize)]
    struct GoogleDeviceResponse {
        device_code: String,
        user_code: String,
        verification_url: String,
        expires_in: Option<u64>,
        interval: Option<u64>,
    }

    let google_resp: GoogleDeviceResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse device code response: {e}"))?;

    Ok(DeviceCodeResponse {
        device_code: google_resp.device_code,
        user_code: google_resp.user_code,
        verification_uri: google_resp.verification_url,
        verification_uri_complete: None,
        expires_in: google_resp.expires_in.unwrap_or(300),
        interval: google_resp.interval,
    })
}

/// Poll Gemini device flow.
pub async fn gemini_poll_device_flow(device_code: &str) -> DeviceFlowStatus {
    let (client_id, client_secret) = match gemini_oauth_credentials() {
        Some(c) => c,
        None => return DeviceFlowStatus::Error("Missing OAuth credentials".to_string()),
    };

    let client = match Client::builder().timeout(Duration::from_secs(15)).build() {
        Ok(c) => c,
        Err(e) => return DeviceFlowStatus::Error(format!("HTTP client error: {e}")),
    };

    let form = [
        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ("device_code", device_code),
        ("client_id", &client_id),
        ("client_secret", &client_secret),
    ];

    let resp = match client
        .post(GOOGLE_TOKEN_URL)
        .header("Accept", "application/json")
        .form(&form)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return DeviceFlowStatus::Error(format!("Token poll failed: {e}")),
    };

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        if let Ok(err) = serde_json::from_str::<serde_json::Value>(&body) {
            if let Some(error) = err.get("error").and_then(|v| v.as_str()) {
                return match error {
                    "authorization_pending" => DeviceFlowStatus::Pending,
                    "slow_down" => {
                        let interval = err.get("interval").and_then(|v| v.as_u64()).unwrap_or(5);
                        DeviceFlowStatus::SlowDown {
                            new_interval: interval,
                        }
                    }
                    "expired_token" => DeviceFlowStatus::Expired,
                    "access_denied" => DeviceFlowStatus::AccessDenied,
                    _ => DeviceFlowStatus::Error(error.to_string()),
                };
            }
        }
        return DeviceFlowStatus::Error(format!("HTTP error: {body}"));
    }

    match resp.json::<TokenResponse>().await {
        Ok(tokens) => DeviceFlowStatus::Complete {
            tokens: OAuthTokenSet::from_response(tokens, "gemini-oauth"),
        },
        Err(e) => DeviceFlowStatus::Error(format!("Failed to parse token response: {e}")),
    }
}

// ─── Qwen OAuth ────────────────────────────────────────────────────────────────

/// Find Qwen OAuth credentials file.
pub fn qwen_credentials_path() -> Option<std::path::PathBuf> {
    let home = home_dir()?;
    let qwen_path = home.join(QWEN_OAUTH_CREDENTIAL_FILE);
    if qwen_path.exists() {
        Some(qwen_path)
    } else {
        None
    }
}

/// Cross-platform home directory (same as qwen_code.rs).
fn home_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .ok()
            .map(std::path::PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok().map(std::path::PathBuf::from)
    }
}

/// Read Qwen OAuth credentials from file.
pub fn read_qwen_credentials() -> Option<OAuthTokenSet> {
    let path = qwen_credentials_path()?;
    let content = std::fs::read_to_string(&path).ok()?;

    #[derive(Deserialize)]
    struct QwenCredsFile {
        access_token: String,
        refresh_token: Option<String>,
        expires_at: Option<String>,
    }

    let creds: QwenCredsFile = serde_json::from_str(&content).ok()?;

    let expires_at = creds.expires_at.and_then(|s| {
        DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    });

    Some(OAuthTokenSet {
        access_token: creds.access_token,
        refresh_token: creds.refresh_token,
        expires_at,
        provider: "qwen-oauth".to_string(),
    })
}

/// Start Qwen "OAuth" flow — reads tokens from ~/.qwen/oauth_creds.json.
///
/// **Note**: This is NOT a true OAuth flow. Qwen tokens must first be obtained
/// via the Qwen CLI (`qwen login`), which creates the credential file.
/// This function merely imports those pre-existing tokens into OpenFang's vault.
pub async fn qwen_start_oauth_flow() -> Result<(), String> {
    // Qwen OAuth is file-based, just verify we can read the credentials
    read_qwen_credentials().ok_or_else(|| {
        "Failed to read Qwen credentials from ~/.qwen/oauth_creds.json. Run 'qwen login' first."
            .to_string()
    })?;
    Ok(())
}

/// Poll Qwen "OAuth" flow — returns tokens from the credential file.
///
/// **Note**: This is a file import, not a polling mechanism.
/// The tokens are read directly from ~/.qwen/oauth_creds.json.
pub async fn qwen_poll_oauth_flow() -> Result<OAuthTokenSet, String> {
    read_qwen_credentials()
        .ok_or_else(|| "Failed to read Qwen credentials. Run 'qwen login' first.".to_string())
}

/// Refresh Qwen OAuth token.
pub async fn refresh_qwen_token(refresh_token: &str) -> Result<OAuthTokenSet, String> {
    let client = Client::new();

    let form = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", QWEN_OAUTH_CLIENT_ID),
    ];

    let resp = client
        .post(QWEN_OAUTH_TOKEN_ENDPOINT)
        .header("Accept", "application/json")
        .form(&form)
        .send()
        .await
        .map_err(|e| format!("Token refresh failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Token refresh failed ({status}): {body}"));
    }

    let tokens: TokenResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse token response: {e}"))?;

    Ok(OAuthTokenSet::from_response(tokens, "qwen-oauth"))
}

// ─── MiniMax OAuth ───────────────────────────────────────────────────────────

/// Refresh MiniMax OAuth token.
pub async fn refresh_minimax_token(
    refresh_token: &str,
    region: &str,
) -> Result<OAuthTokenSet, String> {
    let endpoint = match region {
        "cn" => "https://api.minimaxi.com/oauth/token",
        _ => MINIMAX_OAUTH_TOKEN_ENDPOINT,
    };

    let client = Client::new();

    let form = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", MINIMAX_OAUTH_CLIENT_ID),
    ];

    let resp = client
        .post(endpoint)
        .header("Accept", "application/json")
        .form(&form)
        .send()
        .await
        .map_err(|e| format!("Token refresh failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Token refresh failed ({status}): {body}"));
    }

    let tokens: TokenResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse token response: {e}"))?;

    Ok(OAuthTokenSet::from_response(tokens, "minimax-oauth"))
}

/// Start MiniMax OAuth flow — requires a stored refresh token.
///
/// MiniMax does not support device code or authorization code flow;
/// authentication must be initiated externally (e.g. via their console)
/// and the resulting refresh token stored in the vault before calling
/// [`refresh_minimax_token`].
pub async fn minimax_start_oauth_flow() -> Result<(), String> {
    Err("MiniMax does not support browser-based OAuth. Store a refresh token in the vault first, then use the refresh endpoint.".to_string())
}

/// Check whether a MiniMax refresh token is available in the vault.
///
/// Returns `Ok(())` if a refresh token exists for MiniMax, `Err` otherwise.
/// This is not a traditional OAuth poll — MiniMax has no device code flow.
pub async fn minimax_poll_oauth_flow() -> Result<OAuthTokenSet, String> {
    Err("MiniMax has no device code flow. Store a refresh token in the vault first.".to_string())
}

// ─── Utility Functions ────────────────────────────────────────────────────────

/// URL-encode a string.
fn url_encode(input: &str) -> String {
    input
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{:02X}", b),
        })
        .collect::<String>()
}

/// Generate PKCE code verifier and challenge using a cryptographically secure RNG.
///
/// Uses `OsRng` as the entropy source per RFC 7636 §4.1 requirements.
/// The verifier is 32 bytes (256 bits) of CSPRNG output, base64url-encoded.
/// The challenge is the SHA-256 hash of the verifier, base64url-encoded.
pub fn generate_pkce() -> (String, String) {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);

    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(verifier.as_bytes());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);

    (verifier, challenge)
}

/// Generate a cryptographically random OAuth state parameter (128 bits from OsRng).
///
/// Per RFC 6749 §10.12, the state parameter must be unguessable to prevent CSRF.
pub fn generate_state() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_constants() {
        assert!(OPENAI_CODEX_AUTH_URL.starts_with("https://"));
        assert!(OPENAI_CODEX_TOKEN_URL.starts_with("https://"));
    }

    #[test]
    fn test_pkce_generation() {
        let (verifier, challenge) = generate_pkce();
        assert!(!verifier.is_empty());
        assert!(!challenge.is_empty());
        assert_ne!(verifier, challenge);
        // Verifier should be 43 chars (32 bytes base64url no-pad)
        assert_eq!(
            verifier.len(),
            43,
            "PKCE verifier must be 43 chars (256-bit base64url)"
        );
    }

    #[test]
    fn test_pkce_uniqueness() {
        // Two consecutive calls must produce different verifiers (CSPRNG)
        let (v1, _) = generate_pkce();
        let (v2, _) = generate_pkce();
        assert_ne!(v1, v2, "CSPRNG must produce unique verifiers");
    }

    #[test]
    fn test_state_uniqueness() {
        let s1 = generate_state();
        let s2 = generate_state();
        assert_ne!(s1, s2, "CSPRNG must produce unique state values");
    }

    #[test]
    fn test_url_encode() {
        assert_eq!(url_encode("hello"), "hello");
        assert_eq!(url_encode("hello world"), "hello%20world");
        assert_eq!(url_encode("a=b"), "a%3Db");
    }
}
