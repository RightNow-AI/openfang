//! MemorySubstrate: unified implementation of the `Memory` trait.
//!
//! Composes the structured store, semantic store, knowledge store,
//! session store, and consolidation engine behind a single async API.

use crate::agency_profiles::AgencyProfileStore;
use crate::consolidation::ConsolidationEngine;
use crate::knowledge::KnowledgeStore;
use crate::migration::run_migrations;
use crate::planner::{
    clarify_inbox_item as planner_clarify_inbox_item,
    hydrate_today_plan_recommendations, inbox_items_with_recommendations, list_agent_catalog,
    rebuild_today_plan, set_agent_catalog_enabled, PlannerStore,
};
use crate::semantic::SemanticStore;
use crate::session::{Session, SessionStore};
use crate::structured::StructuredStore;
use crate::usage::UsageStore;

use async_trait::async_trait;
use openfang_agency_import::import_profile_from_path;
use openfang_types::agent::{AgentEntry, AgentId, SessionId};
use openfang_types::agent_profile::AgentProfile;
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::memory::{
    ConsolidationReport, Entity, ExportFormat, GraphMatch, GraphPattern, ImportReport, Memory,
    MemoryFilter, MemoryFragment, MemoryId, MemorySource, Relation,
};
use openfang_types::planner::{
    PlannerAgentCatalogEntry, PlannerInboxItem, PlannerProject, PlannerRoutine, PlannerTask,
    PlannerTodayPlan,
};
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// The unified memory substrate. Implements the `Memory` trait by delegating
/// to specialized stores backed by a shared SQLite connection.
pub struct MemorySubstrate {
    conn: Arc<Mutex<Connection>>,
    structured: StructuredStore,
    semantic: SemanticStore,
    knowledge: KnowledgeStore,
    agency_profiles: AgencyProfileStore,
    planner: PlannerStore,
    sessions: SessionStore,
    consolidation: ConsolidationEngine,
    usage: UsageStore,
}

impl MemorySubstrate {
    /// Open or create a memory substrate at the given database path.
    pub fn open(db_path: &Path, decay_rate: f32) -> OpenFangResult<Self> {
        let conn = Connection::open(db_path).map_err(|e| OpenFangError::Memory(e.to_string()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        run_migrations(&conn).map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let shared = Arc::new(Mutex::new(conn));

        Ok(Self {
            conn: Arc::clone(&shared),
            structured: StructuredStore::new(Arc::clone(&shared)),
            semantic: SemanticStore::new(Arc::clone(&shared)),
            knowledge: KnowledgeStore::new(Arc::clone(&shared)),
            agency_profiles: AgencyProfileStore::new(Arc::clone(&shared)),
            planner: PlannerStore::new(Arc::clone(&shared)),
            sessions: SessionStore::new(Arc::clone(&shared)),
            usage: UsageStore::new(Arc::clone(&shared)),
            consolidation: ConsolidationEngine::new(shared, decay_rate),
        })
    }

    /// Create an in-memory substrate (for testing).
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
            agency_profiles: AgencyProfileStore::new(Arc::clone(&shared)),
            planner: PlannerStore::new(Arc::clone(&shared)),
            sessions: SessionStore::new(Arc::clone(&shared)),
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

    /// List planner inbox items.
    pub fn planner_list_inbox(&self) -> OpenFangResult<Vec<PlannerInboxItem>> {
        self.planner.list_inbox()
    }

    /// List planner inbox items along with clarified tasks and recommendations.
    pub fn planner_list_inbox_with_recommendations(
        &self,
    ) -> OpenFangResult<Vec<(PlannerInboxItem, Vec<PlannerTask>)>> {
        inbox_items_with_recommendations(&self.planner)
    }

    /// Create a planner inbox item.
    pub fn planner_create_inbox_item(&self, text: &str) -> OpenFangResult<PlannerInboxItem> {
        self.planner.create_inbox_item(text)
    }

    /// Clarify a planner inbox item into a structured planner task and optional project.
    pub fn planner_clarify_inbox_item(
        &self,
        inbox_item_id: &str,
    ) -> OpenFangResult<(PlannerInboxItem, Option<PlannerProject>, PlannerTask, Vec<PlannerTask>)> {
        planner_clarify_inbox_item(&self.planner, inbox_item_id)
    }

    /// Get an existing today plan for the given date or build one if missing.
    pub fn planner_get_or_rebuild_today_plan(
        &self,
        date: chrono::NaiveDate,
    ) -> OpenFangResult<PlannerTodayPlan> {
        match self.planner.get_today_plan(date)? {
            Some(plan) => hydrate_today_plan_recommendations(&self.planner, plan),
            None => rebuild_today_plan(&self.planner, date),
        }
    }

    /// Force a rebuild of the planner today plan for the given date.
    pub fn planner_rebuild_today_plan(
        &self,
        date: chrono::NaiveDate,
    ) -> OpenFangResult<PlannerTodayPlan> {
        rebuild_today_plan(&self.planner, date)
    }

    /// List planner routines.
    pub fn planner_list_routines(&self) -> OpenFangResult<Vec<PlannerRoutine>> {
        self.planner.list_routines()
    }

    /// Load a planner project by id.
    pub fn planner_get_project(&self, project_id: &str) -> OpenFangResult<Option<PlannerProject>> {
        self.planner.get_project(project_id)
    }

    /// List planner catalog entries and enabled state.
    pub fn planner_list_agent_catalog(&self) -> OpenFangResult<Vec<PlannerAgentCatalogEntry>> {
        list_agent_catalog(&self.planner)
    }

    /// Update enabled state for a planner catalog agent entry.
    pub fn planner_set_agent_enabled(
        &self,
        agent_id: &str,
        enabled: bool,
    ) -> OpenFangResult<PlannerAgentCatalogEntry> {
        set_agent_catalog_enabled(&self.planner, agent_id, enabled)
    }

    /// Import an agency profile from markdown and persist it.
    pub fn agency_import_profile_from_path(&self, path: &Path) -> OpenFangResult<AgentProfile> {
        let profile = import_profile_from_path(path).map_err(|error| {
            let details = error
                .errors
                .iter()
                .map(|entry| format!("{}: {}", entry.section, entry.message))
                .collect::<Vec<_>>()
                .join("; ");
            OpenFangError::InvalidInput(format!(
                "Agency profile import failed for {}: {}",
                error.source_path, details
            ))
        })?;

        self.agency_profiles.upsert_profile(&profile, path)
    }

    /// List all persisted agency profiles.
    pub fn agency_list_profiles(&self) -> OpenFangResult<Vec<AgentProfile>> {
        self.agency_profiles.list_profiles()
    }

    /// Load a persisted agency profile by id.
    pub fn agency_get_profile(&self, profile_id: &str) -> OpenFangResult<Option<AgentProfile>> {
        self.agency_profiles.get_profile(profile_id)
    }

    /// Update enabled state for a persisted agency profile.
    pub fn agency_set_profile_enabled(
        &self,
        profile_id: &str,
        enabled: bool,
    ) -> OpenFangResult<AgentProfile> {
        self.agency_profiles.set_profile_enabled(profile_id, enabled)
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
    use std::fs;

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

    #[test]
    fn test_agency_import_and_toggle_round_trip() {
        let substrate = MemorySubstrate::open_in_memory(0.1).unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let profile_dir = temp_dir.path().join("support");
        fs::create_dir_all(&profile_dir).unwrap();
        let profile_path = profile_dir.join("support-support-responder.md");
        fs::write(
            &profile_path,
            "# Support Responder Agent Personality\n\n## 🧠 Your Identity & Memory\n- **Role**: Customer support specialist\n- **Personality**: Empathetic, precise\n- **Memory**: Successful support patterns\n\n## 🎯 Your Core Mission\n### Resolve customer issues\n- Keep response quality high\n\n## 🔄 Your Workflow Process\n### Step 1: Intake\n- Review context\n\n## 📋 Your Deliverable Template\n```markdown\n# Support Report\n## Summary\n```\n",
        )
        .unwrap();

        let imported = substrate
            .agency_import_profile_from_path(&profile_path)
            .unwrap();
        assert_eq!(imported.id, "support-responder");

        let listed = substrate.agency_list_profiles().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "support-responder");

        let updated = substrate
            .agency_set_profile_enabled("support-responder", false)
            .unwrap();
        assert!(!updated.enabled);

        let fetched = substrate
            .agency_get_profile("support-responder")
            .unwrap()
            .unwrap();
        assert!(!fetched.enabled);
    }

    #[test]
    fn test_agency_import_rejects_invalid_markdown() {
        let substrate = MemorySubstrate::open_in_memory(0.1).unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let profile_dir = temp_dir.path().join("support");
        fs::create_dir_all(&profile_dir).unwrap();
        let profile_path = profile_dir.join("support-bad-profile.md");
        fs::write(&profile_path, "# Broken Profile\n\n## 🎯 Your Core Mission\n- Help users\n")
            .unwrap();

        let error = substrate
            .agency_import_profile_from_path(&profile_path)
            .unwrap_err();
        assert!(matches!(error, OpenFangError::InvalidInput(_)));
        assert!(error.to_string().contains("Role section is required"));
    }
}
