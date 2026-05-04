//! PostgreSQL implementation of the task queue store.

use crate::backends::TaskQueueBackend;
use deadpool_postgres::Pool;
use openfang_types::error::{OpenFangError, OpenFangResult};

pub struct PgTaskQueueStore {
    pool: Pool,
}

impl PgTaskQueueStore {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    fn block_on_pg<F, T>(&self, f: F) -> OpenFangResult<T>
    where
        F: std::future::Future<Output = OpenFangResult<T>>,
    {
        tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(f))
    }
}

impl TaskQueueBackend for PgTaskQueueStore {
    fn task_post(
        &self,
        title: &str,
        description: &str,
        assigned_to: &str,
        created_by: &str,
    ) -> OpenFangResult<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let title = title.to_string();
        let description = description.to_string();
        let assigned_to = assigned_to.to_string();
        let created_by = created_by.to_string();
        let id_clone = id.clone();
        self.block_on_pg(async {
            let client = self
                .pool
                .get()
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            client
                .execute(
                    "INSERT INTO task_queue (id, agent_id, task_type, payload, status, priority, created_at, title, description, assigned_to, created_by)
                     VALUES ($1, $2, $3, $4, 'pending', 0, $5, $6, $7, $8, $9)",
                    &[
                        &id_clone,
                        &created_by,
                        &title,
                        &Vec::<u8>::new() as &(dyn tokio_postgres::types::ToSql + Sync),
                        &now,
                        &title,
                        &description,
                        &assigned_to,
                        &created_by,
                    ],
                )
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(())
        })?;
        Ok(id)
    }

    fn task_claim(&self, agent_id: &str) -> OpenFangResult<Option<serde_json::Value>> {
        let agent_id = agent_id.to_string();
        self.block_on_pg(async {
            let client = self
                .pool
                .get()
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;

            let row = client
                .query_opt(
                    "SELECT id, title, description, assigned_to, created_by, created_at
                     FROM task_queue
                     WHERE status = 'pending' AND (assigned_to = $1 OR assigned_to = '')
                     ORDER BY priority DESC, created_at ASC
                     LIMIT 1",
                    &[&agent_id],
                )
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;

            match row {
                Some(r) => {
                    let id: String = r.get(0);
                    let title: String = r.get(1);
                    let description: String = r.get(2);
                    let assigned: String = r.get(3);
                    let created_by: String = r.get(4);
                    let created_at: String = r.get(5);

                    // Update status to in_progress
                    client
                        .execute(
                            "UPDATE task_queue SET status = 'in_progress', assigned_to = $2 WHERE id = $1",
                            &[&id, &agent_id],
                        )
                        .await
                        .map_err(|e| OpenFangError::Memory(e.to_string()))?;

                    let display_assigned = if assigned.is_empty() {
                        agent_id.clone()
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
                None => Ok(None),
            }
        })
    }

    fn task_complete(&self, task_id: &str, result: &str) -> OpenFangResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let task_id = task_id.to_string();
        let result = result.to_string();
        self.block_on_pg(async {
            let client = self
                .pool
                .get()
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let rows = client
                .execute(
                    "UPDATE task_queue SET status = 'completed', result = $2, completed_at = $3 WHERE id = $1",
                    &[&task_id, &result, &now],
                )
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            if rows == 0 {
                return Err(OpenFangError::Internal(format!(
                    "Task not found: {task_id}"
                )));
            }
            Ok(())
        })
    }

    fn task_list(&self, status: Option<&str>) -> OpenFangResult<Vec<serde_json::Value>> {
        let status = status.map(|s| s.to_string());
        self.block_on_pg(async {
            let client = self
                .pool
                .get()
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;

            let rows = match &status {
                Some(s) => {
                    client
                        .query(
                            "SELECT id, title, description, status, assigned_to, created_by, created_at, completed_at, result
                             FROM task_queue WHERE status = $1 ORDER BY created_at DESC",
                            &[s],
                        )
                        .await
                }
                None => {
                    client
                        .query(
                            "SELECT id, title, description, status, assigned_to, created_by, created_at, completed_at, result
                             FROM task_queue ORDER BY created_at DESC",
                            &[],
                        )
                        .await
                }
            }
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

            Ok(rows
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "id": r.get::<_, String>(0),
                        "title": r.get::<_, String>(1),
                        "description": r.get::<_, String>(2),
                        "status": r.get::<_, String>(3),
                        "assigned_to": r.get::<_, String>(4),
                        "created_by": r.get::<_, String>(5),
                        "created_at": r.get::<_, String>(6),
                        "completed_at": r.get::<_, Option<String>>(7),
                        "result": r.get::<_, Option<String>>(8),
                    })
                })
                .collect())
        })
    }
}
