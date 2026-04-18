//! PostgreSQL implementation of the knowledge graph store.

use crate::helpers;
use deadpool_postgres::Pool;
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::memory::{Entity, GraphMatch, GraphPattern, Relation};
use openfang_types::storage::KnowledgeBackend;

pub struct PgKnowledgeStore {
    pool: Pool,
}

impl PgKnowledgeStore {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    fn block_on_pg<F, T>(&self, f: F) -> OpenFangResult<T>
    where
        F: std::future::Future<Output = OpenFangResult<T>>,
    {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(f)
        })
    }
}

impl KnowledgeBackend for PgKnowledgeStore {
    fn add_entity(&self, entity: Entity) -> OpenFangResult<String> {
        let id = helpers::entity_id_or_generate(&entity.id);
        let entity_type_str = helpers::serialize_entity_type(&entity.entity_type)?;
        let props_json = serde_json::to_value(&entity.properties)
            .map_err(|e| OpenFangError::Serialization(e.to_string()))?;

        self.block_on_pg(async {
            let client = self.pool.get().await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            client
                .execute(
                    "INSERT INTO entities (id, entity_type, name, properties, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, NOW(), NOW())
                     ON CONFLICT (id) DO UPDATE SET name = $3, properties = $4, updated_at = NOW()",
                    &[&id, &entity_type_str, &entity.name, &props_json],
                )
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(id)
        })
    }

    fn add_relation(&self, relation: Relation) -> OpenFangResult<String> {
        let id = helpers::new_relation_id();
        let rel_type_str = helpers::serialize_relation_type(&relation.relation)?;
        let props_json = serde_json::to_value(&relation.properties)
            .map_err(|e| OpenFangError::Serialization(e.to_string()))?;

        self.block_on_pg(async {
            let client = self.pool.get().await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            client
                .execute(
                    "INSERT INTO relations (id, source_entity, relation_type, target_entity, properties, confidence, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6, NOW())",
                    &[&id, &relation.source, &rel_type_str, &relation.target, &props_json, &relation.confidence],
                )
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(id)
        })
    }

    fn query_graph(&self, pattern: GraphPattern) -> OpenFangResult<Vec<GraphMatch>> {
        self.block_on_pg(async {
            let client = self.pool.get().await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;

            let mut conditions = vec!["TRUE".to_string()];
            let mut params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = Vec::new();
            let mut idx = 1u32;

            if let Some(ref source) = pattern.source {
                conditions.push(format!("(s.id = ${idx} OR s.name = ${idx})"));
                params.push(Box::new(source.clone()));
                idx += 1;
            }
            if let Some(ref relation) = pattern.relation {
                let rel_str = helpers::serialize_relation_type(relation)?;
                conditions.push(format!("r.relation_type = ${idx}"));
                params.push(Box::new(rel_str));
                idx += 1;
            }
            if let Some(ref target) = pattern.target {
                conditions.push(format!("(t.id = ${idx} OR t.name = ${idx})"));
                params.push(Box::new(target.clone()));
                idx += 1;
            }
            let _ = idx;

            let where_clause = conditions.join(" AND ");
            let sql = format!(
                "SELECT s.id, s.entity_type, s.name, s.properties, s.created_at, s.updated_at,
                        r.source_entity, r.relation_type, r.target_entity, r.properties, r.confidence, r.created_at,
                        t.id, t.entity_type, t.name, t.properties, t.created_at, t.updated_at
                 FROM relations r
                 JOIN entities s ON r.source_entity = s.id
                 JOIN entities t ON r.target_entity = t.id
                 WHERE {where_clause}
                 LIMIT 100"
            );

            let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
                params.iter().map(|b| b.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync)).collect();
            let rows = client.query(&sql, &param_refs).await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;

            let mut matches = Vec::new();
            for row in &rows {
                let parse_entity = |id_idx: usize| -> Entity {
                    let id: String = row.get(id_idx);
                    let etype_str: String = row.get(id_idx + 1);
                    let name: String = row.get(id_idx + 2);
                    let props_json: serde_json::Value = row.get(id_idx + 3);
                    let created_at: chrono::DateTime<chrono::Utc> = row.get(id_idx + 4);
                    let updated_at: chrono::DateTime<chrono::Utc> = row.get(id_idx + 5);
                    let props_str = props_json.to_string();
                    helpers::build_entity(
                        &id, &etype_str, &name, &props_str,
                        &created_at.to_rfc3339(), &updated_at.to_rfc3339(),
                    )
                };

                let source = parse_entity(0);
                let target = parse_entity(12);

                let r_source: String = row.get(6);
                let r_type_str: String = row.get(7);
                let r_target: String = row.get(8);
                let r_props_json: serde_json::Value = row.get(9);
                let r_confidence: f32 = row.get(10);
                let r_created: chrono::DateTime<chrono::Utc> = row.get(11);
                let r_props_str = r_props_json.to_string();

                let relation = helpers::build_relation(
                    &r_source, &r_type_str, &r_target, &r_props_str,
                    r_confidence as f64, &r_created.to_rfc3339(),
                );

                matches.push(GraphMatch { source, relation, target });
            }
            Ok(matches)
        })
    }
}
