//! MongoDB semantic memory store with vector embedding support.

use bson::doc;
use chrono::Utc;
use futures::TryStreamExt;
use mongodb::Collection;
use openfang_types::agent::AgentId;
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::memory::{MemoryFilter, MemoryFragment, MemoryId, MemorySource};
use std::collections::HashMap;
use tracing::debug;

/// Semantic store backed by MongoDB with optional vector search.
#[derive(Clone)]
pub struct MongoSemanticStore {
    memories: Collection<bson::Document>,
}

impl MongoSemanticStore {
    pub fn new(db: mongodb::Database) -> Self {
        Self {
            memories: db.collection("memories"),
        }
    }

    /// Store a new memory fragment (without embedding).
    pub async fn remember(
        &self,
        agent_id: AgentId,
        content: &str,
        source: MemorySource,
        scope: &str,
        metadata: HashMap<String, serde_json::Value>,
    ) -> OpenFangResult<MemoryId> {
        self.remember_with_embedding(agent_id, content, source, scope, metadata, None)
            .await
    }

    /// Store a new memory fragment with an optional embedding vector.
    pub async fn remember_with_embedding(
        &self,
        agent_id: AgentId,
        content: &str,
        source: MemorySource,
        scope: &str,
        metadata: HashMap<String, serde_json::Value>,
        embedding: Option<&[f32]>,
    ) -> OpenFangResult<MemoryId> {
        let id = MemoryId::new();
        let now = bson::DateTime::from_chrono(Utc::now());
        let source_str = serde_json::to_string(&source)
            .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
        let meta_str = serde_json::to_string(&metadata)
            .map_err(|e| OpenFangError::Serialization(e.to_string()))?;

        let mut doc = doc! {
            "_id": id.0.to_string(),
            "agent_id": agent_id.0.to_string(),
            "content": content,
            "source": &source_str,
            "scope": scope,
            "confidence": 1.0_f64,
            "metadata": &meta_str,
            "created_at": now,
            "accessed_at": now,
            "access_count": 0_i64,
            "deleted": false,
        };

        if let Some(emb) = embedding {
            let bson_emb: Vec<bson::Bson> = emb.iter().map(|&v| bson::Bson::Double(v as f64)).collect();
            doc.insert("embedding", bson_emb);
        }

        self.memories
            .insert_one(doc)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(id)
    }

    /// Search for memories using text matching (fallback, no embeddings).
    pub async fn recall(
        &self,
        query: &str,
        limit: usize,
        filter: Option<MemoryFilter>,
    ) -> OpenFangResult<Vec<MemoryFragment>> {
        self.recall_with_embedding(query, limit, filter, None).await
    }

    /// Search for memories using vector similarity when a query embedding is provided,
    /// falling back to regex matching otherwise.
    pub async fn recall_with_embedding(
        &self,
        query: &str,
        limit: usize,
        filter: Option<MemoryFilter>,
        query_embedding: Option<&[f32]>,
    ) -> OpenFangResult<Vec<MemoryFragment>> {
        let fetch_limit = if query_embedding.is_some() {
            (limit * 10).max(100)
        } else {
            limit
        };

        let mut filter_doc = doc! { "deleted": false };

        // Text search filter (only when no embeddings)
        if query_embedding.is_none() && !query.is_empty() {
            // Escape regex special characters for safe literal matching
            let escaped = regex_escape(query);
            filter_doc.insert("content", doc! { "$regex": &escaped, "$options": "i" });
        }

        // Apply filters
        if let Some(ref f) = filter {
            if let Some(agent_id) = f.agent_id {
                filter_doc.insert("agent_id", agent_id.0.to_string());
            }
            if let Some(ref scope) = f.scope {
                filter_doc.insert("scope", scope.as_str());
            }
            if let Some(min_conf) = f.min_confidence {
                filter_doc.insert("confidence", doc! { "$gte": min_conf as f64 });
            }
            if let Some(ref source) = f.source {
                let source_str = serde_json::to_string(source)
                    .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
                filter_doc.insert("source", source_str);
            }
        }

        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "accessed_at": -1, "access_count": -1 })
            .limit(fetch_limit as i64)
            .build();

        let mut cursor = self
            .memories
            .find(filter_doc)
            .with_options(opts)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut fragments = Vec::new();
        while let Some(d) = cursor
            .try_next()
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?
        {
            if let Some(frag) = parse_memory_doc(&d) {
                fragments.push(frag);
            }
        }

        // If we have a query embedding, re-rank by cosine similarity
        if let Some(qe) = query_embedding {
            fragments.sort_by(|a, b| {
                let sim_a = a
                    .embedding
                    .as_deref()
                    .map(|e| cosine_similarity(qe, e))
                    .unwrap_or(-1.0);
                let sim_b = b
                    .embedding
                    .as_deref()
                    .map(|e| cosine_similarity(qe, e))
                    .unwrap_or(-1.0);
                sim_b
                    .partial_cmp(&sim_a)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            fragments.truncate(limit);
            debug!(
                "Vector recall: {} results from {} candidates",
                fragments.len(),
                fetch_limit
            );
        }

        // Update access counts for returned memories
        let now = bson::DateTime::from_chrono(Utc::now());
        for frag in &fragments {
            let _ = self
                .memories
                .update_one(
                    doc! { "_id": frag.id.0.to_string() },
                    doc! { "$inc": { "access_count": 1_i64 }, "$set": { "accessed_at": now } },
                )
                .await;
        }

        Ok(fragments)
    }

    /// Soft-delete a memory fragment.
    pub async fn forget(&self, id: MemoryId) -> OpenFangResult<()> {
        self.memories
            .update_one(
                doc! { "_id": id.0.to_string() },
                doc! { "$set": { "deleted": true } },
            )
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Update the embedding for an existing memory.
    pub async fn update_embedding(&self, id: MemoryId, embedding: &[f32]) -> OpenFangResult<()> {
        let bson_emb: Vec<bson::Bson> = embedding.iter().map(|&v| bson::Bson::Double(v as f64)).collect();
        self.memories
            .update_one(
                doc! { "_id": id.0.to_string() },
                doc! { "$set": { "embedding": bson_emb } },
            )
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }
}

fn parse_memory_doc(d: &bson::Document) -> Option<MemoryFragment> {
    let id_str = d.get_str("_id").ok()?;
    let id = uuid::Uuid::parse_str(id_str).ok().map(MemoryId)?;

    let agent_str = d.get_str("agent_id").unwrap_or_default();
    let agent_id = uuid::Uuid::parse_str(agent_str)
        .map(openfang_types::agent::AgentId)
        .ok()?;

    let content = d.get_str("content").unwrap_or_default().to_string();

    let source_str = d.get_str("source").unwrap_or("\"system\"");
    let source: MemorySource = serde_json::from_str(source_str).unwrap_or(MemorySource::System);

    let scope = d.get_str("scope").unwrap_or("episodic").to_string();

    let confidence = d.get_f64("confidence").unwrap_or(1.0) as f32;

    let meta_str = d.get_str("metadata").unwrap_or("{}");
    let metadata: HashMap<String, serde_json::Value> =
        serde_json::from_str(meta_str).unwrap_or_default();

    let created_at = d
        .get_datetime("created_at")
        .ok()
        .map(|dt| dt.to_chrono())
        .unwrap_or_else(Utc::now);

    let accessed_at = d
        .get_datetime("accessed_at")
        .ok()
        .map(|dt| dt.to_chrono())
        .unwrap_or_else(Utc::now);

    let access_count = d.get_i64("access_count").unwrap_or(0) as u64;

    let embedding = d.get_array("embedding").ok().map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect::<Vec<f32>>()
    });

    Some(MemoryFragment {
        id,
        agent_id,
        content,
        embedding,
        metadata,
        source,
        confidence,
        created_at,
        accessed_at,
        access_count,
        scope,
    })
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

/// Escape regex special characters for safe use in MongoDB $regex.
fn regex_escape(s: &str) -> String {
    let special = ['.', '*', '+', '?', '(', ')', '[', ']', '{', '}', '\\', '^', '$', '|'];
    let mut escaped = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        if special.contains(&c) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}
