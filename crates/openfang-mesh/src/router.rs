//! [`MeshRouter`] — capability-aware execution target selection.
//!
//! The router implements a four-tier priority system for selecting where to
//! execute a task:
//!
//! 1. **Active Hand** — a Hand instance that is `Active` and whose definition
//!    lists tools that overlap with the required capabilities.
//! 2. **Local agent** — a running local agent whose tags overlap with the
//!    required capabilities.
//! 3. **Remote peer** — a connected OFP peer that has a remote agent whose
//!    tags overlap with the required capabilities.
//! 4. **Spawn new** — no match found; the caller should spawn a new agent.

use std::sync::Arc;

use openfang_hands::registry::HandRegistry;
use openfang_hands::{HandDefinition, HandInstance, HandStatus};
use openfang_types::agent::AgentId;
use openfang_wire::registry::PeerRegistry;
use tracing::{debug, info};
use uuid::Uuid;

/// Configuration for the [`MeshRouter`].
#[derive(Debug, Clone)]
pub struct MeshRouterConfig {
    /// Minimum capability overlap score (0.0–1.0) required to select a target.
    /// A score of 0.0 means any overlap is acceptable; 1.0 requires all
    /// capabilities to be matched.
    pub min_score: f32,
    /// Whether to consider remote peers when no local target is found.
    pub enable_remote_routing: bool,
}

impl Default for MeshRouterConfig {
    fn default() -> Self {
        Self {
            min_score: 0.0,
            enable_remote_routing: true,
        }
    }
}

/// The selected execution target for a task.
#[derive(Debug, Clone)]
pub enum ExecutionTarget {
    /// An active Hand instance with the given instance ID and linked agent ID.
    Hand {
        /// The Hand instance ID.
        instance_id: Uuid,
        /// The agent ID linked to this Hand instance (if spawned).
        agent_id: Option<AgentId>,
        /// The Hand definition ID (e.g. `"clip"`).
        hand_id: String,
        /// Capability overlap score (0.0–1.0).
        score: f32,
    },
    /// A running local agent.
    LocalAgent {
        /// The agent ID.
        agent_id: AgentId,
        /// The agent's display name.
        agent_name: String,
        /// Capability overlap score (0.0–1.0).
        score: f32,
    },
    /// A remote agent on a connected OFP peer.
    RemotePeer {
        /// The peer's node ID.
        node_id: String,
        /// The remote agent's name or ID.
        agent_id: String,
        /// Capability overlap score (0.0–1.0).
        score: f32,
    },
    /// No match found — the caller should spawn a new agent.
    SpawnNew {
        /// The capabilities that were requested.
        requested_capabilities: Vec<String>,
    },
}

/// A snapshot of a local agent for routing purposes.
///
/// This is a lightweight view of an agent that the router uses to avoid
/// depending on the full kernel types.
#[derive(Debug, Clone)]
pub struct LocalAgentView {
    /// The agent ID.
    pub agent_id: AgentId,
    /// The agent's display name.
    pub name: String,
    /// Whether the agent is currently running.
    pub is_running: bool,
    /// Tags associated with the agent (used for capability matching).
    pub tags: Vec<String>,
    /// Tools available to the agent.
    pub tools: Vec<String>,
}

/// The mesh router.
///
/// The router holds references to the Hand registry and peer registry, and
/// accepts a snapshot of local agents at routing time (to avoid a circular
/// dependency on the kernel).
pub struct MeshRouter {
    hand_registry: Arc<HandRegistry>,
    peer_registry: Arc<PeerRegistry>,
    config: MeshRouterConfig,
}

impl MeshRouter {
    /// Create a new [`MeshRouter`].
    pub fn new(
        hand_registry: Arc<HandRegistry>,
        peer_registry: Arc<PeerRegistry>,
        config: MeshRouterConfig,
    ) -> Self {
        Self {
            hand_registry,
            peer_registry,
            config,
        }
    }

    /// Select the best execution target for a task with the given capabilities.
    ///
    /// `local_agents` is a snapshot of currently running local agents provided
    /// by the caller (typically the kernel or supervisor engine).
    pub fn route(
        &self,
        capabilities: &[String],
        local_agents: &[LocalAgentView],
    ) -> ExecutionTarget {
        // ── Tier 1: Active Hands ────────────────────────────────────────────
        if let Some(target) = self.find_hand_target(capabilities) {
            info!(
                hand_id = %match &target { ExecutionTarget::Hand { hand_id, .. } => hand_id.as_str(), _ => "" },
                "MeshRouter: routing to active Hand"
            );
            return target;
        }

        // ── Tier 2: Local agents ────────────────────────────────────────────
        if let Some(target) = self.find_local_agent_target(capabilities, local_agents) {
            info!(
                agent_name = %match &target { ExecutionTarget::LocalAgent { agent_name, .. } => agent_name.as_str(), _ => "" },
                "MeshRouter: routing to local agent"
            );
            return target;
        }

        // ── Tier 3: Remote peers ────────────────────────────────────────────
        if self.config.enable_remote_routing {
            if let Some(target) = self.find_remote_target(capabilities) {
                info!(
                    node_id = %match &target { ExecutionTarget::RemotePeer { node_id, .. } => node_id.as_str(), _ => "" },
                    "MeshRouter: routing to remote peer"
                );
                return target;
            }
        }

        // ── Tier 4: Spawn new ───────────────────────────────────────────────
        debug!(
            capabilities = ?capabilities,
            "MeshRouter: no suitable target found, recommending spawn"
        );
        ExecutionTarget::SpawnNew {
            requested_capabilities: capabilities.to_vec(),
        }
    }

    /// Compute the capability overlap score between a set of available
    /// capabilities and the required capabilities.
    ///
    /// Returns a value in `[0.0, 1.0]` where `1.0` means all required
    /// capabilities are covered.
    pub fn score_capabilities(available: &[String], required: &[String]) -> f32 {
        if required.is_empty() {
            return 1.0;
        }
        let matched = required
            .iter()
            .filter(|req| {
                available
                    .iter()
                    .any(|avail| avail.to_lowercase().contains(&req.to_lowercase()))
            })
            .count();
        matched as f32 / required.len() as f32
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    fn find_hand_target(&self, capabilities: &[String]) -> Option<ExecutionTarget> {
        let instances = self.hand_registry.list_instances();
        let definitions: Vec<HandDefinition> = self.hand_registry.list_definitions();

        let mut best: Option<(f32, HandInstance, HandDefinition)> = None;

        for instance in instances {
            if instance.status != HandStatus::Active {
                continue;
            }
            let def = definitions.iter().find(|d| d.id == instance.hand_id)?;
            let available: Vec<String> =
                def.tools.iter().chain(def.skills.iter()).cloned().collect();
            let score = Self::score_capabilities(&available, capabilities);
            if score >= self.config.min_score && best.as_ref().is_none_or(|(s, _, _)| score > *s) {
                best = Some((score, instance, def.clone()));
            }
        }

        best.map(|(score, instance, def)| ExecutionTarget::Hand {
            instance_id: instance.instance_id,
            agent_id: instance.agent_id,
            hand_id: def.id,
            score,
        })
    }

    fn find_local_agent_target(
        &self,
        capabilities: &[String],
        agents: &[LocalAgentView],
    ) -> Option<ExecutionTarget> {
        let mut best: Option<(f32, &LocalAgentView)> = None;

        for agent in agents {
            if !agent.is_running {
                continue;
            }
            let available: Vec<String> = agent
                .tags
                .iter()
                .chain(agent.tools.iter())
                .cloned()
                .collect();
            let score = Self::score_capabilities(&available, capabilities);
            if score >= self.config.min_score && best.as_ref().is_none_or(|(s, _)| score > *s) {
                best = Some((score, agent));
            }
        }

        best.map(|(score, agent)| ExecutionTarget::LocalAgent {
            agent_id: agent.agent_id,
            agent_name: agent.name.clone(),
            score,
        })
    }

    fn find_remote_target(&self, capabilities: &[String]) -> Option<ExecutionTarget> {
        // Use the peer registry's find_agents to search by capability keywords
        let query = capabilities.join(" ");
        let remote_agents = if query.is_empty() {
            self.peer_registry.all_remote_agents()
        } else {
            self.peer_registry.find_agents(&query)
        };

        let mut best: Option<(f32, String, String)> = None; // (score, node_id, agent_id)

        for remote in remote_agents {
            let available: Vec<String> = remote.info.tags.to_vec();
            let score = Self::score_capabilities(&available, capabilities);
            if score >= self.config.min_score && best.as_ref().is_none_or(|(s, _, _)| score > *s) {
                best = Some((score, remote.peer_node_id.clone(), remote.info.id.clone()));
            }
        }

        best.map(|(score, node_id, agent_id)| ExecutionTarget::RemotePeer {
            node_id,
            agent_id,
            score,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_capabilities_full_match() {
        let available = vec!["code".to_string(), "rust".to_string(), "review".to_string()];
        let required = vec!["code".to_string(), "rust".to_string()];
        let score = MeshRouter::score_capabilities(&available, &required);
        assert!(
            (score - 1.0).abs() < f32::EPSILON,
            "Expected 1.0, got {score}"
        );
    }

    #[test]
    fn test_score_capabilities_partial_match() {
        let available = vec!["code".to_string()];
        let required = vec!["code".to_string(), "rust".to_string()];
        let score = MeshRouter::score_capabilities(&available, &required);
        assert!(
            (score - 0.5).abs() < f32::EPSILON,
            "Expected 0.5, got {score}"
        );
    }

    #[test]
    fn test_score_capabilities_no_match() {
        let available = vec!["design".to_string()];
        let required = vec!["code".to_string(), "rust".to_string()];
        let score = MeshRouter::score_capabilities(&available, &required);
        assert!(score < f32::EPSILON, "Expected 0.0, got {score}");
    }

    #[test]
    fn test_score_capabilities_empty_required() {
        let available = vec!["code".to_string()];
        let required: Vec<String> = vec![];
        let score = MeshRouter::score_capabilities(&available, &required);
        assert!(
            (score - 1.0).abs() < f32::EPSILON,
            "Expected 1.0 for empty required"
        );
    }

    #[test]
    fn test_score_capabilities_case_insensitive() {
        let available = vec!["Code-Review".to_string()];
        let required = vec!["code-review".to_string()];
        let score = MeshRouter::score_capabilities(&available, &required);
        assert!(
            (score - 1.0).abs() < f32::EPSILON,
            "Expected case-insensitive match"
        );
    }
}
