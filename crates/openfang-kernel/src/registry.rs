//! Agent registry — tracks all agents, their state, and indexes.

use dashmap::DashMap;
use openfang_types::agent::{AgentEntry, AgentId, AgentMode, AgentState};
use openfang_types::error::{OpenFangError, OpenFangResult};

/// Registry of all agents in the kernel.
pub struct AgentRegistry {
    /// Primary index: agent ID → entry.
    agents: DashMap<AgentId, AgentEntry>,
    /// Name index: human-readable name → agent ID.
    name_index: DashMap<String, AgentId>,
    /// Tag index: tag → list of agent IDs.
    tag_index: DashMap<String, Vec<AgentId>>,
}

impl AgentRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            agents: DashMap::new(),
            name_index: DashMap::new(),
            tag_index: DashMap::new(),
        }
    }

    /// Register a new agent.
    pub fn register(&self, entry: AgentEntry) -> OpenFangResult<()> {
        let key = entry.name.to_lowercase();
        if self.name_index.contains_key(&key) {
            return Err(OpenFangError::AgentAlreadyExists(entry.name.clone()));
        }
        let id = entry.id;
        self.name_index.insert(key, id);
        for tag in &entry.tags {
            self.tag_index.entry(tag.clone()).or_default().push(id);
        }
        self.agents.insert(id, entry);
        Ok(())
    }

    /// Get an agent entry by ID.
    pub fn get(&self, id: AgentId) -> Option<AgentEntry> {
        self.agents.get(&id).map(|e| e.value().clone())
    }

    /// Find an agent by name (case-insensitive).
    pub fn find_by_name(&self, name: &str) -> Option<AgentEntry> {
        self.name_index
            .get(&name.to_lowercase())
            .and_then(|id| self.agents.get(id.value()).map(|e| e.value().clone()))
    }

    /// Update agent state.
    pub fn set_state(&self, id: AgentId, state: AgentState) -> OpenFangResult<()> {
        let mut entry = self
            .agents
            .get_mut(&id)
            .ok_or_else(|| OpenFangError::AgentNotFound(id.to_string()))?;
        entry.state = state;
        entry.last_active = chrono::Utc::now();
        Ok(())
    }

    /// Update agent operational mode.
    pub fn set_mode(&self, id: AgentId, mode: AgentMode) -> OpenFangResult<()> {
        let mut entry = self
            .agents
            .get_mut(&id)
            .ok_or_else(|| OpenFangError::AgentNotFound(id.to_string()))?;
        entry.mode = mode;
        entry.last_active = chrono::Utc::now();
        Ok(())
    }

    /// Remove an agent from the registry.
    pub fn remove(&self, id: AgentId) -> OpenFangResult<AgentEntry> {
        let (_, entry) = self
            .agents
            .remove(&id)
            .ok_or_else(|| OpenFangError::AgentNotFound(id.to_string()))?;
        self.name_index.remove(&entry.name.to_lowercase());
        for tag in &entry.tags {
            if let Some(mut ids) = self.tag_index.get_mut(tag) {
                ids.retain(|&agent_id| agent_id != id);
            }
        }
        Ok(entry)
    }

    /// List all agents.
    pub fn list(&self) -> Vec<AgentEntry> {
        self.agents.iter().map(|e| e.value().clone()).collect()
    }

    /// Add a child agent ID to a parent's children list.
    pub fn add_child(&self, parent_id: AgentId, child_id: AgentId) {
        if let Some(mut entry) = self.agents.get_mut(&parent_id) {
            entry.children.push(child_id);
        }
    }

    /// Count of registered agents.
    pub fn count(&self) -> usize {
        self.agents.len()
    }

    /// Update an agent's session ID (for session reset).
    pub fn update_session_id(
        &self,
        id: AgentId,
        new_session_id: openfang_types::agent::SessionId,
    ) -> OpenFangResult<()> {
        let mut entry = self
            .agents
            .get_mut(&id)
            .ok_or_else(|| OpenFangError::AgentNotFound(id.to_string()))?;
        entry.session_id = new_session_id;
        entry.last_active = chrono::Utc::now();
        Ok(())
    }

    /// Update an agent's workspace path.
    pub fn update_workspace(
        &self,
        id: AgentId,
        workspace: Option<std::path::PathBuf>,
    ) -> OpenFangResult<()> {
        let mut entry = self
            .agents
            .get_mut(&id)
            .ok_or_else(|| OpenFangError::AgentNotFound(id.to_string()))?;
        entry.manifest.workspace = workspace;
        entry.last_active = chrono::Utc::now();
        Ok(())
    }

    /// Update an agent's visual identity (emoji, avatar, color).
    pub fn update_identity(
        &self,
        id: AgentId,
        identity: openfang_types::agent::AgentIdentity,
    ) -> OpenFangResult<()> {
        let mut entry = self
            .agents
            .get_mut(&id)
            .ok_or_else(|| OpenFangError::AgentNotFound(id.to_string()))?;
        entry.identity = identity;
        entry.last_active = chrono::Utc::now();
        Ok(())
    }

    /// Update an agent's model configuration.
    pub fn update_model(&self, id: AgentId, new_model: String) -> OpenFangResult<()> {
        let mut entry = self
            .agents
            .get_mut(&id)
            .ok_or_else(|| OpenFangError::AgentNotFound(id.to_string()))?;
        entry.manifest.model.model = new_model;
        entry.last_active = chrono::Utc::now();
        Ok(())
    }

    /// Update an agent's model AND provider together.
    pub fn update_model_and_provider(
        &self,
        id: AgentId,
        new_model: String,
        new_provider: String,
    ) -> OpenFangResult<()> {
        let mut entry = self
            .agents
            .get_mut(&id)
            .ok_or_else(|| OpenFangError::AgentNotFound(id.to_string()))?;
        entry.manifest.model.model = new_model;
        entry.manifest.model.provider = new_provider;
        entry.last_active = chrono::Utc::now();
        Ok(())
    }

    /// Update an agent's skill allowlist.
    pub fn update_skills(&self, id: AgentId, skills: Vec<String>) -> OpenFangResult<()> {
        let mut entry = self
            .agents
            .get_mut(&id)
            .ok_or_else(|| OpenFangError::AgentNotFound(id.to_string()))?;
        entry.manifest.skills = skills;
        entry.last_active = chrono::Utc::now();
        Ok(())
    }

    /// Update an agent's MCP server allowlist.
    pub fn update_mcp_servers(&self, id: AgentId, servers: Vec<String>) -> OpenFangResult<()> {
        let mut entry = self
            .agents
            .get_mut(&id)
            .ok_or_else(|| OpenFangError::AgentNotFound(id.to_string()))?;
        entry.manifest.mcp_servers = servers;
        entry.last_active = chrono::Utc::now();
        Ok(())
    }

    /// Update an agent's system prompt (hot-swap, takes effect on next message).
    pub fn update_system_prompt(&self, id: AgentId, new_prompt: String) -> OpenFangResult<()> {
        let mut entry = self
            .agents
            .get_mut(&id)
            .ok_or_else(|| OpenFangError::AgentNotFound(id.to_string()))?;
        entry.manifest.model.system_prompt = new_prompt;
        entry.last_active = chrono::Utc::now();
        Ok(())
    }

    /// Update an agent's name (also updates the name index).
    pub fn update_name(&self, id: AgentId, new_name: String) -> OpenFangResult<()> {
        let new_key = new_name.to_lowercase();
        if self.name_index.contains_key(&new_key) {
            return Err(OpenFangError::AgentAlreadyExists(new_name));
        }
        let mut entry = self
            .agents
            .get_mut(&id)
            .ok_or_else(|| OpenFangError::AgentNotFound(id.to_string()))?;
        let old_key = entry.name.to_lowercase();
        entry.name = new_name.clone();
        entry.manifest.name = new_name;
        entry.last_active = chrono::Utc::now();
        // Update name index
        drop(entry);
        self.name_index.remove(&old_key);
        self.name_index.insert(new_key, id);
        Ok(())
    }

    /// Update an agent's description.
    pub fn update_description(&self, id: AgentId, new_desc: String) -> OpenFangResult<()> {
        let mut entry = self
            .agents
            .get_mut(&id)
            .ok_or_else(|| OpenFangError::AgentNotFound(id.to_string()))?;
        entry.manifest.description = new_desc;
        entry.last_active = chrono::Utc::now();
        Ok(())
    }

    /// Mark an agent's onboarding as complete.
    pub fn mark_onboarding_complete(&self, id: AgentId) -> OpenFangResult<()> {
        let mut entry = self
            .agents
            .get_mut(&id)
            .ok_or_else(|| OpenFangError::AgentNotFound(id.to_string()))?;
        entry.onboarding_completed = true;
        entry.onboarding_completed_at = Some(chrono::Utc::now());
        entry.last_active = chrono::Utc::now();
        Ok(())
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use openfang_types::agent::*;
    use std::collections::HashMap;

    fn test_entry(name: &str) -> AgentEntry {
        AgentEntry {
            id: AgentId::new(),
            name: name.to_string(),
            manifest: AgentManifest {
                name: name.to_string(),
                version: "0.1.0".to_string(),
                description: "test".to_string(),
                author: "test".to_string(),
                module: "test".to_string(),
                schedule: ScheduleMode::default(),
                model: ModelConfig::default(),
                fallback_models: vec![],
                resources: ResourceQuota::default(),
                priority: Priority::default(),
                capabilities: ManifestCapabilities::default(),
                profile: None,
                tools: HashMap::new(),
                skills: vec![],
                mcp_servers: vec![],
                metadata: HashMap::new(),
                tags: vec![],
                routing: None,
                autonomous: None,
                pinned_model: None,
                workspace: None,
                generate_identity_files: true,
                exec_policy: None,
            },
            state: AgentState::Created,
            mode: AgentMode::default(),
            created_at: Utc::now(),
            last_active: Utc::now(),
            parent: None,
            children: vec![],
            session_id: SessionId::new(),
            tags: vec![],
            identity: Default::default(),
            onboarding_completed: false,
            onboarding_completed_at: None,
        }
    }

    #[test]
    fn test_register_and_get() {
        let registry = AgentRegistry::new();
        let entry = test_entry("test-agent");
        let id = entry.id;
        registry.register(entry).unwrap();
        assert!(registry.get(id).is_some());
    }

    #[test]
    fn test_find_by_name() {
        let registry = AgentRegistry::new();
        let entry = test_entry("my-agent");
        registry.register(entry).unwrap();
        assert!(registry.find_by_name("my-agent").is_some());
    }

    #[test]
    fn test_duplicate_name() {
        let registry = AgentRegistry::new();
        registry.register(test_entry("dup")).unwrap();
        assert!(registry.register(test_entry("dup")).is_err());
    }

    #[test]
    fn test_remove() {
        let registry = AgentRegistry::new();
        let entry = test_entry("removable");
        let id = entry.id;
        registry.register(entry).unwrap();
        registry.remove(id).unwrap();
        assert!(registry.get(id).is_none());
    }

    #[test]
    fn test_find_by_name_case_insensitive() {
        let registry = AgentRegistry::new();
        let entry = test_entry("Data Analyst");
        registry.register(entry).unwrap();

        // Exact match
        assert!(registry.find_by_name("Data Analyst").is_some());
        // All lowercase
        assert!(registry.find_by_name("data analyst").is_some());
        // All uppercase
        assert!(registry.find_by_name("DATA ANALYST").is_some());
        // Mixed case
        assert!(registry.find_by_name("data Analyst").is_some());
    }

    #[test]
    fn test_duplicate_name_case_insensitive() {
        let registry = AgentRegistry::new();
        registry.register(test_entry("Assistant")).unwrap();
        // Same name different case should be rejected
        assert!(registry.register(test_entry("assistant")).is_err());
        assert!(registry.register(test_entry("ASSISTANT")).is_err());
    }

    #[test]
    fn test_remove_case_insensitive() {
        let registry = AgentRegistry::new();
        let entry = test_entry("My Agent");
        let id = entry.id;
        registry.register(entry).unwrap();
        // Should find it before removal
        assert!(registry.find_by_name("my agent").is_some());
        registry.remove(id).unwrap();
        // Gone after removal
        assert!(registry.find_by_name("my agent").is_none());
        assert!(registry.find_by_name("My Agent").is_none());
    }

    #[test]
    fn test_update_name_case_insensitive() {
        let registry = AgentRegistry::new();
        let entry = test_entry("old-name");
        let id = entry.id;
        registry.register(entry).unwrap();

        registry.update_name(id, "New Name".to_string()).unwrap();
        // Old name gone
        assert!(registry.find_by_name("old-name").is_none());
        // New name works case-insensitively
        assert!(registry.find_by_name("New Name").is_some());
        assert!(registry.find_by_name("new name").is_some());
        assert!(registry.find_by_name("NEW NAME").is_some());
    }
}
