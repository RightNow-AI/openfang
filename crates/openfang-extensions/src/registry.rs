//! Integration Registry — manages bundled + installed integration templates.
//!
//! Loads 25 bundled MCP server templates at compile time, merges with user's
//! installed state from `~/.openfang/integrations.toml`, and converts installed
//! integrations to `McpServerConfigEntry` for kernel consumption.

use crate::{
    ExtensionError, ExtensionResult, InstalledIntegration, IntegrationCategory, IntegrationInfo,
    IntegrationStatus, IntegrationTemplate, IntegrationsFile,
};
use openfang_types::config::{McpServerConfigEntry, McpTransportEntry};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// The integration registry — holds all known templates and install state.
pub struct IntegrationRegistry {
    /// All known templates (bundled + custom).
    templates: HashMap<String, IntegrationTemplate>,
    /// Current installed state.
    installed: HashMap<String, InstalledIntegration>,
    /// Path to integrations.toml.
    integrations_path: PathBuf,
}

impl IntegrationRegistry {
    /// Create a new registry with no templates.
    pub fn new(home_dir: &Path) -> Self {
        Self {
            templates: HashMap::new(),
            installed: HashMap::new(),
            integrations_path: home_dir.join("integrations.toml"),
        }
    }

    /// Load bundled templates (compile-time embedded). Returns count loaded.
    pub fn load_bundled(&mut self) -> usize {
        let bundled = crate::bundled::bundled_integrations();
        let count = bundled.len();
        for (id, toml_content) in bundled {
            match toml::from_str::<IntegrationTemplate>(toml_content) {
                Ok(template) => {
                    self.templates.insert(id.to_string(), template);
                }
                Err(e) => {
                    warn!("Failed to parse bundled integration '{}': {}", id, e);
                }
            }
        }
        debug!("Loaded {count} bundled integration template(s)");
        count
    }

    /// Load installed state from integrations.toml.
    pub fn load_installed(&mut self) -> ExtensionResult<usize> {
        if !self.integrations_path.exists() {
            return Ok(0);
        }
        let content = std::fs::read_to_string(&self.integrations_path)?;
        let file: IntegrationsFile =
            toml::from_str(&content).map_err(|e| ExtensionError::TomlParse(e.to_string()))?;
        let count = file.installed.len();
        for entry in file.installed {
            self.installed.insert(entry.id.clone(), entry);
        }
        info!("Loaded {count} installed integration(s)");
        Ok(count)
    }

    /// Save installed state to integrations.toml.
    pub fn save_installed(&self) -> ExtensionResult<()> {
        let file = IntegrationsFile {
            installed: self.installed.values().cloned().collect(),
        };
        let content =
            toml::to_string_pretty(&file).map_err(|e| ExtensionError::TomlParse(e.to_string()))?;
        if let Some(parent) = self.integrations_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.integrations_path, content)?;
        Ok(())
    }

    /// Get a template by ID.
    pub fn get_template(&self, id: &str) -> Option<&IntegrationTemplate> {
        self.templates.get(id)
    }

    /// Get an installed record by ID.
    pub fn get_installed(&self, id: &str) -> Option<&InstalledIntegration> {
        self.installed.get(id)
    }

    /// Check if an integration is installed.
    pub fn is_installed(&self, id: &str) -> bool {
        self.installed.contains_key(id)
    }

    /// Mark an integration as installed.
    pub fn install(&mut self, entry: InstalledIntegration) -> ExtensionResult<()> {
        if self.installed.contains_key(&entry.id) {
            return Err(ExtensionError::AlreadyInstalled(entry.id.clone()));
        }
        self.installed.insert(entry.id.clone(), entry);
        self.save_installed()
    }

    /// Remove an installed integration.
    pub fn uninstall(&mut self, id: &str) -> ExtensionResult<()> {
        if self.installed.remove(id).is_none() {
            return Err(ExtensionError::NotInstalled(id.to_string()));
        }
        self.save_installed()
    }

    /// Enable/disable an installed integration.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> ExtensionResult<()> {
        let entry = self
            .installed
            .get_mut(id)
            .ok_or_else(|| ExtensionError::NotInstalled(id.to_string()))?;
        entry.enabled = enabled;
        self.save_installed()
    }

    /// List all templates.
    pub fn list_templates(&self) -> Vec<&IntegrationTemplate> {
        let mut templates: Vec<_> = self.templates.values().collect();
        templates.sort_by(|a, b| a.id.cmp(&b.id));
        templates
    }

    /// List templates by category.
    pub fn list_by_category(&self, category: &IntegrationCategory) -> Vec<&IntegrationTemplate> {
        self.templates
            .values()
            .filter(|t| &t.category == category)
            .collect()
    }

    /// Search templates by query (matches id, name, description, tags).
    pub fn search(&self, query: &str) -> Vec<&IntegrationTemplate> {
        let q = query.to_lowercase();
        self.templates
            .values()
            .filter(|t| {
                t.id.to_lowercase().contains(&q)
                    || t.name.to_lowercase().contains(&q)
                    || t.description.to_lowercase().contains(&q)
                    || t.tags.iter().any(|tag| tag.to_lowercase().contains(&q))
            })
            .collect()
    }

    /// Get combined info for all integrations (template + install state).
    pub fn list_all_info(&self) -> Vec<IntegrationInfo> {
        self.templates
            .values()
            .map(|t| {
                let installed = self.installed.get(&t.id);
                let status = match installed {
                    Some(inst) if !inst.enabled => IntegrationStatus::Disabled,
                    Some(_) => IntegrationStatus::Ready,
                    None => IntegrationStatus::Available,
                };
                IntegrationInfo {
                    template: t.clone(),
                    status,
                    installed: installed.cloned(),
                    tool_count: 0,
                }
            })
            .collect()
    }

    /// Convert all enabled installed integrations to MCP server config entries.
    /// These can be merged into the kernel's MCP server list.
    pub fn to_mcp_configs(&self) -> Vec<McpServerConfigEntry> {
        self.installed
            .values()
            .filter(|inst| inst.enabled)
            .filter_map(|inst| {
                let template = self.templates.get(&inst.id)?;
                let transport = match &template.transport {
                    crate::McpTransportTemplate::Stdio { command, args } => {
                        McpTransportEntry::Stdio {
                            command: command.clone(),
                            args: args.clone(),
                        }
                    }
                    crate::McpTransportTemplate::Sse { url } => {
                        McpTransportEntry::Sse { url: url.clone() }
                    }
                    crate::McpTransportTemplate::Http { url } => {
                        McpTransportEntry::Http { url: url.clone() }
                    }
                };
                let env: Vec<String> = template
                    .required_env
                    .iter()
                    .map(|e| e.name.clone())
                    .collect();
                Some(McpServerConfigEntry {
                    name: inst.id.clone(),
                    transport,
                    timeout_secs: 30,
                    env,
                    headers: Vec::new(),
                })
            })
            .collect()
    }

    /// Get the path to integrations.toml.
    pub fn integrations_path(&self) -> &Path {
        &self.integrations_path
    }

    /// Total template count.
    pub fn template_count(&self) -> usize {
        self.templates.len()
    }

    /// Total installed count.
    pub fn installed_count(&self) -> usize {
        self.installed.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_load_bundled() {
        let dir = tempfile::tempdir().unwrap();
        let mut reg = IntegrationRegistry::new(dir.path());
        let count = reg.load_bundled();
        assert_eq!(count, 25);
        assert_eq!(reg.template_count(), 25);
    }

    #[test]
    fn registry_get_template() {
        let dir = tempfile::tempdir().unwrap();
        let mut reg = IntegrationRegistry::new(dir.path());
        reg.load_bundled();
        let gh = reg.get_template("github").unwrap();
        assert_eq!(gh.name, "GitHub");
        assert_eq!(gh.category, IntegrationCategory::DevTools);
    }

    #[test]
    fn registry_search() {
        let dir = tempfile::tempdir().unwrap();
        let mut reg = IntegrationRegistry::new(dir.path());
        reg.load_bundled();
        let results = reg.search("search");
        assert!(results.len() >= 2); // brave-search, exa-search
    }

    #[test]
    fn registry_install_uninstall() {
        let dir = tempfile::tempdir().unwrap();
        let mut reg = IntegrationRegistry::new(dir.path());
        reg.load_bundled();

        let entry = InstalledIntegration {
            id: "github".to_string(),
            installed_at: chrono::Utc::now(),
            enabled: true,
            oauth_provider: None,
            config: HashMap::new(),
        };
        reg.install(entry).unwrap();
        assert!(reg.is_installed("github"));
        assert_eq!(reg.installed_count(), 1);

        // Double install should fail
        let entry2 = InstalledIntegration {
            id: "github".to_string(),
            installed_at: chrono::Utc::now(),
            enabled: true,
            oauth_provider: None,
            config: HashMap::new(),
        };
        assert!(reg.install(entry2).is_err());

        reg.uninstall("github").unwrap();
        assert!(!reg.is_installed("github"));
    }

    #[test]
    fn registry_to_mcp_configs() {
        let dir = tempfile::tempdir().unwrap();
        let mut reg = IntegrationRegistry::new(dir.path());
        reg.load_bundled();

        let entry = InstalledIntegration {
            id: "github".to_string(),
            installed_at: chrono::Utc::now(),
            enabled: true,
            oauth_provider: None,
            config: HashMap::new(),
        };
        reg.install(entry).unwrap();

        let configs = reg.to_mcp_configs();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].name, "github");
    }

    #[test]
    fn registry_save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut reg = IntegrationRegistry::new(dir.path());
        reg.load_bundled();

        let entry = InstalledIntegration {
            id: "notion".to_string(),
            installed_at: chrono::Utc::now(),
            enabled: true,
            oauth_provider: None,
            config: HashMap::new(),
        };
        reg.install(entry).unwrap();

        // Load from same path
        let mut reg2 = IntegrationRegistry::new(dir.path());
        reg2.load_bundled();
        let count = reg2.load_installed().unwrap();
        assert_eq!(count, 1);
        assert!(reg2.is_installed("notion"));
    }

    #[test]
    fn registry_list_by_category() {
        let dir = tempfile::tempdir().unwrap();
        let mut reg = IntegrationRegistry::new(dir.path());
        reg.load_bundled();
        let devtools = reg.list_by_category(&IntegrationCategory::DevTools);
        assert_eq!(devtools.len(), 6);
    }

    #[test]
    fn registry_set_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let mut reg = IntegrationRegistry::new(dir.path());
        reg.load_bundled();

        let entry = InstalledIntegration {
            id: "github".to_string(),
            installed_at: chrono::Utc::now(),
            enabled: true,
            oauth_provider: None,
            config: HashMap::new(),
        };
        reg.install(entry).unwrap();

        reg.set_enabled("github", false).unwrap();
        let configs = reg.to_mcp_configs();
        assert!(configs.is_empty()); // disabled = not in MCP configs
    }
}
