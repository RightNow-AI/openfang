use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A user account in the FangHub registry.
/// Users authenticate via GitHub OAuth and receive an API token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAccount {
    pub id: Uuid,
    pub github_login: String,
    pub github_id: u64,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub api_token_hash: String, // SHA-256 hash of the API token
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A Hand package in the FangHub registry.
/// Represents the top-level package entry — versions are stored separately.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandPackage {
    pub id: Uuid,
    /// Unique slug identifier (e.g. "my-weather-hand"). URL-safe, lowercase.
    pub package_id: String,
    /// Human-readable display name (e.g. "My Weather Hand").
    pub name: String,
    /// Short description of what the Hand does.
    pub description: String,
    /// Package category (e.g. "Productivity", "Communication", "Information").
    pub category: String,
    /// GitHub login of the package owner.
    pub owner: String,
    /// Latest published version string (e.g. "1.2.3").
    pub latest_version: Option<String>,
    /// Total number of installs across all versions.
    pub install_count: u64,
    /// Tags for discovery (e.g. ["weather", "api", "forecast"]).
    pub tags: Vec<String>,
    /// URL to the package's repository or homepage.
    pub repository_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A specific published version of a Hand package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageVersion {
    pub id: Uuid,
    /// The package this version belongs to.
    pub package_id: String,
    /// Semver version string (e.g. "1.2.3").
    pub version: String,
    /// The full HAND.toml manifest content.
    pub manifest: String,
    /// SHA-256 checksum of the package archive.
    pub checksum: String,
    /// Ed25519 signature of the checksum, signed by the publisher.
    pub signature: Option<String>,
    /// Download URL for the package archive (.tar.gz).
    pub download_url: String,
    /// Size of the package archive in bytes.
    pub archive_size_bytes: u64,
    /// Release notes for this version.
    pub release_notes: Option<String>,
    /// Number of times this specific version has been installed.
    pub install_count: u64,
    pub published_at: DateTime<Utc>,
    pub published_by: String,
}

/// Request body for publishing a new package version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishRequest {
    /// The full HAND.toml manifest content.
    pub manifest: String,
    /// Optional Ed25519 signature of the manifest checksum (hex-encoded).
    pub signature: Option<String>,
    /// Optional release notes for this version.
    pub release_notes: Option<String>,
}

/// Sort order for package search results.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    #[default]
    Installs,
    Updated,
    Name,
}

impl std::fmt::Display for SortOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SortOrder::Installs => write!(f, "installs"),
            SortOrder::Updated => write!(f, "updated"),
            SortOrder::Name => write!(f, "name"),
        }
    }
}

/// Query parameters for searching packages.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, Hash)]
pub struct SearchQuery {
    /// Free-text search query.
    pub q: Option<String>,
    /// Filter by category.
    pub category: Option<String>,
    /// Filter by tag.
    pub tag: Option<String>,
    /// Sort order (default: Installs).
    pub sort: Option<SortOrder>,
    /// Page number (1-indexed, default: 1).
    pub page: Option<u32>,
    /// Results per page (default: 20, max: 100).
    pub per_page: Option<u32>,
}

/// A single result entry in a search response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub package_id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub owner: String,
    pub latest_version: Option<String>,
    pub install_count: u64,
    pub tags: Vec<String>,
    pub updated_at: DateTime<Utc>,
}

/// Response body for a search request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub total: u64,
    pub page: u32,
    pub per_page: u32,
    pub total_pages: u32,
}

/// Response body for the publish endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishResponse {
    pub package_id: String,
    pub version: String,
    pub download_url: String,
    pub checksum: String,
    pub published_at: DateTime<Utc>,
}

/// Response body for the registry stats endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryStats {
    pub total_packages: u64,
    pub total_versions: u64,
    pub total_installs: u64,
    pub total_publishers: u64,
}

impl From<&HandPackage> for SearchResult {
    fn from(pkg: &HandPackage) -> Self {
        SearchResult {
            package_id: pkg.package_id.clone(),
            name: pkg.name.clone(),
            description: pkg.description.clone(),
            category: pkg.category.clone(),
            owner: pkg.owner.clone(),
            latest_version: pkg.latest_version.clone(),
            install_count: pkg.install_count,
            tags: pkg.tags.clone(),
            updated_at: pkg.updated_at,
        }
    }
}
