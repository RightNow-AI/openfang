//! Cypher query templates and analytics API for the FalkorDB graph.
//!
//! Provides typed, parameterized Cypher queries over the knowledge graph
//! populated by the ETL pipeline. All queries use `ro_query` (read-only)
//! where possible and return strongly-typed result structs.

use crate::FalkorAnalytics;
use openfang_types::error::{OpenFangError, OpenFangResult};
use serde::Serialize;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// A node in the graph with its properties.
#[derive(Debug, Clone, Serialize)]
pub struct GraphNode {
    /// The entity ID stored as a property (our domain ID, not FalkorDB internal).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Entity type label (Person, Organization, Concept, etc.).
    pub entity_type: String,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
    /// ISO-8601 last-update timestamp.
    pub updated_at: String,
}

/// An edge in the graph with its properties.
#[derive(Debug, Clone, Serialize)]
pub struct GraphEdge {
    /// Source entity ID.
    pub source_id: String,
    /// Target entity ID.
    pub target_id: String,
    /// Relation type label (WORKS_AT, KNOWS_ABOUT, etc.).
    pub relation_type: String,
    /// Confidence score (0.0–1.0).
    pub confidence: f64,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
}

/// A neighbor result: the connected entity plus the connecting edge.
#[derive(Debug, Clone, Serialize)]
pub struct Neighbor {
    pub entity: GraphNode,
    pub relation_type: String,
    pub confidence: f64,
    /// "outgoing" or "incoming"
    pub direction: String,
}

/// Distribution count for a single category.
#[derive(Debug, Clone, Serialize)]
pub struct TypeCount {
    pub label: String,
    pub count: i64,
}

/// A hub entity with its connection count.
#[derive(Debug, Clone, Serialize)]
pub struct HubEntity {
    pub entity: GraphNode,
    pub connection_count: i64,
}

/// A memory node stored in the graph.
#[derive(Debug, Clone, Serialize)]
pub struct GraphMemory {
    pub id: String,
    pub content: String,
    pub agent_id: String,
    pub source: String,
    pub confidence: f64,
    pub created_at: String,
    pub accessed_at: String,
    pub access_count: i64,
    pub scope: String,
}

/// A shortest-path step.
#[derive(Debug, Clone, Serialize)]
pub struct PathStep {
    pub entity_id: String,
    pub entity_name: String,
    pub entity_type: String,
}

/// Full shortest-path result.
#[derive(Debug, Clone, Serialize)]
pub struct ShortestPath {
    pub steps: Vec<PathStep>,
    pub length: usize,
}

/// Summary statistics for the entire graph.
#[derive(Debug, Clone, Serialize)]
pub struct GraphStats {
    pub total_entities: i64,
    pub total_memories: i64,
    pub total_relations: i64,
    pub entity_type_distribution: Vec<TypeCount>,
    pub relation_type_distribution: Vec<TypeCount>,
}

/// Per-agent memory statistics.
#[derive(Debug, Clone, Serialize)]
pub struct AgentMemoryStats {
    pub agent_id: String,
    pub memory_count: i64,
    pub avg_confidence: f64,
    pub oldest: String,
    pub newest: String,
}

// ---------------------------------------------------------------------------
// Cypher templates (constants)
// ---------------------------------------------------------------------------

mod cypher {
    /// Find all neighbors of an entity by its domain ID.
    pub const ENTITY_NEIGHBORS: &str = r#"
        MATCH (e:Entity {id: $entity_id})-[r:RELATION]-(n:Entity)
        RETURN e.id AS src_id,
               n.id AS id, n.name AS name, n.type AS type,
               n.created_at AS created_at, n.updated_at AS updated_at,
               r.type AS rel_type, r.confidence AS confidence,
               CASE WHEN startNode(r) = e THEN 'outgoing' ELSE 'incoming' END AS direction
    "#;

    /// Shortest path between two entities (max 10 hops).
    pub const SHORTEST_PATH: &str = r#"
        MATCH p = shortestPath((a:Entity {id: $source_id})-[:RELATION*..10]-(b:Entity {id: $target_id}))
        RETURN nodes(p) AS path_nodes
    "#;

    /// Entity type distribution.
    pub const ENTITY_TYPE_DISTRIBUTION: &str = r#"
        MATCH (e:Entity)
        RETURN e.type AS label, count(e) AS cnt
        ORDER BY cnt DESC
    "#;

    /// Relation type distribution.
    pub const RELATION_TYPE_DISTRIBUTION: &str = r#"
        MATCH ()-[r:RELATION]->()
        RETURN r.type AS label, count(r) AS cnt
        ORDER BY cnt DESC
    "#;

    /// Most connected entities (hubs).
    pub const TOP_HUBS: &str = r#"
        MATCH (e:Entity)-[r:RELATION]-()
        RETURN e.id AS id, e.name AS name, e.type AS type,
               e.created_at AS created_at, e.updated_at AS updated_at,
               count(r) AS connections
        ORDER BY connections DESC
        LIMIT $limit
    "#;

    /// Search entities by name (case-insensitive substring).
    pub const ENTITY_SEARCH: &str = r#"
        MATCH (e:Entity)
        WHERE toLower(e.name) CONTAINS toLower($query)
        RETURN e.id AS id, e.name AS name, e.type AS type,
               e.created_at AS created_at, e.updated_at AS updated_at
        ORDER BY e.name
        LIMIT $limit
    "#;

    /// Get entity by exact ID.
    pub const ENTITY_BY_ID: &str = r#"
        MATCH (e:Entity {id: $entity_id})
        RETURN e.id AS id, e.name AS name, e.type AS type,
               e.created_at AS created_at, e.updated_at AS updated_at
    "#;

    /// Agent memory timeline (most recent first).
    pub const AGENT_MEMORIES: &str = r#"
        MATCH (m:Memory)
        WHERE m.agent_id = $agent_id
        RETURN m.id AS id, m.content AS content, m.agent_id AS agent_id,
               m.source AS source, m.confidence AS confidence,
               m.created_at AS created_at, m.accessed_at AS accessed_at,
               m.access_count AS access_count, m.scope AS scope
        ORDER BY m.created_at DESC
        LIMIT $limit
    "#;

    /// Per-agent memory statistics.
    pub const AGENT_MEMORY_STATS: &str = r#"
        MATCH (m:Memory)
        RETURN m.agent_id AS agent_id,
               count(m) AS memory_count,
               avg(m.confidence) AS avg_confidence,
               min(m.created_at) AS oldest,
               max(m.created_at) AS newest
        ORDER BY memory_count DESC
    "#;

    /// Total entity count.
    pub const COUNT_ENTITIES: &str = "MATCH (e:Entity) RETURN count(e) AS cnt";

    /// Total memory count.
    pub const COUNT_MEMORIES: &str = "MATCH (m:Memory) RETURN count(m) AS cnt";

    /// Total relation count.
    pub const COUNT_RELATIONS: &str = "MATCH ()-[r:RELATION]->() RETURN count(r) AS cnt";

    /// High-confidence relations (above threshold).
    pub const HIGH_CONFIDENCE_RELATIONS: &str = r#"
        MATCH (a:Entity)-[r:RELATION]->(b:Entity)
        WHERE r.confidence >= $threshold
        RETURN a.id AS source_id, a.name AS source_name,
               r.type AS rel_type, r.confidence AS confidence, r.created_at AS created_at,
               b.id AS target_id, b.name AS target_name
        ORDER BY r.confidence DESC
        LIMIT $limit
    "#;
}

// ---------------------------------------------------------------------------
// Helper: extract values from FalkorValue
// ---------------------------------------------------------------------------

use falkordb::FalkorValue;

fn val_to_string(v: &FalkorValue) -> String {
    match v {
        FalkorValue::String(s) => s.clone(),
        FalkorValue::I64(i) => i.to_string(),
        FalkorValue::F64(f) => f.to_string(),
        FalkorValue::None => String::new(),
        other => format!("{:?}", other),
    }
}

fn val_to_i64(v: &FalkorValue) -> i64 {
    match v {
        FalkorValue::I64(i) => *i,
        FalkorValue::F64(f) => *f as i64,
        FalkorValue::String(s) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

fn val_to_f64(v: &FalkorValue) -> f64 {
    match v {
        FalkorValue::F64(f) => *f,
        FalkorValue::I64(i) => *i as f64,
        FalkorValue::String(s) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

// ---------------------------------------------------------------------------
// Analytics query methods on FalkorAnalytics
// ---------------------------------------------------------------------------

impl FalkorAnalytics {
    // -- Internal helper: execute a read-only query with params and collect rows --

    async fn ro_query_rows(
        &self,
        cypher: &str,
        params: HashMap<String, String>,
    ) -> OpenFangResult<Vec<Vec<FalkorValue>>> {
        let mut graph = self.graph.lock().await;
        let result = if params.is_empty() {
            graph
                .ro_query(cypher)
                .execute()
                .await
                .map_err(|e| OpenFangError::Memory(format!("RO query failed: {}", e)))?
        } else {
            graph
                .query(cypher)
                .with_params(&params)
                .execute()
                .await
                .map_err(|e| OpenFangError::Memory(format!("Parameterized query failed: {}", e)))?
        };
        let rows: Vec<Vec<FalkorValue>> = result.data.collect();
        Ok(rows)
    }

    // -- Public analytics API --

    /// Get an entity by its domain ID.
    pub async fn get_entity(&self, entity_id: &str) -> OpenFangResult<Option<GraphNode>> {
        let mut params = HashMap::new();
        params.insert("entity_id".into(), entity_id.into());
        let rows = self.ro_query_rows(cypher::ENTITY_BY_ID, params).await?;
        Ok(rows.first().map(|row| GraphNode {
            id: val_to_string(&row[0]),
            name: val_to_string(&row[1]),
            entity_type: val_to_string(&row[2]),
            created_at: val_to_string(&row[3]),
            updated_at: val_to_string(&row[4]),
        }))
    }

    /// Find all entities connected to a given entity.
    pub async fn entity_neighbors(&self, entity_id: &str) -> OpenFangResult<Vec<Neighbor>> {
        let mut params = HashMap::new();
        params.insert("entity_id".into(), entity_id.into());
        let rows = self
            .ro_query_rows(cypher::ENTITY_NEIGHBORS, params)
            .await?;
        let mut neighbors = Vec::with_capacity(rows.len());
        for row in &rows {
            neighbors.push(Neighbor {
                entity: GraphNode {
                    id: val_to_string(&row[1]),
                    name: val_to_string(&row[2]),
                    entity_type: val_to_string(&row[3]),
                    created_at: val_to_string(&row[4]),
                    updated_at: val_to_string(&row[5]),
                },
                relation_type: val_to_string(&row[6]),
                confidence: val_to_f64(&row[7]),
                direction: val_to_string(&row[8]),
            });
        }
        Ok(neighbors)
    }

    /// Find the shortest path between two entities.
    pub async fn shortest_path(
        &self,
        source_id: &str,
        target_id: &str,
    ) -> OpenFangResult<Option<ShortestPath>> {
        let mut params = HashMap::new();
        params.insert("source_id".into(), source_id.into());
        params.insert("target_id".into(), target_id.into());
        let rows = self.ro_query_rows(cypher::SHORTEST_PATH, params).await?;
        if rows.is_empty() {
            return Ok(None);
        }
        // The first row, first column is the path nodes array
        let path_nodes = match &rows[0][0] {
            FalkorValue::Array(nodes) => nodes,
            _ => return Ok(None),
        };
        let mut steps = Vec::new();
        for node_val in path_nodes {
            if let FalkorValue::Node(node) = node_val {
                let id = node
                    .properties
                    .get("id")
                    .map(val_to_string)
                    .unwrap_or_default();
                let name = node
                    .properties
                    .get("name")
                    .map(val_to_string)
                    .unwrap_or_default();
                let entity_type = node
                    .properties
                    .get("type")
                    .map(val_to_string)
                    .unwrap_or_default();
                steps.push(PathStep {
                    entity_id: id,
                    entity_name: name,
                    entity_type,
                });
            }
        }
        let length = if steps.is_empty() {
            0
        } else {
            steps.len() - 1
        };
        Ok(Some(ShortestPath { steps, length }))
    }

    /// Get entity type distribution (count per type).
    pub async fn entity_type_distribution(&self) -> OpenFangResult<Vec<TypeCount>> {
        let rows = self
            .ro_query_rows(cypher::ENTITY_TYPE_DISTRIBUTION, HashMap::new())
            .await?;
        Ok(rows
            .iter()
            .map(|row| TypeCount {
                label: val_to_string(&row[0]),
                count: val_to_i64(&row[1]),
            })
            .collect())
    }

    /// Get relation type distribution (count per type).
    pub async fn relation_type_distribution(&self) -> OpenFangResult<Vec<TypeCount>> {
        let rows = self
            .ro_query_rows(cypher::RELATION_TYPE_DISTRIBUTION, HashMap::new())
            .await?;
        Ok(rows
            .iter()
            .map(|row| TypeCount {
                label: val_to_string(&row[0]),
                count: val_to_i64(&row[1]),
            })
            .collect())
    }

    /// Get the top N most-connected entities (hubs).
    pub async fn top_hubs(&self, limit: usize) -> OpenFangResult<Vec<HubEntity>> {
        let mut params = HashMap::new();
        params.insert("limit".into(), limit.to_string());
        let rows = self.ro_query_rows(cypher::TOP_HUBS, params).await?;
        Ok(rows
            .iter()
            .map(|row| HubEntity {
                entity: GraphNode {
                    id: val_to_string(&row[0]),
                    name: val_to_string(&row[1]),
                    entity_type: val_to_string(&row[2]),
                    created_at: val_to_string(&row[3]),
                    updated_at: val_to_string(&row[4]),
                },
                connection_count: val_to_i64(&row[5]),
            })
            .collect())
    }

    /// Search entities by name (case-insensitive substring match).
    pub async fn search_entities(
        &self,
        query: &str,
        limit: usize,
    ) -> OpenFangResult<Vec<GraphNode>> {
        let mut params = HashMap::new();
        params.insert("query".into(), query.into());
        params.insert("limit".into(), limit.to_string());
        let rows = self.ro_query_rows(cypher::ENTITY_SEARCH, params).await?;
        Ok(rows
            .iter()
            .map(|row| GraphNode {
                id: val_to_string(&row[0]),
                name: val_to_string(&row[1]),
                entity_type: val_to_string(&row[2]),
                created_at: val_to_string(&row[3]),
                updated_at: val_to_string(&row[4]),
            })
            .collect())
    }

    /// Get memories for a specific agent, ordered by creation time (newest first).
    pub async fn agent_memories(
        &self,
        agent_id: &str,
        limit: usize,
    ) -> OpenFangResult<Vec<GraphMemory>> {
        let mut params = HashMap::new();
        params.insert("agent_id".into(), agent_id.into());
        params.insert("limit".into(), limit.to_string());
        let rows = self.ro_query_rows(cypher::AGENT_MEMORIES, params).await?;
        Ok(rows
            .iter()
            .map(|row| GraphMemory {
                id: val_to_string(&row[0]),
                content: val_to_string(&row[1]),
                agent_id: val_to_string(&row[2]),
                source: val_to_string(&row[3]),
                confidence: val_to_f64(&row[4]),
                created_at: val_to_string(&row[5]),
                accessed_at: val_to_string(&row[6]),
                access_count: val_to_i64(&row[7]),
                scope: val_to_string(&row[8]),
            })
            .collect())
    }

    /// Get per-agent memory statistics.
    pub async fn agent_memory_stats(&self) -> OpenFangResult<Vec<AgentMemoryStats>> {
        let rows = self
            .ro_query_rows(cypher::AGENT_MEMORY_STATS, HashMap::new())
            .await?;
        Ok(rows
            .iter()
            .map(|row| AgentMemoryStats {
                agent_id: val_to_string(&row[0]),
                memory_count: val_to_i64(&row[1]),
                avg_confidence: val_to_f64(&row[2]),
                oldest: val_to_string(&row[3]),
                newest: val_to_string(&row[4]),
            })
            .collect())
    }

    /// Get high-confidence relations above a threshold.
    pub async fn high_confidence_relations(
        &self,
        threshold: f64,
        limit: usize,
    ) -> OpenFangResult<Vec<GraphEdge>> {
        let mut params = HashMap::new();
        params.insert("threshold".into(), threshold.to_string());
        params.insert("limit".into(), limit.to_string());
        let rows = self
            .ro_query_rows(cypher::HIGH_CONFIDENCE_RELATIONS, params)
            .await?;
        Ok(rows
            .iter()
            .map(|row| GraphEdge {
                source_id: val_to_string(&row[0]),
                target_id: val_to_string(&row[5]),
                relation_type: val_to_string(&row[2]),
                confidence: val_to_f64(&row[3]),
                created_at: val_to_string(&row[4]),
            })
            .collect())
    }

    /// Get comprehensive graph statistics.
    pub async fn graph_stats(&self) -> OpenFangResult<GraphStats> {
        // Run the three count queries
        let entity_rows = self
            .ro_query_rows(cypher::COUNT_ENTITIES, HashMap::new())
            .await?;
        let memory_rows = self
            .ro_query_rows(cypher::COUNT_MEMORIES, HashMap::new())
            .await?;
        let relation_rows = self
            .ro_query_rows(cypher::COUNT_RELATIONS, HashMap::new())
            .await?;

        let total_entities = entity_rows
            .first()
            .map(|r| val_to_i64(&r[0]))
            .unwrap_or(0);
        let total_memories = memory_rows
            .first()
            .map(|r| val_to_i64(&r[0]))
            .unwrap_or(0);
        let total_relations = relation_rows
            .first()
            .map(|r| val_to_i64(&r[0]))
            .unwrap_or(0);

        let entity_type_distribution = self.entity_type_distribution().await?;
        let relation_type_distribution = self.relation_type_distribution().await?;

        Ok(GraphStats {
            total_entities,
            total_memories,
            total_relations,
            entity_type_distribution,
            relation_type_distribution,
        })
    }
}
