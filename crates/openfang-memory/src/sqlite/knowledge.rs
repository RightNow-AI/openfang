//! SQLite backend for the knowledge graph.
//!
//! Stores entities and relations with support for graph pattern queries.

use crate::helpers;
use chrono::Utc;
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::memory::{Entity, GraphMatch, GraphPattern, Relation};
use openfang_types::storage::KnowledgeBackend;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

/// Knowledge graph store backed by SQLite.
#[derive(Clone)]
pub struct KnowledgeStore {
    conn: Arc<Mutex<Connection>>,
}

impl KnowledgeStore {
    /// Create a new knowledge store wrapping the given connection.
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    /// Add an entity to the knowledge graph.
    pub fn add_entity(&self, entity: Entity) -> OpenFangResult<String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let id = helpers::entity_id_or_generate(&entity.id);
        let entity_type_str = helpers::serialize_entity_type(&entity.entity_type)?;
        let props_str = helpers::serialize_properties(&entity.properties)?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO entities (id, entity_type, name, properties, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(id) DO UPDATE SET name = ?3, properties = ?4, updated_at = ?5",
            rusqlite::params![id, entity_type_str, entity.name, props_str, now],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(id)
    }

    /// Add a relation between two entities.
    pub fn add_relation(&self, relation: Relation) -> OpenFangResult<String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let id = helpers::new_relation_id();
        let rel_type_str = helpers::serialize_relation_type(&relation.relation)?;
        let props_str = helpers::serialize_properties(&relation.properties)?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO relations (id, source_entity, relation_type, target_entity, properties, confidence, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                id,
                relation.source,
                rel_type_str,
                relation.target,
                props_str,
                relation.confidence as f64,
                now,
            ],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(id)
    }

    /// Query the knowledge graph with a pattern.
    pub fn query_graph(&self, pattern: GraphPattern) -> OpenFangResult<Vec<GraphMatch>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;

        let mut sql = String::from(
            "SELECT
                s.id, s.entity_type, s.name, s.properties, s.created_at, s.updated_at,
                r.source_entity, r.relation_type, r.target_entity, r.properties, r.confidence, r.created_at,
                t.id, t.entity_type, t.name, t.properties, t.created_at, t.updated_at
             FROM relations r
             JOIN entities s ON r.source_entity = s.id
             JOIN entities t ON r.target_entity = t.id
             WHERE 1=1",
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut idx = 1;

        if let Some(ref source) = pattern.source {
            sql.push_str(&format!(" AND (s.id = ?{} OR s.name = ?{})", idx, idx + 1));
            params.push(Box::new(source.clone()));
            params.push(Box::new(source.clone()));
            idx += 2;
        }
        if let Some(ref relation) = pattern.relation {
            let rel_str = helpers::serialize_relation_type(relation)?;
            sql.push_str(&format!(" AND r.relation_type = ?{idx}"));
            params.push(Box::new(rel_str));
            idx += 1;
        }
        if let Some(ref target) = pattern.target {
            sql.push_str(&format!(" AND (t.id = ?{} OR t.name = ?{})", idx, idx + 1));
            params.push(Box::new(target.clone()));
            params.push(Box::new(target.clone()));
            idx += 2;
        }
        let _ = idx;

        sql.push_str(" LIMIT 100");

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let rows = stmt
            .query_map(param_refs.as_slice(), |row| {
                Ok(RawGraphRow {
                    s_id: row.get(0)?,
                    s_type: row.get(1)?,
                    s_name: row.get(2)?,
                    s_props: row.get(3)?,
                    s_created: row.get(4)?,
                    s_updated: row.get(5)?,
                    r_source: row.get(6)?,
                    r_type: row.get(7)?,
                    r_target: row.get(8)?,
                    r_props: row.get(9)?,
                    r_confidence: row.get(10)?,
                    r_created: row.get(11)?,
                    t_id: row.get(12)?,
                    t_type: row.get(13)?,
                    t_name: row.get(14)?,
                    t_props: row.get(15)?,
                    t_created: row.get(16)?,
                    t_updated: row.get(17)?,
                })
            })
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut matches = Vec::new();
        for row_result in rows {
            let r = row_result.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            matches.push(GraphMatch {
                source: helpers::build_entity(
                    &r.s_id,
                    &r.s_type,
                    &r.s_name,
                    &r.s_props,
                    &r.s_created,
                    &r.s_updated,
                ),
                relation: helpers::build_relation(
                    &r.r_source,
                    &r.r_type,
                    &r.r_target,
                    &r.r_props,
                    r.r_confidence,
                    &r.r_created,
                ),
                target: helpers::build_entity(
                    &r.t_id,
                    &r.t_type,
                    &r.t_name,
                    &r.t_props,
                    &r.t_created,
                    &r.t_updated,
                ),
            });
        }
        Ok(matches)
    }
}

impl KnowledgeBackend for KnowledgeStore {
    fn add_entity(&self, entity: Entity) -> OpenFangResult<String> {
        KnowledgeStore::add_entity(self, entity)
    }
    fn add_relation(&self, relation: Relation) -> OpenFangResult<String> {
        KnowledgeStore::add_relation(self, relation)
    }
    fn query_graph(&self, pattern: GraphPattern) -> OpenFangResult<Vec<GraphMatch>> {
        KnowledgeStore::query_graph(self, pattern)
    }
}

/// Raw row from a graph query.
struct RawGraphRow {
    s_id: String,
    s_type: String,
    s_name: String,
    s_props: String,
    s_created: String,
    s_updated: String,
    r_source: String,
    r_type: String,
    r_target: String,
    r_props: String,
    r_confidence: f64,
    r_created: String,
    t_id: String,
    t_type: String,
    t_name: String,
    t_props: String,
    t_created: String,
    t_updated: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::migration::run_migrations;
    use openfang_types::memory::{EntityType, RelationType};
    use std::collections::HashMap;

    fn setup() -> KnowledgeStore {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        KnowledgeStore::new(Arc::new(Mutex::new(conn)))
    }

    #[test]
    fn test_add_and_query_entity() {
        let store = setup();
        let id = store
            .add_entity(Entity {
                id: String::new(),
                entity_type: EntityType::Person,
                name: "Alice".to_string(),
                properties: HashMap::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .unwrap();
        assert!(!id.is_empty());
    }

    #[test]
    fn test_add_relation_and_query() {
        let store = setup();
        let alice_id = store
            .add_entity(Entity {
                id: "alice".to_string(),
                entity_type: EntityType::Person,
                name: "Alice".to_string(),
                properties: HashMap::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .unwrap();
        let company_id = store
            .add_entity(Entity {
                id: "acme".to_string(),
                entity_type: EntityType::Organization,
                name: "Acme Corp".to_string(),
                properties: HashMap::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .unwrap();
        store
            .add_relation(Relation {
                source: alice_id.clone(),
                relation: RelationType::WorksAt,
                target: company_id,
                properties: HashMap::new(),
                confidence: 0.95,
                created_at: Utc::now(),
            })
            .unwrap();

        let matches = store
            .query_graph(GraphPattern {
                source: Some(alice_id),
                relation: Some(RelationType::WorksAt),
                target: None,
                max_depth: 1,
            })
            .unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].target.name, "Acme Corp");
    }
}
