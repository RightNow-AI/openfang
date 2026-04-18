//! PostgreSQL + pgvector implementation of the semantic store.
//!
//! Mirrors the semantics of [`crate::qdrant::QdrantSemanticStore`] using
//! pgvector for similarity search. `remember`/`recall`/`forget`/`update_embedding`
//! all operate against the shared `memories` table created by the
//! [`crate::postgres::migration`] module.

use crate::helpers;
use deadpool_postgres::Pool;
use openfang_types::agent::AgentId;
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::memory::{MemoryFilter, MemoryFragment, MemoryId, MemorySource};
use openfang_types::storage::SemanticBackend;
use pgvector::Vector;
use std::collections::HashMap;

/// PostgreSQL-backed semantic store with pgvector for similarity search.
///
/// This is the Postgres equivalent of `QdrantSemanticStore`: it requires an
/// embedding for `recall()` (same as Qdrant) and uses cosine distance
/// (`<=>`) for vector ordering.
pub struct PostgresSemanticStore {
    pool: Pool,
}

impl PostgresSemanticStore {
    /// Create a new PostgreSQL-backed semantic store from an existing pool.
    ///
    /// The caller is expected to have already run `run_migrations` on the pool
    /// so that the `memories` table (with its `embedding vector` column) exists.
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    fn block_on_pg<F, T>(&self, f: F) -> OpenFangResult<T>
    where
        F: std::future::Future<Output = OpenFangResult<T>>,
    {
        tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(f))
    }
}

impl SemanticBackend for PostgresSemanticStore {
    fn remember(
        &self,
        agent_id: AgentId,
        content: &str,
        source: MemorySource,
        scope: &str,
        metadata: HashMap<String, serde_json::Value>,
        embedding: Option<&[f32]>,
    ) -> OpenFangResult<MemoryId> {
        let id = MemoryId::new();
        let source_str = helpers::serialize_source(&source)?;
        let meta_str = helpers::serialize_metadata(&metadata)?;
        let vec_embedding = embedding.map(|e| Vector::from(e.to_vec()));
        let content = content.to_string();
        let scope = scope.to_string();

        self.block_on_pg(async {
            let client = self
                .pool
                .get()
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            client
                .execute(
                    "INSERT INTO memories (id, agent_id, content, source, scope, confidence, metadata, embedding, created_at, accessed_at, access_count, deleted)
                     VALUES ($1, $2, $3, $4, $5, 1.0, $6, $7, NOW(), NOW(), 0, FALSE)",
                    &[
                        &id.0.to_string(),
                        &agent_id.0.to_string(),
                        &content,
                        &source_str,
                        &scope,
                        &meta_str,
                        &vec_embedding,
                    ],
                )
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(id)
        })
    }

    fn recall(
        &self,
        _query: &str,
        limit: usize,
        filter: Option<MemoryFilter>,
        query_embedding: Option<&[f32]>,
    ) -> OpenFangResult<Vec<MemoryFragment>> {
        // pgvector similarity search requires an embedding — mirror Qdrant's
        // contract (embedding-required). Callers that want text fallback
        // should use the SQLite semantic backend instead.
        let vec_embedding = match query_embedding {
            Some(e) => Vector::from(e.to_vec()),
            None => {
                return Err(OpenFangError::Memory(
                    "postgres semantic backend requires a query embedding for recall()".to_string(),
                ));
            }
        };

        self.block_on_pg(async {
            let client = self
                .pool
                .get()
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;

            // Build WHERE clause dynamically from filter, collecting owned params.
            let mut conditions = vec!["deleted = FALSE".to_string()];
            let mut param_idx: u32 = 1;
            let mut params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = Vec::new();

            if let Some(ref f) = filter {
                if let Some(agent_id) = f.agent_id {
                    conditions.push(format!("agent_id = ${param_idx}"));
                    params.push(Box::new(agent_id.0.to_string()));
                    param_idx += 1;
                }
                if let Some(ref scope) = f.scope {
                    conditions.push(format!("scope = ${param_idx}"));
                    params.push(Box::new(scope.clone()));
                    param_idx += 1;
                }
                if let Some(min_conf) = f.min_confidence {
                    conditions.push(format!("confidence >= ${param_idx}"));
                    params.push(Box::new(min_conf as f64));
                    param_idx += 1;
                }
            }

            let where_clause = conditions.join(" AND ");

            // Cosine distance ordering: `<=>` in pgvector.
            let sql = format!(
                "SELECT id, agent_id, content, source, scope, confidence, metadata, created_at, accessed_at, access_count
                 FROM memories
                 WHERE {where_clause} AND embedding IS NOT NULL
                 ORDER BY embedding <=> ${param_idx}
                 LIMIT {limit}"
            );

            let mut final_params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = params
                .iter()
                .map(|b| b.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync))
                .collect();
            final_params.push(&vec_embedding);

            let rows = client
                .query(&sql, &final_params)
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;

            let mut fragments = Vec::with_capacity(rows.len());
            for row in &rows {
                let id_str: String = row.get(0);
                let agent_str: String = row.get(1);
                let content: String = row.get(2);
                let source_str: String = row.get(3);
                let scope: String = row.get(4);
                let confidence: f32 = row.get(5);
                let meta_str: String = row.get(6);
                let created_at: chrono::DateTime<chrono::Utc> = row.get(7);
                let accessed_at: chrono::DateTime<chrono::Utc> = row.get(8);
                let access_count: i64 = row.get(9);

                let id = helpers::parse_memory_id(&id_str)?;
                let agent_id = helpers::parse_agent_id(&agent_str)?;
                let source: MemorySource = helpers::deserialize_source(&source_str);
                let metadata: HashMap<String, serde_json::Value> =
                    helpers::deserialize_metadata(&meta_str);

                fragments.push(MemoryFragment {
                    id,
                    agent_id,
                    content,
                    embedding: None,
                    metadata,
                    source,
                    confidence,
                    created_at,
                    accessed_at,
                    access_count: access_count as u64,
                    scope,
                });

                // Best-effort access bump; ignore errors.
                let _ = client
                    .execute(
                        "UPDATE memories SET access_count = access_count + 1, accessed_at = NOW() WHERE id = $1",
                        &[&id_str],
                    )
                    .await;
            }

            Ok(fragments)
        })
    }

    fn forget(&self, id: MemoryId) -> OpenFangResult<()> {
        self.block_on_pg(async {
            let client = self
                .pool
                .get()
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            client
                .execute(
                    "UPDATE memories SET deleted = TRUE WHERE id = $1",
                    &[&id.0.to_string()],
                )
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(())
        })
    }

    fn update_embedding(&self, id: MemoryId, embedding: &[f32]) -> OpenFangResult<()> {
        let vec = Vector::from(embedding.to_vec());
        self.block_on_pg(async {
            let client = self
                .pool
                .get()
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            client
                .execute(
                    "UPDATE memories SET embedding = $1 WHERE id = $2",
                    &[&vec, &id.0.to_string()],
                )
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(())
        })
    }
}
