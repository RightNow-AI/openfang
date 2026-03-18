//! Stateless session token authentication for the dashboard.
//! Tokens are HMAC-SHA256 signed and contain username + expiry.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use hmac::{Hmac, Mac};
use rand::rngs::OsRng;
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

fn legacy_sha256_hash(password: &str) -> String {
    use sha2::Digest;
    hex::encode(Sha256::digest(password.as_bytes()))
}

fn is_legacy_sha256_hash_format(stored_hash: &str) -> bool {
    stored_hash.len() == 64 && stored_hash.chars().all(|c| c.is_ascii_hexdigit())
}

/// Return true when the stored password hash uses a supported format.
///
/// Supported formats:
/// - Argon2 PHC strings (recommended)
/// - legacy SHA-256 hex digests kept for backward compatibility
pub fn is_supported_password_hash_format(stored_hash: &str) -> bool {
    let stored_hash = stored_hash.trim();
    if stored_hash.starts_with("$argon2") {
        return PasswordHash::new(stored_hash).is_ok();
    }
    is_legacy_sha256_hash_format(stored_hash)
}

/// Hash a password for config storage using Argon2id (PHC string format).
///
/// Legacy SHA-256 hex hashes are still accepted by [`verify_password`] so
/// existing config files continue to work after upgrade.
pub fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| format!("failed to hash password: {e}"))
}

/// Verify a password against a stored hash.
///
/// Supports both Argon2 PHC strings (preferred) and legacy SHA-256 hex hashes.
pub fn verify_password(password: &str, stored_hash: &str) -> bool {
    let stored_hash = stored_hash.trim();
    if stored_hash.starts_with("$argon2") {
        let parsed = match PasswordHash::new(stored_hash) {
            Ok(parsed) => parsed,
            Err(_) => return false,
        };
        return Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok();
    }

    if !is_legacy_sha256_hash_format(stored_hash) {
        return false;
    }

    let computed = legacy_sha256_hash(password);
    use subtle::ConstantTimeEq;
    if computed.len() != stored_hash.len() {
        return false;
    }
    computed.as_bytes().ct_eq(stored_hash.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify_password() {
        let hash = hash_password("secret123").unwrap();
        assert!(hash.starts_with("$argon2"));
        assert!(verify_password("secret123", &hash));
        assert!(!verify_password("wrong", &hash));
    }

    #[test]
    fn test_verify_legacy_sha256_password() {
        let hash = legacy_sha256_hash("secret123");
        assert!(verify_password("secret123", &hash));
        assert!(!verify_password("wrong", &hash));
    }

    #[test]
    fn test_supported_password_hash_format_argon2() {
        let hash = hash_password("secret123").unwrap();
        assert!(is_supported_password_hash_format(&hash));
    }

    #[test]
    fn test_supported_password_hash_format_legacy_sha256() {
        let hash = legacy_sha256_hash("secret123");
        assert!(is_supported_password_hash_format(&hash));
    }

    #[test]
    fn test_supported_password_hash_format_rejects_invalid_value() {
        assert!(!is_supported_password_hash_format("not-a-real-hash"));
        assert!(!verify_password("secret123", "not-a-real-hash"));
    }

    #[test]
    fn test_create_and_verify_token() {
        let token = create_session_token("admin", "my-secret", 1);
        let user = verify_session_token(&token, "my-secret");
        assert_eq!(user, Some("admin".to_string()));
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
    fn test_password_hash_length_mismatch() {
        assert!(!verify_password("x", "short"));
    }
}
