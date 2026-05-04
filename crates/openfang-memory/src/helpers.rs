//! Shared serialization, parsing, and construction helpers.
//!
//! These functions eliminate boilerplate duplicated across SQLite, PostgreSQL,
//! and Qdrant backend implementations.

use openfang_types::agent::{AgentId, SessionId};
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::memory::{Entity, EntityType, MemoryId, MemorySource, Relation, RelationType};
use openfang_types::message::Message;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// UUID parsing
// ---------------------------------------------------------------------------

/// Parse a string into an [`AgentId`], returning a descriptive memory error.
pub fn parse_agent_id(s: &str) -> OpenFangResult<AgentId> {
    uuid::Uuid::parse_str(s)
        .map(AgentId)
        .map_err(|e| OpenFangError::Memory(format!("invalid agent UUID: {e}")))
}

/// Parse a string into a [`SessionId`], returning a descriptive memory error.
pub fn parse_session_id(s: &str) -> OpenFangResult<SessionId> {
    uuid::Uuid::parse_str(s)
        .map(SessionId)
        .map_err(|e| OpenFangError::Memory(format!("invalid session UUID: {e}")))
}

/// Parse a string into a [`MemoryId`], returning a descriptive memory error.
pub fn parse_memory_id(s: &str) -> OpenFangResult<MemoryId> {
    uuid::Uuid::parse_str(s)
        .map(MemoryId)
        .map_err(|e| OpenFangError::Memory(format!("invalid memory UUID: {e}")))
}

// ---------------------------------------------------------------------------
// Message msgpack ser/de
// ---------------------------------------------------------------------------

/// Serialize a message slice to msgpack (compact encoding).
///
/// Used by `save_canonical` and similar paths where field-name stability is
/// not required.
pub fn serialize_messages(messages: &[Message]) -> OpenFangResult<Vec<u8>> {
    rmp_serde::to_vec(messages).map_err(|e| OpenFangError::Serialization(e.to_string()))
}

/// Serialize a message slice to msgpack with named fields.
///
/// Preferred for session persistence so that new fields with `#[serde(default)]`
/// are handled gracefully across schema changes.
pub fn serialize_messages_named(messages: &[Message]) -> OpenFangResult<Vec<u8>> {
    rmp_serde::to_vec_named(messages).map_err(|e| OpenFangError::Serialization(e.to_string()))
}

/// Deserialize messages from a msgpack blob, returning an empty vec on failure.
pub fn deserialize_messages_lossy(blob: &[u8]) -> Vec<Message> {
    rmp_serde::from_slice(blob).unwrap_or_default()
}

/// Deserialize messages from a msgpack blob, propagating errors.
pub fn deserialize_messages(blob: &[u8]) -> OpenFangResult<Vec<Message>> {
    rmp_serde::from_slice(blob).map_err(|e| OpenFangError::Serialization(e.to_string()))
}

// ---------------------------------------------------------------------------
// JSON ser/de for domain types
// ---------------------------------------------------------------------------

/// Serialize a [`MemorySource`] to its JSON string representation.
pub fn serialize_source(source: &MemorySource) -> OpenFangResult<String> {
    serde_json::to_string(source).map_err(|e| OpenFangError::Serialization(e.to_string()))
}

/// Deserialize a [`MemorySource`] from JSON, falling back to [`MemorySource::System`].
pub fn deserialize_source(s: &str) -> MemorySource {
    serde_json::from_str(s).unwrap_or(MemorySource::System)
}

/// Serialize an [`EntityType`] to its JSON string representation.
pub fn serialize_entity_type(et: &EntityType) -> OpenFangResult<String> {
    serde_json::to_string(et).map_err(|e| OpenFangError::Serialization(e.to_string()))
}

/// Deserialize an [`EntityType`] from JSON, falling back to `Custom("unknown")`.
pub fn deserialize_entity_type(s: &str) -> EntityType {
    serde_json::from_str(s).unwrap_or(EntityType::Custom("unknown".to_string()))
}

/// Serialize a [`RelationType`] to its JSON string representation.
pub fn serialize_relation_type(rt: &RelationType) -> OpenFangResult<String> {
    serde_json::to_string(rt).map_err(|e| OpenFangError::Serialization(e.to_string()))
}

/// Deserialize a [`RelationType`] from JSON, falling back to [`RelationType::RelatedTo`].
pub fn deserialize_relation_type(s: &str) -> RelationType {
    serde_json::from_str(s).unwrap_or(RelationType::RelatedTo)
}

/// Serialize a properties map to a JSON string.
pub fn serialize_properties(props: &HashMap<String, serde_json::Value>) -> OpenFangResult<String> {
    serde_json::to_string(props).map_err(|e| OpenFangError::Serialization(e.to_string()))
}

/// Deserialize a properties map from a JSON string, returning an empty map on failure.
pub fn deserialize_properties(s: &str) -> HashMap<String, serde_json::Value> {
    serde_json::from_str(s).unwrap_or_default()
}

/// Serialize metadata to a JSON string (alias for [`serialize_properties`]).
pub fn serialize_metadata(meta: &HashMap<String, serde_json::Value>) -> OpenFangResult<String> {
    serialize_properties(meta)
}

/// Deserialize metadata from a JSON string (alias for [`deserialize_properties`]).
pub fn deserialize_metadata(s: &str) -> HashMap<String, serde_json::Value> {
    deserialize_properties(s)
}

// ---------------------------------------------------------------------------
// Agent manifest msgpack
// ---------------------------------------------------------------------------

/// Serialize an [`AgentManifest`] to msgpack with named fields.
///
/// Named-field encoding ensures new fields with `#[serde(default)]` are
/// handled gracefully when the struct evolves between versions.
pub fn serialize_manifest(
    manifest: &openfang_types::agent::AgentManifest,
) -> OpenFangResult<Vec<u8>> {
    rmp_serde::to_vec_named(manifest).map_err(|e| OpenFangError::Serialization(e.to_string()))
}

/// Deserialize an [`AgentManifest`] from a msgpack blob.
pub fn deserialize_manifest(
    blob: &[u8],
) -> OpenFangResult<openfang_types::agent::AgentManifest> {
    rmp_serde::from_slice(blob).map_err(|e| OpenFangError::Serialization(e.to_string()))
}

// ---------------------------------------------------------------------------
// Timestamp helpers
// ---------------------------------------------------------------------------

/// Parse an RFC 3339 timestamp, falling back to `Utc::now()` on failure.
pub fn parse_rfc3339_or_now(s: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now())
}

// ---------------------------------------------------------------------------
// Knowledge graph builders
// ---------------------------------------------------------------------------

/// Build an [`Entity`] from raw database column values.
pub fn build_entity(
    id: &str,
    etype_str: &str,
    name: &str,
    props_str: &str,
    created_str: &str,
    updated_str: &str,
) -> Entity {
    Entity {
        id: id.to_string(),
        entity_type: deserialize_entity_type(etype_str),
        name: name.to_string(),
        properties: deserialize_properties(props_str),
        created_at: parse_rfc3339_or_now(created_str),
        updated_at: parse_rfc3339_or_now(updated_str),
    }
}

/// Build a [`Relation`] from raw database column values.
pub fn build_relation(
    source: &str,
    rtype_str: &str,
    target: &str,
    props_str: &str,
    confidence: f64,
    created_str: &str,
) -> Relation {
    Relation {
        source: source.to_string(),
        relation: deserialize_relation_type(rtype_str),
        target: target.to_string(),
        properties: deserialize_properties(props_str),
        confidence: confidence as f32,
        created_at: parse_rfc3339_or_now(created_str),
    }
}

// ---------------------------------------------------------------------------
// ID generation
// ---------------------------------------------------------------------------

/// Return `existing` if non-empty, otherwise generate a new v4 UUID string.
pub fn entity_id_or_generate(existing: &str) -> String {
    if existing.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        existing.to_string()
    }
}

/// Generate a new v4 UUID string for a relation row.
pub fn new_relation_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
