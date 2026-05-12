//! External authentication provider trait and types.
//!
//! This module defines the trait interface for external directory authentication
//! providers (LDAP, SAML, OIDC) and common types used across all providers.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

use openfang_types::config::ExternalAuthConfig;
use openfang_types::agent::UserId;

/// Authentication credentials for external directory providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthCredentials {
    /// Username or email for authentication.
    pub username: String,
    /// Password for authentication.
    pub password: String,
    /// Optional additional context (e.g., IP address, user agent).
    #[serde(default)]
    pub context: HashMap<String, String>,
}

/// Result of a successful authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResult {
    /// OpenFang user ID (auto-generated or from directory).
    pub user_id: UserId,
    /// Assigned role based on directory group membership.
    pub role: String,
    /// User profile attributes from directory.
    pub attributes: serde_json::Value,
    /// External authentication provider name.
    pub provider: String,
    /// Last sync timestamp.
    pub last_sync: chrono::DateTime<chrono::Utc>,
}

/// Errors that can occur during authentication.
#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("User not found in directory")]
    UserNotFound,

    #[error("Token expired or invalid")]
    TokenExpired,

    #[error("Role mapping failed: {0}")]
    RoleMappingError(String),

    #[error("Group sync failed: {0}")]
    GroupSyncError(String),

    #[error("Not implemented: {0}")]
    NotImplemented(String),

    #[error("Internal error: {0}")]
    InternalError(#[from] Box<dyn std::error::Error + Send + Sync>),
}

/// Trait for external directory authentication providers.
///
/// All directory providers (LDAP, SAML, OIDC) must implement this trait
/// to integrate with OpenFang's RBAC system.
#[async_trait]
pub trait ExternalAuthProviderTrait: Send + Sync {
    /// Authenticate user and return identity.
    ///
    /// # Arguments
    /// * `credentials` - Username and password
    ///
    /// # Returns
    /// * `Ok(AuthResult)` - User authenticated successfully with profile data
    /// * `Err(AuthError)` - Authentication failed
    async fn authenticate(&self, credentials: &AuthCredentials) -> Result<AuthResult, AuthError>;

    /// Sync users from directory (for batch provisioning).
    ///
    /// # Returns
    /// * `Ok(Vec<AuthResult>)` - List of users synced from directory
    /// * `Err(AuthError)` - Sync failed
    async fn sync_users(&self) -> Result<Vec<AuthResult>, AuthError>;

    /// Get user by external ID.
    ///
    /// # Arguments
    /// * `external_id` - User identifier from directory (e.g., sAMAccountName)
    ///
    /// # Returns
    /// * `Ok(Some(AuthResult))` - User found
    /// * `Ok(None)` - User not found
    /// * `Err(AuthError)` - Error occurred
    async fn get_user_by_external_id(&self, external_id: &str) -> Result<Option<AuthResult>, AuthError>;

    /// Refresh user's group membership and role.
    ///
    /// # Arguments
    /// * `user_id` - OpenFang user ID
    ///
    /// # Returns
    /// * `Ok(AuthResult)` - User refreshed with updated groups/role
    /// * `Err(AuthError)` - Refresh failed
    async fn refresh_user(&self, user_id: UserId) -> Result<AuthResult, AuthError>;

    /// Validate session/token.
    ///
    /// # Arguments
    /// * `session_token` - Session token to validate
    ///
    /// # Returns
    /// * `Ok(Some(UserId))` - Token valid, returns user ID
    /// * `Ok(None)` - Token invalid or expired
    /// * `Err(AuthError)` - Error occurred
    async fn validate_session(&self, session_token: &str) -> Result<Option<UserId>, AuthError>;

    /// Get provider type (e.g., "ldap", "saml", "oidc").
    fn provider_type(&self) -> &str;

    /// Get provider name (human-readable).
    fn provider_name(&self) -> &str;
}

/// Factory function to create authentication providers from configuration.
pub fn create_provider(config: ExternalAuthConfig) -> Result<Box<dyn ExternalAuthProviderTrait>, AuthError> {
    match config.provider_type.as_str() {
        "ldap" => {
            #[cfg(feature = "ldap")]
            {
                use crate::providers::ldap::LdapAuthProvider;
                Ok(Box::new(LdapAuthProvider::new(config)?))
            }
            #[cfg(not(feature = "ldap"))]
            Err(AuthError::ConfigurationError(
                "LDAP provider not compiled in. Enable 'ldap' feature.".into(),
            ))
        }
        "saml" => Err(AuthError::NotImplemented(
            "SAML provider not yet implemented".into(),
        )),
        "oidc" => Err(AuthError::NotImplemented(
            "OIDC provider not yet implemented".into(),
        )),
        _ => Err(AuthError::ConfigurationError(format!(
            "Unknown provider type: {}",
            config.provider_type
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_error_display() {
        let err = AuthError::AuthenticationFailed("Invalid credentials".into());
        assert_eq!(
            format!("{}", err),
            "Authentication failed: Invalid credentials"
        );
    }

    #[test]
    fn test_auth_credentials_serialization() {
        let creds = AuthCredentials {
            username: "testuser".to_string(),
            password: "testpass".to_string(),
            context: HashMap::new(),
        };

        let json = serde_json::to_string(&creds).unwrap();
        assert!(json.contains("testuser"));
        assert!(json.contains("testpass"));
    }
}