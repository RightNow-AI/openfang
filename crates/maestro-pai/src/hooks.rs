//! Learning hooks — capture structured feedback from every agent task
//! interaction. The `LearningStore` provides SurrealDB v3-backed persistence
//! for all learning entries, enabling pattern mining and self-evolution.
//!
//! Migrated from rusqlite to SurrealDB v3 to align with the workspace-wide
//! persistence strategy. All methods are async.
//!
//! ## SurrealDB v3 Pattern
//!
//! SurrealDB v3 does not implement `SurrealValue` for custom structs.
//! All inserts use `CREATE ... CONTENT $data` with `serde_json::Value`
//! bindings. All selects return `Vec<serde_json::Value>` which are then
//! deserialized via `serde_json::from_value`. This matches the pattern
//! used in `openfang-memory`.

use crate::{LearningEntry, LearningError};
use std::collections::HashMap;
use std::sync::Arc;
use surrealdb::engine::local::{Db, Mem, SurrealKv};
use surrealdb::Surreal;
use tracing::{debug, info};

const NS: &str = "maestro";
const DB_NAME: &str = "pai";
const TABLE: &str = "learnings";

/// SurrealDB v3-backed learning store.
///
/// Use [`LearningStore::in_memory()`] for tests and ephemeral sessions,
/// or [`LearningStore::open(path)`] for persistent on-disk storage.
pub struct LearningStore {
    db: Surreal<Db>,
}

impl LearningStore {
    /// Create an in-memory store (backed by SurrealDB `kv-mem`).
    pub async fn in_memory() -> Result<Self, LearningError> {
        let db: Surreal<Db> = Surreal::new::<Mem>(()).await?;
        db.use_ns(NS).use_db(DB_NAME).await?;
        let store = Self { db };
        store.init_schema().await?;
        Ok(store)
    }

    /// Open a persistent store at the given path (backed by `kv-surrealkv`).
    pub async fn open(path: &std::path::Path) -> Result<Self, LearningError> {
        let db: Surreal<Db> = Surreal::new::<SurrealKv>(path).await?;
        db.use_ns(NS).use_db(DB_NAME).await?;
        let store = Self { db };
        store.init_schema().await?;
        Ok(store)
    }

    /// Define the learnings table schema. Called automatically on construction.
    ///
    /// Uses SCHEMALESS so SurrealDB does not conflict with the auto-managed
    /// RecordId `id` field and our application-level `id` UUID string.
    async fn init_schema(&self) -> Result<(), LearningError> {
        self.db
            .query(format!("DEFINE TABLE IF NOT EXISTS {} SCHEMALESS", TABLE))
            .await?;
        Ok(())
    }

    /// Append a new learning entry.
    pub async fn append(&self, entry: &LearningEntry) -> Result<(), LearningError> {
        // Include "id" as a plain string in CONTENT so it is retrievable
        // via SELECT as a JSON string (matching openfang-memory's pattern).
        // SurrealDB stores the RecordId separately from the CONTENT fields.
        let data = serde_json::json!({
            "id": entry.id.to_string(),
            "entry_id": entry.id.to_string(),
            "timestamp": entry.timestamp.to_rfc3339(),
            "task_id": entry.task_id,
            "category": entry.category,
            "insight": entry.insight,
            "context": entry.context,
            "actionable": entry.actionable,
            "user_rating": entry.user_rating,
            "tags": entry.tags,
        });
        self.db
            .query("CREATE type::record('learnings', $id) CONTENT $data")
            .bind(("id", entry.id.to_string()))
            .bind(("data", data))
            .await?;
        debug!(id = %entry.id, category = %entry.category, "Appended learning entry");
        Ok(())
    }

    /// Return all learning entries, oldest first.
    pub async fn all(&self) -> Result<Vec<LearningEntry>, LearningError> {
        let rows: Vec<serde_json::Value> = self
            .db
            .query("SELECT * OMIT id FROM learnings ORDER BY timestamp ASC")
            .await?
            .take(0)?;
        rows.into_iter().map(deserialize_entry).collect()
    }

    /// Return all entries for a given category.
    pub async fn by_category(&self, category: &str) -> Result<Vec<LearningEntry>, LearningError> {
        let rows: Vec<serde_json::Value> = self
            .db
            .query("SELECT * OMIT id FROM learnings WHERE category = $cat ORDER BY timestamp DESC")
            .bind(("cat", category.to_string()))
            .await?
            .take(0)?;
        rows.into_iter().map(deserialize_entry).collect()
    }

    /// Return all actionable entries.
    pub async fn actionable(&self) -> Result<Vec<LearningEntry>, LearningError> {
        let rows: Vec<serde_json::Value> = self
            .db
            .query("SELECT * OMIT id FROM learnings WHERE actionable = true ORDER BY timestamp DESC")
            .await?
            .take(0)?;
        rows.into_iter().map(deserialize_entry).collect()
    }

    /// Return the N most recent entries.
    pub async fn recent(&self, n: usize) -> Result<Vec<LearningEntry>, LearningError> {
        let rows: Vec<serde_json::Value> = self
            .db
            .query("SELECT * OMIT id FROM learnings ORDER BY timestamp DESC LIMIT $n")
            .bind(("n", n as i64))
            .await?
            .take(0)?;
        rows.into_iter().map(deserialize_entry).collect()
    }

    /// Return a map of category → count.
    pub async fn count_by_category(&self) -> Result<HashMap<String, usize>, LearningError> {
        let rows: Vec<serde_json::Value> = self
            .db
            .query("SELECT * OMIT id FROM learnings")
            .await?
            .take(0)?;
        let mut map: HashMap<String, usize> = HashMap::new();
        for row in rows {
            if let Some(cat) = row.get("category").and_then(|v| v.as_str()) {
                *map.entry(cat.to_string()).or_insert(0) += 1;
            }
        }
        Ok(map)
    }

    /// Total number of entries.
    pub async fn len(&self) -> Result<usize, LearningError> {
        let rows: Vec<serde_json::Value> = self
            .db
            .query("SELECT * OMIT id FROM learnings")
            .await?
            .take(0)?;
        Ok(rows.len())
    }

    /// True if the store has no entries.
    pub async fn is_empty(&self) -> Result<bool, LearningError> {
        Ok(self.len().await? == 0)
    }
}

/// Deserialize a `serde_json::Value` row from SurrealDB into a `LearningEntry`.
fn deserialize_entry(row: serde_json::Value) -> Result<LearningEntry, LearningError> {
    use chrono::DateTime;
    use uuid::Uuid;

    // The record's SurrealDB ID is a RecordId object; our application UUID
    // is stored as "entry_id" (a plain string) to avoid conflicts.
    let id = row
        .get("entry_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(Uuid::new_v4);

    let timestamp = row
        .get("timestamp")
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(chrono::Utc::now);

    let task_id = row
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let category = row
        .get("category")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let insight = row
        .get("insight")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let context = row
        .get("context")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let actionable = row
        .get("actionable")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let user_rating = row
        .get("user_rating")
        .and_then(|v| v.as_u64())
        .map(|n| n as u8);

    let tags: Vec<String> = row
        .get("tags")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    Ok(LearningEntry {
        id,
        timestamp,
        task_id,
        category,
        insight,
        context,
        actionable,
        user_rating,
        tags,
    })
}

/// Convenience wrapper that records learnings by category.
pub struct LearningHook {
    store: Arc<LearningStore>,
}

impl LearningHook {
    pub fn new(store: Arc<LearningStore>) -> Self {
        Self { store }
    }

    /// Record an ALGORITHM-category learning.
    pub async fn record_algorithm(
        &self,
        task_id: &str,
        insight: &str,
    ) -> Result<(), LearningError> {
        self.record(task_id, "ALGORITHM", insight, "algorithm-execution", true)
            .await
    }

    /// Record a FAILURE-category learning.
    pub async fn record_failure(
        &self,
        task_id: &str,
        insight: &str,
    ) -> Result<(), LearningError> {
        self.record(task_id, "FAILURE", insight, "task-execution", true)
            .await
    }

    /// Record a SYNTHESIS-category learning.
    pub async fn record_synthesis(
        &self,
        task_id: &str,
        insight: &str,
    ) -> Result<(), LearningError> {
        self.record(task_id, "SYNTHESIS", insight, "pattern-synthesis", false)
            .await
    }

    /// Record a REFLECTION-category learning.
    pub async fn record_reflection(
        &self,
        task_id: &str,
        insight: &str,
    ) -> Result<(), LearningError> {
        self.record(task_id, "REFLECTION", insight, "post-task-review", false)
            .await
    }

    async fn record(
        &self,
        task_id: &str,
        category: &str,
        insight: &str,
        context: &str,
        actionable: bool,
    ) -> Result<(), LearningError> {
        use chrono::Utc;
        use uuid::Uuid;
        let entry = crate::LearningEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            task_id: task_id.to_string(),
            category: category.to_string(),
            insight: insight.to_string(),
            context: context.to_string(),
            actionable,
            user_rating: None,
            tags: vec![],
        };
        self.store.append(&entry).await?;
        info!(
            task = task_id,
            category = category,
            insight = %insight.chars().take(60).collect::<String>(),
            "Recorded learning"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LearningEntry;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_entry(task_id: &str, category: &str, insight: &str) -> LearningEntry {
        LearningEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            task_id: task_id.to_string(),
            category: category.to_string(),
            insight: insight.to_string(),
            context: "test context".to_string(),
            actionable: true,
            user_rating: None,
            tags: vec![],
        }
    }

    #[tokio::test]
    async fn test_append_and_all() {
        let store = LearningStore::in_memory().await.unwrap();
        assert!(store.is_empty().await.unwrap());
        store
            .append(&make_entry("t1", "ALGORITHM", "Use parallel execution"))
            .await
            .unwrap();
        store
            .append(&make_entry("t2", "FAILURE", "Timeout on large inputs"))
            .await
            .unwrap();
        assert_eq!(store.len().await.unwrap(), 2);
        let all = store.all().await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_by_category() {
        let store = LearningStore::in_memory().await.unwrap();
        store
            .append(&make_entry("t1", "ALGORITHM", "insight 1"))
            .await
            .unwrap();
        store
            .append(&make_entry("t2", "ALGORITHM", "insight 2"))
            .await
            .unwrap();
        store
            .append(&make_entry("t3", "FAILURE", "failure 1"))
            .await
            .unwrap();
        let alg = store.by_category("ALGORITHM").await.unwrap();
        assert_eq!(alg.len(), 2);
        let fail = store.by_category("FAILURE").await.unwrap();
        assert_eq!(fail.len(), 1);
        let empty = store.by_category("SYNTHESIS").await.unwrap();
        assert!(empty.is_empty());
    }

    #[tokio::test]
    async fn test_actionable() {
        let store = LearningStore::in_memory().await.unwrap();
        let mut e1 = make_entry("t1", "ALGORITHM", "actionable insight");
        e1.actionable = true;
        let mut e2 = make_entry("t2", "REFLECTION", "non-actionable");
        e2.actionable = false;
        store.append(&e1).await.unwrap();
        store.append(&e2).await.unwrap();
        let actionable = store.actionable().await.unwrap();
        assert_eq!(actionable.len(), 1);
        assert_eq!(actionable[0].insight, "actionable insight");
    }

    #[tokio::test]
    async fn test_recent() {
        let store = LearningStore::in_memory().await.unwrap();
        for i in 0..5 {
            store
                .append(&make_entry(
                    &format!("t{i}"),
                    "ALGORITHM",
                    &format!("insight {i}"),
                ))
                .await
                .unwrap();
        }
        let recent = store.recent(3).await.unwrap();
        assert_eq!(recent.len(), 3);
    }

    #[tokio::test]
    async fn test_count_by_category() {
        let store = LearningStore::in_memory().await.unwrap();
        for _ in 0..3 {
            store
                .append(&make_entry("t1", "ALGORITHM", "algo insight"))
                .await
                .unwrap();
        }
        store
            .append(&make_entry("t4", "FAILURE", "a failure"))
            .await
            .unwrap();
        let counts = store.count_by_category().await.unwrap();
        assert_eq!(counts["ALGORITHM"], 3);
        assert_eq!(counts["FAILURE"], 1);
    }

    #[tokio::test]
    async fn test_learning_hook() {
        let store = Arc::new(LearningStore::in_memory().await.unwrap());
        let hook = LearningHook::new(store.clone());
        hook.record_algorithm("task-1", "Use parallel execution for independent sub-tasks")
            .await
            .unwrap();
        hook.record_failure("task-1", "Sequential execution timed out")
            .await
            .unwrap();
        hook.record_synthesis("task-2", "Parallel + caching = 10x speedup")
            .await
            .unwrap();
        assert_eq!(store.len().await.unwrap(), 3);
        let algo = store.by_category("ALGORITHM").await.unwrap();
        assert_eq!(algo.len(), 1);
        assert_eq!(algo[0].task_id, "task-1");
    }
}
