//! Integration installer — one-click add/remove flow.
//!
//! Handles the complete flow: template lookup → credential resolution →
//! OAuth if needed → write to integrations.toml → hot-reload daemon.

use crate::credentials::CredentialResolver;
use crate::registry::IntegrationRegistry;
use crate::{ExtensionError, ExtensionResult, InstalledIntegration, IntegrationStatus};
use chrono::Utc;
use std::collections::HashMap;
use tracing::{info, warn};
use zeroize::Zeroizing;

/// Result of an installation attempt.
#[derive(Debug)]
pub struct InstallResult {
    /// Integration ID.
    pub id: String,
    /// Final status.
    pub status: IntegrationStatus,
    /// Number of MCP tools that will be available.
    pub tool_count: usize,
    /// Message to display to the user.
    pub message: String,
}

/// Install an integration.
///
/// Steps:
/// 1. Look up template in registry.
/// 2. Check credentials (vault → .env → env → prompt).
/// 3. If `--key` provided, store in vault.
/// 4. If OAuth required, run PKCE flow.
/// 5. Write to integrations.toml.
/// 6. Return install result.
pub fn install_integration(
    registry: &mut IntegrationRegistry,
    resolver: &mut CredentialResolver,
    id: &str,
    provided_keys: &HashMap<String, String>,
) -> ExtensionResult<InstallResult> {
    // 1. Look up template
    let template = registry
        .get_template(id)
        .ok_or_else(|| ExtensionError::NotFound(id.to_string()))?
        .clone();

    // Check not already installed
    if registry.is_installed(id) {
        return Err(ExtensionError::AlreadyInstalled(id.to_string()));
    }

    // 2. Store provided keys in vault
    for (key, value) in provided_keys {
        if let Err(e) = resolver.store_in_vault(key, Zeroizing::new(value.clone())) {
            warn!("Could not store {} in vault: {}", key, e);
            // Fall through — the key is still in the provided_keys map
        }
    }

    // 3. Check all required credentials
    let required_keys: Vec<&str> = template
        .required_env
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    let missing = resolver.missing_credentials(&required_keys);

    // For provided keys, check them too
    let actually_missing: Vec<String> = missing
        .into_iter()
        .filter(|k| !provided_keys.contains_key(k))
        .collect();

    let status = if actually_missing.is_empty() {
        IntegrationStatus::Ready
    } else {
        IntegrationStatus::Setup
    };

    // 4. Determine OAuth provider
    let oauth_provider = template.oauth.as_ref().map(|o| o.provider.clone());

    // 5. Write install record
    let entry = InstalledIntegration {
        id: id.to_string(),
        installed_at: Utc::now(),
        enabled: true,
        oauth_provider,
        config: HashMap::new(),
    };
    registry.install(entry)?;

    // 6. Build result message
    let message = match &status {
        IntegrationStatus::Ready => {
            format!(
                "{} added. MCP tools will be available as mcp_{}_*.",
                template.name, id
            )
        }
        IntegrationStatus::Setup => {
            let missing_labels: Vec<String> = actually_missing
                .iter()
                .filter_map(|key| {
                    template
                        .required_env
                        .iter()
                        .find(|e| e.name == *key)
                        .map(|e| format!("{} ({})", e.label, e.name))
                })
                .collect();
            format!(
                "{} installed but needs credentials: {}",
                template.name,
                missing_labels.join(", ")
            )
        }
        _ => format!("{} installed.", template.name),
    };

    info!("{}", message);

    Ok(InstallResult {
        id: id.to_string(),
        status,
        tool_count: 0,
        message,
    })
}

/// Remove an installed integration.
pub fn remove_integration(registry: &mut IntegrationRegistry, id: &str) -> ExtensionResult<String> {
    let template = registry.get_template(id);
    let name = template
        .map(|t| t.name.clone())
        .unwrap_or_else(|| id.to_string());

    registry.uninstall(id)?;
    let msg = format!("{name} removed.");
    info!("{msg}");
    Ok(msg)
}

/// List all integrations with their status.
pub fn list_integrations(
    registry: &IntegrationRegistry,
    resolver: &CredentialResolver,
) -> Vec<IntegrationListEntry> {
    let mut entries = Vec::new();
    for template in registry.list_templates() {
        let installed = registry.get_installed(&template.id);
        let status = match installed {
            Some(inst) if !inst.enabled => IntegrationStatus::Disabled,
            Some(_inst) => {
                let required_keys: Vec<&str> = template
                    .required_env
                    .iter()
                    .map(|e| e.name.as_str())
                    .collect();
                let missing = resolver.missing_credentials(&required_keys);
                if missing.is_empty() {
                    IntegrationStatus::Ready
                } else {
                    IntegrationStatus::Setup
                }
            }
            None => IntegrationStatus::Available,
        };

        entries.push(IntegrationListEntry {
            id: template.id.clone(),
            name: template.name.clone(),
            icon: template.icon.clone(),
            category: template.category.to_string(),
            status,
            description: template.description.clone(),
        });
    }
    entries
}

/// Flat list entry for display.
#[derive(Debug, Clone)]
pub struct IntegrationListEntry {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub category: String,
    pub status: IntegrationStatus,
    pub description: String,
}

/// Search available integrations.
pub fn search_integrations(
    registry: &IntegrationRegistry,
    query: &str,
) -> Vec<IntegrationListEntry> {
    registry
        .search(query)
        .into_iter()
        .map(|t| {
            let installed = registry.get_installed(&t.id);
            let status = match installed {
                Some(inst) if !inst.enabled => IntegrationStatus::Disabled,
                Some(_) => IntegrationStatus::Ready,
                None => IntegrationStatus::Available,
            };
            IntegrationListEntry {
                id: t.id.clone(),
                name: t.name.clone(),
                icon: t.icon.clone(),
                category: t.category.to_string(),
                status,
                description: t.description.clone(),
            }
        })
        .collect()
}

/// Generate scaffold files for a new custom integration.
pub fn scaffold_integration(dir: &std::path::Path) -> ExtensionResult<String> {
    let template = r#"# Custom Integration Template
# Place this in ~/.openfang/integrations/ or use `openfang add --custom <path>`

id = "my-integration"
name = "My Integration"
description = "A custom MCP server integration"
category = "devtools"
icon = "🔧"
tags = ["custom"]

[transport]
type = "stdio"
command = "npx"
args = ["my-mcp-server"]

[[required_env]]
name = "MY_API_KEY"
label = "API Key"
help = "Get your API key from https://example.com/api-keys"
is_secret = true

[health_check]
interval_secs = 60
unhealthy_threshold = 3

setup_instructions = """
1. Install the MCP server: npm install -g my-mcp-server
2. Get your API key from https://example.com/api-keys
3. Run: openfang add my-integration --key=<your-key>
"""
"#;
    let path = dir.join("integration.toml");
    std::fs::create_dir_all(dir)?;
    std::fs::write(&path, template)?;
    Ok(format!(
        "Integration template created at {}",
        path.display()
    ))
}

/// Generate scaffold files for a new skill.
pub fn scaffold_skill(dir: &std::path::Path) -> ExtensionResult<String> {
    let skill_toml = r#"name = "my-skill"
description = "A custom skill"
version = "0.1.0"
runtime = "prompt_only"
"#;
    let skill_md = r#"---
name: my-skill
description: A custom skill
version: 0.1.0
runtime: prompt_only
---

# My Skill

You are an expert at [domain]. When the user asks about [topic], provide [behavior].

## Guidelines

- Be concise and accurate
- Cite sources when possible
"#;
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("skill.toml"), skill_toml)?;
    std::fs::write(dir.join("SKILL.md"), skill_md)?;
    Ok(format!("Skill scaffold created at {}", dir.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::IntegrationRegistry;

    #[test]
    fn install_and_remove() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = IntegrationRegistry::new(dir.path());
        registry.load_bundled();

        let mut resolver = CredentialResolver::new(None, None);

        // Install github (will be Setup status since no token)
        let result =
            install_integration(&mut registry, &mut resolver, "github", &HashMap::new()).unwrap();
        assert_eq!(result.id, "github");
        // Status depends on whether GITHUB_PERSONAL_ACCESS_TOKEN is in env
        assert!(
            result.status == IntegrationStatus::Ready || result.status == IntegrationStatus::Setup
        );

        // Remove
        let msg = remove_integration(&mut registry, "github").unwrap();
        assert!(msg.contains("GitHub"));
        assert!(!registry.is_installed("github"));
    }

    #[test]
    fn install_with_key() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = IntegrationRegistry::new(dir.path());
        registry.load_bundled();

        let mut resolver = CredentialResolver::new(None, None);

        // Provide key directly
        let mut keys = HashMap::new();
        keys.insert("NOTION_TOKEN".to_string(), "ntn_test_key_123".to_string());

        let result = install_integration(&mut registry, &mut resolver, "notion", &keys).unwrap();
        assert_eq!(result.id, "notion");
    }

    #[test]
    fn install_already_installed() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = IntegrationRegistry::new(dir.path());
        registry.load_bundled();

        let mut resolver = CredentialResolver::new(None, None);

        install_integration(&mut registry, &mut resolver, "github", &HashMap::new()).unwrap();
        let err = install_integration(&mut registry, &mut resolver, "github", &HashMap::new())
            .unwrap_err();
        assert!(err.to_string().contains("already"));
    }

    #[test]
    fn remove_not_installed() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = IntegrationRegistry::new(dir.path());
        registry.load_bundled();
        let err = remove_integration(&mut registry, "github").unwrap_err();
        assert!(err.to_string().contains("not installed"));
    }

    #[test]
    fn list_integrations_all() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = IntegrationRegistry::new(dir.path());
        registry.load_bundled();
        let resolver = CredentialResolver::new(None, None);

        let list = list_integrations(&registry, &resolver);
        assert_eq!(list.len(), 25);
        assert!(list
            .iter()
            .all(|e| e.status == IntegrationStatus::Available));
    }

    #[test]
    fn search_integrations_query() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = IntegrationRegistry::new(dir.path());
        registry.load_bundled();

        let results = search_integrations(&registry, "git");
        assert!(results.iter().any(|e| e.id == "github"));
        assert!(results.iter().any(|e| e.id == "gitlab"));
    }

    #[test]
    fn scaffold_integration_creates_files() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("my-integration");
        let msg = scaffold_integration(&sub).unwrap();
        assert!(sub.join("integration.toml").exists());
        assert!(msg.contains("integration.toml"));
    }

    #[test]
    fn scaffold_skill_creates_files() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("my-skill");
        let msg = scaffold_skill(&sub).unwrap();
        assert!(sub.join("skill.toml").exists());
        assert!(sub.join("SKILL.md").exists());
        assert!(msg.contains("my-skill"));
    }
}
