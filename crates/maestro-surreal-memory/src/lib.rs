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
pub use session::Session;

const DEFAULT_NAMESPACE: &str = "openfang";
const DEFAULT_DATABASE: &str = "memory";

pub struct SurrealMemorySubstrate {
    db: Surreal<Db>,
}

impl SurrealMemorySubstrate {
    pub async fn connect<P: AsRef<Path>>(db_path: P) -> OpenFangResult<Self> {
        let db = Surreal::new::<RocksDb>(db_path.as_ref())
            .await
            .map_err(|e| OpenFangError::Memory(format!("Failed to connect to SurrealDB: {}", e)))?;

        db.use_ns(DEFAULT_NAMESPACE)
            .use_db(DEFAULT_DATABASE)
            .await
            .map_err(|e| OpenFangError::Memory(format!("Failed to select namespace/database: {}", e)))?;

        let substrate = Self { db };
        substrate.initialize_tables().await?;
        Ok(substrate)
    }

    pub async fn connect_in_memory() -> OpenFangResult<Self> {
        let db = Surreal::new::<surrealdb::engine::local::Mem>(())
            .await
            .map_err(|e| OpenFangError::Memory(format!("Failed to connect to SurrealDB: {}", e)))?;

        db.use_ns(DEFAULT_NAMESPACE)
            .use_db(DEFAULT_DATABASE)
            .await
            .map_err(|e| OpenFangError::Memory(format!("Failed to select namespace/database: {}", e)))?;

        let substrate = Self { db };
        substrate.initialize_tables().await?;
        Ok(substrate)
    }

    pub fn connect_sync<P: AsRef<Path>>(db_path: P) -> OpenFangResult<Self> {
        tokio::runtime::Handle::current().block_on(async move {
            Self::connect(db_path).await
        })
    }

    // Helper method to deserialize a memory fragment from SurrealDB result
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

    // Helper method to deserialize a graph match from SurrealDB result
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

        // Fetch source entity
        let source_result: Option<serde_json::Value> = self.db
            .select(("entities", source_id))
            .await
            .map_err(|e| OpenFangError::Memory(format!("Entity fetch failed: {}", e)))?;

        let source_entity = if let Some(ref ent) = source_result {
            Entity {
                id: source_id.to_string(),
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
        } else {
            return Ok(None);
        };

        // Fetch target entity (similar logic)
        let target_result: Option<serde_json::Value> = self.db
            .select(("entities", target_id))
            .await
            .map_err(|e| OpenFangError::Memory(format!("Entity fetch failed: {}", e)))?;

        let target_entity = if let Some(ref ent) = target_result {
            Entity {
                id: target_id.to_string(),
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
        } else {
            return Ok(None);
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

    // Initialize tables if they don't exist
    pub async fn initialize_tables(&self) -> OpenFangResult<()> {
        // Define tables for memory fragments
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
            "#)
            .await
            .map_err(|e| OpenFangError::Memory(format!("Table initialization failed: {}", e)))?;

        // Define tables for entities
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

        // Define tables for relations
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

        // Define tables for agents
        self.db
            .query(r#"
                DEFINE TABLE IF NOT EXISTS agents SCHEMAFULL;
                DEFINE FIELD IF NOT EXISTS id ON agents TYPE string;
                DEFINE FIELD IF NOT EXISTS name ON agents TYPE string;
                DEFINE FIELD IF NOT EXISTS manifest ON agents TYPE object;
                DEFINE FIELD IF NOT EXISTS created_at ON agents TYPE string;
            "#)
            .await
            .map_err(|e| OpenFangError::Memory(format!("Agent table initialization failed: {}", e)))?;

        // Define tables for sessions
        self.db.query(r#"
                DEFINE TABLE IF NOT EXISTS sessions SCHEMAFULL;
                DEFINE FIELD IF NOT EXISTS id ON sessions TYPE string;
                DEFINE FIELD IF NOT EXISTS agent_id ON sessions TYPE string;
                DEFINE FIELD IF NOT EXISTS messages ON sessions TYPE array;
                DEFINE FIELD IF NOT EXISTS context_window_tokens ON sessions TYPE int;
                DEFINE FIELD IF NOT EXISTS label ON sessions TYPE option<string>;
                DEFINE FIELD IF NOT EXISTS created_at ON sessions TYPE string;
                DEFINE FIELD IF NOT EXISTS updated_at ON sessions TYPE string;
            "#)
            .await
            .map_err(|e| OpenFangError::Memory(format!("Session table initialization failed: {}", e)))?;

        Ok(())
    }

    // Kernel-compatible sync methods
    pub fn save_agent(&self, entry: &AgentEntry) -> OpenFangResult<()> {
        tokio::runtime::Handle::current().block_on(async {
            let _: Option<serde_json::Value> = self.db
                .create(("agents", entry.id.0.to_string()))
                .content(serde_json::json!({
                    "id": entry.id.0.to_string(),
                    "name": entry.name,
                    "manifest": entry.manifest,
                }))
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(())
        })
    }

    // Session management methods
    pub fn get_session(&self, session_id: SessionId) -> OpenFangResult<Option<Session>> {
        tokio::runtime::Handle::current().block_on(async {
            let result: Option<serde_json::Value> = self.db
                .select(("sessions", session_id.to_string()))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Session fetch failed: {}", e)))?;

            match result {
                Some(value) => {
                    // Deserialize the session from SurrealDB format
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
                        .unwrap_or(&Vec::new())
                        .clone();
                    let messages: Vec<Message> = serde_json::from_value(serde_json::Value::Array(messages_json))
                        .unwrap_or_default();

                    let context_window_tokens = value.get("context_window_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);

                    let label = value.get("label")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    Ok(Some(Session {
                        id,
                        agent_id,
                        messages,
                        context_window_tokens,
                        label,
                    }))
                }
                None => Ok(None),
            }
        })
    }

    pub fn create_session(&self, agent_id: AgentId) -> OpenFangResult<Session> {
        let session = Session::new(agent_id);
        self.save_session(&session)?;
        Ok(session)
    }

    pub fn create_session_with_label(&self, agent_id: AgentId, label: String) -> OpenFangResult<Session> {
        let session = Session::with_label(agent_id, label);
        self.save_session(&session)?;
        Ok(session)
    }

    pub fn save_session(&self, session: &Session) -> OpenFangResult<()> {
        tokio::runtime::Handle::current().block_on(async {
            let _: Option<serde_json::Value> = self.db
                .create(("sessions", session.id.to_string()))
                .content(serde_json::json!({
                    "id": session.id.0.to_string(),
                    "agent_id": session.agent_id.0.to_string(),
                    "messages": session.messages,
                    "context_window_tokens": session.context_window_tokens,
                    "label": session.label,
                    "created_at": chrono::Utc::now().to_rfc3339(),
                    "updated_at": chrono::Utc::now().to_rfc3339(),
                }))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Session save failed: {}", e)))?;
            Ok(())
        })
    }

    pub fn delete_session(&self, session_id: SessionId) -> OpenFangResult<()> {
        tokio::runtime::Handle::current().block_on(async {
            let _: Option<serde_json::Value> = self.db
                .delete(("sessions", session_id.to_string()))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Session delete failed: {}", e)))?;
            Ok(())
        })
    }

    // Additional kernel compatibility methods - blocking versions for kernel compatibility
    pub fn structured_get(&self, agent_id: AgentId, key: &str) -> OpenFangResult<Option<serde_json::Value>> {
        tokio::runtime::Handle::current().block_on(async {
            self.get(agent_id, key).await
        })
    }

    pub fn structured_set(&self, agent_id: AgentId, key: &str, value: serde_json::Value) -> OpenFangResult<()> {
        tokio::runtime::Handle::current().block_on(async {
            self.set(agent_id, key, value).await
        })
    }

    // Stub implementations for other methods - TODO: implement properly
    pub fn load_all_agents(&self) -> OpenFangResult<Vec<openfang_types::agent::AgentEntry>> {
        // Stub implementation - would need proper AgentEntry type and deserialization
        Ok(Vec::new())
    }

    pub fn delete_agent_sessions(&self, _agent_id: AgentId) -> OpenFangResult<()> {
        // TODO: implement
        Ok(())
    }

    pub fn delete_canonical_session(&self, _agent_id: AgentId) -> OpenFangResult<()> {
        // TODO: implement
        Ok(())
    }

    pub fn list_agent_sessions(&self, _agent_id: AgentId) -> OpenFangResult<Vec<serde_json::Value>> {
        todo!("list_agent_sessions not implemented")
    }

    pub fn remove_agent(&self, _agent_id: AgentId) -> OpenFangResult<()> {
        // TODO: implement
        Ok(())
    }

    pub fn canonical_context(&self, agent_id: AgentId, _limit: Option<usize>) -> OpenFangResult<Vec<openfang_types::message::Message>> {
        tokio::runtime::Handle::current().block_on(async {
            // Query recent messages for the agent
            let limit_val = _limit.unwrap_or(50);
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
        })
    }

    pub fn append_canonical(&self, agent_id: AgentId, messages: &[openfang_types::message::Message], _limit: Option<usize>) -> OpenFangResult<()> {
        tokio::runtime::Handle::current().block_on(async {
            // Get current session for the agent
            if let Ok(Some(mut session)) = self.get_session(SessionId::new()) {
                session.messages.extend_from_slice(messages);
                self.save_session(&session)?;
            }
            Ok(())
        })
    }

    pub fn write_jsonl_mirror(&self, _session: &Session, _path: &std::path::Path) -> OpenFangResult<()> {
        todo!("write_jsonl_mirror not implemented")
    }

    pub fn store_llm_summary(&self, _agent_id: AgentId, _summary: &str, _kept_messages: Vec<openfang_types::message::Message>) -> OpenFangResult<()> {
        todo!("store_llm_summary not implemented")
    }

    pub fn usage_conn(&self) -> OpenFangResult<()> {
        todo!("usage_conn not implemented")
    }

    pub fn load_paired_devices(&self) -> OpenFangResult<Vec<serde_json::Value>> {
        todo!("load_paired_devices not implemented")
    }

    pub fn save_paired_device(&self, _device: serde_json::Value) -> OpenFangResult<()> {
        todo!("save_paired_device not implemented")
    }

    pub fn remove_paired_device(&self, _device_id: &str) -> OpenFangResult<()> {
        todo!("remove_paired_device not implemented")
    }

    pub fn task_post(&self, _title: &str, _description: &str, _assigned_to: Option<AgentId>, _created_by: AgentId) -> OpenFangResult<()> {
        todo!("task_post not implemented")
    }

    pub fn task_claim(&self, _task_id: &str) -> OpenFangResult<()> {
        todo!("task_claim not implemented")
    }

    pub fn task_complete(&self, _task_id: &str, _result: &str) -> OpenFangResult<()> {
        todo!("task_complete not implemented")
    }

    pub fn task_list(&self, _status: Option<&str>) -> OpenFangResult<Vec<serde_json::Value>> {
        todo!("task_list not implemented")
    }
}

#[async_trait]
impl Memory for SurrealMemorySubstrate {
    async fn get(&self, agent_id: AgentId, key: &str) -> OpenFangResult<Option<serde_json::Value>> {
        let table = format!("kv_{}", agent_id.0);
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
        let table = format!("kv_{}", agent_id.0);
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
        let table = format!("kv_{}", agent_id.0);
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

        let _: Option<serde_json::Value> = self.db
            .create(("memory_fragments", id.0.to_string()))
            .content(serde_json::json!({
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
            }))
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
            .bind(bindings)
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

        let _: Option<serde_json::Value> = self.db
            .create(("entities", id.clone()))
            .content(serde_json::json!({
                "id": id,
                "entity_type": serde_json::to_string(&entity.entity_type).map_err(|e| OpenFangError::Memory(format!("Entity type serialization failed: {}", e)))?,
                "name": entity.name,
                "properties": entity.properties,
                "created_at": now.to_rfc3339(),
                "updated_at": now.to_rfc3339()
            }))
            .await
            .map_err(|e| OpenFangError::Memory(format!("Entity add failed: {}", e)))?;

        Ok(id)
    }

    async fn add_relation(&self, relation: Relation) -> OpenFangResult<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now();

        let _: Option<serde_json::Value> = self.db
            .create(("relations", id.clone()))
            .content(serde_json::json!({
                "id": id,
                "source": relation.source,
                "relation": serde_json::to_string(&relation.relation).map_err(|e| OpenFangError::Memory(format!("Relation type serialization failed: {}", e)))?,
                "target": relation.target,
                "properties": relation.properties,
                "confidence": relation.confidence,
                "created_at": now.to_rfc3339()
            }))
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
            .bind(bindings)
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

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(ConsolidationReport {
            memories_merged: 0,  // TODO: implement merging logic
            memories_decayed: 0, // TODO: count actual decayed memories
            duration_ms,
        })
    }

    async fn export(&self, _format: ExportFormat) -> OpenFangResult<Vec<u8>> {
        // Export all memory fragments, entities, and relations
        let export_data = match _format {
            ExportFormat::Json => {
                // Export memory fragments
                let memory_results: Vec<serde_json::Value> = self.db
                    .query("SELECT * FROM memory_fragments WHERE deleted = false")
                    .await
                    .map_err(|e| OpenFangError::Memory(format!("Memory export query failed: {}", e)))?
                    .take(0)
                    .map_err(|e| OpenFangError::Memory(format!("Memory export result parsing failed: {}", e)))?;

                // Export entities
                let entity_results: Vec<serde_json::Value> = self.db
                    .query("SELECT * FROM entities")
                    .await
                    .map_err(|e| OpenFangError::Memory(format!("Entity export query failed: {}", e)))?
                    .take(0)
                    .map_err(|e| OpenFangError::Memory(format!("Entity export result parsing failed: {}", e)))?;

                // Export relations
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
                // For now, just return empty for MessagePack
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

        // Check version compatibility
        if let Some(version) = import_data.get("version").and_then(|v| v.as_str()) {
            if version != "1.0" {
                return Err(OpenFangError::Memory(format!("Unsupported export version: {}", version)));
            }
        }

        let mut entities_imported = 0;
        let mut relations_imported = 0;
        let mut memories_imported = 0;
        let mut errors = Vec::new();

        // Import entities
        if let Some(entities) = import_data.get("entities").and_then(|e| e.as_array()) {
            for entity in entities {
                let entity_id = entity.get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown_entity");
                let _: Option<serde_json::Value> = self.db
                    .create(("entities", entity_id))
                    .content(entity.clone())
                    .await
                    .map_err(|e| OpenFangError::Memory(format!("Entity import failed: {}", e)))?;
                entities_imported += 1;
            }
        }

        // Import relations
        if let Some(relations) = import_data.get("relations").and_then(|r| r.as_array()) {
            for relation in relations {
                let relation_id = relation.get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown_relation");
                let _: Option<serde_json::Value> = self.db
                    .create(("relations", relation_id))
                    .content(relation.clone())
                    .await
                    .map_err(|e| OpenFangError::Memory(format!("Relation import failed: {}", e)))?;
                relations_imported += 1;
            }
        }

        // Import memories
        if let Some(memories) = import_data.get("memories").and_then(|m| m.as_array()) {
            for memory in memories {
                let memory_id = memory.get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown_memory");
                let _: Option<serde_json::Value> = self.db
                    .create(("memory_fragments", memory_id))
                    .content(memory.clone())
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connect_in_memory() {
        let substrate = SurrealMemorySubstrate::connect_in_memory().await.unwrap();
        assert!(true); // Just test that it doesn't panic
    }

    #[tokio::test]
    async fn test_kv_operations() {
        let substrate = SurrealMemorySubstrate::connect_in_memory().await.unwrap();
        let agent_id = AgentId::new();
        let key = "test_key";
        let value = serde_json::json!("test_value");

        // Test set
        substrate.set(agent_id, key, value.clone()).await.unwrap();

        // Test get
        let retrieved = substrate.get(agent_id, key).await.unwrap();
        assert_eq!(retrieved, Some(value));

        // Test delete
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

        // Test remember
        let memory_id = substrate.remember(agent_id, content, MemorySource::Conversation, "test", metadata).await.unwrap();

        // Test recall
        let fragments = substrate.recall(content, 10, None).await.unwrap();
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].content, content);

        // Test forget
        substrate.forget(memory_id).await.unwrap();
        let fragments_after = substrate.recall(content, 10, None).await.unwrap();
        assert_eq!(fragments_after.len(), 0);
    }
}