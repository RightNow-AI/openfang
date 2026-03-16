//! MongoDB knowledge graph store for entities and relations.

use bson::doc;
use chrono::Utc;
use futures::TryStreamExt;
use mongodb::Collection;
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::memory::{
    Entity, EntityType, GraphMatch, GraphPattern, Relation, RelationType,
};
use std::collections::HashMap;
use uuid::Uuid;

/// Knowledge graph store backed by MongoDB.
#[derive(Clone)]
pub struct MongoKnowledgeStore {
    entities: Collection<bson::Document>,
    relations: Collection<bson::Document>,
}

impl MongoKnowledgeStore {
    pub fn new(db: mongodb::Database) -> Self {
        Self {
            entities: db.collection("entities"),
            relations: db.collection("relations"),
        }
    }

    /// Add an entity to the knowledge graph.
    pub async fn add_entity(&self, entity: Entity) -> OpenFangResult<String> {
        let id = if entity.id.is_empty() {
            Uuid::new_v4().to_string()
        } else {
            entity.id.clone()
        };
        let entity_type_str = serde_json::to_string(&entity.entity_type)
            .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
        let props_str = serde_json::to_string(&entity.properties)
            .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
        let now = bson::DateTime::from_chrono(Utc::now());

        let filter = doc! { "_id": &id };
        let update = doc! {
            "$set": {
                "entity_type": &entity_type_str,
                "name": &entity.name,
                "properties": &props_str,
                "updated_at": now,
            },
            "$setOnInsert": {
                "created_at": now,
            },
        };
        self.entities
            .update_one(filter, update)
            .upsert(true)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(id)
    }

    /// Add a relation between two entities.
    pub async fn add_relation(&self, relation: Relation) -> OpenFangResult<String> {
        let id = Uuid::new_v4().to_string();
        let rel_type_str = serde_json::to_string(&relation.relation)
            .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
        let props_str = serde_json::to_string(&relation.properties)
            .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
        let now = bson::DateTime::from_chrono(Utc::now());

        let doc = doc! {
            "_id": &id,
            "source_entity": &relation.source,
            "relation_type": &rel_type_str,
            "target_entity": &relation.target,
            "properties": &props_str,
            "confidence": relation.confidence as f64,
            "created_at": now,
        };
        self.relations
            .insert_one(doc)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(id)
    }

    /// Query the knowledge graph with a pattern.
    pub async fn query_graph(&self, pattern: GraphPattern) -> OpenFangResult<Vec<GraphMatch>> {
        // Build filter on relations
        let mut filter = doc! {};
        if let Some(ref source) = pattern.source {
            // Need to find entities matching by id or name first
            let source_ids = self.resolve_entity_ids(source).await?;
            if source_ids.is_empty() {
                return Ok(Vec::new());
            }
            filter.insert("source_entity", doc! { "$in": &source_ids });
        }
        if let Some(ref relation) = pattern.relation {
            let rel_str = serde_json::to_string(relation)
                .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
            filter.insert("relation_type", rel_str);
        }
        if let Some(ref target) = pattern.target {
            let target_ids = self.resolve_entity_ids(target).await?;
            if target_ids.is_empty() {
                return Ok(Vec::new());
            }
            filter.insert("target_entity", doc! { "$in": &target_ids });
        }

        let opts = mongodb::options::FindOptions::builder()
            .limit(100)
            .build();
        let mut cursor = self
            .relations
            .find(filter)
            .with_options(opts)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut matches = Vec::new();
        while let Some(rel_doc) = cursor
            .try_next()
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?
        {
            let source_id = rel_doc.get_str("source_entity").unwrap_or_default();
            let target_id = rel_doc.get_str("target_entity").unwrap_or_default();

            let source_entity = self.load_entity(source_id).await?;
            let target_entity = self.load_entity(target_id).await?;

            if let (Some(src), Some(tgt)) = (source_entity, target_entity) {
                let relation = parse_relation_doc(&rel_doc);
                matches.push(GraphMatch {
                    source: src,
                    relation,
                    target: tgt,
                });
            }
        }

        Ok(matches)
    }

    /// Resolve an entity identifier (could be id or name) to matching entity IDs.
    async fn resolve_entity_ids(&self, id_or_name: &str) -> OpenFangResult<Vec<String>> {
        let filter = doc! {
            "$or": [
                { "_id": id_or_name },
                { "name": id_or_name },
            ]
        };
        let opts = mongodb::options::FindOptions::builder()
            .projection(doc! { "_id": 1 })
            .build();
        let mut cursor = self
            .entities
            .find(filter)
            .with_options(opts)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut ids = Vec::new();
        while let Some(d) = cursor
            .try_next()
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?
        {
            if let Ok(id) = d.get_str("_id") {
                ids.push(id.to_string());
            }
        }
        Ok(ids)
    }

    /// Load a single entity by ID.
    async fn load_entity(&self, id: &str) -> OpenFangResult<Option<Entity>> {
        let doc = self
            .entities
            .find_one(doc! { "_id": id })
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(doc.as_ref().map(parse_entity_doc))
    }
}

fn parse_entity_doc(d: &bson::Document) -> Entity {
    let id = d.get_str("_id").unwrap_or_default().to_string();
    let etype_str = d.get_str("entity_type").unwrap_or("\"unknown\"");
    let entity_type: EntityType =
        serde_json::from_str(etype_str).unwrap_or(EntityType::Custom("unknown".to_string()));
    let name = d.get_str("name").unwrap_or_default().to_string();
    let props_str = d.get_str("properties").unwrap_or("{}");
    let properties: HashMap<String, serde_json::Value> =
        serde_json::from_str(props_str).unwrap_or_default();
    let created_at = d
        .get_datetime("created_at")
        .ok()
        .map(|dt| dt.to_chrono())
        .unwrap_or_else(Utc::now);
    let updated_at = d
        .get_datetime("updated_at")
        .ok()
        .map(|dt| dt.to_chrono())
        .unwrap_or_else(Utc::now);

    Entity {
        id,
        entity_type,
        name,
        properties,
        created_at,
        updated_at,
    }
}

fn parse_relation_doc(d: &bson::Document) -> Relation {
    let source = d.get_str("source_entity").unwrap_or_default().to_string();
    let rtype_str = d.get_str("relation_type").unwrap_or("\"RelatedTo\"");
    let relation: RelationType = serde_json::from_str(rtype_str).unwrap_or(RelationType::RelatedTo);
    let target = d.get_str("target_entity").unwrap_or_default().to_string();
    let props_str = d.get_str("properties").unwrap_or("{}");
    let properties: HashMap<String, serde_json::Value> =
        serde_json::from_str(props_str).unwrap_or_default();
    let confidence = d.get_f64("confidence").unwrap_or(1.0) as f32;
    let created_at = d
        .get_datetime("created_at")
        .ok()
        .map(|dt| dt.to_chrono())
        .unwrap_or_else(Utc::now);

    Relation {
        source,
        relation,
        target,
        properties,
        confidence,
        created_at,
    }
}
