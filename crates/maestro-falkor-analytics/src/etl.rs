//! ETL (Extract, Transform, Load) module for migrating memory data from SurrealDB to FalkorDB.
//!
//! This module provides:
//! - `MemoryExtractor`: Extracts raw JSON data from a SurrealMemorySubstrate
//! - `MemoryTransformer`: Transforms raw SurrealDB export data into openfang-types structs
//! - `MemoryLoader`: Loads entities, relations, and memory fragments into FalkorDB
//! - `run_etl`: Main function that orchestrates the full ETL pipeline

use crate::FalkorAnalytics;
use anyhow::Result;
use openfang_types::memory::{
    Entity, EntityType, ExportFormat, MemoryFragment, Relation, RelationType,
};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct SurrealExportData {
    #[serde(rename = "memories")]
    memories: Vec<serde_json::Value>,
    #[serde(rename = "entities")]
    entities: Vec<serde_json::Value>,
    #[serde(rename = "relations")]
    relations: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct SurrealEntity {
    #[serde(rename = "id")]
    id: String,
    #[serde(rename = "entity_type")]
    entity_type: String,
    #[serde(rename = "name")]
    name: String,
    #[serde(rename = "properties")]
    properties: Option<HashMap<String, serde_json::Value>>,
    #[serde(rename = "created_at")]
    created_at: Option<String>,
    #[serde(rename = "updated_at")]
    updated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SurrealRelation {
    #[serde(rename = "source")]
    source: String,
    #[serde(rename = "relation")]
    relation: String,
    #[serde(rename = "target")]
    target: String,
    #[serde(rename = "properties")]
    properties: Option<HashMap<String, serde_json::Value>>,
    #[serde(rename = "confidence")]
    confidence: Option<f32>,
    #[serde(rename = "created_at")]
    created_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SurrealMemory {
    #[serde(rename = "id")]
    id: String,
    #[serde(rename = "agent_id")]
    agent_id: String,
    #[serde(rename = "content")]
    content: String,
    #[serde(rename = "metadata")]
    metadata: Option<HashMap<String, serde_json::Value>>,
    #[serde(rename = "source")]
    source: Option<String>,
    #[serde(rename = "confidence")]
    confidence: Option<f32>,
    #[serde(rename = "created_at")]
    created_at: Option<String>,
    #[serde(rename = "accessed_at")]
    accessed_at: Option<String>,
    #[serde(rename = "access_count")]
    access_count: Option<u64>,
    #[serde(rename = "scope")]
    scope: Option<String>,
}

pub struct MemoryExtractor;

impl MemoryExtractor {
    pub async fn extract(memory: &dyn openfang_types::memory::Memory) -> Result<SurrealExportData> {
        let export_data = memory
            .export(ExportFormat::Json)
            .await
            .map_err(|e| anyhow::anyhow!("Export failed: {}", e))?;

        let parsed: SurrealExportData = serde_json::from_slice(&export_data)
            .map_err(|e| anyhow::anyhow!("Failed to parse export data: {}", e))?;

        Ok(parsed)
    }
}

pub struct MemoryTransformer;

impl MemoryTransformer {
    pub fn transform_entities(entities: Vec<serde_json::Value>) -> Result<Vec<Entity>> {
        let mut result = Vec::new();

        for value in entities {
            let surreal_entity: SurrealEntity = serde_json::from_value(value)
                .map_err(|e| anyhow::anyhow!("Failed to parse entity: {}", e))?;

            let entity_type = match surreal_entity.entity_type.as_str() {
                "Person" => EntityType::Person,
                "Organization" => EntityType::Organization,
                "Project" => EntityType::Project,
                "Concept" => EntityType::Concept,
                "Event" => EntityType::Event,
                "Location" => EntityType::Location,
                "Document" => EntityType::Document,
                "Tool" => EntityType::Tool,
                other => EntityType::Custom(other.to_string()),
            };

            let created_at = surreal_entity
                .created_at
                .as_ref()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(chrono::Utc::now);

            let updated_at = surreal_entity
                .updated_at
                .as_ref()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(chrono::Utc::now);

            result.push(Entity {
                id: surreal_entity.id,
                entity_type,
                name: surreal_entity.name,
                properties: surreal_entity.properties.unwrap_or_default(),
                created_at,
                updated_at,
            });
        }

        Ok(result)
    }

    pub fn transform_relations(relations: Vec<serde_json::Value>) -> Result<Vec<Relation>> {
        let mut result = Vec::new();

        for value in relations {
            let surreal_relation: SurrealRelation = serde_json::from_value(value)
                .map_err(|e| anyhow::anyhow!("Failed to parse relation: {}", e))?;

            let relation_type = match surreal_relation.relation.as_str() {
                "WorksAt" => RelationType::WorksAt,
                "KnowsAbout" => RelationType::KnowsAbout,
                "RelatedTo" => RelationType::RelatedTo,
                "DependsOn" => RelationType::DependsOn,
                "OwnedBy" => RelationType::OwnedBy,
                "CreatedBy" => RelationType::CreatedBy,
                "LocatedIn" => RelationType::LocatedIn,
                "PartOf" => RelationType::PartOf,
                "Uses" => RelationType::Uses,
                "Produces" => RelationType::Produces,
                other => RelationType::Custom(other.to_string()),
            };

            let created_at = surreal_relation
                .created_at
                .as_ref()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(chrono::Utc::now);

            result.push(Relation {
                source: surreal_relation.source,
                relation: relation_type,
                target: surreal_relation.target,
                properties: surreal_relation.properties.unwrap_or_default(),
                confidence: surreal_relation.confidence.unwrap_or(1.0),
                created_at,
            });
        }

        Ok(result)
    }

    pub fn transform_memories(memories: Vec<serde_json::Value>) -> Result<Vec<MemoryFragment>> {
        let mut result = Vec::new();

        for value in memories {
            let surreal_memory: SurrealMemory = serde_json::from_value(value)
                .map_err(|e| anyhow::anyhow!("Failed to parse memory: {}", e))?;

            let source = match surreal_memory.source.as_deref() {
                Some("conversation") => openfang_types::memory::MemorySource::Conversation,
                Some("document") => openfang_types::memory::MemorySource::Document,
                Some("observation") => openfang_types::memory::MemorySource::Observation,
                Some("inference") => openfang_types::memory::MemorySource::Inference,
                Some("user_provided") => openfang_types::memory::MemorySource::UserProvided,
                Some("system") => openfang_types::memory::MemorySource::System,
                _ => openfang_types::memory::MemorySource::Observation,
            };

            let created_at = surreal_memory
                .created_at
                .as_ref()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(chrono::Utc::now);

            let accessed_at = surreal_memory
                .accessed_at
                .as_ref()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(chrono::Utc::now);

            result.push(MemoryFragment {
                id: openfang_types::memory::MemoryId(
                    surreal_memory
                        .id
                        .parse()
                        .unwrap_or_else(|_| uuid::Uuid::new_v4()),
                ),
                agent_id: openfang_types::agent::AgentId(
                    surreal_memory
                        .agent_id
                        .parse()
                        .unwrap_or_else(|_| uuid::Uuid::new_v4()),
                ),
                content: surreal_memory.content,
                embedding: None,
                metadata: surreal_memory.metadata.unwrap_or_default(),
                source,
                confidence: surreal_memory.confidence.unwrap_or(1.0),
                created_at,
                accessed_at,
                access_count: surreal_memory.access_count.unwrap_or(0),
                scope: surreal_memory
                    .scope
                    .unwrap_or_else(|| "default".to_string()),
            });
        }

        Ok(result)
    }
}

pub struct MemoryLoader;

impl MemoryLoader {
    pub async fn load_entities(analytics: &FalkorAnalytics, entities: &[Entity]) -> Result<usize> {
        let mut loaded = 0;

        for entity in entities {
            let label = format!("Entity:{}", Self::entity_type_label(&entity.entity_type));

            let mut params: HashMap<String, String> = HashMap::new();
            params.insert("id".to_string(), entity.id.clone());
            params.insert("name".to_string(), entity.name.clone());
            params.insert("created_at".to_string(), entity.created_at.to_rfc3339());
            params.insert("updated_at".to_string(), entity.updated_at.to_rfc3339());

            let props_json = serde_json::to_string(&entity.properties)?;
            params.insert("props".to_string(), props_json);

            let cypher = format!(
                "MERGE (e:{} {{id: $id}}) SET e.name = $name, e.created_at = $created_at, e.updated_at = $updated_at, e += $props",
                label
            );

            analytics.execute_with_params(&cypher, params).await?;
            loaded += 1;
        }

        Ok(loaded)
    }

    pub async fn load_relations(
        analytics: &FalkorAnalytics,
        relations: &[Relation],
    ) -> Result<usize> {
        let mut loaded = 0;

        for relation in relations {
            let rel_type = Self::relation_type_label(&relation.relation);

            let mut params: HashMap<String, String> = HashMap::new();
            params.insert("source".to_string(), relation.source.clone());
            params.insert("target".to_string(), relation.target.clone());
            params.insert("confidence".to_string(), relation.confidence.to_string());
            params.insert("created_at".to_string(), relation.created_at.to_rfc3339());

            let props_json = serde_json::to_string(&relation.properties)?;
            params.insert("props".to_string(), props_json);

            let cypher = format!(
                "MATCH (a {{id: $source}}), (b {{id: $target}}) MERGE (a)-[r:{}]->(b) SET r.confidence = $confidence, r.created_at = $created_at, r += $props",
                rel_type
            );

            analytics.execute_with_params(&cypher, params).await?;
            loaded += 1;
        }

        Ok(loaded)
    }

    pub async fn load_memories(
        analytics: &FalkorAnalytics,
        memories: &[MemoryFragment],
    ) -> Result<usize> {
        let mut loaded = 0;

        for memory in memories {
            let mut params: HashMap<String, String> = HashMap::new();
            params.insert("id".to_string(), memory.id.to_string());
            params.insert("content".to_string(), memory.content.clone());
            params.insert("agent_id".to_string(), memory.agent_id.to_string());
            params.insert("source".to_string(), serde_json::to_string(&memory.source)?);
            params.insert("confidence".to_string(), memory.confidence.to_string());
            params.insert("created_at".to_string(), memory.created_at.to_rfc3339());
            params.insert("accessed_at".to_string(), memory.accessed_at.to_rfc3339());
            params.insert("access_count".to_string(), memory.access_count.to_string());
            params.insert("scope".to_string(), memory.scope.clone());

            let metadata_json = serde_json::to_string(&memory.metadata)?;
            params.insert("metadata".to_string(), metadata_json);

            let cypher = "MERGE (m:Memory {id: $id}) SET m.content = $content, m.agent_id = $agent_id, m.source = $source, m.confidence = $confidence, m.created_at = $created_at, m.accessed_at = $accessed_at, m.access_count = $access_count, m.scope = $scope, m += $metadata";

            analytics.execute_with_params(cypher, params).await?;
            loaded += 1;
        }

        Ok(loaded)
    }

    fn entity_type_label(entity_type: &EntityType) -> String {
        match entity_type {
            EntityType::Person => "Person".to_string(),
            EntityType::Organization => "Organization".to_string(),
            EntityType::Project => "Project".to_string(),
            EntityType::Concept => "Concept".to_string(),
            EntityType::Event => "Event".to_string(),
            EntityType::Location => "Location".to_string(),
            EntityType::Document => "Document".to_string(),
            EntityType::Tool => "Tool".to_string(),
            EntityType::Custom(s) => s.clone(),
        }
    }

    fn relation_type_label(relation_type: &RelationType) -> String {
        match relation_type {
            RelationType::WorksAt => "WORKS_AT".to_string(),
            RelationType::KnowsAbout => "KNOWS_ABOUT".to_string(),
            RelationType::RelatedTo => "RELATED_TO".to_string(),
            RelationType::DependsOn => "DEPENDS_ON".to_string(),
            RelationType::OwnedBy => "OWNED_BY".to_string(),
            RelationType::CreatedBy => "CREATED_BY".to_string(),
            RelationType::LocatedIn => "LOCATED_IN".to_string(),
            RelationType::PartOf => "PART_OF".to_string(),
            RelationType::Uses => "USES".to_string(),
            RelationType::Produces => "PRODUCES".to_string(),
            RelationType::Custom(s) => s.to_uppercase(),
        }
    }
}

pub async fn run_etl(
    memory: &dyn openfang_types::memory::Memory,
    analytics: &FalkorAnalytics,
) -> Result<EtlReport> {
    let export_data = MemoryExtractor::extract(memory).await?;

    let entities = MemoryTransformer::transform_entities(export_data.entities)?;
    let relations = MemoryTransformer::transform_relations(export_data.relations)?;
    let memories = MemoryTransformer::transform_memories(export_data.memories)?;

    let entities_loaded = MemoryLoader::load_entities(analytics, &entities).await?;
    let relations_loaded = MemoryLoader::load_relations(analytics, &relations).await?;
    let memories_loaded = MemoryLoader::load_memories(analytics, &memories).await?;

    Ok(EtlReport {
        entities_extracted: entities.len() as u64,
        entities_loaded: entities_loaded as u64,
        relations_extracted: relations.len() as u64,
        relations_loaded: relations_loaded as u64,
        memories_extracted: memories.len() as u64,
        memories_loaded: memories_loaded as u64,
    })
}

#[derive(Debug)]
pub struct EtlReport {
    pub entities_extracted: u64,
    pub entities_loaded: u64,
    pub relations_extracted: u64,
    pub relations_loaded: u64,
    pub memories_extracted: u64,
    pub memories_loaded: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_type_label() {
        assert_eq!(
            MemoryLoader::entity_type_label(&EntityType::Person),
            "Person"
        );
        assert_eq!(
            MemoryLoader::entity_type_label(&EntityType::Organization),
            "Organization"
        );
        assert_eq!(
            MemoryLoader::entity_type_label(&EntityType::Custom("CustomType".to_string())),
            "CustomType"
        );
    }

    #[test]
    fn test_relation_type_label() {
        assert_eq!(
            MemoryLoader::relation_type_label(&RelationType::WorksAt),
            "WORKS_AT"
        );
        assert_eq!(
            MemoryLoader::relation_type_label(&RelationType::KnowsAbout),
            "KNOWS_ABOUT"
        );
        assert_eq!(
            MemoryLoader::relation_type_label(&RelationType::Custom("CustomRel".to_string())),
            "CUSTOMREL"
        );
    }
}
