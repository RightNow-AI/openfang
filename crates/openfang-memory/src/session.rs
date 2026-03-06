//! Session management — load/save conversation history.

use chrono::Utc;
use openfang_types::agent::{AgentId, SessionId};
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::message::{ContentBlock, Message, MessageContent, Role};
use rusqlite::Connection;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// A conversation session with message history.
#[derive(Debug, Clone)]
pub struct Session {
    /// Session ID.
    pub id: SessionId,
    /// Owning agent ID.
    pub agent_id: AgentId,
    /// Conversation messages.
    pub messages: Vec<Message>,
    /// Estimated token count for the context window.
    pub context_window_tokens: u64,
    /// Optional human-readable session label.
    pub label: Option<String>,
}

/// Session store backed by SQLite.
#[derive(Clone)]
pub struct SessionStore {
    conn: Arc<Mutex<Connection>>,
}

impl SessionStore {
    /// Create a new session store wrapping the given connection.
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    /// Load a session from the database.
    pub fn get_session(&self, session_id: SessionId) -> OpenFangResult<Option<Session>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let mut stmt = conn
            .prepare("SELECT agent_id, messages, context_window_tokens, label FROM sessions WHERE id = ?1")
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let result = stmt.query_row(rusqlite::params![session_id.0.to_string()], |row| {
            let agent_str: String = row.get(0)?;
            let messages_blob: Vec<u8> = row.get(1)?;
            let tokens: i64 = row.get(2)?;
            let label: Option<String> = row.get(3).unwrap_or(None);
            Ok((agent_str, messages_blob, tokens, label))
        });

        match result {
            Ok((agent_str, messages_blob, tokens, label)) => {
                let agent_id = uuid::Uuid::parse_str(&agent_str)
                    .map(AgentId)
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;
                let messages: Vec<Message> = rmp_serde::from_slice(&messages_blob)
                    .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
                Ok(Some(Session {
                    id: session_id,
                    agent_id,
                    messages,
                    context_window_tokens: tokens as u64,
                    label,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(OpenFangError::Memory(e.to_string())),
        }
    }

    /// Save a session to the database.
    pub fn save_session(&self, session: &Session) -> OpenFangResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let messages_blob = rmp_serde::to_vec_named(&session.messages)
            .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO sessions (id, agent_id, messages, context_window_tokens, label, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(id) DO UPDATE SET messages = ?3, context_window_tokens = ?4, label = ?5, updated_at = ?6",
            rusqlite::params![
                session.id.0.to_string(),
                session.agent_id.0.to_string(),
                messages_blob,
                session.context_window_tokens as i64,
                session.label.as_deref(),
                now,
            ],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Delete a session from the database.
    pub fn delete_session(&self, session_id: SessionId) -> OpenFangResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        conn.execute(
            "DELETE FROM sessions WHERE id = ?1",
            rusqlite::params![session_id.0.to_string()],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Delete all sessions belonging to an agent.
    pub fn delete_agent_sessions(&self, agent_id: AgentId) -> OpenFangResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        conn.execute(
            "DELETE FROM sessions WHERE agent_id = ?1",
            rusqlite::params![agent_id.0.to_string()],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Delete the canonical (cross-channel) session for an agent.
    pub fn delete_canonical_session(&self, agent_id: AgentId) -> OpenFangResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        conn.execute(
            "DELETE FROM canonical_sessions WHERE agent_id = ?1",
            rusqlite::params![agent_id.0.to_string()],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// List all sessions with metadata (session_id, agent_id, message_count, created_at).
    pub fn list_sessions(&self) -> OpenFangResult<Vec<serde_json::Value>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, agent_id, messages, created_at, label FROM sessions ORDER BY created_at DESC",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| {
                let session_id: String = row.get(0)?;
                let agent_id: String = row.get(1)?;
                let messages_blob: Vec<u8> = row.get(2)?;
                let created_at: String = row.get(3)?;
                let label: Option<String> = row.get(4)?;
                // Deserialize just to count messages
                let msg_count = rmp_serde::from_slice::<Vec<Message>>(&messages_blob)
                    .map(|m| m.len())
                    .unwrap_or(0);
                Ok(serde_json::json!({
                    "session_id": session_id,
                    "agent_id": agent_id,
                    "message_count": msg_count,
                    "created_at": created_at,
                    "label": label,
                }))
            })
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(row.map_err(|e| OpenFangError::Memory(e.to_string()))?);
        }
        Ok(sessions)
    }

    /// Create a new empty session for an agent.
    pub fn create_session(&self, agent_id: AgentId) -> OpenFangResult<Session> {
        let session = Session {
            id: SessionId::new(),
            agent_id,
            messages: Vec::new(),
            context_window_tokens: 0,
            label: None,
        };
        self.save_session(&session)?;
        Ok(session)
    }

    /// Set the label on an existing session.
    pub fn set_session_label(
        &self,
        session_id: SessionId,
        label: Option<&str>,
    ) -> OpenFangResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        conn.execute(
            "UPDATE sessions SET label = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![label, Utc::now().to_rfc3339(), session_id.0.to_string()],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Find a session by label for a given agent.
    pub fn find_session_by_label(
        &self,
        agent_id: AgentId,
        label: &str,
    ) -> OpenFangResult<Option<Session>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, messages, context_window_tokens, label FROM sessions \
                 WHERE agent_id = ?1 AND label = ?2 LIMIT 1",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let result = stmt.query_row(rusqlite::params![agent_id.0.to_string(), label], |row| {
            let id_str: String = row.get(0)?;
            let messages_blob: Vec<u8> = row.get(1)?;
            let tokens: i64 = row.get(2)?;
            let lbl: Option<String> = row.get(3).unwrap_or(None);
            Ok((id_str, messages_blob, tokens, lbl))
        });

        match result {
            Ok((id_str, messages_blob, tokens, lbl)) => {
                let session_id = uuid::Uuid::parse_str(&id_str)
                    .map(SessionId)
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;
                let messages: Vec<Message> = rmp_serde::from_slice(&messages_blob)
                    .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
                Ok(Some(Session {
                    id: session_id,
                    agent_id,
                    messages,
                    context_window_tokens: tokens as u64,
                    label: lbl,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(OpenFangError::Memory(e.to_string())),
        }
    }
}

impl SessionStore {
    /// List all sessions for a specific agent.
    pub fn list_agent_sessions(&self, agent_id: AgentId) -> OpenFangResult<Vec<serde_json::Value>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, messages, created_at, label FROM sessions WHERE agent_id = ?1 ORDER BY created_at DESC",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let rows = stmt
            .query_map(rusqlite::params![agent_id.0.to_string()], |row| {
                let session_id: String = row.get(0)?;
                let messages_blob: Vec<u8> = row.get(1)?;
                let created_at: String = row.get(2)?;
                let label: Option<String> = row.get(3)?;
                let msg_count = rmp_serde::from_slice::<Vec<Message>>(&messages_blob)
                    .map(|m| m.len())
                    .unwrap_or(0);
                Ok(serde_json::json!({
                    "session_id": session_id,
                    "message_count": msg_count,
                    "created_at": created_at,
                    "label": label,
                }))
            })
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(row.map_err(|e| OpenFangError::Memory(e.to_string()))?);
        }
        Ok(sessions)
    }

    /// Create a new session with an optional label.
    pub fn create_session_with_label(
        &self,
        agent_id: AgentId,
        label: Option<&str>,
    ) -> OpenFangResult<Session> {
        let session = Session {
            id: SessionId::new(),
            agent_id,
            messages: Vec::new(),
            context_window_tokens: 0,
            label: label.map(|s| s.to_string()),
        };
        self.save_session(&session)?;
        Ok(session)
    }

    /// Store an LLM-generated summary, replacing older messages with the summary
    /// and keeping only the specified recent messages.
    ///
    /// This is used by the LLM-based compactor to replace text-truncation compaction
    /// with an intelligent, LLM-generated summary of older conversation history.
    pub fn store_llm_summary(
        &self,
        agent_id: AgentId,
        summary: &str,
        kept_messages: Vec<Message>,
    ) -> OpenFangResult<()> {
        let mut canonical = self.load_canonical(agent_id)?;
        canonical.compacted_summary = Some(summary.to_string());
        canonical.messages = kept_messages;
        canonical.compaction_cursor = 0;
        canonical.updated_at = Utc::now().to_rfc3339();
        self.save_canonical(&canonical)
    }
}

/// Default number of recent messages to include from canonical session.
const DEFAULT_CANONICAL_WINDOW: usize = 50;

/// Default compaction threshold: when message count exceeds this, compact older messages.
const DEFAULT_COMPACTION_THRESHOLD: usize = 100;

/// A canonical session stores persistent cross-channel context for an agent.
///
/// Unlike regular sessions (one per channel interaction), there is one canonical
/// session per agent. All channels contribute to it, so what a user tells an agent
/// on Telegram is remembered on Discord.
#[derive(Debug, Clone)]
pub struct CanonicalSession {
    /// The agent this session belongs to.
    pub agent_id: AgentId,
    /// Full message history (post-compaction window).
    pub messages: Vec<Message>,
    /// Index marking how far compaction has processed.
    pub compaction_cursor: usize,
    /// Summary of compacted (older) messages.
    pub compacted_summary: Option<String>,
    /// Last update time.
    pub updated_at: String,
}

impl SessionStore {
    /// Load the canonical session for an agent, creating one if it doesn't exist.
    pub fn load_canonical(&self, agent_id: AgentId) -> OpenFangResult<CanonicalSession> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT messages, compaction_cursor, compacted_summary, updated_at \
                 FROM canonical_sessions WHERE agent_id = ?1",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let result = stmt.query_row(rusqlite::params![agent_id.0.to_string()], |row| {
            let messages_blob: Vec<u8> = row.get(0)?;
            let cursor: i64 = row.get(1)?;
            let summary: Option<String> = row.get(2)?;
            let updated_at: String = row.get(3)?;
            Ok((messages_blob, cursor, summary, updated_at))
        });

        match result {
            Ok((messages_blob, cursor, summary, updated_at)) => {
                let messages: Vec<Message> = rmp_serde::from_slice(&messages_blob)
                    .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
                Ok(CanonicalSession {
                    agent_id,
                    messages,
                    compaction_cursor: cursor as usize,
                    compacted_summary: summary,
                    updated_at,
                })
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                let now = Utc::now().to_rfc3339();
                Ok(CanonicalSession {
                    agent_id,
                    messages: Vec::new(),
                    compaction_cursor: 0,
                    compacted_summary: None,
                    updated_at: now,
                })
            }
            Err(e) => Err(OpenFangError::Memory(e.to_string())),
        }
    }

    /// Append new messages to the canonical session and compact if over threshold.
    ///
    /// Compaction summarizes old messages into a text summary and trims the
    /// message list. The `compaction_threshold` controls when this happens
    /// (default: 100 messages).
    pub fn append_canonical(
        &self,
        agent_id: AgentId,
        new_messages: &[Message],
        compaction_threshold: Option<usize>,
    ) -> OpenFangResult<CanonicalSession> {
        let mut canonical = self.load_canonical(agent_id)?;
        canonical.messages.extend(new_messages.iter().cloned());

        let threshold = compaction_threshold.unwrap_or(DEFAULT_COMPACTION_THRESHOLD);

        // Compact if over threshold
        if canonical.messages.len() > threshold {
            let keep_count = DEFAULT_CANONICAL_WINDOW;
            let to_compact = canonical.messages.len().saturating_sub(keep_count);
            if to_compact > canonical.compaction_cursor {
                // Build a summary from the messages being compacted
                let compacting = &canonical.messages[canonical.compaction_cursor..to_compact];
                let mut summary_parts: Vec<String> = Vec::new();
                if let Some(ref existing) = canonical.compacted_summary {
                    summary_parts.push(existing.clone());
                }
                for msg in compacting {
                    let role = match msg.role {
                        openfang_types::message::Role::User => "User",
                        openfang_types::message::Role::Assistant => "Assistant",
                        openfang_types::message::Role::System => "System",
                    };
                    let text = msg.content.text_content();
                    if !text.is_empty() {
                        // Truncate individual messages in summary to keep it compact (UTF-8 safe)
                        let truncated = if text.len() > 200 {
                            format!("{}...", openfang_types::truncate_str(&text, 200))
                        } else {
                            text
                        };
                        summary_parts.push(format!("{role}: {truncated}"));
                    }
                }
                // Keep summary under ~4000 chars (UTF-8 safe)
                let mut full_summary = summary_parts.join("\n");
                if full_summary.len() > 4000 {
                    let start = full_summary.len() - 4000;
                    // Find the next char boundary at or after `start`
                    let safe_start = (start..full_summary.len())
                        .find(|&i| full_summary.is_char_boundary(i))
                        .unwrap_or(full_summary.len());
                    full_summary = full_summary[safe_start..].to_string();
                }
                canonical.compacted_summary = Some(full_summary);
                canonical.compaction_cursor = to_compact;
                // Trim messages: keep only the recent window
                canonical.messages = canonical.messages.split_off(to_compact);
                canonical.compaction_cursor = 0; // reset cursor since we trimmed
            }
        }

        canonical.updated_at = Utc::now().to_rfc3339();
        self.save_canonical(&canonical)?;
        Ok(canonical)
    }

    /// Get recent messages from canonical session for context injection.
    ///
    /// Returns up to `window_size` recent messages (default 50), plus
    /// the compacted summary if available.
    pub fn canonical_context(
        &self,
        agent_id: AgentId,
        window_size: Option<usize>,
    ) -> OpenFangResult<(Option<String>, Vec<Message>)> {
        let canonical = self.load_canonical(agent_id)?;
        let window = window_size.unwrap_or(DEFAULT_CANONICAL_WINDOW);
        let start = canonical.messages.len().saturating_sub(window);
        let recent = canonical.messages[start..].to_vec();
        Ok((canonical.compacted_summary.clone(), recent))
    }

    /// Persist a canonical session to SQLite.
    fn save_canonical(&self, canonical: &CanonicalSession) -> OpenFangResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let messages_blob = rmp_serde::to_vec(&canonical.messages)
            .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
        conn.execute(
            "INSERT INTO canonical_sessions (agent_id, messages, compaction_cursor, compacted_summary, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(agent_id) DO UPDATE SET messages = ?2, compaction_cursor = ?3, compacted_summary = ?4, updated_at = ?5",
            rusqlite::params![
                canonical.agent_id.0.to_string(),
                messages_blob,
                canonical.compaction_cursor as i64,
                canonical.compacted_summary,
                canonical.updated_at,
            ],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }
}

/// A single JSONL line in the session mirror file.
#[derive(serde::Serialize)]
struct JsonlLine {
    timestamp: String,
    role: String,
    content: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_use: Option<serde_json::Value>,
}

impl SessionStore {
    /// Write a human-readable JSONL mirror of a session to disk.
    ///
    /// Best-effort: errors are returned but should be logged and never
    /// affect the primary SQLite store.
    pub fn write_jsonl_mirror(
        &self,
        session: &Session,
        sessions_dir: &Path,
    ) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(sessions_dir)?;
        let path = sessions_dir.join(format!("{}.jsonl", session.id.0));
        let mut file = std::fs::File::create(&path)?;
        let now = Utc::now().to_rfc3339();

        for msg in &session.messages {
            let role_str = match msg.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::System => "system",
            };

            let mut text_parts: Vec<String> = Vec::new();
            let mut tool_parts: Vec<serde_json::Value> = Vec::new();

            match &msg.content {
                MessageContent::Text(t) => {
                    text_parts.push(t.clone());
                }
                MessageContent::Blocks(blocks) => {
                    for block in blocks {
                        match block {
                            ContentBlock::Text { text } => {
                                text_parts.push(text.clone());
                            }
                            ContentBlock::ToolUse { id, name, input } => {
                                tool_parts.push(serde_json::json!({
                                    "type": "tool_use",
                                    "id": id,
                                    "name": name,
                                    "input": input,
                                }));
                            }
                            ContentBlock::ToolResult {
                                tool_use_id,
                                tool_name: _,
                                content,
                                is_error,
                            } => {
                                tool_parts.push(serde_json::json!({
                                    "type": "tool_result",
                                    "tool_use_id": tool_use_id,
                                    "content": content,
                                    "is_error": is_error,
                                }));
                            }
                            ContentBlock::Image { media_type, .. } => {
                                text_parts.push(format!("[image: {media_type}]"));
                            }
                            ContentBlock::Thinking { thinking } => {
                                text_parts.push(format!(
                                    "[thinking: {}]",
                                    &thinking[..thinking.len().min(200)]
                                ));
                            }
                            ContentBlock::Unknown => {}
                        }
                    }
                }
            }

            let line = JsonlLine {
                timestamp: now.clone(),
                role: role_str.to_string(),
                content: serde_json::Value::String(text_parts.join("\n")),
                tool_use: if tool_parts.is_empty() {
                    None
                } else {
                    Some(serde_json::Value::Array(tool_parts))
                },
            };

            serde_json::to_writer(&mut file, &line).map_err(std::io::Error::other)?;
            file.write_all(b"\n")?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::run_migrations;

    fn setup() -> SessionStore {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        SessionStore::new(Arc::new(Mutex::new(conn)))
    }

    #[test]
    fn test_create_and_load_session() {
        let store = setup();
        let agent_id = AgentId::new();
        let session = store.create_session(agent_id).unwrap();

        let loaded = store.get_session(session.id).unwrap().unwrap();
        assert_eq!(loaded.agent_id, agent_id);
        assert!(loaded.messages.is_empty());
    }

    #[test]
    fn test_save_and_load_with_messages() {
        let store = setup();
        let agent_id = AgentId::new();
        let mut session = store.create_session(agent_id).unwrap();
        session.messages.push(Message::user("Hello"));
        session.messages.push(Message::assistant("Hi there!"));
        store.save_session(&session).unwrap();

        let loaded = store.get_session(session.id).unwrap().unwrap();
        assert_eq!(loaded.messages.len(), 2);
    }

    #[test]
    fn test_get_missing_session() {
        let store = setup();
        let result = store.get_session(SessionId::new()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_delete_session() {
        let store = setup();
        let agent_id = AgentId::new();
        let session = store.create_session(agent_id).unwrap();
        let sid = session.id;
        assert!(store.get_session(sid).unwrap().is_some());
        store.delete_session(sid).unwrap();
        assert!(store.get_session(sid).unwrap().is_none());
    }

    #[test]
    fn test_delete_agent_sessions() {
        let store = setup();
        let agent_id = AgentId::new();
        let s1 = store.create_session(agent_id).unwrap();
        let s2 = store.create_session(agent_id).unwrap();
        assert!(store.get_session(s1.id).unwrap().is_some());
        assert!(store.get_session(s2.id).unwrap().is_some());
        store.delete_agent_sessions(agent_id).unwrap();
        assert!(store.get_session(s1.id).unwrap().is_none());
        assert!(store.get_session(s2.id).unwrap().is_none());
    }

    #[test]
    fn test_canonical_load_creates_empty() {
        let store = setup();
        let agent_id = AgentId::new();
        let canonical = store.load_canonical(agent_id).unwrap();
        assert_eq!(canonical.agent_id, agent_id);
        assert!(canonical.messages.is_empty());
        assert!(canonical.compacted_summary.is_none());
        assert_eq!(canonical.compaction_cursor, 0);
    }

    #[test]
    fn test_canonical_append_and_load() {
        let store = setup();
        let agent_id = AgentId::new();

        // Append from "Telegram"
        let msgs1 = vec![
            Message::user("Hello from Telegram"),
            Message::assistant("Hi! I'm your agent."),
        ];
        store.append_canonical(agent_id, &msgs1, None).unwrap();

        // Append from "Discord"
        let msgs2 = vec![
            Message::user("Now I'm on Discord"),
            Message::assistant("I remember you from Telegram!"),
        ];
        let canonical = store.append_canonical(agent_id, &msgs2, None).unwrap();

        // Should have all 4 messages
        assert_eq!(canonical.messages.len(), 4);
    }

    #[test]
    fn test_canonical_context_window() {
        let store = setup();
        let agent_id = AgentId::new();

        // Add 10 messages
        let msgs: Vec<Message> = (0..10)
            .map(|i| Message::user(format!("Message {i}")))
            .collect();
        store.append_canonical(agent_id, &msgs, None).unwrap();

        // Request window of 3
        let (summary, recent) = store.canonical_context(agent_id, Some(3)).unwrap();
        assert_eq!(recent.len(), 3);
        assert!(summary.is_none()); // No compaction yet
    }

    #[test]
    fn test_canonical_compaction() {
        let store = setup();
        let agent_id = AgentId::new();

        // Add 120 messages (over the default 100 threshold)
        let msgs: Vec<Message> = (0..120)
            .map(|i| Message::user(format!("Message number {i} with some content")))
            .collect();
        let canonical = store.append_canonical(agent_id, &msgs, Some(100)).unwrap();

        // After compaction: should keep DEFAULT_CANONICAL_WINDOW (50) messages
        assert!(canonical.messages.len() <= 60); // some tolerance
        assert!(canonical.compacted_summary.is_some());
    }

    #[test]
    fn test_canonical_cross_channel_roundtrip() {
        let store = setup();
        let agent_id = AgentId::new();

        // Channel 1: user tells agent their name
        store
            .append_canonical(
                agent_id,
                &[
                    Message::user("My name is Jaber"),
                    Message::assistant("Nice to meet you, Jaber!"),
                ],
                None,
            )
            .unwrap();

        // Channel 2: different channel queries same agent
        let (summary, recent) = store.canonical_context(agent_id, None).unwrap();
        // The agent should have context about "Jaber" from the previous channel
        let all_text: String = recent.iter().map(|m| m.content.text_content()).collect();
        assert!(all_text.contains("Jaber"));
        assert!(summary.is_none()); // Only 2 messages, no compaction
    }

    #[test]
    fn test_jsonl_mirror_write() {
        let store = setup();
        let agent_id = AgentId::new();
        let mut session = store.create_session(agent_id).unwrap();
        session
            .messages
            .push(openfang_types::message::Message::user("Hello"));
        session
            .messages
            .push(openfang_types::message::Message::assistant("Hi there!"));
        store.save_session(&session).unwrap();

        let dir = tempfile::TempDir::new().unwrap();
        let sessions_dir = dir.path().join("sessions");
        store.write_jsonl_mirror(&session, &sessions_dir).unwrap();

        let jsonl_path = sessions_dir.join(format!("{}.jsonl", session.id.0));
        assert!(jsonl_path.exists());

        let content = std::fs::read_to_string(&jsonl_path).unwrap();
        let lines: Vec<&str> = content.trim().split('\n').collect();
        assert_eq!(lines.len(), 2);

        // Verify first line is user message
        let line1: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(line1["role"], "user");
        assert_eq!(line1["content"], "Hello");

        // Verify second line is assistant message
        let line2: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(line2["role"], "assistant");
        assert_eq!(line2["content"], "Hi there!");
        assert!(line2.get("tool_use").is_none());
    }
}
