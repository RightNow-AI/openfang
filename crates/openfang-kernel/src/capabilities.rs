//! Capability manager — enforces capability-based security.

use dashmap::DashMap;
use omtae_types::agent::AgentId;
use omtae_types::capability::{capability_matches, Capability, CapabilityCheck};
use tracing::debug;

/// Manages capability grants for all agents.
pub struct CapabilityManager {
    /// Granted capabilities per agent.
    grants: DashMap<AgentId, Vec<Capability>>,
}

impl CapabilityManager {
    /// Create a new capability manager.
    pub fn new() -> Self {
        Self {
            grants: DashMap::new(),
        }
    }

    /// Grant capabilities to an agent.
    pub fn grant(&self, agent_id: AgentId, capabilities: Vec<Capability>) {
        self.grants.insert(agent_id, capabilities);
    }

    /// Check whether an agent has a specific capability.
    pub fn check(&self, agent_id: AgentId, required: &Capability) -> CapabilityCheck {
        let grants = match self.grants.get(&agent_id) {
            Some(g) => g,
            None => {
                return CapabilityCheck::Denied(format!(
                    "No capabilities registered for agent {agent_id}"
                ))
            }
        };

        for granted in grants.value() {
            if capability_matches(granted, required) {
                debug!(agent = %agent_id, ?required, "Capability granted");
                return CapabilityCheck::Granted;
            }
        }

        CapabilityCheck::Denied(format!(
            "Agent {agent_id} does not have capability: {required:?}"
        ))
    }

    /// List all capabilities for an agent.
    pub fn list(&self, agent_id: AgentId) -> Vec<Capability> {
        self.grants
            .get(&agent_id)
            .map(|g| g.value().clone())
            .unwrap_or_default()
    }

    /// Remove all capabilities for an agent.
    pub fn revoke_all(&self, agent_id: AgentId) {
        self.grants.remove(&agent_id);
    }
}

impl Default for CapabilityManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grant_and_check() {
        let mgr = CapabilityManager::new();
        let id = AgentId::new();
        mgr.grant(id, vec![Capability::ToolInvoke("file_read".to_string())]);
        assert!(mgr
            .check(id, &Capability::ToolInvoke("file_read".to_string()))
            .is_granted());
        assert!(!mgr
            .check(id, &Capability::ToolInvoke("shell_exec".to_string()))
            .is_granted());
    }

    #[test]
    fn test_no_grants() {
        let mgr = CapabilityManager::new();
        let id = AgentId::new();
        assert!(!mgr
            .check(id, &Capability::ToolInvoke("anything".to_string()))
            .is_granted());
    }
}
