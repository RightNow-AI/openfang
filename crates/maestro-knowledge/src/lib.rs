//! # maestro-knowledge
//!
//! Knowledge/RAG system inspired by Kore.ai's Knowledge AI module.
//!
//! ## What Kore.ai Has (that OpenFang lacks)
//!
//! - **Document ingestion** from 10+ sources (web, files, connectors)
//! - **Chunking strategies** (fixed-size, semantic, recursive)
//! - **Vector store integration** with multiple backends
//! - **Hybrid search** (vector + keyword + graph)
//! - **Answer generation** with source citations
//! - **Knowledge graph** for structured relationships
//!
//! ## What OpenFang Has
//!
//! - `openfang-memory` (3,924 LOC) — SQLite-backed conversation memory
//! - `openfang-runtime::embedding` — Basic embedding support
//! - Knowledge graph tools (`kg_add`, `kg_query`, `kg_search`)
//! - BUT: No document ingestion, no chunking, no RAG pipeline
//!
//! ## What This Crate Provides
//!
//! A RAG pipeline that uses Rig.rs's `VectorStoreIndex` trait and
//! `EmbeddingModel` trait to provide document ingestion, chunking,
//! and retrieval-augmented generation.
//!
//! ## HONEST GAPS
//!
//! - No knowledge graph integration (OpenFang has one, but it's separate)
//! - No hybrid search (vector only, no keyword fallback)
//! - No web crawling or connector framework
//! - Chunking is basic (fixed-size only, no semantic chunking)
//! - No incremental re-indexing (full re-index required on updates)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

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

/// A chunk of a document after splitting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub id: String,
    pub document_id: String,
    pub content: String,
    pub chunk_index: usize,
    pub metadata: serde_json::Value,
}

/// Configuration for chunking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkConfig {
    pub strategy: ChunkStrategy,
    pub chunk_size: usize,
    pub chunk_overlap: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkStrategy {
    FixedSize,
    // TODO: Semantic, Recursive, Markdown-aware
}

/// Trait for the knowledge store backend.
///
/// Wraps Rig.rs's VectorStoreIndex with document-level operations.
#[async_trait]
pub trait KnowledgeStore: Send + Sync {
    /// Ingest a document (chunk, embed, store).
    async fn ingest(&self, doc: Document, config: &ChunkConfig) -> anyhow::Result<()>;

    /// Search for relevant chunks given a query.
    async fn search(&self, query: &str, top_k: usize) -> anyhow::Result<Vec<Chunk>>;

    /// Delete a document and its chunks.
    async fn delete(&self, document_id: &str) -> anyhow::Result<()>;

    /// Get document count.
    async fn count(&self) -> anyhow::Result<usize>;
}
