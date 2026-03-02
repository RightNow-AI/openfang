//! Peer registry — tracks connected peers and their agents.
//!
//! The [`PeerRegistry`] is a thread-safe, concurrent data structure that
//! records all known remote peers, their connection state, and the agents
//! they advertise.

use crate::message::RemoteAgentInfo;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use std::net::SocketAddr;
use std::sync::Arc;

/// A tracked remote agent, enriched with the owning peer's identity.
#[derive(Debug, Clone)]
pub struct RemoteAgent {
    /// The remote peer that hosts this agent.
    pub peer_node_id: String,
    /// Agent details from the wire protocol.
    pub info: RemoteAgentInfo,
}

/// Connection state of a peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerState {
    /// Handshake completed, fully connected.
    Connected,
    /// Connection lost but not removed yet (eligible for reconnect).
    Disconnected,
}

/// An entry representing a single known peer.
#[derive(Debug, Clone)]
pub struct PeerEntry {
    /// Unique node ID of the peer.
    pub node_id: String,
    /// Human-readable node name.
    pub node_name: String,
    /// Socket address of the peer.
    pub address: SocketAddr,
    /// Agents advertised by this peer.
    pub agents: Vec<RemoteAgentInfo>,
    /// Connection state.
    pub state: PeerState,
    /// When the peer first connected.
    pub connected_at: DateTime<Utc>,
    /// Protocol version negotiated during handshake.
    pub protocol_version: u32,
}

/// Thread-safe registry of all known peers.
///
/// Backed by [`DashMap`] for sharded concurrent access — readers on
/// different shards never block each other or writers.
#[derive(Debug, Clone)]
pub struct PeerRegistry {
    peers: Arc<DashMap<String, PeerEntry>>,
}

impl PeerRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            peers: Arc::new(DashMap::new()),
        }
    }

    /// Register or update a peer after a successful handshake.
    pub fn add_peer(&self, entry: PeerEntry) {
        self.peers.insert(entry.node_id.clone(), entry);
    }

    /// Remove a peer entirely.
    pub fn remove_peer(&self, node_id: &str) -> Option<PeerEntry> {
        self.peers.remove(node_id).map(|(_, v)| v)
    }

    /// Mark a peer as disconnected (but keep its entry for possible reconnect).
    pub fn mark_disconnected(&self, node_id: &str) {
        if let Some(mut entry) = self.peers.get_mut(node_id) {
            entry.state = PeerState::Disconnected;
        }
    }

    /// Mark a peer as connected again.
    pub fn mark_connected(&self, node_id: &str) {
        if let Some(mut entry) = self.peers.get_mut(node_id) {
            entry.state = PeerState::Connected;
        }
    }

    /// Get a snapshot of a specific peer.
    pub fn get_peer(&self, node_id: &str) -> Option<PeerEntry> {
        self.peers.get(node_id).map(|r| r.value().clone())
    }

    /// Get all connected peers.
    pub fn connected_peers(&self) -> Vec<PeerEntry> {
        self.peers
            .iter()
            .filter(|r| r.state == PeerState::Connected)
            .map(|r| r.value().clone())
            .collect()
    }

    /// Get all peers (connected + disconnected).
    pub fn all_peers(&self) -> Vec<PeerEntry> {
        self.peers.iter().map(|r| r.value().clone()).collect()
    }

    /// Update the agent list for a peer (e.g., after an AgentSpawned notification).
    pub fn update_agents(&self, node_id: &str, agents: Vec<RemoteAgentInfo>) {
        if let Some(mut entry) = self.peers.get_mut(node_id) {
            entry.agents = agents;
        }
    }

    /// Add a single agent to a peer's advertised list.
    pub fn add_agent(&self, node_id: &str, agent: RemoteAgentInfo) {
        if let Some(mut entry) = self.peers.get_mut(node_id) {
            if let Some(existing) = entry.agents.iter_mut().find(|a| a.id == agent.id) {
                *existing = agent;
            } else {
                entry.agents.push(agent);
            }
        }
    }

    /// Remove an agent from a peer's advertised list.
    pub fn remove_agent(&self, node_id: &str, agent_id: &str) {
        if let Some(mut entry) = self.peers.get_mut(node_id) {
            entry.agents.retain(|a| a.id != agent_id);
        }
    }

    /// Find all remote agents matching a query (searches name, tags, description).
    ///
    /// Uses allocation-free ASCII case-insensitive matching — the query is
    /// lowercased once; field comparisons are done byte-by-byte without
    /// allocating temporary Strings.
    pub fn find_agents(&self, query: &str) -> Vec<RemoteAgent> {
        let query_lower = query.to_ascii_lowercase();
        let mut results = Vec::new();

        for entry in self.peers.iter() {
            let peer = entry.value();
            if peer.state != PeerState::Connected {
                continue;
            }
            for agent in &peer.agents {
                let matches = ascii_contains_ignore_case(&agent.name, &query_lower)
                    || ascii_contains_ignore_case(&agent.description, &query_lower)
                    || agent
                        .tags
                        .iter()
                        .any(|t| ascii_contains_ignore_case(t, &query_lower));
                if matches {
                    results.push(RemoteAgent {
                        peer_node_id: peer.node_id.clone(),
                        info: agent.clone(),
                    });
                }
            }
        }

        results
    }

    /// Get all remote agents across all connected peers.
    pub fn all_remote_agents(&self) -> Vec<RemoteAgent> {
        let mut results = Vec::new();

        for entry in self.peers.iter() {
            let peer = entry.value();
            if peer.state != PeerState::Connected {
                continue;
            }
            for agent in &peer.agents {
                results.push(RemoteAgent {
                    peer_node_id: peer.node_id.clone(),
                    info: agent.clone(),
                });
            }
        }

        results
    }

    /// Number of connected peers.
    pub fn connected_count(&self) -> usize {
        self.peers
            .iter()
            .filter(|r| r.state == PeerState::Connected)
            .count()
    }

    /// Total number of peers (including disconnected).
    pub fn total_count(&self) -> usize {
        self.peers.len()
    }
}

/// Allocation-free ASCII case-insensitive substring search.
///
/// `needle_lower` **must** already be lowercase. Haystack bytes are
/// folded to lowercase on the fly without allocating.
#[inline]
fn ascii_contains_ignore_case(haystack: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return true;
    }
    let needle_bytes = needle_lower.as_bytes();
    let needle_len = needle_bytes.len();
    if needle_len > haystack.len() {
        return false;
    }
    haystack
        .as_bytes()
        .windows(needle_len)
        .any(|w| w.iter().zip(needle_bytes).all(|(h, n)| h.to_ascii_lowercase() == *n))
}

impl Default for PeerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_peer(node_id: &str, agents: Vec<RemoteAgentInfo>) -> PeerEntry {
        PeerEntry {
            node_id: node_id.to_string(),
            node_name: format!("{node_id}-name"),
            address: "127.0.0.1:9000".parse().unwrap(),
            agents,
            state: PeerState::Connected,
            connected_at: Utc::now(),
            protocol_version: 1,
        }
    }

    fn make_agent(id: &str, name: &str, tags: &[&str]) -> RemoteAgentInfo {
        RemoteAgentInfo {
            id: id.to_string(),
            name: name.to_string(),
            description: format!("{name} agent"),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            tools: vec![],
            state: "running".to_string(),
        }
    }

    #[test]
    fn test_add_and_get_peer() {
        let registry = PeerRegistry::new();
        let peer = make_peer("node-1", vec![make_agent("a1", "coder", &["code"])]);
        registry.add_peer(peer);

        let retrieved = registry.get_peer("node-1").unwrap();
        assert_eq!(retrieved.node_id, "node-1");
        assert_eq!(retrieved.agents.len(), 1);
        assert_eq!(retrieved.agents[0].name, "coder");
    }

    #[test]
    fn test_remove_peer() {
        let registry = PeerRegistry::new();
        registry.add_peer(make_peer("node-1", vec![]));
        assert_eq!(registry.total_count(), 1);

        let removed = registry.remove_peer("node-1");
        assert!(removed.is_some());
        assert_eq!(registry.total_count(), 0);
    }

    #[test]
    fn test_disconnect_reconnect() {
        let registry = PeerRegistry::new();
        registry.add_peer(make_peer("node-1", vec![]));
        assert_eq!(registry.connected_count(), 1);

        registry.mark_disconnected("node-1");
        assert_eq!(registry.connected_count(), 0);
        assert_eq!(registry.total_count(), 1);

        registry.mark_connected("node-1");
        assert_eq!(registry.connected_count(), 1);
    }

    #[test]
    fn test_find_agents_by_name() {
        let registry = PeerRegistry::new();
        registry.add_peer(make_peer(
            "node-1",
            vec![
                make_agent("a1", "coder", &["code"]),
                make_agent("a2", "researcher", &["research"]),
            ],
        ));
        registry.add_peer(make_peer(
            "node-2",
            vec![make_agent("a3", "code-reviewer", &["code", "review"])],
        ));

        let results = registry.find_agents("code");
        assert_eq!(results.len(), 2); // "coder" and "code-reviewer"
    }

    #[test]
    fn test_find_agents_by_tag() {
        let registry = PeerRegistry::new();
        registry.add_peer(make_peer(
            "node-1",
            vec![make_agent("a1", "helper", &["security", "audit"])],
        ));

        let results = registry.find_agents("security");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].info.name, "helper");
        assert_eq!(results[0].peer_node_id, "node-1");
    }

    #[test]
    fn test_find_agents_skips_disconnected() {
        let registry = PeerRegistry::new();
        registry.add_peer(make_peer(
            "node-1",
            vec![make_agent("a1", "coder", &["code"])],
        ));
        registry.mark_disconnected("node-1");

        let results = registry.find_agents("coder");
        assert!(results.is_empty());
    }

    #[test]
    fn test_add_remove_agent() {
        let registry = PeerRegistry::new();
        registry.add_peer(make_peer("node-1", vec![]));

        registry.add_agent("node-1", make_agent("a1", "coder", &[]));
        assert_eq!(registry.get_peer("node-1").unwrap().agents.len(), 1);

        registry.remove_agent("node-1", "a1");
        assert_eq!(registry.get_peer("node-1").unwrap().agents.len(), 0);
    }

    #[test]
    fn test_all_remote_agents() {
        let registry = PeerRegistry::new();
        registry.add_peer(make_peer("node-1", vec![make_agent("a1", "coder", &[])]));
        registry.add_peer(make_peer(
            "node-2",
            vec![
                make_agent("a2", "writer", &[]),
                make_agent("a3", "tester", &[]),
            ],
        ));

        let all = registry.all_remote_agents();
        assert_eq!(all.len(), 3);
    }
}
