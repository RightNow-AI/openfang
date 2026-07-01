//! LoopRegistry — persistent store for Loop records.
//!
//! A Loop ties together:
//!   - a long-running agent (spawned from a manifest)
//!   - a binding (cron job OR event trigger) that drives it
//!   - metadata about safety limits and run history location
//!
//! Persistence mirrors the CronScheduler pattern (kernel.rs:820): JSON file under
//! `config.home_dir`, atomically rewritten on every mutation.

use serde::{Deserialize, Serialize};
use std::io;
use std::path::PathBuf;
use std::sync::RwLock;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Safety guardrails embedded in each loop definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopSafety {
    /// File-path prefixes the loop agent must not write to.
    pub denylist_paths: Vec<String>,
    /// Maximum consecutive tool-call iterations per run.
    pub max_attempts: u32,
    /// Max PRs the loop may open in a calendar day.
    pub daily_pr_cap: u32,
    /// Approximate LLM token ceiling per run (input + output combined).
    pub token_budget: u64,
}

impl Default for LoopSafety {
    fn default() -> Self {
        Self {
            denylist_paths: vec![],
            max_attempts: 20,
            daily_pr_cap: 5,
            token_budget: 100_000,
        }
    }
}

/// What kind of binding drives the loop.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    /// Repeating schedule — e.g. "every 3600 secs" or cron expr.
    Every,
    Cron,
    /// File-system / process event via kernel trigger subsystem.
    Event,
}

/// Persistent record for one loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopRecord {
    /// Stable UUID string assigned at creation time.
    pub id: String,
    /// Human-readable name / pattern tag (e.g. "nightly-review").
    pub pattern_id: String,
    /// Workspace root path the loop operates on.
    pub workspace: String,
    /// Arbitrary level label (e.g. "low", "medium", "high") for steering.
    pub level: String,
    /// Discriminant for the binding type.
    pub trigger_kind: TriggerKind,
    /// For every: number of seconds as string; for cron: the expr; for event: JSON pattern.
    pub trigger_param: String,
    /// Safety configuration.
    pub safety: LoopSafety,
    /// Raw TOML manifest used to spawn the agent.
    pub manifest_toml: String,
    /// Message sent to the agent on each scheduled fire.
    pub turn_prompt: String,
    /// UUID string of the live agent (set after successful spawn).
    pub agent_id: Option<String>,
    /// UUID string of the cron job or trigger id that drives the agent.
    pub binding_id: Option<String>,
    /// Whether the loop is currently active.
    pub enabled: bool,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// In-process registry backed by a JSON file on disk.
///
/// All public methods are synchronous because mutations are cheap local
/// JSON serialisation; no async I/O needed.  The RwLock is an `std` lock so
/// handlers calling `state.loop_registry.*` do not need to be async.
pub struct LoopRegistry {
    /// Path to `<home_dir>/loops.json`.
    path: PathBuf,
    records: RwLock<Vec<LoopRecord>>,
}

impl LoopRegistry {
    /// Load from disk, or start with an empty list if the file is absent.
    pub fn load(path: PathBuf) -> Self {
        let records = if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str::<Vec<LoopRecord>>(&s).ok())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        Self {
            path,
            records: RwLock::new(records),
        }
    }

    /// Write current records to disk atomically (write to `.tmp`, then rename).
    pub fn persist(&self) -> io::Result<()> {
        let records = self
            .records
            .read()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "lock poisoned"))?;
        let json = serde_json::to_vec_pretty(&*records)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        // Atomic write: temp file alongside the real path, then rename.
        let tmp_path = self.path.with_extension("json.tmp");
        std::fs::write(&tmp_path, &json)?;
        std::fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }

    /// Insert a new record. Persists immediately.
    pub fn insert(&self, record: LoopRecord) -> io::Result<()> {
        {
            let mut records = self
                .records
                .write()
                .map_err(|_| io::Error::new(io::ErrorKind::Other, "lock poisoned"))?;
            records.push(record);
        }
        self.persist()
    }

    /// Remove by id. Returns true if found. Persists if found.
    pub fn remove(&self, id: &str) -> io::Result<bool> {
        let found = {
            let mut records = self
                .records
                .write()
                .map_err(|_| io::Error::new(io::ErrorKind::Other, "lock poisoned"))?;
            let before = records.len();
            records.retain(|r| r.id != id);
            records.len() < before
        };
        if found {
            self.persist()?;
        }
        Ok(found)
    }

    /// Get a clone of the record matching `id`.
    pub fn get(&self, id: &str) -> Option<LoopRecord> {
        self.records
            .read()
            .ok()?
            .iter()
            .find(|r| r.id == id)
            .cloned()
    }

    /// Replace the record with matching id. Persists if found.
    pub fn update(&self, id: &str, updated: LoopRecord) -> io::Result<bool> {
        let found = {
            let mut records = self
                .records
                .write()
                .map_err(|_| io::Error::new(io::ErrorKind::Other, "lock poisoned"))?;
            if let Some(slot) = records.iter_mut().find(|r| r.id == id) {
                *slot = updated;
                true
            } else {
                false
            }
        };
        if found {
            self.persist()?;
        }
        Ok(found)
    }

    /// Return a snapshot of all records.
    pub fn list(&self) -> Vec<LoopRecord> {
        self.records
            .read()
            .map(|g| g.clone())
            .unwrap_or_default()
    }
}
