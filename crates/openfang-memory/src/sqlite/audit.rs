//! SQLite implementation of the audit log store.
//!
//! Uses a SHA-256 Merkle hash chain matching the scheme in `openfang-runtime`'s
//! `AuditLog`, so entries written through this backend are chain-compatible with
//! entries written directly by the runtime.

use crate::backends::AuditBackend;
use openfang_types::error::{OpenFangError, OpenFangResult};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

/// Audit-log store backed by SQLite with Merkle hash chain integrity.
#[derive(Clone)]
pub struct SqliteAuditStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteAuditStore {
    /// Create a new audit store wrapping the given connection.
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }
}

/// Compute the SHA-256 hash for a single audit entry, matching the scheme in
/// `openfang-runtime::audit::compute_entry_hash`.
fn compute_entry_hash(
    seq: u64,
    timestamp: &str,
    agent_id: &str,
    action: &str,
    detail: &str,
    outcome: &str,
    prev_hash: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(seq.to_string().as_bytes());
    hasher.update(timestamp.as_bytes());
    hasher.update(agent_id.as_bytes());
    hasher.update(action.as_bytes());
    hasher.update(detail.as_bytes());
    hasher.update(outcome.as_bytes());
    hasher.update(prev_hash.as_bytes());
    hex::encode(hasher.finalize())
}

impl AuditBackend for SqliteAuditStore {
    fn append_entry(
        &self,
        agent_id: &str,
        action: &str,
        detail: &str,
        outcome: &str,
    ) -> OpenFangResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let now = chrono::Utc::now().to_rfc3339();

        // Determine the next sequence number and the previous hash for chain continuity.
        let (seq, prev_hash): (u64, String) = conn
            .query_row(
                "SELECT COALESCE(MAX(seq) + 1, 0), COALESCE((SELECT hash FROM audit_entries ORDER BY seq DESC LIMIT 1), ?1) FROM audit_entries",
                rusqlite::params!["0".repeat(64)],
                |row| Ok((row.get::<_, i64>(0)? as u64, row.get::<_, String>(1)?)),
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let hash = compute_entry_hash(seq, &now, agent_id, action, detail, outcome, &prev_hash);

        conn.execute(
            "INSERT INTO audit_entries (seq, timestamp, agent_id, action, detail, outcome, prev_hash, hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![seq as i64, now, agent_id, action, detail, outcome, prev_hash, hash],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        Ok(())
    }

    fn load_entries(
        &self,
        agent_id: Option<&str>,
        limit: usize,
    ) -> OpenFangResult<Vec<serde_json::Value>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let (sql, params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = match agent_id {
            Some(aid) => (
                "SELECT seq, timestamp, agent_id, action, detail, outcome, prev_hash, hash FROM audit_entries WHERE agent_id = ?1 ORDER BY seq DESC LIMIT ?2",
                vec![Box::new(aid.to_string()), Box::new(limit as i64)],
            ),
            None => (
                "SELECT seq, timestamp, agent_id, action, detail, outcome, prev_hash, hash FROM audit_entries ORDER BY seq DESC LIMIT ?1",
                vec![Box::new(limit as i64)],
            ),
        };

        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(serde_json::json!({
                    "seq": row.get::<_, i64>(0)?,
                    "timestamp": row.get::<_, String>(1)?,
                    "agent_id": row.get::<_, String>(2)?,
                    "action": row.get::<_, String>(3)?,
                    "detail": row.get::<_, String>(4)?,
                    "outcome": row.get::<_, String>(5)?,
                    "prev_hash": row.get::<_, String>(6)?,
                    "hash": row.get::<_, String>(7)?,
                }))
            })
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row.map_err(|e| OpenFangError::Memory(e.to_string()))?);
        }
        Ok(entries)
    }
}
