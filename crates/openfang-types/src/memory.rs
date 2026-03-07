//! Memory substrate types: fragments, sources, filters, and the unified Memory trait.

use crate::agent::AgentId;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Unique identifier for a memory fragment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MemoryId(pub Uuid);

impl MemoryId {
    /// Create a new random MemoryId.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for MemoryId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MemoryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Where a memory came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemorySource {
    /// From a conversation/interaction.
    Conversation,
    /// From a document that was processed.
    Document,
    /// From an observation (tool output, web page, etc.).
    Observation,
    /// Inferred by the agent from existing knowledge.
    Inference,
    /// Explicitly provided by the user.
    UserProvided,
    /// From a system event.
    System,
}

/// A single unit of memory stored in the semantic store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFragment {
    /// Unique ID.
    pub id: MemoryId,
    /// Which agent owns this memory.
    pub agent_id: AgentId,
    /// The textual content of this memory.
    pub content: String,
    /// Vector embedding (populated by the semantic store).
    pub embedding: Option<Vec<f32>>,
    /// Arbitrary metadata.
    pub metadata: HashMap<String, serde_json::Value>,
    /// How this memory was created.
    pub source: MemorySource,
    /// Confidence score (0.0 - 1.0).
    pub confidence: f32,
    /// When this memory was created.
    pub created_at: DateTime<Utc>,
    /// When this memory was last accessed.
    pub accessed_at: DateTime<Utc>,
    /// How many times this memory has been accessed.
    pub access_count: u64,
    /// Memory scope/collection name.
    pub scope: String,
}

/// Filter criteria for memory recall.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryFilter {
    /// Filter by agent ID.
    pub agent_id: Option<AgentId>,
    /// Filter by source type.
    pub source: Option<MemorySource>,
    /// Filter by scope.
    pub scope: Option<String>,
    /// Minimum confidence threshold.
    pub min_confidence: Option<f32>,
    /// Only memories created after this time.
    pub after: Option<DateTime<Utc>>,
    /// Only memories created before this time.
    pub before: Option<DateTime<Utc>>,
    /// Metadata key-value filters.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl MemoryFilter {
    /// Create a filter for a specific agent.
    pub fn agent(agent_id: AgentId) -> Self {
        Self {
            agent_id: Some(agent_id),
            ..Default::default()
        }
    }

    /// Create a filter for a specific scope.
    pub fn scope(scope: impl Into<String>) -> Self {
        Self {
            scope: Some(scope.into()),
            ..Default::default()
        }
    }
}

/// An entity in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    /// Unique entity ID.
    pub id: String,
    /// Entity type (Person, Organization, Project, etc.).
    pub entity_type: EntityType,
    /// Display name.
    pub name: String,
    /// Arbitrary properties.
    pub properties: HashMap<String, serde_json::Value>,
    /// When this entity was created.
    pub created_at: DateTime<Utc>,
    /// When this entity was last updated.
    pub updated_at: DateTime<Utc>,
}

/// Types of entities in the knowledge graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    /// A person.
    Person,
    /// An organization.
    Organization,
    /// A project.
    Project,
    /// A concept or idea.
    Concept,
    /// An event.
    Event,
    /// A location.
    Location,
    /// A document.
    Document,
    /// A tool.
    Tool,
    /// A custom type.
    Custom(String),
}

/// A relation between two entities in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    /// Source entity ID.
    pub source: String,
    /// Relation type.
    pub relation: RelationType,
    /// Target entity ID.
    pub target: String,
    /// Arbitrary properties on the relation.
    pub properties: HashMap<String, serde_json::Value>,
    /// Confidence score (0.0 - 1.0).
    pub confidence: f32,
    /// When this relation was created.
    pub created_at: DateTime<Utc>,
}

/// Types of relations in the knowledge graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    /// Entity works at an organization.
    WorksAt,
    /// Entity knows about a concept.
    KnowsAbout,
    /// Entities are related.
    RelatedTo,
    /// Entity depends on another.
    DependsOn,
    /// Entity is owned by another.
    OwnedBy,
    /// Entity was created by another.
    CreatedBy,
    /// Entity is located in another.
    LocatedIn,
    /// Entity is part of another.
    PartOf,
    /// Entity uses another.
    Uses,
    /// Entity produces another.
    Produces,
    /// A custom relation type.
    Custom(String),
}

/// A pattern for querying the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphPattern {
    /// Optional source entity filter.
    pub source: Option<String>,
    /// Optional relation type filter.
    pub relation: Option<RelationType>,
    /// Optional target entity filter.
    pub target: Option<String>,
    /// Maximum traversal depth.
    pub max_depth: u32,
}

/// A result from a graph query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphMatch {
    /// The source entity.
    pub source: Entity,
    /// The relation.
    pub relation: Relation,
    /// The target entity.
    pub target: Entity,
}

/// Report from memory consolidation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationReport {
    /// Number of memories merged.
    pub memories_merged: u64,
    /// Number of memories whose confidence decayed.
    pub memories_decayed: u64,
    /// How long the consolidation took.
    pub duration_ms: u64,
}

/// Format for memory export/import.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ExportFormat {
    /// JSON format.
    Json,
    /// MessagePack binary format.
    MessagePack,
}

/// Report from memory import.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportReport {
    /// Number of entities imported.
    pub entities_imported: u64,
    /// Number of relations imported.
    pub relations_imported: u64,
    /// Number of memories imported.
    pub memories_imported: u64,
    /// Errors encountered during import.
    pub errors: Vec<String>,
}

/// The unified Memory trait that agents interact with.
///
/// This abstracts over the structured store (SQLite), semantic store,
/// and knowledge graph, presenting a single coherent API.
#[async_trait]
pub trait Memory: Send + Sync {
    // -- Key-value operations (structured store) --

    /// Get a value by key for a specific agent.
    async fn get(
        &self,
        agent_id: AgentId,
        key: &str,
    ) -> crate::error::OpenFangResult<Option<serde_json::Value>>;

    /// Set a key-value pair for a specific agent.
    async fn set(
        &self,
        agent_id: AgentId,
        key: &str,
        value: serde_json::Value,
    ) -> crate::error::OpenFangResult<()>;

    /// Delete a key-value pair for a specific agent.
    async fn delete(&self, agent_id: AgentId, key: &str) -> crate::error::OpenFangResult<()>;

    // -- Semantic operations --

    /// Store a new memory fragment.
    async fn remember(
        &self,
        agent_id: AgentId,
        content: &str,
        source: MemorySource,
        scope: &str,
        metadata: HashMap<String, serde_json::Value>,
    ) -> crate::error::OpenFangResult<MemoryId>;

    /// Semantic search for relevant memories.
    async fn recall(
        &self,
        query: &str,
        limit: usize,
        filter: Option<MemoryFilter>,
    ) -> crate::error::OpenFangResult<Vec<MemoryFragment>>;

    /// Soft-delete a memory fragment.
    async fn forget(&self, id: MemoryId) -> crate::error::OpenFangResult<()>;

    // -- Knowledge graph operations --

    /// Add an entity to the knowledge graph.
    async fn add_entity(&self, entity: Entity) -> crate::error::OpenFangResult<String>;

    /// Add a relation between entities.
    async fn add_relation(&self, relation: Relation) -> crate::error::OpenFangResult<String>;

    /// Query the knowledge graph.
    async fn query_graph(
        &self,
        pattern: GraphPattern,
    ) -> crate::error::OpenFangResult<Vec<GraphMatch>>;

    // -- Session persistence --

    /// Save a session to the backing store.
    async fn save_session(
        &self,
        session: &crate::session::Session,
    ) -> crate::error::OpenFangResult<()> {
        let _ = session;
        Err(crate::error::OpenFangError::Internal(
            "save_session not implemented".into(),
        ))
    }

    // -- Embedding-aware operations --

    /// Semantic search with a pre-computed embedding vector.
    async fn recall_with_embedding_async(
        &self,
        query: &str,
        limit: usize,
        filter: Option<MemoryFilter>,
        _query_embedding: Option<&[f32]>,
    ) -> crate::error::OpenFangResult<Vec<MemoryFragment>> {
        // Default: fall back to text-only recall
        self.recall(query, limit, filter).await
    }

    /// Store a memory fragment with a pre-computed embedding vector.
    async fn remember_with_embedding_async(
        &self,
        agent_id: AgentId,
        content: &str,
        source: MemorySource,
        scope: &str,
        metadata: HashMap<String, serde_json::Value>,
        _embedding: Option<&[f32]>,
    ) -> crate::error::OpenFangResult<MemoryId> {
        // Default: fall back to text-only remember
        self.remember(agent_id, content, source, scope, metadata).await
    }

    // -- Maintenance --

    /// Consolidate and optimize memory.
    async fn consolidate(&self) -> crate::error::OpenFangResult<ConsolidationReport>;

    /// Export all memory data.
    async fn export(&self, format: ExportFormat) -> crate::error::OpenFangResult<Vec<u8>>;

    /// Import memory data.
    async fn import(
        &self,
        data: &[u8],
        format: ExportFormat,
    ) -> crate::error::OpenFangResult<ImportReport>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_filter_agent() {
        let id = AgentId::new();
        let filter = MemoryFilter::agent(id);
        assert_eq!(filter.agent_id, Some(id));
        assert!(filter.source.is_none());
    }

    #[test]
    fn test_memory_fragment_serialization() {
        let fragment = MemoryFragment {
            id: MemoryId::new(),
            agent_id: AgentId::new(),
            content: "Test memory".to_string(),
            embedding: None,
            metadata: HashMap::new(),
            source: MemorySource::Conversation,
            confidence: 0.95,
            created_at: Utc::now(),
            accessed_at: Utc::now(),
            access_count: 0,
            scope: "episodic".to_string(),
        };
        let json = serde_json::to_string(&fragment).unwrap();
        let deserialized: MemoryFragment = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.content, "Test memory");
    }
}
