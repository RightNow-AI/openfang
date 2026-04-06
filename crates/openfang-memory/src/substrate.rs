//! MemorySubstrate: unified implementation of the `Memory` trait.
//!
//! Composes the structured store, semantic store, knowledge store,
//! session store, and consolidation engine behind a single async API.
//!
//! Backends are selected via `MemoryConfig::backend`:
//! - `"sqlite"` (default): all stores backed by a shared SQLite connection
//! - `"http"`: semantic store routes to HTTP gateway, everything else SQLite
//! - `"postgres"`: all stores backed by PostgreSQL + pgvector
//! - `"qdrant"`: semantic via Qdrant, everything else SQLite
//! - `"postgres+qdrant"`: structured/sessions/usage via PostgreSQL, semantic via Qdrant
//!
//! This file is 100% backend-agnostic — zero rusqlite imports.

use crate::backends::{
    AuditBackend, ConsolidationBackend, PairedDevicesBackend, SessionBackend, TaskQueueBackend,
    UsageBackend,
};
use crate::session::Session;

use async_trait::async_trait;
use openfang_types::agent::{AgentEntry, AgentId, SessionId};
use openfang_types::config::MemoryConfig;
use openfang_types::embedding::EmbeddingDriver;
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::memory::{
    ConsolidationReport, Entity, ExportFormat, GraphMatch, GraphPattern, ImportReport, Memory,
    MemoryFilter, MemoryFragment, MemoryId, MemorySource, Relation,
};
use openfang_types::storage::{KnowledgeBackend, SemanticBackend, StructuredBackend};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn};

/// The unified memory substrate. Implements the `Memory` trait by delegating
/// to specialized stores backed by pluggable backends.
///
/// When an `EmbeddingDriver` is set, `remember()` and `recall()` automatically
/// generate embeddings — callers don't need to handle embedding themselves.
pub struct MemorySubstrate {
    structured: Arc<dyn StructuredBackend>,
    semantic: Arc<dyn SemanticBackend>,
    knowledge: Arc<dyn KnowledgeBackend>,
    sessions: Arc<dyn SessionBackend>,
    usage: Arc<dyn UsageBackend>,
    paired_devices: Arc<dyn PairedDevicesBackend>,
    task_queue: Arc<dyn TaskQueueBackend>,
    consolidation: Option<Arc<dyn ConsolidationBackend>>,
    audit: Option<Arc<dyn AuditBackend>>,
    embedding_driver: Option<Arc<dyn EmbeddingDriver>>,
}

impl MemorySubstrate {
    /// Open or create a memory substrate.
    ///
    /// `backend` selects structured/session/usage/etc. storage: `"sqlite"` or `"postgres"`.
    /// `semantic_backend` selects vector search independently: `"sqlite"`, `"postgres"`, `"qdrant"`, or `"http"`.
    /// When `semantic_backend` is not set, it follows the main `backend`.
    pub fn open(
        db_path: &Path,
        decay_rate: f32,
        memory_config: &MemoryConfig,
    ) -> OpenFangResult<Self> {
        #[cfg(feature = "postgres")]
        if memory_config.backend == "postgres" {
            return Self::open_postgres(memory_config, decay_rate);
        }

        Self::open_sqlite(db_path, decay_rate, memory_config)
    }

    /// Open a SQLite-backed memory substrate.
    fn open_sqlite(
        db_path: &Path,
        decay_rate: f32,
        memory_config: &MemoryConfig,
    ) -> OpenFangResult<Self> {
        let backend = crate::sqlite::SqliteBackend::open(db_path)?;
        let default_semantic: Arc<dyn SemanticBackend> = Arc::new(backend.semantic());
        let semantic = Self::select_semantic(memory_config, default_semantic)?;

        Ok(Self {
            structured: Arc::new(backend.structured()),
            semantic,
            knowledge: Arc::new(backend.knowledge()),
            sessions: Arc::new(backend.session()),
            usage: Arc::new(backend.usage()),
            paired_devices: Arc::new(backend.paired_devices()),
            task_queue: Arc::new(backend.task_queue()),
            consolidation: Some(Arc::new(backend.consolidation(decay_rate))),
            audit: Some(Arc::new(backend.audit())),
            embedding_driver: None,
        })
    }

    /// Open a PostgreSQL-backed memory substrate.
    #[cfg(feature = "postgres")]
    fn open_postgres(memory_config: &MemoryConfig, decay_rate: f32) -> OpenFangResult<Self> {
        let pg_url = memory_config.postgres_url.as_deref().ok_or_else(|| {
            OpenFangError::Memory("postgres backend requires postgres_url in config".to_string())
        })?;
        let pool = crate::postgres::create_pool(pg_url, memory_config.postgres_pool_size)?;

        tokio::runtime::Handle::current()
            .block_on(crate::postgres::run_migrations(&pool))?;

        info!(url = %pg_url, pool_size = memory_config.postgres_pool_size, "PostgreSQL memory backend connected");

        let backend = crate::postgres::PgBackend::new(pool);
        let default_semantic: Arc<dyn SemanticBackend> = Arc::new(backend.semantic());
        let semantic = Self::select_semantic(memory_config, default_semantic)?;

        Ok(Self {
            structured: Arc::new(backend.structured()),
            semantic,
            knowledge: Arc::new(backend.knowledge()),
            sessions: Arc::new(backend.session()),
            usage: Arc::new(backend.usage()),
            paired_devices: Arc::new(backend.paired_devices()),
            task_queue: Arc::new(backend.task_queue()),
            consolidation: Some(Arc::new(backend.consolidation().with_decay_rate(decay_rate))),
            audit: Some(Arc::new(backend.audit())),
            embedding_driver: None,
        })
    }

    /// Select the semantic backend based on `semantic_backend` config.
    /// Falls back to the main `backend` if `semantic_backend` is not set.
    fn select_semantic(
        config: &MemoryConfig,
        default: Arc<dyn SemanticBackend>,
    ) -> OpenFangResult<Arc<dyn SemanticBackend>> {
        let sem = config
            .semantic_backend
            .as_deref()
            .unwrap_or(config.backend.as_str());

        match sem {
            #[cfg(feature = "qdrant")]
            "qdrant" => {
                let url = config.qdrant_url.as_deref().unwrap_or("http://localhost:6334");
                let api_key = config
                    .qdrant_api_key_env
                    .as_deref()
                    .and_then(|env_var| std::env::var(env_var).ok());
                match crate::qdrant::QdrantSemanticStore::new(
                    url,
                    api_key.as_deref(),
                    &config.qdrant_collection,
                ) {
                    Ok(store) => {
                        info!(url = %url, collection = %config.qdrant_collection, "Qdrant semantic backend");
                        Ok(Arc::new(store))
                    }
                    Err(e) => {
                        warn!(error = %e, "Qdrant unavailable, using default semantic backend");
                        Ok(default)
                    }
                }
            }
            #[cfg(feature = "http-memory")]
            "http" => {
                let (url, token_env) = match (&config.http_url, &config.http_token_env) {
                    (Some(u), Some(t)) => (u, t),
                    _ => {
                        warn!("semantic_backend=http but http_url/http_token_env not set, using default");
                        return Ok(default);
                    }
                };
                match crate::http::MemoryApiClient::new(url, token_env) {
                    Ok(client) => {
                        match client.health_check() {
                            Ok(()) => info!(url = %url, "HTTP semantic backend connected"),
                            Err(e) => warn!(url = %url, error = %e, "HTTP semantic health check failed, will retry"),
                        }
                        Ok(Arc::new(crate::http::HttpSemanticStore::new(client, default)))
                    }
                    Err(e) => {
                        warn!(error = %e, "HTTP client failed, using default semantic backend");
                        Ok(default)
                    }
                }
            }
            _ => Ok(default),
        }
    }

    /// Create an in-memory substrate (for testing). Always uses SQLite backend.
    pub fn open_in_memory(decay_rate: f32) -> OpenFangResult<Self> {
        let backend = crate::sqlite::SqliteBackend::open_in_memory()?;

        Ok(Self {
            structured: Arc::new(backend.structured()),
            semantic: Arc::new(backend.semantic()),
            knowledge: Arc::new(backend.knowledge()),
            sessions: Arc::new(backend.session()),
            usage: Arc::new(backend.usage()),
            paired_devices: Arc::new(backend.paired_devices()),
            task_queue: Arc::new(backend.task_queue()),
            consolidation: Some(Arc::new(backend.consolidation(decay_rate))),
            audit: Some(Arc::new(backend.audit())),
            embedding_driver: None,
        })
    }

    /// Set the embedding driver for automatic embedding generation.
    pub fn set_embedding_driver(&mut self, driver: Option<Arc<dyn EmbeddingDriver>>) {
        self.embedding_driver = driver;
    }

    // -----------------------------------------------------------------
    // Usage accessors
    // -----------------------------------------------------------------

    /// Get a reference to the usage backend.
    pub fn usage(&self) -> &dyn UsageBackend {
        self.usage.as_ref()
    }

    /// Get a shared-ownership handle to the usage backend.
    pub fn usage_arc(&self) -> Arc<dyn UsageBackend> {
        Arc::clone(&self.usage)
    }

    /// Get the audit backend, if available.
    pub fn audit(&self) -> Option<Arc<dyn AuditBackend>> {
        self.audit.clone()
    }

    // -----------------------------------------------------------------
    // Agent persistence
    // -----------------------------------------------------------------

    /// Save an agent entry to persistent storage.
    pub fn save_agent(&self, entry: &AgentEntry) -> OpenFangResult<()> {
        self.structured.save_agent(entry)
    }

    /// Load an agent entry from persistent storage.
    pub fn load_agent(&self, agent_id: AgentId) -> OpenFangResult<Option<AgentEntry>> {
        self.structured.load_agent(agent_id)
    }

    /// Remove an agent from persistent storage and cascade-delete sessions.
    pub fn remove_agent(&self, agent_id: AgentId) -> OpenFangResult<()> {
        // Delete associated sessions first
        let _ = self.sessions.delete_agent_sessions(agent_id);
        self.structured.remove_agent(agent_id)
    }

    /// Load all agent entries from persistent storage.
    pub fn load_all_agents(&self) -> OpenFangResult<Vec<AgentEntry>> {
        self.structured.load_all_agents()
    }

    /// List all saved agents.
    pub fn list_agents(&self) -> OpenFangResult<Vec<(String, String, String)>> {
        self.structured.list_agents()
    }

    // -----------------------------------------------------------------
    // Structured KV store
    // -----------------------------------------------------------------

    /// Synchronous get from the structured store (for kernel handle use).
    pub fn structured_get(
        &self,
        agent_id: AgentId,
        key: &str,
    ) -> OpenFangResult<Option<serde_json::Value>> {
        self.structured.get(agent_id, key)
    }

    /// List all KV pairs for an agent.
    pub fn list_kv(&self, agent_id: AgentId) -> OpenFangResult<Vec<(String, serde_json::Value)>> {
        self.structured.list_kv(agent_id)
    }

    /// Delete a KV entry for an agent.
    pub fn structured_delete(&self, agent_id: AgentId, key: &str) -> OpenFangResult<()> {
        self.structured.delete(agent_id, key)
    }

    /// Synchronous set in the structured store (for kernel handle use).
    pub fn structured_set(
        &self,
        agent_id: AgentId,
        key: &str,
        value: serde_json::Value,
    ) -> OpenFangResult<()> {
        self.structured.set(agent_id, key, value)
    }

    // -----------------------------------------------------------------
    // Session operations
    // -----------------------------------------------------------------

    /// Get a session by ID.
    pub fn get_session(&self, session_id: SessionId) -> OpenFangResult<Option<Session>> {
        self.sessions.get_session(session_id)
    }

    /// Save a session.
    pub fn save_session(&self, session: &Session) -> OpenFangResult<()> {
        self.sessions.save_session(session)
    }

    /// Save a session asynchronously — runs the write in a blocking
    /// thread so the tokio runtime stays responsive.
    pub async fn save_session_async(&self, session: &Session) -> OpenFangResult<()> {
        let sessions = Arc::clone(&self.sessions);
        let session = session.clone();
        tokio::task::spawn_blocking(move || sessions.save_session(&session))
            .await
            .map_err(|e| OpenFangError::Internal(e.to_string()))?
    }

    /// Create a new empty session for an agent.
    pub fn create_session(&self, agent_id: AgentId) -> OpenFangResult<Session> {
        self.sessions.create_session(agent_id)
    }

    /// List all sessions with metadata.
    pub fn list_sessions(&self) -> OpenFangResult<Vec<serde_json::Value>> {
        self.sessions.list_sessions()
    }

    /// Delete a session by ID.
    pub fn delete_session(&self, session_id: SessionId) -> OpenFangResult<()> {
        self.sessions.delete_session(session_id)
    }

    /// Delete all sessions belonging to an agent.
    pub fn delete_agent_sessions(&self, agent_id: AgentId) -> OpenFangResult<()> {
        self.sessions.delete_agent_sessions(agent_id)
    }

    /// Delete the canonical (cross-channel) session for an agent.
    pub fn delete_canonical_session(&self, agent_id: AgentId) -> OpenFangResult<()> {
        self.sessions.delete_canonical_session(agent_id)
    }

    /// Set or clear a session label.
    pub fn set_session_label(
        &self,
        session_id: SessionId,
        label: Option<&str>,
    ) -> OpenFangResult<()> {
        self.sessions.set_session_label(session_id, label)
    }

    /// Find a session by label for a given agent.
    pub fn find_session_by_label(
        &self,
        agent_id: AgentId,
        label: &str,
    ) -> OpenFangResult<Option<Session>> {
        self.sessions.find_session_by_label(agent_id, label)
    }

    /// List all sessions for a specific agent.
    pub fn list_agent_sessions(&self, agent_id: AgentId) -> OpenFangResult<Vec<serde_json::Value>> {
        self.sessions.list_agent_sessions(agent_id)
    }

    /// Create a new session with an optional label.
    pub fn create_session_with_label(
        &self,
        agent_id: AgentId,
        label: Option<&str>,
    ) -> OpenFangResult<Session> {
        self.sessions.create_session_with_label(agent_id, label)
    }

    /// Load canonical session context for cross-channel memory.
    ///
    /// Returns the compacted summary (if any) and recent messages from the
    /// agent's persistent canonical session.
    pub fn canonical_context(
        &self,
        agent_id: AgentId,
        window_size: Option<usize>,
    ) -> OpenFangResult<(Option<String>, Vec<openfang_types::message::Message>)> {
        self.sessions.canonical_context(agent_id, window_size)
    }

    /// Store an LLM-generated summary, replacing older messages with the kept subset.
    ///
    /// Used by the compactor to replace text-truncation compaction with an
    /// LLM-generated summary of older conversation history.
    pub fn store_llm_summary(
        &self,
        agent_id: AgentId,
        summary: &str,
        kept_messages: Vec<openfang_types::message::Message>,
    ) -> OpenFangResult<()> {
        self.sessions
            .store_llm_summary(agent_id, summary, kept_messages)
    }

    /// Append messages to the agent's canonical session for cross-channel persistence.
    pub fn append_canonical(
        &self,
        agent_id: AgentId,
        messages: &[openfang_types::message::Message],
        compaction_threshold: Option<usize>,
    ) -> OpenFangResult<()> {
        self.sessions
            .append_canonical(agent_id, messages, compaction_threshold)?;
        Ok(())
    }

    // -----------------------------------------------------------------
    // Paired devices persistence
    // -----------------------------------------------------------------

    /// Load all paired devices from the database.
    pub fn load_paired_devices(&self) -> OpenFangResult<Vec<serde_json::Value>> {
        self.paired_devices.load_paired_devices()
    }

    /// Save a paired device to the database (insert or replace).
    pub fn save_paired_device(
        &self,
        device_id: &str,
        display_name: &str,
        platform: &str,
        paired_at: &str,
        last_seen: &str,
        push_token: Option<&str>,
    ) -> OpenFangResult<()> {
        self.paired_devices
            .save_paired_device(device_id, display_name, platform, paired_at, last_seen, push_token)
    }

    /// Remove a paired device from the database.
    pub fn remove_paired_device(&self, device_id: &str) -> OpenFangResult<()> {
        self.paired_devices.remove_paired_device(device_id)
    }

    // -----------------------------------------------------------------
    // Embedding-aware memory operations
    // -----------------------------------------------------------------

    /// Store a memory with an embedding vector.
    pub fn remember_with_embedding(
        &self,
        agent_id: AgentId,
        content: &str,
        source: MemorySource,
        scope: &str,
        metadata: HashMap<String, serde_json::Value>,
        embedding: Option<&[f32]>,
    ) -> OpenFangResult<MemoryId> {
        self.semantic
            .remember(agent_id, content, source, scope, metadata, embedding)
    }

    /// Recall memories using vector similarity when a query embedding is provided.
    pub fn recall_with_embedding(
        &self,
        query: &str,
        limit: usize,
        filter: Option<MemoryFilter>,
        query_embedding: Option<&[f32]>,
    ) -> OpenFangResult<Vec<MemoryFragment>> {
        self.semantic
            .recall(query, limit, filter, query_embedding)
    }

    /// Update the embedding for an existing memory.
    pub fn update_embedding(&self, id: MemoryId, embedding: &[f32]) -> OpenFangResult<()> {
        self.semantic.update_embedding(id, embedding)
    }

    /// Async wrapper for `recall_with_embedding` — runs in a blocking thread.
    pub async fn recall_with_embedding_async(
        &self,
        query: &str,
        limit: usize,
        filter: Option<MemoryFilter>,
        query_embedding: Option<&[f32]>,
    ) -> OpenFangResult<Vec<MemoryFragment>> {
        let store = Arc::clone(&self.semantic);
        let query = query.to_string();
        let embedding_owned = query_embedding.map(|e| e.to_vec());
        tokio::task::spawn_blocking(move || {
            store.recall(&query, limit, filter, embedding_owned.as_deref())
        })
        .await
        .map_err(|e| OpenFangError::Internal(e.to_string()))?
    }

    /// Async wrapper for `remember_with_embedding` — runs in a blocking thread.
    pub async fn remember_with_embedding_async(
        &self,
        agent_id: AgentId,
        content: &str,
        source: MemorySource,
        scope: &str,
        metadata: HashMap<String, serde_json::Value>,
        embedding: Option<&[f32]>,
    ) -> OpenFangResult<MemoryId> {
        let store = Arc::clone(&self.semantic);
        let content = content.to_string();
        let scope = scope.to_string();
        let embedding_owned = embedding.map(|e| e.to_vec());
        tokio::task::spawn_blocking(move || {
            store.remember(
                agent_id,
                &content,
                source,
                &scope,
                metadata,
                embedding_owned.as_deref(),
            )
        })
        .await
        .map_err(|e| OpenFangError::Internal(e.to_string()))?
    }

    // -----------------------------------------------------------------
    // Task queue operations
    // -----------------------------------------------------------------

    /// Post a new task to the shared queue. Returns the task ID.
    pub async fn task_post(
        &self,
        title: &str,
        description: &str,
        assigned_to: Option<&str>,
        created_by: Option<&str>,
    ) -> OpenFangResult<String> {
        let tq = Arc::clone(&self.task_queue);
        let title = title.to_string();
        let description = description.to_string();
        let assigned_to = assigned_to.unwrap_or("").to_string();
        let created_by = created_by.unwrap_or("").to_string();

        tokio::task::spawn_blocking(move || tq.task_post(&title, &description, &assigned_to, &created_by))
            .await
            .map_err(|e| OpenFangError::Internal(e.to_string()))?
    }

    /// Claim the next pending task (optionally for a specific assignee). Returns task JSON or None.
    pub async fn task_claim(&self, agent_id: &str) -> OpenFangResult<Option<serde_json::Value>> {
        let tq = Arc::clone(&self.task_queue);
        let agent_id = agent_id.to_string();

        tokio::task::spawn_blocking(move || tq.task_claim(&agent_id))
            .await
            .map_err(|e| OpenFangError::Internal(e.to_string()))?
    }

    /// Mark a task as completed with a result string.
    pub async fn task_complete(&self, task_id: &str, result: &str) -> OpenFangResult<()> {
        let tq = Arc::clone(&self.task_queue);
        let task_id = task_id.to_string();
        let result = result.to_string();

        tokio::task::spawn_blocking(move || tq.task_complete(&task_id, &result))
            .await
            .map_err(|e| OpenFangError::Internal(e.to_string()))?
    }

    /// List tasks, optionally filtered by status.
    pub async fn task_list(&self, status: Option<&str>) -> OpenFangResult<Vec<serde_json::Value>> {
        let tq = Arc::clone(&self.task_queue);
        let status = status.map(|s| s.to_string());

        tokio::task::spawn_blocking(move || tq.task_list(status.as_deref()))
            .await
            .map_err(|e| OpenFangError::Internal(e.to_string()))?
    }

    // -----------------------------------------------------------------
    // JSONL mirror
    // -----------------------------------------------------------------

    /// Write a human-readable JSONL mirror of a session to disk.
    ///
    /// Best-effort — errors are returned but should be logged,
    /// never affecting the primary store.
    pub fn write_jsonl_mirror(
        &self,
        session: &Session,
        sessions_dir: &Path,
    ) -> Result<(), std::io::Error> {
        crate::jsonl::write_session_mirror(session, sessions_dir)
    }
}

#[async_trait]
impl Memory for MemorySubstrate {
    async fn get(&self, agent_id: AgentId, key: &str) -> OpenFangResult<Option<serde_json::Value>> {
        let store = Arc::clone(&self.structured);
        let key = key.to_string();
        tokio::task::spawn_blocking(move || store.get(agent_id, &key))
            .await
            .map_err(|e| OpenFangError::Internal(e.to_string()))?
    }

    async fn set(
        &self,
        agent_id: AgentId,
        key: &str,
        value: serde_json::Value,
    ) -> OpenFangResult<()> {
        let store = Arc::clone(&self.structured);
        let key = key.to_string();
        tokio::task::spawn_blocking(move || store.set(agent_id, &key, value))
            .await
            .map_err(|e| OpenFangError::Internal(e.to_string()))?
    }

    async fn delete(&self, agent_id: AgentId, key: &str) -> OpenFangResult<()> {
        let store = Arc::clone(&self.structured);
        let key = key.to_string();
        tokio::task::spawn_blocking(move || store.delete(agent_id, &key))
            .await
            .map_err(|e| OpenFangError::Internal(e.to_string()))?
    }

    async fn remember(
        &self,
        agent_id: AgentId,
        content: &str,
        source: MemorySource,
        scope: &str,
        metadata: HashMap<String, serde_json::Value>,
    ) -> OpenFangResult<MemoryId> {
        // Auto-embed if driver is available
        let embedding = if let Some(ref driver) = self.embedding_driver {
            match driver.embed_one(content).await {
                Ok(vec) => Some(vec),
                Err(e) => {
                    warn!("Auto-embedding failed, storing without embedding: {e}");
                    None
                }
            }
        } else {
            None
        };

        let store = Arc::clone(&self.semantic);
        let content = content.to_string();
        let scope = scope.to_string();
        tokio::task::spawn_blocking(move || {
            store.remember(
                agent_id,
                &content,
                source,
                &scope,
                metadata,
                embedding.as_deref(),
            )
        })
        .await
        .map_err(|e| OpenFangError::Internal(e.to_string()))?
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        filter: Option<MemoryFilter>,
    ) -> OpenFangResult<Vec<MemoryFragment>> {
        // Auto-embed query if driver is available
        let query_embedding = if let Some(ref driver) = self.embedding_driver {
            match driver.embed_one(query).await {
                Ok(vec) => Some(vec),
                Err(e) => {
                    warn!("Auto-embedding for recall failed, using text fallback: {e}");
                    None
                }
            }
        } else {
            None
        };

        let store = Arc::clone(&self.semantic);
        let query = query.to_string();
        tokio::task::spawn_blocking(move || {
            store.recall(&query, limit, filter, query_embedding.as_deref())
        })
        .await
        .map_err(|e| OpenFangError::Internal(e.to_string()))?
    }

    async fn forget(&self, id: MemoryId) -> OpenFangResult<()> {
        let store = Arc::clone(&self.semantic);
        tokio::task::spawn_blocking(move || store.forget(id))
            .await
            .map_err(|e| OpenFangError::Internal(e.to_string()))?
    }

    async fn add_entity(&self, entity: Entity) -> OpenFangResult<String> {
        let store = Arc::clone(&self.knowledge);
        tokio::task::spawn_blocking(move || store.add_entity(entity))
            .await
            .map_err(|e| OpenFangError::Internal(e.to_string()))?
    }

    async fn add_relation(&self, relation: Relation) -> OpenFangResult<String> {
        let store = Arc::clone(&self.knowledge);
        tokio::task::spawn_blocking(move || store.add_relation(relation))
            .await
            .map_err(|e| OpenFangError::Internal(e.to_string()))?
    }

    async fn query_graph(&self, pattern: GraphPattern) -> OpenFangResult<Vec<GraphMatch>> {
        let store = Arc::clone(&self.knowledge);
        tokio::task::spawn_blocking(move || store.query_graph(pattern))
            .await
            .map_err(|e| OpenFangError::Internal(e.to_string()))?
    }

    async fn consolidate(&self) -> OpenFangResult<ConsolidationReport> {
        if let Some(ref engine) = self.consolidation {
            let engine = Arc::clone(engine);
            tokio::task::spawn_blocking(move || engine.consolidate())
                .await
                .map_err(|e| OpenFangError::Internal(e.to_string()))?
        } else {
            // Non-SQLite backends: consolidation not yet implemented
            Ok(ConsolidationReport {
                memories_decayed: 0,
                memories_merged: 0,
                duration_ms: 0,
            })
        }
    }

    async fn export(&self, _format: ExportFormat) -> OpenFangResult<Vec<u8>> {
        Ok(Vec::new())
    }

    async fn import(&self, _data: &[u8], _format: ExportFormat) -> OpenFangResult<ImportReport> {
        Ok(ImportReport {
            entities_imported: 0,
            relations_imported: 0,
            memories_imported: 0,
            errors: vec!["Import not yet implemented in Phase 1".to_string()],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_substrate_kv() {
        let substrate = MemorySubstrate::open_in_memory(0.1).unwrap();
        let agent_id = AgentId::new();
        substrate
            .set(agent_id, "key", serde_json::json!("value"))
            .await
            .unwrap();
        let val = substrate.get(agent_id, "key").await.unwrap();
        assert_eq!(val, Some(serde_json::json!("value")));
    }

    #[tokio::test]
    async fn test_substrate_remember_recall() {
        let substrate = MemorySubstrate::open_in_memory(0.1).unwrap();
        let agent_id = AgentId::new();
        substrate
            .remember(
                agent_id,
                "Rust is a great language",
                MemorySource::Conversation,
                "episodic",
                HashMap::new(),
            )
            .await
            .unwrap();
        let results = substrate.recall("Rust", 10, None).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_task_post_and_list() {
        let substrate = MemorySubstrate::open_in_memory(0.1).unwrap();
        let id = substrate
            .task_post(
                "Review code",
                "Check the auth module for issues",
                Some("auditor"),
                Some("orchestrator"),
            )
            .await
            .unwrap();
        assert!(!id.is_empty());

        let tasks = substrate.task_list(Some("pending")).await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0]["title"], "Review code");
        assert_eq!(tasks[0]["assigned_to"], "auditor");
        assert_eq!(tasks[0]["status"], "pending");
    }

    #[tokio::test]
    async fn test_task_claim_and_complete() {
        let substrate = MemorySubstrate::open_in_memory(0.1).unwrap();
        let task_id = substrate
            .task_post(
                "Audit endpoint",
                "Security audit the /api/login endpoint",
                Some("auditor"),
                None,
            )
            .await
            .unwrap();

        // Claim the task
        let claimed = substrate.task_claim("auditor").await.unwrap();
        assert!(claimed.is_some());
        let claimed = claimed.unwrap();
        assert_eq!(claimed["id"], task_id);
        assert_eq!(claimed["status"], "in_progress");

        // Complete the task
        substrate
            .task_complete(&task_id, "No vulnerabilities found")
            .await
            .unwrap();

        // Verify it shows as completed
        let tasks = substrate.task_list(Some("completed")).await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0]["result"], "No vulnerabilities found");
    }

    #[tokio::test]
    async fn test_task_claim_empty() {
        let substrate = MemorySubstrate::open_in_memory(0.1).unwrap();
        let claimed = substrate.task_claim("nobody").await.unwrap();
        assert!(claimed.is_none());
    }
}
