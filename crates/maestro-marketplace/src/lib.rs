//! # maestro-marketplace
//!
//! Local-first agent/skill marketplace for OpenFang.
//!
//! Provides skill packaging, versioning, discovery, and installation
//! for OpenFang agents and skills. Indexes local `agents/` and `skills/`
//! directories and supports optional remote registry sync.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use thiserror::Error;
use tracing::{debug, info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum MarketplaceError {
    #[error("Package not found: {name}@{version}")]
    NotFound { name: String, version: String },
    #[error("Version conflict: {name} already at {installed}, requested {requested}")]
    VersionConflict { name: String, installed: String, requested: String },
    #[error("Dependency not satisfied: {dep}")]
    DependencyError { dep: String },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("Registry error: {0}")]
    Registry(String),
}

pub type MarketplaceResult<T> = Result<T, MarketplaceError>;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub id: Uuid,
    pub name: String,
    pub version: Version,
    pub description: String,
    pub author: String,
    pub license: String,
    pub category: SkillCategory,
    pub tags: Vec<String>,
    pub requirements: Vec<String>,
    pub dependencies: Vec<Dependency>,
    pub files: Vec<String>,
    pub entry_point: Option<String>,
    pub content_hash: String,
    pub published_at: DateTime<Utc>,
    pub downloads: u64,
    pub rating: Option<f32>,
}

impl SkillManifest {
    pub fn new(
        name: impl Into<String>,
        version: Version,
        description: impl Into<String>,
        category: SkillCategory,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            version,
            description: description.into(),
            author: "unknown".to_string(),
            license: "MIT".to_string(),
            category,
            tags: Vec::new(),
            requirements: Vec::new(),
            dependencies: Vec::new(),
            files: Vec::new(),
            entry_point: None,
            content_hash: String::new(),
            published_at: Utc::now(),
            downloads: 0,
            rating: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub version_req: String,
    pub optional: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SkillCategory {
    Coding,
    Research,
    DataAnalysis,
    Writing,
    DevOps,
    Security,
    Testing,
    CustomerService,
    Finance,
    Hr,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPackage {
    pub id: Uuid,
    pub name: String,
    pub version: Version,
    pub description: String,
    pub author: String,
    pub category: SkillCategory,
    pub tags: Vec<String>,
    pub skills: Vec<String>,
    pub config_template: Option<String>,
    pub content_hash: String,
    pub published_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPackage {
    pub manifest: SkillManifest,
    pub install_path: PathBuf,
    pub installed_at: DateTime<Utc>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub manifest: SkillManifest,
    pub relevance_score: f32,
    pub installed: bool,
}

// ---------------------------------------------------------------------------
// MarketplaceBackend trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait MarketplaceBackend: Send + Sync {
    async fn list(&self, category: Option<&SkillCategory>) -> MarketplaceResult<Vec<SkillManifest>>;
    async fn search(&self, query: &str) -> MarketplaceResult<Vec<SearchResult>>;
    async fn get(&self, name: &str, version: &str) -> MarketplaceResult<SkillManifest>;
    async fn install(&self, name: &str, version: &str) -> MarketplaceResult<InstalledPackage>;
    async fn uninstall(&self, name: &str) -> MarketplaceResult<()>;
    async fn list_installed(&self) -> MarketplaceResult<Vec<InstalledPackage>>;
    async fn publish(&self, manifest: &SkillManifest) -> MarketplaceResult<()>;
    async fn update(&self, name: &str) -> MarketplaceResult<InstalledPackage>;
}

// ---------------------------------------------------------------------------
// LocalRegistry — file-system backed registry
// ---------------------------------------------------------------------------

pub struct LocalRegistry {
    registry_dir: PathBuf,
    install_dir: PathBuf,
    packages: Arc<RwLock<HashMap<String, SkillManifest>>>,
    installed: Arc<RwLock<HashMap<String, InstalledPackage>>>,
}

impl LocalRegistry {
    pub fn new(registry_dir: impl Into<PathBuf>, install_dir: impl Into<PathBuf>) -> Self {
        Self {
            registry_dir: registry_dir.into(),
            install_dir: install_dir.into(),
            packages: Arc::new(RwLock::new(HashMap::new())),
            installed: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Load all manifests from the registry directory.
    pub fn load_from_disk(&self) -> MarketplaceResult<usize> {
        let mut packages = self.packages.write().unwrap();
        let mut count = 0;
        if !self.registry_dir.exists() {
            return Ok(0);
        }
        for entry in std::fs::read_dir(&self.registry_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let content = std::fs::read_to_string(&path)?;
                if let Ok(manifest) = serde_json::from_str::<SkillManifest>(&content) {
                    packages.insert(manifest.name.clone(), manifest);
                    count += 1;
                }
            }
        }
        info!("Loaded {} packages from registry at {:?}", count, self.registry_dir);
        Ok(count)
    }

    /// Register a package in memory (useful for testing).
    pub fn register(&self, manifest: SkillManifest) {
        let mut packages = self.packages.write().unwrap();
        packages.insert(manifest.name.clone(), manifest);
    }

    fn score_relevance(manifest: &SkillManifest, query: &str) -> f32 {
        let query_lower = query.to_lowercase();
        let mut score = 0.0f32;
        if manifest.name.to_lowercase().contains(&query_lower) { score += 3.0; }
        if manifest.description.to_lowercase().contains(&query_lower) { score += 2.0; }
        for tag in &manifest.tags {
            if tag.to_lowercase().contains(&query_lower) { score += 1.0; }
        }
        score
    }
}

#[async_trait]
impl MarketplaceBackend for LocalRegistry {
    async fn list(&self, category: Option<&SkillCategory>) -> MarketplaceResult<Vec<SkillManifest>> {
        let packages = self.packages.read().unwrap();
        let mut result: Vec<SkillManifest> = packages.values()
            .filter(|p| {
                if let Some(cat) = category {
                    &p.category == cat
                } else {
                    true
                }
            })
            .cloned()
            .collect();
        result.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(result)
    }

    async fn search(&self, query: &str) -> MarketplaceResult<Vec<SearchResult>> {
        let packages = self.packages.read().unwrap();
        let installed = self.installed.read().unwrap();
        let mut results: Vec<SearchResult> = packages.values()
            .filter_map(|p| {
                let score = Self::score_relevance(p, query);
                if score > 0.0 {
                    Some(SearchResult {
                        manifest: p.clone(),
                        relevance_score: score,
                        installed: installed.contains_key(&p.name),
                    })
                } else {
                    None
                }
            })
            .collect();
        results.sort_by(|a, b| b.relevance_score.partial_cmp(&a.relevance_score).unwrap());
        Ok(results)
    }

    async fn get(&self, name: &str, version: &str) -> MarketplaceResult<SkillManifest> {
        let packages = self.packages.read().unwrap();
        packages.get(name)
            .filter(|p| version == "latest" || p.version.to_string() == version)
            .cloned()
            .ok_or_else(|| MarketplaceError::NotFound { name: name.to_string(), version: version.to_string() })
    }

    async fn install(&self, name: &str, version: &str) -> MarketplaceResult<InstalledPackage> {
        let manifest = self.get(name, version).await?;

        // Check for version conflict
        {
            let installed = self.installed.read().unwrap();
            if let Some(existing) = installed.get(name) {
                if existing.manifest.version != manifest.version {
                    return Err(MarketplaceError::VersionConflict {
                        name: name.to_string(),
                        installed: existing.manifest.version.to_string(),
                        requested: manifest.version.to_string(),
                    });
                }
                // Already installed at same version
                return Ok(existing.clone());
            }
        }

        let install_path = self.install_dir.join(&manifest.name);
        let installed_pkg = InstalledPackage {
            manifest: manifest.clone(),
            install_path,
            installed_at: Utc::now(),
            enabled: true,
        };

        let mut installed = self.installed.write().unwrap();
        installed.insert(name.to_string(), installed_pkg.clone());
        info!("Installed {}@{}", name, manifest.version);
        Ok(installed_pkg)
    }

    async fn uninstall(&self, name: &str) -> MarketplaceResult<()> {
        let mut installed = self.installed.write().unwrap();
        if installed.remove(name).is_none() {
            return Err(MarketplaceError::NotFound { name: name.to_string(), version: "any".to_string() });
        }
        info!("Uninstalled {}", name);
        Ok(())
    }

    async fn list_installed(&self) -> MarketplaceResult<Vec<InstalledPackage>> {
        let installed = self.installed.read().unwrap();
        Ok(installed.values().cloned().collect())
    }

    async fn publish(&self, manifest: &SkillManifest) -> MarketplaceResult<()> {
        let mut packages = self.packages.write().unwrap();
        packages.insert(manifest.name.clone(), manifest.clone());
        info!("Published {}@{}", manifest.name, manifest.version);
        Ok(())
    }

    async fn update(&self, name: &str) -> MarketplaceResult<InstalledPackage> {
        // For local registry, "update" re-installs the latest version
        let manifest = self.get(name, "latest").await?;
        let mut installed = self.installed.write().unwrap();
        let install_path = self.install_dir.join(name);
        let updated = InstalledPackage {
            manifest,
            install_path,
            installed_at: Utc::now(),
            enabled: true,
        };
        installed.insert(name.to_string(), updated.clone());
        Ok(updated)
    }
}

// ---------------------------------------------------------------------------
// PackageManager — high-level interface
// ---------------------------------------------------------------------------

pub struct PackageManager {
    backend: Arc<dyn MarketplaceBackend>,
}

impl PackageManager {
    pub fn new(backend: Arc<dyn MarketplaceBackend>) -> Self {
        Self { backend }
    }

    pub fn local(registry_dir: impl Into<PathBuf>, install_dir: impl Into<PathBuf>) -> Self {
        let registry = Arc::new(LocalRegistry::new(registry_dir, install_dir));
        Self { backend: registry }
    }

    pub async fn list(&self, category: Option<&SkillCategory>) -> MarketplaceResult<Vec<SkillManifest>> {
        self.backend.list(category).await
    }

    pub async fn search(&self, query: &str) -> MarketplaceResult<Vec<SearchResult>> {
        self.backend.search(query).await
    }

    pub async fn install(&self, name: &str, version: &str) -> MarketplaceResult<InstalledPackage> {
        self.backend.install(name, version).await
    }

    pub async fn uninstall(&self, name: &str) -> MarketplaceResult<()> {
        self.backend.uninstall(name).await
    }

    pub async fn list_installed(&self) -> MarketplaceResult<Vec<InstalledPackage>> {
        self.backend.list_installed().await
    }

    pub async fn publish(&self, manifest: SkillManifest) -> MarketplaceResult<()> {
        self.backend.publish(&manifest).await
    }

    pub async fn update_all(&self) -> MarketplaceResult<Vec<InstalledPackage>> {
        let installed = self.list_installed().await?;
        let mut updated = Vec::new();
        for pkg in installed {
            match self.backend.update(&pkg.manifest.name).await {
                Ok(u) => updated.push(u),
                Err(e) => warn!("Failed to update {}: {}", pkg.manifest.name, e),
            }
        }
        Ok(updated)
    }
}

// ---------------------------------------------------------------------------
// Hash utilities
// ---------------------------------------------------------------------------

pub fn hash_content(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    format!("{:x}", hasher.finalize())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn make_manifest(name: &str) -> SkillManifest {
        SkillManifest::new(
            name,
            Version::from_str("1.0.0").unwrap(),
            format!("A {} skill for testing", name),
            SkillCategory::Coding,
        )
    }

    #[tokio::test]
    async fn test_local_registry_list() {
        let registry = LocalRegistry::new("/tmp/reg", "/tmp/install");
        registry.register(make_manifest("skill-a"));
        registry.register(make_manifest("skill-b"));
        let list = registry.list(None).await.unwrap();
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_local_registry_search() {
        let registry = LocalRegistry::new("/tmp/reg", "/tmp/install");
        let mut m = make_manifest("rust-formatter");
        m.description = "Formats Rust code automatically".to_string();
        m.tags = vec!["rust".to_string(), "formatting".to_string()];
        registry.register(m);
        registry.register(make_manifest("python-linter"));

        let results = registry.search("rust").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].manifest.name, "rust-formatter");
    }

    #[tokio::test]
    async fn test_install_and_list_installed() {
        let registry = Arc::new(LocalRegistry::new("/tmp/reg", "/tmp/install"));
        registry.register(make_manifest("my-skill"));
        let pm = PackageManager::new(registry);

        let installed = pm.install("my-skill", "latest").await.unwrap();
        assert_eq!(installed.manifest.name, "my-skill");
        assert!(installed.enabled);

        let all = pm.list_installed().await.unwrap();
        assert_eq!(all.len(), 1);
    }

    #[tokio::test]
    async fn test_uninstall() {
        let registry = Arc::new(LocalRegistry::new("/tmp/reg", "/tmp/install"));
        registry.register(make_manifest("removable"));
        let pm = PackageManager::new(registry);

        pm.install("removable", "latest").await.unwrap();
        pm.uninstall("removable").await.unwrap();
        let all = pm.list_installed().await.unwrap();
        assert_eq!(all.len(), 0);
    }

    #[tokio::test]
    async fn test_not_found() {
        let registry = Arc::new(LocalRegistry::new("/tmp/reg", "/tmp/install"));
        let pm = PackageManager::new(registry);
        let result = pm.install("nonexistent", "1.0.0").await;
        assert!(matches!(result, Err(MarketplaceError::NotFound { .. })));
    }

    #[tokio::test]
    async fn test_category_filter() {
        let registry = LocalRegistry::new("/tmp/reg", "/tmp/install");
        registry.register(make_manifest("coder"));
        let mut writer = make_manifest("writer");
        writer.category = SkillCategory::Writing;
        registry.register(writer);

        let coding = registry.list(Some(&SkillCategory::Coding)).await.unwrap();
        assert_eq!(coding.len(), 1);
        assert_eq!(coding[0].name, "coder");
    }

    #[test]
    fn test_hash_content() {
        let h1 = hash_content(b"hello");
        let h2 = hash_content(b"hello");
        let h3 = hash_content(b"world");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
        assert_eq!(h1.len(), 64); // SHA256 hex
    }

    #[tokio::test]
    async fn test_publish() {
        let registry = Arc::new(LocalRegistry::new("/tmp/reg", "/tmp/install"));
        let pm = PackageManager::new(registry);
        let manifest = make_manifest("new-skill");
        pm.publish(manifest).await.unwrap();
        let list = pm.list(None).await.unwrap();
        assert_eq!(list.len(), 1);
    }
}
