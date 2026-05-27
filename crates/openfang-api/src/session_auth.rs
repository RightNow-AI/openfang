//! Stateless session token authentication for the dashboard.
//! Tokens are HMAC-SHA256 signed and contain username + expiry.

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Create a session token: base64(username:expiry_unix:hmac_hex)
pub fn create_session_token(username: &str, secret: &str, ttl_hours: u64) -> String {
    use base64::Engine;
    let expiry = chrono::Utc::now().timestamp() + (ttl_hours as i64 * 3600);
    let payload = format!("{username}:{expiry}");
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC key");
    mac.update(payload.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    base64::engine::general_purpose::STANDARD.encode(format!("{payload}:{signature}"))
}

/// Create a short-lived agent workspace download token.
pub fn create_download_token(
    agent_id: &str,
    rel_path: &str,
    secret: &str,
    ttl_seconds: u64,
) -> String {
    use base64::Engine;
    let expiry = chrono::Utc::now().timestamp() + ttl_seconds as i64;
    let path = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rel_path);
    let payload = format!("download:{agent_id}:{expiry}:{path}");
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC key");
    mac.update(payload.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    base64::engine::general_purpose::STANDARD.encode(format!("{payload}:{signature}"))
}

/// Extract the `openfang_session` cookie value from a `Cookie` header string.
///
/// Returns `None` if the header is absent or the cookie is not present.
/// Used by both the HTTP auth middleware and the WebSocket upgrade handler so
/// that browser sessions established via `sessionLogin()` are honored on both
/// surfaces (issue #1085).
pub fn extract_session_cookie(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|c| {
                c.trim()
                    .strip_prefix("openfang_session=")
                    .map(|v| v.to_string())
            })
        })
}

/// Verify a session token. Returns the username if valid and not expired.
pub fn verify_session_token(token: &str, secret: &str) -> Option<String> {
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(token)
        .ok()?;
    let decoded_str = String::from_utf8(decoded).ok()?;
    let parts: Vec<&str> = decoded_str.splitn(3, ':').collect();
    if parts.len() != 3 {
        return None;
    }
    let (username, expiry_str, provided_sig) = (parts[0], parts[1], parts[2]);

    let expiry: i64 = expiry_str.parse().ok()?;
    if chrono::Utc::now().timestamp() > expiry {
        return None;
    }

    let payload = format!("{username}:{expiry_str}");
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(payload.as_bytes());
    let expected_sig = hex::encode(mac.finalize().into_bytes());

    use subtle::ConstantTimeEq;
    if provided_sig.len() != expected_sig.len() {
        return None;
    }
    if provided_sig
        .as_bytes()
        .ct_eq(expected_sig.as_bytes())
        .into()
    {
        Some(username.to_string())
    } else {
        None
    }
}

/// Verify a download token for one exact agent/path pair.
pub fn verify_download_token(
    token: &str,
    secret: &str,
    expected_agent_id: &str,
    expected_rel_path: &str,
) -> bool {
    use base64::Engine;
    let decoded = match base64::engine::general_purpose::STANDARD.decode(token) {
        Ok(decoded) => decoded,
        Err(_) => return false,
    };
    let decoded_str = match String::from_utf8(decoded) {
        Ok(decoded) => decoded,
        Err(_) => return false,
    };
    let parts: Vec<&str> = decoded_str.splitn(5, ':').collect();
    if parts.len() != 5 {
        return false;
    }
    let (kind, agent_id, expiry_str, rel_path_b64, provided_sig) =
        (parts[0], parts[1], parts[2], parts[3], parts[4]);
    let rel_path_bytes = match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(rel_path_b64)
    {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let rel_path = match String::from_utf8(rel_path_bytes) {
        Ok(path) => path,
        Err(_) => return false,
    };
    if kind != "download" || agent_id != expected_agent_id || rel_path != expected_rel_path {
        return false;
    }

    let expiry: i64 = match expiry_str.parse() {
        Ok(expiry) => expiry,
        Err(_) => return false,
    };
    if chrono::Utc::now().timestamp() > expiry {
        return false;
    }

    let payload = format!("{kind}:{agent_id}:{expiry_str}:{rel_path_b64}");
    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(mac) => mac,
        Err(_) => return false,
    };
    mac.update(payload.as_bytes());
    let expected_sig = hex::encode(mac.finalize().into_bytes());

    use subtle::ConstantTimeEq;
    provided_sig.len() == expected_sig.len()
        && bool::from(provided_sig.as_bytes().ct_eq(expected_sig.as_bytes()))
}

/// Hash a password with Argon2id for config storage.
///
/// Returns a PHC-format string (e.g. `$argon2id$v=19$m=19456,t=2,p=1$...`).
pub fn hash_password(password: &str) -> String {
    use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
    let salt = SaltString::generate(&mut rand::thread_rng());
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("Argon2 hashing should not fail with valid inputs")
        .to_string()
}

/// Verify a password against a stored Argon2id hash (PHC string format).
pub fn verify_password(password: &str, stored_hash: &str) -> bool {
    use argon2::{password_hash::PasswordHash, Argon2, PasswordVerifier};
    let Ok(parsed) = PasswordHash::new(stored_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify_password() {
        let hash = hash_password("secret123");
        assert!(
            hash.starts_with("$argon2id$"),
            "should produce Argon2id PHC string"
        );
        assert!(verify_password("secret123", &hash));
        assert!(!verify_password("wrong", &hash));
    }

    #[test]
    fn test_hash_produces_unique_salts() {
        let h1 = hash_password("same");
        let h2 = hash_password("same");
        assert_ne!(h1, h2, "each hash should use a unique salt");
        assert!(verify_password("same", &h1));
        assert!(verify_password("same", &h2));
    }

    #[test]
    fn test_rejects_non_argon2_hash() {
        // A plain SHA256 hex string should no longer be accepted.
        use sha2::Digest;
        let sha256_hash = hex::encode(sha2::Sha256::digest(b"password"));
        assert!(!verify_password("password", &sha256_hash));
    }

    #[test]
    fn test_create_and_verify_token() {
        let token = create_session_token("admin", "my-secret", 1);
        let user = verify_session_token(&token, "my-secret");
        assert_eq!(user, Some("admin".to_string()));
    }

    #[test]
    fn test_download_token_is_agent_and_path_scoped() {
        let token = create_download_token("agent-a", "reports/final:v1.md", "secret", 300);

        assert!(verify_download_token(
            &token,
            "secret",
            "agent-a",
            "reports/final:v1.md"
        ));
        assert!(!verify_download_token(
            &token,
            "secret",
            "agent-b",
            "reports/final:v1.md"
        ));
        assert!(!verify_download_token(
            &token,
            "secret",
            "agent-a",
            "reports/other.md"
        ));
    }

    #[test]
    fn test_token_wrong_secret() {
        let token = create_session_token("admin", "my-secret", 1);
        let user = verify_session_token(&token, "wrong-secret");
        assert_eq!(user, None);
    }

    #[test]
    fn test_token_invalid_base64() {
        let user = verify_session_token("not-valid-base64!!!", "secret");
        assert_eq!(user, None);
    }

    #[test]
    fn test_rejects_garbage_input() {
        assert!(!verify_password("x", "short"));
        assert!(!verify_password("x", ""));
    }

    #[test]
    fn test_verify_malformed_argon2_hash() {
        // Starts with $argon2 but is not a valid PHC string.
        assert!(!verify_password("x", "$argon2id$garbage"));
    }

    #[test]
    fn test_extract_session_cookie_present() {
        let mut h = axum::http::HeaderMap::new();
        h.insert(
            "cookie",
            "foo=bar; openfang_session=abc.def.ghi; baz=qux"
                .parse()
                .unwrap(),
        );
        assert_eq!(extract_session_cookie(&h).as_deref(), Some("abc.def.ghi"));
    }

    #[test]
    fn test_extract_session_cookie_absent() {
        let mut h = axum::http::HeaderMap::new();
        h.insert("cookie", "foo=bar; baz=qux".parse().unwrap());
        assert_eq!(extract_session_cookie(&h), None);
    }

    #[test]
    fn test_extract_session_cookie_no_header() {
        let h = axum::http::HeaderMap::new();
        assert_eq!(extract_session_cookie(&h), None);
    }

    #[test]
    fn test_extract_session_cookie_only_value() {
        let mut h = axum::http::HeaderMap::new();
        h.insert("cookie", "openfang_session=lonely".parse().unwrap());
        assert_eq!(extract_session_cookie(&h).as_deref(), Some("lonely"));
    }
}
