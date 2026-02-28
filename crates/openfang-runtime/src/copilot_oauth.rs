//! GitHub Copilot OAuth — device flow for obtaining a GitHub PAT via browser login.
//!
//! Implements the OAuth 2.0 Device Authorization Grant (RFC 8628) using GitHub's
//! device flow endpoint. Users visit a URL, enter a code, and authorize the app.
//! Once complete, the resulting access token can be used with the CopilotDriver.

use serde::Deserialize;
use zeroize::Zeroizing;

/// GitHub device code request URL.
const GITHUB_DEVICE_CODE_URL: &str = "https://github.com/login/device/code";

/// GitHub OAuth token URL.
const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";

/// Public OAuth client ID — same as VSCode Copilot extension.
const COPILOT_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";

/// Response from the device code initiation request.
#[derive(Debug, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// Status of a device flow polling attempt.
pub enum DeviceFlowStatus {
    /// Authorization is pending — user hasn't completed the flow yet.
    Pending,
    /// Authorization succeeded — contains the access token.
    Complete { access_token: Zeroizing<String> },
    /// Server asked to slow down — use the new interval.
    SlowDown { new_interval: u64 },
    /// The device code expired — user must restart the flow.
    Expired,
    /// User explicitly denied access.
    AccessDenied,
    /// An unexpected error occurred.
    Error(String),
}

/// Start a GitHub device flow for Copilot OAuth.
///
/// POST https://github.com/login/device/code
/// Returns a device code and user code for the user to enter at the verification URI.
pub async fn start_device_flow() -> Result<DeviceCodeResponse, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let resp = client
        .post(GITHUB_DEVICE_CODE_URL)
        .header("Accept", "application/json")
        .form(&[("client_id", COPILOT_CLIENT_ID), ("scope", "read:user")])
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

/// Poll the GitHub token endpoint for the device flow result.
///
/// POST https://github.com/login/oauth/access_token
/// Returns the current status of the authorization flow.
pub async fn poll_device_flow(device_code: &str) -> DeviceFlowStatus {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(e) => return DeviceFlowStatus::Error(format!("HTTP client error: {e}")),
    };

    let resp = match client
        .post(GITHUB_TOKEN_URL)
        .header("Accept", "application/json")
        .form(&[
            ("client_id", COPILOT_CLIENT_ID),
            (
                "grant_type",
                "urn:ietf:params:oauth:grant-type:device_code",
            ),
            ("device_code", device_code),
        ])
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return DeviceFlowStatus::Error(format!("Token poll failed: {e}")),
    };

    let body: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return DeviceFlowStatus::Error(format!("Failed to parse token response: {e}")),
    };

    // Check for error field first (GitHub returns 200 with error during polling)
    if let Some(error) = body.get("error").and_then(|v| v.as_str()) {
        return match error {
            "authorization_pending" => DeviceFlowStatus::Pending,
            "slow_down" => {
                let interval = body
                    .get("interval")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(10);
                DeviceFlowStatus::SlowDown {
                    new_interval: interval,
                }
            }
            "expired_token" => DeviceFlowStatus::Expired,
            "access_denied" => DeviceFlowStatus::AccessDenied,
            _ => {
                let desc = body
                    .get("error_description")
                    .and_then(|v| v.as_str())
                    .unwrap_or(error);
                DeviceFlowStatus::Error(desc.to_string())
            }
        };
    }

    // Success — extract access token
    if let Some(token) = body.get("access_token").and_then(|v| v.as_str()) {
        DeviceFlowStatus::Complete {
            access_token: Zeroizing::new(token.to_string()),
        }
    } else {
        DeviceFlowStatus::Error("No access_token in response".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert!(GITHUB_DEVICE_CODE_URL.starts_with("https://"));
        assert!(GITHUB_TOKEN_URL.starts_with("https://"));
        assert!(!COPILOT_CLIENT_ID.is_empty());
    }
}
