//! WorkItem store — persistence layer for the canonical unit-of-work system.
//!
//! Wraps the shared SQLite connection and provides typed CRUD + state-transition
//! methods for `WorkItem`, `WorkEvent`, and `ApprovalRecord`.

use chrono::Utc;
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::work_item::{
    ApprovalRecord, ApprovalStatus, WorkEvent, WorkItem, WorkItemFilter, WorkStatus,
};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/// SQLite-backed store for work items and their audit trail.
#[derive(Clone)]
pub struct WorkItemStore {
    conn: Arc<Mutex<Connection>>,
}

impl WorkItemStore {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    // -----------------------------------------------------------------------
    // Work items — write
    // -----------------------------------------------------------------------

    /// Persist a new work item.
    ///
    /// If `idempotency_key` is set and a row with the same key already exists,
    /// returns the existing item without creating a duplicate.
    pub fn create(&self, item: &WorkItem) -> OpenFangResult<WorkItem> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;

        // Idempotency check
        if let Some(ref key) = item.idempotency_key {
            let existing: Option<String> = conn
                .query_row(
                    "SELECT id FROM work_items WHERE idempotency_key = ?1",
                    rusqlite::params![key],
                    |row| row.get(0),
                )
                .ok();
            if let Some(existing_id) = existing {
                // Return the existing item
                drop(conn);
                return self.get_by_id(&existing_id)?.ok_or_else(|| {
                    OpenFangError::Internal("idempotency_key collision but item missing".into())
                });
            }
        }

        let tags_json = serde_json::to_string(&item.tags)
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let payload_json = serde_json::to_string(&item.payload)
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;

        conn.execute(
            "INSERT INTO work_items (
                id, title, description, work_type, source, status, approval_status,
                assigned_agent_id, assigned_agent_name, result, error, iterations, priority,
                scheduled_at, started_at, completed_at, deadline, requires_approval,
                approved_by, approved_at, approval_note, payload, tags, created_by,
                idempotency_key, created_at, updated_at, retry_count, max_retries, parent_id,
                run_id, workspace_id
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7,
                ?8, ?9, ?10, ?11, ?12, ?13,
                ?14, ?15, ?16, ?17, ?18,
                ?19, ?20, ?21, ?22, ?23, ?24,
                ?25, ?26, ?27, ?28, ?29, ?30,
                ?31, ?32
            )",
            rusqlite::params![
                item.id,
                item.title,
                item.description,
                item.work_type.as_str(),
                item.source.as_str(),
                item.status.as_str(),
                item.approval_status.as_str(),
                item.assigned_agent_id,
                item.assigned_agent_name,
                item.result,
                item.error,
                item.iterations as i64,
                item.priority as i64,
                item.scheduled_at.map(|t| t.to_rfc3339()),
                item.started_at.map(|t| t.to_rfc3339()),
                item.completed_at.map(|t| t.to_rfc3339()),
                item.deadline.map(|t| t.to_rfc3339()),
                item.requires_approval as i64,
                item.approved_by,
                item.approved_at.map(|t| t.to_rfc3339()),
                item.approval_note,
                payload_json,
                tags_json,
                item.created_by,
                item.idempotency_key,
                item.created_at.to_rfc3339(),
                item.updated_at.to_rfc3339(),
                item.retry_count as i64,
                item.max_retries as i64,
                item.parent_id,
                item.run_id,
                item.workspace_id,
            ],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        // Emit a "created" event for the audit trail
        let event_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO work_events (id, work_item_id, event_type, from_status, to_status, actor, detail, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                event_id,
                item.id,
                "created",
                Option::<String>::None,
                item.status.as_str(),
                item.created_by,
                Option::<String>::None,
                item.created_at.to_rfc3339(),
            ],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        drop(conn);
        self.get_by_id(&item.id)?
            .ok_or_else(|| OpenFangError::Internal("item missing after insert".into()))
    }

    /// Persist durable execution linkage for a work item.
    pub fn set_run_context(
        &self,
        id: &str,
        run_id: Option<&str>,
        workspace_id: Option<&str>,
    ) -> OpenFangResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;

        conn.execute(
            "UPDATE work_items
             SET run_id = ?2,
                 workspace_id = ?3,
                 updated_at = ?4
             WHERE id = ?1",
            rusqlite::params![id, run_id, workspace_id, Utc::now().to_rfc3339()],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        Ok(())
    }

    /// Transition a work item to a new status, updating related fields.
    ///
    /// Validates the transition is legal, then writes an event and updates
    /// the row. Returns the updated item.
    pub fn transition(
        &self,
        id: &str,
        new_status: WorkStatus,
        actor: Option<&str>,
        detail: Option<&str>,
        extra: Option<TransitionExtra>,
    ) -> OpenFangResult<WorkItem> {
        let item = self
            .get_by_id(id)?
            .ok_or_else(|| OpenFangError::Memory(format!("work item {id} not found")))?;

        if !item.status.can_transition_to(&new_status) {
            return Err(OpenFangError::InvalidInput(format!(
                "invalid transition: {:?} → {:?}",
                item.status, new_status
            )));
        }

        let from_status = item.status.as_str().to_string();
        let now = Utc::now().to_rfc3339();

        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;

        let started_at = if new_status == WorkStatus::Running && item.started_at.is_none() {
            Some(now.clone())
        } else {
            item.started_at.map(|t: chrono::DateTime<Utc>| t.to_rfc3339())
        };

        let completed_at = if matches!(
            new_status,
            WorkStatus::Completed | WorkStatus::Failed | WorkStatus::Cancelled | WorkStatus::Rejected
        ) {
            Some(now.clone())
        } else {
            item.completed_at.map(|t: chrono::DateTime<Utc>| t.to_rfc3339())
        };

        let (approval_status, approved_by, approved_at, approval_note) = if let Some(ref ex) = extra
        {
            (
                ex.approval_status
                    .as_ref()
                    .map(|s| s.as_str())
                    .unwrap_or(item.approval_status.as_str()),
                ex.approved_by.as_deref().or(item.approved_by.as_deref()),
                ex.approved_at
                    .map(|t| t.to_rfc3339())
                    .or(item.approved_at.map(|t: chrono::DateTime<Utc>| t.to_rfc3339())),
                ex.approval_note.as_deref().or(item.approval_note.as_deref()),
            )
        } else {
            (
                item.approval_status.as_str(),
                item.approved_by.as_deref(),
                item.approved_at.map(|t: chrono::DateTime<Utc>| t.to_rfc3339()),
                item.approval_note.as_deref(),
            )
        };

        let (result, error, iterations) = if let Some(ref ex) = extra {
            (
                ex.result.as_deref().or(item.result.as_deref()),
                ex.error.as_deref().or(item.error.as_deref()),
                ex.iterations.unwrap_or(item.iterations) as i64,
            )
        } else {
            (
                item.result.as_deref(),
                item.error.as_deref(),
                item.iterations as i64,
            )
        };

        // For retry: bump retry_count and clear error/result
        let (retry_count, clear_result, clear_error) = if new_status == WorkStatus::Pending
            && item.status == WorkStatus::Failed
        {
            (item.retry_count + 1, true, true)
        } else {
            (item.retry_count, false, false)
        };

        conn.execute(
            "UPDATE work_items SET
                status = ?1, approval_status = ?2,
                started_at = ?3, completed_at = ?4,
                approved_by = ?5, approved_at = ?6, approval_note = ?7,
                result = ?8, error = ?9, iterations = ?10,
                retry_count = ?11, updated_at = ?12
             WHERE id = ?13",
            rusqlite::params![
                new_status.as_str(),
                approval_status,
                started_at,
                completed_at,
                approved_by,
                approved_at,
                approval_note,
                if clear_result { None } else { result },
                if clear_error { None } else { error },
                iterations,
                retry_count as i64,
                now.clone(),
                id,
            ],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        // Append event with a semantic event type
        let event_id = uuid::Uuid::new_v4().to_string();
        let event_type = semantic_event_type(&item.status, &new_status);
        conn.execute(
            "INSERT INTO work_events (id, work_item_id, event_type, from_status, to_status, actor, detail, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                event_id,
                id,
                event_type,
                from_status,
                new_status.as_str(),
                actor,
                detail,
                now,
            ],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        drop(conn);
        self.get_by_id(id)?
            .ok_or_else(|| OpenFangError::Internal("item missing after update".into()))
    }

    /// Append a free-form event without changing the status.
    pub fn append_event(&self, event: &WorkEvent) -> OpenFangResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        conn.execute(
            "INSERT INTO work_events (id, work_item_id, event_type, from_status, to_status, actor, detail, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                event.id,
                event.work_item_id,
                event.event_type,
                event.from_status,
                event.to_status,
                event.actor,
                event.detail,
                event.created_at.to_rfc3339(),
            ],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Record an approval/rejection decision.
    pub fn record_approval(&self, record: &ApprovalRecord) -> OpenFangResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        conn.execute(
            "INSERT INTO approval_records (id, work_item_id, decision, actor, note, decided_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                record.id,
                record.work_item_id,
                record.decision,
                record.actor,
                record.note,
                record.decided_at.to_rfc3339(),
            ],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Work items — read
    // -----------------------------------------------------------------------

    /// Fetch a single work item by ID.
    pub fn get_by_id(&self, id: &str) -> OpenFangResult<Option<WorkItem>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, title, description, work_type, source, status, approval_status,
                        assigned_agent_id, assigned_agent_name, result, error, iterations, priority,
                        scheduled_at, started_at, completed_at, deadline, requires_approval,
                        approved_by, approved_at, approval_note, payload, tags, created_by,
                    idempotency_key, created_at, updated_at, retry_count, max_retries, parent_id,
                    run_id, workspace_id
                 FROM work_items WHERE id = ?1",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut rows = stmt
            .query_map(rusqlite::params![id], row_to_work_item)
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        match rows.next() {
            Some(Ok(item)) => Ok(Some(item)),
            Some(Err(e)) => Err(OpenFangError::Memory(e.to_string())),
            None => Ok(None),
        }
    }

    /// List work items optionally filtered.
    pub fn list(&self, filter: &WorkItemFilter) -> OpenFangResult<Vec<WorkItem>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;

        let limit = filter.limit.unwrap_or(100).min(500) as i64;
        let offset = filter.offset.unwrap_or(0) as i64;

        // Build parameterised query dynamically
        let mut conditions: Vec<String> = Vec::new();
        let mut param_idx = 1usize;

        if filter.status.is_some() {
            conditions.push(format!("status = ?{param_idx}"));
            param_idx += 1;
        }
        if filter.work_type.is_some() {
            conditions.push(format!("work_type = ?{param_idx}"));
            param_idx += 1;
        }
        if filter.source.is_some() {
            conditions.push(format!("source = ?{param_idx}"));
            param_idx += 1;
        }
        if filter.assigned_agent_id.is_some() {
            conditions.push(format!("assigned_agent_id = ?{param_idx}"));
            param_idx += 1;
        }
        if filter.tag.is_some() {
            conditions.push(format!("tags LIKE ?{param_idx}"));
            param_idx += 1;
        }
        if filter.approval_status.is_some() {
            conditions.push(format!("approval_status = ?{param_idx}"));
            param_idx += 1;
        }
        if filter.parent_id.is_some() {
            conditions.push(format!("parent_id = ?{param_idx}"));
            param_idx += 1;
        }
        if filter.scheduled == Some(true) {
            conditions.push("scheduled_at IS NOT NULL".to_string());
        }
        if filter.scheduled == Some(false) {
            conditions.push("scheduled_at IS NULL".to_string());
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let limit_idx = param_idx;
        let offset_idx = param_idx + 1;

        let sql = format!(
            "SELECT id, title, description, work_type, source, status, approval_status,
                    assigned_agent_id, assigned_agent_name, result, error, iterations, priority,
                    scheduled_at, started_at, completed_at, deadline, requires_approval,
                    approved_by, approved_at, approval_note, payload, tags, created_by,
                    idempotency_key, created_at, updated_at, retry_count, max_retries, parent_id,
                    run_id, workspace_id
             FROM work_items
             {where_clause}
             ORDER BY priority DESC, created_at ASC
             LIMIT ?{limit_idx} OFFSET ?{offset_idx}"
        );

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        // Build parameter list dynamically
        let mut params_box: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(ref v) = filter.status {
            params_box.push(Box::new(v.clone()));
        }
        if let Some(ref v) = filter.work_type {
            params_box.push(Box::new(v.clone()));
        }
        if let Some(ref v) = filter.source {
            params_box.push(Box::new(v.clone()));
        }
        if let Some(ref v) = filter.assigned_agent_id {
            params_box.push(Box::new(v.clone()));
        }
        if let Some(ref v) = filter.tag {
            params_box.push(Box::new(format!("%\"{v}\"%")));
        }
        if let Some(ref v) = filter.approval_status {
            params_box.push(Box::new(v.clone()));
        }
        if let Some(ref v) = filter.parent_id {
            params_box.push(Box::new(v.clone()));
        }
        params_box.push(Box::new(limit));
        params_box.push(Box::new(offset));

        let params_ref: Vec<&dyn rusqlite::ToSql> =
            params_box.iter().map(|b| b.as_ref()).collect();

        let rows = stmt
            .query_map(params_ref.as_slice(), row_to_work_item)
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| OpenFangError::Memory(e.to_string()))?);
        }
        Ok(items)
    }

    /// List events for a work item, oldest first.
    pub fn list_events(&self, work_item_id: &str) -> OpenFangResult<Vec<WorkEvent>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, work_item_id, event_type, from_status, to_status, actor, detail, created_at
                 FROM work_events WHERE work_item_id = ?1 ORDER BY created_at ASC",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let rows = stmt
            .query_map(rusqlite::params![work_item_id], |row| {
                Ok(WorkEvent {
                    id: row.get(0)?,
                    work_item_id: row.get(1)?,
                    event_type: row.get(2)?,
                    from_status: row.get(3)?,
                    to_status: row.get(4)?,
                    actor: row.get(5)?,
                    detail: row.get(6)?,
                    created_at: row
                        .get::<_, String>(7)
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or_else(Utc::now),
                })
            })
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut events = Vec::new();
        for row in rows {
            events.push(row.map_err(|e| OpenFangError::Memory(e.to_string()))?);
        }
        Ok(events)
    }

    /// Count work items by status — used for the operator dashboard summary.
    pub fn count_by_status(&self) -> OpenFangResult<std::collections::HashMap<String, u64>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;

        let mut stmt = conn
            .prepare("SELECT status, COUNT(*) FROM work_items GROUP BY status")
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| {
                let status: String = row.get(0)?;
                let count: i64 = row.get(1)?;
                Ok((status, count as u64))
            })
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut map = std::collections::HashMap::new();
        for row in rows {
            let (k, v) = row.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            map.insert(k, v);
        }
        Ok(map)
    }

    /// Count work items with a scheduled_at date set.
    pub fn count_scheduled(&self) -> OpenFangResult<u64> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM work_items WHERE scheduled_at IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(count as u64)
    }

    /// List direct children of a parent work item.
    pub fn list_children(&self, parent_id: &str) -> OpenFangResult<Vec<WorkItem>> {
        self.list(&WorkItemFilter {
            parent_id: Some(parent_id.to_string()),
            limit: Some(200),
            ..Default::default()
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Map a status transition to a semantic event type string.
fn semantic_event_type(from: &WorkStatus, to: &WorkStatus) -> &'static str {
    match (from, to) {
        (_, WorkStatus::Running) => "started",
        (WorkStatus::Pending, WorkStatus::Ready) => "queued",
        (WorkStatus::Running, WorkStatus::WaitingApproval) => "approval_requested",
        (WorkStatus::Running, WorkStatus::Completed) => "completed",
        (_, WorkStatus::Completed) => "completed",
        (WorkStatus::Running, WorkStatus::Failed) => "failed",
        (_, WorkStatus::Failed) => "failed",
        (_, WorkStatus::Cancelled) => "cancelled",
        (_, WorkStatus::Approved) => "approved",
        (_, WorkStatus::Rejected) => "rejected",
        (WorkStatus::Failed, WorkStatus::Pending) => "retried",
        _ => "status_changed",
    }
}

/// Optional extra fields that may be set during a status transition.
pub struct TransitionExtra {
    pub approval_status: Option<ApprovalStatus>,
    pub approved_by: Option<String>,
    pub approved_at: Option<chrono::DateTime<Utc>>,
    pub approval_note: Option<String>,
    pub result: Option<String>,
    pub error: Option<String>,
    pub iterations: Option<u32>,
}

fn row_to_work_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkItem> {
    let parse_dt = |s: Option<String>| -> Option<chrono::DateTime<Utc>> {
        s.and_then(|v| v.parse().ok())
    };

    let tags_str: String = row.get(22)?;
    let tags: Vec<String> =
        serde_json::from_str(&tags_str).unwrap_or_default();

    let payload_str: String = row.get(21)?;
    let payload: serde_json::Value =
        serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Object(Default::default()));

    let status_str: String = row.get(5)?;
    let approval_status_str: String = row.get(6)?;
    let work_type_str: String = row.get(3)?;
    let source_str: String = row.get(4)?;

    Ok(WorkItem {
        id: row.get(0)?,
        title: row.get(1)?,
        description: row.get(2)?,
        work_type: work_type_str.parse().unwrap_or_default(),
        source: source_str.parse().unwrap_or_default(),
        status: status_str.parse().unwrap_or_default(),
        approval_status: approval_status_str.parse().unwrap_or_default(),
        assigned_agent_id: row.get(7)?,
        assigned_agent_name: row.get(8)?,
        result: row.get(9)?,
        error: row.get(10)?,
        iterations: row.get::<_, i64>(11).unwrap_or(0) as u32,
        priority: row.get::<_, i64>(12).unwrap_or(128) as u8,
        scheduled_at: parse_dt(row.get(13)?),
        started_at: parse_dt(row.get(14)?),
        completed_at: parse_dt(row.get(15)?),
        deadline: parse_dt(row.get(16)?),
        requires_approval: row.get::<_, i64>(17).unwrap_or(0) != 0,
        approved_by: row.get(18)?,
        approved_at: parse_dt(row.get(19)?),
        approval_note: row.get(20)?,
        payload,
        tags,
        created_by: row.get(23)?,
        idempotency_key: row.get(24)?,
        created_at: row
            .get::<_, String>(25)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(Utc::now),
        updated_at: row
            .get::<_, String>(26)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(Utc::now),
        retry_count: row.get::<_, i64>(27).unwrap_or(0) as u32,
        max_retries: row.get::<_, i64>(28).unwrap_or(0) as u32,
        parent_id: row.get(29)?,
        run_id: row.get(30)?,
        workspace_id: row.get(31)?,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::run_migrations;
    use openfang_types::work_item::{
        WorkSource, WorkType,
    };
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    fn make_store() -> WorkItemStore {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        WorkItemStore::new(Arc::new(Mutex::new(conn)))
    }

    fn sample_item(title: &str) -> WorkItem {
        let now = Utc::now();
        let id = uuid::Uuid::new_v4().to_string();
        WorkItem {
            id: id.clone(),
            title: title.to_string(),
            description: "test description".into(),
            work_type: WorkType::AgentTask,
            source: WorkSource::Api,
            status: WorkStatus::Pending,
            approval_status: ApprovalStatus::NotRequired,
            assigned_agent_id: None,
            assigned_agent_name: None,
            result: None,
            error: None,
            iterations: 0,
            priority: 128,
            scheduled_at: None,
            started_at: None,
            completed_at: None,
            deadline: None,
            requires_approval: false,
            approved_by: None,
            approved_at: None,
            approval_note: None,
            payload: serde_json::json!({}),
            tags: vec!["test".into()],
            created_by: Some("user:test".into()),
            idempotency_key: None,
            created_at: now,
            updated_at: now,
            retry_count: 0,
            max_retries: 0,
            parent_id: None,
            run_id: Some(id),
            workspace_id: Some("test-workspace".into()),
        }
    }

    #[test]
    fn test_create_and_get() {
        let store = make_store();
        let item = sample_item("Create test");
        let id = item.id.clone();
        store.create(&item).unwrap();
        let loaded = store.get_by_id(&id).unwrap().unwrap();
        assert_eq!(loaded.title, "Create test");
        assert_eq!(loaded.status, WorkStatus::Pending);
        assert_eq!(loaded.workspace_id.as_deref(), Some("test-workspace"));
    }

    #[test]
    fn test_set_run_context() {
        let store = make_store();
        let item = sample_item("run-context");
        let id = item.id.clone();
        store.create(&item).unwrap();

        store
            .set_run_context(&id, Some("run-123"), Some("workspace-123"))
            .unwrap();

        let loaded = store.get_by_id(&id).unwrap().unwrap();
        assert_eq!(loaded.run_id.as_deref(), Some("run-123"));
        assert_eq!(loaded.workspace_id.as_deref(), Some("workspace-123"));
    }

    #[test]
    fn test_list_with_status_filter() {
        let store = make_store();
        let item1 = sample_item("pending item");
        store.create(&item1).unwrap();
        store
            .transition(&item1.id, WorkStatus::Running, Some("system"), None, None)
            .unwrap();

        let item2 = sample_item("another pending");
        store.create(&item2).unwrap();

        let filter = WorkItemFilter {
            status: Some("pending".into()),
            ..Default::default()
        };
        let list = store.list(&filter).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, item2.id);
    }

    #[test]
    fn test_run_transition() {
        let store = make_store();
        let item = sample_item("run test");
        store.create(&item).unwrap();
        let updated = store
            .transition(&item.id, WorkStatus::Running, Some("system"), Some("starting"), None)
            .unwrap();
        assert_eq!(updated.status, WorkStatus::Running);
        assert!(updated.started_at.is_some());
    }

    #[test]
    fn test_approve_flow() {
        let store = make_store();
        let mut item = sample_item("approval test");
        item.requires_approval = true;
        store.create(&item).unwrap();
        store
            .transition(&item.id, WorkStatus::Running, Some("system"), None, None)
            .unwrap();
        let waiting = store
            .transition(
                &item.id,
                WorkStatus::WaitingApproval,
                Some("system"),
                None,
                None,
            )
            .unwrap();
        assert_eq!(waiting.status, WorkStatus::WaitingApproval);

        let approved = store
            .transition(
                &item.id,
                WorkStatus::Approved,
                Some("alice"),
                Some("looks good"),
                Some(TransitionExtra {
                    approval_status: Some(ApprovalStatus::Approved),
                    approved_by: Some("alice".into()),
                    approved_at: Some(Utc::now()),
                    approval_note: Some("looks good".into()),
                    result: None,
                    error: None,
                    iterations: None,
                }),
            )
            .unwrap();
        assert_eq!(approved.approval_status, ApprovalStatus::Approved);
    }

    #[test]
    fn test_reject_flow() {
        let store = make_store();
        let mut item = sample_item("reject test");
        item.requires_approval = true;
        store.create(&item).unwrap();
        store
            .transition(&item.id, WorkStatus::Running, Some("system"), None, None)
            .unwrap();
        store
            .transition(&item.id, WorkStatus::WaitingApproval, Some("system"), None, None)
            .unwrap();
        let rejected = store
            .transition(
                &item.id,
                WorkStatus::Rejected,
                Some("bob"),
                Some("not approved"),
                Some(TransitionExtra {
                    approval_status: Some(ApprovalStatus::Rejected),
                    approved_by: Some("bob".into()),
                    approved_at: Some(Utc::now()),
                    approval_note: Some("not approved".into()),
                    result: None,
                    error: None,
                    iterations: None,
                }),
            )
            .unwrap();
        assert_eq!(rejected.status, WorkStatus::Rejected);
    }

    #[test]
    fn test_cancel() {
        let store = make_store();
        let item = sample_item("cancel test");
        store.create(&item).unwrap();
        let cancelled = store
            .transition(&item.id, WorkStatus::Cancelled, Some("user"), Some("no longer needed"), None)
            .unwrap();
        assert_eq!(cancelled.status, WorkStatus::Cancelled);
        assert!(cancelled.completed_at.is_some());
    }

    #[test]
    fn test_retry() {
        let store = make_store();
        let mut item = sample_item("retry test");
        item.max_retries = 3;
        store.create(&item).unwrap();
        store
            .transition(&item.id, WorkStatus::Running, Some("system"), None, None)
            .unwrap();
        store
            .transition(
                &item.id,
                WorkStatus::Failed,
                Some("system"),
                Some("network error"),
                None,
            )
            .unwrap();
        let retried = store
            .transition(&item.id, WorkStatus::Pending, Some("system"), Some("retrying"), None)
            .unwrap();
        assert_eq!(retried.status, WorkStatus::Pending);
        assert_eq!(retried.retry_count, 1);
    }

    #[test]
    fn test_invalid_transition() {
        let store = make_store();
        let item = sample_item("invalid transition test");
        store.create(&item).unwrap();
        let result = store.transition(&item.id, WorkStatus::Completed, Some("system"), None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_events_emitted() {
        let store = make_store();
        let item = sample_item("events test");
        store.create(&item).unwrap();
        store
            .transition(&item.id, WorkStatus::Running, Some("system"), None, None)
            .unwrap();
        store
            .transition(&item.id, WorkStatus::Completed, Some("system"), Some("done"), None)
            .unwrap();
        let events = store.list_events(&item.id).unwrap();
        // 1 "created" event + 2 transition events
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].event_type, "created");
        assert_eq!(events[1].from_status.as_deref(), Some("pending"));
        assert_eq!(events[2].from_status.as_deref(), Some("running"));
    }

    #[test]
    fn test_idempotency_key() {
        let store = make_store();
        let mut item = sample_item("idempotent item");
        item.idempotency_key = Some("unique-key-123".into());
        store.create(&item).unwrap();

        // Second create with same key should return existing item, not error
        let mut item2 = sample_item("different title");
        item2.idempotency_key = Some("unique-key-123".into());
        let returned = store.create(&item2).unwrap();
        assert_eq!(returned.title, "idempotent item"); // original title
    }

    #[test]
    fn test_list_default() {
        let store = make_store();
        for i in 0..3 {
            store.create(&sample_item(&format!("item {i}"))).unwrap();
        }
        let list = store.list(&WorkItemFilter::default()).unwrap();
        assert_eq!(list.len(), 3);
    }
}
