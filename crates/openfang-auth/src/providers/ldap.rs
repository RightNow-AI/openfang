//! LDAP Authentication Provider Implementation
//!
//! This module provides LDAP/Active Directory authentication support for OpenFang.
//! It implements the `ExternalAuthProviderTrait` to enable:
//! - User authentication against LDAP directory
//! - Group membership resolution
//! - Automatic user provisioning based on directory groups
//! - Role mapping from LDAP groups to OpenFang RBAC roles
//!
//! # Security Features
//! - LDAPS (LDAP over SSL/TLS) support with certificate validation
//! - Self-signed certificate support via CA certificate path configuration
//! - Credential separation (bind DN/password stored in environment/vault)
//! - Connection timeout enforcement
//! - StartTLS support as alternative to LDAPS
//!
//! # Configuration Example
//! ```toml
//! [[external_auth_providers]]
//! type = "ldap"
//! name = "active-directory"
//! uri = "ldaps://dc01.example.com:636"
//! bind_dn = "CN=Service Account,OU=Service Accounts,DC=example,DC=com"
//! bind_password_env = "LDAP_BIND_PASSWORD"
//! base_dn = "DC=example,DC=com"
//!
//! [external_auth_providers.attribute_mapping]
//! user_id_attr = "sAMAccountName"
//! name_attr = "displayName"
//! email_attr = "mail"
//! group_attr = "memberOf"
//!
//! [external_auth_providers.role_mappings]
//! { group_pattern = "CN=OpenFang-Admins,OU=Groups,DC=example,DC=com", role = "admin" }
//! ```

use std::collections::HashSet;
use std::time::Duration;
use std::sync::Arc;

use async_trait::async_trait;
use native_tls::{Certificate, TlsConnector};
use regex::Regex;
use tokio_native_tls::{native_tls, TlsConnectorExt};

use crate::auth::{ExternalAuthProviderTrait, AuthCredentials, AuthResult, AuthError};
use openfang_types::config::ExternalAuthConfig;
use openfang_types::agent::UserId;

/// LDAP Authentication Provider
///
/// Handles authentication against LDAP/Active Directory servers with support for:
/// - LDAPS (port 636) and StartTLS
/// - Self-signed certificate validation
/// - Group membership resolution
/// - Dynamic user provisioning
/// - Role mapping from directory groups
#[derive(Clone)]
pub struct LdapAuthProvider {
    /// Configuration for this LDAP provider
    config: Arc<ExternalAuthConfig>,
    /// Regex patterns for role mapping (pre-compiled for performance)
    role_patterns: Vec<RoleMappingRule>,
    /// Cache for authenticated users (in-memory, short-lived)
    user_cache: Arc<tokio::sync::RwLock<HashSet<UserId>>>,
}

/// Role mapping rule combining regex pattern matching with role assignment
struct RoleMappingRule {
    /// Compiled regex pattern for group DN matching
    pattern: Regex,
    /// OpenFang role to assign when pattern matches
    role: String,
}

impl LdapAuthProvider {
    /// Create a new LDAP authentication provider from configuration
    ///
    /// # Arguments
    /// * `config` - LDAP provider configuration
    ///
    /// # Returns
    /// * `Ok(Self)` - Provider initialized successfully
    /// * `Err(AuthError)` - Configuration or setup failure
    pub fn new(config: ExternalAuthConfig) -> Result<Self, AuthError> {
        // Compile role mapping patterns
        let role_patterns = config
            .role_mappings
            .iter()
            .map(|mapping| {
                let pattern = mapping.group_pattern.replace('.', r"\.").replace('*', ".*");
                Regex::new(&format!("^{}$", pattern))
                    .map(|regex| RoleMappingRule {
                        pattern: regex,
                        role: mapping.role.clone(),
                    })
                    .map_err(|e| AuthError::InternalError(e.into()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let user_cache = Arc::new(tokio::sync::RwLock::new(HashSet::new()));

        Ok(Self {
            config: Arc::new(config),
            role_patterns,
            user_cache,
        })
    }

    /// Parse the bind password from environment variable
    fn get_bind_password(&self) -> Result<String, AuthError> {
        self.config
            .bind_password_env
            .as_ref()
            .and_then(|env_var| std::env::var(env_var).ok())
            .ok_or_else(|| AuthError::ConfigurationError("LDAP_BIND_PASSWORD not set".into()))
    }

    /// Build TLS connector with custom CA certificate support
    fn build_tls_connector(&self) -> Result<TlsConnector, AuthError> {
        let mut builder = native_tls::TlsConnector::builder();

        // Load CA certificate if specified (for self-signed server certs)
        if let Some(ca_cert_path) = &self.config.connection.ca_cert_path {
            let ca_cert_data = std::fs::read(ca_cert_path)
                .map_err(|e| AuthError::ConfigurationError(format!("Failed to read CA cert: {}", e)))?;
            
            let cert = Certificate::from_pem(&ca_cert_data)
                .map_err(|e| AuthError::ConfigurationError(format!("Invalid CA certificate: {}", e)))?;
            
            builder.add_root_certificate(cert);
        }

        // Configure certificate verification
        if self.config.connection.disable_tls_verify.unwrap_or(false) {
            warn!("WARNING: TLS certificate verification disabled for LDAP provider '{}'!", 
                  self.config.name);
            builder.danger_accept_invalid_certs(true);
        }

        builder.build()
            .map_err(|e| AuthError::ConfigurationError(format!("Failed to build TLS connector: {}", e)))
    }

    /// Create LDAP connection with TLS support
    async fn create_ldap_connection(&self) -> Result<ldap3::LdapConnAsync, AuthError> {
        let tls_connector = self.build_tls_connector()?;
        
        let url = self.config.uri.as_str();
        
        // Parse URL to determine connection type
        let (connector, url) = if url.starts_with("ldaps://") {
            // Direct LDAPS connection
            (Some(tls_connector), url.to_string())
        } else if self.config.connection.start_tls.unwrap_or(false) {
            // Clear text LDAP with StartTLS upgrade
            (Some(tls_connector), format!("ldap://{}", &url["ldap://".len()..]))
        } else {
            // Plain LDAP (not recommended for production)
            (None, url.to_string())
        };

        let settings = ldap3::LdapConnSettings::new()
            .set_conn_timeout(Duration::from_secs(self.config.connection.timeout_secs))
            .set_starttls(self.config.connection.start_tls.unwrap_or(false));

        let (conn, mut ldap) = if let Some(tls) = connector {
            settings
                .set_connector(tls)
                .from_url(&url)
                .await
                .map_err(|e| AuthError::ConnectionError(format!("LDAP connection failed: {}", e)))?
        } else {
            ldap3::LdapConnAsync::from_url(&url)
                .await
                .map_err(|e| AuthError::ConnectionError(format!("LDAP connection failed: {}", e)))?
        };

        ldap3::drive!(conn);
        Ok(ldap)
    }

    /// Authenticate user with credentials
    async fn authenticate_user(&self, username: &str, password: &str) -> Result<ldap3::Entry, AuthError> {
        // Attempt direct bind with user DN
        let bind_dn = self.build_user_dn(username)?;
        
        let mut ldap = self.create_ldap_connection().await?;
        
        // Bind as user
        ldap.bind(&bind_dn, password)
            .await
            .map_err(|e| AuthError::AuthenticationFailed(format!("LDAP bind failed: {}", e)))?
            .success()
            .map_err(|e| AuthError::AuthenticationFailed(format!("Bind rejected: {}", e)))?;

        // Search for user entry to get full profile
        let user_filter = self.config.attribute_mapping.user_filter
            .as_ref()
            .map(|f| f.replace("{username}", username))
            .unwrap_or_else(|| format!("(&(objectClass=user)(sAMAccountName={}))", username));

        let search_base = &self.config.base_dn;
        
        let attrs: Vec<String> = vec![
            self.config.attribute_mapping.user_id_attr.clone(),
            self.config.attribute_mapping.name_attr.clone(),
            self.config.attribute_mapping.email_attr.clone(),
            self.config.attribute_mapping.group_attr.clone(),
        ];

        let (mut search_results, result) = ldap
            .search(search_base, ldap3::Scope::Subtree, &user_filter, attrs)
            .await
            .map_err(|e| AuthError::InternalError(e.into()))?
            .success()
            .map_err(|e| AuthError::AuthenticationFailed(format!("Search failed: {}", e)))?;

        // Get first result (should be only user entry)
        let entry = search_results
            .pop()
            .ok_or_else(|| AuthError::AuthenticationFailed("User not found in directory".into()))?;

        Ok(entry)
    }

    /// Build user's DN from username and configuration
    fn build_user_dn(&self, username: &str) -> Result<String, AuthError> {
        // Check if custom user_dn_template is configured
        if let Some(template) = &self.config.attribute_mapping.user_dn_template {
            let dn = template.replace("{username}", username)
                .replace("{user_id}", username);
            Ok(dn)
        } else {
            // Default: assume username is the sAMAccountName and build standard AD DN
            Ok(format!(
                "CN={},{}",
                username,
                self.config.base_dn
            ))
        }
    }

    /// Extract user attributes from LDAP entry
    fn extract_user_attributes(&self, entry: &ldap3::Entry) -> Result<serde_json::Value, AuthError> {
        let mut attrs = serde_json::Map::new();

        // Extract user ID
        if let Some(user_id_attr) = entry.get_first_value_str(&self.config.attribute_mapping.user_id_attr) {
            attrs.insert("user_id".to_string(), serde_json::Value::String(user_id_attr));
        }

        // Extract display name
        if let Some(name_attr) = entry.get_first_value_str(&self.config.attribute_mapping.name_attr) {
            attrs.insert("name".to_string(), serde_json::Value::String(name_attr));
        }

        // Extract email
        if let Some(email_attr) = entry.get_first_value_str(&self.config.attribute_mapping.email_attr) {
            attrs.insert("email".to_string(), serde_json::Value::String(email_attr));
        }

        // Extract groups
        if let Some(groups_val) = self.extract_groups(entry)? {
            attrs.insert("groups".to_string(), serde_json::Value::Array(groups_val));
        }

        Ok(serde_json::Value::Object(attrs))
    }

    /// Extract group membership from LDAP entry
    fn extract_groups(&self, entry: &ldap3::Entry) -> Result<Option<Vec<serde_json::Value>>, AuthError> {
        let group_attr_name = &self.config.attribute_mapping.group_attr;
        
        // Try to get group values
        let groups = entry.get_all_values(group_attr_name);
        
        if groups.is_empty() {
            return Ok(None);
        }

        // Extract group DNs
        let group_dns: Vec<String> = groups.iter()
            .filter_map(|g| g.as_str())
            .map(|s| s.to_string())
            .collect();

        Ok(Some(group_dns.into_iter()
            .map(|g| serde_json::Value::String(g))
            .collect()))
    }

    /// Map LDAP groups to OpenFang roles
    fn map_groups_to_roles(&self, groups: &[String]) -> String {
        let mut highest_role = None;
        let mut highest_level = 0;

        for group_dn in groups {
            for rule in &self.role_patterns {
                if rule.pattern.is_match(group_dn) {
                    // Get role level (higher = more privileged)
                    let level = Self::get_role_level(&rule.role);
                    if level > highest_level {
                        highest_level = level;
                        highest_role = Some(rule.role.clone());
                    }
                }
            }
        }

        highest_role.unwrap_or_else(|| "viewer".to_string())
    }

    /// Get numeric level for role (for comparison)
    fn get_role_level(role: &str) -> u8 {
        match role {
            "owner" => 4,
            "admin" => 3,
            "user" => 2,
            _ => 1, // viewer or unknown
        }
    }

    /// Sync all users from directory (for batch provisioning)
    async fn sync_all_users(&self) -> Result<Vec<AuthResult>, AuthError> {
        let mut ldap = self.create_ldap_connection().await?;
        
        // Bind as service account
        let bind_password = self.get_bind_password()?;
        ldap.bind(&self.config.bind_dn, &bind_password)
            .await
            .map_err(|e| AuthError::ConnectionError(format!("Service bind failed: {}", e)))?
            .success()
            .map_err(|e| AuthError::ConnectionError(format!("Service bind rejected: {}", e)))?;

        // Search for all users
        let user_filter = self.config.attribute_mapping.user_filter
            .as_ref()
            .unwrap_or("(&(objectClass=user)(objectClass=person))");

        let attrs: Vec<String> = vec![
            self.config.attribute_mapping.user_id_attr.clone(),
            self.config.attribute_mapping.name_attr.clone(),
            self.config.attribute_mapping.email_attr.clone(),
            self.config.attribute_mapping.group_attr.clone(),
        ];

        let search_base = &self.config.base_dn;
        
        let (results, _) = ldap
            .search(search_base, ldap3::Scope::Subtree, user_filter, attrs)
            .await
            .map_err(|e| AuthError::InternalError(e.into()))?
            .success()
            .map_err(|e| AuthError::InternalError(e.into()))?;

        // Process each user entry
        let mut auth_results = Vec::new();
        for entry in results {
            match self.process_user_entry(entry) {
                Ok(auth_result) => auth_results.push(auth_result),
                Err(e) => {
                    error!("Failed to process user entry: {}", e);
                    // Continue processing other users
                }
            }
        }

        Ok(auth_results)
    }

    /// Process a single LDAP user entry
    fn process_user_entry(&self, entry: ldap3::Entry) -> Result<AuthResult, AuthError> {
        let attributes = self.extract_user_attributes(&entry)?;
        let groups = attributes.get("groups")
            .and_then(|g| g.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()).collect::<Vec<_>>())
            .unwrap_or_default();

        let role = self.map_groups_to_roles(&groups);

        let user_id = attributes.get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(AuthResult {
            user_id: UserId::new(user_id),
            role: role.clone(),
            attributes,
            provider: self.config.name.clone(),
            last_sync: chrono::Utc::now(),
        })
    }
}

#[async_trait]
impl ExternalAuthProviderTrait for LdapAuthProvider {
    /// Authenticate a user against LDAP directory
    ///
    /// # Arguments
    /// * `credentials` - Username and password
    ///
    /// # Returns
    /// * `Ok(AuthResult)` - User authenticated successfully with profile data
    /// * `Err(AuthError)` - Authentication failed
    async fn authenticate(&self, credentials: &AuthCredentials) -> Result<AuthResult, AuthError> {
        let username = &credentials.username;
        let password = &credentials.password;

        // Authenticate user
        let entry = self.authenticate_user(username, password).await?;

        // Extract user attributes and map to OpenFang
        let attributes = self.extract_user_attributes(&entry)?;
        
        // Get groups and determine role
        let groups = attributes.get("groups")
            .and_then(|g| g.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()).collect::<Vec<_>>())
            .unwrap_or_default();

        let role = self.map_groups_to_roles(&groups);

        let user_id = attributes.get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or(username)
            .to_string();

        let result = AuthResult {
            user_id: UserId::new(user_id.clone()),
            role: role.clone(),
            attributes,
            provider: self.config.name.clone(),
            last_sync: chrono::Utc::now(),
        };

        // Cache authenticated user
        {
            let mut cache = self.user_cache.write().await;
            cache.insert(UserId::new(user_id));
        }

        Ok(result)
    }

    /// Sync all users from directory (for batch provisioning)
    async fn sync_users(&self) -> Result<Vec<AuthResult>, AuthError> {
        self.sync_all_users().await
    }

    /// Get user by external ID (from directory)
    async fn get_user_by_external_id(&self, external_id: &str) -> Result<Option<AuthResult>, AuthError> {
        let mut ldap = self.create_ldap_connection().await?;
        
        // Bind as service account
        let bind_password = self.get_bind_password()?;
        ldap.bind(&self.config.bind_dn, &bind_password)
            .await
            .map_err(|e| AuthError::ConnectionError(format!("Service bind failed: {}", e)))?
            .success()
            .map_err(|e| AuthError::ConnectionError(format!("Service bind rejected: {}", e)))?;

        // Search for user
        let user_filter = self.config.attribute_mapping.user_filter
            .as_ref()
            .unwrap_or("(&(objectClass=user)(objectClass=person))")
            .replace("{username}", external_id);

        let attrs: Vec<String> = vec![
            self.config.attribute_mapping.user_id_attr.clone(),
            self.config.attribute_mapping.name_attr.clone(),
            self.config.attribute_mapping.email_attr.clone(),
            self.config.attribute_mapping.group_attr.clone(),
        ];

        let search_base = &self.config.base_dn;
        
        let (results, _) = ldap
            .search(search_base, ldap3::Scope::Subtree, &user_filter, attrs)
            .await
            .map_err(|e| AuthError::InternalError(e.into()))?
            .success()
            .map_err(|e| AuthError::InternalError(e.into()))?;

        if let Some(entry) = results.first() {
            Ok(Some(self.process_user_entry(entry.clone())?))
        } else {
            Ok(None)
        }
    }

    /// Refresh user's group membership and role
    async fn refresh_user(&self, user_id: UserId) -> Result<AuthResult, AuthError> {
        // Get user by external ID (user_id maps to sAMAccountName)
        let external_id = user_id.to_string();
        match self.get_user_by_external_id(&external_id).await? {
            Some(result) => Ok(result),
            None => Err(AuthError::UserNotFound),
        }
    }

    /// Validate session (not applicable for pure LDAP, but required by trait)
    async fn validate_session(&self, _session_token: &str) -> Result<Option<UserId>, AuthError> {
        Err(AuthError::NotImplemented("LDAP does not support session validation".into()))
    }

    fn provider_type(&self) -> &str {
        "ldap"
    }

    fn provider_name(&self) -> &str {
        &self.config.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_role_mapping() {
        let config = ExternalAuthConfig {
            name: "test-ad".to_string(),
            uri: "ldaps://dc01.example.com:636".to_string(),
            bind_dn: "CN=Service Account,OU=Service Accounts,DC=example,DC=com".to_string(),
            bind_password_env: Some("LDAP_BIND_PASSWORD".to_string()),
            base_dn: "DC=example,DC=com".to_string(),
            attribute_mapping: AttributeMapping {
                user_id_attr: "sAMAccountName".to_string(),
                name_attr: "displayName".to_string(),
                email_attr: "mail".to_string(),
                group_attr: "memberOf".to_string(),
                user_filter: Some("(&(objectClass=user)(sAMAccountName={username}))".to_string()),
                user_dn_template: None,
            },
            role_mappings: vec![
                RoleMappingRule {
                    group_pattern: "CN=OpenFang-Admins,OU=Groups,DC=example,DC=com".to_string(),
                    role: "admin".to_string(),
                },
                RoleMappingRule {
                    group_pattern: "CN=OpenFang-Users,OU=Groups,DC=example,DC=com".to_string(),
                    role: "user".to_string(),
                },
            ],
            connection: ConnectionConfig {
                timeout_secs: 30,
                tls_enabled: true,
                ca_cert_path: None,
                start_tls: Some(false),
                disable_tls_verify: Some(false),
            },
        };

        let provider = LdapAuthProvider::new(config).unwrap();
        
        let groups = vec![
            "CN=OpenFang-Admins,OU=Groups,DC=example,DC=com".to_string(),
            "CN=OpenFang-Users,OU=Groups,DC=example,DC=com".to_string(),
        ];

        let role = provider.map_groups_to_roles(&groups);
        assert_eq!(role, "admin");
    }

    #[tokio::test]
    async fn test_higher_role_preferred() {
        let config = ExternalAuthConfig {
            name: "test-ad".to_string(),
            uri: "ldaps://dc01.example.com:636".to_string(),
            bind_dn: "CN=Service Account,OU=Service Accounts,DC=example,DC=com".to_string(),
            bind_password_env: Some("LDAP_BIND_PASSWORD".to_string()),
            base_dn: "DC=example,DC=com".to_string(),
            attribute_mapping: AttributeMapping {
                user_id_attr: "sAMAccountName".to_string(),
                name_attr: "displayName".to_string(),
                email_attr: "mail".to_string(),
                group_attr: "memberOf".to_string(),
                user_filter: None,
                user_dn_template: None,
            },
            role_mappings: vec![
                RoleMappingRule {
                    group_pattern: "CN=OpenFang-Admins,OU=Groups,DC=example,DC=com".to_string(),
                    role: "admin".to_string(),
                },
                RoleMappingRule {
                    group_pattern: "CN=OpenFang-Owners,OU=Groups,DC=example,DC=com".to_string(),
                    role: "owner".to_string(),
                },
            ],
            connection: ConnectionConfig {
                timeout_secs: 30,
                tls_enabled: true,
                ca_cert_path: None,
                start_tls: Some(false),
                disable_tls_verify: Some(false),
            },
        };

        let provider = LdapAuthProvider::new(config).unwrap();
        
        // Owner role should be preferred over admin
        let groups = vec![
            "CN=OpenFang-Admins,OU=Groups,DC=example,DC=com".to_string(),
            "CN=OpenFang-Owners,OU=Groups,DC=example,DC=com".to_string(),
        ];

        let role = provider.map_groups_to_roles(&groups);
        assert_eq!(role, "owner");
    }
}