//! PostgreSQL implementation of the audit log store.
//!
//! Uses a SHA-256 Merkle hash chain matching the scheme in `openfang-runtime`'s
//! `AuditLog`, so entries written through this backend are chain-compatible with
//! entries written directly by the runtime.

use crate::backends::AuditBackend;
use deadpool_postgres::Pool;
use openfang_types::error::{OpenFangError, OpenFangResult};
use sha2::{Digest, Sha256};

pub struct PgAuditStore {
    pool: Pool,
}

impl PgAuditStore {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    fn block_on_pg<F, T>(&self, f: F) -> OpenFangResult<T>
    where
        F: std::future::Future<Output = OpenFangResult<T>>,
    {
        tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(f))
    }
}

/// Compute the SHA-256 hash for a single audit entry, matching the scheme in
/// `openfang-runtime::audit::compute_entry_hash`.
fn compute_entry_hash(
    seq: i64,
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

impl AuditBackend for PgAuditStore {
    fn append_entry(
        &self,
        agent_id: &str,
        action: &str,
        detail: &str,
        outcome: &str,
    ) -> OpenFangResult<()> {
        let agent_id = agent_id.to_string();
        let action = action.to_string();
        let detail = detail.to_string();
        let outcome = outcome.to_string();
        self.block_on_pg(async {
            let client = self
                .pool
                .get()
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let now = chrono::Utc::now().to_rfc3339();

            // Get the previous hash for chain continuity.
            let prev_row = client
                .query_opt(
                    "SELECT hash FROM audit_entries ORDER BY seq DESC LIMIT 1",
                    &[],
                )
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let prev_hash = prev_row
                .map(|r| r.get::<_, String>(0))
                .unwrap_or_else(|| "0".repeat(64));

            // Insert using BIGSERIAL seq (returned by RETURNING).
            let row = client
                .query_one(
                    "INSERT INTO audit_entries (timestamp, agent_id, action, detail, outcome, prev_hash, hash)
                     VALUES ($1, $2, $3, $4, $5, $6, $7)
                     RETURNING seq",
                    &[&now, &agent_id, &action, &detail, &outcome, &prev_hash, &String::new()],
                )
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let seq: i64 = row.get(0);

            // Now compute the hash with the real seq and update.
            let hash = compute_entry_hash(seq, &now, &agent_id, &action, &detail, &outcome, &prev_hash);
            client
                .execute(
                    "UPDATE audit_entries SET hash = $2 WHERE seq = $1",
                    &[&seq, &hash],
                )
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;

            Ok(())
        })
    }

    fn load_entries(
        &self,
        agent_id: Option<&str>,
        limit: usize,
    ) -> OpenFangResult<Vec<serde_json::Value>> {
        let agent_id = agent_id.map(|s| s.to_string());
        self.block_on_pg(async {
            let client = self
                .pool
                .get()
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;

            let limit_i64 = limit as i64;
            let rows = match &agent_id {
                Some(aid) => {
                    client
                        .query(
                            "SELECT seq, timestamp, agent_id, action, detail, outcome, prev_hash, hash
                             FROM audit_entries WHERE agent_id = $1 ORDER BY seq DESC LIMIT $2",
                            &[aid, &limit_i64],
                        )
                        .await
                }
                None => {
                    client
                        .query(
                            "SELECT seq, timestamp, agent_id, action, detail, outcome, prev_hash, hash
                             FROM audit_entries ORDER BY seq DESC LIMIT $1",
                            &[&limit_i64],
                        )
                        .await
                }
            }
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

            Ok(rows
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "seq": r.get::<_, i64>(0),
                        "timestamp": r.get::<_, String>(1),
                        "agent_id": r.get::<_, String>(2),
                        "action": r.get::<_, String>(3),
                        "detail": r.get::<_, String>(4),
                        "outcome": r.get::<_, String>(5),
                        "prev_hash": r.get::<_, String>(6),
                        "hash": r.get::<_, String>(7),
                    })
                })
                .collect())
        })
    }
}
