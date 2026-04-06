//! SQLite implementation of the task queue store.

use crate::backends::TaskQueueBackend;
use openfang_types::error::{OpenFangError, OpenFangResult};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

/// Task-queue store backed by SQLite.
#[derive(Clone)]
pub struct SqliteTaskQueueStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteTaskQueueStore {
    /// Create a new task-queue store wrapping the given connection.
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }
}

impl TaskQueueBackend for SqliteTaskQueueStore {
    fn task_post(
        &self,
        title: &str,
        description: &str,
        assigned_to: &str,
        created_by: &str,
    ) -> OpenFangResult<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let db = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        db.execute(
            "INSERT INTO task_queue (id, agent_id, task_type, payload, status, priority, created_at, title, description, assigned_to, created_by)
             VALUES (?1, ?2, ?3, ?4, 'pending', 0, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![id, created_by, title, b"", now, title, description, assigned_to, created_by],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(id)
    }

    fn task_claim(&self, agent_id: &str) -> OpenFangResult<Option<serde_json::Value>> {
        let db = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let mut stmt = db
            .prepare(
                "SELECT id, title, description, assigned_to, created_by, created_at
                 FROM task_queue
                 WHERE status = 'pending' AND (assigned_to = ?1 OR assigned_to = '')
                 ORDER BY priority DESC, created_at ASC
                 LIMIT 1",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let result = stmt.query_row(rusqlite::params![agent_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        });

        match result {
            Ok((id, title, description, assigned, created_by, created_at)) => {
                // Update status to in_progress
                db.execute(
                    "UPDATE task_queue SET status = 'in_progress', assigned_to = ?2 WHERE id = ?1",
                    rusqlite::params![id, agent_id],
                )
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;

                let display_assigned = if assigned.is_empty() {
                    agent_id.to_string()
                } else {
                    assigned
                };

                Ok(Some(serde_json::json!({
                    "id": id,
                    "title": title,
                    "description": description,
                    "status": "in_progress",
                    "assigned_to": display_assigned,
                    "created_by": created_by,
                    "created_at": created_at,
                })))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(OpenFangError::Memory(e.to_string())),
        }
    }

    fn task_complete(&self, task_id: &str, result: &str) -> OpenFangResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let db = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let rows = db
            .execute(
                "UPDATE task_queue SET status = 'completed', result = ?2, completed_at = ?3 WHERE id = ?1",
                rusqlite::params![task_id, result, now],
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        if rows == 0 {
            return Err(OpenFangError::Internal(format!(
                "Task not found: {task_id}"
            )));
        }
        Ok(())
    }

    fn task_list(&self, status: Option<&str>) -> OpenFangResult<Vec<serde_json::Value>> {
        let db = self
            .conn
            .lock()
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let (sql, params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = match status {
            Some(s) => (
                "SELECT id, title, description, status, assigned_to, created_by, created_at, completed_at, result FROM task_queue WHERE status = ?1 ORDER BY created_at DESC",
                vec![Box::new(s.to_string())],
            ),
            None => (
                "SELECT id, title, description, status, assigned_to, created_by, created_at, completed_at, result FROM task_queue ORDER BY created_at DESC",
                vec![],
            ),
        };

        let mut stmt = db
            .prepare(sql)
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "title": row.get::<_, String>(1).unwrap_or_default(),
                    "description": row.get::<_, String>(2).unwrap_or_default(),
                    "status": row.get::<_, String>(3)?,
                    "assigned_to": row.get::<_, String>(4).unwrap_or_default(),
                    "created_by": row.get::<_, String>(5).unwrap_or_default(),
                    "created_at": row.get::<_, String>(6).unwrap_or_default(),
                    "completed_at": row.get::<_, Option<String>>(7).unwrap_or(None),
                    "result": row.get::<_, Option<String>>(8).unwrap_or(None),
                }))
            })
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut tasks = Vec::new();
        for row in rows {
            tasks.push(row.map_err(|e| OpenFangError::Memory(e.to_string()))?);
        }
        Ok(tasks)
    }
}
