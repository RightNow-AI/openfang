//! Learning hooks — capture structured feedback from every task execution.
//!
//! The `LearningHook` appends structured JSONL entries after every agent
//! interaction. The `LearningStore` provides SQLite-backed persistence
//! with query capabilities for pattern mining.

use crate::{LearningEntry, Pattern};
use anyhow::Result;
use chrono::Utc;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::{debug, info};
use uuid::Uuid;

/// In-memory learning store backed by SQLite.
pub struct LearningStore {
    conn: Arc<Mutex<Connection>>,
}

impl LearningStore {
    /// Create a new in-memory store (for testing).
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn: Arc::new(Mutex::new(conn)) };
        store.init_schema()?;
        Ok(store)
    }

    /// Open or create a persistent store at the given path.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        let store = Self { conn: Arc::new(Mutex::new(conn)) };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS learnings (
                id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL,
                task_id TEXT NOT NULL,
                category TEXT NOT NULL,
                insight TEXT NOT NULL,
                context TEXT NOT NULL,
                actionable INTEGER NOT NULL DEFAULT 0,
                user_rating INTEGER,
                tags TEXT NOT NULL DEFAULT '[]'
            );
            CREATE INDEX IF NOT EXISTS idx_learnings_category ON learnings(category);
            CREATE INDEX IF NOT EXISTS idx_learnings_task ON learnings(task_id);
            CREATE INDEX IF NOT EXISTS idx_learnings_actionable ON learnings(actionable);
        ")?;
        Ok(())
    }

    /// Append a learning entry.
    pub fn append(&self, entry: &LearningEntry) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let tags = serde_json::to_string(&entry.tags)?;
        conn.execute(
            "INSERT OR REPLACE INTO learnings
             (id, timestamp, task_id, category, insight, context, actionable, user_rating, tags)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                entry.id.to_string(),
                entry.timestamp.to_rfc3339(),
                entry.task_id,
                entry.category,
                entry.insight,
                entry.context,
                entry.actionable as i32,
                entry.user_rating.map(|r| r as i32),
                tags,
            ],
        )?;
        debug!("Appended learning {} ({})", entry.id, entry.category);
        Ok(())
    }

    /// Query learnings by category.
    pub fn by_category(&self, category: &str) -> Result<Vec<LearningEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, task_id, category, insight, context, actionable, user_rating, tags
             FROM learnings WHERE category = ?1 ORDER BY timestamp DESC"
        )?;
        let entries = stmt.query_map(params![category], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i32>(6)?,
                row.get::<_, Option<i32>>(7)?,
                row.get::<_, String>(8)?,
            ))
        })?.filter_map(|r| r.ok()).map(|(id, ts, task_id, cat, insight, ctx, actionable, rating, tags)| {
            LearningEntry {
                id: Uuid::parse_str(&id).unwrap_or_default(),
                timestamp: ts.parse().unwrap_or_else(|_| Utc::now()),
                task_id,
                category: cat,
                insight,
                context: ctx,
                actionable: actionable != 0,
                user_rating: rating.map(|r| r as u8),
                tags: serde_json::from_str(&tags).unwrap_or_default(),
            }
        }).collect();
        Ok(entries)
    }

    /// Get all actionable learnings.
    pub fn actionable(&self) -> Result<Vec<LearningEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, task_id, category, insight, context, actionable, user_rating, tags
             FROM learnings WHERE actionable = 1 ORDER BY timestamp DESC"
        )?;
        let entries = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i32>(6)?,
                row.get::<_, Option<i32>>(7)?,
                row.get::<_, String>(8)?,
            ))
        })?.filter_map(|r| r.ok()).map(|(id, ts, task_id, cat, insight, ctx, actionable, rating, tags)| {
            LearningEntry {
                id: Uuid::parse_str(&id).unwrap_or_default(),
                timestamp: ts.parse().unwrap_or_else(|_| Utc::now()),
                task_id,
                category: cat,
                insight,
                context: ctx,
                actionable: actionable != 0,
                user_rating: rating.map(|r| r as u8),
                tags: serde_json::from_str(&tags).unwrap_or_default(),
            }
        }).collect();
        Ok(entries)
    }

    /// Count learnings by category.
    pub fn count_by_category(&self) -> Result<std::collections::HashMap<String, u64>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT category, COUNT(*) FROM learnings GROUP BY category")?;
        let mut map = std::collections::HashMap::new();
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows.filter_map(|r| r.ok()) {
            map.insert(row.0, row.1 as u64);
        }
        Ok(map)
    }

    /// Total count of learnings.
    pub fn total_count(&self) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM learnings", [], |r| r.get(0))?;
        Ok(count as u64)
    }
}

/// Learning hook — call this after every agent task to capture insights.
pub struct LearningHook {
    store: Arc<LearningStore>,
}

impl LearningHook {
    pub fn new(store: Arc<LearningStore>) -> Self {
        Self { store }
    }

    /// Record a learning from a completed task.
    pub fn record(
        &self,
        task_id: &str,
        category: &str,
        insight: &str,
        context: &str,
        actionable: bool,
    ) -> Result<LearningEntry> {
        let entry = LearningEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            task_id: task_id.to_string(),
            category: category.to_string(),
            insight: insight.to_string(),
            context: context.to_string(),
            actionable,
            user_rating: None,
            tags: Vec::new(),
        };
        self.store.append(&entry)?;
        info!("Recorded learning: {} ({})", insight.chars().take(60).collect::<String>(), category);
        Ok(entry)
    }

    /// Record a failure learning.
    pub fn record_failure(&self, task_id: &str, error: &str, context: &str) -> Result<LearningEntry> {
        self.record(task_id, "FAILURE", error, context, true)
    }

    /// Record an algorithm insight.
    pub fn record_algorithm(&self, task_id: &str, insight: &str) -> Result<LearningEntry> {
        self.record(task_id, "ALGORITHM", insight, "algorithm-execution", true)
    }

    /// Record a synthesis insight.
    pub fn record_synthesis(&self, task_id: &str, insight: &str) -> Result<LearningEntry> {
        self.record(task_id, "SYNTHESIS", insight, "pattern-synthesis", false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_learning_store_append_and_query() {
        let store = LearningStore::in_memory().unwrap();
        let hook = LearningHook::new(Arc::new(store));
        hook.record("task-1", "ALGORITHM", "Use parallel execution for independent tasks", "phase-3", true).unwrap();
        hook.record("task-1", "FAILURE", "Timeout on large context", "phase-5", true).unwrap();
        hook.record("task-2", "SYNTHESIS", "Agents work better with explicit constraints", "pattern-mining", false).unwrap();

        let store = LearningStore::in_memory().unwrap();
        // Re-test with direct store
        let entry = LearningEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            task_id: "t1".to_string(),
            category: "ALGORITHM".to_string(),
            insight: "test insight".to_string(),
            context: "ctx".to_string(),
            actionable: true,
            user_rating: Some(4),
            tags: vec!["perf".to_string()],
        };
        store.append(&entry).unwrap();
        assert_eq!(store.total_count().unwrap(), 1);

        let by_cat = store.by_category("ALGORITHM").unwrap();
        assert_eq!(by_cat.len(), 1);
        assert_eq!(by_cat[0].insight, "test insight");

        let actionable = store.actionable().unwrap();
        assert_eq!(actionable.len(), 1);
    }

    #[test]
    fn test_count_by_category() {
        let store = LearningStore::in_memory().unwrap();
        for i in 0..3 {
            store.append(&LearningEntry {
                id: Uuid::new_v4(),
                timestamp: Utc::now(),
                task_id: format!("t{}", i),
                category: "ALGORITHM".to_string(),
                insight: format!("insight {}", i),
                context: "ctx".to_string(),
                actionable: true,
                user_rating: None,
                tags: vec![],
            }).unwrap();
        }
        store.append(&LearningEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            task_id: "t4".to_string(),
            category: "FAILURE".to_string(),
            insight: "a failure".to_string(),
            context: "ctx".to_string(),
            actionable: true,
            user_rating: None,
            tags: vec![],
        }).unwrap();

        let counts = store.count_by_category().unwrap();
        assert_eq!(counts["ALGORITHM"], 3);
        assert_eq!(counts["FAILURE"], 1);
    }
}
