use chrono::{DateTime, NaiveDate, Utc};
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::planner::{
    EnergyLevel, PlannerInboxItem, PlannerInboxStatus, PlannerProject, PlannerProjectStatus,
    PlannerReview, PlannerRoutine, PlannerTask, PlannerTaskStatus, PlannerTodayPlan,
    PriorityBand,
};
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct PlannerStore {
    conn: Arc<Mutex<Connection>>,
}

impl PlannerStore {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    pub fn list_inbox(&self) -> OpenFangResult<Vec<PlannerInboxItem>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, text, status, created_at, clarified_at, task_id, project_id
                 FROM planner_inbox_items
                 ORDER BY created_at DESC",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(PlannerInboxItem {
                    id: row.get(0)?,
                    text: row.get(1)?,
                    status: parse_inbox_status(&row.get::<_, String>(2)?),
                    created_at: parse_dt(&row.get::<_, String>(3)?),
                    clarified_at: row
                        .get::<_, Option<String>>(4)?
                        .and_then(|v| parse_dt_opt(v.as_str())),
                    task_id: row.get(5)?,
                    project_id: row.get(6)?,
                })
            })
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| OpenFangError::Memory(e.to_string()))?);
        }
        Ok(items)
    }

    pub fn create_inbox_item(&self, text: &str) -> OpenFangResult<PlannerInboxItem> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(OpenFangError::InvalidInput("Inbox text must not be empty".to_string()));
        }

        let item = PlannerInboxItem {
            id: uuid::Uuid::new_v4().to_string(),
            text: trimmed.to_string(),
            status: PlannerInboxStatus::Captured,
            created_at: Utc::now(),
            clarified_at: None,
            task_id: None,
            project_id: None,
        };

        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO planner_inbox_items (id, text, status, created_at, clarified_at, task_id, project_id)
             VALUES (?1, ?2, ?3, ?4, NULL, NULL, NULL)",
            params![
                item.id,
                item.text,
                inbox_status_str(&item.status),
                item.created_at.to_rfc3339(),
            ],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        Ok(item)
    }

    pub fn get_today_plan(&self, date: NaiveDate) -> OpenFangResult<Option<PlannerTodayPlan>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare("SELECT payload FROM planner_today_plans WHERE date = ?1")
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let result: Result<String, rusqlite::Error> =
            stmt.query_row(params![date.format("%Y-%m-%d").to_string()], |row| row.get(0));

        match result {
            Ok(payload) => serde_json::from_str::<PlannerTodayPlan>(&payload)
                .map(Some)
                .map_err(|e| OpenFangError::Serialization(e.to_string())),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(OpenFangError::Memory(e.to_string())),
        }
    }

    pub fn list_routines(&self) -> OpenFangResult<Vec<PlannerRoutine>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, trigger, thread_label, active, last_run_at, next_run_at
                 FROM planner_routines ORDER BY name ASC",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(PlannerRoutine {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    trigger: row.get(2)?,
                    thread_label: row.get(3)?,
                    active: row.get::<_, i64>(4)? != 0,
                    last_run_at: row
                        .get::<_, Option<String>>(5)?
                        .and_then(|v| parse_dt_opt(v.as_str())),
                    next_run_at: row
                        .get::<_, Option<String>>(6)?
                        .and_then(|v| parse_dt_opt(v.as_str())),
                })
            })
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut routines = Vec::new();
        for row in rows {
            routines.push(row.map_err(|e| OpenFangError::Memory(e.to_string()))?);
        }
        Ok(routines)
    }

    pub fn list_agent_preferences(&self) -> OpenFangResult<HashMap<String, bool>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare("SELECT agent_id, enabled FROM planner_agent_preferences")
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0)))
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut preferences = HashMap::new();
        for row in rows {
            let (agent_id, enabled) = row.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            preferences.insert(agent_id, enabled);
        }
        Ok(preferences)
    }

    pub fn set_agent_enabled(&self, agent_id: &str, enabled: bool) -> OpenFangResult<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO planner_agent_preferences (agent_id, enabled, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(agent_id) DO UPDATE SET enabled = excluded.enabled, updated_at = excluded.updated_at",
            params![agent_id, if enabled { 1 } else { 0 }, Utc::now().to_rfc3339()],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    pub fn create_review(&self, review: &PlannerReview) -> OpenFangResult<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO planner_reviews (id, scope, created_at, summary, wins, misses, adjustments)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                review.id,
                review_scope_str(&review.scope),
                review.created_at.to_rfc3339(),
                review.summary,
                serde_json::to_string(&review.wins)
                    .map_err(|e| OpenFangError::Serialization(e.to_string()))?,
                serde_json::to_string(&review.misses)
                    .map_err(|e| OpenFangError::Serialization(e.to_string()))?,
                serde_json::to_string(&review.adjustments)
                    .map_err(|e| OpenFangError::Serialization(e.to_string()))?,
            ],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    pub(crate) fn upsert_today_plan(&self, plan: &PlannerTodayPlan) -> OpenFangResult<()> {
        let conn = self.lock_conn()?;
        let payload = serde_json::to_string(plan)
            .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
        conn.execute(
            "INSERT INTO planner_today_plans (date, payload, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(date) DO UPDATE SET payload = excluded.payload, updated_at = excluded.updated_at",
            params![plan.date, payload, Utc::now().to_rfc3339()],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    pub(crate) fn create_task(&self, task: &PlannerTask) -> OpenFangResult<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO planner_tasks (
                id, title, status, priority, effort_minutes, energy, project_id, due_at,
                scheduled_for, blocked_by, next_action, source_inbox_item_id, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                task.id,
                task.title,
                task_status_str(&task.status),
                priority_str(&task.priority),
                task.effort_minutes.map(|v| v as i64),
                energy_str(&task.energy),
                task.project_id,
                task.due_at.map(|v| v.to_rfc3339()),
                task.scheduled_for.map(|v| v.to_rfc3339()),
                serde_json::to_string(&task.blocked_by)
                    .map_err(|e| OpenFangError::Serialization(e.to_string()))?,
                task.next_action,
                task.source_inbox_item_id,
                task.created_at.to_rfc3339(),
                task.updated_at.to_rfc3339(),
            ],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    pub(crate) fn save_project(&self, project: &PlannerProject) -> OpenFangResult<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO planner_projects (id, title, outcome, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                project.id,
                project.title,
                project.outcome,
                project_status_str(&project.status),
                project.created_at.to_rfc3339(),
                project.updated_at.to_rfc3339(),
            ],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    pub(crate) fn update_inbox_item(&self, item: &PlannerInboxItem) -> OpenFangResult<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "UPDATE planner_inbox_items
             SET status = ?2, clarified_at = ?3, task_id = ?4, project_id = ?5
             WHERE id = ?1",
            params![
                item.id,
                inbox_status_str(&item.status),
                item.clarified_at.map(|v| v.to_rfc3339()),
                item.task_id,
                item.project_id,
            ],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    pub(crate) fn get_inbox_item(
        &self,
        inbox_item_id: &str,
    ) -> OpenFangResult<Option<PlannerInboxItem>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, text, status, created_at, clarified_at, task_id, project_id
                 FROM planner_inbox_items WHERE id = ?1",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let result = stmt.query_row(params![inbox_item_id], |row| {
            Ok(PlannerInboxItem {
                id: row.get(0)?,
                text: row.get(1)?,
                status: parse_inbox_status(&row.get::<_, String>(2)?),
                created_at: parse_dt(&row.get::<_, String>(3)?),
                clarified_at: row
                    .get::<_, Option<String>>(4)?
                    .and_then(|v| parse_dt_opt(v.as_str())),
                task_id: row.get(5)?,
                project_id: row.get(6)?,
            })
        });

        match result {
            Ok(item) => Ok(Some(item)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(OpenFangError::Memory(e.to_string())),
        }
    }

    pub(crate) fn get_task(&self, task_id: &str) -> OpenFangResult<Option<PlannerTask>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, title, status, priority, effort_minutes, energy, project_id, due_at,
                        scheduled_for, blocked_by, next_action, source_inbox_item_id, created_at, updated_at
                 FROM planner_tasks WHERE id = ?1",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let result = stmt.query_row(params![task_id], row_to_task);
        match result {
            Ok(task) => Ok(Some(task)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(OpenFangError::Memory(e.to_string())),
        }
    }

    pub(crate) fn list_tasks_for_inbox_item(
        &self,
        inbox_item_id: &str,
    ) -> OpenFangResult<Vec<PlannerTask>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, title, status, priority, effort_minutes, energy, project_id, due_at,
                        scheduled_for, blocked_by, next_action, source_inbox_item_id, created_at, updated_at
                 FROM planner_tasks
                 WHERE source_inbox_item_id = ?1
                 ORDER BY created_at ASC",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let rows = stmt
            .query_map(params![inbox_item_id], row_to_task)
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut tasks = Vec::new();
        for row in rows {
            tasks.push(row.map_err(|e| OpenFangError::Memory(e.to_string()))?);
        }
        Ok(tasks)
    }

    pub(crate) fn get_project(&self, project_id: &str) -> OpenFangResult<Option<PlannerProject>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, title, outcome, status, created_at, updated_at
                 FROM planner_projects WHERE id = ?1",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let result = stmt.query_row(params![project_id], |row| {
            Ok(PlannerProject {
                id: row.get(0)?,
                title: row.get(1)?,
                outcome: row.get(2)?,
                status: parse_project_status(&row.get::<_, String>(3)?),
                created_at: parse_dt(&row.get::<_, String>(4)?),
                updated_at: parse_dt(&row.get::<_, String>(5)?),
            })
        });

        match result {
            Ok(project) => Ok(Some(project)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(OpenFangError::Memory(e.to_string())),
        }
    }

    pub(crate) fn list_open_tasks(&self) -> OpenFangResult<Vec<PlannerTask>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, title, status, priority, effort_minutes, energy, project_id, due_at,
                        scheduled_for, blocked_by, next_action, source_inbox_item_id, created_at, updated_at
                 FROM planner_tasks
                 WHERE status != 'done'
                 ORDER BY updated_at DESC",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let rows = stmt
            .query_map([], row_to_task)
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut tasks = Vec::new();
        for row in rows {
            tasks.push(row.map_err(|e| OpenFangError::Memory(e.to_string()))?);
        }
        Ok(tasks)
    }

    fn lock_conn(&self) -> OpenFangResult<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|e| OpenFangError::Internal(e.to_string()))
    }
}

fn row_to_task(row: &rusqlite::Row<'_>) -> Result<PlannerTask, rusqlite::Error> {
    let blocked_by_raw: String = row.get(9)?;
    let blocked_by = serde_json::from_str::<Vec<String>>(&blocked_by_raw).unwrap_or_default();
    Ok(PlannerTask {
        id: row.get(0)?,
        title: row.get(1)?,
        status: parse_task_status(&row.get::<_, String>(2)?),
        priority: parse_priority(&row.get::<_, String>(3)?),
        effort_minutes: row.get::<_, Option<i64>>(4)?.map(|v| v as u32),
        energy: parse_energy(&row.get::<_, String>(5)?),
        project_id: row.get(6)?,
        due_at: row
            .get::<_, Option<String>>(7)?
            .and_then(|v| parse_dt_opt(v.as_str())),
        scheduled_for: row
            .get::<_, Option<String>>(8)?
            .and_then(|v| parse_dt_opt(v.as_str())),
        blocked_by,
        next_action: row.get(10)?,
        source_inbox_item_id: row.get(11)?,
        agent_recommendation: None,
        created_at: parse_dt(&row.get::<_, String>(12)?),
        updated_at: parse_dt(&row.get::<_, String>(13)?),
    })
}

fn parse_dt(input: &str) -> DateTime<Utc> {
    parse_dt_opt(input).unwrap_or_else(Utc::now)
}

fn parse_dt_opt(input: &str) -> Option<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(input)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn inbox_status_str(status: &PlannerInboxStatus) -> &'static str {
    match status {
        PlannerInboxStatus::Captured => "captured",
        PlannerInboxStatus::Clarified => "clarified",
    }
}

fn parse_inbox_status(status: &str) -> PlannerInboxStatus {
    match status {
        "clarified" => PlannerInboxStatus::Clarified,
        _ => PlannerInboxStatus::Captured,
    }
}

fn task_status_str(status: &PlannerTaskStatus) -> &'static str {
    match status {
        PlannerTaskStatus::Todo => "todo",
        PlannerTaskStatus::InProgress => "in_progress",
        PlannerTaskStatus::Blocked => "blocked",
        PlannerTaskStatus::Done => "done",
    }
}

fn parse_task_status(status: &str) -> PlannerTaskStatus {
    match status {
        "in_progress" => PlannerTaskStatus::InProgress,
        "blocked" => PlannerTaskStatus::Blocked,
        "done" => PlannerTaskStatus::Done,
        _ => PlannerTaskStatus::Todo,
    }
}

fn priority_str(priority: &PriorityBand) -> &'static str {
    match priority {
        PriorityBand::Low => "low",
        PriorityBand::Medium => "medium",
        PriorityBand::High => "high",
        PriorityBand::Urgent => "urgent",
    }
}

fn parse_priority(priority: &str) -> PriorityBand {
    match priority {
        "low" => PriorityBand::Low,
        "high" => PriorityBand::High,
        "urgent" => PriorityBand::Urgent,
        _ => PriorityBand::Medium,
    }
}

fn energy_str(energy: &EnergyLevel) -> &'static str {
    match energy {
        EnergyLevel::Low => "low",
        EnergyLevel::Medium => "medium",
        EnergyLevel::High => "high",
    }
}

fn parse_energy(energy: &str) -> EnergyLevel {
    match energy {
        "low" => EnergyLevel::Low,
        "high" => EnergyLevel::High,
        _ => EnergyLevel::Medium,
    }
}

fn project_status_str(status: &PlannerProjectStatus) -> &'static str {
    match status {
        PlannerProjectStatus::Active => "active",
        PlannerProjectStatus::OnHold => "on_hold",
        PlannerProjectStatus::Done => "done",
    }
}

fn parse_project_status(status: &str) -> PlannerProjectStatus {
    match status {
        "on_hold" => PlannerProjectStatus::OnHold,
        "done" => PlannerProjectStatus::Done,
        _ => PlannerProjectStatus::Active,
    }
}

fn review_scope_str(scope: &openfang_types::planner::PlannerReviewScope) -> &'static str {
    match scope {
        openfang_types::planner::PlannerReviewScope::Shutdown => "shutdown",
        openfang_types::planner::PlannerReviewScope::Weekly => "weekly",
    }
}
