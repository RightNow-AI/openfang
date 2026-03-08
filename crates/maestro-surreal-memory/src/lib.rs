//! SurrealDB-based memory substrate for OpenFang.

use async_trait::async_trait;
use chrono;
use openfang_types::agent::{AgentEntry, AgentId, SessionId};
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::message::Message;
use openfang_types::memory::{
    ConsolidationReport, Entity, ExportFormat, GraphMatch, GraphPattern, ImportReport, Memory,
    MemoryFilter, MemoryFragment, MemoryId, MemorySource, Relation,
};

use std::collections::HashMap;
use std::path::Path;
use surrealdb::engine::local::{Db, RocksDb};
use surrealdb::Surreal;
use uuid;

pub mod session;
pub mod usage;
pub use session::Session;
pub use usage::SurrealUsageStore;

const DEFAULT_NAMESPACE: &str = "openfang";
const DEFAULT_DATABASE: &str = "memory";

/// SurrealDB-backed memory substrate for OpenFang.
///
/// Stores all memory fragments, entities, relations, sessions, paired devices,
/// tasks, usage records, and LLM summaries in a single SurrealDB instance.
pub struct SurrealMemorySubstrate {
    db: Surreal<Db>,
    usage_store: SurrealUsageStore,
}

impl SurrealMemorySubstrate {
    /// Connect to a SurrealDB instance backed by RocksDB at the given path.
    pub async fn connect<P: AsRef<Path>>(db_path: P) -> OpenFangResult<Self> {
        let db = Surreal::new::<RocksDb>(db_path.as_ref())
            .await
            .map_err(|e| OpenFangError::Memory(format!("Failed to connect to SurrealDB: {}", e)))?;

        db.use_ns(DEFAULT_NAMESPACE)
            .use_db(DEFAULT_DATABASE)
            .await
            .map_err(|e| OpenFangError::Memory(format!("Failed to select namespace/database: {}", e)))?;

        let usage_store = SurrealUsageStore::with_db(db.clone());
        let substrate = Self { db, usage_store };
        substrate.initialize_tables().await?;
        Ok(substrate)
    }

    /// Connect to an in-memory SurrealDB instance (for testing).
    pub async fn connect_in_memory() -> OpenFangResult<Self> {
        let db = Surreal::new::<surrealdb::engine::local::Mem>(())
            .await
            .map_err(|e| OpenFangError::Memory(format!("Failed to connect to SurrealDB: {}", e)))?;

        db.use_ns(DEFAULT_NAMESPACE)
            .use_db(DEFAULT_DATABASE)
            .await
            .map_err(|e| OpenFangError::Memory(format!("Failed to select namespace/database: {}", e)))?;

        let usage_store = SurrealUsageStore::with_db(db.clone());
        let substrate = Self { db, usage_store };
        substrate.initialize_tables().await?;
        Ok(substrate)
    }

    // connect_sync removed — use connect().await instead

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    // block_on removed — all methods are now native async

    /// Deserialize a `MemoryFragment` from a SurrealDB JSON value.
    fn deserialize_memory_fragment(value: &serde_json::Value) -> Result<MemoryFragment, OpenFangError> {
        let id_str = value.get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let id = MemoryId(uuid::Uuid::parse_str(id_str).unwrap_or_else(|_| uuid::Uuid::new_v4()));

        let agent_id_str = value.get("agent_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let agent_id = AgentId(uuid::Uuid::parse_str(agent_id_str).unwrap_or_else(|_| uuid::Uuid::new_v4()));

        let content = value.get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| OpenFangError::Memory("Missing content".to_string()))?
            .to_string();

        let metadata = value.get("metadata")
            .and_then(|v| v.as_object())
            .map(|obj| obj.clone().into_iter().collect())
            .unwrap_or_default();

        let source = value.get("source")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(MemorySource::System);

        let confidence = value.get("confidence")
            .and_then(|v| v.as_f64())
            .map(|f| f as f32)
            .unwrap_or(1.0);

        let created_at = value.get("created_at")
            .and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|| chrono::Utc::now());

        let accessed_at = value.get("accessed_at")
            .and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|| chrono::Utc::now());

        let access_count = value.get("access_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let scope = value.get("scope")
            .and_then(|v| v.as_str())
            .unwrap_or("default")
            .to_string();

        Ok(MemoryFragment {
            id,
            agent_id,
            content,
            embedding: None,
            metadata,
            source,
            confidence,
            created_at,
            accessed_at,
            access_count,
            scope,
        })
    }

    /// Deserialize a `GraphMatch` from a SurrealDB relation row.
    async fn deserialize_graph_match(&self, value: &serde_json::Value) -> Result<Option<GraphMatch>, OpenFangError> {
        let source_id = value.get("source")
            .and_then(|v| v.as_str())
            .ok_or_else(|| OpenFangError::Memory("Missing source".to_string()))?;

        let relation_type = value.get("relation")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(openfang_types::memory::RelationType::RelatedTo);

        let target_id = value.get("target")
            .and_then(|v| v.as_str())
            .ok_or_else(|| OpenFangError::Memory("Missing target".to_string()))?;

        let fetch_entity = |ent: &serde_json::Value, eid: &str| -> Entity {
            Entity {
                id: eid.to_string(),
                entity_type: ent.get("entity_type")
                    .and_then(|v| v.as_str())
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or(openfang_types::memory::EntityType::Custom("unknown".to_string())),
                name: ent.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                properties: ent.get("properties")
                    .and_then(|v| v.as_object())
                    .map(|obj| obj.clone().into_iter().collect())
                    .unwrap_or_default(),
                created_at: ent.get("created_at")
                    .and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|| chrono::Utc::now()),
                updated_at: ent.get("updated_at")
                    .and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|| chrono::Utc::now()),
            }
        };

        let source_result: Option<serde_json::Value> = self.db
            .select(("entities", source_id))
            .await
            .map_err(|e| OpenFangError::Memory(format!("Entity fetch failed: {}", e)))?;

        let source_entity = match source_result {
            Some(ref ent) => fetch_entity(ent, source_id),
            None => return Ok(None),
        };

        let target_result: Option<serde_json::Value> = self.db
            .select(("entities", target_id))
            .await
            .map_err(|e| OpenFangError::Memory(format!("Entity fetch failed: {}", e)))?;

        let target_entity = match target_result {
            Some(ref ent) => fetch_entity(ent, target_id),
            None => return Ok(None),
        };

        let confidence = value.get("confidence")
            .and_then(|v| v.as_f64())
            .map(|f| f as f32)
            .unwrap_or(1.0);

        let created_at = value.get("created_at")
            .and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|| chrono::Utc::now());

        let relation = Relation {
            source: source_id.to_string(),
            relation: relation_type,
            target: target_id.to_string(),
            properties: value.get("properties")
                .and_then(|v| v.as_object())
                .map(|obj| obj.clone().into_iter().collect())
                .unwrap_or_default(),
            confidence,
            created_at,
        };

        Ok(Some(GraphMatch {
            source: source_entity,
            relation,
            target: target_entity,
        }))
    }

    /// Deserialize a `Session` from a SurrealDB JSON value.
    fn deserialize_session(value: &serde_json::Value) -> OpenFangResult<Session> {
        let id_str = value.get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let id = SessionId(uuid::Uuid::parse_str(id_str).unwrap_or_else(|_| SessionId::new().0));

        let agent_id_str = value.get("agent_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let agent_id = AgentId(uuid::Uuid::parse_str(agent_id_str).unwrap_or_else(|_| AgentId::new().0));

        let messages_json = value.get("messages")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let messages: Vec<Message> = serde_json::from_value(serde_json::Value::Array(messages_json))
            .unwrap_or_default();

        let context_window_tokens = value.get("context_window_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let label = value.get("label")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        Ok(Session {
            id,
            agent_id,
            messages,
            context_window_tokens,
            label,
        })
    }

    // -----------------------------------------------------------------------
    // Table initialization
    // -----------------------------------------------------------------------

    /// Initialize all SurrealDB tables and indexes.
    pub async fn initialize_tables(&self) -> OpenFangResult<()> {
        // Memory fragments
        self.db.query(r#"
            DEFINE TABLE IF NOT EXISTS memory_fragments SCHEMAFULL;
            DEFINE FIELD IF NOT EXISTS id ON memory_fragments TYPE string;
            DEFINE FIELD IF NOT EXISTS agent_id ON memory_fragments TYPE string;
            DEFINE FIELD IF NOT EXISTS content ON memory_fragments TYPE string;
            DEFINE FIELD IF NOT EXISTS embedding ON memory_fragments TYPE option<array<float>>;
            DEFINE FIELD IF NOT EXISTS metadata ON memory_fragments TYPE object;
            DEFINE FIELD IF NOT EXISTS source ON memory_fragments TYPE string;
            DEFINE FIELD IF NOT EXISTS confidence ON memory_fragments TYPE float;
            DEFINE FIELD IF NOT EXISTS created_at ON memory_fragments TYPE string;
            DEFINE FIELD IF NOT EXISTS accessed_at ON memory_fragments TYPE string;
            DEFINE FIELD IF NOT EXISTS access_count ON memory_fragments TYPE int;
            DEFINE FIELD IF NOT EXISTS scope ON memory_fragments TYPE string;
            DEFINE FIELD IF NOT EXISTS deleted ON memory_fragments TYPE bool DEFAULT false;
            DEFINE INDEX IF NOT EXISTS idx_mf_agent ON memory_fragments FIELDS agent_id;
            DEFINE INDEX IF NOT EXISTS idx_mf_scope ON memory_fragments FIELDS scope;
        "#)
        .await
        .map_err(|e| OpenFangError::Memory(format!("Table initialization failed: {}", e)))?;

        // Entities
        self.db.query(r#"
            DEFINE TABLE IF NOT EXISTS entities SCHEMAFULL;
            DEFINE FIELD IF NOT EXISTS id ON entities TYPE string;
            DEFINE FIELD IF NOT EXISTS entity_type ON entities TYPE string;
            DEFINE FIELD IF NOT EXISTS name ON entities TYPE string;
            DEFINE FIELD IF NOT EXISTS properties ON entities TYPE object;
            DEFINE FIELD IF NOT EXISTS created_at ON entities TYPE string;
            DEFINE FIELD IF NOT EXISTS updated_at ON entities TYPE string;
        "#)
        .await
        .map_err(|e| OpenFangError::Memory(format!("Entity table initialization failed: {}", e)))?;

        // Relations
        self.db.query(r#"
            DEFINE TABLE IF NOT EXISTS relations SCHEMAFULL;
            DEFINE FIELD IF NOT EXISTS id ON relations TYPE string;
            DEFINE FIELD IF NOT EXISTS source ON relations TYPE string;
            DEFINE FIELD IF NOT EXISTS relation ON relations TYPE string;
            DEFINE FIELD IF NOT EXISTS target ON relations TYPE string;
            DEFINE FIELD IF NOT EXISTS properties ON relations TYPE object;
            DEFINE FIELD IF NOT EXISTS confidence ON relations TYPE float;
            DEFINE FIELD IF NOT EXISTS created_at ON relations TYPE string;
        "#)
        .await
        .map_err(|e| OpenFangError::Memory(format!("Relation table initialization failed: {}", e)))?;

        // Agents
        self.db.query(r#"
            DEFINE TABLE IF NOT EXISTS agents SCHEMAFULL;
            DEFINE FIELD IF NOT EXISTS id ON agents TYPE string;
            DEFINE FIELD IF NOT EXISTS name ON agents TYPE string;
            DEFINE FIELD IF NOT EXISTS manifest ON agents TYPE object;
            DEFINE FIELD IF NOT EXISTS created_at ON agents TYPE string;
        "#)
        .await
        .map_err(|e| OpenFangError::Memory(format!("Agent table initialization failed: {}", e)))?;

        // Sessions
        self.db.query(r#"
            DEFINE TABLE IF NOT EXISTS sessions SCHEMAFULL;
            DEFINE FIELD IF NOT EXISTS id ON sessions TYPE string;
            DEFINE FIELD IF NOT EXISTS agent_id ON sessions TYPE string;
            DEFINE FIELD IF NOT EXISTS messages ON sessions TYPE array;
            DEFINE FIELD IF NOT EXISTS context_window_tokens ON sessions TYPE int;
            DEFINE FIELD IF NOT EXISTS label ON sessions TYPE option<string>;
            DEFINE FIELD IF NOT EXISTS created_at ON sessions TYPE string;
            DEFINE FIELD IF NOT EXISTS updated_at ON sessions TYPE string;
            DEFINE INDEX IF NOT EXISTS idx_sess_agent ON sessions FIELDS agent_id;
            DEFINE INDEX IF NOT EXISTS idx_sess_label ON sessions FIELDS label;
        "#)
        .await
        .map_err(|e| OpenFangError::Memory(format!("Session table initialization failed: {}", e)))?;

        // Paired devices
        self.db.query(r#"
            DEFINE TABLE IF NOT EXISTS paired_devices SCHEMAFULL;
            DEFINE FIELD IF NOT EXISTS device_id ON paired_devices TYPE string;
            DEFINE FIELD IF NOT EXISTS display_name ON paired_devices TYPE string;
            DEFINE FIELD IF NOT EXISTS platform ON paired_devices TYPE string;
            DEFINE FIELD IF NOT EXISTS paired_at ON paired_devices TYPE string;
            DEFINE FIELD IF NOT EXISTS last_seen ON paired_devices TYPE string;
            DEFINE FIELD IF NOT EXISTS push_token ON paired_devices TYPE option<string>;
            DEFINE INDEX IF NOT EXISTS idx_pd_device ON paired_devices FIELDS device_id UNIQUE;
        "#)
        .await
        .map_err(|e| OpenFangError::Memory(format!("Paired devices table initialization failed: {}", e)))?;

        // Tasks (agent task queue)
        self.db.query(r#"
            DEFINE TABLE IF NOT EXISTS tasks SCHEMAFULL;
            DEFINE FIELD IF NOT EXISTS task_id ON tasks TYPE string;
            DEFINE FIELD IF NOT EXISTS title ON tasks TYPE string;
            DEFINE FIELD IF NOT EXISTS description ON tasks TYPE string;
            DEFINE FIELD IF NOT EXISTS status ON tasks TYPE string DEFAULT 'pending';
            DEFINE FIELD IF NOT EXISTS assigned_to ON tasks TYPE option<string>;
            DEFINE FIELD IF NOT EXISTS claimed_by ON tasks TYPE option<string>;
            DEFINE FIELD IF NOT EXISTS created_by ON tasks TYPE option<string>;
            DEFINE FIELD IF NOT EXISTS result ON tasks TYPE option<string>;
            DEFINE FIELD IF NOT EXISTS created_at ON tasks TYPE string;
            DEFINE FIELD IF NOT EXISTS updated_at ON tasks TYPE string;
            DEFINE INDEX IF NOT EXISTS idx_task_status ON tasks FIELDS status;
        "#)
        .await
        .map_err(|e| OpenFangError::Memory(format!("Tasks table initialization failed: {}", e)))?;

        // Usage records
        self.db.query(r#"
            DEFINE TABLE IF NOT EXISTS usage_records SCHEMAFULL;
            DEFINE FIELD IF NOT EXISTS agent_id ON usage_records TYPE string;
            DEFINE FIELD IF NOT EXISTS model ON usage_records TYPE string;
            DEFINE FIELD IF NOT EXISTS input_tokens ON usage_records TYPE int;
            DEFINE FIELD IF NOT EXISTS output_tokens ON usage_records TYPE int;
            DEFINE FIELD IF NOT EXISTS cost_usd ON usage_records TYPE float;
            DEFINE FIELD IF NOT EXISTS tool_calls ON usage_records TYPE int;
            DEFINE FIELD IF NOT EXISTS created_at ON usage_records TYPE string;
            DEFINE INDEX IF NOT EXISTS idx_ur_agent ON usage_records FIELDS agent_id;
            DEFINE INDEX IF NOT EXISTS idx_ur_model ON usage_records FIELDS model;
            DEFINE INDEX IF NOT EXISTS idx_ur_date ON usage_records FIELDS created_at;
        "#)
        .await
        .map_err(|e| OpenFangError::Memory(format!("Usage records table initialization failed: {}", e)))?;

        // LLM summaries (compaction results)
        self.db.query(r#"
            DEFINE TABLE IF NOT EXISTS llm_summaries SCHEMAFULL;
            DEFINE FIELD IF NOT EXISTS agent_id ON llm_summaries TYPE string;
            DEFINE FIELD IF NOT EXISTS summary ON llm_summaries TYPE string;
            DEFINE FIELD IF NOT EXISTS kept_messages ON llm_summaries TYPE array;
            DEFINE FIELD IF NOT EXISTS created_at ON llm_summaries TYPE string;
            DEFINE INDEX IF NOT EXISTS idx_llm_agent ON llm_summaries FIELDS agent_id;
        "#)
        .await
        .map_err(|e| OpenFangError::Memory(format!("LLM summaries table initialization failed: {}", e)))?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Agent management
    // -----------------------------------------------------------------------

    /// Save an agent entry to SurrealDB.
    pub async fn save_agent(&self, entry: &AgentEntry) -> OpenFangResult<()> {
            // Upsert: delete then create to handle re-registration
            let _: Vec<serde_json::Value> = self.db
                .query("DELETE agents WHERE id = $id")
                .bind(("id", entry.id.0.to_string()))
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?
                .take(0)
                .unwrap_or_default();

            self.db
                .query("CREATE type::record('agents', $id) CONTENT $data")
                .bind(("id", entry.id.0.to_string()))
                .bind(("data", serde_json::json!({
                    "id": entry.id.0.to_string(),
                    "name": entry.name,
                    "manifest": entry.manifest,
                    "created_at": chrono::Utc::now().to_rfc3339(),
                })))
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(())
    }

    /// Load all registered agents.
    pub async fn load_all_agents(&self) -> OpenFangResult<Vec<AgentEntry>> {
            let results: Vec<serde_json::Value> = self.db
                .query("SELECT * FROM agents")
                .await
                .map_err(|e| OpenFangError::Memory(format!("Agent load failed: {}", e)))?
                .take(0)
                .map_err(|e| OpenFangError::Memory(format!("Agent parse failed: {}", e)))?;

            let mut agents = Vec::new();
            for row in results {
                let id_str = row.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
                let id = AgentId(uuid::Uuid::parse_str(id_str).unwrap_or_else(|_| uuid::Uuid::new_v4()));
                let name = row.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let manifest = row.get("manifest").cloned().unwrap_or(serde_json::json!({}));
                let manifest: openfang_types::agent::AgentManifest = serde_json::from_value(manifest)
                    .unwrap_or_default();
                let now = chrono::Utc::now();
                agents.push(AgentEntry {
                    id,
                    name,
                    manifest,
                    state: openfang_types::agent::AgentState::Created,
                    mode: openfang_types::agent::AgentMode::default(),
                    created_at: now,
                    last_active: now,
                    parent: None,
                    children: Vec::new(),
                    session_id: SessionId::new(),
                    tags: Vec::new(),
                    identity: openfang_types::agent::AgentIdentity::default(),
                    onboarding_completed: false,
                    onboarding_completed_at: None,
                });
            }
            Ok(agents)
    }

    /// Remove an agent and all its data.
    pub async fn remove_agent(&self, agent_id: AgentId) -> OpenFangResult<()> {
            let aid = agent_id.0.to_string();
            self.db.query("DELETE agents WHERE id = $id")
                .bind(("id", aid.clone()))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Agent delete failed: {}", e)))?;
            // Also delete agent's sessions
            self.db.query("DELETE sessions WHERE agent_id = $id")
                .bind(("id", aid.clone()))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Agent session cleanup failed: {}", e)))?;
            // Delete agent's memory fragments
            self.db.query("DELETE memory_fragments WHERE agent_id = $id")
                .bind(("id", aid))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Agent memory cleanup failed: {}", e)))?;
            Ok(())
    }

    // -----------------------------------------------------------------------
    // Session management
    // -----------------------------------------------------------------------

    /// Fetch a session by ID.
    pub async fn get_session(&self, session_id: SessionId) -> OpenFangResult<Option<Session>> {
            let result: Option<serde_json::Value> = self.db
                .select(("sessions", session_id.to_string()))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Session fetch failed: {}", e)))?;

            match result {
                Some(value) => Ok(Some(Self::deserialize_session(&value)?)),
                None => Ok(None),
            }
    }

    /// Create a new session for an agent.
    pub async fn create_session(&self, agent_id: AgentId) -> OpenFangResult<Session> {
        let session = Session::new(agent_id);
        self.save_session(&session).await?;
        Ok(session)
    }

    /// Create a new session with an optional label.
    pub async fn create_session_with_label(&self, agent_id: AgentId, label: Option<&str>) -> OpenFangResult<Session> {
        let session = Session::with_label(agent_id, label.unwrap_or_default().to_string());
        self.save_session(&session).await?;
        Ok(session)
    }

    /// Persist a session to SurrealDB (upsert semantics).
    pub async fn save_session(&self, session: &Session) -> OpenFangResult<()> {
            let sid = session.id.to_string();
            // Delete existing then create (upsert)
            let _ = self.db
                .query("DELETE sessions WHERE id = $id")
                .bind(("id", sid.clone()))
                .await;

            self.db
                .query("CREATE type::record('sessions', $sid) CONTENT $data")
                .bind(("sid", sid))
                .bind(("data", serde_json::json!({
                    "id": session.id.0.to_string(),
                    "agent_id": session.agent_id.0.to_string(),
                    "messages": session.messages,
                    "context_window_tokens": session.context_window_tokens,
                    "label": session.label,
                    "created_at": chrono::Utc::now().to_rfc3339(),
                    "updated_at": chrono::Utc::now().to_rfc3339(),
                })))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Session save failed: {}", e)))?;
            Ok(())
    }

    /// Delete a session by ID.
    pub async fn delete_session(&self, session_id: SessionId) -> OpenFangResult<()> {
            let _: Option<serde_json::Value> = self.db
                .delete(("sessions", session_id.to_string()))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Session delete failed: {}", e)))?;
            Ok(())
    }

    /// Delete all sessions for a given agent.
    pub async fn delete_agent_sessions(&self, agent_id: AgentId) -> OpenFangResult<()> {
            self.db.query("DELETE sessions WHERE agent_id = $agent_id")
                .bind(("agent_id", agent_id.0.to_string()))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Delete agent sessions failed: {}", e)))?;
            Ok(())
    }

    /// Delete the canonical (most recent) session for an agent.
    pub async fn delete_canonical_session(&self, agent_id: AgentId) -> OpenFangResult<()> {
            // Find the most recent session for the agent and delete it
            let results: Vec<serde_json::Value> = self.db
                .query("SELECT id FROM sessions WHERE agent_id = $agent_id ORDER BY updated_at DESC LIMIT 1")
                .bind(("agent_id", agent_id.0.to_string()))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Canonical session query failed: {}", e)))?
                .take(0)
                .map_err(|e| OpenFangError::Memory(format!("Canonical session parse failed: {}", e)))?;

            if let Some(row) = results.first() {
                if let Some(sid) = row.get("id").and_then(|v| v.as_str()) {
                    let _: Option<serde_json::Value> = self.db
                        .delete(("sessions", sid))
                        .await
                        .map_err(|e| OpenFangError::Memory(format!("Canonical session delete failed: {}", e)))?;
                }
            }
            Ok(())
    }

    /// List all sessions for a specific agent, returned as JSON values.
    pub async fn list_agent_sessions(&self, agent_id: AgentId) -> OpenFangResult<Vec<serde_json::Value>> {
            let results: Vec<serde_json::Value> = self.db
                .query("SELECT * FROM sessions WHERE agent_id = $agent_id ORDER BY updated_at DESC")
                .bind(("agent_id", agent_id.0.to_string()))
                .await
                .map_err(|e| OpenFangError::Memory(format!("List agent sessions failed: {}", e)))?
                .take(0)
                .map_err(|e| OpenFangError::Memory(format!("List agent sessions parse failed: {}", e)))?;
            Ok(results)
    }

    /// List all sessions across all agents.
    pub async fn list_sessions(&self) -> OpenFangResult<Vec<serde_json::Value>> {
            let results: Vec<serde_json::Value> = self.db
                .query("SELECT * FROM sessions ORDER BY updated_at DESC")
                .await
                .map_err(|e| OpenFangError::Memory(format!("List sessions failed: {}", e)))?
                .take(0)
                .map_err(|e| OpenFangError::Memory(format!("List sessions parse failed: {}", e)))?;
            Ok(results)
    }

    /// Set or clear a label on a session.
    pub async fn set_session_label(&self, session_id: SessionId, label: Option<String>) -> OpenFangResult<()> {
            self.db
                .query("UPDATE sessions SET label = $label, updated_at = $now WHERE id = $id")
                .bind(("id", session_id.0.to_string()))
                .bind(("label", label))
                .bind(("now", chrono::Utc::now().to_rfc3339()))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Set session label failed: {}", e)))?;
            Ok(())
    }

    /// Find a session by label for a given agent.
    pub async fn find_session_by_label(&self, agent_id: AgentId, label: &str) -> OpenFangResult<Option<Session>> {
            let results: Vec<serde_json::Value> = self.db
                .query("SELECT * FROM sessions WHERE agent_id = $agent_id AND label = $label LIMIT 1")
                .bind(("agent_id", agent_id.0.to_string()))
                .bind(("label", label.to_string()))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Find session by label failed: {}", e)))?
                .take(0)
                .map_err(|e| OpenFangError::Memory(format!("Find session by label parse failed: {}", e)))?;

            match results.into_iter().next() {
                Some(value) => Ok(Some(Self::deserialize_session(&value)?)),
                None => Ok(None),
            }
    }

    // -----------------------------------------------------------------------
    // KV operations (blocking wrappers)
    // -----------------------------------------------------------------------

    /// Blocking wrapper for `Memory::get`.
    pub async fn structured_get(&self, agent_id: AgentId, key: &str) -> OpenFangResult<Option<serde_json::Value>> {
            self.get(agent_id, key).await
    }

    /// Blocking wrapper for `Memory::set`.
    pub async fn structured_set(&self, agent_id: AgentId, key: &str, value: serde_json::Value) -> OpenFangResult<()> {
            self.set(agent_id, key, value).await
    }

    /// Blocking wrapper for `Memory::delete`.
    pub async fn structured_delete(&self, agent_id: AgentId, key: &str) -> OpenFangResult<()> {
            self.delete(agent_id, key).await
    }

    /// List all KV pairs for an agent.
    pub async fn list_kv(&self, agent_id: AgentId) -> OpenFangResult<Vec<(String, serde_json::Value)>> {
            let table = format!("`kv_{}`", agent_id.0.to_string().replace('-', "_"));
            let sql = format!("SELECT key, value FROM {}", table);
            let results: Vec<serde_json::Value> = self.db
                .query(&sql)
                .await
                .map_err(|e| OpenFangError::Memory(format!("KV list failed: {}", e)))?
                .take(0)
                .unwrap_or_default();

            let pairs = results.into_iter().filter_map(|row| {
                let key = row.get("key")?.as_str()?.to_string();
                let value = row.get("value")?.clone();
                Some((key, value))
            }).collect();
            Ok(pairs)
    }

    // -----------------------------------------------------------------------
    // Canonical context
    // -----------------------------------------------------------------------

    /// Get the canonical (most recent) context messages for an agent.
    pub async fn canonical_context(&self, agent_id: AgentId, limit: Option<usize>) -> OpenFangResult<Vec<Message>> {
            let _limit_val = limit.unwrap_or(50);
            let results: Vec<serde_json::Value> = self.db
                .query("SELECT messages FROM sessions WHERE agent_id = $agent_id ORDER BY updated_at DESC LIMIT 1")
                .bind(("agent_id", agent_id.to_string()))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Canonical context query failed: {}", e)))?
                .take(0)
                .map_err(|e| OpenFangError::Memory(format!("Canonical context results parsing failed: {}", e)))?;

            if let Some(result) = results.into_iter().next() {
                if let Some(messages_json) = result.get("messages").and_then(|v| v.as_array()) {
                    return serde_json::from_value(serde_json::Value::Array(messages_json.clone()))
                        .map_err(|e| OpenFangError::Memory(format!("Message deserialization failed: {}", e)));
                }
            }
            Ok(Vec::new())
    }

    /// Append messages to the canonical session for an agent.
    pub async fn append_canonical(&self, agent_id: AgentId, messages: &[Message], _limit: Option<usize>) -> OpenFangResult<()> {
            // Find the most recent session for this agent
            let results: Vec<serde_json::Value> = self.db
                .query("SELECT id FROM sessions WHERE agent_id = $agent_id ORDER BY updated_at DESC LIMIT 1")
                .bind(("agent_id", agent_id.0.to_string()))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Append canonical query failed: {}", e)))?
                .take(0)
                .map_err(|e| OpenFangError::Memory(format!("Append canonical parse failed: {}", e)))?;

            if let Some(row) = results.first() {
                if let Some(sid_str) = row.get("id").and_then(|v| v.as_str()) {
                    let sid = SessionId(uuid::Uuid::parse_str(sid_str).unwrap_or_else(|_| SessionId::new().0));
                    if let Some(mut session) = self.get_session(sid).await? {
                        session.messages.extend_from_slice(messages);
                        self.save_session(&session).await?;
                    }
                }
            }
            Ok(())
    }

    // -----------------------------------------------------------------------
    // JSONL mirror
    // -----------------------------------------------------------------------

    /// Write a JSONL mirror of a session to the filesystem.
    pub fn write_jsonl_mirror(&self, session: &Session, path: &std::path::Path) -> OpenFangResult<()> {
        use std::io::Write;

        // Ensure the directory exists
        if let Err(e) = std::fs::create_dir_all(path) {
            return Err(OpenFangError::Memory(format!("Failed to create JSONL mirror directory: {}", e)));
        }

        let file_path = path.join(format!("{}.jsonl", session.id.0));
        let file = std::fs::File::create(&file_path)
            .map_err(|e| OpenFangError::Memory(format!("Failed to create JSONL file: {}", e)))?;
        let mut writer = std::io::BufWriter::new(file);

        for message in &session.messages {
            let json = serde_json::to_string(message)
                .map_err(|e| OpenFangError::Memory(format!("Message serialization failed: {}", e)))?;
            writeln!(writer, "{}", json)
                .map_err(|e| OpenFangError::Memory(format!("JSONL write failed: {}", e)))?;
        }

        writer.flush()
            .map_err(|e| OpenFangError::Memory(format!("JSONL flush failed: {}", e)))?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // LLM summary storage
    // -----------------------------------------------------------------------

    /// Store an LLM-generated summary after session compaction.
    pub async fn store_llm_summary(&self, agent_id: AgentId, summary: &str, kept_messages: Vec<Message>) -> OpenFangResult<()> {
            let id = uuid::Uuid::new_v4().to_string();
            self.db
                .query("CREATE type::record('llm_summaries', $id) CONTENT $data")
                .bind(("id", id.clone()))
                .bind(("data", serde_json::json!({
                    "agent_id": agent_id.0.to_string(),
                    "summary": summary,
                    "kept_messages": kept_messages,
                    "created_at": chrono::Utc::now().to_rfc3339(),
                })))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Store LLM summary failed: {}", e)))?;
            Ok(())
    }

    /// `usage_conn` is a legacy SQLite method. In the SurrealDB substrate, usage
    /// is handled by `SurrealUsageStore` via `self.usage()`. This method exists
    /// only for API compatibility and is not expected to be called.
    pub fn usage_conn(&self) -> OpenFangResult<()> {
        // No-op: SurrealDB usage is accessed via self.usage()
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Paired devices
    // -----------------------------------------------------------------------

    /// Load all paired devices from SurrealDB.
    pub async fn load_paired_devices(&self) -> OpenFangResult<Vec<serde_json::Value>> {
            let results: Vec<serde_json::Value> = self.db
                .query("SELECT * FROM paired_devices")
                .await
                .map_err(|e| OpenFangError::Memory(format!("Load paired devices failed: {}", e)))?
                .take(0)
                .map_err(|e| OpenFangError::Memory(format!("Load paired devices parse failed: {}", e)))?;
            Ok(results)
    }

    /// Save (upsert) a paired device.
    pub async fn save_paired_device(&self, device: serde_json::Value) -> OpenFangResult<()> {
            let device_id = device.get("device_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| OpenFangError::Memory("Missing device_id in paired device".to_string()))?
                .to_string();

            // Delete existing entry if present (upsert)
            self.db.query("DELETE paired_devices WHERE device_id = $device_id")
                .bind(("device_id", device_id.clone()))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Paired device delete failed: {}", e)))?;

            self.db
                .query("CREATE type::record('paired_devices', $device_id) CONTENT $data")
                .bind(("device_id", device_id))
                .bind(("data", device))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Save paired device failed: {}", e)))?;
            Ok(())
    }

    /// Remove a paired device by device ID.
    pub async fn remove_paired_device(&self, device_id: &str) -> OpenFangResult<()> {
            self.db.query("DELETE paired_devices WHERE device_id = $device_id")
                .bind(("device_id", device_id.to_string()))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Remove paired device failed: {}", e)))?;
            Ok(())
    }

    // -----------------------------------------------------------------------
    // Task queue
    // -----------------------------------------------------------------------

    /// Post a new task to the queue.
    pub async fn task_post(&self, title: &str, description: &str, assigned_to: Option<&str>, created_by: Option<&str>) -> OpenFangResult<String> {
        let task_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        self.db
            .query("CREATE type::record('tasks', $task_id) CONTENT $data")
            .bind(("task_id", task_id.clone()))
            .bind(("data", serde_json::json!({
                "task_id": task_id,
                "title": title,
                "description": description,
                "status": "pending",
                "assigned_to": assigned_to,
                "claimed_by": null,
                "created_by": created_by,
                "result": null,
                "created_at": now,
                "updated_at": now,
            })))
            .await
            .map_err(|e| OpenFangError::Memory(format!("Task post failed: {}", e)))?;

        Ok(task_id)
    }

    /// Claim the next pending task assigned to (or unassigned for) the given agent.
    pub async fn task_claim(&self, agent_id: &str) -> OpenFangResult<Option<serde_json::Value>> {
        let now = chrono::Utc::now().to_rfc3339();

        // Find the oldest pending task assigned to this agent (or unassigned)
        let results: Vec<serde_json::Value> = self.db
            .query("SELECT * FROM tasks WHERE status = 'pending' AND (assigned_to = $agent_id OR assigned_to IS NULL) ORDER BY created_at ASC LIMIT 1")
            .bind(("agent_id", agent_id.to_string()))
            .await
            .map_err(|e| OpenFangError::Memory(format!("Task claim query failed: {}", e)))?
            .take(0)
            .map_err(|e| OpenFangError::Memory(format!("Task claim parse failed: {}", e)))?;

        if let Some(task) = results.into_iter().next() {
            let task_id = task.get("task_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            // Update status to claimed
            self.db
                .query("UPDATE tasks SET status = 'claimed', claimed_by = $agent_id, updated_at = $now WHERE task_id = $task_id")
                .bind(("task_id", task_id))
                .bind(("agent_id", agent_id.to_string()))
                .bind(("now", now))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Task claim update failed: {}", e)))?;

            Ok(Some(task))
        } else {
            Ok(None)
        }
    }

    /// Mark a task as complete with a result string.
    pub async fn task_complete(&self, task_id: &str, result: &str) -> OpenFangResult<()> {
        let now = chrono::Utc::now().to_rfc3339();

        self.db
            .query("UPDATE tasks SET status = 'completed', result = $result, updated_at = $now WHERE task_id = $task_id")
            .bind(("task_id", task_id.to_string()))
            .bind(("result", result.to_string()))
            .bind(("now", now))
            .await
            .map_err(|e| OpenFangError::Memory(format!("Task complete failed: {}", e)))?;

        Ok(())
    }

    /// List tasks, optionally filtered by status.
    pub async fn task_list(&self, status: Option<&str>) -> OpenFangResult<Vec<serde_json::Value>> {
        let results: Vec<serde_json::Value> = if let Some(status) = status {
            self.db
                .query("SELECT * FROM tasks WHERE status = $status ORDER BY created_at DESC")
                .bind(("status", status.to_string()))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Task list failed: {}", e)))?
                .take(0)
                .map_err(|e| OpenFangError::Memory(format!("Task list parse failed: {}", e)))?
        } else {
            self.db
                .query("SELECT * FROM tasks ORDER BY created_at DESC")
                .await
                .map_err(|e| OpenFangError::Memory(format!("Task list failed: {}", e)))?
                .take(0)
                .map_err(|e| OpenFangError::Memory(format!("Task list parse failed: {}", e)))?
        };

        Ok(results)
    }

    // -----------------------------------------------------------------------
    // Usage store access
    // -----------------------------------------------------------------------

    /// Get a reference to the usage store.
    pub fn usage(&self) -> &SurrealUsageStore {
        &self.usage_store
    }
}

// ---------------------------------------------------------------------------
// Memory trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl Memory for SurrealMemorySubstrate {
    async fn get(&self, agent_id: AgentId, key: &str) -> OpenFangResult<Option<serde_json::Value>> {
        let table = format!("`kv_{}`", agent_id.0.to_string().replace('-', "_"));
        let sql = format!("SELECT value FROM {} WHERE key = $key LIMIT 1", table);
        let key_owned = key.to_string();

        let result: Vec<serde_json::Value> = self.db
            .query(&sql)
            .bind(("key", key_owned))
            .await
            .map_err(|e| OpenFangError::Memory(format!("KV get failed: {}", e)))?
            .take(0)
            .map_err(|e| OpenFangError::Memory(format!("KV get result parsing failed: {}", e)))?;

        Ok(result.into_iter().next().and_then(|r| r.get("value").cloned()))
    }

    async fn set(&self, agent_id: AgentId, key: &str, value: serde_json::Value) -> OpenFangResult<()> {
        let table = format!("`kv_{}`", agent_id.0.to_string().replace('-', "_"));
        let sql = format!("DELETE {} WHERE key = $key", table);
        let key_owned = key.to_string();

        // Delete if exists
        self.db
            .query(&sql)
            .bind(("key", key_owned.clone()))
            .await
            .map_err(|e| OpenFangError::Memory(format!("KV delete failed: {}", e)))?;

        // Insert new record
        let insert_sql = format!("CREATE {} CONTENT {{ key: $key, value: $value, updated_at: $updated_at }}", table);
        self.db
            .query(&insert_sql)
            .bind(("key", key_owned))
            .bind(("value", value))
            .bind(("updated_at", chrono::Utc::now().to_rfc3339()))
            .await
            .map_err(|e| OpenFangError::Memory(format!("KV set failed: {}", e)))?;

        Ok(())
    }

    async fn delete(&self, agent_id: AgentId, key: &str) -> OpenFangResult<()> {
        let table = format!("`kv_{}`", agent_id.0.to_string().replace('-', "_"));
        let sql = format!("DELETE {} WHERE key = $key", table);
        let key_owned = key.to_string();

        self.db
            .query(&sql)
            .bind(("key", key_owned))
            .await
            .map_err(|e| OpenFangError::Memory(format!("KV delete failed: {}", e)))?;

        Ok(())
    }

    async fn remember(&self, agent_id: AgentId, content: &str, source: MemorySource, scope: &str, metadata: HashMap<String, serde_json::Value>) -> OpenFangResult<MemoryId> {
        let id = MemoryId::new();
        let now = chrono::Utc::now();

        self.db
            .query("CREATE type::record('memory_fragments', $id) CONTENT $data")
            .bind(("id", id.0.to_string()))
            .bind(("data", serde_json::json!({
                "id": id.0.to_string(),
                "agent_id": agent_id.0.to_string(),
                "content": content,
                "embedding": None::<Vec<f32>>,
                "metadata": metadata,
                "source": serde_json::to_string(&source).map_err(|e| OpenFangError::Memory(format!("Source serialization failed: {}", e)))?,
                "confidence": 1.0,
                "created_at": now.to_rfc3339(),
                "accessed_at": now.to_rfc3339(),
                "access_count": 0,
                "scope": scope,
                "deleted": false
            })))
            .await
            .map_err(|e| OpenFangError::Memory(format!("Memory remember failed: {}", e)))?;

        Ok(id)
    }

    async fn recall(&self, _query: &str, limit: usize, filter: Option<MemoryFilter>) -> OpenFangResult<Vec<MemoryFragment>> {
        let mut sql = "SELECT * FROM memory_fragments WHERE deleted = false".to_string();
        let mut bindings = serde_json::Map::new();

        if let Some(ref f) = filter {
            if let Some(ref agent_id) = f.agent_id {
                sql.push_str(" AND agent_id = $agent_id");
                bindings.insert("agent_id".to_string(), serde_json::json!(agent_id.0.to_string()));
            }
            if let Some(ref source) = f.source {
                sql.push_str(" AND source = $source");
                bindings.insert("source".to_string(), serde_json::json!(serde_json::to_string(source).map_err(|e| OpenFangError::Memory(format!("Source serialization failed: {}", e)))?));
            }
            if let Some(ref scope) = f.scope {
                sql.push_str(" AND scope = $scope");
                bindings.insert("scope".to_string(), serde_json::json!(scope));
            }
            if let Some(min_conf) = f.min_confidence {
                sql.push_str(" AND confidence >= $min_confidence");
                bindings.insert("min_confidence".to_string(), serde_json::json!(min_conf));
            }
        }

        sql.push_str(" ORDER BY accessed_at DESC LIMIT $limit");
        bindings.insert("limit".to_string(), serde_json::json!(limit));

        let results: Vec<serde_json::Value> = self.db
            .query(&sql)
            .bind(serde_json::Value::Object(bindings))
            .await
            .map_err(|e| OpenFangError::Memory(format!("Memory recall failed: {}", e)))?
            .take(0)
            .map_err(|e| OpenFangError::Memory(format!("Result parsing failed: {}", e)))?;

        let mut fragments = Vec::new();
        for result in results {
            if let Ok(fragment) = Self::deserialize_memory_fragment(&result) {
                fragments.push(fragment);
            }
        }

        Ok(fragments)
    }

    async fn forget(&self, id: MemoryId) -> OpenFangResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.db
            .query("UPDATE memory_fragments SET deleted = true, accessed_at = $accessed_at WHERE id = $id")
            .bind(("id", id.0.to_string()))
            .bind(("accessed_at", now))
            .await
            .map_err(|e| OpenFangError::Memory(format!("Memory forget failed: {}", e)))?;

        Ok(())
    }

    async fn add_entity(&self, entity: Entity) -> OpenFangResult<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now();

        self.db
            .query("CREATE type::record('entities', $id) CONTENT $data")
            .bind(("id", id.clone()))
            .bind(("data", serde_json::json!({
                "id": id,
                "entity_type": serde_json::to_string(&entity.entity_type).map_err(|e| OpenFangError::Memory(format!("Entity type serialization failed: {}", e)))?,
                "name": entity.name,
                "properties": entity.properties,
                "created_at": now.to_rfc3339(),
                "updated_at": now.to_rfc3339()
            })))
            .await
            .map_err(|e| OpenFangError::Memory(format!("Entity add failed: {}", e)))?;

        Ok(id)
    }

    async fn add_relation(&self, relation: Relation) -> OpenFangResult<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now();

        self.db
            .query("CREATE type::record('relations', $id) CONTENT $data")
            .bind(("id", id.clone()))
            .bind(("data", serde_json::json!({
                "id": id,
                "source": relation.source,
                "relation": serde_json::to_string(&relation.relation).map_err(|e| OpenFangError::Memory(format!("Relation type serialization failed: {}", e)))?,
                "target": relation.target,
                "properties": relation.properties,
                "confidence": relation.confidence,
                "created_at": now.to_rfc3339()
            })))
            .await
            .map_err(|e| OpenFangError::Memory(format!("Relation add failed: {}", e)))?;

        Ok(id)
    }

    async fn query_graph(&self, pattern: GraphPattern) -> OpenFangResult<Vec<GraphMatch>> {
        let mut sql = "SELECT * FROM relations".to_string();
        let mut bindings = serde_json::Map::new();
        let mut conditions = Vec::new();

        if let Some(ref src) = pattern.source {
            conditions.push("source = $source".to_string());
            bindings.insert("source".to_string(), serde_json::json!(src));
        }
        if let Some(ref rel) = pattern.relation {
            conditions.push("relation = $relation".to_string());
            bindings.insert("relation".to_string(), serde_json::json!(serde_json::to_string(rel).map_err(|e| OpenFangError::Memory(format!("Relation pattern serialization failed: {}", e)))?));
        }
        if let Some(ref tgt) = pattern.target {
            conditions.push("target = $target".to_string());
            bindings.insert("target".to_string(), serde_json::json!(tgt));
        }

        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        let results: Vec<serde_json::Value> = self.db
            .query(&sql)
            .bind(serde_json::Value::Object(bindings))
            .await
            .map_err(|e| OpenFangError::Memory(format!("Graph query failed: {}", e)))?
            .take(0)
            .map_err(|e| OpenFangError::Memory(format!("Result parsing failed: {}", e)))?;

        let mut matches = Vec::new();
        for result in results {
            if let Ok(Some(graph_match)) = self.deserialize_graph_match(&result).await {
                matches.push(graph_match);
            }
        }

        Ok(matches)
    }

    async fn consolidate(&self) -> OpenFangResult<ConsolidationReport> {
        let start = std::time::Instant::now();

        // Decay confidence over time for all non-deleted memories
        self.db
            .query("UPDATE memory_fragments SET confidence = confidence * 0.99 WHERE deleted = false")
            .await
            .map_err(|e| OpenFangError::Memory(format!("Consolidation failed: {}", e)))?;

        // Count decayed memories
        let count_result: Vec<serde_json::Value> = self.db
            .query("SELECT count() AS cnt FROM memory_fragments WHERE deleted = false GROUP ALL")
            .await
            .map_err(|e| OpenFangError::Memory(format!("Consolidation count failed: {}", e)))?
            .take(0)
            .unwrap_or_default();

        let decayed = count_result.first()
            .and_then(|v| v.get("cnt"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(ConsolidationReport {
            memories_merged: 0,
            memories_decayed: decayed as u64,
            duration_ms,
        })
    }

    async fn export(&self, format: ExportFormat) -> OpenFangResult<Vec<u8>> {
        let export_data = match format {
            ExportFormat::Json => {
                let memory_results: Vec<serde_json::Value> = self.db
                    .query("SELECT * FROM memory_fragments WHERE deleted = false")
                    .await
                    .map_err(|e| OpenFangError::Memory(format!("Memory export query failed: {}", e)))?
                    .take(0)
                    .map_err(|e| OpenFangError::Memory(format!("Memory export result parsing failed: {}", e)))?;

                let entity_results: Vec<serde_json::Value> = self.db
                    .query("SELECT * FROM entities")
                    .await
                    .map_err(|e| OpenFangError::Memory(format!("Entity export query failed: {}", e)))?
                    .take(0)
                    .map_err(|e| OpenFangError::Memory(format!("Entity export result parsing failed: {}", e)))?;

                let relation_results: Vec<serde_json::Value> = self.db
                    .query("SELECT * FROM relations")
                    .await
                    .map_err(|e| OpenFangError::Memory(format!("Relation export query failed: {}", e)))?
                    .take(0)
                    .map_err(|e| OpenFangError::Memory(format!("Relation export result parsing failed: {}", e)))?;

                serde_json::json!({
                    "version": "1.0",
                    "format": "openfang_memory_export",
                    "memories": memory_results,
                    "entities": entity_results,
                    "relations": relation_results,
                    "exported_at": chrono::Utc::now().to_rfc3339()
                })
            }
            ExportFormat::MessagePack => {
                serde_json::json!({
                    "version": "1.0",
                    "format": "openfang_memory_export",
                    "memories": [],
                    "entities": [],
                    "relations": [],
                    "exported_at": chrono::Utc::now().to_rfc3339()
                })
            }
        };

        serde_json::to_vec(&export_data)
            .map_err(|e| OpenFangError::Memory(format!("Export serialization failed: {}", e)))
    }

    async fn import(&self, data: &[u8], _format: ExportFormat) -> OpenFangResult<ImportReport> {
        let import_data: serde_json::Value = serde_json::from_slice(data)
            .map_err(|e| OpenFangError::Memory(format!("Import data parsing failed: {}", e)))?;

        if let Some(version) = import_data.get("version").and_then(|v| v.as_str()) {
            if version != "1.0" {
                return Err(OpenFangError::Memory(format!("Unsupported export version: {}", version)));
            }
        }

        let mut entities_imported = 0;
        let mut relations_imported = 0;
        let mut memories_imported = 0;
        let errors = Vec::new();

        if let Some(entities) = import_data.get("entities").and_then(|e| e.as_array()) {
            for entity in entities {
                let entity_id = entity.get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown_entity");
                self.db
                    .query("CREATE type::record('entities', $id) CONTENT $data")
                    .bind(("id", entity_id))
                    .bind(("data", entity.clone()))
                    .await
                    .map_err(|e| OpenFangError::Memory(format!("Entity import failed: {}", e)))?;
                entities_imported += 1;
            }
        }

        if let Some(relations) = import_data.get("relations").and_then(|r| r.as_array()) {
            for relation in relations {
                let relation_id = relation.get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown_relation");
                self.db
                    .query("CREATE type::record('relations', $id) CONTENT $data")
                    .bind(("id", relation_id))
                    .bind(("data", relation.clone()))
                    .await
                    .map_err(|e| OpenFangError::Memory(format!("Relation import failed: {}", e)))?;
                relations_imported += 1;
            }
        }

        if let Some(memories) = import_data.get("memories").and_then(|m| m.as_array()) {
            for memory in memories {
                let memory_id = memory.get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown_memory");
                self.db
                    .query("CREATE type::record('memory_fragments', $id) CONTENT $data")
                    .bind(("id", memory_id))
                    .bind(("data", memory.clone()))
                    .await
                    .map_err(|e| OpenFangError::Memory(format!("Memory import failed: {}", e)))?;
                memories_imported += 1;
            }
        }

        Ok(ImportReport {
            entities_imported,
            relations_imported,
            memories_imported,
            errors,
        })
    }

    async fn save_session(
        &self,
        session: &openfang_types::session::Session,
    ) -> OpenFangResult<()> {
        // Delegate to the inherent async method
        SurrealMemorySubstrate::save_session(self, session).await
    }
}

// ---------------------------------------------------------------------------
// SessionPersistence trait implementation
// ---------------------------------------------------------------------------

impl openfang_types::session::SessionPersistence for SurrealMemorySubstrate {
    fn save_session(&self, session: &Session) -> OpenFangResult<()> {
        tokio::runtime::Handle::current().block_on(
            SurrealMemorySubstrate::save_session(self, session)
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connect_in_memory() {
        let substrate = SurrealMemorySubstrate::connect_in_memory().await.unwrap();
        assert!(substrate.load_all_agents().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_kv_operations() {
        let substrate = SurrealMemorySubstrate::connect_in_memory().await.unwrap();
        let agent_id = AgentId::new();
        let key = "test_key";
        let value = serde_json::json!("test_value");

        substrate.set(agent_id, key, value.clone()).await.unwrap();
        let retrieved = substrate.get(agent_id, key).await.unwrap();
        assert_eq!(retrieved, Some(value));

        substrate.delete(agent_id, key).await.unwrap();
        let deleted = substrate.get(agent_id, key).await.unwrap();
        assert_eq!(deleted, None);
    }

    #[tokio::test]
    async fn test_memory_operations() {
        let substrate = SurrealMemorySubstrate::connect_in_memory().await.unwrap();
        let agent_id = AgentId::new();
        let content = "Test memory content";
        let metadata = HashMap::new();

        let memory_id = substrate.remember(agent_id, content, MemorySource::Conversation, "test", metadata).await.unwrap();
        let fragments = substrate.recall(content, 10, None).await.unwrap();
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].content, content);

        substrate.forget(memory_id).await.unwrap();
        let fragments_after = substrate.recall(content, 10, None).await.unwrap();
        assert_eq!(fragments_after.len(), 0);
    }

    #[tokio::test]
    async fn test_session_operations() {
        let substrate = SurrealMemorySubstrate::connect_in_memory().await.unwrap();
        let agent_id = AgentId::new();

        // Create session
        let session = substrate.create_session(agent_id).await.unwrap();
        assert_eq!(session.agent_id, agent_id);

        // Get session
        let fetched = substrate.get_session(session.id).await.unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().id, session.id);

        // List agent sessions
        let sessions = substrate.list_agent_sessions(agent_id).await.unwrap();
        assert_eq!(sessions.len(), 1);

        // Delete session
        substrate.delete_session(session.id).await.unwrap();
        let gone = substrate.get_session(session.id).await.unwrap();
        assert!(gone.is_none());
    }

    #[tokio::test]
    async fn test_session_labels() {
        let substrate = SurrealMemorySubstrate::connect_in_memory().await.unwrap();
        let agent_id = AgentId::new();

        // Create with label
        let session = substrate.create_session_with_label(agent_id, Some("my-label")).await.unwrap();
        assert_eq!(session.label.as_deref(), Some("my-label"));

        // Find by label
        let found = substrate.find_session_by_label(agent_id, "my-label").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, session.id);

        // Set label
        substrate.set_session_label(session.id, Some("new-label".to_string())).await.unwrap();
        let found2 = substrate.find_session_by_label(agent_id, "new-label").await.unwrap();
        assert!(found2.is_some());
    }

    #[tokio::test]
    async fn test_paired_devices() {
        let substrate = SurrealMemorySubstrate::connect_in_memory().await.unwrap();

        let device = serde_json::json!({
            "device_id": "dev-001",
            "display_name": "Test Phone",
            "platform": "ios",
            "paired_at": chrono::Utc::now().to_rfc3339(),
            "last_seen": chrono::Utc::now().to_rfc3339(),
            "push_token": null,
        });

        // Save
        substrate.save_paired_device(device).await.unwrap();

        // Load
        let devices = substrate.load_paired_devices().await.unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].get("device_id").unwrap().as_str().unwrap(), "dev-001");

        // Remove
        substrate.remove_paired_device("dev-001").await.unwrap();
        let devices_after = substrate.load_paired_devices().await.unwrap();
        assert_eq!(devices_after.len(), 0);
    }

    #[tokio::test]
    async fn test_task_queue() {
        let substrate = SurrealMemorySubstrate::connect_in_memory().await.unwrap();

        // Post task
        let task_id = substrate.task_post("Test Task", "Do something", Some("agent-1"), Some("user-1")).await.unwrap();
        assert!(!task_id.is_empty());

        // List tasks
        let tasks = substrate.task_list(Some("pending")).await.unwrap();
        assert_eq!(tasks.len(), 1);

        // Claim task
        let claimed = substrate.task_claim("agent-1").await.unwrap();
        assert!(claimed.is_some());

        // Complete task
        substrate.task_complete(&task_id, "done").await.unwrap();
        let completed = substrate.task_list(Some("completed")).await.unwrap();
        assert_eq!(completed.len(), 1);
    }

    #[tokio::test]
    async fn test_llm_summary() {
        let substrate = SurrealMemorySubstrate::connect_in_memory().await.unwrap();
        let agent_id = AgentId::new();

        // Should not panic
        substrate.store_llm_summary(agent_id, "Summary text", vec![]).await.unwrap();
    }

    #[tokio::test]
    async fn test_write_jsonl_mirror() {
        let substrate = SurrealMemorySubstrate::connect_in_memory().await.unwrap();
        let agent_id = AgentId::new();
        let session = substrate.create_session(agent_id).await.unwrap();

        let tmp = tempfile::tempdir().unwrap();
        substrate.write_jsonl_mirror(&session, tmp.path()).unwrap();

        let file_path = tmp.path().join(format!("{}.jsonl", session.id.0));
        assert!(file_path.exists());
    }
}
