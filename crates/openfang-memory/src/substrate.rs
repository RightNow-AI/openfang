//! MemorySubstrate: unified implementation of the `Memory` trait.
//!
//! Composes the structured store, semantic store, knowledge store,
//! session store, and consolidation engine behind a single async API.

use crate::consolidation::ConsolidationEngine;
use crate::knowledge::KnowledgeStore;
use crate::migration::run_migrations;
use crate::semantic::SemanticStore;
use crate::session::{Session, SessionExtraction, SessionExtractionStore, SessionStore};
use crate::structured::StructuredStore;
use crate::usage::UsageStore;
use crate::user_agent_memory::{
    UserAgentMemoryStore, UserAgentMemoryTopic, UserAgentTopicIndexEntry,
};
use crate::user_memory::{MemoryTopic, TopicIndexEntry, UserMemoryStore};

use async_trait::async_trait;
use openfang_types::agent::{AgentEntry, AgentId, SessionId, UserId};
use openfang_types::config::MemoryConfig;
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::memory::{
    ConsolidationReport, Entity, ExportFormat, GraphMatch, GraphPattern, ImportReport, Memory,
    MemoryFilter, MemoryFragment, MemoryId, MemorySource, Relation,
};
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

/// Per-bucket row counts returned by [`MemorySubstrate::wipe_user`].
///
/// Mirrors the response body of `DELETE /api/users/:user_id/memory` so the
/// handler can forward the struct verbatim. All three buckets are reported
/// even when zero so the UI can render "0 deleted" rather than guessing.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WipeUserCounts {
    /// Rows deleted from `user_memory_topics`.
    pub topics_deleted: usize,
    /// Rows deleted from `user_agent_memory_topics`.
    pub agent_topics_deleted: usize,
    /// Rows deleted from `session_extractions` (audit log).
    pub extractions_deleted: usize,
}

/// Row shape returned by [`MemorySubstrate::list_user_extraction_audit`].
///
/// Fields, in order: `extraction_id`, `session_id`, `agent_id`,
/// `created_at_rfc3339`, `session_deleted`. The control-API handler maps
/// this to a JSON entry; keeping the substrate-layer return as a tuple
/// keeps the substrate JSON-free.
pub type ExtractionAuditRow = (String, String, String, String, bool);

/// The unified memory substrate. Implements the `Memory` trait by delegating
/// to specialized stores backed by a shared SQLite connection.
pub struct MemorySubstrate {
    conn: Arc<Mutex<Connection>>,
    structured: StructuredStore,
    semantic: SemanticStore,
    knowledge: KnowledgeStore,
    sessions: SessionStore,
    extractions: SessionExtractionStore,
    user_memory: UserMemoryStore,
    user_agent_memory: UserAgentMemoryStore,
    consolidation: ConsolidationEngine,
    usage: UsageStore,
}

impl MemorySubstrate {
    /// Open or create a memory substrate at the given database path.
    ///
    /// When `memory_config.backend == "http"` and `http_url`/`http_token_env` are set,
    /// the semantic store routes `remember`/`recall` to the memory-api gateway.
    /// All other stores (KV, knowledge graph, sessions) remain local SQLite.
    pub fn open(
        db_path: &Path,
        decay_rate: f32,
        memory_config: &MemoryConfig,
    ) -> OpenFangResult<Self> {
        let conn = Connection::open(db_path).map_err(|e| OpenFangError::Memory(e.to_string()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        run_migrations(&conn).map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let shared = Arc::new(Mutex::new(conn));

        let semantic = Self::create_semantic_store(Arc::clone(&shared), memory_config);

        Ok(Self {
            conn: Arc::clone(&shared),
            structured: StructuredStore::new(Arc::clone(&shared)),
            semantic,
            knowledge: KnowledgeStore::new(Arc::clone(&shared)),
            sessions: SessionStore::new(Arc::clone(&shared)),
            extractions: SessionExtractionStore::new(Arc::clone(&shared)),
            user_memory: UserMemoryStore::new(Arc::clone(&shared)),
            user_agent_memory: UserAgentMemoryStore::new(Arc::clone(&shared)),
            usage: UsageStore::new(Arc::clone(&shared)),
            consolidation: ConsolidationEngine::new(shared, decay_rate),
        })
    }

    /// Create the semantic store, optionally with HTTP backend.
    fn create_semantic_store(
        conn: Arc<Mutex<Connection>>,
        memory_config: &MemoryConfig,
    ) -> SemanticStore {
        #[cfg(feature = "http-memory")]
        if memory_config.backend == "http" {
            if let (Some(url), Some(token_env)) =
                (&memory_config.http_url, &memory_config.http_token_env)
            {
                match crate::http_client::MemoryApiClient::new(url, token_env) {
                    Ok(client) => {
                        // Best-effort health check on startup
                        match client.health_check() {
                            Ok(()) => info!(url = %url, "HTTP memory backend connected"),
                            Err(e) => {
                                warn!(url = %url, error = %e, "HTTP memory backend health check failed, will retry on use")
                            }
                        }
                        return SemanticStore::new_with_http(conn, client);
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to create HTTP memory client, falling back to SQLite");
                    }
                }
            } else {
                warn!("backend=http but http_url/http_token_env not set, falling back to SQLite");
            }
        }

        #[cfg(not(feature = "http-memory"))]
        let _ = memory_config;

        SemanticStore::new(conn)
    }

    /// Create an in-memory substrate (for testing). Always uses SQLite backend.
    pub fn open_in_memory(decay_rate: f32) -> OpenFangResult<Self> {
        let conn =
            Connection::open_in_memory().map_err(|e| OpenFangError::Memory(e.to_string()))?;
        run_migrations(&conn).map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let shared = Arc::new(Mutex::new(conn));

        Ok(Self {
            conn: Arc::clone(&shared),
            structured: StructuredStore::new(Arc::clone(&shared)),
            semantic: SemanticStore::new(Arc::clone(&shared)),
            knowledge: KnowledgeStore::new(Arc::clone(&shared)),
            sessions: SessionStore::new(Arc::clone(&shared)),
            extractions: SessionExtractionStore::new(Arc::clone(&shared)),
            user_memory: UserMemoryStore::new(Arc::clone(&shared)),
            user_agent_memory: UserAgentMemoryStore::new(Arc::clone(&shared)),
            usage: UsageStore::new(Arc::clone(&shared)),
            consolidation: ConsolidationEngine::new(shared, decay_rate),
        })
    }

    /// Get a reference to the usage store.
    pub fn usage(&self) -> &UsageStore {
        &self.usage
    }

    /// Get the shared database connection (for constructing stores from outside).
    pub fn usage_conn(&self) -> Arc<Mutex<Connection>> {
        Arc::clone(&self.conn)
    }

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

    /// Get a session by ID.
    pub fn get_session(&self, session_id: SessionId) -> OpenFangResult<Option<Session>> {
        self.sessions.get_session(session_id)
    }

    /// Save a session.
    pub fn save_session(&self, session: &Session) -> OpenFangResult<()> {
        self.sessions.save_session(session)
    }

    /// Save a session asynchronously — runs the SQLite write in a blocking
    /// thread so the tokio runtime stays responsive.
    pub async fn save_session_async(&self, session: &Session) -> OpenFangResult<()> {
        let sessions = self.sessions.clone();
        let session = session.clone();
        tokio::task::spawn_blocking(move || sessions.save_session(&session))
            .await
            .map_err(|e| OpenFangError::Internal(e.to_string()))?
    }

    /// Create a new empty session for an agent owned by `user_id`.
    ///
    /// Callers without a specific user pass `default_user_id()` so the
    /// session attaches to the kernel's persistent default identity.
    pub fn create_session(&self, agent_id: AgentId, user_id: UserId) -> OpenFangResult<Session> {
        self.sessions.create_session(agent_id, user_id)
    }

    /// Rewrite all sessions that are still tagged with the nil-UUID owner
    /// (the legacy "anonymous bucket" default) to the given `target` user.
    ///
    /// Returns the number of sessions updated. Called once at kernel boot
    /// after the persistent default-user UUID is determined; subsequent calls
    /// are no-ops because no nil-UUID rows remain.
    ///
    /// The UPDATE runs inside an explicit transaction so a crash or lock
    /// failure mid-flight rolls back cleanly — the caller's bootstrap
    /// sentinel relies on this being all-or-nothing to avoid permanently
    /// stranding sessions on the nil bucket.
    pub fn rewrite_nil_user_sessions(&self, target: UserId) -> OpenFangResult<usize> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let nil_str = uuid::Uuid::nil().to_string();
        let target_str = target.0.to_string();

        let tx = conn
            .transaction()
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let sessions_updated = tx
            .execute(
                "UPDATE sessions SET user_id = ?1 WHERE user_id = ?2 OR user_id IS NULL",
                rusqlite::params![target_str, nil_str],
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        tx.commit()
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        info!(
            sessions_updated = sessions_updated,
            target = %target.0,
            "Rewrote nil-UUID sessions to persistent default user"
        );
        Ok(sessions_updated)
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

    /// Write a human-readable JSONL mirror of a session to disk.
    ///
    /// Best-effort — errors are returned but should be logged,
    /// never affecting the primary SQLite store.
    pub fn write_jsonl_mirror(
        &self,
        session: &Session,
        sessions_dir: &Path,
    ) -> Result<(), std::io::Error> {
        self.sessions.write_jsonl_mirror(session, sessions_dir)
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
    // Structured memory: extractions
    // -----------------------------------------------------------------

    /// Append a structured extraction for a session.
    ///
    /// No producer in this PR — this is the storage surface that PR 3's
    /// `extract_structured` calls into. Default agents never reach this
    /// path because the producer is gated on `MemoryConfig::is_structured()`.
    pub fn append_extraction(
        &self,
        session_id: SessionId,
        extraction: &SessionExtraction,
    ) -> OpenFangResult<()> {
        self.extractions.append(session_id, extraction)
    }

    /// Load all structured extractions for a session, oldest first.
    pub fn load_extractions(
        &self,
        session_id: SessionId,
    ) -> OpenFangResult<Vec<SessionExtraction>> {
        self.extractions.load_all(session_id)
    }

    /// Delete all extractions for a session.
    pub fn delete_extractions(&self, session_id: SessionId) -> OpenFangResult<()> {
        self.extractions.delete_for_session(session_id)
    }

    /// Delete every extraction event attributed to `user_id`.
    ///
    /// Used by the user-memory wipe endpoint so that "wipe all memory" actually
    /// wipes everything — the audit log is part of what the user is asking us
    /// to forget. Returns the number of rows deleted.
    ///
    /// Uses the denormalized `session_extractions.user_id` column, so rows
    /// orphaned by a prior session delete are still caught. The
    /// JOIN-through-sessions clause is kept as a belt-and-suspenders
    /// fallback in case a row was somehow inserted without the
    /// denormalized column populated.
    pub fn delete_user_extractions(&self, user_id: UserId) -> OpenFangResult<usize> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let uid = user_id.0.to_string();
        let n = conn
            .execute(
                "DELETE FROM session_extractions \
                 WHERE user_id = ?1 \
                    OR session_id IN (SELECT id FROM sessions WHERE user_id = ?1)",
                rusqlite::params![uid],
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(n)
    }

    /// List extraction events for a user, newest first, with a `session_deleted`
    /// flag derived from a LEFT JOIN against `sessions`.
    ///
    /// Returned tuple shape: `(extraction_id, session_id, agent_id,
    /// created_at_rfc3339, session_deleted)`. Caller is the control API,
    /// which serialises this to JSON.
    pub fn list_user_extraction_audit(
        &self,
        user_id: UserId,
        limit: usize,
    ) -> OpenFangResult<Vec<ExtractionAuditRow>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT se.id, se.session_id, se.agent_id, se.created_at, \
                        CASE WHEN s.id IS NULL THEN 1 ELSE 0 END AS deleted \
                 FROM session_extractions se \
                 LEFT JOIN sessions s ON s.id = se.session_id \
                 WHERE se.user_id = ?1 \
                 ORDER BY se.created_at DESC \
                 LIMIT ?2",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let rows = stmt
            .query_map(
                rusqlite::params![user_id.0.to_string(), limit as i64],
                |row| {
                    let id: String = row.get(0)?;
                    let sid: String = row.get(1)?;
                    let aid: String = row.get(2)?;
                    let created: String = row.get(3)?;
                    let deleted: i64 = row.get(4)?;
                    Ok((id, sid, aid, created, deleted != 0))
                },
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| OpenFangError::Memory(e.to_string()))?);
        }
        Ok(out)
    }

    // -----------------------------------------------------------------
    // Structured memory: per-user topics
    // -----------------------------------------------------------------

    /// Upsert a user memory topic.
    pub fn upsert_user_topic(&self, topic: &MemoryTopic) -> OpenFangResult<()> {
        self.user_memory.upsert_topic(topic)
    }

    /// Delete a single user memory topic (used by conflict resolution).
    pub fn delete_user_topic(&self, user_id: UserId, topic: &str) -> OpenFangResult<()> {
        self.user_memory.delete_topic(user_id, topic)
    }

    /// Get the per-user topic index (no content) for session-start injection.
    pub fn user_topic_index(&self, user_id: UserId) -> OpenFangResult<Vec<TopicIndexEntry>> {
        self.user_memory.get_index(user_id)
    }

    /// Fetch full content for one user memory topic. `None` when missing or
    /// expired.
    pub fn user_topic(&self, user_id: UserId, topic: &str) -> OpenFangResult<Option<MemoryTopic>> {
        self.user_memory.get_topic(user_id, topic)
    }

    /// Store an embedding for a user topic. No-op when the topic doesn't exist.
    pub fn store_user_topic_embedding(
        &self,
        user_id: UserId,
        topic: &str,
        embedding: &[f32],
    ) -> OpenFangResult<()> {
        self.user_memory.store_embedding(user_id, topic, embedding)
    }

    /// Search user topics by cosine similarity against a query embedding.
    pub fn search_user_topics_by_embedding(
        &self,
        user_id: UserId,
        query: &[f32],
        top_k: usize,
    ) -> OpenFangResult<Vec<String>> {
        self.user_memory.search_by_embedding(user_id, query, top_k)
    }

    /// Prune expired topics for a user (called opportunistically).
    pub fn prune_expired_user_topics(&self, user_id: UserId) -> OpenFangResult<usize> {
        self.user_memory.prune_expired(user_id)
    }

    // -----------------------------------------------------------------
    // Structured memory: per-(user, agent) topics
    // -----------------------------------------------------------------

    /// Upsert a per-(user, agent) memory topic.
    pub fn upsert_user_agent_topic(&self, topic: &UserAgentMemoryTopic) -> OpenFangResult<()> {
        self.user_agent_memory.upsert_topic(topic)
    }

    /// Get the per-(user, agent) topic index for session-start injection.
    pub fn user_agent_topic_index(
        &self,
        user_id: UserId,
        agent_id: AgentId,
    ) -> OpenFangResult<Vec<UserAgentTopicIndexEntry>> {
        self.user_agent_memory.get_index(user_id, agent_id)
    }

    /// Fetch full content for one per-(user, agent) memory topic.
    pub fn user_agent_topic(
        &self,
        user_id: UserId,
        agent_id: AgentId,
        topic: &str,
    ) -> OpenFangResult<Option<UserAgentMemoryTopic>> {
        self.user_agent_memory.get_topic(user_id, agent_id, topic)
    }

    /// Delete per-(user, agent) memory for one agent. Returns rows deleted.
    pub fn delete_user_agent_memory(
        &self,
        user_id: UserId,
        agent_id: AgentId,
    ) -> OpenFangResult<usize> {
        self.user_agent_memory
            .delete_user_agent_memory(user_id, agent_id)
    }

    // -----------------------------------------------------------------
    // Atomic user wipe (general + per-agent + audit)
    // -----------------------------------------------------------------

    /// Atomically wipe everything attributed to `user_id` across the three
    /// structured-memory buckets.
    ///
    /// The three DELETEs run inside a single SQLite transaction so a partial
    /// failure rolls back — callers never observe a half-wiped user.
    /// The previous shape (three independent single-table deletes with
    /// early-return on each error) could leave the user partially wiped if
    /// the second or third call failed. Returns a per-bucket count so the
    /// control-API handler can forward it verbatim.
    pub fn wipe_user(&self, user_id: UserId) -> OpenFangResult<WipeUserCounts> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let uid = user_id.0.to_string();
        let tx = conn
            .transaction()
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let topics_deleted = tx
            .execute(
                "DELETE FROM user_memory_topics WHERE user_id = ?1",
                rusqlite::params![uid],
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let agent_topics_deleted = tx
            .execute(
                "DELETE FROM user_agent_memory_topics WHERE user_id = ?1",
                rusqlite::params![uid],
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let extractions_deleted = tx
            .execute(
                "DELETE FROM session_extractions \
                 WHERE user_id = ?1 \
                    OR session_id IN (SELECT id FROM sessions WHERE user_id = ?1)",
                rusqlite::params![uid],
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        tx.commit()
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        Ok(WipeUserCounts {
            topics_deleted,
            agent_topics_deleted,
            extractions_deleted,
        })
    }

    /// Delete the session subtree rooted at `session_id` (the session itself
    /// plus any descendants linked via `parent_session_id`). Returns the
    /// number of sessions removed.
    ///
    /// Forks land in a later PR; this is wired here so the control API can
    /// surface a single recursive delete now without coupling to forks.
    pub fn delete_session_tree(&self, session_id: SessionId) -> OpenFangResult<usize> {
        self.sessions.delete_session_tree(session_id)
    }

    // -----------------------------------------------------------------
    // Paired devices persistence
    // -----------------------------------------------------------------

    /// Load all paired devices from the database.
    pub fn load_paired_devices(&self) -> OpenFangResult<Vec<serde_json::Value>> {
        let conn = self
            .conn
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
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO paired_devices (device_id, display_name, platform, paired_at, last_seen, push_token) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![device_id, display_name, platform, paired_at, last_seen, push_token],
        ).map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Remove a paired device from the database.
    pub fn remove_paired_device(&self, device_id: &str) -> OpenFangResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        conn.execute(
            "DELETE FROM paired_devices WHERE device_id = ?1",
            rusqlite::params![device_id],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
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
            .remember_with_embedding(agent_id, content, source, scope, metadata, embedding)
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
            .recall_with_embedding(query, limit, filter, query_embedding)
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
        let store = self.semantic.clone();
        let query = query.to_string();
        let embedding_owned = query_embedding.map(|e| e.to_vec());
        tokio::task::spawn_blocking(move || {
            store.recall_with_embedding(&query, limit, filter, embedding_owned.as_deref())
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
        let store = self.semantic.clone();
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
        let conn = Arc::clone(&self.conn);
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

    /// Claim the next pending task (optionally for a specific assignee). Returns task JSON or None.
    pub async fn task_claim(&self, agent_id: &str) -> OpenFangResult<Option<serde_json::Value>> {
        let conn = Arc::clone(&self.conn);
        let agent_id = agent_id.to_string();

        tokio::task::spawn_blocking(move || {
            let db = conn.lock().map_err(|e| OpenFangError::Internal(e.to_string()))?;
            // Find first pending task assigned to this agent, or any unassigned pending task
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
                    // Update status to in_progress
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

    /// Mark a task as completed with a result string.
    pub async fn task_complete(&self, task_id: &str, result: &str) -> OpenFangResult<()> {
        let conn = Arc::clone(&self.conn);
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

    /// List tasks, optionally filtered by status.
    pub async fn task_list(&self, status: Option<&str>) -> OpenFangResult<Vec<serde_json::Value>> {
        let conn = Arc::clone(&self.conn);
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
}

#[async_trait]
impl Memory for MemorySubstrate {
    async fn get(&self, agent_id: AgentId, key: &str) -> OpenFangResult<Option<serde_json::Value>> {
        let store = self.structured.clone();
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
        let store = self.structured.clone();
        let key = key.to_string();
        tokio::task::spawn_blocking(move || store.set(agent_id, &key, value))
            .await
            .map_err(|e| OpenFangError::Internal(e.to_string()))?
    }

    async fn delete(&self, agent_id: AgentId, key: &str) -> OpenFangResult<()> {
        let store = self.structured.clone();
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
        let store = self.semantic.clone();
        let content = content.to_string();
        let scope = scope.to_string();
        tokio::task::spawn_blocking(move || {
            store.remember(agent_id, &content, source, &scope, metadata)
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
        let store = self.semantic.clone();
        let query = query.to_string();
        tokio::task::spawn_blocking(move || store.recall(&query, limit, filter))
            .await
            .map_err(|e| OpenFangError::Internal(e.to_string()))?
    }

    async fn forget(&self, id: MemoryId) -> OpenFangResult<()> {
        let store = self.semantic.clone();
        tokio::task::spawn_blocking(move || store.forget(id))
            .await
            .map_err(|e| OpenFangError::Internal(e.to_string()))?
    }

    async fn add_entity(&self, entity: Entity) -> OpenFangResult<String> {
        let store = self.knowledge.clone();
        tokio::task::spawn_blocking(move || store.add_entity(entity))
            .await
            .map_err(|e| OpenFangError::Internal(e.to_string()))?
    }

    async fn add_relation(&self, relation: Relation) -> OpenFangResult<String> {
        let store = self.knowledge.clone();
        tokio::task::spawn_blocking(move || store.add_relation(relation))
            .await
            .map_err(|e| OpenFangError::Internal(e.to_string()))?
    }

    async fn query_graph(&self, pattern: GraphPattern) -> OpenFangResult<Vec<GraphMatch>> {
        let store = self.knowledge.clone();
        tokio::task::spawn_blocking(move || store.query_graph(pattern))
            .await
            .map_err(|e| OpenFangError::Internal(e.to_string()))?
    }

    async fn consolidate(&self) -> OpenFangResult<ConsolidationReport> {
        let engine = self.consolidation.clone();
        tokio::task::spawn_blocking(move || engine.consolidate())
            .await
            .map_err(|e| OpenFangError::Internal(e.to_string()))?
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
            errors: vec!["Import not yet implemented in Phase 1".to_string()],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::default_user_id;

    #[test]
    fn test_rewrite_nil_user_sessions_is_idempotent_and_targeted() {
        // The one-shot fixup must rewrite ONLY the nil-UUID rows and leave
        // every other session alone; a second call after the rewrite must
        // find zero matching rows and behave as a no-op.
        let substrate = MemorySubstrate::open_in_memory(0.1).unwrap();
        let agent_id = AgentId::new();
        let other_user = UserId::new();
        let target = UserId::new();
        let nil_user = UserId(uuid::Uuid::nil());

        // Two nil-bucket sessions (the legacy default) + one explicit-user session.
        let s1 = substrate
            .sessions
            .create_session(agent_id, nil_user)
            .unwrap();
        let s2 = substrate
            .sessions
            .create_session(agent_id, nil_user)
            .unwrap();
        let s3 = substrate
            .sessions
            .create_session(agent_id, other_user)
            .unwrap();

        let updated = substrate.rewrite_nil_user_sessions(target).unwrap();
        assert_eq!(
            updated, 2,
            "exactly the two nil-UUID sessions must be rewritten"
        );

        let s1 = substrate.sessions.get_session(s1.id).unwrap().unwrap();
        let s2 = substrate.sessions.get_session(s2.id).unwrap().unwrap();
        let s3 = substrate.sessions.get_session(s3.id).unwrap().unwrap();
        assert_eq!(s1.user_id, target);
        assert_eq!(s2.user_id, target);
        assert_eq!(
            s3.user_id, other_user,
            "non-nil sessions must NOT be touched"
        );

        // Second call is a no-op (no nil rows left).
        let updated2 = substrate.rewrite_nil_user_sessions(target).unwrap();
        assert_eq!(updated2, 0);
    }

    #[test]
    fn test_rewrite_nil_user_sessions_returns_err_when_table_missing() {
        // When the `sessions` table is missing, the UPDATE cannot run and the
        // caller must observe an error — not a silent success. This is what
        // lets `bootstrap_default_user` skip setting the
        // `default_user_bootstrap_done` sentinel on failure so the rewrite is
        // retried on the next boot.
        //
        // NOTE: This test does NOT exercise transactional atomicity. A single
        // `UPDATE` is implicitly all-or-nothing under SQLite, so there is no
        // partial-commit state to roll back from. True multi-statement
        // atomicity becomes testable in PR 2 when the `extractions` table
        // joins the same transaction — at that point we add a dedicated
        // `…_is_atomic_on_partial_failure` test that forces the second
        // statement to fail and verifies the first one is rolled back.
        let substrate = MemorySubstrate::open_in_memory(0.1).unwrap();
        let agent_id = AgentId::new();
        let target = UserId::new();
        let nil_user = UserId(uuid::Uuid::nil());

        let _s1 = substrate
            .sessions
            .create_session(agent_id, nil_user)
            .unwrap();

        // Drop the only table the UPDATE touches; the next call must error.
        {
            let conn = substrate.conn.lock().unwrap();
            conn.execute("DROP TABLE sessions", [])
                .expect("drop sessions");
        }

        let err = substrate.rewrite_nil_user_sessions(target);
        assert!(
            err.is_err(),
            "rewrite must fail when the UPDATE cannot complete"
        );
    }

    #[test]
    fn test_create_session_via_substrate_routes_user_id() {
        // The substrate's `create_session` proxy must forward the user_id
        // through to the session store rather than silently dropping it.
        let substrate = MemorySubstrate::open_in_memory(0.1).unwrap();
        let agent_id = AgentId::new();
        let user = UserId::new();

        let session = substrate.create_session(agent_id, user).unwrap();
        assert_eq!(session.user_id, user);

        let default_session = substrate
            .create_session(agent_id, default_user_id())
            .unwrap();
        assert_eq!(default_session.user_id, default_user_id());
    }

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

    // -----------------------------------------------------------------
    // wipe_user — atomic per-user delete across the three structured-memory
    // buckets. Must be scoped to the target user and never touch siblings.
    // -----------------------------------------------------------------

    fn seed_memory(substrate: &MemorySubstrate, user_id: UserId, agent_id: AgentId) {
        substrate
            .upsert_user_topic(&MemoryTopic {
                user_id,
                topic: "prefs".into(),
                summary: "summary".into(),
                content: "content".into(),
                updated_at: chrono::Utc::now(),
                expires_at: None,
            })
            .unwrap();
        substrate
            .upsert_user_agent_topic(&UserAgentMemoryTopic {
                user_id,
                agent_id,
                topic: "with-jeeves".into(),
                summary: "summary".into(),
                content: "content".into(),
                updated_at: chrono::Utc::now(),
            })
            .unwrap();
        let session = substrate
            .sessions
            .create_session(agent_id, user_id)
            .unwrap();
        substrate
            .append_extraction(
                session.id,
                &SessionExtraction {
                    facts: vec!["a fact".into()],
                    ..Default::default()
                },
            )
            .unwrap();
    }

    #[test]
    fn test_wipe_user_returns_per_bucket_counts() {
        let substrate = MemorySubstrate::open_in_memory(0.1).unwrap();
        let user = UserId::new();
        let agent = AgentId::new();
        seed_memory(&substrate, user, agent);

        let counts = substrate.wipe_user(user).unwrap();
        assert_eq!(counts.topics_deleted, 1);
        assert_eq!(counts.agent_topics_deleted, 1);
        assert_eq!(counts.extractions_deleted, 1);

        // Confirm the buckets are actually empty.
        assert!(substrate.user_topic_index(user).unwrap().is_empty());
        assert!(substrate
            .user_agent_topic_index(user, agent)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn test_wipe_user_is_scoped_to_target() {
        // Wiping user A must not touch user B's data in any of the three
        // buckets.
        let substrate = MemorySubstrate::open_in_memory(0.1).unwrap();
        let agent = AgentId::new();
        let user_a = UserId::new();
        let user_b = UserId::new();

        seed_memory(&substrate, user_a, agent);
        seed_memory(&substrate, user_b, agent);

        let counts = substrate.wipe_user(user_a).unwrap();
        assert_eq!(counts.topics_deleted, 1);
        assert_eq!(counts.agent_topics_deleted, 1);
        assert_eq!(counts.extractions_deleted, 1);

        // User B still has everything.
        assert_eq!(substrate.user_topic_index(user_b).unwrap().len(), 1);
        assert_eq!(
            substrate
                .user_agent_topic_index(user_b, agent)
                .unwrap()
                .len(),
            1
        );
        let audit = substrate.list_user_extraction_audit(user_b, 10).unwrap();
        assert_eq!(audit.len(), 1);
    }

    #[test]
    fn test_wipe_user_idempotent_zero_counts() {
        // Re-running wipe on an already-clean user returns 0/0/0 without
        // erroring.
        let substrate = MemorySubstrate::open_in_memory(0.1).unwrap();
        let user = UserId::new();
        let counts = substrate.wipe_user(user).unwrap();
        assert_eq!(counts.topics_deleted, 0);
        assert_eq!(counts.agent_topics_deleted, 0);
        assert_eq!(counts.extractions_deleted, 0);
    }

    #[test]
    fn test_audit_surfaces_session_deleted_flag() {
        // Deleting the originating session must NOT lose the audit row —
        // attribution survives via the denormalized user_id column, and the
        // LEFT JOIN flips `session_deleted` to true.
        let substrate = MemorySubstrate::open_in_memory(0.1).unwrap();
        let user = UserId::new();
        let agent = AgentId::new();
        let session = substrate.sessions.create_session(agent, user).unwrap();
        substrate
            .append_extraction(session.id, &SessionExtraction::default())
            .unwrap();

        // Before delete: session_deleted = false.
        let audit = substrate.list_user_extraction_audit(user, 10).unwrap();
        assert_eq!(audit.len(), 1);
        assert!(!audit[0].4, "session_deleted should start false");

        // Delete the session.
        substrate.sessions.delete_session(session.id).unwrap();

        // After delete: extraction row still attributed to `user`, flag flips.
        let audit = substrate.list_user_extraction_audit(user, 10).unwrap();
        assert_eq!(audit.len(), 1, "extraction must survive session delete");
        assert!(audit[0].4, "session_deleted should flip to true");
    }
}
