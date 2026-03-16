//! MongoDB session management — load/save conversation history.

use bson::doc;
use chrono::Utc;
use futures::TryStreamExt;
use mongodb::Collection;
use openfang_types::agent::{AgentId, SessionId};
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::message::Message;
use std::path::Path;

use crate::session::{CanonicalSession, Session};

/// Default number of recent messages to include from canonical session.
const DEFAULT_CANONICAL_WINDOW: usize = 50;

/// Default compaction threshold: when message count exceeds this, compact older messages.
const DEFAULT_COMPACTION_THRESHOLD: usize = 100;

/// Session store backed by MongoDB.
#[derive(Clone)]
pub struct MongoSessionStore {
    sessions: Collection<bson::Document>,
    canonical: Collection<bson::Document>,
}

impl MongoSessionStore {
    pub fn new(db: mongodb::Database) -> Self {
        Self {
            sessions: db.collection("sessions"),
            canonical: db.collection("canonical_sessions"),
        }
    }

    /// Load a session from the database.
    pub async fn get_session(&self, session_id: SessionId) -> OpenFangResult<Option<Session>> {
        let doc = self
            .sessions
            .find_one(doc! { "_id": session_id.0.to_string() })
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        match doc {
            Some(d) => {
                let agent_str = d.get_str("agent_id").unwrap_or_default();
                let agent_id = uuid::Uuid::parse_str(agent_str)
                    .map(AgentId)
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;
                let messages_bytes = d
                    .get_binary_generic("messages")
                    .map_err(|_| OpenFangError::Memory("Missing messages field".into()))?;
                let messages: Vec<Message> = rmp_serde::from_slice(messages_bytes)
                    .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
                let tokens = d.get_i64("context_window_tokens").unwrap_or(0) as u64;
                let label = d.get_str("label").ok().map(|s| s.to_string());

                Ok(Some(Session {
                    id: session_id,
                    agent_id,
                    messages,
                    context_window_tokens: tokens,
                    label,
                }))
            }
            None => Ok(None),
        }
    }

    /// Save a session to the database.
    pub async fn save_session(&self, session: &Session) -> OpenFangResult<()> {
        let messages_blob = rmp_serde::to_vec_named(&session.messages)
            .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
        let now = bson::DateTime::from_chrono(Utc::now());

        let filter = doc! { "_id": session.id.0.to_string() };
        let update = doc! {
            "$set": {
                "agent_id": session.agent_id.0.to_string(),
                "messages": bson::Binary { subtype: bson::spec::BinarySubtype::Generic, bytes: messages_blob },
                "context_window_tokens": session.context_window_tokens as i64,
                "label": session.label.as_deref(),
                "updated_at": now,
            },
            "$setOnInsert": {
                "created_at": now,
            },
        };
        self.sessions
            .update_one(filter, update)
            .upsert(true)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Delete a session from the database.
    pub async fn delete_session(&self, session_id: SessionId) -> OpenFangResult<()> {
        self.sessions
            .delete_one(doc! { "_id": session_id.0.to_string() })
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Delete all sessions belonging to an agent.
    pub async fn delete_agent_sessions(&self, agent_id: AgentId) -> OpenFangResult<()> {
        self.sessions
            .delete_many(doc! { "agent_id": agent_id.0.to_string() })
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Delete the canonical (cross-channel) session for an agent.
    pub async fn delete_canonical_session(&self, agent_id: AgentId) -> OpenFangResult<()> {
        self.canonical
            .delete_one(doc! { "_id": agent_id.0.to_string() })
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// List all sessions with metadata.
    pub async fn list_sessions(&self) -> OpenFangResult<Vec<serde_json::Value>> {
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "created_at": -1 })
            .build();
        let mut cursor = self
            .sessions
            .find(doc! {})
            .with_options(opts)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut sessions = Vec::new();
        while let Some(d) = cursor
            .try_next()
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?
        {
            let session_id = d.get_str("_id").unwrap_or_default().to_string();
            let agent_id = d.get_str("agent_id").unwrap_or_default().to_string();
            let label = d.get_str("label").ok().map(|s| s.to_string());
            let created_at = d
                .get_datetime("created_at")
                .ok()
                .map(|dt| dt.to_chrono().to_rfc3339())
                .unwrap_or_default();
            let msg_count = d
                .get_binary_generic("messages")
                .ok()
                .and_then(|b| rmp_serde::from_slice::<Vec<Message>>(b).ok())
                .map(|m| m.len())
                .unwrap_or(0);

            sessions.push(serde_json::json!({
                "session_id": session_id,
                "agent_id": agent_id,
                "message_count": msg_count,
                "created_at": created_at,
                "label": label,
            }));
        }
        Ok(sessions)
    }

    /// Create a new empty session for an agent.
    pub async fn create_session(&self, agent_id: AgentId) -> OpenFangResult<Session> {
        let session = Session {
            id: SessionId::new(),
            agent_id,
            messages: Vec::new(),
            context_window_tokens: 0,
            label: None,
        };
        self.save_session(&session).await?;
        Ok(session)
    }

    /// Set the label on an existing session.
    pub async fn set_session_label(
        &self,
        session_id: SessionId,
        label: Option<&str>,
    ) -> OpenFangResult<()> {
        let now = bson::DateTime::from_chrono(Utc::now());
        self.sessions
            .update_one(
                doc! { "_id": session_id.0.to_string() },
                doc! { "$set": { "label": label, "updated_at": now } },
            )
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Find a session by label for a given agent.
    pub async fn find_session_by_label(
        &self,
        agent_id: AgentId,
        label: &str,
    ) -> OpenFangResult<Option<Session>> {
        let doc = self
            .sessions
            .find_one(doc! { "agent_id": agent_id.0.to_string(), "label": label })
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        match doc {
            Some(d) => {
                let id_str = d.get_str("_id").unwrap_or_default();
                let session_id = uuid::Uuid::parse_str(id_str)
                    .map(SessionId)
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;
                let messages_bytes = d
                    .get_binary_generic("messages")
                    .map_err(|_| OpenFangError::Memory("Missing messages field".into()))?;
                let messages: Vec<Message> = rmp_serde::from_slice(messages_bytes)
                    .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
                let tokens = d.get_i64("context_window_tokens").unwrap_or(0) as u64;
                let lbl = d.get_str("label").ok().map(|s| s.to_string());

                Ok(Some(Session {
                    id: session_id,
                    agent_id,
                    messages,
                    context_window_tokens: tokens,
                    label: lbl,
                }))
            }
            None => Ok(None),
        }
    }

    /// List all sessions for a specific agent.
    pub async fn list_agent_sessions(
        &self,
        agent_id: AgentId,
    ) -> OpenFangResult<Vec<serde_json::Value>> {
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "created_at": -1 })
            .build();
        let mut cursor = self
            .sessions
            .find(doc! { "agent_id": agent_id.0.to_string() })
            .with_options(opts)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut sessions = Vec::new();
        while let Some(d) = cursor
            .try_next()
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?
        {
            let session_id = d.get_str("_id").unwrap_or_default().to_string();
            let label = d.get_str("label").ok().map(|s| s.to_string());
            let created_at = d
                .get_datetime("created_at")
                .ok()
                .map(|dt| dt.to_chrono().to_rfc3339())
                .unwrap_or_default();
            let msg_count = d
                .get_binary_generic("messages")
                .ok()
                .and_then(|b| rmp_serde::from_slice::<Vec<Message>>(b).ok())
                .map(|m| m.len())
                .unwrap_or(0);

            sessions.push(serde_json::json!({
                "session_id": session_id,
                "message_count": msg_count,
                "created_at": created_at,
                "label": label,
            }));
        }
        Ok(sessions)
    }

    /// Create a new session with an optional label.
    pub async fn create_session_with_label(
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
        self.save_session(&session).await?;
        Ok(session)
    }

    /// Store an LLM-generated summary, replacing older messages with the summary
    /// and keeping only the specified recent messages.
    pub async fn store_llm_summary(
        &self,
        agent_id: AgentId,
        summary: &str,
        kept_messages: Vec<Message>,
    ) -> OpenFangResult<()> {
        let mut canonical = self.load_canonical(agent_id).await?;
        canonical.compacted_summary = Some(summary.to_string());
        canonical.messages = kept_messages;
        canonical.compaction_cursor = 0;
        canonical.updated_at = Utc::now().to_rfc3339();
        self.save_canonical(&canonical).await
    }

    /// Load the canonical session for an agent, creating one if it doesn't exist.
    pub async fn load_canonical(&self, agent_id: AgentId) -> OpenFangResult<CanonicalSession> {
        let doc = self
            .canonical
            .find_one(doc! { "_id": agent_id.0.to_string() })
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        match doc {
            Some(d) => {
                let messages_bytes = d
                    .get_binary_generic("messages")
                    .map_err(|_| OpenFangError::Memory("Missing messages field".into()))?;
                let messages: Vec<Message> = rmp_serde::from_slice(messages_bytes)
                    .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
                let cursor = d.get_i64("compaction_cursor").unwrap_or(0) as usize;
                let summary = d.get_str("compacted_summary").ok().map(|s| s.to_string());
                let updated_at = d
                    .get_datetime("updated_at")
                    .ok()
                    .map(|dt| dt.to_chrono().to_rfc3339())
                    .unwrap_or_else(|| Utc::now().to_rfc3339());

                Ok(CanonicalSession {
                    agent_id,
                    messages,
                    compaction_cursor: cursor,
                    compacted_summary: summary,
                    updated_at,
                })
            }
            None => {
                let now = Utc::now().to_rfc3339();
                Ok(CanonicalSession {
                    agent_id,
                    messages: Vec::new(),
                    compaction_cursor: 0,
                    compacted_summary: None,
                    updated_at: now,
                })
            }
        }
    }

    /// Append new messages to the canonical session and compact if over threshold.
    pub async fn append_canonical(
        &self,
        agent_id: AgentId,
        new_messages: &[Message],
        compaction_threshold: Option<usize>,
    ) -> OpenFangResult<CanonicalSession> {
        let mut canonical = self.load_canonical(agent_id).await?;
        canonical.messages.extend(new_messages.iter().cloned());

        let threshold = compaction_threshold.unwrap_or(DEFAULT_COMPACTION_THRESHOLD);

        // Compact if over threshold
        if canonical.messages.len() > threshold {
            let keep_count = DEFAULT_CANONICAL_WINDOW;
            let to_compact = canonical.messages.len().saturating_sub(keep_count);
            if to_compact > canonical.compaction_cursor {
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
                        let truncated = if text.len() > 200 {
                            format!("{}...", openfang_types::truncate_str(&text, 200))
                        } else {
                            text
                        };
                        summary_parts.push(format!("{role}: {truncated}"));
                    }
                }
                let mut full_summary = summary_parts.join("\n");
                if full_summary.len() > 4000 {
                    let start = full_summary.len() - 4000;
                    let safe_start = (start..full_summary.len())
                        .find(|&i| full_summary.is_char_boundary(i))
                        .unwrap_or(full_summary.len());
                    full_summary = full_summary[safe_start..].to_string();
                }
                canonical.compacted_summary = Some(full_summary);
                canonical.compaction_cursor = to_compact;
                canonical.messages = canonical.messages.split_off(to_compact);
                canonical.compaction_cursor = 0;
            }
        }

        canonical.updated_at = Utc::now().to_rfc3339();
        self.save_canonical(&canonical).await?;
        Ok(canonical)
    }

    /// Get recent messages from canonical session for context injection.
    pub async fn canonical_context(
        &self,
        agent_id: AgentId,
        window_size: Option<usize>,
    ) -> OpenFangResult<(Option<String>, Vec<Message>)> {
        let canonical = self.load_canonical(agent_id).await?;
        let window = window_size.unwrap_or(DEFAULT_CANONICAL_WINDOW);
        let start = canonical.messages.len().saturating_sub(window);
        let recent = canonical.messages[start..].to_vec();
        Ok((canonical.compacted_summary.clone(), recent))
    }

    /// Persist a canonical session to MongoDB.
    async fn save_canonical(&self, canonical: &CanonicalSession) -> OpenFangResult<()> {
        let messages_blob = rmp_serde::to_vec(&canonical.messages)
            .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
        let now = bson::DateTime::from_chrono(Utc::now());

        let filter = doc! { "_id": canonical.agent_id.0.to_string() };
        let update = doc! {
            "$set": {
                "messages": bson::Binary { subtype: bson::spec::BinarySubtype::Generic, bytes: messages_blob },
                "compaction_cursor": canonical.compaction_cursor as i64,
                "compacted_summary": &canonical.compacted_summary,
                "updated_at": now,
            },
        };
        self.canonical
            .update_one(filter, update)
            .upsert(true)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Write a human-readable JSONL mirror of a session to disk.
    /// (Same as SQLite version — file I/O only, no DB involved.)
    pub fn write_jsonl_mirror(
        &self,
        session: &Session,
        sessions_dir: &Path,
    ) -> Result<(), std::io::Error> {
        // Delegate to the shared implementation
        crate::session::write_jsonl_mirror_impl(session, sessions_dir)
    }
}
