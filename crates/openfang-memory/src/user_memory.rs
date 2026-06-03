//! Per-user persistent memory organized into named topics.
//!
//! Dream writes topic entries at session end; injection reads the index
//! (topic names + summaries) at session start and fetches full topic content
//! per-message for relevant topics.
//!
//! Topics can optionally carry an `expires_at` timestamp for time-sensitive facts
//! (travel plans, temporary preferences). Expired topics are pruned at read time.
//! The `embedding` column stores a packed little-endian f32 vector for optional
//! cosine-similarity retrieval; it is populated asynchronously after write and
//! may be NULL for topics created before embedding was configured.

use chrono::{DateTime, Utc};
use openfang_types::agent::UserId;
use openfang_types::error::{OpenFangError, OpenFangResult};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

/// A single memory topic for a user.
#[derive(Debug, Clone)]
pub struct MemoryTopic {
    pub user_id: UserId,
    /// Topic key, e.g. "work_context", "preferences", "open_items".
    pub topic: String,
    /// One-line description shown in the session-start index.
    pub summary: String,
    /// Full topic content injected on per-turn retrieval.
    pub content: String,
    pub updated_at: DateTime<Utc>,
    /// Optional expiry for time-sensitive facts. Expired topics are invisible at read time.
    pub expires_at: Option<DateTime<Utc>>,
}

/// Lightweight index entry for a topic (no full content).
#[derive(Debug, Clone)]
pub struct TopicIndexEntry {
    pub topic: String,
    pub summary: String,
    pub updated_at: DateTime<Utc>,
}

/// User memory store backed by SQLite.
#[derive(Clone)]
pub struct UserMemoryStore {
    conn: Arc<Mutex<Connection>>,
}

impl UserMemoryStore {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    /// Upsert a topic — replaces existing content for this (user_id, topic) pair.
    pub fn upsert_topic(&self, topic: &MemoryTopic) -> OpenFangResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        conn.execute(
            "INSERT INTO user_memory_topics (user_id, topic, summary, content, updated_at, expires_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT(user_id, topic) DO UPDATE SET \
               summary = ?3, content = ?4, updated_at = ?5, expires_at = ?6",
            rusqlite::params![
                topic.user_id.0.to_string(),
                topic.topic,
                topic.summary,
                topic.content,
                topic.updated_at.to_rfc3339(),
                topic.expires_at.as_ref().map(|d| d.to_rfc3339()),
            ],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Delete a single topic for a user. Used by conflict resolution (supersedes).
    pub fn delete_topic(&self, user_id: UserId, topic: &str) -> OpenFangResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        conn.execute(
            "DELETE FROM user_memory_topics WHERE user_id = ?1 AND topic = ?2",
            rusqlite::params![user_id.0.to_string(), topic],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Get the index (topic names + summaries) for a user — no full content.
    /// Expired topics are excluded.
    pub fn get_index(&self, user_id: UserId) -> OpenFangResult<Vec<TopicIndexEntry>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT topic, summary, updated_at FROM user_memory_topics \
                 WHERE user_id = ?1 \
                   AND (expires_at IS NULL OR datetime(expires_at) > datetime('now')) \
                 ORDER BY topic ASC",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let rows = stmt
            .query_map(rusqlite::params![user_id.0.to_string()], |row| {
                let topic: String = row.get(0)?;
                let summary: String = row.get(1)?;
                let updated_at_str: String = row.get(2)?;
                Ok((topic, summary, updated_at_str))
            })
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut entries = Vec::new();
        for row in rows {
            let (topic, summary, updated_at_str) =
                row.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let updated_at = updated_at_str
                .parse::<DateTime<Utc>>()
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            entries.push(TopicIndexEntry {
                topic,
                summary,
                updated_at,
            });
        }
        Ok(entries)
    }

    /// Get full content for a specific topic. Returns None if the topic does not exist
    /// or has expired.
    pub fn get_topic(&self, user_id: UserId, topic: &str) -> OpenFangResult<Option<MemoryTopic>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT summary, content, updated_at, expires_at FROM user_memory_topics \
                 WHERE user_id = ?1 AND topic = ?2 \
                   AND (expires_at IS NULL OR datetime(expires_at) > datetime('now'))",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let result = stmt.query_row(rusqlite::params![user_id.0.to_string(), topic], |row| {
            let summary: String = row.get(0)?;
            let content: String = row.get(1)?;
            let updated_at_str: String = row.get(2)?;
            let expires_at_str: Option<String> = row.get(3)?;
            Ok((summary, content, updated_at_str, expires_at_str))
        });

        match result {
            Ok((summary, content, updated_at_str, expires_at_str)) => {
                let updated_at = updated_at_str
                    .parse::<DateTime<Utc>>()
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;
                let expires_at = expires_at_str
                    .map(|s| {
                        s.parse::<DateTime<Utc>>()
                            .map_err(|e| OpenFangError::Memory(e.to_string()))
                    })
                    .transpose()?;
                Ok(Some(MemoryTopic {
                    user_id,
                    topic: topic.to_string(),
                    summary,
                    content,
                    updated_at,
                    expires_at,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(OpenFangError::Memory(e.to_string())),
        }
    }

    /// Store a pre-computed embedding for a topic.
    ///
    /// Embeddings are packed as little-endian f32 bytes. Called asynchronously
    /// after `upsert_topic` when an embedding provider is configured.
    pub fn store_embedding(
        &self,
        user_id: UserId,
        topic: &str,
        embedding: &[f32],
    ) -> OpenFangResult<()> {
        let blob = pack_embedding(embedding);
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        conn.execute(
            "UPDATE user_memory_topics SET embedding = ?1 WHERE user_id = ?2 AND topic = ?3",
            rusqlite::params![blob, user_id.0.to_string(), topic],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Search topics by cosine similarity to a query embedding.
    ///
    /// Returns topic names sorted by descending similarity, limited to `top_k` results.
    /// Topics without an embedding are skipped. Falls back to an empty list if no
    /// embeddings are stored (caller should use the LLM selector instead).
    pub fn search_by_embedding(
        &self,
        user_id: UserId,
        query: &[f32],
        top_k: usize,
    ) -> OpenFangResult<Vec<String>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT topic, embedding FROM user_memory_topics \
                 WHERE user_id = ?1 AND embedding IS NOT NULL \
                   AND (expires_at IS NULL OR datetime(expires_at) > datetime('now'))",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let rows = stmt
            .query_map(rusqlite::params![user_id.0.to_string()], |row| {
                let topic: String = row.get(0)?;
                let blob: Vec<u8> = row.get(1)?;
                Ok((topic, blob))
            })
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut scored: Vec<(String, f32)> = Vec::new();
        for row in rows {
            let (topic, blob) = row.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let emb = unpack_embedding(&blob);
            let score = cosine_similarity(query, &emb);
            scored.push((topic, score));
        }

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored.into_iter().take(top_k).map(|(t, _)| t).collect())
    }

    /// Delete all memory for a user.
    ///
    /// Returns the number of rows deleted so that callers (e.g. the wipe
    /// endpoint) can report a per-bucket count back to the user.
    pub fn delete_user_memory(&self, user_id: UserId) -> OpenFangResult<usize> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let n = conn
            .execute(
                "DELETE FROM user_memory_topics WHERE user_id = ?1",
                rusqlite::params![user_id.0.to_string()],
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(n)
    }

    /// Delete expired topics for a user. Called opportunistically — expiry is also
    /// enforced at read time, but periodic pruning keeps the table tidy.
    pub fn prune_expired(&self, user_id: UserId) -> OpenFangResult<usize> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let n = conn
            .execute(
                "DELETE FROM user_memory_topics \
                 WHERE user_id = ?1 AND expires_at IS NOT NULL AND datetime(expires_at) <= datetime('now')",
                rusqlite::params![user_id.0.to_string()],
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(n)
    }
}

// ── Embedding helpers ────────────────────────────────────────────────────────

/// Pack f32 slice to little-endian bytes.
fn pack_embedding(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for &x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

/// Unpack little-endian bytes to f32 slice.
fn unpack_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Cosine similarity between two equal-length vectors. Returns 0.0 on zero vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::run_migrations;

    fn setup() -> UserMemoryStore {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        UserMemoryStore::new(Arc::new(Mutex::new(conn)))
    }

    fn make_topic(user_id: UserId, topic: &str, summary: &str, content: &str) -> MemoryTopic {
        MemoryTopic {
            user_id,
            topic: topic.to_string(),
            summary: summary.to_string(),
            content: content.to_string(),
            updated_at: Utc::now(),
            expires_at: None,
        }
    }

    #[test]
    fn test_upsert_and_get_topic() {
        let store = setup();
        let user_id = UserId::new();
        let t = make_topic(user_id, "work_context", "Work summary", "I work at Acme.");
        store.upsert_topic(&t).unwrap();

        let loaded = store.get_topic(user_id, "work_context").unwrap().unwrap();
        assert_eq!(loaded.topic, "work_context");
        assert_eq!(loaded.summary, "Work summary");
        assert_eq!(loaded.content, "I work at Acme.");
        assert!(loaded.expires_at.is_none());
    }

    #[test]
    fn test_upsert_replaces_existing() {
        let store = setup();
        let user_id = UserId::new();
        let t1 = make_topic(user_id, "preferences", "Prefs v1", "I prefer tea.");
        store.upsert_topic(&t1).unwrap();

        let t2 = make_topic(user_id, "preferences", "Prefs v2", "I prefer coffee.");
        store.upsert_topic(&t2).unwrap();

        let loaded = store.get_topic(user_id, "preferences").unwrap().unwrap();
        assert_eq!(loaded.summary, "Prefs v2");
        assert_eq!(loaded.content, "I prefer coffee.");
    }

    #[test]
    fn test_get_index_returns_no_content() {
        let store = setup();
        let user_id = UserId::new();
        let t1 = make_topic(user_id, "aaa", "Summary A", "Full content A");
        let t2 = make_topic(user_id, "bbb", "Summary B", "Full content B");
        store.upsert_topic(&t1).unwrap();
        store.upsert_topic(&t2).unwrap();

        let index = store.get_index(user_id).unwrap();
        assert_eq!(index.len(), 2);
        assert_eq!(index[0].topic, "aaa");
        assert_eq!(index[0].summary, "Summary A");
        assert_eq!(index[1].topic, "bbb");
        assert_eq!(index[1].summary, "Summary B");
    }

    #[test]
    fn test_get_missing_topic_returns_none() {
        let store = setup();
        let user_id = UserId::new();
        let result = store.get_topic(user_id, "nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_different_users_isolated() {
        let store = setup();
        let user_a = UserId::new();
        let user_b = UserId::new();
        let t = make_topic(user_a, "private", "A's summary", "A's content");
        store.upsert_topic(&t).unwrap();

        let result = store.get_topic(user_b, "private").unwrap();
        assert!(result.is_none());

        let index = store.get_index(user_b).unwrap();
        assert!(index.is_empty());
    }

    #[test]
    fn test_delete_user_memory() {
        let store = setup();
        let user_id = UserId::new();
        let t1 = make_topic(user_id, "t1", "s1", "c1");
        let t2 = make_topic(user_id, "t2", "s2", "c2");
        store.upsert_topic(&t1).unwrap();
        store.upsert_topic(&t2).unwrap();

        store.delete_user_memory(user_id).unwrap();

        let index = store.get_index(user_id).unwrap();
        assert!(index.is_empty());
    }

    #[test]
    fn test_delete_single_topic() {
        let store = setup();
        let user_id = UserId::new();
        store
            .upsert_topic(&make_topic(user_id, "keep", "keep", "keep"))
            .unwrap();
        store
            .upsert_topic(&make_topic(user_id, "remove", "remove", "remove"))
            .unwrap();

        store.delete_topic(user_id, "remove").unwrap();

        let index = store.get_index(user_id).unwrap();
        assert_eq!(index.len(), 1);
        assert_eq!(index[0].topic, "keep");
    }

    #[test]
    fn test_expired_topic_invisible() {
        let store = setup();
        let user_id = UserId::new();
        // Topic that expired 1 hour ago
        let past = Utc::now() - chrono::Duration::hours(1);
        let t = MemoryTopic {
            user_id,
            topic: "expired".to_string(),
            summary: "old".to_string(),
            content: "stale content".to_string(),
            updated_at: Utc::now(),
            expires_at: Some(past),
        };
        store.upsert_topic(&t).unwrap();

        // Should not be returned by get_topic or get_index
        assert!(store.get_topic(user_id, "expired").unwrap().is_none());
        assert!(store.get_index(user_id).unwrap().is_empty());
    }

    #[test]
    fn test_non_expired_topic_visible() {
        let store = setup();
        let user_id = UserId::new();
        let future = Utc::now() + chrono::Duration::days(7);
        let t = MemoryTopic {
            user_id,
            topic: "travel".to_string(),
            summary: "upcoming trip".to_string(),
            content: "Paris trip on 2026-04-10".to_string(),
            updated_at: Utc::now(),
            expires_at: Some(future),
        };
        store.upsert_topic(&t).unwrap();

        let loaded = store.get_topic(user_id, "travel").unwrap();
        assert!(loaded.is_some());
        let index = store.get_index(user_id).unwrap();
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn test_prune_expired() {
        let store = setup();
        let user_id = UserId::new();
        let past = Utc::now() - chrono::Duration::hours(1);
        let future = Utc::now() + chrono::Duration::days(1);

        store
            .upsert_topic(&MemoryTopic {
                user_id,
                topic: "expired".into(),
                summary: "".into(),
                content: "".into(),
                updated_at: Utc::now(),
                expires_at: Some(past),
            })
            .unwrap();
        store
            .upsert_topic(&MemoryTopic {
                user_id,
                topic: "valid".into(),
                summary: "".into(),
                content: "".into(),
                updated_at: Utc::now(),
                expires_at: Some(future),
            })
            .unwrap();
        store
            .upsert_topic(&make_topic(user_id, "permanent", "", ""))
            .unwrap();

        let pruned = store.prune_expired(user_id).unwrap();
        assert_eq!(pruned, 1);

        // Permanent and future-expiry remain
        let conn = store.conn.lock().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM user_memory_topics WHERE user_id = ?1",
                rusqlite::params![user_id.0.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0f32, 0.0, 0.0];
        let b = vec![1.0f32, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-5);

        let c = vec![0.0f32, 1.0, 0.0];
        assert!((cosine_similarity(&a, &c)).abs() < 1e-5);
    }

    #[test]
    fn test_embedding_store_and_search() {
        let store = setup();
        let user_id = UserId::new();
        store
            .upsert_topic(&make_topic(
                user_id,
                "rust",
                "Rust programming",
                "loves Rust",
            ))
            .unwrap();
        store
            .upsert_topic(&make_topic(
                user_id,
                "python",
                "Python programming",
                "also uses Python",
            ))
            .unwrap();

        // Embed "rust" topic with [1,0,0] and "python" with [0,1,0]
        store
            .store_embedding(user_id, "rust", &[1.0, 0.0, 0.0])
            .unwrap();
        store
            .store_embedding(user_id, "python", &[0.0, 1.0, 0.0])
            .unwrap();

        // Query closest to [1,0,0] — should be "rust"
        let results = store
            .search_by_embedding(user_id, &[1.0, 0.0, 0.0], 1)
            .unwrap();
        assert_eq!(results, vec!["rust".to_string()]);

        // Query closest to [0,1,0] — should be "python"
        let results = store
            .search_by_embedding(user_id, &[0.0, 1.0, 0.0], 1)
            .unwrap();
        assert_eq!(results, vec!["python".to_string()]);
    }
}
