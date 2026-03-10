//! # maestro-knowledge
//!
//! Knowledge/RAG system for OpenFang. Provides document ingestion, chunking,
//! embedding, and retrieval-augmented generation (RAG) using SurrealDB as the
//! vector store backend.
//!
//! ## Architecture
//!
//! ```text
//! Document → Chunker → EmbeddingModel → SurrealDB (HNSW index)
//!                                            ↓
//! Query → EmbeddingModel → KNN search → Ranked chunks → LLM → Answer
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use surrealdb::engine::local::{Db, SurrealKv};
use surrealdb::Surreal;
use thiserror::Error;
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum KnowledgeError {
    #[error("SurrealDB error: {0}")]
    Database(String),
    #[error("Embedding error: {0}")]
    Embedding(String),
    #[error("Document not found: {0}")]
    NotFound(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Invalid configuration: {0}")]
    Config(String),
}

pub type KnowledgeResult<T> = Result<T, KnowledgeError>;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// A document to be ingested into the knowledge base.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub title: String,
    pub content: String,
    pub source: String,
    pub metadata: serde_json::Value,
    pub content_hash: String,
}

impl Document {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        content: impl Into<String>,
        source: impl Into<String>,
        metadata: serde_json::Value,
    ) -> Self {
        let content = content.into();
        let content_hash = format!("{:x}", Sha256::digest(content.as_bytes()));
        Self {
            id: id.into(),
            title: title.into(),
            content,
            source: source.into(),
            metadata,
            content_hash,
        }
    }
}

/// A chunk of a document after splitting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub id: String,
    pub document_id: String,
    pub content: String,
    pub chunk_index: usize,
    pub metadata: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
}

/// A search result from the knowledge base.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub chunk: Chunk,
    pub score: f32,
    pub document_title: String,
    pub document_source: String,
}

/// A source citation for a RAG answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceCitation {
    pub document_id: String,
    pub document_title: String,
    pub document_source: String,
    pub chunk_content: String,
    pub relevance_score: f32,
}

// ---------------------------------------------------------------------------
// Chunking
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkConfig {
    pub strategy: ChunkStrategy,
    pub chunk_size: usize,
    pub chunk_overlap: usize,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            strategy: ChunkStrategy::FixedSize,
            chunk_size: 512,
            chunk_overlap: 64,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkStrategy {
    FixedSize,
    SentenceAware,
    MarkdownAware,
}

pub fn chunk_document(doc: &Document, config: &ChunkConfig) -> Vec<Chunk> {
    match config.strategy {
        ChunkStrategy::FixedSize => chunk_fixed(doc, &doc.content, config),
        ChunkStrategy::SentenceAware => chunk_sentences(doc, &doc.content, config),
        ChunkStrategy::MarkdownAware => chunk_markdown(doc, &doc.content, config),
    }
}

fn make_chunk(doc: &Document, content: String, index: usize) -> Chunk {
    Chunk {
        id: format!("{}:{}", doc.id, index),
        document_id: doc.id.clone(),
        content,
        chunk_index: index,
        metadata: doc.metadata.clone(),
        embedding: None,
    }
}

fn chunk_fixed(doc: &Document, text: &str, config: &ChunkConfig) -> Vec<Chunk> {
    let chars: Vec<char> = text.chars().collect();
    let size = config.chunk_size.max(1);
    let overlap = config.chunk_overlap.min(size.saturating_sub(1));
    let step = size - overlap;
    let mut chunks = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while start < chars.len() {
        let end = (start + size).min(chars.len());
        let content: String = chars[start..end].iter().collect();
        let trimmed = content.trim().to_string();
        if !trimmed.is_empty() {
            chunks.push(make_chunk(doc, trimmed, index));
            index += 1;
        }
        if end == chars.len() { break; }
        start += step;
    }
    chunks
}

fn chunk_sentences(doc: &Document, text: &str, config: &ChunkConfig) -> Vec<Chunk> {
    let sentences: Vec<&str> = text
        .split_inclusive(|c| c == '.' || c == '!' || c == '?' || c == '\n')
        .filter(|s| !s.trim().is_empty())
        .collect();
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut index = 0;
    for sentence in sentences {
        if current.len() + sentence.len() > config.chunk_size && !current.is_empty() {
            chunks.push(make_chunk(doc, current.trim().to_string(), index));
            index += 1;
            let overlap_start = current.len().saturating_sub(config.chunk_overlap);
            current = current[overlap_start..].to_string();
        }
        current.push_str(sentence);
    }
    if !current.trim().is_empty() {
        chunks.push(make_chunk(doc, current.trim().to_string(), index));
    }
    chunks
}

fn chunk_markdown(doc: &Document, text: &str, config: &ChunkConfig) -> Vec<Chunk> {
    let mut sections: Vec<String> = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        if line.starts_with('#') && !current.is_empty() {
            sections.push(current.trim().to_string());
            current = String::new();
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.trim().is_empty() {
        sections.push(current.trim().to_string());
    }
    let mut chunks = Vec::new();
    let mut index = 0;
    for section in sections {
        if section.len() > config.chunk_size {
            let sub_doc = Document {
                id: doc.id.clone(),
                title: doc.title.clone(),
                content: section.clone(),
                source: doc.source.clone(),
                metadata: doc.metadata.clone(),
                content_hash: String::new(),
            };
            for mut sub in chunk_fixed(&sub_doc, &section, config) {
                sub.chunk_index = index;
                sub.id = format!("{}:{}", doc.id, index);
                chunks.push(sub);
                index += 1;
            }
        } else {
            chunks.push(make_chunk(doc, section, index));
            index += 1;
        }
    }
    chunks
}

// ---------------------------------------------------------------------------
// KnowledgeStore trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait KnowledgeStore: Send + Sync {
    async fn ingest(&self, doc: Document, config: &ChunkConfig) -> KnowledgeResult<usize>;
    async fn search_by_embedding(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        filter: Option<HashMap<String, String>>,
    ) -> KnowledgeResult<Vec<SearchResult>>;
    async fn search_by_text(&self, query: &str, top_k: usize) -> KnowledgeResult<Vec<SearchResult>>;
    async fn delete(&self, document_id: &str) -> KnowledgeResult<()>;
    async fn count_documents(&self) -> KnowledgeResult<usize>;
    async fn count_chunks(&self) -> KnowledgeResult<usize>;
    async fn list_documents(&self) -> KnowledgeResult<Vec<(String, String)>>;
}

// ---------------------------------------------------------------------------
// SurrealKnowledgeStore
// ---------------------------------------------------------------------------

const KG_NAMESPACE: &str = "openfang";
const KG_DATABASE: &str = "knowledge";

pub struct SurrealKnowledgeStore {
    db: Surreal<Db>,
}

impl SurrealKnowledgeStore {
    pub async fn connect(db_path: impl AsRef<std::path::Path>) -> KnowledgeResult<Self> {
        let db: Surreal<Db> = Surreal::new::<SurrealKv>(db_path.as_ref())
            .await
            .map_err(|e| KnowledgeError::Database(format!("Connect failed: {}", e)))?;
        db.use_ns(KG_NAMESPACE).use_db(KG_DATABASE).await
            .map_err(|e| KnowledgeError::Database(format!("NS/DB select failed: {}", e)))?;
        let store = Self { db };
        store.initialize_schema().await?;
        Ok(store)
    }

    pub async fn connect_in_memory() -> KnowledgeResult<Self> {
        let db: Surreal<Db> = Surreal::new::<surrealdb::engine::local::Mem>(())
            .await
            .map_err(|e| KnowledgeError::Database(format!("In-memory connect failed: {}", e)))?;
        db.use_ns(KG_NAMESPACE).use_db(KG_DATABASE).await
            .map_err(|e| KnowledgeError::Database(format!("NS/DB select failed: {}", e)))?;
        let store = Self { db };
        store.initialize_schema().await?;
        Ok(store)
    }

    async fn initialize_schema(&self) -> KnowledgeResult<()> {
        self.db.query(r#"
            DEFINE TABLE IF NOT EXISTS kg_documents SCHEMAFULL;
            DEFINE FIELD IF NOT EXISTS id ON kg_documents TYPE string;
            DEFINE FIELD IF NOT EXISTS title ON kg_documents TYPE string;
            DEFINE FIELD IF NOT EXISTS source ON kg_documents TYPE string;
            DEFINE FIELD IF NOT EXISTS content_hash ON kg_documents TYPE string;
            DEFINE FIELD IF NOT EXISTS metadata ON kg_documents TYPE object;
            DEFINE FIELD IF NOT EXISTS chunk_count ON kg_documents TYPE int;
            DEFINE FIELD IF NOT EXISTS created_at ON kg_documents TYPE string;
            DEFINE FIELD IF NOT EXISTS updated_at ON kg_documents TYPE string;
            DEFINE INDEX IF NOT EXISTS idx_kg_doc_hash ON kg_documents FIELDS content_hash UNIQUE;

            DEFINE TABLE IF NOT EXISTS kg_chunks SCHEMAFULL;
            DEFINE FIELD IF NOT EXISTS id ON kg_chunks TYPE string;
            DEFINE FIELD IF NOT EXISTS document_id ON kg_chunks TYPE string;
            DEFINE FIELD IF NOT EXISTS content ON kg_chunks TYPE string;
            DEFINE FIELD IF NOT EXISTS chunk_index ON kg_chunks TYPE int;
            DEFINE FIELD IF NOT EXISTS metadata ON kg_chunks TYPE object;
            DEFINE FIELD IF NOT EXISTS embedding ON kg_chunks TYPE option<array<float>>;
            DEFINE FIELD IF NOT EXISTS created_at ON kg_chunks TYPE string;
            DEFINE INDEX IF NOT EXISTS idx_kg_chunk_doc ON kg_chunks FIELDS document_id;
            DEFINE INDEX IF NOT EXISTS idx_kg_chunk_embedding ON kg_chunks FIELDS embedding HNSW DIMENSION 1536 DIST COSINE;
        "#)
        .await
        .map_err(|e| KnowledgeError::Database(format!("Schema init failed: {}", e)))?;
        Ok(())
    }

    pub async fn store_chunk(&self, chunk: &Chunk) -> KnowledgeResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.db
            .query("CREATE type::record('kg_chunks', $id) CONTENT $data")
            .bind(("id", chunk.id.clone()))
            .bind(("data", serde_json::json!({
                "id": chunk.id,
                "document_id": chunk.document_id,
                "content": chunk.content,
                "chunk_index": chunk.chunk_index,
                "metadata": chunk.metadata,
                "embedding": chunk.embedding,
                "created_at": now,
            })))
            .await
            .map_err(|e| KnowledgeError::Database(format!("Chunk store failed: {}", e)))?;
        Ok(())
    }

    pub async fn store_document_meta(&self, doc: &Document, chunk_count: usize) -> KnowledgeResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.db
            .query("UPSERT type::record('kg_documents', $id) CONTENT $data")
            .bind(("id", doc.id.clone()))
            .bind(("data", serde_json::json!({
                "id": doc.id,
                "title": doc.title,
                "source": doc.source,
                "content_hash": doc.content_hash,
                "metadata": doc.metadata,
                "chunk_count": chunk_count,
                "created_at": now,
                "updated_at": now,
            })))
            .await
            .map_err(|e| KnowledgeError::Database(format!("Document meta store failed: {}", e)))?;
        Ok(())
    }

    async fn find_by_hash(&self, hash: &str) -> KnowledgeResult<Option<String>> {
        let results: Vec<serde_json::Value> = self.db
            .query("SELECT id FROM kg_documents WHERE content_hash = $hash LIMIT 1")
            .bind(("hash", hash.to_string()))
            .await
            .map_err(|e| KnowledgeError::Database(format!("Hash lookup failed: {}", e)))?
            .take(0)
            .map_err(|e| KnowledgeError::Database(format!("Hash lookup result failed: {}", e)))?;
        Ok(results.first().and_then(|v| v.get("id")).and_then(|v| v.as_str()).map(|s| s.to_string()))
    }

    async fn get_document_meta(&self, doc_id: &str) -> KnowledgeResult<(String, String)> {
        let result: Option<serde_json::Value> = self.db
            .select(("kg_documents", doc_id))
            .await
            .map_err(|e| KnowledgeError::Database(format!("Document meta fetch failed: {}", e)))?;
        match result {
            Some(v) => {
                let title = v.get("title").and_then(|t| t.as_str()).unwrap_or("Unknown").to_string();
                let source = v.get("source").and_then(|s| s.as_str()).unwrap_or("").to_string();
                Ok((title, source))
            }
            None => Ok(("Unknown".to_string(), "".to_string())),
        }
    }
}

#[async_trait]
impl KnowledgeStore for SurrealKnowledgeStore {
    async fn ingest(&self, doc: Document, config: &ChunkConfig) -> KnowledgeResult<usize> {
        if let Some(existing_id) = self.find_by_hash(&doc.content_hash).await? {
            if existing_id == doc.id {
                debug!("Document {} already indexed (same hash), skipping", doc.id);
                return Ok(0);
            }
            warn!("Document {} has same content hash as {}, re-indexing", doc.id, existing_id);
        }
        let chunks = chunk_document(&doc, config);
        let chunk_count = chunks.len();
        info!("Ingesting document '{}' → {} chunks", doc.title, chunk_count);
        self.store_document_meta(&doc, chunk_count).await?;
        for chunk in &chunks {
            self.store_chunk(chunk).await?;
        }
        Ok(chunk_count)
    }

    async fn search_by_embedding(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        _filter: Option<HashMap<String, String>>,
    ) -> KnowledgeResult<Vec<SearchResult>> {
        let sql = format!(
            "SELECT *, vector::similarity::cosine(embedding, $vec) AS score \
             FROM kg_chunks WHERE embedding != NONE \
             ORDER BY embedding <|{},64|> $vec LIMIT $k",
            top_k
        );
        let results: Vec<serde_json::Value> = self.db
            .query(&sql)
            .bind(("vec", serde_json::json!(query_embedding)))
            .bind(("k", top_k))
            .await
            .map_err(|e| KnowledgeError::Database(format!("Vector search failed: {}", e)))?
            .take(0)
            .map_err(|e| KnowledgeError::Database(format!("Vector search result failed: {}", e)))?;

        let mut search_results = Vec::new();
        for row in results {
            let doc_id = row.get("document_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let (doc_title, doc_source) = self.get_document_meta(&doc_id).await.unwrap_or_default();
            let chunk = Chunk {
                id: row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                document_id: doc_id,
                content: row.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                chunk_index: row.get("chunk_index").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                metadata: row.get("metadata").cloned().unwrap_or(serde_json::json!({})),
                embedding: None,
            };
            let score = row.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            search_results.push(SearchResult { chunk, score, document_title: doc_title, document_source: doc_source });
        }
        Ok(search_results)
    }

    async fn search_by_text(&self, query: &str, top_k: usize) -> KnowledgeResult<Vec<SearchResult>> {
        let results: Vec<serde_json::Value> = self.db
            .query("SELECT * FROM kg_chunks WHERE string::contains(string::lowercase(content), $q) LIMIT $k")
            .bind(("q", query.to_lowercase()))
            .bind(("k", top_k))
            .await
            .map_err(|e| KnowledgeError::Database(format!("Text search failed: {}", e)))?
            .take(0)
            .map_err(|e| KnowledgeError::Database(format!("Text search result failed: {}", e)))?;

        let mut search_results = Vec::new();
        for row in results {
            let doc_id = row.get("document_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let (doc_title, doc_source) = self.get_document_meta(&doc_id).await.unwrap_or_default();
            let chunk = Chunk {
                id: row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                document_id: doc_id,
                content: row.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                chunk_index: row.get("chunk_index").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                metadata: row.get("metadata").cloned().unwrap_or(serde_json::json!({})),
                embedding: None,
            };
            search_results.push(SearchResult { chunk, score: 0.5, document_title: doc_title, document_source: doc_source });
        }
        Ok(search_results)
    }

    async fn delete(&self, document_id: &str) -> KnowledgeResult<()> {
        self.db.query("DELETE kg_chunks WHERE document_id = $doc_id")
            .bind(("doc_id", document_id.to_string()))
            .await
            .map_err(|e| KnowledgeError::Database(format!("Chunk delete failed: {}", e)))?;
        self.db.query("DELETE kg_documents WHERE id = $doc_id")
            .bind(("doc_id", document_id.to_string()))
            .await
            .map_err(|e| KnowledgeError::Database(format!("Document delete failed: {}", e)))?;
        Ok(())
    }

    async fn count_documents(&self) -> KnowledgeResult<usize> {
        let results: Vec<serde_json::Value> = self.db
            .query("SELECT count() AS c FROM kg_documents GROUP ALL")
            .await
            .map_err(|e| KnowledgeError::Database(format!("Count query failed: {}", e)))?
            .take(0)
            .map_err(|e| KnowledgeError::Database(format!("Count result failed: {}", e)))?;
        Ok(results.first().and_then(|v| v.get("c")).and_then(|v| v.as_u64()).unwrap_or(0) as usize)
    }

    async fn count_chunks(&self) -> KnowledgeResult<usize> {
        let results: Vec<serde_json::Value> = self.db
            .query("SELECT count() AS c FROM kg_chunks GROUP ALL")
            .await
            .map_err(|e| KnowledgeError::Database(format!("Count query failed: {}", e)))?
            .take(0)
            .map_err(|e| KnowledgeError::Database(format!("Count result failed: {}", e)))?;
        Ok(results.first().and_then(|v| v.get("c")).and_then(|v| v.as_u64()).unwrap_or(0) as usize)
    }

    async fn list_documents(&self) -> KnowledgeResult<Vec<(String, String)>> {
        let results: Vec<serde_json::Value> = self.db
            .query("SELECT id, title FROM kg_documents ORDER BY created_at DESC")
            .await
            .map_err(|e| KnowledgeError::Database(format!("List query failed: {}", e)))?
            .take(0)
            .map_err(|e| KnowledgeError::Database(format!("List result failed: {}", e)))?;
        Ok(results.iter().map(|v| {
            let id = v.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let title = v.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
            (id, title)
        }).collect())
    }
}

// ---------------------------------------------------------------------------
// KnowledgeBase — high-level RAG facade
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct KnowledgeBaseConfig {
    pub top_k: usize,
    pub min_score: f32,
    pub max_context_tokens: usize,
    pub chunk_config: ChunkConfig,
}

impl Default for KnowledgeBaseConfig {
    fn default() -> Self {
        Self {
            top_k: 5,
            min_score: 0.5,
            max_context_tokens: 4096,
            chunk_config: ChunkConfig::default(),
        }
    }
}

pub struct KnowledgeBase {
    store: SurrealKnowledgeStore,
    config: KnowledgeBaseConfig,
}

impl KnowledgeBase {
    pub fn new(store: SurrealKnowledgeStore, config: KnowledgeBaseConfig) -> Self {
        Self { store, config }
    }

    /// Ingest a document with optional pre-computed embeddings.
    pub async fn ingest(
        &self,
        doc: Document,
        embeddings: Option<Vec<Vec<f32>>>,
    ) -> KnowledgeResult<usize> {
        let mut chunks = chunk_document(&doc, &self.config.chunk_config);
        if let Some(embs) = embeddings {
            for (chunk, emb) in chunks.iter_mut().zip(embs.into_iter()) {
                chunk.embedding = Some(emb);
            }
        }
        self.store.store_document_meta(&doc, chunks.len()).await?;
        for chunk in &chunks {
            self.store.store_chunk(chunk).await?;
        }
        Ok(chunks.len())
    }

    /// Retrieve relevant chunks for a query.
    pub async fn retrieve(
        &self,
        query: &str,
        query_embedding: Option<&[f32]>,
    ) -> KnowledgeResult<Vec<SearchResult>> {
        let results = match query_embedding {
            Some(emb) => self.store.search_by_embedding(emb, self.config.top_k, None).await?,
            None => self.store.search_by_text(query, self.config.top_k).await?,
        };
        Ok(results.into_iter().filter(|r| r.score >= self.config.min_score).collect())
    }

    /// Build a RAG context string from search results.
    pub fn build_context(results: &[SearchResult]) -> String {
        let mut context = String::from("## Relevant Knowledge\n\n");
        for (i, result) in results.iter().enumerate() {
            context.push_str(&format!(
                "### Source {} — {} (score: {:.2})\n{}\n\n",
                i + 1, result.document_title, result.score, result.chunk.content
            ));
        }
        context
    }

    /// Build source citations from search results.
    pub fn build_citations(results: &[SearchResult]) -> Vec<SourceCitation> {
        results.iter().map(|r| SourceCitation {
            document_id: r.chunk.document_id.clone(),
            document_title: r.document_title.clone(),
            document_source: r.document_source.clone(),
            chunk_content: r.chunk.content.clone(),
            relevance_score: r.score,
        }).collect()
    }

    pub fn store(&self) -> &SurrealKnowledgeStore { &self.store }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_chunk_fixed_size() {
        let doc = Document::new("d1", "T", "abcdefghijklmnopqrstuvwxyz", "t://", serde_json::json!({}));
        let config = ChunkConfig { strategy: ChunkStrategy::FixedSize, chunk_size: 10, chunk_overlap: 2 };
        let chunks = chunk_document(&doc, &config);
        assert!(!chunks.is_empty());
    }

    #[tokio::test]
    async fn test_chunk_sentence_aware() {
        let doc = Document::new("d2", "T", "First. Second! Third?", "t://", serde_json::json!({}));
        let config = ChunkConfig { strategy: ChunkStrategy::SentenceAware, chunk_size: 10, chunk_overlap: 0 };
        let chunks = chunk_document(&doc, &config);
        assert!(!chunks.is_empty());
    }

    #[tokio::test]
    async fn test_chunk_markdown_aware() {
        let doc = Document::new("d3", "T", "# H1\nContent.\n\n## H2\nMore.", "t://", serde_json::json!({}));
        let config = ChunkConfig { strategy: ChunkStrategy::MarkdownAware, chunk_size: 512, chunk_overlap: 0 };
        let chunks = chunk_document(&doc, &config);
        assert!(chunks.len() >= 2);
    }

    #[tokio::test]
    async fn test_document_hash() {
        let d1 = Document::new("d1", "T", "Same", "t://", serde_json::json!({}));
        let d2 = Document::new("d2", "T", "Same", "t://", serde_json::json!({}));
        let d3 = Document::new("d3", "T", "Diff", "t://", serde_json::json!({}));
        assert_eq!(d1.content_hash, d2.content_hash);
        assert_ne!(d1.content_hash, d3.content_hash);
    }

    #[tokio::test]
    async fn test_surreal_store_ingest_and_search() {
        let store = SurrealKnowledgeStore::connect_in_memory().await.unwrap();
        let doc = Document::new("doc1", "Rust", "Rust is a systems language.", "t://", serde_json::json!({}));
        let config = ChunkConfig::default();
        let n = store.ingest(doc, &config).await.unwrap();
        assert!(n > 0);
        assert_eq!(store.count_documents().await.unwrap(), 1);
        let results = store.search_by_text("rust", 5).await.unwrap();
        assert!(!results.is_empty());
        store.delete("doc1").await.unwrap();
        assert_eq!(store.count_documents().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_deduplication() {
        let store = SurrealKnowledgeStore::connect_in_memory().await.unwrap();
        let config = ChunkConfig::default();
        let doc = Document::new("dup", "D", "Same content.", "t://", serde_json::json!({}));
        let n1 = store.ingest(doc.clone(), &config).await.unwrap();
        assert!(n1 > 0);
        let n2 = store.ingest(doc, &config).await.unwrap();
        assert_eq!(n2, 0);
    }

    #[tokio::test]
    async fn test_build_context() {
        let results = vec![SearchResult {
            chunk: Chunk { id: "c1".into(), document_id: "d1".into(), content: "Rust is fast.".into(), chunk_index: 0, metadata: serde_json::json!({}), embedding: None },
            score: 0.95,
            document_title: "Rust Guide".into(),
            document_source: "t://".into(),
        }];
        let ctx = KnowledgeBase::build_context(&results);
        assert!(ctx.contains("Rust Guide"));
        assert!(ctx.contains("Rust is fast."));
    }
}

pub mod ingestion;
