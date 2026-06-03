//! Per-user-per-agent persistent memory organized into named topics.
//!
//! Stores memory specific to how a particular user interacts with a particular agent.
//! Separate from `user_memory` (general facts about a user across all agents).
//!
//! Examples:
//! - User Philippe + agent Jeeves: "Philippe wants Jeeves to use military time"
//! - User Philippe + agent google-mail-hand: "Philippe marks emails from acme.com as urgent"
//!
//! Dream writes these at session end. Injection reads the index at session start
//! and fetches relevant topic content per-message.

use chrono::{DateTime, Utc};
use openfang_types::agent::{AgentId, UserId};
use openfang_types::error::{OpenFangError, OpenFangResult};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

/// A single memory topic scoped to a (user, agent) pair.
#[derive(Debug, Clone)]
pub struct UserAgentMemoryTopic {
    pub user_id: UserId,
    pub agent_id: AgentId,
    /// Topic key, e.g. "interaction_preferences", "delegated_tasks", "known_context".
    pub topic: String,
    /// One-line description shown in the session-start index.
    pub summary: String,
    /// Full topic content injected on per-turn retrieval.
    pub content: String,
    pub updated_at: DateTime<Utc>,
}

/// Lightweight index entry (no full content).
#[derive(Debug, Clone)]
pub struct UserAgentTopicIndexEntry {
    pub topic: String,
    pub summary: String,
    pub updated_at: DateTime<Utc>,
}

/// Store for per-user-per-agent memory topics backed by SQLite.
#[derive(Clone)]
pub struct UserAgentMemoryStore {
    conn: Arc<Mutex<Connection>>,
}

impl UserAgentMemoryStore {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    /// Upsert a topic — replaces existing content for this (user_id, agent_id, topic).
    pub fn upsert_topic(&self, topic: &UserAgentMemoryTopic) -> OpenFangResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        conn.execute(
            "INSERT INTO user_agent_memory_topics \
             (user_id, agent_id, topic, summary, content, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT(user_id, agent_id, topic) \
             DO UPDATE SET summary = ?4, content = ?5, updated_at = ?6",
            rusqlite::params![
                topic.user_id.0.to_string(),
                topic.agent_id.0.to_string(),
                topic.topic,
                topic.summary,
                topic.content,
                topic.updated_at.to_rfc3339(),
            ],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Get the index (topic names + summaries) for a (user, agent) pair — no full content.
    pub fn get_index(
        &self,
        user_id: UserId,
        agent_id: AgentId,
    ) -> OpenFangResult<Vec<UserAgentTopicIndexEntry>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT topic, summary, updated_at FROM user_agent_memory_topics \
                 WHERE user_id = ?1 AND agent_id = ?2 ORDER BY topic ASC",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let rows = stmt
            .query_map(
                rusqlite::params![user_id.0.to_string(), agent_id.0.to_string()],
                |row| {
                    let topic: String = row.get(0)?;
                    let summary: String = row.get(1)?;
                    let updated_at_str: String = row.get(2)?;
                    Ok((topic, summary, updated_at_str))
                },
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut entries = Vec::new();
        for row in rows {
            let (topic, summary, updated_at_str) =
                row.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let updated_at = updated_at_str
                .parse::<DateTime<Utc>>()
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            entries.push(UserAgentTopicIndexEntry {
                topic,
                summary,
                updated_at,
            });
        }
        Ok(entries)
    }

    /// Get full content for a specific topic.
    pub fn get_topic(
        &self,
        user_id: UserId,
        agent_id: AgentId,
        topic: &str,
    ) -> OpenFangResult<Option<UserAgentMemoryTopic>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT summary, content, updated_at FROM user_agent_memory_topics \
                 WHERE user_id = ?1 AND agent_id = ?2 AND topic = ?3",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let result = stmt.query_row(
            rusqlite::params![user_id.0.to_string(), agent_id.0.to_string(), topic],
            |row| {
                let summary: String = row.get(0)?;
                let content: String = row.get(1)?;
                let updated_at_str: String = row.get(2)?;
                Ok((summary, content, updated_at_str))
            },
        );

        match result {
            Ok((summary, content, updated_at_str)) => {
                let updated_at = updated_at_str
                    .parse::<DateTime<Utc>>()
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;
                Ok(Some(UserAgentMemoryTopic {
                    user_id,
                    agent_id,
                    topic: topic.to_string(),
                    summary,
                    content,
                    updated_at,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(OpenFangError::Memory(e.to_string())),
        }
    }

    /// Delete all memory for a (user, agent) pair.
    ///
    /// Returns the number of rows deleted so the control API can report a
    /// per-bucket count back to the caller, matching the shape used by the
    /// other delete methods on this store.
    pub fn delete_user_agent_memory(
        &self,
        user_id: UserId,
        agent_id: AgentId,
    ) -> OpenFangResult<usize> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let n = conn
            .execute(
                "DELETE FROM user_agent_memory_topics WHERE user_id = ?1 AND agent_id = ?2",
                rusqlite::params![user_id.0.to_string(), agent_id.0.to_string()],
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(n)
    }

    /// Delete all memory across all agents for a user (e.g. on account deletion).
    ///
    /// Returns the number of rows deleted so the wipe endpoint can report a
    /// per-bucket count back to the caller.
    pub fn delete_all_user_memory(&self, user_id: UserId) -> OpenFangResult<usize> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let n = conn
            .execute(
                "DELETE FROM user_agent_memory_topics WHERE user_id = ?1",
                rusqlite::params![user_id.0.to_string()],
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::run_migrations;

    fn setup() -> UserAgentMemoryStore {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        UserAgentMemoryStore::new(Arc::new(Mutex::new(conn)))
    }

    fn make_topic(
        user_id: UserId,
        agent_id: AgentId,
        topic: &str,
        summary: &str,
        content: &str,
    ) -> UserAgentMemoryTopic {
        UserAgentMemoryTopic {
            user_id,
            agent_id,
            topic: topic.to_string(),
            summary: summary.to_string(),
            content: content.to_string(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_upsert_and_get_topic() {
        let store = setup();
        let user_id = UserId::new();
        let agent_id = AgentId::new();
        let t = make_topic(
            user_id,
            agent_id,
            "interaction_preferences",
            "How user prefers this agent to behave",
            "Always use military time. Never suggest meetings before 9am.",
        );
        store.upsert_topic(&t).unwrap();

        let loaded = store
            .get_topic(user_id, agent_id, "interaction_preferences")
            .unwrap()
            .unwrap();
        assert_eq!(loaded.topic, "interaction_preferences");
        assert_eq!(loaded.agent_id, agent_id);
        assert_eq!(loaded.user_id, user_id);
    }

    #[test]
    fn test_upsert_replaces_existing() {
        let store = setup();
        let user_id = UserId::new();
        let agent_id = AgentId::new();
        let t1 = make_topic(user_id, agent_id, "prefs", "v1", "Use formal tone.");
        store.upsert_topic(&t1).unwrap();
        let t2 = make_topic(user_id, agent_id, "prefs", "v2", "Use casual tone.");
        store.upsert_topic(&t2).unwrap();

        let loaded = store
            .get_topic(user_id, agent_id, "prefs")
            .unwrap()
            .unwrap();
        assert_eq!(loaded.summary, "v2");
        assert_eq!(loaded.content, "Use casual tone.");
    }

    #[test]
    fn test_get_index_returns_no_content() {
        let store = setup();
        let user_id = UserId::new();
        let agent_id = AgentId::new();
        let t1 = make_topic(user_id, agent_id, "aaa", "Summary A", "Full content A");
        let t2 = make_topic(user_id, agent_id, "bbb", "Summary B", "Full content B");
        store.upsert_topic(&t1).unwrap();
        store.upsert_topic(&t2).unwrap();

        let index = store.get_index(user_id, agent_id).unwrap();
        assert_eq!(index.len(), 2);
        assert_eq!(index[0].topic, "aaa");
        assert_eq!(index[1].topic, "bbb");
    }

    #[test]
    fn test_get_missing_topic_returns_none() {
        let store = setup();
        let result = store
            .get_topic(UserId::new(), AgentId::new(), "nonexistent")
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_different_users_isolated() {
        let store = setup();
        let user_a = UserId::new();
        let user_b = UserId::new();
        let agent_id = AgentId::new();
        let t = make_topic(user_a, agent_id, "secret", "A's topic", "A's content");
        store.upsert_topic(&t).unwrap();

        // user_b should not see user_a's topics for the same agent
        let result = store.get_topic(user_b, agent_id, "secret").unwrap();
        assert!(result.is_none());
        let index = store.get_index(user_b, agent_id).unwrap();
        assert!(index.is_empty());
    }

    #[test]
    fn test_different_agents_isolated() {
        let store = setup();
        let user_id = UserId::new();
        let agent_a = AgentId::new();
        let agent_b = AgentId::new();
        let t = make_topic(user_id, agent_a, "prefs", "A's prefs", "Use formal tone.");
        store.upsert_topic(&t).unwrap();

        // Same user, different agent — should not see agent_a's topics
        let result = store.get_topic(user_id, agent_b, "prefs").unwrap();
        assert!(result.is_none());
        let index = store.get_index(user_id, agent_b).unwrap();
        assert!(index.is_empty());
    }

    #[test]
    fn test_same_topic_key_different_agents() {
        let store = setup();
        let user_id = UserId::new();
        let agent_a = AgentId::new();
        let agent_b = AgentId::new();
        let ta = make_topic(user_id, agent_a, "prefs", "Agent A prefs", "Formal tone.");
        let tb = make_topic(user_id, agent_b, "prefs", "Agent B prefs", "Casual tone.");
        store.upsert_topic(&ta).unwrap();
        store.upsert_topic(&tb).unwrap();

        let loaded_a = store.get_topic(user_id, agent_a, "prefs").unwrap().unwrap();
        let loaded_b = store.get_topic(user_id, agent_b, "prefs").unwrap().unwrap();
        assert_eq!(loaded_a.content, "Formal tone.");
        assert_eq!(loaded_b.content, "Casual tone.");
    }

    #[test]
    fn test_delete_user_agent_memory() {
        let store = setup();
        let user_id = UserId::new();
        let agent_id = AgentId::new();
        let t1 = make_topic(user_id, agent_id, "t1", "s1", "c1");
        let t2 = make_topic(user_id, agent_id, "t2", "s2", "c2");
        store.upsert_topic(&t1).unwrap();
        store.upsert_topic(&t2).unwrap();

        store.delete_user_agent_memory(user_id, agent_id).unwrap();
        let index = store.get_index(user_id, agent_id).unwrap();
        assert!(index.is_empty());
    }

    #[test]
    fn test_delete_all_user_memory_across_agents() {
        let store = setup();
        let user_id = UserId::new();
        let agent_a = AgentId::new();
        let agent_b = AgentId::new();
        store
            .upsert_topic(&make_topic(user_id, agent_a, "t1", "s", "c"))
            .unwrap();
        store
            .upsert_topic(&make_topic(user_id, agent_b, "t1", "s", "c"))
            .unwrap();

        store.delete_all_user_memory(user_id).unwrap();
        assert!(store.get_index(user_id, agent_a).unwrap().is_empty());
        assert!(store.get_index(user_id, agent_b).unwrap().is_empty());
    }
}
