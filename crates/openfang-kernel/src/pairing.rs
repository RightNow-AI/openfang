//! Device pairing — QR-code flow for mobile/desktop clients.
//!
//! Supports pairing via short-lived tokens, device management, and
//! push notifications via ntfy.sh or gotify.

use dashmap::DashMap;
use openfang_types::config::PairingConfig;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// Maximum concurrent pairing requests (prevent token flooding).
const MAX_PENDING_REQUESTS: usize = 5;

/// A paired device record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedDevice {
    pub device_id: String,
    pub display_name: String,
    pub platform: String,
    pub paired_at: chrono::DateTime<chrono::Utc>,
    pub last_seen: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing)]
    pub push_token: Option<String>,
}

/// Pairing request (short-lived, for QR code flow).
#[derive(Debug, Clone)]
pub struct PairingRequest {
    pub token: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Persistence callback — kernel injects this so PairingManager can save without
/// taking a direct dependency on openfang-memory.
pub type PersistFn = Box<dyn Fn(&PairedDevice, PersistOp) + Send + Sync>;

/// Persistence operation kind.
#[derive(Debug, Clone, Copy)]
pub enum PersistOp {
    Save,
    Remove,
}

/// Device pairing manager.
pub struct PairingManager {
    config: PairingConfig,
    pending: DashMap<String, PairingRequest>,
    devices: DashMap<String, PairedDevice>,
    persist: Option<PersistFn>,
    client: reqwest::Client,
}

impl PairingManager {
    pub fn new(config: PairingConfig, client: reqwest::Client) -> Self {
        Self {
            config,
            pending: DashMap::new(),
            devices: DashMap::new(),
            persist: None,
            client,
        }
    }

    /// Attach a persistence callback (called after pair/unpair operations).
    pub fn set_persist(&mut self, f: PersistFn) {
        self.persist = Some(f);
    }

    /// Bulk-load devices from persistence (call once at boot).
    pub fn load_devices(&self, devices: Vec<PairedDevice>) {
        for d in devices {
            self.devices.insert(d.device_id.clone(), d);
        }
        debug!(
            count = self.devices.len(),
            "Loaded paired devices from database"
        );
    }

    /// Generate a new pairing request. Returns token for QR encoding.
    pub fn create_pairing_request(&self) -> Result<PairingRequest, String> {
        if !self.config.enabled {
            return Err("Device pairing is disabled".into());
        }

        // Enforce max pending limit
        if self.pending.len() >= MAX_PENDING_REQUESTS {
            // Clean expired first
            self.clean_expired();
            if self.pending.len() >= MAX_PENDING_REQUESTS {
                return Err("Too many pending pairing requests. Try again later.".into());
            }
        }

        // Generate secure random token (32 bytes = 64 hex chars)
        let mut token_bytes = [0u8; 32];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut token_bytes);
        let token = hex::encode(token_bytes);

        let now = chrono::Utc::now();
        let expires_at = now + chrono::Duration::seconds(self.config.token_expiry_secs as i64);

        let request = PairingRequest {
            token: token.clone(),
            created_at: now,
            expires_at,
        };

        self.pending.insert(token, request.clone());

        Ok(request)
    }

    /// Complete pairing — device submits token + device info.
    pub fn complete_pairing(
        &self,
        token: &str,
        device_info: PairedDevice,
    ) -> Result<PairedDevice, String> {
        // SECURITY: Constant-time token comparison
        let found = self.pending.iter().find(|entry| {
            use subtle::ConstantTimeEq;
            let stored = entry.value().token.as_bytes();
            let provided = token.as_bytes();
            if stored.len() != provided.len() {
                return false;
            }
            stored.ct_eq(provided).into()
        });

        let entry = found.ok_or("Invalid or expired pairing token")?;
        let request = entry.value().clone();
        let key = entry.key().clone();
        drop(entry);

        // Check expiry
        if chrono::Utc::now() > request.expires_at {
            self.pending.remove(&key);
            return Err("Pairing token has expired".into());
        }

        // Check max devices
        if self.devices.len() >= self.config.max_devices {
            return Err(format!(
                "Maximum paired devices ({}) reached. Remove a device first.",
                self.config.max_devices
            ));
        }

        // Remove the used token
        self.pending.remove(&key);

        // Store the device
        let device_id = device_info.device_id.clone();
        self.devices.insert(device_id.clone(), device_info.clone());

        // Persist to database
        if let Some(ref persist) = self.persist {
            persist(&device_info, PersistOp::Save);
        }

        debug!(device_id = %device_id, "Device paired successfully");

        Ok(device_info)
    }

    /// List paired devices.
    pub fn list_devices(&self) -> Vec<PairedDevice> {
        self.devices.iter().map(|e| e.value().clone()).collect()
    }

    /// Remove a paired device.
    pub fn remove_device(&self, device_id: &str) -> Result<(), String> {
        let removed = self
            .devices
            .remove(device_id)
            .ok_or_else(|| format!("Device '{device_id}' not found"))?;

        // Persist removal to database
        if let Some(ref persist) = self.persist {
            persist(&removed.1, PersistOp::Remove);
        }

        Ok(())
    }

    /// Send push notification to all paired devices.
    pub async fn notify_devices(
        &self,
        title: &str,
        body: &str,
    ) -> Vec<(String, Result<(), String>)> {
        let mut results = Vec::new();

        match self.config.push_provider.as_str() {
            "ntfy" => {
                let url = self.config.ntfy_url.as_deref().unwrap_or("https://ntfy.sh");
                let topic = match &self.config.ntfy_topic {
                    Some(t) => t.clone(),
                    None => {
                        results.push(("ntfy".to_string(), Err("ntfy_topic not configured".into())));
                        return results;
                    }
                };

                let full_url = format!("{}/{}", url.trim_end_matches('/'), topic);

                match self.client
                    .post(&full_url)
                    .header("Title", title)
                    .body(body.to_string())
                    .timeout(std::time::Duration::from_secs(10))
                    .send()
                    .await
                {
                    Ok(resp) if resp.status().is_success() => {
                        for device in self.devices.iter() {
                            results.push((device.device_id.clone(), Ok(())));
                        }
                    }
                    Ok(resp) => {
                        let status = resp.status();
                        results.push((
                            "ntfy".to_string(),
                            Err(format!("ntfy returned HTTP {status}")),
                        ));
                    }
                    Err(e) => {
                        results
                            .push(("ntfy".to_string(), Err(format!("ntfy request failed: {e}"))));
                    }
                }
            }
            "gotify" => {
                // Gotify requires an app token
                let app_token = match std::env::var("GOTIFY_APP_TOKEN") {
                    Ok(t) => t,
                    Err(_) => {
                        results
                            .push(("gotify".to_string(), Err("GOTIFY_APP_TOKEN not set".into())));
                        return results;
                    }
                };

                let server_url = match std::env::var("GOTIFY_SERVER_URL") {
                    Ok(u) => u,
                    Err(_) => {
                        results.push((
                            "gotify".to_string(),
                            Err("GOTIFY_SERVER_URL not set".into()),
                        ));
                        return results;
                    }
                };

                let url = format!("{}/message", server_url.trim_end_matches('/'));
                let body_json = serde_json::json!({
                    "title": title,
                    "message": body,
                    "priority": 5,
                });

                match self.client
                    .post(&url)
                    .header("X-Gotify-Key", &app_token)
                    .json(&body_json)
                    .timeout(std::time::Duration::from_secs(10))
                    .send()
                    .await
                {
                    Ok(resp) if resp.status().is_success() => {
                        for device in self.devices.iter() {
                            results.push((device.device_id.clone(), Ok(())));
                        }
                    }
                    Ok(resp) => {
                        let status = resp.status();
                        results.push((
                            "gotify".to_string(),
                            Err(format!("gotify returned HTTP {status}")),
                        ));
                    }
                    Err(e) => {
                        results.push((
                            "gotify".to_string(),
                            Err(format!("gotify request failed: {e}")),
                        ));
                    }
                }
            }
            "none" | "" => {
                // No push provider configured — silent
            }
            other => {
                warn!(provider = other, "Unknown push notification provider");
            }
        }

        results
    }

    /// Clean expired pairing requests.
    pub fn clean_expired(&self) {
        let now = chrono::Utc::now();
        self.pending.retain(|_, req| req.expires_at > now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> PairingConfig {
        PairingConfig::default()
    }

    fn enabled_config() -> PairingConfig {
        PairingConfig {
            enabled: true,
            ..Default::default()
        }
    }

    #[test]
    fn test_manager_creation() {
        let mgr = PairingManager::new(default_config(), reqwest::Client::new());
        assert!(mgr.devices.is_empty());
        assert!(mgr.pending.is_empty());
    }

    #[test]
    fn test_create_request_disabled() {
        let mgr = PairingManager::new(default_config(), reqwest::Client::new());
        let result = mgr.create_pairing_request();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("disabled"));
    }

    #[test]
    fn test_create_request_success() {
        let mgr = PairingManager::new(enabled_config(), reqwest::Client::new());
        let req = mgr.create_pairing_request().unwrap();
        assert_eq!(req.token.len(), 64); // 32 bytes = 64 hex chars
        assert!(req.expires_at > req.created_at);
    }

    #[test]
    fn test_max_pending_requests() {
        let mgr = PairingManager::new(enabled_config(), reqwest::Client::new());
        for _ in 0..MAX_PENDING_REQUESTS {
            mgr.create_pairing_request().unwrap();
        }
        let result = mgr.create_pairing_request();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Too many"));
    }

    #[test]
    fn test_complete_pairing_invalid_token() {
        let mgr = PairingManager::new(enabled_config(), reqwest::Client::new());
        let device = PairedDevice {
            device_id: "dev-1".to_string(),
            display_name: "My Phone".to_string(),
            platform: "android".to_string(),
            paired_at: chrono::Utc::now(),
            last_seen: chrono::Utc::now(),
            push_token: None,
        };
        let result = mgr.complete_pairing("invalid-token", device);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid"));
    }

    #[test]
    fn test_complete_pairing_success() {
        let mgr = PairingManager::new(enabled_config(), reqwest::Client::new());
        let req = mgr.create_pairing_request().unwrap();

        let device = PairedDevice {
            device_id: "dev-1".to_string(),
            display_name: "My Phone".to_string(),
            platform: "android".to_string(),
            paired_at: chrono::Utc::now(),
            last_seen: chrono::Utc::now(),
            push_token: None,
        };

        let result = mgr.complete_pairing(&req.token, device);
        assert!(result.is_ok());
        assert_eq!(mgr.devices.len(), 1);
        assert!(mgr.pending.is_empty()); // Token consumed
    }

    #[test]
    fn test_max_devices_enforced() {
        let config = PairingConfig {
            enabled: true,
            max_devices: 1,
            ..Default::default()
        };
        let mgr = PairingManager::new(config, reqwest::Client::new());

        // Pair first device
        let req1 = mgr.create_pairing_request().unwrap();
        let d1 = PairedDevice {
            device_id: "dev-1".to_string(),
            display_name: "Phone 1".to_string(),
            platform: "ios".to_string(),
            paired_at: chrono::Utc::now(),
            last_seen: chrono::Utc::now(),
            push_token: None,
        };
        mgr.complete_pairing(&req1.token, d1).unwrap();

        // Try second device
        let req2 = mgr.create_pairing_request().unwrap();
        let d2 = PairedDevice {
            device_id: "dev-2".to_string(),
            display_name: "Phone 2".to_string(),
            platform: "android".to_string(),
            paired_at: chrono::Utc::now(),
            last_seen: chrono::Utc::now(),
            push_token: None,
        };
        let result = mgr.complete_pairing(&req2.token, d2);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Maximum"));
    }

    #[test]
    fn test_list_devices() {
        let mgr = PairingManager::new(enabled_config(), reqwest::Client::new());
        let req = mgr.create_pairing_request().unwrap();
        let device = PairedDevice {
            device_id: "dev-1".to_string(),
            display_name: "My Phone".to_string(),
            platform: "android".to_string(),
            paired_at: chrono::Utc::now(),
            last_seen: chrono::Utc::now(),
            push_token: None,
        };
        mgr.complete_pairing(&req.token, device).unwrap();

        let devices = mgr.list_devices();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].display_name, "My Phone");
    }

    #[test]
    fn test_remove_device() {
        let mgr = PairingManager::new(enabled_config(), reqwest::Client::new());
        let req = mgr.create_pairing_request().unwrap();
        let device = PairedDevice {
            device_id: "dev-1".to_string(),
            display_name: "My Phone".to_string(),
            platform: "android".to_string(),
            paired_at: chrono::Utc::now(),
            last_seen: chrono::Utc::now(),
            push_token: None,
        };
        mgr.complete_pairing(&req.token, device).unwrap();

        assert!(mgr.remove_device("dev-1").is_ok());
        assert!(mgr.devices.is_empty());
    }

    #[test]
    fn test_remove_nonexistent_device() {
        let mgr = PairingManager::new(enabled_config(), reqwest::Client::new());
        assert!(mgr.remove_device("nonexistent").is_err());
    }

    #[test]
    fn test_clean_expired() {
        let config = PairingConfig {
            enabled: true,
            token_expiry_secs: 0, // Expire immediately
            ..Default::default()
        };
        let mgr = PairingManager::new(config, reqwest::Client::new());
        mgr.create_pairing_request().unwrap();
        assert_eq!(mgr.pending.len(), 1);

        // Wait a tiny bit for expiry
        std::thread::sleep(std::time::Duration::from_millis(10));
        mgr.clean_expired();
        assert!(mgr.pending.is_empty());
    }

    #[test]
    fn test_token_length() {
        let mgr = PairingManager::new(enabled_config(), reqwest::Client::new());
        let req = mgr.create_pairing_request().unwrap();
        // 32 random bytes = 64 hex chars
        assert_eq!(req.token.len(), 64);
    }

    #[test]
    fn test_config_defaults() {
        let config = PairingConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.max_devices, 10);
        assert_eq!(config.token_expiry_secs, 300);
        assert_eq!(config.push_provider, "none");
        assert!(config.ntfy_url.is_none());
        assert!(config.ntfy_topic.is_none());
    }
}
