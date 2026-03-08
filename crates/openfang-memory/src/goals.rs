//! SQLite-backed goal storage with hierarchical support.

use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// A goal in the hierarchical goal tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub level: String,
    pub status: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub owner_agent_id: Option<String>,
    #[serde(default)]
    pub progress: u8,
    pub created_at: String,
    pub updated_at: String,
}

/// Request to create a new goal.
#[derive(Debug, Deserialize)]
pub struct CreateGoalRequest {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_level")]
    pub level: String,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub owner_agent_id: Option<String>,
    #[serde(default)]
    pub progress: u8,
}

fn default_level() -> String {
    "task".to_string()
}
fn default_status() -> String {
    "planned".to_string()
}

/// Request to update an existing goal.
#[derive(Debug, Deserialize)]
pub struct UpdateGoalRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<Option<String>>,
    #[serde(default)]
    pub level: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub parent_id: Option<Option<String>>,
    #[serde(default)]
    pub owner_agent_id: Option<Option<String>>,
    #[serde(default)]
    pub progress: Option<u8>,
}

/// SQLite-backed goal store.
#[derive(Clone)]
pub struct GoalStore {
    conn: Arc<Mutex<Connection>>,
}

impl GoalStore {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    pub fn list(&self) -> Result<Vec<Goal>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, title, description, level, status, parent_id, \
                 owner_agent_id, progress, created_at, updated_at \
                 FROM goals ORDER BY created_at ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Goal {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    description: row.get(2)?,
                    level: row.get(3)?,
                    status: row.get(4)?,
                    parent_id: row.get(5)?,
                    owner_agent_id: row.get(6)?,
                    progress: row.get::<_, i32>(7)? as u8,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    pub fn get(&self, id: &str) -> Result<Option<Goal>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, title, description, level, status, parent_id, \
                 owner_agent_id, progress, created_at, updated_at \
                 FROM goals WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let result = stmt.query_row(rusqlite::params![id], |row| {
            Ok(Goal {
                id: row.get(0)?,
                title: row.get(1)?,
                description: row.get(2)?,
                level: row.get(3)?,
                status: row.get(4)?,
                parent_id: row.get(5)?,
                owner_agent_id: row.get(6)?,
                progress: row.get::<_, i32>(7)? as u8,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        });
        match result {
            Ok(goal) => Ok(Some(goal)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn create(&self, req: &CreateGoalRequest) -> Result<Goal, String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO goals (id, title, description, level, status, parent_id, \
             owner_agent_id, progress, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                id,
                req.title,
                req.description,
                req.level,
                req.status,
                req.parent_id,
                req.owner_agent_id,
                req.progress as i32,
                now,
                now,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(Goal {
            id,
            title: req.title.clone(),
            description: req.description.clone(),
            level: req.level.clone(),
            status: req.status.clone(),
            parent_id: req.parent_id.clone(),
            owner_agent_id: req.owner_agent_id.clone(),
            progress: req.progress,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    pub fn update(&self, id: &str, req: &UpdateGoalRequest) -> Result<Option<Goal>, String> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().map_err(|e| e.to_string())?;

        // Build dynamic UPDATE
        let mut sets = vec!["updated_at = ?1".to_string()];
        let mut param_idx = 2u32;
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(now.clone())];

        if let Some(ref title) = req.title {
            sets.push(format!("title = ?{param_idx}"));
            params.push(Box::new(title.clone()));
            param_idx += 1;
        }
        if let Some(ref desc) = req.description {
            sets.push(format!("description = ?{param_idx}"));
            params.push(Box::new(desc.clone()));
            param_idx += 1;
        }
        if let Some(ref level) = req.level {
            sets.push(format!("level = ?{param_idx}"));
            params.push(Box::new(level.clone()));
            param_idx += 1;
        }
        if let Some(ref status) = req.status {
            sets.push(format!("status = ?{param_idx}"));
            params.push(Box::new(status.clone()));
            param_idx += 1;
        }
        if let Some(ref parent_id) = req.parent_id {
            sets.push(format!("parent_id = ?{param_idx}"));
            params.push(Box::new(parent_id.clone()));
            param_idx += 1;
        }
        if let Some(ref owner) = req.owner_agent_id {
            sets.push(format!("owner_agent_id = ?{param_idx}"));
            params.push(Box::new(owner.clone()));
            param_idx += 1;
        }
        if let Some(progress) = req.progress {
            sets.push(format!("progress = ?{param_idx}"));
            params.push(Box::new(progress as i32));
            param_idx += 1;
        }

        let sql = format!(
            "UPDATE goals SET {} WHERE id = ?{param_idx}",
            sets.join(", ")
        );
        params.push(Box::new(id.to_string()));

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let affected = conn
            .execute(&sql, param_refs.as_slice())
            .map_err(|e| e.to_string())?;
        drop(conn);

        if affected == 0 {
            return Ok(None);
        }
        self.get(id)
    }

    pub fn delete(&self, id: &str) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        // Clear parent_id references first (children become roots)
        conn.execute(
            "UPDATE goals SET parent_id = NULL WHERE parent_id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| e.to_string())?;
        let affected = conn
            .execute("DELETE FROM goals WHERE id = ?1", rusqlite::params![id])
            .map_err(|e| e.to_string())?;
        Ok(affected > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::run_migrations;

    fn test_store() -> GoalStore {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        GoalStore::new(Arc::new(Mutex::new(conn)))
    }

    #[test]
    fn test_create_and_list() {
        let store = test_store();
        let req = CreateGoalRequest {
            title: "Ship v1.0".to_string(),
            description: Some("First stable release".to_string()),
            level: "mission".to_string(),
            status: "active".to_string(),
            parent_id: None,
            owner_agent_id: None,
            progress: 0,
        };
        let goal = store.create(&req).unwrap();
        assert_eq!(goal.title, "Ship v1.0");
        assert_eq!(goal.level, "mission");

        let all = store.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, goal.id);
    }

    #[test]
    fn test_hierarchy() {
        let store = test_store();
        let parent = store
            .create(&CreateGoalRequest {
                title: "Mission".to_string(),
                description: None,
                level: "mission".to_string(),
                status: "active".to_string(),
                parent_id: None,
                owner_agent_id: None,
                progress: 0,
            })
            .unwrap();
        let child = store
            .create(&CreateGoalRequest {
                title: "Strategy".to_string(),
                description: None,
                level: "strategy".to_string(),
                status: "planned".to_string(),
                parent_id: Some(parent.id.clone()),
                owner_agent_id: None,
                progress: 0,
            })
            .unwrap();
        assert_eq!(child.parent_id.as_deref(), Some(parent.id.as_str()));
    }

    #[test]
    fn test_update() {
        let store = test_store();
        let goal = store
            .create(&CreateGoalRequest {
                title: "Draft".to_string(),
                description: None,
                level: "task".to_string(),
                status: "planned".to_string(),
                parent_id: None,
                owner_agent_id: None,
                progress: 0,
            })
            .unwrap();
        let updated = store
            .update(
                &goal.id,
                &UpdateGoalRequest {
                    title: Some("Final".to_string()),
                    status: Some("completed".to_string()),
                    progress: Some(100),
                    description: None,
                    level: None,
                    parent_id: None,
                    owner_agent_id: None,
                },
            )
            .unwrap()
            .unwrap();
        assert_eq!(updated.title, "Final");
        assert_eq!(updated.status, "completed");
        assert_eq!(updated.progress, 100);
    }

    #[test]
    fn test_delete_reparents_children() {
        let store = test_store();
        let parent = store
            .create(&CreateGoalRequest {
                title: "Parent".to_string(),
                description: None,
                level: "mission".to_string(),
                status: "active".to_string(),
                parent_id: None,
                owner_agent_id: None,
                progress: 0,
            })
            .unwrap();
        let child = store
            .create(&CreateGoalRequest {
                title: "Child".to_string(),
                description: None,
                level: "task".to_string(),
                status: "planned".to_string(),
                parent_id: Some(parent.id.clone()),
                owner_agent_id: None,
                progress: 0,
            })
            .unwrap();
        store.delete(&parent.id).unwrap();
        let orphan = store.get(&child.id).unwrap().unwrap();
        assert!(orphan.parent_id.is_none());
    }
}
