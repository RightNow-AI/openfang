//! PostgreSQL implementation of the structured (KV + agent) store.

use crate::helpers;
use deadpool_postgres::Pool;
use openfang_types::agent::{AgentEntry, AgentId};
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::storage::StructuredBackend;

/// PostgreSQL-backed structured store.
pub struct PgStructuredStore {
    pool: Pool,
}

impl PgStructuredStore {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    fn block_on_pg<F, T>(&self, f: F) -> OpenFangResult<T>
    where
        F: std::future::Future<Output = OpenFangResult<T>>,
    {
        // The backend traits are synchronous. Use tokio::runtime::Handle to
        // bridge from sync to async when called from spawn_blocking context.
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(f)
        })
    }
}

impl StructuredBackend for PgStructuredStore {
    fn get(&self, agent_id: AgentId, key: &str) -> OpenFangResult<Option<serde_json::Value>> {
        self.block_on_pg(async {
            let client = self.pool.get().await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let row = client
                .query_opt(
                    "SELECT value FROM kv_store WHERE agent_id = $1 AND key = $2",
                    &[&agent_id.0.to_string(), &key],
                )
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            match row {
                Some(row) => {
                    let blob: Vec<u8> = row.get(0);
                    let value: serde_json::Value = serde_json::from_slice(&blob)
                        .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
                    Ok(Some(value))
                }
                None => Ok(None),
            }
        })
    }

    fn set(&self, agent_id: AgentId, key: &str, value: serde_json::Value) -> OpenFangResult<()> {
        self.block_on_pg(async {
            let client = self.pool.get().await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let blob = serde_json::to_vec(&value)
                .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
            client
                .execute(
                    "INSERT INTO kv_store (agent_id, key, value, version, updated_at)
                     VALUES ($1, $2, $3, 1, NOW())
                     ON CONFLICT (agent_id, key) DO UPDATE SET value = $3, version = kv_store.version + 1, updated_at = NOW()",
                    &[&agent_id.0.to_string(), &key, &blob],
                )
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(())
        })
    }

    fn delete(&self, agent_id: AgentId, key: &str) -> OpenFangResult<()> {
        self.block_on_pg(async {
            let client = self.pool.get().await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            client
                .execute(
                    "DELETE FROM kv_store WHERE agent_id = $1 AND key = $2",
                    &[&agent_id.0.to_string(), &key],
                )
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(())
        })
    }

    fn list_kv(&self, agent_id: AgentId) -> OpenFangResult<Vec<(String, serde_json::Value)>> {
        self.block_on_pg(async {
            let client = self.pool.get().await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let rows = client
                .query(
                    "SELECT key, value FROM kv_store WHERE agent_id = $1 ORDER BY key",
                    &[&agent_id.0.to_string()],
                )
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let mut pairs = Vec::new();
            for row in rows {
                let key: String = row.get(0);
                let blob: Vec<u8> = row.get(1);
                let value: serde_json::Value = serde_json::from_slice(&blob)
                    .unwrap_or(serde_json::Value::Null);
                pairs.push((key, value));
            }
            Ok(pairs)
        })
    }

    fn save_agent(&self, entry: &AgentEntry) -> OpenFangResult<()> {
        let manifest_blob = helpers::serialize_manifest(&entry.manifest)?;
        let state_str = serde_json::to_string(&entry.state)
            .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
        let identity_json = serde_json::to_string(&entry.identity)
            .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
        let entry_id = entry.id.0.to_string();
        let entry_name = entry.name.clone();
        let session_id = entry.session_id.0.to_string();

        self.block_on_pg(async {
            let client = self.pool.get().await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            client
                .execute(
                    "INSERT INTO agents (id, name, manifest, state, session_id, identity, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())
                     ON CONFLICT (id) DO UPDATE SET name = $2, manifest = $3, state = $4, session_id = $5, identity = $6, updated_at = NOW()",
                    &[&entry_id, &entry_name, &manifest_blob, &state_str, &session_id, &identity_json],
                )
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(())
        })
    }

    fn load_agent(&self, agent_id: AgentId) -> OpenFangResult<Option<AgentEntry>> {
        self.block_on_pg(async {
            let client = self.pool.get().await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let row = client
                .query_opt(
                    "SELECT id, name, manifest, state, created_at, session_id, identity FROM agents WHERE id = $1",
                    &[&agent_id.0.to_string()],
                )
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;

            match row {
                Some(row) => {
                    let name: String = row.get(1);
                    let manifest_blob: Vec<u8> = row.get(2);
                    let state_str: String = row.get(3);
                    let created_at: chrono::DateTime<chrono::Utc> = row.get(4);
                    let session_id_str: String = row.get(5);
                    let identity_str: String = row.get(6);

                    let manifest = helpers::deserialize_manifest(&manifest_blob)?;
                    let state = serde_json::from_str(&state_str)
                        .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
                    let session_id = helpers::parse_session_id(&session_id_str)
                        .unwrap_or_else(|_| openfang_types::agent::SessionId::new());
                    let identity = serde_json::from_str(&identity_str).unwrap_or_default();

                    Ok(Some(AgentEntry {
                        id: agent_id, name, manifest, state,
                        mode: Default::default(), created_at,
                        last_active: chrono::Utc::now(),
                        parent: None, children: vec![], session_id,
                        tags: vec![], identity,
                        onboarding_completed: false, onboarding_completed_at: None,
                    }))
                }
                None => Ok(None),
            }
        })
    }

    fn remove_agent(&self, agent_id: AgentId) -> OpenFangResult<()> {
        self.block_on_pg(async {
            let client = self.pool.get().await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            client
                .execute("DELETE FROM agents WHERE id = $1", &[&agent_id.0.to_string()])
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(())
        })
    }

    fn load_all_agents(&self) -> OpenFangResult<Vec<AgentEntry>> {
        self.block_on_pg(async {
            let client = self.pool.get().await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let rows = client
                .query(
                    "SELECT id, name, manifest, state, created_at, session_id, identity FROM agents",
                    &[],
                )
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;

            let mut agents = Vec::new();
            for row in rows {
                let id_str: String = row.get(0);
                let name: String = row.get(1);
                let manifest_blob: Vec<u8> = row.get(2);
                let state_str: String = row.get(3);
                let created_at: chrono::DateTime<chrono::Utc> = row.get(4);
                let session_id_str: String = row.get(5);
                let identity_str: String = row.get(6);

                let agent_id = match helpers::parse_agent_id(&id_str) {
                    Ok(id) => id,
                    Err(_) => continue,
                };
                let manifest = match helpers::deserialize_manifest(&manifest_blob) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let state = match serde_json::from_str(&state_str) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let session_id = helpers::parse_session_id(&session_id_str)
                    .unwrap_or_else(|_| openfang_types::agent::SessionId::new());
                let identity = serde_json::from_str(&identity_str).unwrap_or_default();

                agents.push(AgentEntry {
                    id: agent_id, name, manifest, state,
                    mode: Default::default(), created_at,
                    last_active: chrono::Utc::now(),
                    parent: None, children: vec![], session_id,
                    tags: vec![], identity,
                    onboarding_completed: false, onboarding_completed_at: None,
                });
            }
            Ok(agents)
        })
    }

    fn list_agents(&self) -> OpenFangResult<Vec<(String, String, String)>> {
        self.block_on_pg(async {
            let client = self.pool.get().await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let rows = client
                .query("SELECT id, name, state FROM agents", &[])
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(rows.iter().map(|r| (r.get(0), r.get(1), r.get(2))).collect())
        })
    }
}
