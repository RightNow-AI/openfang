//! HAND.toml manifest parsing and validation.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandManifest {
    pub hand: HandSection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandSection {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub repository: Option<String>,
    pub author: Option<String>,
}

pub fn load_manifest(dir: &Path) -> Result<HandManifest> {
    let path = dir.join("HAND.toml");
    if !path.exists() {
        bail!("No HAND.toml found in {}", dir.display());
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let manifest: HandManifest = toml::from_str(&content)
        .context("Failed to parse HAND.toml")?;

    // Validate semver
    manifest.hand.version.parse::<semver::Version>()
        .with_context(|| format!("Invalid version '{}' in HAND.toml — must be semver (e.g. 1.0.0)", manifest.hand.version))?;

    Ok(manifest)
}
