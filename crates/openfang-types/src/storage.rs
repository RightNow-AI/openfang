//! Backend storage traits for pluggable persistence.
//!
//! These traits define the interface for each storage concern. Concrete
//! implementations (SQLite, PostgreSQL, Qdrant) live in `openfang-memory`.
//!
//! Traits that reference types local to `openfang-memory` (Session, UsageRecord,
//! etc.) are defined there instead. This module contains only the traits whose
//! types are fully available in `openfang-types`.

use crate::agent::{AgentEntry, AgentId};
use crate::error::OpenFangResult;
use crate::memory::{
    Entity, GraphMatch, GraphPattern, MemoryFilter, MemoryFragment, MemoryId, MemorySource,
    Relation,
};
use serde_json::Value;
use std::collections::HashMap;

/// Backend for agent registry and key-value storage.
pub trait StructuredBackend: Send + Sync {
    /// Get a value by key for a specific agent.
    fn get(&self, agent_id: AgentId, key: &str) -> OpenFangResult<Option<Value>>;
    /// Set a key-value pair for a specific agent.
    fn set(&self, agent_id: AgentId, key: &str, value: Value) -> OpenFangResult<()>;
    /// Delete a key-value pair.
    fn delete(&self, agent_id: AgentId, key: &str) -> OpenFangResult<()>;
    /// List all key-value pairs for an agent.
    fn list_kv(&self, agent_id: AgentId) -> OpenFangResult<Vec<(String, Value)>>;
    /// Save an agent entry.
    fn save_agent(&self, entry: &AgentEntry) -> OpenFangResult<()>;
    /// Load an agent by ID.
    fn load_agent(&self, agent_id: AgentId) -> OpenFangResult<Option<AgentEntry>>;
    /// Remove an agent and its data.
    fn remove_agent(&self, agent_id: AgentId) -> OpenFangResult<()>;
    /// Load all agents.
    fn load_all_agents(&self) -> OpenFangResult<Vec<AgentEntry>>;
    /// List agents as (id, name, state) tuples.
    fn list_agents(&self) -> OpenFangResult<Vec<(String, String, String)>>;
}

/// Backend for semantic memory with vector search.
pub trait SemanticBackend: Send + Sync {
    /// Store a memory fragment, optionally with a pre-computed embedding.
    fn remember(
        &self,
        agent_id: AgentId,
        content: &str,
        source: MemorySource,
        scope: &str,
        metadata: HashMap<String, Value>,
        embedding: Option<&[f32]>,
    ) -> OpenFangResult<MemoryId>;

    /// Search for relevant memories, optionally using a query embedding for vector search.
    fn recall(
        &self,
        query: &str,
        limit: usize,
        filter: Option<MemoryFilter>,
        query_embedding: Option<&[f32]>,
    ) -> OpenFangResult<Vec<MemoryFragment>>;

    /// Soft-delete a memory fragment.
    fn forget(&self, id: MemoryId) -> OpenFangResult<()>;

    /// Update the embedding for an existing memory.
    fn update_embedding(&self, id: MemoryId, embedding: &[f32]) -> OpenFangResult<()>;
}

/// Backend for the knowledge graph.
pub trait KnowledgeBackend: Send + Sync {
    /// Add an entity to the graph.
    fn add_entity(&self, entity: Entity) -> OpenFangResult<String>;
    /// Add a relation between entities.
    fn add_relation(&self, relation: Relation) -> OpenFangResult<String>;
    /// Query the graph by pattern.
    fn query_graph(&self, pattern: GraphPattern) -> OpenFangResult<Vec<GraphMatch>>;
}
