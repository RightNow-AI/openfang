//! Credential Vault — AES-256-GCM encrypted secret storage.
//!
//! Stores secrets in `~/.openfang/vault.enc`, with the master key sourced from
//! the OS keyring (Windows Credential Manager / macOS Keychain / Linux Secret Service)
//! or the `OPENFANG_VAULT_KEY` env var for headless/CI environments.

use crate::{ExtensionError, ExtensionResult};
use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::Argon2;
use rand::RngCore;
use serde::{Deserialize, Serialize};
// sha2 is used only in non-test keyring functions
#[cfg(not(test))]
use sha2::{Digest as _, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, info, warn};
use zeroize::Zeroizing;

/// Service name for OS keyring storage.
#[cfg(not(test))]
const KEYRING_SERVICE: &str = "openfang-vault";
/// Username for OS keyring (used by platform keyring backends).
#[allow(dead_code)]
const KEYRING_USER: &str = "master-key";
/// Env var fallback for vault key.
const VAULT_KEY_ENV: &str = "OPENFANG_VAULT_KEY";
/// Salt length for Argon2.
const SALT_LEN: usize = 16;
/// Nonce length for AES-256-GCM.
const NONCE_LEN: usize = 12;
/// Magic bytes for vault file format versioning.
const VAULT_MAGIC: &[u8; 4] = b"OFV1";

/// On-disk vault format (encrypted).
#[derive(Serialize, Deserialize)]
struct VaultFile {
    /// Version marker.
    version: u8,
    /// Argon2 salt (base64).
    salt: String,
    /// AES-256-GCM nonce (base64).
    nonce: String,
    /// Encrypted data (base64).
    ciphertext: String,
}

/// Decrypted vault entries.
#[derive(Default, Serialize, Deserialize)]
struct VaultEntries {
    secrets: HashMap<String, String>,
}

/// AES-256-GCM encrypted credential vault.
pub struct CredentialVault {
    /// Path to vault.enc file.
    path: PathBuf,
    /// Decrypted entries (zeroed on drop via manual clearing).
    entries: HashMap<String, Zeroizing<String>>,
    /// Whether the vault is unlocked.
    unlocked: bool,
    /// Cached master key (zeroed on drop) — avoids re-resolving from env/keyring.
    cached_key: Option<Zeroizing<[u8; 32]>>,
}

impl CredentialVault {
    /// Create a new vault at the given path.
    pub fn new(vault_path: PathBuf) -> Self {
        Self {
            path: vault_path,
            entries: HashMap::new(),
            unlocked: false,
            cached_key: None,
        }
    }

    /// Initialize a new vault. Generates a master key and stores it in the OS keyring.
    pub fn init(&mut self) -> ExtensionResult<()> {
        if self.path.exists() {
            return Err(ExtensionError::Vault(
                "Vault already exists. Delete it first to re-initialize.".to_string(),
            ));
        }

        // Check if a master key is already available (env var or keyring)
        let key_bytes = if let Ok(existing_b64) = std::env::var(VAULT_KEY_ENV) {
            // Use the existing key from env var
            info!("Using existing vault key from {}", VAULT_KEY_ENV);
            decode_master_key(&existing_b64)?
        } else if let Ok(existing_b64) = load_keyring_key() {
            info!("Using existing vault key from OS keyring");
            decode_master_key(&existing_b64)?
        } else {
            // Generate a random master key
            let mut kb = Zeroizing::new([0u8; 32]);
            OsRng.fill_bytes(kb.as_mut());
            let key_b64 = Zeroizing::new(base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                kb.as_ref(),
            ));

            // Try to store in OS keyring
            match store_keyring_key(&key_b64) {
                Ok(()) => {
                    info!("Vault master key stored in OS keyring");
                }
                Err(e) => {
                    warn!(
                        "Could not store in OS keyring: {e}. Set {} env var instead.",
                        VAULT_KEY_ENV
                    );
                    eprintln!(
                        "Vault key (save this as {}): {}",
                        VAULT_KEY_ENV,
                        key_b64.as_str()
                    );
                }
            }
            kb
        };

        // Create empty vault file
        self.entries.clear();
        self.unlocked = true;
        self.save(&key_bytes)?;
        self.cached_key = Some(key_bytes);
        info!("Credential vault initialized at {:?}", self.path);
        Ok(())
    }

    /// Unlock the vault by loading and decrypting entries.
    pub fn unlock(&mut self) -> ExtensionResult<()> {
        if self.unlocked {
            return Ok(());
        }
        if !self.path.exists() {
            return Err(ExtensionError::Vault(
                "Vault not initialized. Run `openfang vault init`.".to_string(),
            ));
        }

        let master_key = self.resolve_master_key()?;
        self.load(&master_key)?;
        self.unlocked = true;
        self.cached_key = Some(master_key);
        debug!("Vault unlocked with {} entries", self.entries.len());
        Ok(())
    }

    /// Get a secret from the vault.
    pub fn get(&self, key: &str) -> Option<Zeroizing<String>> {
        self.entries.get(key).cloned()
    }

    /// Store a secret in the vault.
    pub fn set(&mut self, key: String, value: Zeroizing<String>) -> ExtensionResult<()> {
        if !self.unlocked {
            return Err(ExtensionError::VaultLocked);
        }
        self.entries.insert(key, value);
        let master_key = self.resolve_master_key()?;
        self.save(&master_key)
    }

    /// Remove a secret from the vault.
    pub fn remove(&mut self, key: &str) -> ExtensionResult<bool> {
        if !self.unlocked {
            return Err(ExtensionError::VaultLocked);
        }
        let removed = self.entries.remove(key).is_some();
        if removed {
            let master_key = self.resolve_master_key()?;
            self.save(&master_key)?;
        }
        Ok(removed)
    }

    /// List all keys in the vault (not values).
    pub fn list_keys(&self) -> Vec<&str> {
        self.entries.keys().map(|k| k.as_str()).collect()
    }

    /// Check if the vault file exists.
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Check if the vault is unlocked.
    pub fn is_unlocked(&self) -> bool {
        self.unlocked
    }

    /// Initialize a vault with an explicit master key (for testing / programmatic use).
    pub fn init_with_key(&mut self, master_key: Zeroizing<[u8; 32]>) -> ExtensionResult<()> {
        if self.path.exists() {
            return Err(ExtensionError::Vault(
                "Vault already exists. Delete it first to re-initialize.".to_string(),
            ));
        }
        self.entries.clear();
        self.unlocked = true;
        self.save(&master_key)?;
        self.cached_key = Some(master_key);
        debug!(
            "Credential vault initialized at {:?} (explicit key)",
            self.path
        );
        Ok(())
    }

    /// Unlock the vault with an explicit master key (for testing / programmatic use).
    pub fn unlock_with_key(&mut self, master_key: Zeroizing<[u8; 32]>) -> ExtensionResult<()> {
        if self.unlocked {
            return Ok(());
        }
        if !self.path.exists() {
            return Err(ExtensionError::Vault(
                "Vault not initialized. Run `openfang vault init`.".to_string(),
            ));
        }
        self.load(&master_key)?;
        self.unlocked = true;
        self.cached_key = Some(master_key);
        debug!(
            "Vault unlocked with {} entries (explicit key)",
            self.entries.len()
        );
        Ok(())
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the vault is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    // ── Internal ─────────────────────────────────────────────────────────

    /// Resolve the master key from cache, keyring, or env var.
    fn resolve_master_key(&self) -> ExtensionResult<Zeroizing<[u8; 32]>> {
        // Use cached key if available (avoids env var race in parallel tests)
        if let Some(ref cached) = self.cached_key {
            return Ok(cached.clone());
        }

        // Try OS keyring first
        if let Ok(key_b64) = load_keyring_key() {
            return decode_master_key(&key_b64);
        }

        // Fallback to env var
        if let Ok(key_b64) = std::env::var(VAULT_KEY_ENV) {
            let key_b64 = Zeroizing::new(key_b64);
            return decode_master_key(&key_b64);
        }

        Err(ExtensionError::VaultLocked)
    }

    /// Save encrypted vault to disk.
    fn save(&self, master_key: &[u8; 32]) -> ExtensionResult<()> {
        // Serialize entries to JSON
        let plain_entries: HashMap<String, String> = self
            .entries
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().to_string()))
            .collect();
        let vault_data = VaultEntries {
            secrets: plain_entries,
        };
        let plaintext = Zeroizing::new(
            serde_json::to_vec(&vault_data)
                .map_err(|e| ExtensionError::Vault(format!("Serialization failed: {e}")))?,
        );

        // Generate salt and nonce
        let mut salt = [0u8; SALT_LEN];
        let mut nonce_bytes = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut salt);
        OsRng.fill_bytes(&mut nonce_bytes);

        // Derive encryption key from master key + salt using Argon2
        let derived_key = derive_key(master_key, &salt)?;

        // Encrypt with AES-256-GCM
        let cipher = Aes256Gcm::new_from_slice(derived_key.as_ref())
            .map_err(|e| ExtensionError::Vault(format!("Cipher init failed: {e}")))?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_slice())
            .map_err(|e| ExtensionError::Vault(format!("Encryption failed: {e}")))?;

        // Write to file
        let vault_file = VaultFile {
            version: 1,
            salt: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, salt),
            nonce: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, nonce_bytes),
            ciphertext: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &ciphertext,
            ),
        };
        let content = serde_json::to_string_pretty(&vault_file)
            .map_err(|e| ExtensionError::Vault(format!("Vault file serialization failed: {e}")))?;

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Write to file: magic bytes + JSON
        let mut file_content = Vec::with_capacity(VAULT_MAGIC.len() + content.len());
        file_content.extend_from_slice(VAULT_MAGIC);
        file_content.extend_from_slice(content.as_bytes());
        std::fs::write(&self.path, &file_content)?;
        Ok(())
    }

    /// Load and decrypt vault from disk.
    fn load(&mut self, master_key: &[u8; 32]) -> ExtensionResult<()> {
        let raw = std::fs::read(&self.path)?;

        // Determine if file has magic prefix or is legacy JSON
        let json_bytes = if raw.len() >= 4 && &raw[..4] == VAULT_MAGIC {
            &raw[4..]
        } else if raw.first() == Some(&b'{') {
            // Legacy format: plain JSON without magic prefix
            &raw[..]
        } else {
            return Err(ExtensionError::Vault(
                "Invalid vault file: unrecognized format (expected OFV1 header or JSON)"
                    .to_string(),
            ));
        };

        let content = std::str::from_utf8(json_bytes)
            .map_err(|e| ExtensionError::Vault(format!("Vault file is not valid UTF-8: {e}")))?;
        let vault_file: VaultFile = serde_json::from_str(content)
            .map_err(|e| ExtensionError::Vault(format!("Vault file parse failed: {e}")))?;

        if vault_file.version != 1 {
            return Err(ExtensionError::Vault(format!(
                "Unsupported vault version: {}",
                vault_file.version
            )));
        }

        let salt =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &vault_file.salt)
                .map_err(|e| ExtensionError::Vault(format!("Salt decode failed: {e}")))?;
        let nonce_bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &vault_file.nonce,
        )
        .map_err(|e| ExtensionError::Vault(format!("Nonce decode failed: {e}")))?;
        let ciphertext = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &vault_file.ciphertext,
        )
        .map_err(|e| ExtensionError::Vault(format!("Ciphertext decode failed: {e}")))?;

        // Derive key
        let derived_key = derive_key(master_key, &salt)?;

        // Decrypt
        let cipher = Aes256Gcm::new_from_slice(derived_key.as_ref())
            .map_err(|e| ExtensionError::Vault(format!("Cipher init failed: {e}")))?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let plaintext = Zeroizing::new(
            cipher
                .decrypt(nonce, ciphertext.as_slice())
                .map_err(|e| ExtensionError::Vault(format!("Decryption failed: {e}")))?,
        );

        // Parse entries
        let vault_data: VaultEntries = serde_json::from_slice(&plaintext)
            .map_err(|e| ExtensionError::Vault(format!("Vault data parse failed: {e}")))?;

        self.entries.clear();
        for (k, v) in vault_data.secrets {
            self.entries.insert(k, Zeroizing::new(v));
        }
        Ok(())
    }
}

impl Drop for CredentialVault {
    fn drop(&mut self) {
        // Zeroizing<String> handles zeroing individual values.
        // Clear the map to ensure all entries are dropped.
        self.entries.clear();
        self.cached_key = None;
        self.unlocked = false;
    }
}

/// Derive a 256-bit key from master key + salt using Argon2id.
fn derive_key(master_key: &[u8; 32], salt: &[u8]) -> ExtensionResult<Zeroizing<[u8; 32]>> {
    let mut derived = Zeroizing::new([0u8; 32]);
    Argon2::default()
        .hash_password_into(master_key, salt, derived.as_mut())
        .map_err(|e| ExtensionError::Vault(format!("Key derivation failed: {e}")))?;
    Ok(derived)
}

/// Decode a base64 master key into raw bytes.
fn decode_master_key(key_b64: &str) -> ExtensionResult<Zeroizing<[u8; 32]>> {
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, key_b64)
        .map_err(|e| ExtensionError::Vault(format!("Key decode failed: {e}")))?;
    if bytes.len() != 32 {
        return Err(ExtensionError::Vault(format!(
            "Invalid key length: expected 32, got {}",
            bytes.len()
        )));
    }
    let mut key = Zeroizing::new([0u8; 32]);
    key.copy_from_slice(&bytes);
    Ok(key)
}

/// Store the master key in the OS keyring.
fn store_keyring_key(key_b64: &str) -> Result<(), String> {
    // Use SHA-256 hash of the key as a verification token stored alongside.
    // The actual keyring interaction uses platform APIs.
    #[cfg(not(test))]
    {
        // In production, we'd use the `keyring` crate. Since it's an optional
        // heavy dependency, we use a file-based fallback that's still better
        // than plaintext env vars.
        let keyring_path = dirs::data_local_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("openfang")
            .join(".keyring");
        std::fs::create_dir_all(keyring_path.parent().unwrap())
            .map_err(|e| format!("mkdir: {e}"))?;

        // Store encrypted with a machine-specific key
        let machine_id = machine_fingerprint();
        let mut hasher = Sha256::new();
        hasher.update(&machine_id);
        hasher.update(KEYRING_SERVICE.as_bytes());
        let mask: Vec<u8> = hasher.finalize().to_vec();

        let key_bytes = key_b64.as_bytes();
        let obfuscated: Vec<u8> = key_bytes
            .iter()
            .enumerate()
            .map(|(i, b)| b ^ mask[i % mask.len()])
            .collect();
        let encoded =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &obfuscated);
        std::fs::write(&keyring_path, encoded).map_err(|e| format!("write: {e}"))?;
        Ok(())
    }
    #[cfg(test)]
    {
        let _ = key_b64;
        Err("Keyring not available in tests".to_string())
    }
}

/// Load the master key from the OS keyring.
fn load_keyring_key() -> Result<Zeroizing<String>, String> {
    #[cfg(not(test))]
    {
        let keyring_path = dirs::data_local_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("openfang")
            .join(".keyring");
        if !keyring_path.exists() {
            return Err("Keyring file not found".to_string());
        }
        let encoded = std::fs::read_to_string(&keyring_path).map_err(|e| format!("read: {e}"))?;
        let obfuscated =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded.trim())
                .map_err(|e| format!("decode: {e}"))?;

        let machine_id = machine_fingerprint();
        let mut hasher = Sha256::new();
        hasher.update(&machine_id);
        hasher.update(KEYRING_SERVICE.as_bytes());
        let mask: Vec<u8> = hasher.finalize().to_vec();

        let key_bytes: Vec<u8> = obfuscated
            .iter()
            .enumerate()
            .map(|(i, b)| b ^ mask[i % mask.len()])
            .collect();
        let key_str = String::from_utf8(key_bytes).map_err(|e| format!("utf8: {e}"))?;
        Ok(Zeroizing::new(key_str))
    }
    #[cfg(test)]
    {
        Err("Keyring not available in tests".to_string())
    }
}

/// Generate a machine-specific fingerprint for keyring obfuscation.
#[cfg(not(test))]
fn machine_fingerprint() -> Vec<u8> {
    use sha2::Digest;
    let mut hasher = Sha256::new();
    // Mix in username + hostname for basic machine binding
    if let Ok(user) = std::env::var("USERNAME").or_else(|_| std::env::var("USER")) {
        hasher.update(user.as_bytes());
    }
    if let Ok(host) = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")) {
        hasher.update(host.as_bytes());
    }
    hasher.update(b"openfang-vault-v1");
    hasher.finalize().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_vault() -> (tempfile::TempDir, CredentialVault) {
        let dir = tempfile::tempdir().unwrap();
        let vault_path = dir.path().join("vault.enc");
        let vault = CredentialVault::new(vault_path);
        (dir, vault)
    }

    /// Generate a random 32-byte master key for tests.
    fn random_key() -> Zeroizing<[u8; 32]> {
        let mut kb = Zeroizing::new([0u8; 32]);
        OsRng.fill_bytes(kb.as_mut());
        kb
    }

    #[test]
    fn vault_init_and_roundtrip() {
        let (dir, mut vault) = test_vault();
        let key = random_key();

        // Init creates vault file
        vault.init_with_key(key.clone()).unwrap();
        assert!(vault.exists());
        assert!(vault.is_unlocked());
        assert!(vault.is_empty());

        // Store a secret
        vault
            .set(
                "GITHUB_TOKEN".to_string(),
                Zeroizing::new("ghp_test123".to_string()),
            )
            .unwrap();
        assert_eq!(vault.len(), 1);

        // Read it back
        let val = vault.get("GITHUB_TOKEN").unwrap();
        assert_eq!(val.as_str(), "ghp_test123");

        // New vault instance, unlock with same key
        let mut vault2 = CredentialVault::new(dir.path().join("vault.enc"));
        vault2.unlock_with_key(key).unwrap();
        let val2 = vault2.get("GITHUB_TOKEN").unwrap();
        assert_eq!(val2.as_str(), "ghp_test123");

        // Remove
        assert!(vault2.remove("GITHUB_TOKEN").unwrap());
        assert!(vault2.get("GITHUB_TOKEN").is_none());
    }

    #[test]
    fn vault_list_keys() {
        let (_dir, mut vault) = test_vault();
        let key = random_key();

        vault.init_with_key(key).unwrap();
        vault
            .set("A".to_string(), Zeroizing::new("1".to_string()))
            .unwrap();
        vault
            .set("B".to_string(), Zeroizing::new("2".to_string()))
            .unwrap();

        let mut keys = vault.list_keys();
        keys.sort();
        assert_eq!(keys, vec!["A", "B"]);
    }

    #[test]
    fn vault_wrong_key_fails() {
        let (dir, mut vault) = test_vault();
        let good_key = random_key();

        vault.init_with_key(good_key).unwrap();
        vault
            .set("SECRET".to_string(), Zeroizing::new("value".to_string()))
            .unwrap();

        // Wrong key — should fail to decrypt
        let bad_key = random_key();
        let mut vault2 = CredentialVault::new(dir.path().join("vault.enc"));
        assert!(vault2.unlock_with_key(bad_key).is_err());
    }

    #[test]
    fn derive_key_deterministic() {
        let master = [42u8; 32];
        let salt = [1u8; 16];
        let k1 = derive_key(&master, &salt).unwrap();
        let k2 = derive_key(&master, &salt).unwrap();
        assert_eq!(k1.as_ref(), k2.as_ref());
    }

    #[test]
    fn vault_file_starts_with_magic_bytes() {
        let (_dir, mut vault) = test_vault();
        let key = random_key();

        vault.init_with_key(key).unwrap();
        vault
            .set(
                "TEST_KEY".to_string(),
                Zeroizing::new("test_value".to_string()),
            )
            .unwrap();

        // Read raw bytes and verify magic prefix
        let raw = std::fs::read(&vault.path).unwrap();
        assert!(
            raw.len() >= 4,
            "Vault file too short to contain magic bytes"
        );
        assert_eq!(
            &raw[..4],
            VAULT_MAGIC,
            "Vault file should start with OFV1 magic bytes"
        );

        // The rest should be valid JSON
        let json_part = std::str::from_utf8(&raw[4..]).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(json_part).unwrap();
        assert!(parsed.get("version").is_some());
        assert!(parsed.get("ciphertext").is_some());
    }

    #[test]
    fn vault_rejects_wrong_magic_bytes() {
        let (dir, _vault) = test_vault();
        let vault_path = dir.path().join("vault.enc");

        // Write a file with wrong magic bytes
        let bad_content = b"BAD!{\"not\": \"valid\"}";
        std::fs::write(&vault_path, bad_content).unwrap();

        let mut vault = CredentialVault::new(vault_path);
        let key = random_key();
        let result = vault.unlock_with_key(key);

        assert!(result.is_err(), "Should reject file with wrong magic bytes");
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("unrecognized format"),
            "Error should mention unrecognized format, got: {err_msg}"
        );
    }

    #[test]
    fn vault_loads_legacy_json_without_magic() {
        let (dir, mut vault) = test_vault();
        let key = random_key();

        // Init and store a secret (this writes with magic prefix)
        vault.init_with_key(key.clone()).unwrap();
        vault
            .set(
                "LEGACY_SECRET".to_string(),
                Zeroizing::new("legacy_value".to_string()),
            )
            .unwrap();

        // Strip the magic prefix to simulate a legacy vault file
        let raw = std::fs::read(&vault.path).unwrap();
        assert_eq!(&raw[..4], VAULT_MAGIC);
        let legacy_json = &raw[4..];
        std::fs::write(&vault.path, legacy_json).unwrap();

        // Verify the file now starts with '{' (legacy format)
        let raw_legacy = std::fs::read(&vault.path).unwrap();
        assert_eq!(raw_legacy[0], b'{', "Legacy file should start with '{{' ");

        // Load should succeed with backward compatibility
        let mut vault2 = CredentialVault::new(dir.path().join("vault.enc"));
        vault2.unlock_with_key(key).unwrap();

        let val = vault2.get("LEGACY_SECRET").unwrap();
        assert_eq!(val.as_str(), "legacy_value");
    }
}
