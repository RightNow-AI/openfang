//! Backend traits for session and usage stores.
//!
//! These traits reference types defined in this crate (`Session`, `UsageRecord`, etc.)
//! and therefore cannot live in `openfang-types`. The structured, semantic, and
//! knowledge backend traits are in `openfang_types::storage`.

use chrono::Utc;
use openfang_types::agent::{AgentId, SessionId};
use openfang_types::error::OpenFangResult;
use openfang_types::message::Message;

use crate::session::{CanonicalSession, Session};
use crate::usage::{DailyBreakdown, ModelUsage, UsageRecord, UsageSummary};

// Re-export the traits from openfang-types for convenience.
pub use openfang_types::storage::{KnowledgeBackend, SemanticBackend, StructuredBackend};

/// Backend for conversation session persistence.
pub trait SessionBackend: Send + Sync {
    /// Get a session by ID.
    fn get_session(&self, id: SessionId) -> OpenFangResult<Option<Session>>;
    /// Save (upsert) a session.
    fn save_session(&self, session: &Session) -> OpenFangResult<()>;
    /// Delete a session.
    fn delete_session(&self, id: SessionId) -> OpenFangResult<()>;
    /// Delete all sessions for an agent.
    fn delete_agent_sessions(&self, agent_id: AgentId) -> OpenFangResult<()>;
    /// Create a new empty session for an agent.
    fn create_session(&self, agent_id: AgentId) -> OpenFangResult<Session> {
        self.create_session_with_label(agent_id, None)
    }
    /// Create a new session with an optional label.
    fn create_session_with_label(
        &self,
        agent_id: AgentId,
        label: Option<&str>,
    ) -> OpenFangResult<Session> {
        let session = Session {
            id: SessionId::new(),
            agent_id,
            messages: vec![],
            context_window_tokens: 0,
            label: label.map(|s| s.to_string()),
        };
        self.save_session(&session)?;
        Ok(session)
    }
    /// List all sessions (metadata only).
    fn list_sessions(&self) -> OpenFangResult<Vec<serde_json::Value>>;
    /// List sessions for a specific agent.
    fn list_agent_sessions(&self, agent_id: AgentId) -> OpenFangResult<Vec<serde_json::Value>>;
    /// Set a human-readable label on a session.
    fn set_session_label(&self, id: SessionId, label: Option<&str>) -> OpenFangResult<()>;
    /// Find a session by label for a given agent.
    fn find_session_by_label(
        &self,
        agent_id: AgentId,
        label: &str,
    ) -> OpenFangResult<Option<Session>>;
    /// Delete the canonical session for an agent.
    fn delete_canonical_session(&self, agent_id: AgentId) -> OpenFangResult<()>;

    // -- Canonical session methods --

    /// Load the canonical (cross-channel) session, creating if absent.
    fn load_canonical(&self, agent_id: AgentId) -> OpenFangResult<CanonicalSession>;
    /// Persist a canonical session (insert or update).
    fn save_canonical(&self, canonical: &CanonicalSession) -> OpenFangResult<()>;
    /// Append messages to the canonical session, compacting if needed.
    fn append_canonical(
        &self,
        agent_id: AgentId,
        new_messages: &[Message],
        compaction_threshold: Option<usize>,
    ) -> OpenFangResult<CanonicalSession> {
        let mut canonical = self.load_canonical(agent_id)?;
        canonical.messages.extend_from_slice(new_messages);

        let threshold = compaction_threshold.unwrap_or(100);
        let keep_count = 50;

        if canonical.messages.len() > threshold {
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
                canonical.messages = canonical.messages.split_off(to_compact);
                canonical.compaction_cursor = 0;
            }
        }

        canonical.updated_at = Utc::now().to_rfc3339();
        self.save_canonical(&canonical)?;
        Ok(canonical)
    }
    /// Get the canonical context window (optional summary + recent messages).
    fn canonical_context(
        &self,
        agent_id: AgentId,
        window_size: Option<usize>,
    ) -> OpenFangResult<(Option<String>, Vec<Message>)> {
        let session = self.load_canonical(agent_id)?;
        let window = window_size.unwrap_or(50);
        let messages = if session.messages.len() > window {
            session.messages[session.messages.len() - window..].to_vec()
        } else {
            session.messages
        };
        Ok((session.compacted_summary, messages))
    }
    /// Store an LLM-generated summary for the canonical session.
    fn store_llm_summary(
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

/// Backend for LLM usage tracking and cost metering.
pub trait UsageBackend: Send + Sync {
    /// Record a usage event.
    fn record(&self, record: &UsageRecord) -> OpenFangResult<()>;
    /// Query total cost for an agent in the last hour.
    fn query_hourly(&self, agent_id: AgentId) -> OpenFangResult<f64>;
    /// Query total cost for an agent today.
    fn query_daily(&self, agent_id: AgentId) -> OpenFangResult<f64>;
    /// Query total cost for an agent this month.
    fn query_monthly(&self, agent_id: AgentId) -> OpenFangResult<f64>;
    /// Query total cost across all agents in the last hour.
    fn query_global_hourly(&self) -> OpenFangResult<f64>;
    /// Query total cost across all agents this month.
    fn query_global_monthly(&self) -> OpenFangResult<f64>;
    /// Get a summary of usage, optionally filtered by agent.
    fn query_summary(&self, agent_id: Option<AgentId>) -> OpenFangResult<UsageSummary>;
    /// Get usage breakdown by model.
    fn query_by_model(&self) -> OpenFangResult<Vec<ModelUsage>>;
    /// Get daily usage breakdown for the last N days.
    fn query_daily_breakdown(&self, days: u32) -> OpenFangResult<Vec<DailyBreakdown>>;
    /// Get the date of the first usage event.
    fn query_first_event_date(&self) -> OpenFangResult<Option<String>>;
    /// Get today's total cost.
    fn query_today_cost(&self) -> OpenFangResult<f64>;
    /// Delete usage events older than N days. Returns count deleted.
    fn cleanup_old(&self, days: u32) -> OpenFangResult<usize>;
}

/// Backend for paired device persistence.
pub trait PairedDevicesBackend: Send + Sync {
    fn load_paired_devices(&self) -> OpenFangResult<Vec<serde_json::Value>>;
    fn save_paired_device(
        &self,
        device_id: &str,
        display_name: &str,
        platform: &str,
        paired_at: &str,
        last_seen: &str,
        push_token: Option<&str>,
    ) -> OpenFangResult<()>;
    fn remove_paired_device(&self, device_id: &str) -> OpenFangResult<()>;
}

/// Backend for the shared task queue.
pub trait TaskQueueBackend: Send + Sync {
    fn task_post(&self, title: &str, description: &str, assigned_to: &str, created_by: &str) -> OpenFangResult<String>;
    fn task_claim(&self, agent_id: &str) -> OpenFangResult<Option<serde_json::Value>>;
    fn task_complete(&self, task_id: &str, result: &str) -> OpenFangResult<()>;
    fn task_list(&self, status: Option<&str>) -> OpenFangResult<Vec<serde_json::Value>>;
}

/// Backend for memory consolidation (confidence decay).
pub trait ConsolidationBackend: Send + Sync {
    fn consolidate(&self) -> OpenFangResult<openfang_types::memory::ConsolidationReport>;
}

/// Backend for audit log persistence.
pub trait AuditBackend: Send + Sync {
    fn append_entry(&self, agent_id: &str, action: &str, detail: &str, outcome: &str) -> OpenFangResult<()>;
    fn load_entries(&self, agent_id: Option<&str>, limit: usize) -> OpenFangResult<Vec<serde_json::Value>>;
}
