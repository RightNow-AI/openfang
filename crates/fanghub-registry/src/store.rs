use crate::{
    error::{RegistryError, RegistryResult},
    models::{HandPackage, PackageVersion, RegistryStats, SearchQuery, SearchResult, SortOrder, UserAccount},
};
use surrealdb::{engine::any::Any, Surreal};

#[cfg(test)]
use uuid::Uuid;

/// The FangHub data store — wraps SurrealDB with typed query methods.
#[derive(Clone)]
pub struct RegistryStore {
    db: Surreal<Any>,
}

/// Helper: deserialize a serde_json::Value row into a typed struct.
fn from_row<T: serde::de::DeserializeOwned>(row: serde_json::Value) -> RegistryResult<T> {
    serde_json::from_value(row).map_err(RegistryError::Serialization)
}

impl RegistryStore {
    pub fn new(db: Surreal<Any>) -> Self {
        Self { db }
    }

    // ─── User operations ────────────────────────────────────────────────────

    /// Create or update a user account (upsert by github_id).
    pub async fn upsert_user(&self, user: &UserAccount) -> RegistryResult<UserAccount> {
        let data = serde_json::to_value(user)?;
        let results: Vec<serde_json::Value> = self
            .db
            .query(
                "UPSERT fh_users SET data = $data WHERE data.github_id = $github_id RETURN AFTER",
            )
            .bind(("data", data))
            .bind(("github_id", user.github_id))
            .await?
            .take(0)
            .map_err(|e| RegistryError::Internal(e.to_string()))?;

        // If UPSERT returned nothing, do a plain CREATE
        if results.is_empty() {
            let data2 = serde_json::to_value(user)?;
            let created: Vec<serde_json::Value> = self
                .db
                .query("CREATE fh_users CONTENT $data RETURN record::id(id) AS id, *")
                .bind(("data", data2))
                .await?
                .take(0)
                .map_err(|e| RegistryError::Internal(e.to_string()))?;
            let row = created
                .into_iter()
                .next()
                .ok_or_else(|| RegistryError::Internal("Create user returned no result".to_string()))?;
            return from_row(row);
        }

        let row = results
            .into_iter()
            .next()
            .ok_or_else(|| RegistryError::Internal("Upsert user returned no result".to_string()))?;
        from_row(row)
    }

    /// Find a user by their API token hash.
    pub async fn find_user_by_token_hash(
        &self,
        token_hash: &str,
    ) -> RegistryResult<Option<UserAccount>> {
        let results: Vec<serde_json::Value> = self
            .db
            .query("SELECT record::id(id) AS id, * FROM fh_users WHERE api_token_hash = $hash LIMIT 1")
            .bind(("hash", token_hash.to_string()))
            .await?
            .take(0)
            .map_err(|e| RegistryError::Internal(e.to_string()))?;
        results
            .into_iter()
            .next()
            .map(from_row)
            .transpose()
    }

    /// Find a user by their GitHub login.
    pub async fn find_user_by_login(&self, login: &str) -> RegistryResult<Option<UserAccount>> {
        let results: Vec<serde_json::Value> = self
            .db
            .query("SELECT record::id(id) AS id, * FROM fh_users WHERE github_login = $login LIMIT 1")
            .bind(("login", login.to_string()))
            .await?
            .take(0)
            .map_err(|e| RegistryError::Internal(e.to_string()))?;
        results
            .into_iter()
            .next()
            .map(from_row)
            .transpose()
    }

    // ─── Package operations ─────────────────────────────────────────────────

    /// Create a new package entry. Returns error if package_id already exists.
    pub async fn create_package(&self, pkg: &HandPackage) -> RegistryResult<HandPackage> {
        // Check for existing package
        if self.get_package(&pkg.package_id).await?.is_some() {
            return Err(RegistryError::PackageAlreadyExists(pkg.package_id.clone()));
        }
        let data = serde_json::to_value(pkg)?;
        // SurrealDB v3 returns record IDs as record types, so we need to cast them
        let results: Vec<serde_json::Value> = self
            .db
            .query("CREATE fh_packages CONTENT $data RETURN record::id(id) AS id, *")
            .bind(("data", data))
            .await?
            .take(0)
            .map_err(|e| RegistryError::Internal(e.to_string()))?;
        let row = results
            .into_iter()
            .next()
            .ok_or_else(|| RegistryError::Internal("Create package returned no result".to_string()))?;
        from_row(row)
    }

    /// Get a package by its slug ID.
    pub async fn get_package(&self, package_id: &str) -> RegistryResult<Option<HandPackage>> {
        // SurrealDB v3 returns record IDs as record types, cast to string
        let results: Vec<serde_json::Value> = self
            .db
            .query("SELECT record::id(id) AS id, * FROM fh_packages WHERE package_id = $id LIMIT 1")
            .bind(("id", package_id.to_string()))
            .await?
            .take(0)
            .map_err(|e| RegistryError::Internal(e.to_string()))?;
        results
            .into_iter()
            .next()
            .map(from_row)
            .transpose()
    }

    /// Update the latest_version and updated_at fields for a package.
    pub async fn update_package_version(
        &self,
        package_id: &str,
        version: &str,
    ) -> RegistryResult<()> {
        self.db
            .query(
                "UPDATE fh_packages SET latest_version = $version, updated_at = time::now() WHERE package_id = $package_id",
            )
            .bind(("version", version.to_string()))
            .bind(("package_id", package_id.to_string()))
            .await?;
        Ok(())
    }

    /// Increment the install count for a package and version.
    pub async fn increment_install_count(
        &self,
        package_id: &str,
        version: &str,
    ) -> RegistryResult<()> {
        self.db
            .query(
                "UPDATE fh_packages SET install_count += 1 WHERE package_id = $package_id;
                 UPDATE fh_versions SET install_count += 1 WHERE package_id = $package_id AND version = $version;",
            )
            .bind(("package_id", package_id.to_string()))
            .bind(("version", version.to_string()))
            .await?;
        Ok(())
    }

    /// List packages owned by a specific user.
    pub async fn list_packages_by_owner(&self, owner: &str) -> RegistryResult<Vec<HandPackage>> {
        let results: Vec<serde_json::Value> = self
            .db
            .query("SELECT record::id(id) AS id, * FROM fh_packages WHERE owner = $owner ORDER BY updated_at DESC")
            .bind(("owner", owner.to_string()))
            .await?
            .take(0)
            .map_err(|e| RegistryError::Internal(e.to_string()))?;
        results.into_iter().map(from_row).collect()
    }

    // ─── Version operations ─────────────────────────────────────────────────

    /// Publish a new version of a package.
    pub async fn publish_version(&self, ver: &PackageVersion) -> RegistryResult<PackageVersion> {
        // Check for duplicate version
        if self
            .get_version(&ver.package_id, &ver.version)
            .await?
            .is_some()
        {
            return Err(RegistryError::VersionAlreadyPublished {
                package: ver.package_id.clone(),
                version: ver.version.clone(),
            });
        }
        let data = serde_json::to_value(ver)?;
        let results: Vec<serde_json::Value> = self
            .db
            .query("CREATE fh_versions CONTENT $data RETURN record::id(id) AS id, *")
            .bind(("data", data))
            .await?
            .take(0)
            .map_err(|e| RegistryError::Internal(e.to_string()))?;
        let row = results
            .into_iter()
            .next()
            .ok_or_else(|| RegistryError::Internal("Publish version returned no result".to_string()))?;
        from_row(row)
    }

    /// Get a specific version of a package.
    pub async fn get_version(
        &self,
        package_id: &str,
        version: &str,
    ) -> RegistryResult<Option<PackageVersion>> {
        let results: Vec<serde_json::Value> = self
            .db
            .query(
                "SELECT record::id(id) AS id, * FROM fh_versions WHERE package_id = $package_id AND version = $version LIMIT 1",
            )
            .bind(("package_id", package_id.to_string()))
            .bind(("version", version.to_string()))
            .await?
            .take(0)
            .map_err(|e| RegistryError::Internal(e.to_string()))?;
        results
            .into_iter()
            .next()
            .map(from_row)
            .transpose()
    }

    /// List all versions of a package, sorted newest first.
    pub async fn list_versions(&self, package_id: &str) -> RegistryResult<Vec<PackageVersion>> {
        let results: Vec<serde_json::Value> = self
            .db
            .query(
                "SELECT record::id(id) AS id, * FROM fh_versions WHERE package_id = $package_id ORDER BY published_at DESC",
            )
            .bind(("package_id", package_id.to_string()))
            .await?
            .take(0)
            .map_err(|e| RegistryError::Internal(e.to_string()))?;
        results.into_iter().map(from_row).collect()
    }

    // ─── Search ─────────────────────────────────────────────────────────────

    /// Search packages with optional text, category, and tag filters.
    pub async fn search(&self, query: &SearchQuery) -> RegistryResult<(Vec<SearchResult>, u64)> {
        let page = query.page.unwrap_or(1).max(1);
        let per_page = query.per_page.unwrap_or(20).clamp(1, 100);
        let offset = (page - 1) * per_page;

        // Build a SurrealQL query with optional filters
        // We use a simple approach: fetch all and filter in Rust for correctness
        // In production this would use SurrealDB full-text search
        let all_packages: Vec<serde_json::Value> = self
            .db
            .query("SELECT record::id(id) AS id, * FROM fh_packages ORDER BY install_count DESC")
            .await?
            .take(0)
            .map_err(|e| RegistryError::Internal(e.to_string()))?;

        let mut packages: Vec<HandPackage> = all_packages
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();

        // Apply filters
        if let Some(cat) = &query.category {
            packages.retain(|p| p.category.eq_ignore_ascii_case(cat));
        }
        if let Some(tag) = &query.tag {
            packages.retain(|p| p.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)));
        }
        if let Some(q) = &query.q {
            let q_lower = q.to_lowercase();
            packages.retain(|p| {
                p.name.to_lowercase().contains(&q_lower)
                    || p.description.to_lowercase().contains(&q_lower)
                    || p.package_id.to_lowercase().contains(&q_lower)
                    || p.tags.iter().any(|t| t.to_lowercase().contains(&q_lower))
            });
        }

        // Sort
        match &query.sort {
            Some(SortOrder::Updated) => packages.sort_by(|a, b| b.updated_at.cmp(&a.updated_at)),
            Some(SortOrder::Name) => packages.sort_by(|a, b| a.name.cmp(&b.name)),
            _ => packages.sort_by(|a, b| b.install_count.cmp(&a.install_count)),
        }

        let total = packages.len() as u64;
        let results: Vec<SearchResult> = packages
            .iter()
            .skip(offset as usize)
            .take(per_page as usize)
            .map(SearchResult::from)
            .collect();

        Ok((results, total))
    }

    // ─── Convenience aliases used by Leptos server functions ─────────────────

    /// Alias for `search` — used by Leptos server functions.
    pub async fn search_packages(&self, query: &SearchQuery) -> RegistryResult<(Vec<SearchResult>, u64)> {
        self.search(query).await
    }

    /// Alias for `list_versions` — used by Leptos server functions.
    pub async fn get_versions(&self, package_id: &str) -> RegistryResult<Vec<PackageVersion>> {
        self.list_versions(package_id).await
    }

    /// Alias for `list_packages_by_owner` — used by Leptos server functions.
    pub async fn get_packages_by_owner(&self, owner: &str) -> RegistryResult<Vec<HandPackage>> {
        self.list_packages_by_owner(owner).await
    }

    // ─── Stats ───────────────────────────────────────────────────────────────

    /// Get aggregate registry statistics.
    pub async fn get_stats(&self) -> RegistryResult<RegistryStats> {
        let packages: Vec<serde_json::Value> = self
            .db
            .query("SELECT * FROM fh_packages")
            .await?
            .take(0)
            .map_err(|e| RegistryError::Internal(e.to_string()))?;

        let total_packages = packages.len() as u64;
        let total_installs: u64 = packages
            .iter()
            .filter_map(|p| p.get("install_count").and_then(|v| v.as_u64()))
            .sum();

        let mut owners = std::collections::HashSet::new();
        for p in &packages {
            if let Some(owner) = p.get("owner").and_then(|v| v.as_str()) {
                owners.insert(owner.to_string());
            }
        }

        let versions: Vec<serde_json::Value> = self
            .db
            .query("SELECT * FROM fh_versions")
            .await?
            .take(0)
            .map_err(|e| RegistryError::Internal(e.to_string()))?;
        let total_versions = versions.len() as u64;

        Ok(RegistryStats {
            total_packages,
            total_versions,
            total_installs,
            total_publishers: owners.len() as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_test_db;
    use chrono::Utc;

    async fn make_store() -> RegistryStore {
        let db = init_test_db().await.unwrap();
        RegistryStore::new(db)
    }

    #[tokio::test]
    async fn test_create_and_get_package() {
        let store = make_store().await;
        let pkg = HandPackage {
            id: Uuid::new_v4(),
            package_id: "test-hand".to_string(),
            name: "Test Hand".to_string(),
            description: "A test Hand package".to_string(),
            category: "Testing".to_string(),
            owner: "alice".to_string(),
            latest_version: None,
            install_count: 0,
            tags: vec!["test".to_string()],
            repository_url: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let created = store.create_package(&pkg).await.unwrap();
        assert_eq!(created.package_id, "test-hand");

        let fetched = store.get_package("test-hand").await.unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().name, "Test Hand");
    }

    #[tokio::test]
    async fn test_duplicate_package_fails() {
        let store = make_store().await;
        let pkg = HandPackage {
            id: Uuid::new_v4(),
            package_id: "dup-hand".to_string(),
            name: "Dup Hand".to_string(),
            description: "Duplicate test".to_string(),
            category: "Testing".to_string(),
            owner: "bob".to_string(),
            latest_version: None,
            install_count: 0,
            tags: vec![],
            repository_url: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.create_package(&pkg).await.unwrap();
        let result = store.create_package(&pkg).await;
        assert!(matches!(result, Err(RegistryError::PackageAlreadyExists(_))));
    }

    #[tokio::test]
    async fn test_publish_and_get_version() {
        let store = make_store().await;
        let pkg = HandPackage {
            id: Uuid::new_v4(),
            package_id: "versioned-hand".to_string(),
            name: "Versioned Hand".to_string(),
            description: "A versioned Hand".to_string(),
            category: "Testing".to_string(),
            owner: "carol".to_string(),
            latest_version: None,
            install_count: 0,
            tags: vec![],
            repository_url: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.create_package(&pkg).await.unwrap();

        let ver = PackageVersion {
            id: Uuid::new_v4(),
            package_id: "versioned-hand".to_string(),
            version: "1.0.0".to_string(),
            manifest: "[hand]\nid = \"versioned-hand\"\nversion = \"1.0.0\"".to_string(),
            checksum: "abc123".to_string(),
            signature: None,
            download_url: "https://fanghub.example.com/packages/versioned-hand/1.0.0.tar.gz"
                .to_string(),
            archive_size_bytes: 1024,
            release_notes: Some("Initial release".to_string()),
            install_count: 0,
            published_at: Utc::now(),
            published_by: "carol".to_string(),
        };
        let published = store.publish_version(&ver).await.unwrap();
        assert_eq!(published.version, "1.0.0");

        let fetched = store.get_version("versioned-hand", "1.0.0").await.unwrap();
        assert!(fetched.is_some());
    }

    #[tokio::test]
    async fn test_search_packages() {
        let store = make_store().await;
        let pkg = HandPackage {
            id: Uuid::new_v4(),
            package_id: "search-weather-hand".to_string(),
            name: "Weather Hand".to_string(),
            description: "Fetches weather forecasts".to_string(),
            category: "Information".to_string(),
            owner: "dave".to_string(),
            latest_version: Some("1.0.0".to_string()),
            install_count: 42,
            tags: vec!["weather".to_string(), "api".to_string()],
            repository_url: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.create_package(&pkg).await.unwrap();

        let query = SearchQuery {
            q: Some("weather".to_string()),
            ..Default::default()
        };
        let (results, total) = store.search(&query).await.unwrap();
        assert!(total >= 1);
        assert!(results
            .iter()
            .any(|r| r.package_id == "search-weather-hand"));
    }

    #[tokio::test]
    async fn test_get_stats_empty() {
        let store = make_store().await;
        let stats = store.get_stats().await.unwrap();
        assert_eq!(stats.total_packages, 0);
        assert_eq!(stats.total_versions, 0);
    }
}
