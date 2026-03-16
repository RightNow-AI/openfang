//! MemorySubstrate: unified implementation of the `Memory` trait.
//!
//! Composes the structured store, semantic store, knowledge store,
//! session store, and consolidation engine behind a single async API.
//! Supports both SQLite and MongoDB backends via `BackendInner` dispatch.

use crate::consolidation::ConsolidationEngine;
use crate::knowledge::KnowledgeStore;
use crate::migration::run_migrations;
use crate::mongo::MongoBackend;
use crate::semantic::SemanticStore;
use crate::session::{Session, SessionStore};
use crate::structured::StructuredStore;
use crate::usage::UsageStore;

use async_trait::async_trait;
use openfang_types::agent::{AgentEntry, AgentId, SessionId};
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::memory::{
    ConsolidationReport, Entity, ExportFormat, GraphMatch, GraphPattern, ImportReport, Memory,
    MemoryFilter, MemoryFragment, MemoryId, MemorySource, Relation,
};
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Internal backend discriminator.
enum BackendInner {
    Sqlite {
        conn: Arc<Mutex<Connection>>,
        structured: StructuredStore,
        semantic: SemanticStore,
        knowledge: KnowledgeStore,
        sessions: SessionStore,
        consolidation: ConsolidationEngine,
        usage: UsageStore,
    },
    Mongo(MongoBackend),
}

/// Helper: run an async future from a sync context on the current tokio runtime.
fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(f))
}

/// The unified memory substrate. Implements the `Memory` trait by delegating
/// to specialized stores backed by either SQLite or MongoDB.
pub struct MemorySubstrate {
    inner: BackendInner,
}

impl MemorySubstrate {
    /// Open or create a SQLite-backed memory substrate at the given database path.
    pub fn open(db_path: &Path, decay_rate: f32) -> OpenFangResult<Self> {
        let conn = Connection::open(db_path).map_err(|e| OpenFangError::Memory(e.to_string()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        run_migrations(&conn).map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let shared = Arc::new(Mutex::new(conn));

        Ok(Self {
            inner: BackendInner::Sqlite {
                conn: Arc::clone(&shared),
                structured: StructuredStore::new(Arc::clone(&shared)),
                semantic: SemanticStore::new(Arc::clone(&shared)),
                knowledge: KnowledgeStore::new(Arc::clone(&shared)),
                sessions: SessionStore::new(Arc::clone(&shared)),
                usage: UsageStore::new(Arc::clone(&shared)),
                consolidation: ConsolidationEngine::new(shared, decay_rate),
            },
        })
    }

    /// Open a MongoDB-backed memory substrate.
    pub async fn open_mongo(
        mongo_url: &str,
        db_name: &str,
        decay_rate: f32,
    ) -> OpenFangResult<Self> {
        let client = mongodb::Client::with_uri_str(mongo_url)
            .await
            .map_err(|e| OpenFangError::Memory(format!("MongoDB connection failed: {e}")))?;
        let db = client.database(db_name);
        crate::mongo::indexes::ensure_indexes(&db).await?;
        Ok(Self {
            inner: BackendInner::Mongo(MongoBackend::new(db, decay_rate)),
        })
    }

    /// Open a memory substrate from configuration — dispatches to SQLite or MongoDB.
    pub async fn open_with_config(
        config: &openfang_types::config::MemoryConfig,
    ) -> OpenFangResult<Self> {
        match config.backend.as_str() {
            "mongodb" => {
                Self::open_mongo(&config.mongo_url, &config.mongo_db_name, config.decay_rate)
                    .await
            }
            _ => {
                let db_path = config
                    .sqlite_path
                    .clone()
                    .expect("sqlite_path must be set when backend is sqlite");
                Self::open(&db_path, config.decay_rate)
            }
        }
    }

    /// Create an in-memory SQLite substrate (for testing).
    pub fn open_in_memory(decay_rate: f32) -> OpenFangResult<Self> {
        let conn =
            Connection::open_in_memory().map_err(|e| OpenFangError::Memory(e.to_string()))?;
        run_migrations(&conn).map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let shared = Arc::new(Mutex::new(conn));

        Ok(Self {
            inner: BackendInner::Sqlite {
                conn: Arc::clone(&shared),
                structured: StructuredStore::new(Arc::clone(&shared)),
                semantic: SemanticStore::new(Arc::clone(&shared)),
                knowledge: KnowledgeStore::new(Arc::clone(&shared)),
                sessions: SessionStore::new(Arc::clone(&shared)),
                usage: UsageStore::new(Arc::clone(&shared)),
                consolidation: ConsolidationEngine::new(shared, decay_rate),
            },
        })
    }

    /// Get a reference to the usage store (SQLite only).
    pub fn usage(&self) -> Option<&UsageStore> {
        match &self.inner {
            BackendInner::Sqlite { usage, .. } => Some(usage),
            BackendInner::Mongo(_) => None,
        }
    }

    /// Get the shared database connection (SQLite only, for external UsageStore/AuditLog).
    pub fn usage_conn(&self) -> Option<Arc<Mutex<Connection>>> {
        match &self.inner {
            BackendInner::Sqlite { conn, .. } => Some(Arc::clone(conn)),
            BackendInner::Mongo(_) => None,
        }
    }

    /// Get the MongoDB database handle (MongoDB only, for external components).
    pub fn mongo_db(&self) -> Option<mongodb::Database> {
        match &self.inner {
            BackendInner::Sqlite { .. } => None,
            BackendInner::Mongo(m) => Some(m.db.clone()),
        }
    }

    /// Returns true if using the MongoDB backend.
    pub fn is_mongo(&self) -> bool {
        matches!(&self.inner, BackendInner::Mongo(_))
    }

    /// Create a backend-appropriate `UsageStore` instance.
    ///
    /// Returns a `UsageStore` backed by either SQLite or MongoDB depending
    /// on which backend is active.
    pub fn create_usage_store(&self) -> UsageStore {
        match &self.inner {
            BackendInner::Sqlite { conn, .. } => UsageStore::new(Arc::clone(conn)),
            BackendInner::Mongo(m) => UsageStore::from_mongo(m.usage.clone()),
        }
    }

    // -----------------------------------------------------------------
    // Agent CRUD
    // -----------------------------------------------------------------

    pub fn save_agent(&self, entry: &AgentEntry) -> OpenFangResult<()> {
        match &self.inner {
            BackendInner::Sqlite { structured, .. } => structured.save_agent(entry),
            BackendInner::Mongo(m) => block_on(m.structured.save_agent(entry)),
        }
    }

    pub fn load_agent(&self, agent_id: AgentId) -> OpenFangResult<Option<AgentEntry>> {
        match &self.inner {
            BackendInner::Sqlite { structured, .. } => structured.load_agent(agent_id),
            BackendInner::Mongo(m) => block_on(m.structured.load_agent(agent_id)),
        }
    }

    pub fn remove_agent(&self, agent_id: AgentId) -> OpenFangResult<()> {
        match &self.inner {
            BackendInner::Sqlite {
                structured,
                sessions,
                ..
            } => {
                let _ = sessions.delete_agent_sessions(agent_id);
                structured.remove_agent(agent_id)
            }
            BackendInner::Mongo(m) => block_on(async {
                let _ = m.sessions.delete_agent_sessions(agent_id).await;
                m.structured.remove_agent(agent_id).await
            }),
        }
    }

    pub fn load_all_agents(&self) -> OpenFangResult<Vec<AgentEntry>> {
        match &self.inner {
            BackendInner::Sqlite { structured, .. } => structured.load_all_agents(),
            BackendInner::Mongo(m) => block_on(m.structured.load_all_agents()),
        }
    }

    pub fn list_agents(&self) -> OpenFangResult<Vec<(String, String, String)>> {
        match &self.inner {
            BackendInner::Sqlite { structured, .. } => structured.list_agents(),
            BackendInner::Mongo(m) => block_on(m.structured.list_agents()),
        }
    }

    // -----------------------------------------------------------------
    // Structured KV
    // -----------------------------------------------------------------

    pub fn structured_get(
        &self,
        agent_id: AgentId,
        key: &str,
    ) -> OpenFangResult<Option<serde_json::Value>> {
        match &self.inner {
            BackendInner::Sqlite { structured, .. } => structured.get(agent_id, key),
            BackendInner::Mongo(m) => block_on(m.structured.get(agent_id, key)),
        }
    }

    pub fn list_kv(&self, agent_id: AgentId) -> OpenFangResult<Vec<(String, serde_json::Value)>> {
        match &self.inner {
            BackendInner::Sqlite { structured, .. } => structured.list_kv(agent_id),
            BackendInner::Mongo(m) => block_on(m.structured.list_kv(agent_id)),
        }
    }

    pub fn structured_delete(&self, agent_id: AgentId, key: &str) -> OpenFangResult<()> {
        match &self.inner {
            BackendInner::Sqlite { structured, .. } => structured.delete(agent_id, key),
            BackendInner::Mongo(m) => block_on(m.structured.delete(agent_id, key)),
        }
    }

    pub fn structured_set(
        &self,
        agent_id: AgentId,
        key: &str,
        value: serde_json::Value,
    ) -> OpenFangResult<()> {
        match &self.inner {
            BackendInner::Sqlite { structured, .. } => structured.set(agent_id, key, value),
            BackendInner::Mongo(m) => block_on(m.structured.set(agent_id, key, value)),
        }
    }

    // -----------------------------------------------------------------
    // Sessions
    // -----------------------------------------------------------------

    pub fn get_session(&self, session_id: SessionId) -> OpenFangResult<Option<Session>> {
        match &self.inner {
            BackendInner::Sqlite { sessions, .. } => sessions.get_session(session_id),
            BackendInner::Mongo(m) => block_on(m.sessions.get_session(session_id)),
        }
    }

    pub fn save_session(&self, session: &Session) -> OpenFangResult<()> {
        match &self.inner {
            BackendInner::Sqlite { sessions, .. } => sessions.save_session(session),
            BackendInner::Mongo(m) => block_on(m.sessions.save_session(session)),
        }
    }

    pub fn create_session(&self, agent_id: AgentId) -> OpenFangResult<Session> {
        match &self.inner {
            BackendInner::Sqlite { sessions, .. } => sessions.create_session(agent_id),
            BackendInner::Mongo(m) => block_on(m.sessions.create_session(agent_id)),
        }
    }

    pub fn list_sessions(&self) -> OpenFangResult<Vec<serde_json::Value>> {
        match &self.inner {
            BackendInner::Sqlite { sessions, .. } => sessions.list_sessions(),
            BackendInner::Mongo(m) => block_on(m.sessions.list_sessions()),
        }
    }

    pub fn delete_session(&self, session_id: SessionId) -> OpenFangResult<()> {
        match &self.inner {
            BackendInner::Sqlite { sessions, .. } => sessions.delete_session(session_id),
            BackendInner::Mongo(m) => block_on(m.sessions.delete_session(session_id)),
        }
    }

    pub fn delete_agent_sessions(&self, agent_id: AgentId) -> OpenFangResult<()> {
        match &self.inner {
            BackendInner::Sqlite { sessions, .. } => sessions.delete_agent_sessions(agent_id),
            BackendInner::Mongo(m) => block_on(m.sessions.delete_agent_sessions(agent_id)),
        }
    }

    pub fn delete_canonical_session(&self, agent_id: AgentId) -> OpenFangResult<()> {
        match &self.inner {
            BackendInner::Sqlite { sessions, .. } => sessions.delete_canonical_session(agent_id),
            BackendInner::Mongo(m) => block_on(m.sessions.delete_canonical_session(agent_id)),
        }
    }

    pub fn set_session_label(
        &self,
        session_id: SessionId,
        label: Option<&str>,
    ) -> OpenFangResult<()> {
        match &self.inner {
            BackendInner::Sqlite { sessions, .. } => {
                sessions.set_session_label(session_id, label)
            }
            BackendInner::Mongo(m) => block_on(m.sessions.set_session_label(session_id, label)),
        }
    }

    pub fn find_session_by_label(
        &self,
        agent_id: AgentId,
        label: &str,
    ) -> OpenFangResult<Option<Session>> {
        match &self.inner {
            BackendInner::Sqlite { sessions, .. } => {
                sessions.find_session_by_label(agent_id, label)
            }
            BackendInner::Mongo(m) => {
                block_on(m.sessions.find_session_by_label(agent_id, label))
            }
        }
    }

    pub fn list_agent_sessions(&self, agent_id: AgentId) -> OpenFangResult<Vec<serde_json::Value>> {
        match &self.inner {
            BackendInner::Sqlite { sessions, .. } => sessions.list_agent_sessions(agent_id),
            BackendInner::Mongo(m) => block_on(m.sessions.list_agent_sessions(agent_id)),
        }
    }

    pub fn create_session_with_label(
        &self,
        agent_id: AgentId,
        label: Option<&str>,
    ) -> OpenFangResult<Session> {
        match &self.inner {
            BackendInner::Sqlite { sessions, .. } => {
                sessions.create_session_with_label(agent_id, label)
            }
            BackendInner::Mongo(m) => {
                block_on(m.sessions.create_session_with_label(agent_id, label))
            }
        }
    }

    pub fn canonical_context(
        &self,
        agent_id: AgentId,
        window_size: Option<usize>,
    ) -> OpenFangResult<(Option<String>, Vec<openfang_types::message::Message>)> {
        match &self.inner {
            BackendInner::Sqlite { sessions, .. } => {
                sessions.canonical_context(agent_id, window_size)
            }
            BackendInner::Mongo(m) => {
                block_on(m.sessions.canonical_context(agent_id, window_size))
            }
        }
    }

    pub fn store_llm_summary(
        &self,
        agent_id: AgentId,
        summary: &str,
        kept_messages: Vec<openfang_types::message::Message>,
    ) -> OpenFangResult<()> {
        match &self.inner {
            BackendInner::Sqlite { sessions, .. } => {
                sessions.store_llm_summary(agent_id, summary, kept_messages)
            }
            BackendInner::Mongo(m) => {
                block_on(m.sessions.store_llm_summary(agent_id, summary, kept_messages))
            }
        }
    }

    pub fn write_jsonl_mirror(
        &self,
        session: &Session,
        sessions_dir: &Path,
    ) -> Result<(), std::io::Error> {
        match &self.inner {
            BackendInner::Sqlite { sessions, .. } => {
                sessions.write_jsonl_mirror(session, sessions_dir)
            }
            BackendInner::Mongo(m) => m.sessions.write_jsonl_mirror(session, sessions_dir),
        }
    }

    pub fn append_canonical(
        &self,
        agent_id: AgentId,
        messages: &[openfang_types::message::Message],
        compaction_threshold: Option<usize>,
    ) -> OpenFangResult<()> {
        match &self.inner {
            BackendInner::Sqlite { sessions, .. } => {
                sessions.append_canonical(agent_id, messages, compaction_threshold)?;
                Ok(())
            }
            BackendInner::Mongo(m) => {
                block_on(m.sessions.append_canonical(agent_id, messages, compaction_threshold))?;
                Ok(())
            }
        }
    }

    // -----------------------------------------------------------------
    // Paired devices persistence
    // -----------------------------------------------------------------

    pub fn load_paired_devices(&self) -> OpenFangResult<Vec<serde_json::Value>> {
        match &self.inner {
            BackendInner::Sqlite { conn, .. } => {
                let conn = conn
                    .lock()
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;
                let mut stmt = conn.prepare(
                    "SELECT device_id, display_name, platform, paired_at, last_seen, push_token FROM paired_devices"
                ).map_err(|e| OpenFangError::Memory(e.to_string()))?;
                let rows = stmt
                    .query_map([], |row| {
                        Ok(serde_json::json!({
                            "device_id": row.get::<_, String>(0)?,
                            "display_name": row.get::<_, String>(1)?,
                            "platform": row.get::<_, String>(2)?,
                            "paired_at": row.get::<_, String>(3)?,
                            "last_seen": row.get::<_, String>(4)?,
                            "push_token": row.get::<_, Option<String>>(5)?,
                        }))
                    })
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;
                let mut devices = Vec::new();
                for row in rows {
                    devices.push(row.map_err(|e| OpenFangError::Memory(e.to_string()))?);
                }
                Ok(devices)
            }
            BackendInner::Mongo(m) => block_on(m.load_paired_devices()),
        }
    }

    pub fn save_paired_device(
        &self,
        device_id: &str,
        display_name: &str,
        platform: &str,
        paired_at: &str,
        last_seen: &str,
        push_token: Option<&str>,
    ) -> OpenFangResult<()> {
        match &self.inner {
            BackendInner::Sqlite { conn, .. } => {
                let conn = conn
                    .lock()
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;
                conn.execute(
                    "INSERT OR REPLACE INTO paired_devices (device_id, display_name, platform, paired_at, last_seen, push_token) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![device_id, display_name, platform, paired_at, last_seen, push_token],
                ).map_err(|e| OpenFangError::Memory(e.to_string()))?;
                Ok(())
            }
            BackendInner::Mongo(m) => block_on(
                m.save_paired_device(device_id, display_name, platform, paired_at, last_seen, push_token),
            ),
        }
    }

    pub fn remove_paired_device(&self, device_id: &str) -> OpenFangResult<()> {
        match &self.inner {
            BackendInner::Sqlite { conn, .. } => {
                let conn = conn
                    .lock()
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;
                conn.execute(
                    "DELETE FROM paired_devices WHERE device_id = ?1",
                    rusqlite::params![device_id],
                )
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
                Ok(())
            }
            BackendInner::Mongo(m) => block_on(m.remove_paired_device(device_id)),
        }
    }

    // -----------------------------------------------------------------
    // Embedding-aware memory operations
    // -----------------------------------------------------------------

    pub fn remember_with_embedding(
        &self,
        agent_id: AgentId,
        content: &str,
        source: MemorySource,
        scope: &str,
        metadata: HashMap<String, serde_json::Value>,
        embedding: Option<&[f32]>,
    ) -> OpenFangResult<MemoryId> {
        match &self.inner {
            BackendInner::Sqlite { semantic, .. } => {
                semantic.remember_with_embedding(agent_id, content, source, scope, metadata, embedding)
            }
            BackendInner::Mongo(m) => {
                block_on(m.semantic.remember_with_embedding(agent_id, content, source, scope, metadata, embedding))
            }
        }
    }

    pub fn recall_with_embedding(
        &self,
        query: &str,
        limit: usize,
        filter: Option<MemoryFilter>,
        query_embedding: Option<&[f32]>,
    ) -> OpenFangResult<Vec<MemoryFragment>> {
        match &self.inner {
            BackendInner::Sqlite { semantic, .. } => {
                semantic.recall_with_embedding(query, limit, filter, query_embedding)
            }
            BackendInner::Mongo(m) => {
                block_on(m.semantic.recall_with_embedding(query, limit, filter, query_embedding))
            }
        }
    }

    pub fn update_embedding(&self, id: MemoryId, embedding: &[f32]) -> OpenFangResult<()> {
        match &self.inner {
            BackendInner::Sqlite { semantic, .. } => semantic.update_embedding(id, embedding),
            BackendInner::Mongo(m) => block_on(m.semantic.update_embedding(id, embedding)),
        }
    }

    pub async fn recall_with_embedding_async(
        &self,
        query: &str,
        limit: usize,
        filter: Option<MemoryFilter>,
        query_embedding: Option<&[f32]>,
    ) -> OpenFangResult<Vec<MemoryFragment>> {
        match &self.inner {
            BackendInner::Sqlite { semantic, .. } => {
                let store = semantic.clone();
                let query = query.to_string();
                let embedding_owned = query_embedding.map(|e| e.to_vec());
                tokio::task::spawn_blocking(move || {
                    store.recall_with_embedding(&query, limit, filter, embedding_owned.as_deref())
                })
                .await
                .map_err(|e| OpenFangError::Internal(e.to_string()))?
            }
            BackendInner::Mongo(m) => {
                m.semantic
                    .recall_with_embedding(query, limit, filter, query_embedding)
                    .await
            }
        }
    }

    pub async fn remember_with_embedding_async(
        &self,
        agent_id: AgentId,
        content: &str,
        source: MemorySource,
        scope: &str,
        metadata: HashMap<String, serde_json::Value>,
        embedding: Option<&[f32]>,
    ) -> OpenFangResult<MemoryId> {
        match &self.inner {
            BackendInner::Sqlite { semantic, .. } => {
                let store = semantic.clone();
                let content = content.to_string();
                let scope = scope.to_string();
                let embedding_owned = embedding.map(|e| e.to_vec());
                tokio::task::spawn_blocking(move || {
                    store.remember_with_embedding(
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
            BackendInner::Mongo(m) => {
                m.semantic
                    .remember_with_embedding(agent_id, content, source, scope, metadata, embedding)
                    .await
            }
        }
    }

    // -----------------------------------------------------------------
    // Task queue operations
    // -----------------------------------------------------------------

    pub async fn task_post(
        &self,
        title: &str,
        description: &str,
        assigned_to: Option<&str>,
        created_by: Option<&str>,
    ) -> OpenFangResult<String> {
        match &self.inner {
            BackendInner::Sqlite { conn, .. } => {
                let conn = Arc::clone(conn);
                let title = title.to_string();
                let description = description.to_string();
                let assigned_to = assigned_to.unwrap_or("").to_string();
                let created_by = created_by.unwrap_or("").to_string();

                tokio::task::spawn_blocking(move || {
                    let id = uuid::Uuid::new_v4().to_string();
                    let now = chrono::Utc::now().to_rfc3339();
                    let db = conn.lock().map_err(|e| OpenFangError::Internal(e.to_string()))?;
                    db.execute(
                        "INSERT INTO task_queue (id, agent_id, task_type, payload, status, priority, created_at, title, description, assigned_to, created_by)
                         VALUES (?1, ?2, ?3, ?4, 'pending', 0, ?5, ?6, ?7, ?8, ?9)",
                        rusqlite::params![id, &created_by, &title, b"", now, title, description, assigned_to, created_by],
                    )
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;
                    Ok(id)
                })
                .await
                .map_err(|e| OpenFangError::Internal(e.to_string()))?
            }
            BackendInner::Mongo(m) => {
                m.task_post(title, description, assigned_to, created_by).await
            }
        }
    }

    pub async fn task_claim(&self, agent_id: &str) -> OpenFangResult<Option<serde_json::Value>> {
        match &self.inner {
            BackendInner::Sqlite { conn, .. } => {
                let conn = Arc::clone(conn);
                let agent_id = agent_id.to_string();

                tokio::task::spawn_blocking(move || {
                    let db = conn.lock().map_err(|e| OpenFangError::Internal(e.to_string()))?;
                    let mut stmt = db.prepare(
                        "SELECT id, title, description, assigned_to, created_by, created_at
                         FROM task_queue
                         WHERE status = 'pending' AND (assigned_to = ?1 OR assigned_to = '')
                         ORDER BY priority DESC, created_at ASC
                         LIMIT 1"
                    ).map_err(|e| OpenFangError::Memory(e.to_string()))?;

                    let result = stmt.query_row(rusqlite::params![agent_id], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, String>(5)?,
                        ))
                    });

                    match result {
                        Ok((id, title, description, assigned, created_by, created_at)) => {
                            db.execute(
                                "UPDATE task_queue SET status = 'in_progress', assigned_to = ?2 WHERE id = ?1",
                                rusqlite::params![id, agent_id],
                            ).map_err(|e| OpenFangError::Memory(e.to_string()))?;

                            Ok(Some(serde_json::json!({
                                "id": id,
                                "title": title,
                                "description": description,
                                "status": "in_progress",
                                "assigned_to": if assigned.is_empty() { &agent_id } else { &assigned },
                                "created_by": created_by,
                                "created_at": created_at,
                            })))
                        }
                        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                        Err(e) => Err(OpenFangError::Memory(e.to_string())),
                    }
                })
                .await
                .map_err(|e| OpenFangError::Internal(e.to_string()))?
            }
            BackendInner::Mongo(m) => m.task_claim(agent_id).await,
        }
    }

    pub async fn task_complete(&self, task_id: &str, result: &str) -> OpenFangResult<()> {
        match &self.inner {
            BackendInner::Sqlite { conn, .. } => {
                let conn = Arc::clone(conn);
                let task_id = task_id.to_string();
                let result = result.to_string();

                tokio::task::spawn_blocking(move || {
                    let now = chrono::Utc::now().to_rfc3339();
                    let db = conn.lock().map_err(|e| OpenFangError::Internal(e.to_string()))?;
                    let rows = db.execute(
                        "UPDATE task_queue SET status = 'completed', result = ?2, completed_at = ?3 WHERE id = ?1",
                        rusqlite::params![task_id, result, now],
                    ).map_err(|e| OpenFangError::Memory(e.to_string()))?;
                    if rows == 0 {
                        return Err(OpenFangError::Internal(format!("Task not found: {task_id}")));
                    }
                    Ok(())
                })
                .await
                .map_err(|e| OpenFangError::Internal(e.to_string()))?
            }
            BackendInner::Mongo(m) => m.task_complete(task_id, result).await,
        }
    }

    pub async fn task_list(&self, status: Option<&str>) -> OpenFangResult<Vec<serde_json::Value>> {
        match &self.inner {
            BackendInner::Sqlite { conn, .. } => {
                let conn = Arc::clone(conn);
                let status = status.map(|s| s.to_string());

                tokio::task::spawn_blocking(move || {
                    let db = conn.lock().map_err(|e| OpenFangError::Internal(e.to_string()))?;
                    let (sql, params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = match &status {
                        Some(s) => (
                            "SELECT id, title, description, status, assigned_to, created_by, created_at, completed_at, result FROM task_queue WHERE status = ?1 ORDER BY created_at DESC",
                            vec![Box::new(s.clone())],
                        ),
                        None => (
                            "SELECT id, title, description, status, assigned_to, created_by, created_at, completed_at, result FROM task_queue ORDER BY created_at DESC",
                            vec![],
                        ),
                    };

                    let mut stmt = db.prepare(sql).map_err(|e| OpenFangError::Memory(e.to_string()))?;
                    let params_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
                    let rows = stmt.query_map(params_refs.as_slice(), |row| {
                        Ok(serde_json::json!({
                            "id": row.get::<_, String>(0)?,
                            "title": row.get::<_, String>(1).unwrap_or_default(),
                            "description": row.get::<_, String>(2).unwrap_or_default(),
                            "status": row.get::<_, String>(3)?,
                            "assigned_to": row.get::<_, String>(4).unwrap_or_default(),
                            "created_by": row.get::<_, String>(5).unwrap_or_default(),
                            "created_at": row.get::<_, String>(6).unwrap_or_default(),
                            "completed_at": row.get::<_, Option<String>>(7).unwrap_or(None),
                            "result": row.get::<_, Option<String>>(8).unwrap_or(None),
                        }))
                    }).map_err(|e| OpenFangError::Memory(e.to_string()))?;

                    let mut tasks = Vec::new();
                    for row in rows {
                        tasks.push(row.map_err(|e| OpenFangError::Memory(e.to_string()))?);
                    }
                    Ok(tasks)
                })
                .await
                .map_err(|e| OpenFangError::Internal(e.to_string()))?
            }
            BackendInner::Mongo(m) => m.task_list(status).await,
        }
    }
}

#[async_trait]
impl Memory for MemorySubstrate {
    async fn get(&self, agent_id: AgentId, key: &str) -> OpenFangResult<Option<serde_json::Value>> {
        match &self.inner {
            BackendInner::Sqlite { structured, .. } => {
                let store = structured.clone();
                let key = key.to_string();
                tokio::task::spawn_blocking(move || store.get(agent_id, &key))
                    .await
                    .map_err(|e| OpenFangError::Internal(e.to_string()))?
            }
            BackendInner::Mongo(m) => m.structured.get(agent_id, key).await,
        }
    }

    async fn set(
        &self,
        agent_id: AgentId,
        key: &str,
        value: serde_json::Value,
    ) -> OpenFangResult<()> {
        match &self.inner {
            BackendInner::Sqlite { structured, .. } => {
                let store = structured.clone();
                let key = key.to_string();
                tokio::task::spawn_blocking(move || store.set(agent_id, &key, value))
                    .await
                    .map_err(|e| OpenFangError::Internal(e.to_string()))?
            }
            BackendInner::Mongo(m) => m.structured.set(agent_id, key, value).await,
        }
    }

    async fn delete(&self, agent_id: AgentId, key: &str) -> OpenFangResult<()> {
        match &self.inner {
            BackendInner::Sqlite { structured, .. } => {
                let store = structured.clone();
                let key = key.to_string();
                tokio::task::spawn_blocking(move || store.delete(agent_id, &key))
                    .await
                    .map_err(|e| OpenFangError::Internal(e.to_string()))?
            }
            BackendInner::Mongo(m) => m.structured.delete(agent_id, key).await,
        }
    }

    async fn remember(
        &self,
        agent_id: AgentId,
        content: &str,
        source: MemorySource,
        scope: &str,
        metadata: HashMap<String, serde_json::Value>,
    ) -> OpenFangResult<MemoryId> {
        match &self.inner {
            BackendInner::Sqlite { semantic, .. } => {
                let store = semantic.clone();
                let content = content.to_string();
                let scope = scope.to_string();
                tokio::task::spawn_blocking(move || {
                    store.remember(agent_id, &content, source, &scope, metadata)
                })
                .await
                .map_err(|e| OpenFangError::Internal(e.to_string()))?
            }
            BackendInner::Mongo(m) => {
                m.semantic
                    .remember(agent_id, content, source, scope, metadata)
                    .await
            }
        }
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        filter: Option<MemoryFilter>,
    ) -> OpenFangResult<Vec<MemoryFragment>> {
        match &self.inner {
            BackendInner::Sqlite { semantic, .. } => {
                let store = semantic.clone();
                let query = query.to_string();
                tokio::task::spawn_blocking(move || store.recall(&query, limit, filter))
                    .await
                    .map_err(|e| OpenFangError::Internal(e.to_string()))?
            }
            BackendInner::Mongo(m) => m.semantic.recall(query, limit, filter).await,
        }
    }

    async fn forget(&self, id: MemoryId) -> OpenFangResult<()> {
        match &self.inner {
            BackendInner::Sqlite { semantic, .. } => {
                let store = semantic.clone();
                tokio::task::spawn_blocking(move || store.forget(id))
                    .await
                    .map_err(|e| OpenFangError::Internal(e.to_string()))?
            }
            BackendInner::Mongo(m) => m.semantic.forget(id).await,
        }
    }

    async fn add_entity(&self, entity: Entity) -> OpenFangResult<String> {
        match &self.inner {
            BackendInner::Sqlite { knowledge, .. } => {
                let store = knowledge.clone();
                tokio::task::spawn_blocking(move || store.add_entity(entity))
                    .await
                    .map_err(|e| OpenFangError::Internal(e.to_string()))?
            }
            BackendInner::Mongo(m) => m.knowledge.add_entity(entity).await,
        }
    }

    async fn add_relation(&self, relation: Relation) -> OpenFangResult<String> {
        match &self.inner {
            BackendInner::Sqlite { knowledge, .. } => {
                let store = knowledge.clone();
                tokio::task::spawn_blocking(move || store.add_relation(relation))
                    .await
                    .map_err(|e| OpenFangError::Internal(e.to_string()))?
            }
            BackendInner::Mongo(m) => m.knowledge.add_relation(relation).await,
        }
    }

    async fn query_graph(&self, pattern: GraphPattern) -> OpenFangResult<Vec<GraphMatch>> {
        match &self.inner {
            BackendInner::Sqlite { knowledge, .. } => {
                let store = knowledge.clone();
                tokio::task::spawn_blocking(move || store.query_graph(pattern))
                    .await
                    .map_err(|e| OpenFangError::Internal(e.to_string()))?
            }
            BackendInner::Mongo(m) => m.knowledge.query_graph(pattern).await,
        }
    }

    async fn consolidate(&self) -> OpenFangResult<ConsolidationReport> {
        match &self.inner {
            BackendInner::Sqlite { consolidation, .. } => {
                let engine = consolidation.clone();
                tokio::task::spawn_blocking(move || engine.consolidate())
                    .await
                    .map_err(|e| OpenFangError::Internal(e.to_string()))?
            }
            BackendInner::Mongo(m) => m.consolidation.consolidate().await,
        }
    }

    async fn export(&self, format: ExportFormat) -> OpenFangResult<Vec<u8>> {
        let _ = format;
        Ok(Vec::new())
    }

    async fn import(&self, _data: &[u8], _format: ExportFormat) -> OpenFangResult<ImportReport> {
        Ok(ImportReport {
            entities_imported: 0,
            relations_imported: 0,
            memories_imported: 0,
            errors: vec!["Import not yet implemented".to_string()],
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

        let claimed = substrate.task_claim("auditor").await.unwrap();
        assert!(claimed.is_some());
        let claimed = claimed.unwrap();
        assert_eq!(claimed["id"], task_id);
        assert_eq!(claimed["status"], "in_progress");

        substrate
            .task_complete(&task_id, "No vulnerabilities found")
            .await
            .unwrap();

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
