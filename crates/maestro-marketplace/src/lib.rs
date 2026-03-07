//! # maestro-marketplace
//!
//! Agent/Skill Marketplace inspired by Kore.ai's Agent Marketplace.
//!
//! ## What Kore.ai Has
//!
//! - Pre-built agents for 40+ enterprise use cases
//! - One-click deployment of marketplace agents
//! - Versioned agent packages with dependency management
//! - Categories: IT, HR, Finance, Sales, Customer Service, etc.
//!
//! ## What OpenFang Has
//!
//! - 30+ pre-built agent templates in `agents/` directory
//! - TOML-based agent configuration
//! - BUT: No packaging, no versioning, no discovery, no remote registry
//!
//! ## What This Crate Provides
//!
//! A local-first marketplace that packages OpenFang agent templates
//! with metadata, versioning, and dependency tracking. Can optionally
//! sync with a remote registry.
//!
//! ## HONEST GAPS
//!
//! - No remote registry implementation (local-only for now)
//! - No dependency resolution (skills can't declare dependencies on other skills)
//! - No sandboxed installation (skills have full agent permissions)
//! - No review/approval workflow for community contributions
//! - No usage analytics or ratings

use semver::Version;
use serde::{Deserialize, Serialize};

/// A skill package manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub name: String,
    pub version: Version,
    pub description: String,
    pub author: String,
    pub license: String,
    pub category: SkillCategory,
    /// Required OpenFang capabilities (tools, models, etc.)
    pub requirements: Vec<String>,
    /// Files included in the package.
    pub files: Vec<String>,
    /// SHA256 hash of the package contents.
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillCategory {
    Coding,
    Research,
    DataAnalysis,
    Writing,
    DevOps,
    Security,
    Testing,
    Custom(String),
}

/// Trait for marketplace backends.
#[async_trait::async_trait]
pub trait MarketplaceBackend: Send + Sync {
    /// List available skills, optionally filtered by category.
    async fn list(&self, category: Option<&SkillCategory>) -> anyhow::Result<Vec<SkillManifest>>;

    /// Search for skills by query string.
    async fn search(&self, query: &str) -> anyhow::Result<Vec<SkillManifest>>;

    /// Install a skill by name and version.
    async fn install(&self, name: &str, version: &Version) -> anyhow::Result<()>;

    /// Publish a skill package.
    async fn publish(&self, manifest: &SkillManifest, package_path: &str) -> anyhow::Result<()>;
}
