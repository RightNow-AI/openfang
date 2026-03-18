//! Credential resolution chain — resolves secrets from multiple sources.
//!
//! Resolution order:
//! 1. Encrypted vault (`~/.openfang/vault.enc`)
//! 2. Runtime env files (`~/.openfang/secrets.env` overrides `~/.openfang/.env`)
//! 3. Process environment variable
//! 4. Interactive prompt (CLI only, when `interactive` is true)

use crate::vault::CredentialVault;
use crate::ExtensionResult;
use std::collections::HashMap;
use std::path::Path;
use tracing::debug;
use zeroize::Zeroizing;

/// Credential resolver — tries multiple sources in priority order.
pub struct CredentialResolver {
    /// Reference to the credential vault.
    vault: Option<CredentialVault>,
    /// Runtime env entries loaded from `.env` and sibling `secrets.env`.
    dotenv: HashMap<String, String>,
    /// Whether to prompt interactively as a last resort.
    interactive: bool,
}

impl CredentialResolver {
    /// Create a resolver with optional vault and dotenv path.
    ///
    /// When the provided path is `.../.env`, the resolver also loads the sibling
    /// `secrets.env` file. Values in `secrets.env` take precedence over `.env`
    /// so dashboard/API writes remain effective after daemon restart.
    pub fn new(vault: Option<CredentialVault>, dotenv_path: Option<&Path>) -> Self {
        let dotenv = if let Some(path) = dotenv_path {
            load_runtime_env_files(path).unwrap_or_default()
        } else {
            HashMap::new()
        };
        Self {
            vault,
            dotenv,
            interactive: false,
        }
    }

    /// Enable interactive prompting as a last-resort source.
    pub fn with_interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }

    /// Resolve a credential by key, trying all sources in order.
    pub fn resolve(&self, key: &str) -> Option<Zeroizing<String>> {
        // 1. Vault
        if let Some(ref vault) = self.vault {
            if vault.is_unlocked() {
                if let Some(val) = vault.get(key) {
                    debug!("Credential '{}' resolved from vault", key);
                    return Some(val);
                }
            }
        }

        // 2. Runtime env files
        if let Some(val) = self.dotenv.get(key) {
            debug!("Credential '{}' resolved from runtime env file", key);
            return Some(Zeroizing::new(val.clone()));
        }

        // 3. Environment variable
        if let Ok(val) = std::env::var(key) {
            debug!("Credential '{}' resolved from env var", key);
            return Some(Zeroizing::new(val));
        }

        // 4. Interactive prompt (CLI only)
        if self.interactive {
            if let Some(val) = prompt_secret(key) {
                debug!("Credential '{}' resolved from interactive prompt", key);
                return Some(val);
            }
        }

        None
    }

    /// Check if a credential is available (without prompting).
    pub fn has_credential(&self, key: &str) -> bool {
        // Check vault
        if let Some(ref vault) = self.vault {
            if vault.is_unlocked() && vault.get(key).is_some() {
                return true;
            }
        }
        // Check dotenv
        if self.dotenv.contains_key(key) {
            return true;
        }
        // Check env
        std::env::var(key).is_ok()
    }

    /// Resolve all required credentials for an integration.
    /// Returns a map of env_var_name -> value for all resolved credentials.
    pub fn resolve_all(&self, keys: &[&str]) -> HashMap<String, Zeroizing<String>> {
        let mut result = HashMap::new();
        for key in keys {
            if let Some(val) = self.resolve(key) {
                result.insert(key.to_string(), val);
            }
        }
        result
    }

    /// Check which credentials are missing.
    pub fn missing_credentials(&self, keys: &[&str]) -> Vec<String> {
        keys.iter()
            .filter(|k| !self.has_credential(k))
            .map(|k| k.to_string())
            .collect()
    }

    /// Store a credential in the vault (if available).
    pub fn store_in_vault(&mut self, key: &str, value: Zeroizing<String>) -> ExtensionResult<()> {
        if let Some(ref mut vault) = self.vault {
            vault.set(key.to_string(), value)?;
            Ok(())
        } else {
            Err(crate::ExtensionError::Vault(
                "No vault configured".to_string(),
            ))
        }
    }

    /// Remove a credential from the vault (if available).
    pub fn remove_from_vault(&mut self, key: &str) -> ExtensionResult<bool> {
        if let Some(ref mut vault) = self.vault {
            vault.remove(key)
        } else {
            Err(crate::ExtensionError::Vault(
                "No vault configured".to_string(),
            ))
        }
    }
}

/// Load runtime env files into a HashMap.
///
/// If `path` points at `.env`, also loads sibling `secrets.env` on top so
/// dashboard/API-managed secrets override stale `.env` values.
fn load_runtime_env_files(path: &Path) -> Result<HashMap<String, String>, std::io::Error> {
    let mut map = HashMap::new();

    extend_env_map(&mut map, &load_dotenv(path)?);

    if path.file_name().and_then(|name| name.to_str()) == Some(".env") {
        let secrets_path = path.with_file_name("secrets.env");
        extend_env_map(&mut map, &load_dotenv(&secrets_path)?);
    }

    Ok(map)
}

fn extend_env_map(target: &mut HashMap<String, String>, overlay: &HashMap<String, String>) {
    for (key, value) in overlay {
        target.insert(key.clone(), value.clone());
    }
}

/// Load a single dotenv file into a HashMap.
fn load_dotenv(path: &Path) -> Result<HashMap<String, String>, std::io::Error> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let content = std::fs::read_to_string(path)?;
    let mut map = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let mut value = value.trim().to_string();
            // Strip surrounding quotes
            if (value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\''))
            {
                value = value[1..value.len() - 1].to_string();
            }
            map.insert(key.to_string(), value);
        }
    }
    Ok(map)
}

/// Prompt the user interactively for a secret value.
fn prompt_secret(key: &str) -> Option<Zeroizing<String>> {
    use std::io::{self, Write};

    eprint!("Enter value for {}: ", key);
    io::stderr().flush().ok()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input).ok()?;
    let trimmed = input.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(Zeroizing::new(trimmed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_dotenv_basic() {
        let dir = tempfile::tempdir().unwrap();
        let env_path = dir.path().join(".env");
        std::fs::write(
            &env_path,
            r#"
# Comment
GITHUB_TOKEN=ghp_test123
SLACK_TOKEN="xoxb-quoted"
EMPTY=
SINGLE_QUOTED='single'
"#,
        )
        .unwrap();

        let map = load_dotenv(&env_path).unwrap();
        assert_eq!(map.get("GITHUB_TOKEN").unwrap(), "ghp_test123");
        assert_eq!(map.get("SLACK_TOKEN").unwrap(), "xoxb-quoted");
        assert_eq!(map.get("EMPTY").unwrap(), "");
        assert_eq!(map.get("SINGLE_QUOTED").unwrap(), "single");
    }

    #[test]
    fn load_dotenv_nonexistent() {
        let map = load_dotenv(Path::new("/nonexistent/.env")).unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn load_runtime_env_files_merges_sibling_secrets_env() {
        let dir = tempfile::tempdir().unwrap();
        let env_path = dir.path().join(".env");
        let secrets_path = dir.path().join("secrets.env");

        std::fs::write(&env_path, "SHARED_KEY=from_env\nENV_ONLY=env_value\n").unwrap();
        std::fs::write(
            &secrets_path,
            "SHARED_KEY=from_secrets\nSECRET_ONLY=secret_value\n",
        )
        .unwrap();

        let map = load_runtime_env_files(&env_path).unwrap();
        assert_eq!(map.get("SHARED_KEY").unwrap(), "from_secrets");
        assert_eq!(map.get("ENV_ONLY").unwrap(), "env_value");
        assert_eq!(map.get("SECRET_ONLY").unwrap(), "secret_value");
    }

    #[test]
    fn load_runtime_env_files_supports_secrets_without_dotenv() {
        let dir = tempfile::tempdir().unwrap();
        let env_path = dir.path().join(".env");
        let secrets_path = dir.path().join("secrets.env");

        std::fs::write(&secrets_path, "ONLY_IN_SECRETS=secret_value\n").unwrap();

        let map = load_runtime_env_files(&env_path).unwrap();
        assert_eq!(map.get("ONLY_IN_SECRETS").unwrap(), "secret_value");
    }

    #[test]
    fn resolver_env_var() {
        std::env::set_var("TEST_CRED_RESOLVE_123", "from_env");
        let resolver = CredentialResolver::new(None, None);
        let val = resolver.resolve("TEST_CRED_RESOLVE_123").unwrap();
        assert_eq!(val.as_str(), "from_env");
        assert!(resolver.has_credential("TEST_CRED_RESOLVE_123"));
        std::env::remove_var("TEST_CRED_RESOLVE_123");
    }

    #[test]
    fn resolver_dotenv_overrides_env() {
        let dir = tempfile::tempdir().unwrap();
        let env_path = dir.path().join(".env");
        std::fs::write(&env_path, "TEST_CRED_DOT_456=from_dotenv\n").unwrap();

        std::env::set_var("TEST_CRED_DOT_456", "from_env");

        let resolver = CredentialResolver::new(None, Some(&env_path));
        let val = resolver.resolve("TEST_CRED_DOT_456").unwrap();
        assert_eq!(val.as_str(), "from_dotenv"); // dotenv takes priority

        std::env::remove_var("TEST_CRED_DOT_456");
    }

    #[test]
    fn resolver_missing_credentials() {
        let resolver = CredentialResolver::new(None, None);
        let missing = resolver.missing_credentials(&["DEFINITELY_NOT_SET_XYZ_789"]);
        assert_eq!(missing, vec!["DEFINITELY_NOT_SET_XYZ_789"]);
    }

    #[test]
    fn resolver_resolve_all() {
        std::env::set_var("TEST_MULTI_A", "a_val");
        std::env::set_var("TEST_MULTI_B", "b_val");

        let resolver = CredentialResolver::new(None, None);
        let resolved = resolver.resolve_all(&["TEST_MULTI_A", "TEST_MULTI_B", "TEST_MULTI_C"]);
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved["TEST_MULTI_A"].as_str(), "a_val");
        assert_eq!(resolved["TEST_MULTI_B"].as_str(), "b_val");

        std::env::remove_var("TEST_MULTI_A");
        std::env::remove_var("TEST_MULTI_B");
    }
}
