//! SQLite backend for semantic memory with vector embedding support.
//!
//! Phase 1: SQLite LIKE matching (fallback when no embeddings).
//! Phase 2: Vector cosine similarity search using stored embeddings.
//!
//! Embeddings are stored as BLOBs in the `embedding` column of the memories table.
//! When a query embedding is provided, recall uses cosine similarity ranking.
//! When no embeddings are available, falls back to LIKE matching.

use crate::helpers;
use chrono::Utc;
use openfang_types::agent::AgentId;
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::memory::{MemoryFilter, MemoryFragment, MemoryId, MemorySource};
use openfang_types::storage::SemanticBackend;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::debug;

/// Semantic store backed by SQLite with vector search via sqlite-vec.
#[derive(Clone)]
pub struct SemanticStore {
    conn: Arc<Mutex<Connection>>,
}

impl SemanticStore {
    /// Create a new semantic store wrapping the given connection.
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    /// Store a new memory fragment (without embedding).
    pub fn remember(
        &self,
        agent_id: AgentId,
        content: &str,
        source: MemorySource,
        scope: &str,
        metadata: HashMap<String, serde_json::Value>,
    ) -> OpenFangResult<MemoryId> {
        self.remember_with_embedding(agent_id, content, source, scope, metadata, None)
    }

    /// Store a new memory fragment with an optional embedding vector.
    pub fn remember_with_embedding(
        &self,
        agent_id: AgentId,
        content: &str,
        source: MemorySource,
        scope: &str,
        metadata: HashMap<String, serde_json::Value>,
        embedding: Option<&[f32]>,
    ) -> OpenFangResult<MemoryId> {
        self.remember_sqlite(agent_id, content, source, scope, metadata, embedding)
    }

    /// SQLite implementation of remember_with_embedding.
    fn remember_sqlite(
        &self,
        agent_id: AgentId,
        content: &str,
        source: MemorySource,
        scope: &str,
        metadata: HashMap<String, serde_json::Value>,
        embedding: Option<&[f32]>,
    ) -> OpenFangResult<MemoryId> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let id = MemoryId::new();
        let now = Utc::now().to_rfc3339();
        let source_str = helpers::serialize_source(&source)?;
        let meta_str = helpers::serialize_metadata(&metadata)?;
        let embedding_bytes: Option<Vec<u8>> = embedding.map(embedding_to_bytes);

        conn.execute(
            "INSERT INTO memories (id, agent_id, content, source, scope, confidence, metadata, created_at, accessed_at, access_count, deleted, embedding)
             VALUES (?1, ?2, ?3, ?4, ?5, 1.0, ?6, ?7, ?7, 0, 0, ?8)",
            rusqlite::params![
                id.0.to_string(),
                agent_id.0.to_string(),
                content,
                source_str,
                scope,
                meta_str,
                now,
                embedding_bytes,
            ],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        // Dual-write to sqlite-vec virtual table for indexed vector search
        if let Some(ref emb_bytes) = embedding_bytes {
            if let Some(emb) = embedding {
                let _ = Self::ensure_vec_table(&conn, emb.len());
                let _ = conn.execute(
                    "INSERT INTO memories_vec (memory_id, embedding) VALUES (?1, ?2)",
                    rusqlite::params![id.0.to_string(), emb_bytes],
                );
            }
        }

        Ok(id)
    }

    /// Ensure the sqlite-vec virtual table exists with the given dimensions.
    /// Called lazily on the first write with an embedding.
    fn ensure_vec_table(
        conn: &Connection,
        dims: usize,
    ) -> Result<(), OpenFangError> {
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='memories_vec'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !exists {
            conn.execute_batch(&format!(
                "CREATE VIRTUAL TABLE memories_vec USING vec0(
                    memory_id TEXT PRIMARY KEY,
                    embedding float[{dims}]
                );"
            ))
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        }
        Ok(())
    }

    /// Search for memories using text matching (fallback, no embeddings).
    pub fn recall(
        &self,
        query: &str,
        limit: usize,
        filter: Option<MemoryFilter>,
    ) -> OpenFangResult<Vec<MemoryFragment>> {
        self.recall_with_embedding(query, limit, filter, None)
    }

    /// Search for memories using vector similarity when a query embedding is provided,
    /// falling back to LIKE matching otherwise.
    pub fn recall_with_embedding(
        &self,
        query: &str,
        limit: usize,
        filter: Option<MemoryFilter>,
        query_embedding: Option<&[f32]>,
    ) -> OpenFangResult<Vec<MemoryFragment>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;

        // Fast path: use sqlite-vec indexed search when query embedding is available
        // and the vec table exists.
        if let Some(qe) = query_embedding {
            let vec_exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='memories_vec'",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(false);

            if vec_exists {
                let result = self.recall_via_vec(&conn, qe, limit, &filter);
                if let Ok(fragments) = result {
                    // Update access counts
                    for frag in &fragments {
                        let _ = conn.execute(
                            "UPDATE memories SET access_count = access_count + 1, accessed_at = ?1 WHERE id = ?2",
                            rusqlite::params![Utc::now().to_rfc3339(), frag.id.0.to_string()],
                        );
                    }
                    return Ok(fragments);
                }
                // Fall through to brute-force on error
                debug!("sqlite-vec recall failed, falling back to brute-force");
            }
        }

        // Fallback: brute-force LIKE matching + optional cosine re-ranking
        let fetch_limit = if query_embedding.is_some() {
            (limit * 10).max(100)
        } else {
            limit
        };

        let mut sql = String::from(
            "SELECT id, agent_id, content, source, scope, confidence, metadata, created_at, accessed_at, access_count, embedding
             FROM memories WHERE deleted = 0",
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut param_idx = 1;

        if query_embedding.is_none() && !query.is_empty() {
            sql.push_str(&format!(" AND content LIKE ?{param_idx}"));
            params.push(Box::new(format!("%{query}%")));
            param_idx += 1;
        }

        if let Some(ref f) = filter {
            if let Some(agent_id) = f.agent_id {
                sql.push_str(&format!(" AND agent_id = ?{param_idx}"));
                params.push(Box::new(agent_id.0.to_string()));
                param_idx += 1;
            }
            if let Some(ref scope) = f.scope {
                sql.push_str(&format!(" AND scope = ?{param_idx}"));
                params.push(Box::new(scope.clone()));
                param_idx += 1;
            }
            if let Some(min_conf) = f.min_confidence {
                sql.push_str(&format!(" AND confidence >= ?{param_idx}"));
                params.push(Box::new(min_conf as f64));
                param_idx += 1;
            }
            if let Some(ref source) = f.source {
                let source_str = helpers::serialize_source(source)?;
                sql.push_str(&format!(" AND source = ?{param_idx}"));
                params.push(Box::new(source_str));
                #[allow(unused_assignments)]
                { param_idx += 1; }
            }
        }

        sql.push_str(" ORDER BY accessed_at DESC, access_count DESC");
        sql.push_str(&format!(" LIMIT {fetch_limit}"));

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt
            .query_map(param_refs.as_slice(), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, f64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, Option<Vec<u8>>>(10)?,
                ))
            })
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut fragments = Vec::new();
        for row_result in rows {
            let (id_str, agent_str, content, source_str, scope, confidence, meta_str, created_str, accessed_str, access_count, embedding_bytes) =
                row_result.map_err(|e| OpenFangError::Memory(e.to_string()))?;

            let id = helpers::parse_memory_id(&id_str)?;
            let agent_id = helpers::parse_agent_id(&agent_str)?;
            let source: MemorySource = helpers::deserialize_source(&source_str);
            let metadata: HashMap<String, serde_json::Value> = helpers::deserialize_metadata(&meta_str);
            let created_at = helpers::parse_rfc3339_or_now(&created_str);
            let accessed_at = helpers::parse_rfc3339_or_now(&accessed_str);
            let embedding = embedding_bytes.as_deref().map(embedding_from_bytes);

            fragments.push(MemoryFragment {
                id, agent_id, content, embedding, metadata, source,
                confidence: confidence as f32, created_at, accessed_at,
                access_count: access_count as u64, scope,
            });
        }

        // Brute-force cosine re-ranking when no vec table was available
        if let Some(qe) = query_embedding {
            fragments.sort_by(|a, b| {
                let sim_a = a.embedding.as_deref().map(|e| cosine_similarity(qe, e)).unwrap_or(-1.0);
                let sim_b = b.embedding.as_deref().map(|e| cosine_similarity(qe, e)).unwrap_or(-1.0);
                sim_b.partial_cmp(&sim_a).unwrap_or(std::cmp::Ordering::Equal)
            });
            fragments.truncate(limit);
        }

        // Update access counts
        for frag in &fragments {
            let _ = conn.execute(
                "UPDATE memories SET access_count = access_count + 1, accessed_at = ?1 WHERE id = ?2",
                rusqlite::params![Utc::now().to_rfc3339(), frag.id.0.to_string()],
            );
        }

        Ok(fragments)
    }

    /// sqlite-vec indexed vector search: uses MATCH on the vec0 virtual table,
    /// then JOINs back to `memories` for full fragment data.
    fn recall_via_vec(
        &self,
        conn: &Connection,
        query_embedding: &[f32],
        limit: usize,
        filter: &Option<MemoryFilter>,
    ) -> OpenFangResult<Vec<MemoryFragment>> {
        let query_bytes = embedding_to_bytes(query_embedding);
        // Fetch more than needed to allow post-filtering
        let fetch_limit = limit * 5;

        let mut stmt = conn
            .prepare(
                "SELECT m.id, m.agent_id, m.content, m.source, m.scope, m.confidence,
                        m.metadata, m.created_at, m.accessed_at, m.access_count, m.embedding,
                        v.distance
                 FROM memories_vec v
                 JOIN memories m ON m.id = v.memory_id
                 WHERE v.embedding MATCH ?1
                   AND m.deleted = 0
                 ORDER BY v.distance
                 LIMIT ?2",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let rows = stmt
            .query_map(rusqlite::params![query_bytes, fetch_limit as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, f64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, Option<Vec<u8>>>(10)?,
                    row.get::<_, f64>(11)?,
                ))
            })
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut fragments = Vec::new();
        for row_result in rows {
            let (id_str, agent_str, content, source_str, scope, confidence, meta_str,
                 created_str, accessed_str, access_count, embedding_bytes, _distance) =
                row_result.map_err(|e| OpenFangError::Memory(e.to_string()))?;

            // Post-filter by agent, scope, confidence, source
            if let Some(ref f) = filter {
                if let Some(filter_agent) = f.agent_id {
                    if agent_str != filter_agent.0.to_string() {
                        continue;
                    }
                }
                if let Some(ref filter_scope) = f.scope {
                    if &scope != filter_scope {
                        continue;
                    }
                }
                if let Some(min_conf) = f.min_confidence {
                    if (confidence as f32) < min_conf {
                        continue;
                    }
                }
                if let Some(ref filter_source) = f.source {
                    let source_str_expected = helpers::serialize_source(filter_source).unwrap_or_default();
                    if source_str != source_str_expected {
                        continue;
                    }
                }
            }

            let id = helpers::parse_memory_id(&id_str)?;
            let agent_id = helpers::parse_agent_id(&agent_str)?;
            let source: MemorySource = helpers::deserialize_source(&source_str);
            let metadata: HashMap<String, serde_json::Value> = helpers::deserialize_metadata(&meta_str);
            let created_at = helpers::parse_rfc3339_or_now(&created_str);
            let accessed_at = helpers::parse_rfc3339_or_now(&accessed_str);
            let embedding = embedding_bytes.as_deref().map(embedding_from_bytes);

            fragments.push(MemoryFragment {
                id, agent_id, content, embedding, metadata, source,
                confidence: confidence as f32, created_at, accessed_at,
                access_count: access_count as u64, scope,
            });

            if fragments.len() >= limit {
                break;
            }
        }

        debug!("sqlite-vec recall: {} results", fragments.len());
        Ok(fragments)
    }

    /// Soft-delete a memory fragment.
    pub fn forget(&self, id: MemoryId) -> OpenFangResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        conn.execute(
            "UPDATE memories SET deleted = 1 WHERE id = ?1",
            rusqlite::params![id.0.to_string()],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        // Also remove from vec table
        let _ = conn.execute(
            "DELETE FROM memories_vec WHERE memory_id = ?1",
            rusqlite::params![id.0.to_string()],
        );

        Ok(())
    }

    /// Update the embedding for an existing memory.
    pub fn update_embedding(&self, id: MemoryId, embedding: &[f32]) -> OpenFangResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let bytes = embedding_to_bytes(embedding);
        conn.execute(
            "UPDATE memories SET embedding = ?1 WHERE id = ?2",
            rusqlite::params![bytes, id.0.to_string()],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        // Also upsert into vec table
        let _ = Self::ensure_vec_table(&conn, embedding.len());
        let _ = conn.execute(
            "INSERT OR REPLACE INTO memories_vec (memory_id, embedding) VALUES (?1, ?2)",
            rusqlite::params![id.0.to_string(), bytes],
        );

        Ok(())
    }

}

impl SemanticBackend for SemanticStore {
    fn remember(
        &self,
        agent_id: AgentId,
        content: &str,
        source: MemorySource,
        scope: &str,
        metadata: HashMap<String, serde_json::Value>,
        embedding: Option<&[f32]>,
    ) -> OpenFangResult<MemoryId> {
        SemanticStore::remember_with_embedding(self, agent_id, content, source, scope, metadata, embedding)
    }

    fn recall(
        &self,
        query: &str,
        limit: usize,
        filter: Option<MemoryFilter>,
        query_embedding: Option<&[f32]>,
    ) -> OpenFangResult<Vec<MemoryFragment>> {
        SemanticStore::recall_with_embedding(self, query, limit, filter, query_embedding)
    }

    fn forget(&self, id: MemoryId) -> OpenFangResult<()> {
        SemanticStore::forget(self, id)
    }

    fn update_embedding(&self, id: MemoryId, embedding: &[f32]) -> OpenFangResult<()> {
        SemanticStore::update_embedding(self, id, embedding)
    }
}

/// Compute cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom < f32::EPSILON {
        0.0
    } else {
        dot / denom
    }
}

/// Serialize embedding to bytes for SQLite BLOB storage.
fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(embedding.len() * 4);
    for &val in embedding {
        bytes.extend_from_slice(&val.to_le_bytes());
    }
    bytes
}

/// Deserialize embedding from bytes.
fn embedding_from_bytes(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::migration::run_migrations;

    fn setup() -> SemanticStore {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        SemanticStore::new(Arc::new(Mutex::new(conn)))
    }

    #[test]
    fn test_remember_and_recall() {
        let store = setup();
        let agent_id = AgentId::new();
        store
            .remember(
                agent_id,
                "The user likes Rust programming",
                MemorySource::Conversation,
                "episodic",
                HashMap::new(),
            )
            .unwrap();
        let results = store.recall("Rust", 10, None).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("Rust"));
    }

    #[test]
    fn test_recall_with_filter() {
        let store = setup();
        let agent_id = AgentId::new();
        store
            .remember(
                agent_id,
                "Memory A",
                MemorySource::Conversation,
                "episodic",
                HashMap::new(),
            )
            .unwrap();
        store
            .remember(
                AgentId::new(),
                "Memory B",
                MemorySource::Conversation,
                "episodic",
                HashMap::new(),
            )
            .unwrap();
        let filter = MemoryFilter::agent(agent_id);
        let results = store.recall("Memory", 10, Some(filter)).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "Memory A");
    }

    #[test]
    fn test_forget() {
        let store = setup();
        let agent_id = AgentId::new();
        let id = store
            .remember(
                agent_id,
                "To forget",
                MemorySource::Conversation,
                "episodic",
                HashMap::new(),
            )
            .unwrap();
        store.forget(id).unwrap();
        let results = store.recall("To forget", 10, None).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_remember_with_embedding() {
        let store = setup();
        let agent_id = AgentId::new();
        let embedding = vec![0.1, 0.2, 0.3, 0.4];
        let id = store
            .remember_with_embedding(
                agent_id,
                "Rust is great",
                MemorySource::Conversation,
                "episodic",
                HashMap::new(),
                Some(&embedding),
            )
            .unwrap();
        assert_ne!(id.0.to_string(), "");
    }

    #[test]
    fn test_vector_recall_ranking() {
        let store = setup();
        let agent_id = AgentId::new();

        // Store 3 memories with embeddings pointing in different directions
        let emb_rust = vec![0.9, 0.1, 0.0, 0.0]; // "Rust" direction
        let emb_python = vec![0.0, 0.0, 0.9, 0.1]; // "Python" direction
        let emb_mixed = vec![0.5, 0.5, 0.0, 0.0]; // mixed

        store
            .remember_with_embedding(
                agent_id,
                "Rust is a systems language",
                MemorySource::Conversation,
                "episodic",
                HashMap::new(),
                Some(&emb_rust),
            )
            .unwrap();
        store
            .remember_with_embedding(
                agent_id,
                "Python is interpreted",
                MemorySource::Conversation,
                "episodic",
                HashMap::new(),
                Some(&emb_python),
            )
            .unwrap();
        store
            .remember_with_embedding(
                agent_id,
                "Both are popular",
                MemorySource::Conversation,
                "episodic",
                HashMap::new(),
                Some(&emb_mixed),
            )
            .unwrap();

        // Query with a "Rust"-like embedding
        let query_emb = vec![0.85, 0.15, 0.0, 0.0];
        let results = store
            .recall_with_embedding("", 3, None, Some(&query_emb))
            .unwrap();

        assert_eq!(results.len(), 3);
        // Rust memory should be first (highest cosine similarity)
        assert!(results[0].content.contains("Rust"));
        // Python memory should be last (lowest similarity)
        assert!(results[2].content.contains("Python"));
    }

    #[test]
    fn test_update_embedding() {
        let store = setup();
        let agent_id = AgentId::new();
        let id = store
            .remember(
                agent_id,
                "No embedding yet",
                MemorySource::Conversation,
                "episodic",
                HashMap::new(),
            )
            .unwrap();

        // Update with embedding
        let emb = vec![1.0, 0.0, 0.0];
        store.update_embedding(id, &emb).unwrap();

        // Verify the embedding is stored by doing vector recall
        let query_emb = vec![1.0, 0.0, 0.0];
        let results = store
            .recall_with_embedding("", 10, None, Some(&query_emb))
            .unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].embedding.is_some());
        assert_eq!(results[0].embedding.as_ref().unwrap().len(), 3);
    }

    #[test]
    fn test_mixed_embedded_and_non_embedded() {
        let store = setup();
        let agent_id = AgentId::new();

        // One memory with embedding, one without
        store
            .remember_with_embedding(
                agent_id,
                "Has embedding",
                MemorySource::Conversation,
                "episodic",
                HashMap::new(),
                Some(&[1.0, 0.0]),
            )
            .unwrap();
        store
            .remember(
                agent_id,
                "No embedding",
                MemorySource::Conversation,
                "episodic",
                HashMap::new(),
            )
            .unwrap();

        // Vector recall should rank embedded memory higher
        let results = store
            .recall_with_embedding("", 10, None, Some(&[1.0, 0.0]))
            .unwrap();
        assert_eq!(results.len(), 2);
        // Embedded memory should rank first
        assert_eq!(results[0].content, "Has embedding");
    }
}
