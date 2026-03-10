use crate::error::{RegistryError, RegistryResult};
use base64::Engine;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

/// JWT claims stored in API tokens.
#[derive(Debug, Serialize, Deserialize)]
pub struct TokenClaims {
    /// Subject — the GitHub login of the user.
    pub sub: String,
    /// GitHub user ID.
    pub github_id: u64,
    /// Issued-at timestamp (Unix seconds).
    pub iat: u64,
    /// Expiration timestamp (Unix seconds). Tokens expire after 90 days.
    pub exp: u64,
}

/// Issue a new JWT API token for a user.
pub fn issue_token(
    github_login: &str,
    github_id: u64,
    secret: &[u8],
) -> RegistryResult<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let claims = TokenClaims {
        sub: github_login.to_string(),
        github_id,
        iat: now,
        exp: now + 90 * 24 * 3600, // 90 days
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret),
    )
    .map_err(|e| RegistryError::Internal(format!("Token encoding failed: {e}")))
}

/// Validate a JWT API token and return the claims.
pub fn validate_token(token: &str, secret: &[u8]) -> RegistryResult<TokenClaims> {
    decode::<TokenClaims>(
        token,
        &DecodingKey::from_secret(secret),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|e| match e.kind() {
        jsonwebtoken::errors::ErrorKind::ExpiredSignature => RegistryError::Unauthenticated,
        _ => RegistryError::Internal(format!("Token validation failed: {e}")),
    })
}

/// Hash an API token for storage (SHA-256, hex-encoded).
pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Extract the Bearer token from an Authorization header value.
pub fn extract_bearer(header_value: &str) -> Option<&str> {
    header_value.strip_prefix("Bearer ").map(str::trim)
}

/// Compute SHA-256 checksum of bytes, returning hex string.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Validate a package ID slug: lowercase alphanumeric + hyphens, 2-64 chars.
pub fn validate_package_id(id: &str) -> RegistryResult<()> {
    if id.len() < 2 || id.len() > 64 {
        return Err(RegistryError::InvalidPackageId(format!(
            "Package ID must be 2-64 characters, got {}",
            id.len()
        )));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(RegistryError::InvalidPackageId(
            "Package ID must contain only lowercase letters, digits, and hyphens".to_string(),
        ));
    }
    if id.starts_with('-') || id.ends_with('-') {
        return Err(RegistryError::InvalidPackageId(
            "Package ID must not start or end with a hyphen".to_string(),
        ));
    }
    Ok(())
}

/// Encode bytes as URL-safe base64 (no padding).
pub fn base64_encode(data: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

/// Decode URL-safe base64 bytes.
pub fn base64_decode(s: &str) -> RegistryResult<Vec<u8>> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s)
        .map_err(|e| RegistryError::InvalidManifest(format!("Base64 decode failed: {e}")))
}

/// A minimal user struct returned by `extract_user_from_request`.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub github_login: String,
    pub github_id: u64,
}

/// Extract and validate the authenticated user from the current Leptos/Axum request context.
/// Returns an error if no valid Bearer token is present.
pub async fn extract_user_from_request() -> crate::error::RegistryResult<AuthenticatedUser> {
    use axum::http::HeaderMap;
    use leptos_axum::extract;

    // Extract the JWT secret from the AppState extension
    let state: axum::extract::Extension<std::sync::Arc<crate::routes::AppState>> =
        extract().await.map_err(|_| crate::error::RegistryError::Unauthenticated)?;

    // Extract the Authorization header
    let headers: HeaderMap = extract().await.map_err(|_| crate::error::RegistryError::Unauthenticated)?;
    let auth_header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(crate::error::RegistryError::Unauthenticated)?;

    let token = extract_bearer(auth_header).ok_or(crate::error::RegistryError::Unauthenticated)?;
    let claims = validate_token(token, &state.jwt_secret)?;

    Ok(AuthenticatedUser {
        github_login: claims.sub,
        github_id: claims.github_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_issue_and_validate_token() {
        let secret = b"test-secret-key-32-bytes-minimum!";
        let token = issue_token("alice", 12345, secret).unwrap();
        let claims = validate_token(&token, secret).unwrap();
        assert_eq!(claims.sub, "alice");
        assert_eq!(claims.github_id, 12345);
    }

    #[test]
    fn test_validate_package_id() {
        assert!(validate_package_id("my-weather-hand").is_ok());
        assert!(validate_package_id("clip").is_ok());
        assert!(validate_package_id("a").is_err()); // too short
        assert!(validate_package_id("-bad").is_err()); // starts with hyphen
        assert!(validate_package_id("Bad_Name").is_err()); // uppercase + underscore
    }

    #[test]
    fn test_sha256_hex() {
        let hash = sha256_hex(b"hello");
        assert_eq!(hash.len(), 64); // 32 bytes = 64 hex chars
    }

    #[test]
    fn test_extract_bearer() {
        assert_eq!(extract_bearer("Bearer mytoken123"), Some("mytoken123"));
        assert_eq!(extract_bearer("Basic abc"), None);
    }
}
