//! Merkle hash chain audit trail for security-critical actions.
//!
//! Every auditable event is appended to an append-only log where each entry
//! contains the SHA-256 hash of its own contents concatenated with the hash of
//! the previous entry, forming a tamper-evident chain (similar to a blockchain).
//!
//! When a backend is provided (`with_backend`), entries are persisted to the
//! configured database (SQLite or PostgreSQL) so the trail survives restarts.

use chrono::Utc;
use openfang_memory::backends::AuditBackend;
use openfang_types::error::{OpenFangError, OpenFangResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

/// Categories of auditable actions within the agent runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditAction {
    ToolInvoke,
    CapabilityCheck,
    AgentSpawn,
    AgentKill,
    AgentMessage,
    MemoryAccess,
    FileAccess,
    NetworkAccess,
    ShellExec,
    AuthAttempt,
    WireConnect,
    ConfigChange,
}

impl std::fmt::Display for AuditAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// A single entry in the Merkle hash chain audit log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Monotonically increasing sequence number (0-indexed).
    pub seq: u64,
    /// ISO-8601 timestamp of when this entry was recorded.
    pub timestamp: String,
    /// The agent that triggered (or is the subject of) this action.
    pub agent_id: String,
    /// The category of action being audited.
    pub action: AuditAction,
    /// Free-form detail about the action (e.g. tool name, file path).
    pub detail: String,
    /// The outcome of the action (e.g. "ok", "denied", an error message).
    pub outcome: String,
    /// SHA-256 hash of the previous entry (or all-zeros for the genesis).
    pub prev_hash: String,
    /// SHA-256 hash of this entry's content concatenated with `prev_hash`.
    pub hash: String,
}

/// Computes the SHA-256 hash for a single audit entry from its fields.
fn compute_entry_hash(
    seq: u64,
    timestamp: &str,
    agent_id: &str,
    action: &AuditAction,
    detail: &str,
    outcome: &str,
    prev_hash: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(seq.to_string().as_bytes());
    hasher.update(timestamp.as_bytes());
    hasher.update(agent_id.as_bytes());
    hasher.update(action.to_string().as_bytes());
    hasher.update(detail.as_bytes());
    hasher.update(outcome.as_bytes());
    hasher.update(prev_hash.as_bytes());
    hex::encode(hasher.finalize())
}

/// Parse an action string back into an `AuditAction` enum.
fn parse_action(s: &str) -> AuditAction {
    match s {
        "ToolInvoke" => AuditAction::ToolInvoke,
        "CapabilityCheck" => AuditAction::CapabilityCheck,
        "AgentSpawn" => AuditAction::AgentSpawn,
        "AgentKill" => AuditAction::AgentKill,
        "AgentMessage" => AuditAction::AgentMessage,
        "MemoryAccess" => AuditAction::MemoryAccess,
        "FileAccess" => AuditAction::FileAccess,
        "NetworkAccess" => AuditAction::NetworkAccess,
        "ShellExec" => AuditAction::ShellExec,
        "AuthAttempt" => AuditAction::AuthAttempt,
        "WireConnect" => AuditAction::WireConnect,
        "ConfigChange" => AuditAction::ConfigChange,
        _ => AuditAction::ToolInvoke, // fallback
    }
}

/// An append-only, tamper-evident audit log using a Merkle hash chain.
///
/// Thread-safe — all access is serialised through internal mutexes.
/// Optionally backed by a persistent store (SQLite or PostgreSQL) via the
/// `AuditBackend` trait.
pub struct AuditLog {
    entries: Mutex<Vec<AuditEntry>>,
    tip: Mutex<String>,
    /// Optional persistent backend (works with any configured database).
    db: Option<Arc<dyn AuditBackend>>,
}

impl AuditLog {
    /// Creates a new empty audit log (in-memory only, no persistence).
    ///
    /// The initial tip hash is 64 zero characters (the "genesis" sentinel).
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
            tip: Mutex::new("0".repeat(64)),
            db: None,
        }
    }

    /// Creates an audit log backed by a persistent backend.
    ///
    /// On construction, loads all existing entries from the backend and
    /// verifies the Merkle chain integrity. New entries are written to both
    /// the in-memory chain and the backend.
    ///
    /// Fails-closed: if the backend cannot return its entries or the chain
    /// integrity check fails, an error is returned and the kernel must refuse
    /// to boot. A silent restart with a broken audit trail is not acceptable.
    pub fn with_backend(backend: Arc<dyn AuditBackend>) -> OpenFangResult<Self> {
        let mut entries = Vec::new();
        let mut tip = "0".repeat(64);

        // Load existing entries from backend. Propagate any error -- a
        // security-critical audit trail must not silently start empty.
        let rows = backend.load_entries(None, usize::MAX).map_err(|e| {
            tracing::error!("Audit backend load_entries failed on boot: {e}");
            OpenFangError::Memory(format!(
                "audit backend load_entries failed on boot: {e}"
            ))
        })?;

        for row in rows {
            let seq = row["seq"].as_u64().unwrap_or(0);
            let timestamp = row["timestamp"].as_str().unwrap_or("").to_string();
            let agent_id = row["agent_id"].as_str().unwrap_or("").to_string();
            let action_str = row["action"].as_str().unwrap_or("ToolInvoke");
            let detail = row["detail"].as_str().unwrap_or("").to_string();
            let outcome = row["outcome"].as_str().unwrap_or("").to_string();
            let prev_hash = row["prev_hash"].as_str().unwrap_or("").to_string();
            let hash = row["hash"].as_str().unwrap_or("").to_string();

            tip = hash.clone();
            entries.push(AuditEntry {
                seq,
                timestamp,
                agent_id,
                action: parse_action(action_str),
                detail,
                outcome,
                prev_hash,
                hash,
            });
        }

        let count = entries.len();
        let log = Self {
            entries: Mutex::new(entries),
            tip: Mutex::new(tip),
            db: Some(backend),
        };

        // Verify chain integrity on load. A failed integrity check on a
        // persistent audit trail means the chain has been tampered with or
        // truncated -- refuse to boot rather than pretend everything is fine.
        if count > 0 {
            if let Err(e) = log.verify_integrity() {
                tracing::error!("Audit trail integrity check FAILED on boot: {e}");
                return Err(OpenFangError::Memory(format!(
                    "audit trail integrity check failed on boot: {e}"
                )));
            }
            tracing::info!("Audit trail loaded: {count} entries, chain integrity OK");
        }

        Ok(log)
    }

    /// Records a new auditable event and returns the SHA-256 hash of the entry.
    ///
    /// The entry is atomically appended to the chain with the current tip as
    /// its `prev_hash`, and the tip is advanced to the new hash.
    /// If a backend is available, the entry is also persisted.
    pub fn record(
        &self,
        agent_id: impl Into<String>,
        action: AuditAction,
        detail: impl Into<String>,
        outcome: impl Into<String>,
    ) -> String {
        let agent_id = agent_id.into();
        let detail = detail.into();
        let outcome = outcome.into();
        let timestamp = Utc::now().to_rfc3339();

        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let mut tip = self.tip.lock().unwrap_or_else(|e| e.into_inner());

        let seq = entries.len() as u64;
        let prev_hash = tip.clone();

        let hash = compute_entry_hash(
            seq, &timestamp, &agent_id, &action, &detail, &outcome, &prev_hash,
        );

        let entry = AuditEntry {
            seq,
            timestamp,
            agent_id,
            action,
            detail,
            outcome,
            prev_hash,
            hash: hash.clone(),
        };

        // Persist to backend if available
        if let Some(ref db) = self.db {
            let _ = db.append_entry(
                &entry.agent_id,
                &entry.action.to_string(),
                &entry.detail,
                &entry.outcome,
            );
        }

        entries.push(entry);
        *tip = hash.clone();
        hash
    }

    /// Walks the entire chain and recomputes every hash to detect tampering.
    ///
    /// Returns `Ok(())` if the chain is intact, or `Err(msg)` describing
    /// the first inconsistency found.
    pub fn verify_integrity(&self) -> Result<(), String> {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let mut expected_prev = "0".repeat(64);

        for entry in entries.iter() {
            if entry.prev_hash != expected_prev {
                return Err(format!(
                    "chain break at seq {}: expected prev_hash {} but found {}",
                    entry.seq, expected_prev, entry.prev_hash
                ));
            }

            let recomputed = compute_entry_hash(
                entry.seq,
                &entry.timestamp,
                &entry.agent_id,
                &entry.action,
                &entry.detail,
                &entry.outcome,
                &entry.prev_hash,
            );

            if recomputed != entry.hash {
                return Err(format!(
                    "hash mismatch at seq {}: expected {} but found {}",
                    entry.seq, recomputed, entry.hash
                ));
            }

            expected_prev = entry.hash.clone();
        }

        Ok(())
    }

    /// Returns the current tip hash (the hash of the most recent entry,
    /// or the genesis sentinel if the log is empty).
    pub fn tip_hash(&self) -> String {
        self.tip.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Returns the number of entries in the log.
    pub fn len(&self) -> usize {
        self.entries.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Returns whether the log is empty.
    pub fn is_empty(&self) -> bool {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
    }

    /// Returns up to the most recent `n` entries (cloned).
    pub fn recent(&self, n: usize) -> Vec<AuditEntry> {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let start = entries.len().saturating_sub(n);
        entries[start..].to_vec()
    }
}

impl Default for AuditLog {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_chain_integrity() {
        let log = AuditLog::new();
        log.record(
            "agent-1",
            AuditAction::ToolInvoke,
            "read_file /etc/passwd",
            "ok",
        );
        log.record("agent-1", AuditAction::ShellExec, "ls -la", "ok");
        log.record("agent-2", AuditAction::AgentSpawn, "spawning helper", "ok");
        log.record(
            "agent-1",
            AuditAction::NetworkAccess,
            "https://example.com",
            "denied",
        );

        assert_eq!(log.len(), 4);
        assert!(log.verify_integrity().is_ok());

        // Verify the chain links are correct
        let entries = log.recent(4);
        assert_eq!(entries[0].prev_hash, "0".repeat(64));
        assert_eq!(entries[1].prev_hash, entries[0].hash);
        assert_eq!(entries[2].prev_hash, entries[1].hash);
        assert_eq!(entries[3].prev_hash, entries[2].hash);
    }

    #[test]
    fn test_audit_tamper_detection() {
        let log = AuditLog::new();
        log.record("agent-1", AuditAction::ToolInvoke, "read_file /tmp/a", "ok");
        log.record("agent-1", AuditAction::ShellExec, "rm -rf /", "denied");
        log.record("agent-1", AuditAction::MemoryAccess, "read key foo", "ok");

        // Tamper with an entry
        {
            let mut entries = log.entries.lock().unwrap();
            entries[1].detail = "echo hello".to_string(); // change the detail
        }

        let result = log.verify_integrity();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("hash mismatch at seq 1"));
    }

    #[test]
    fn test_audit_tip_changes() {
        let log = AuditLog::new();
        let genesis_tip = log.tip_hash();
        assert_eq!(genesis_tip, "0".repeat(64));

        let h1 = log.record("a", AuditAction::AgentSpawn, "spawn", "ok");
        assert_eq!(log.tip_hash(), h1);
        assert_ne!(log.tip_hash(), genesis_tip);

        let h2 = log.record("b", AuditAction::AgentKill, "kill", "ok");
        assert_eq!(log.tip_hash(), h2);
        assert_ne!(h2, h1);
    }
}
