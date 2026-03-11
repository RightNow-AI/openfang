//! FangHub client — discover and install Hands from the OpenFang marketplace.
//!
//! FangHub is the official marketplace for OpenFang Hands (autonomous agent
//! configurations).  This client provides search, install, update, and
//! uninstall operations.
//!
//! # Architecture
//!
//! The FangHub API mirrors the GitHub Releases API for the initial
//! implementation.  Each Hand is a GitHub repository under the
//! `openfang-hands` organisation, and releases contain a `hand.zip` asset
//! with the `HAND.toml`, `SKILL.md`, and any supporting files.
//!
//! ```text
//! FangHubClient::search("researcher")
//!     └─► GET https://api.github.com/search/repositories?q=researcher+org:openfang-hands
//!
//! FangHubClient::install("researcher", "1.0.0", &install_dir)
//!     └─► GET https://api.github.com/repos/openfang-hands/researcher/releases/tags/v1.0.0
//!     └─► download hand.zip asset
//!     └─► extract to install_dir/researcher/
//! ```

use crate::SkillError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// FangHub registry configuration.
#[derive(Debug, Clone)]
pub struct FangHubConfig {
    /// GitHub API base URL (overridable for testing).
    pub api_base: String,
    /// GitHub organisation hosting the Hand repositories.
    pub github_org: String,
    /// Optional GitHub personal access token for higher rate limits.
    pub github_token: Option<String>,
}

impl Default for FangHubConfig {
    fn default() -> Self {
        Self {
            api_base: "https://api.github.com".to_string(),
            github_org: "openfang-hands".to_string(),
            github_token: std::env::var("GITHUB_TOKEN").ok(),
        }
    }
}

// ---------------------------------------------------------------------------
// API response types
// ---------------------------------------------------------------------------

/// A Hand listed in FangHub search results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandSearchResult {
    /// Unique hand identifier (GitHub repo name).
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Short description.
    pub description: String,
    /// Latest published version.
    pub latest_version: String,
    /// Number of GitHub stars (popularity proxy).
    pub stars: u64,
    /// Tags / topics.
    pub tags: Vec<String>,
    /// Author / organisation.
    pub author: String,
    /// Repository URL.
    pub repo_url: String,
}

/// A specific release of a Hand in FangHub.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandRelease {
    /// Version string (e.g. `"1.2.0"`).
    pub version: String,
    /// Release notes.
    pub changelog: String,
    /// Download URL for the `hand.zip` asset.
    pub download_url: String,
    /// SHA-256 checksum of the zip (if provided by the publisher).
    pub checksum: Option<String>,
    /// When this release was published.
    pub published_at: String,
}

// ---------------------------------------------------------------------------
// FangHubClient
// ---------------------------------------------------------------------------

/// Client for the FangHub Hand marketplace.
pub struct FangHubClient {
    config: FangHubConfig,
    http: reqwest::Client,
}

impl FangHubClient {
    /// Create a new client with default configuration.
    pub fn new() -> Self {
        Self::with_config(FangHubConfig::default())
    }

    /// Create a new client with custom configuration.
    pub fn with_config(config: FangHubConfig) -> Self {
        let builder = reqwest::Client::builder()
            .user_agent("openfang-fanghub/0.1");

        // Reqwest doesn't expose a direct header-on-builder API in all
        // versions; we set the auth header per-request instead.
        let _ = &config; // suppress unused warning
        let http = builder
            .build()
            .expect("Failed to build FangHub HTTP client");

        Self { config, http }
    }

    /// Add the GitHub auth header if a token is configured.
    fn auth_request(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let req = req.header("Accept", "application/vnd.github.v3+json");
        if let Some(token) = &self.config.github_token {
            req.header("Authorization", format!("Bearer {token}"))
        } else {
            req
        }
    }

    /// Search for Hands by query string.
    ///
    /// Returns up to 20 results sorted by GitHub stars.
    pub async fn search(&self, query: &str) -> Result<Vec<HandSearchResult>, SkillError> {
        let url = format!(
            "{}/search/repositories?q={}+org:{}&sort=stars&per_page=20",
            self.config.api_base, query, self.config.github_org
        );
        debug!("FangHub search: {url}");

        let resp = self
            .auth_request(self.http.get(&url))
            .send()
            .await
            .map_err(|e| SkillError::Network(format!("FangHub search failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(SkillError::Network(format!(
                "FangHub search returned {status}: {body}"
            )));
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| SkillError::Network(format!("FangHub search parse error: {e}")))?;

        let items = json
            .get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let results = items
            .iter()
            .map(|item| HandSearchResult {
                id: item["name"].as_str().unwrap_or("").to_string(),
                name: item["name"]
                    .as_str()
                    .unwrap_or("")
                    .replace('-', " ")
                    .to_string(),
                description: item["description"].as_str().unwrap_or("").to_string(),
                latest_version: "latest".to_string(),
                stars: item["stargazers_count"].as_u64().unwrap_or(0),
                tags: item["topics"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|t| t.as_str())
                            .map(String::from)
                            .collect()
                    })
                    .unwrap_or_default(),
                author: item["owner"]["login"].as_str().unwrap_or("").to_string(),
                repo_url: item["html_url"].as_str().unwrap_or("").to_string(),
            })
            .collect();

        Ok(results)
    }

    /// Fetch the latest release for a Hand.
    pub async fn latest_release(&self, hand_id: &str) -> Result<HandRelease, SkillError> {
        let url = format!(
            "{}/repos/{}/{}/releases/latest",
            self.config.api_base, self.config.github_org, hand_id
        );
        debug!("FangHub latest release: {url}");

        let resp = self
            .auth_request(self.http.get(&url))
            .send()
            .await
            .map_err(|e| SkillError::Network(format!("FangHub release fetch failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(SkillError::Network(format!(
                "FangHub release returned {status} for hand '{hand_id}'"
            )));
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| SkillError::Network(format!("FangHub release parse error: {e}")))?;

        Self::parse_release(&json)
    }

    /// Fetch a specific version of a Hand.
    pub async fn get_release(
        &self,
        hand_id: &str,
        version: &str,
    ) -> Result<HandRelease, SkillError> {
        let tag = if version.starts_with('v') {
            version.to_string()
        } else {
            format!("v{version}")
        };

        let url = format!(
            "{}/repos/{}/{}/releases/tags/{}",
            self.config.api_base, self.config.github_org, hand_id, tag
        );
        debug!("FangHub get release: {url}");

        let resp = self
            .auth_request(self.http.get(&url))
            .send()
            .await
            .map_err(|e| SkillError::Network(format!("FangHub release fetch failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(SkillError::Network(format!(
                "FangHub release {version} not found for hand '{hand_id}': {status}"
            )));
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| SkillError::Network(format!("FangHub release parse error: {e}")))?;

        Self::parse_release(&json)
    }

    /// Download and install a Hand into `install_dir/<hand_id>/`.
    ///
    /// Downloads the `hand.zip` asset from the release, extracts it, and
    /// returns the path to the installed directory.
    pub async fn install(
        &self,
        hand_id: &str,
        version: &str,
        install_dir: &Path,
    ) -> Result<PathBuf, SkillError> {
        let release = if version == "latest" {
            self.latest_release(hand_id).await?
        } else {
            self.get_release(hand_id, version).await?
        };

        info!(
            "Installing hand '{}' v{} from {}",
            hand_id, release.version, release.download_url
        );

        // Download the zip
        let zip_bytes = self
            .auth_request(self.http.get(&release.download_url))
            .send()
            .await
            .map_err(|e| SkillError::Network(format!("Download failed: {e}")))?
            .bytes()
            .await
            .map_err(|e| SkillError::Network(format!("Download read failed: {e}")))?;

        // Extract to install_dir/<hand_id>/
        let hand_dir = install_dir.join(hand_id);
        std::fs::create_dir_all(&hand_dir).map_err(|e| {
            SkillError::ExecutionFailed(format!("Failed to create hand directory: {e}"))
        })?;

        let cursor = std::io::Cursor::new(zip_bytes.as_ref());
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|e| SkillError::InvalidManifest(format!("Invalid hand zip: {e}")))?;

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| SkillError::InvalidManifest(format!("Zip read error: {e}")))?;

            let out_path = hand_dir.join(file.name());

            if file.name().ends_with('/') {
                std::fs::create_dir_all(&out_path).map_err(|e| {
                    SkillError::ExecutionFailed(format!("Failed to create dir: {e}"))
                })?;
            } else {
                if let Some(parent) = out_path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        SkillError::ExecutionFailed(format!("Failed to create parent dir: {e}"))
                    })?;
                }
                let mut out_file = std::fs::File::create(&out_path)
                    .map_err(|e| SkillError::ExecutionFailed(format!("Failed to create file: {e}")))?;
                std::io::copy(&mut file, &mut out_file)
                    .map_err(|e| SkillError::ExecutionFailed(format!("Failed to write file: {e}")))?;
            }
        }

        info!("Hand '{}' installed to {:?}", hand_id, hand_dir);
        Ok(hand_dir)
    }

    /// Update a Hand to its latest version.
    ///
    /// Removes the existing installation and re-installs from the latest release.
    pub async fn update(&self, hand_id: &str, install_dir: &Path) -> Result<PathBuf, SkillError> {
        let hand_dir = install_dir.join(hand_id);
        if hand_dir.exists() {
            std::fs::remove_dir_all(&hand_dir)
                .map_err(|e| SkillError::ExecutionFailed(format!("Failed to remove old hand: {e}")))?;
        }
        self.install(hand_id, "latest", install_dir).await
    }

    /// Uninstall a Hand by removing its directory.
    pub fn uninstall(&self, hand_id: &str, install_dir: &Path) -> Result<(), SkillError> {
        let hand_dir = install_dir.join(hand_id);
        if !hand_dir.exists() {
            return Err(SkillError::NotFound(format!(
                "Hand '{hand_id}' is not installed at {:?}",
                install_dir
            )));
        }
        std::fs::remove_dir_all(&hand_dir)
            .map_err(|e| SkillError::ExecutionFailed(format!("Failed to uninstall hand '{hand_id}': {e}")))?;
        info!("Hand '{hand_id}' uninstalled.");
        Ok(())
    }

    /// List installed Hands in a directory.
    pub fn list_installed(install_dir: &Path) -> Vec<String> {
        if !install_dir.exists() {
            return vec![];
        }
        std::fs::read_dir(install_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().is_dir())
                    .filter(|e| e.path().join("HAND.toml").exists())
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    // -------------------------------------------------------------------------
    // Private helpers
    // -------------------------------------------------------------------------

    fn parse_release(json: &serde_json::Value) -> Result<HandRelease, SkillError> {
        let version = json["tag_name"]
            .as_str()
            .unwrap_or("unknown")
            .trim_start_matches('v')
            .to_string();

        let changelog = json["body"].as_str().unwrap_or("").to_string();
        let published_at = json["published_at"].as_str().unwrap_or("").to_string();

        // Find the hand.zip asset
        let download_url = json["assets"]
            .as_array()
            .and_then(|assets| {
                assets.iter().find(|a| {
                    a["name"].as_str().map(|n| n == "hand.zip").unwrap_or(false)
                })
            })
            .and_then(|a| a["browser_download_url"].as_str())
            .map(String::from)
            .ok_or_else(|| {
                SkillError::InvalidManifest(
                    "Release does not contain a hand.zip asset".to_string(),
                )
            })?;

        Ok(HandRelease {
            version,
            changelog,
            download_url,
            checksum: None,
            published_at,
        })
    }
}

impl Default for FangHubClient {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[allow(dead_code)]
    fn test_config() -> FangHubConfig {
        FangHubConfig {
            api_base: "https://api.github.com".to_string(),
            github_org: "openfang-hands".to_string(),
            github_token: None,
        }
    }

    #[test]
    fn client_creates_with_default_config() {
        let client = FangHubClient::new();
        assert_eq!(client.config.github_org, "openfang-hands");
        assert_eq!(client.config.api_base, "https://api.github.com");
    }

    #[test]
    fn client_creates_with_custom_config() {
        let config = FangHubConfig {
            api_base: "https://my-fanghub.example.com".to_string(),
            github_org: "my-org".to_string(),
            github_token: Some("test-token".to_string()),
        };
        let client = FangHubClient::with_config(config);
        assert_eq!(client.config.api_base, "https://my-fanghub.example.com");
        assert_eq!(client.config.github_org, "my-org");
        assert_eq!(client.config.github_token.as_deref(), Some("test-token"));
    }

    #[test]
    fn list_installed_empty_dir() {
        let dir = TempDir::new().unwrap();
        let installed = FangHubClient::list_installed(dir.path());
        assert!(installed.is_empty());
    }

    #[test]
    fn list_installed_finds_hand_dirs() {
        let dir = TempDir::new().unwrap();

        // Create two fake installed hands (with HAND.toml)
        for name in &["researcher", "clip"] {
            let hand_dir = dir.path().join(name);
            std::fs::create_dir_all(&hand_dir).unwrap();
            std::fs::write(hand_dir.join("HAND.toml"), b"[hand]\nid = \"test\"").unwrap();
        }

        // Create a non-hand directory (no HAND.toml)
        std::fs::create_dir_all(dir.path().join("not-a-hand")).unwrap();

        let mut installed = FangHubClient::list_installed(dir.path());
        installed.sort();
        assert_eq!(installed, vec!["clip", "researcher"]);
    }

    #[test]
    fn list_installed_nonexistent_dir() {
        let installed = FangHubClient::list_installed(Path::new("/nonexistent/path/xyz"));
        assert!(installed.is_empty());
    }

    #[test]
    fn uninstall_removes_directory() {
        let dir = TempDir::new().unwrap();
        let hand_dir = dir.path().join("test-hand");
        std::fs::create_dir_all(&hand_dir).unwrap();
        std::fs::write(hand_dir.join("HAND.toml"), b"[hand]\nid = \"test\"").unwrap();

        let client = FangHubClient::new();
        client.uninstall("test-hand", dir.path()).unwrap();
        assert!(!hand_dir.exists());
    }

    #[test]
    fn uninstall_not_installed_returns_error() {
        let dir = TempDir::new().unwrap();
        let client = FangHubClient::new();
        let result = client.uninstall("nonexistent-hand", dir.path());
        assert!(result.is_err());
        match result {
            Err(SkillError::NotFound(_)) => {}
            other => panic!("Expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn parse_release_missing_asset_returns_error() {
        let json = serde_json::json!({
            "tag_name": "v1.0.0",
            "body": "Initial release",
            "published_at": "2026-01-01T00:00:00Z",
            "assets": []
        });
        let result = FangHubClient::parse_release(&json);
        assert!(result.is_err());
    }

    #[test]
    fn parse_release_with_asset_succeeds() {
        let json = serde_json::json!({
            "tag_name": "v2.1.0",
            "body": "Bug fixes",
            "published_at": "2026-03-01T12:00:00Z",
            "assets": [
                {
                    "name": "hand.zip",
                    "browser_download_url": "https://github.com/openfang-hands/researcher/releases/download/v2.1.0/hand.zip"
                }
            ]
        });
        let release = FangHubClient::parse_release(&json).unwrap();
        assert_eq!(release.version, "2.1.0");
        assert_eq!(release.changelog, "Bug fixes");
        assert!(release.download_url.contains("hand.zip"));
    }

    #[test]
    fn version_tag_normalization() {
        // The tag "v2.1.0" should produce version "2.1.0" (strip the v prefix)
        let json = serde_json::json!({
            "tag_name": "v2.1.0",
            "body": "",
            "published_at": "",
            "assets": [{"name": "hand.zip", "browser_download_url": "https://example.com/hand.zip"}]
        });
        let release = FangHubClient::parse_release(&json).unwrap();
        assert_eq!(release.version, "2.1.0");
        assert!(!release.version.starts_with('v'));
    }
}
