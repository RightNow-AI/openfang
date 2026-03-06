//! SQLite structured store for key-value pairs and agent persistence.

use chrono::Utc;
use openfang_types::agent::{AgentEntry, AgentId};
use openfang_types::error::{OpenFangError, OpenFangResult};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

/// Structured store backed by SQLite for key-value operations and agent storage.
#[derive(Clone)]
pub struct StructuredStore {
    conn: Arc<Mutex<Connection>>,
}

impl StructuredStore {
    /// Create a new structured store wrapping the given connection.
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    /// Get a value from the key-value store.
    pub fn get(&self, agent_id: AgentId, key: &str) -> OpenFangResult<Option<serde_json::Value>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let mut stmt = conn
            .prepare("SELECT value FROM kv_store WHERE agent_id = ?1 AND key = ?2")
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let result = stmt.query_row(rusqlite::params![agent_id.0.to_string(), key], |row| {
            let blob: Vec<u8> = row.get(0)?;
            Ok(blob)
        });
        match result {
            Ok(blob) => {
                let value: serde_json::Value = serde_json::from_slice(&blob)
                    .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
                Ok(Some(value))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(OpenFangError::Memory(e.to_string())),
        }
    }

    /// Set a value in the key-value store.
    pub fn set(
        &self,
        agent_id: AgentId,
        key: &str,
        value: serde_json::Value,
    ) -> OpenFangResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let blob =
            serde_json::to_vec(&value).map_err(|e| OpenFangError::Serialization(e.to_string()))?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO kv_store (agent_id, key, value, version, updated_at) VALUES (?1, ?2, ?3, 1, ?4)
             ON CONFLICT(agent_id, key) DO UPDATE SET value = ?3, version = version + 1, updated_at = ?4",
            rusqlite::params![agent_id.0.to_string(), key, blob, now],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Delete a value from the key-value store.
    pub fn delete(&self, agent_id: AgentId, key: &str) -> OpenFangResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        conn.execute(
            "DELETE FROM kv_store WHERE agent_id = ?1 AND key = ?2",
            rusqlite::params![agent_id.0.to_string(), key],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// List all key-value pairs for an agent.
    pub fn list_kv(&self, agent_id: AgentId) -> OpenFangResult<Vec<(String, serde_json::Value)>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let mut stmt = conn
            .prepare("SELECT key, value FROM kv_store WHERE agent_id = ?1 ORDER BY key")
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let rows = stmt
            .query_map(rusqlite::params![agent_id.0.to_string()], |row| {
                let key: String = row.get(0)?;
                let blob: Vec<u8> = row.get(1)?;
                Ok((key, blob))
            })
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut pairs = Vec::new();
        for row in rows {
            let (key, blob) = row.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let value: serde_json::Value = serde_json::from_slice(&blob)
                .unwrap_or_else(|_| {
                    // Fallback: try as UTF-8 string
                    String::from_utf8(blob)
                        .map(serde_json::Value::String)
                        .unwrap_or(serde_json::Value::Null)
                });
            pairs.push((key, value));
        }
        Ok(pairs)
    }

    /// Save an agent entry to the database.
    pub fn save_agent(&self, entry: &AgentEntry) -> OpenFangResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        // Use named-field encoding so new fields with #[serde(default)] are
        // handled gracefully when the struct evolves between versions.
        let manifest_blob = rmp_serde::to_vec_named(&entry.manifest)
            .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
        let state_str = serde_json::to_string(&entry.state)
            .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
        let now = Utc::now().to_rfc3339();

        // Add session_id column if it doesn't exist yet (migration compat)
        let _ = conn.execute(
            "ALTER TABLE agents ADD COLUMN session_id TEXT DEFAULT ''",
            [],
        );
        // Add identity column (migration compat)
        let _ = conn.execute(
            "ALTER TABLE agents ADD COLUMN identity TEXT DEFAULT '{}'",
            [],
        );

        let identity_json = serde_json::to_string(&entry.identity)
            .map_err(|e| OpenFangError::Serialization(e.to_string()))?;

        conn.execute(
            "INSERT INTO agents (id, name, manifest, state, created_at, updated_at, session_id, identity)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET name = ?2, manifest = ?3, state = ?4, updated_at = ?6, session_id = ?7, identity = ?8",
            rusqlite::params![
                entry.id.0.to_string(),
                entry.name,
                manifest_blob,
                state_str,
                entry.created_at.to_rfc3339(),
                now,
                entry.session_id.0.to_string(),
                identity_json,
            ],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Load an agent entry from the database.
    pub fn load_agent(&self, agent_id: AgentId) -> OpenFangResult<Option<AgentEntry>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;

        let mut stmt = conn
            .prepare("SELECT id, name, manifest, state, created_at, updated_at, session_id, identity FROM agents WHERE id = ?1")
            .or_else(|_| {
                conn.prepare("SELECT id, name, manifest, state, created_at, updated_at, session_id FROM agents WHERE id = ?1")
                    .or_else(|_| {
                        // Fallback without session_id column for old DBs
                        conn.prepare("SELECT id, name, manifest, state, created_at, updated_at FROM agents WHERE id = ?1")
                    })
            })
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let col_count = stmt.column_count();
        let result = stmt.query_row(rusqlite::params![agent_id.0.to_string()], |row| {
            let manifest_blob: Vec<u8> = row.get(2)?;
            let state_str: String = row.get(3)?;
            let created_str: String = row.get(4)?;
            let name: String = row.get(1)?;
            let session_id_str: Option<String> = if col_count >= 7 {
                row.get(6).ok()
            } else {
                None
            };
            let identity_str: Option<String> = if col_count >= 8 {
                row.get(7).ok()
            } else {
                None
            };
            Ok((name, manifest_blob, state_str, created_str, session_id_str, identity_str))
        });

        match result {
            Ok((name, manifest_blob, state_str, created_str, session_id_str, identity_str)) => {
                let manifest = rmp_serde::from_slice(&manifest_blob)
                    .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
                let state = serde_json::from_str(&state_str)
                    .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
                let created_at = chrono::DateTime::parse_from_rfc3339(&created_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                let session_id = session_id_str
                    .and_then(|s| uuid::Uuid::parse_str(&s).ok())
                    .map(openfang_types::agent::SessionId)
                    .unwrap_or_else(openfang_types::agent::SessionId::new);
                let identity = identity_str
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_default();
                Ok(Some(AgentEntry {
                    id: agent_id,
                    name,
                    manifest,
                    state,
                    mode: Default::default(),
                    created_at,
                    last_active: Utc::now(),
                    parent: None,
                    children: vec![],
                    session_id,
                    tags: vec![],
                    identity,
                    onboarding_completed: false,
                    onboarding_completed_at: None,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(OpenFangError::Memory(e.to_string())),
        }
    }

    /// Remove an agent from the database.
    pub fn remove_agent(&self, agent_id: AgentId) -> OpenFangResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        conn.execute(
            "DELETE FROM agents WHERE id = ?1",
            rusqlite::params![agent_id.0.to_string()],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Load all agent entries from the database.
    ///
    /// Uses lenient deserialization (via `serde_compat`) to handle schema-mismatched
    /// fields gracefully. When an agent is loaded with lenient defaults, it is
    /// automatically re-saved to upgrade the stored blob. Duplicate agent names
    /// are deduplicated (first occurrence wins).
    pub fn load_all_agents(&self) -> OpenFangResult<Vec<AgentEntry>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;

        // Try with identity+session_id columns first, fall back gracefully
        let mut stmt = conn
            .prepare(
                "SELECT id, name, manifest, state, created_at, updated_at, session_id, identity FROM agents",
            )
            .or_else(|_| {
                conn.prepare("SELECT id, name, manifest, state, created_at, updated_at, session_id FROM agents")
            })
            .or_else(|_| {
                conn.prepare("SELECT id, name, manifest, state, created_at, updated_at FROM agents")
            })
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let col_count = stmt.column_count();
        let rows = stmt
            .query_map([], |row| {
                let id_str: String = row.get(0)?;
                let name: String = row.get(1)?;
                let manifest_blob: Vec<u8> = row.get(2)?;
                let state_str: String = row.get(3)?;
                let created_str: String = row.get(4)?;
                let session_id_str: Option<String> = if col_count >= 7 {
                    row.get(6).ok()
                } else {
                    None
                };
                let identity_str: Option<String> = if col_count >= 8 {
                    row.get(7).ok()
                } else {
                    None
                };
                Ok((
                    id_str,
                    name,
                    manifest_blob,
                    state_str,
                    created_str,
                    session_id_str,
                    identity_str,
                ))
            })
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut agents = Vec::new();
        let mut seen_names = std::collections::HashSet::new();
        let mut repair_queue: Vec<(String, Vec<u8>, String)> = Vec::new();

        for row in rows {
            let (id_str, name, manifest_blob, state_str, created_str, session_id_str, identity_str) = match row {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("Skipping agent row with read error: {e}");
                    continue;
                }
            };

            // Deduplicate: skip agents with names we've already seen
            let name_lower = name.to_lowercase();
            if !seen_names.insert(name_lower) {
                tracing::info!(agent = %name, id = %id_str, "Skipping duplicate agent name");
                continue;
            }

            let agent_id = match uuid::Uuid::parse_str(&id_str).map(openfang_types::agent::AgentId)
            {
                Ok(id) => id,
                Err(e) => {
                    tracing::warn!(agent = %name, "Skipping agent with bad UUID '{id_str}': {e}");
                    continue;
                }
            };

            let manifest: openfang_types::agent::AgentManifest = match rmp_serde::from_slice(
                &manifest_blob,
            ) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!(
                        agent = %name, id = %id_str,
                        "Skipping agent with incompatible manifest (schema may have changed): {e}"
                    );
                    continue;
                }
            };

            // Auto-repair: re-serialize with current schema and queue for update.
            // This upgrades the stored blob so future boots don't hit lenient paths.
            let new_blob = rmp_serde::to_vec_named(&manifest)
                .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
            if new_blob != manifest_blob {
                tracing::info!(
                    agent = %name, id = %id_str,
                    "Auto-repaired agent manifest (schema upgraded)"
                );
                repair_queue.push((id_str.clone(), new_blob, name.clone()));
            }

            let state = match serde_json::from_str(&state_str) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(agent = %name, "Skipping agent with bad state: {e}");
                    continue;
                }
            };
            let created_at = chrono::DateTime::parse_from_rfc3339(&created_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            let session_id = session_id_str
                .and_then(|s| uuid::Uuid::parse_str(&s).ok())
                .map(openfang_types::agent::SessionId)
                .unwrap_or_else(openfang_types::agent::SessionId::new);

            let identity = identity_str
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();

            agents.push(AgentEntry {
                id: agent_id,
                name,
                manifest,
                state,
                mode: Default::default(),
                created_at,
                last_active: Utc::now(),
                parent: None,
                children: vec![],
                session_id,
                tags: vec![],
                identity,
                onboarding_completed: false,
                onboarding_completed_at: None,
            });
        }

        // Apply queued repairs (re-save upgraded blobs)
        for (id_str, new_blob, name) in repair_queue {
            if let Err(e) = conn.execute(
                "UPDATE agents SET manifest = ?1 WHERE id = ?2",
                rusqlite::params![new_blob, id_str],
            ) {
                tracing::warn!(agent = %name, "Failed to auto-repair agent blob: {e}");
            }
        }

        Ok(agents)
    }

    /// List all agents in the database.
    pub fn list_agents(&self) -> OpenFangResult<Vec<(String, String, String)>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let mut stmt = conn
            .prepare("SELECT id, name, state FROM agents")
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let mut agents = Vec::new();
        for row in rows {
            agents.push(row.map_err(|e| OpenFangError::Memory(e.to_string()))?);
        }
        Ok(agents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::run_migrations;

    fn setup() -> StructuredStore {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        StructuredStore::new(Arc::new(Mutex::new(conn)))
    }

    #[test]
    fn test_kv_set_get() {
        let store = setup();
        let agent_id = AgentId::new();
        store
            .set(agent_id, "test_key", serde_json::json!("test_value"))
            .unwrap();
        let value = store.get(agent_id, "test_key").unwrap();
        assert_eq!(value, Some(serde_json::json!("test_value")));
    }

    #[test]
    fn test_kv_get_missing() {
        let store = setup();
        let agent_id = AgentId::new();
        let value = store.get(agent_id, "nonexistent").unwrap();
        assert!(value.is_none());
    }

    #[test]
    fn test_kv_delete() {
        let store = setup();
        let agent_id = AgentId::new();
        store
            .set(agent_id, "to_delete", serde_json::json!(42))
            .unwrap();
        store.delete(agent_id, "to_delete").unwrap();
        let value = store.get(agent_id, "to_delete").unwrap();
        assert!(value.is_none());
    }

    #[test]
    fn test_kv_update() {
        let store = setup();
        let agent_id = AgentId::new();
        store.set(agent_id, "key", serde_json::json!("v1")).unwrap();
        store.set(agent_id, "key", serde_json::json!("v2")).unwrap();
        let value = store.get(agent_id, "key").unwrap();
        assert_eq!(value, Some(serde_json::json!("v2")));
    }
}
