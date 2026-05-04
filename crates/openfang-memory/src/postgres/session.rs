//! PostgreSQL implementation of the session store.

use crate::backends::SessionBackend;
use crate::helpers;
use crate::session::{CanonicalSession, Session};
use deadpool_postgres::Pool;
use openfang_types::agent::{AgentId, SessionId};
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::message::Message;

pub struct PgSessionStore {
    pool: Pool,
}

impl PgSessionStore {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    fn block_on_pg<F, T>(&self, f: F) -> OpenFangResult<T>
    where
        F: std::future::Future<Output = OpenFangResult<T>>,
    {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(f)
        })
    }
}

impl SessionBackend for PgSessionStore {
    fn get_session(&self, id: SessionId) -> OpenFangResult<Option<Session>> {
        self.block_on_pg(async {
            let client = self.pool.get().await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let row = client
                .query_opt(
                    "SELECT id, agent_id, messages, context_window_tokens, label FROM sessions WHERE id = $1",
                    &[&id.0.to_string()],
                )
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;

            match row {
                Some(row) => {
                    let agent_str: String = row.get(1);
                    let messages_blob: Vec<u8> = row.get(2);
                    let tokens: i64 = row.get(3);
                    let label: Option<String> = row.get(4);
                    let agent_id = helpers::parse_agent_id(&agent_str)?;
                    let messages: Vec<Message> = helpers::deserialize_messages_lossy(&messages_blob);
                    Ok(Some(Session { id, agent_id, messages, context_window_tokens: tokens as u64, label }))
                }
                None => Ok(None),
            }
        })
    }

    fn save_session(&self, session: &Session) -> OpenFangResult<()> {
        let messages_blob = helpers::serialize_messages(&session.messages)?;
        let session = session.clone();

        self.block_on_pg(async {
            let client = self.pool.get().await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            client
                .execute(
                    "INSERT INTO sessions (id, agent_id, messages, context_window_tokens, label, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $5, NOW(), NOW())
                     ON CONFLICT (id) DO UPDATE SET messages = $3, context_window_tokens = $4, label = $5, updated_at = NOW()",
                    &[
                        &session.id.0.to_string(),
                        &session.agent_id.0.to_string(),
                        &messages_blob,
                        &(session.context_window_tokens as i64),
                        &session.label,
                    ],
                )
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(())
        })
    }

    fn delete_session(&self, id: SessionId) -> OpenFangResult<()> {
        self.block_on_pg(async {
            let client = self.pool.get().await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            client.execute("DELETE FROM sessions WHERE id = $1", &[&id.0.to_string()])
                .await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(())
        })
    }

    fn delete_agent_sessions(&self, agent_id: AgentId) -> OpenFangResult<()> {
        self.block_on_pg(async {
            let client = self.pool.get().await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            client.execute("DELETE FROM sessions WHERE agent_id = $1", &[&agent_id.0.to_string()])
                .await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(())
        })
    }

    fn list_sessions(&self) -> OpenFangResult<Vec<serde_json::Value>> {
        self.block_on_pg(async {
            let client = self.pool.get().await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let rows = client
                .query("SELECT id, agent_id, label, updated_at FROM sessions ORDER BY updated_at DESC", &[])
                .await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(rows.iter().map(|r| {
                serde_json::json!({
                    "id": r.get::<_, String>(0),
                    "agent_id": r.get::<_, String>(1),
                    "label": r.get::<_, Option<String>>(2),
                    "updated_at": r.get::<_, chrono::DateTime<chrono::Utc>>(3).to_rfc3339(),
                })
            }).collect())
        })
    }

    fn list_agent_sessions(&self, agent_id: AgentId) -> OpenFangResult<Vec<serde_json::Value>> {
        self.block_on_pg(async {
            let client = self.pool.get().await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let rows = client
                .query(
                    "SELECT id, label, updated_at FROM sessions WHERE agent_id = $1 ORDER BY updated_at DESC",
                    &[&agent_id.0.to_string()],
                )
                .await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(rows.iter().map(|r| {
                serde_json::json!({
                    "id": r.get::<_, String>(0),
                    "agent_id": agent_id.0.to_string(),
                    "label": r.get::<_, Option<String>>(1),
                    "updated_at": r.get::<_, chrono::DateTime<chrono::Utc>>(2).to_rfc3339(),
                })
            }).collect())
        })
    }

    fn set_session_label(&self, id: SessionId, label: Option<&str>) -> OpenFangResult<()> {
        let label = label.map(|s| s.to_string());
        self.block_on_pg(async {
            let client = self.pool.get().await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            client.execute("UPDATE sessions SET label = $1 WHERE id = $2", &[&label, &id.0.to_string()])
                .await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(())
        })
    }

    fn find_session_by_label(&self, agent_id: AgentId, label: &str) -> OpenFangResult<Option<Session>> {
        self.block_on_pg(async {
            let client = self.pool.get().await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let row = client
                .query_opt(
                    "SELECT id, messages, context_window_tokens, label FROM sessions WHERE agent_id = $1 AND label = $2",
                    &[&agent_id.0.to_string(), &label],
                )
                .await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            match row {
                Some(row) => {
                    let id_str: String = row.get(0);
                    let messages_blob: Vec<u8> = row.get(1);
                    let tokens: i64 = row.get(2);
                    let label: Option<String> = row.get(3);
                    let id = helpers::parse_session_id(&id_str)?;
                    let messages: Vec<Message> = helpers::deserialize_messages_lossy(&messages_blob);
                    Ok(Some(Session { id, agent_id, messages, context_window_tokens: tokens as u64, label }))
                }
                None => Ok(None),
            }
        })
    }

    fn delete_canonical_session(&self, agent_id: AgentId) -> OpenFangResult<()> {
        self.block_on_pg(async {
            let client = self.pool.get().await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            client.execute("DELETE FROM canonical_sessions WHERE agent_id = $1", &[&agent_id.0.to_string()])
                .await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(())
        })
    }

    fn load_canonical(&self, agent_id: AgentId) -> OpenFangResult<CanonicalSession> {
        self.block_on_pg(async {
            let client = self.pool.get().await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let row = client
                .query_opt(
                    "SELECT messages, compaction_cursor, compacted_summary, updated_at FROM canonical_sessions WHERE agent_id = $1",
                    &[&agent_id.0.to_string()],
                )
                .await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            match row {
                Some(row) => {
                    let messages_blob: Vec<u8> = row.get(0);
                    let cursor: i32 = row.get(1);
                    let summary: Option<String> = row.get(2);
                    let updated_at: chrono::DateTime<chrono::Utc> = row.get(3);
                    let messages: Vec<Message> = helpers::deserialize_messages_lossy(&messages_blob);
                    Ok(CanonicalSession {
                        agent_id, messages, compaction_cursor: cursor as usize,
                        compacted_summary: summary, updated_at: updated_at.to_rfc3339(),
                    })
                }
                None => {
                    // Auto-create
                    let empty = helpers::serialize_messages(&[])?;
                    client.execute(
                        "INSERT INTO canonical_sessions (agent_id, messages, compaction_cursor, updated_at) VALUES ($1, $2, 0, NOW())",
                        &[&agent_id.0.to_string(), &empty],
                    ).await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
                    Ok(CanonicalSession {
                        agent_id, messages: vec![], compaction_cursor: 0,
                        compacted_summary: None, updated_at: chrono::Utc::now().to_rfc3339(),
                    })
                }
            }
        })
    }

    fn save_canonical(&self, canonical: &CanonicalSession) -> OpenFangResult<()> {
        let messages_blob = helpers::serialize_messages(&canonical.messages)?;
        let agent_id_str = canonical.agent_id.0.to_string();
        let cursor = canonical.compaction_cursor as i32;
        let summary = canonical.compacted_summary.clone();
        self.block_on_pg(async {
            let client = self.pool.get().await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            client.execute(
                "INSERT INTO canonical_sessions (agent_id, messages, compaction_cursor, compacted_summary, updated_at)
                 VALUES ($1, $2, $3, $4, NOW())
                 ON CONFLICT (agent_id) DO UPDATE SET messages = $2, compaction_cursor = $3, compacted_summary = $4, updated_at = NOW()",
                &[&agent_id_str, &messages_blob, &cursor, &summary],
            ).await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(())
        })
    }

}
