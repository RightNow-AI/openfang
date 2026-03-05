# Goals Feature Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a hierarchical Goals system with a dedicated dashboard tab, SQLite persistence, and full CRUD API — inspired by Paperclip's goal alignment model.

**Architecture:** API-only with direct SQLite storage via MemorySubstrate's shared connection. No kernel subsystem — goals are stored in a new `goals` table and served via 5 REST endpoints. The dashboard gets a new "Goals" page with 3 tabs: Tree view, Board (kanban), and Timeline.

**Tech Stack:** Rust (axum routes, rusqlite), Alpine.js (dashboard UI), SQLite (persistence)

---

### Task 1: Add Goals SQLite Migration

**Files:**
- Modify: `crates/openfang-memory/src/migration.rs`

**Step 1: Add migrate_v8 function and bump schema version**

In `migration.rs`, change `SCHEMA_VERSION` from 7 to 8, add the `migrate_v8` call in `run_migrations`, and add the migration function:

```rust
// At top: change
const SCHEMA_VERSION: u32 = 8;

// In run_migrations, after the v7 block add:
    if current_version < 8 {
        migrate_v8(conn)?;
    }

// New function after migrate_v7:
fn migrate_v8(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS goals (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            description TEXT,
            level TEXT NOT NULL DEFAULT 'task',
            status TEXT NOT NULL DEFAULT 'planned',
            parent_id TEXT,
            owner_agent_id TEXT,
            progress INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (parent_id) REFERENCES goals(id) ON DELETE SET NULL
        );

        CREATE INDEX IF NOT EXISTS idx_goals_parent ON goals(parent_id);
        CREATE INDEX IF NOT EXISTS idx_goals_status ON goals(status);
        CREATE INDEX IF NOT EXISTS idx_goals_level ON goals(level);

        INSERT OR IGNORE INTO migrations (version, applied_at, description)
        VALUES (8, datetime('now'), 'Add goals table for hierarchical goal tracking');
        ",
    )?;
    Ok(())
}
```

**Step 2: Run tests to verify migration**

Run: `cargo test -p openfang-memory test_migration_creates_tables -v`
Expected: PASS (existing test still works, new table created)

**Step 3: Commit**

```bash
git add crates/openfang-memory/src/migration.rs
git commit -m "feat: add goals table migration (v8)"
```

---

### Task 2: Add GoalStore in openfang-memory

**Files:**
- Create: `crates/openfang-memory/src/goals.rs`
- Modify: `crates/openfang-memory/src/lib.rs`
- Modify: `crates/openfang-memory/src/substrate.rs`

**Step 1: Create `goals.rs` with full CRUD**

```rust
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

fn default_level() -> String { "task".to_string() }
fn default_status() -> String { "planned".to_string() }

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
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
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

        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let affected = conn.execute(&sql, param_refs.as_slice()).map_err(|e| e.to_string())?;
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
```

**Step 2: Register module in lib.rs**

Add `pub mod goals;` to `crates/openfang-memory/src/lib.rs`.

**Step 3: Add GoalStore to MemorySubstrate**

In `crates/openfang-memory/src/substrate.rs`:
- Add `use crate::goals::GoalStore;` at the top
- Add `goals: GoalStore,` field to `MemorySubstrate` struct
- Initialize it in both `open()` and `open_in_memory()`: `goals: GoalStore::new(Arc::clone(&shared)),`
- Add accessor:
```rust
    /// Get a reference to the goal store.
    pub fn goals(&self) -> &GoalStore {
        &self.goals
    }
```

**Step 4: Run tests**

Run: `cargo test -p openfang-memory -- -v`
Expected: All existing tests PASS + 4 new goal tests PASS

**Step 5: Commit**

```bash
git add crates/openfang-memory/src/goals.rs crates/openfang-memory/src/lib.rs crates/openfang-memory/src/substrate.rs
git commit -m "feat: add GoalStore with CRUD and hierarchy support"
```

---

### Task 3: Add Goals API Routes

**Files:**
- Modify: `crates/openfang-api/src/routes.rs`
- Modify: `crates/openfang-api/src/server.rs`

**Step 1: Add 5 route handlers in `routes.rs`**

Add these at the end of the file (before the final `#[cfg(test)]` block if any, or at the very end):

```rust
// ═══════════════════════════════════════════════════════════════════
// Goals endpoints
// ═══════════════════════════════════════════════════════════════════

/// GET /api/goals — List all goals.
pub async fn list_goals(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.kernel.memory.goals().list() {
        Ok(goals) => Json(serde_json::json!({ "goals": goals, "total": goals.len() })),
        Err(e) => Json(serde_json::json!({ "goals": [], "total": 0, "error": e })),
    }
}

/// POST /api/goals — Create a new goal.
pub async fn create_goal(
    State(state): State<Arc<AppState>>,
    Json(req): Json<openfang_memory::goals::CreateGoalRequest>,
) -> impl IntoResponse {
    // Validate level
    let valid_levels = ["mission", "strategy", "objective", "task"];
    if !valid_levels.contains(&req.level.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!("Invalid level '{}'. Must be one of: {}", req.level, valid_levels.join(", "))})),
        );
    }
    // Validate status
    let valid_statuses = ["planned", "active", "completed", "paused"];
    if !valid_statuses.contains(&req.status.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!("Invalid status '{}'. Must be one of: {}", req.status, valid_statuses.join(", "))})),
        );
    }
    match state.kernel.memory.goals().create(&req) {
        Ok(goal) => (StatusCode::CREATED, Json(serde_json::json!(goal))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

/// GET /api/goals/{id} — Get a single goal.
pub async fn get_goal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.kernel.memory.goals().get(&id) {
        Ok(Some(goal)) => (StatusCode::OK, Json(serde_json::json!(goal))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Goal not found"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

/// PUT /api/goals/{id} — Update a goal.
pub async fn update_goal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<openfang_memory::goals::UpdateGoalRequest>,
) -> impl IntoResponse {
    if let Some(ref level) = req.level {
        let valid_levels = ["mission", "strategy", "objective", "task"];
        if !valid_levels.contains(&level.as_str()) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Invalid level '{level}'")})),
            );
        }
    }
    if let Some(ref status) = req.status {
        let valid_statuses = ["planned", "active", "completed", "paused"];
        if !valid_statuses.contains(&status.as_str()) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Invalid status '{status}'")})),
            );
        }
    }
    match state.kernel.memory.goals().update(&id, &req) {
        Ok(Some(goal)) => (StatusCode::OK, Json(serde_json::json!(goal))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Goal not found"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

/// DELETE /api/goals/{id} — Delete a goal (children become root goals).
pub async fn delete_goal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.kernel.memory.goals().delete(&id) {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({"deleted": true}))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Goal not found"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}
```

**Step 2: Register routes in `server.rs`**

In `crates/openfang-api/src/server.rs`, after the hands routes block (around line 399), add:

```rust
        // Goals endpoints
        .route("/api/goals", axum::routing::get(routes::list_goals).post(routes::create_goal))
        .route(
            "/api/goals/{id}",
            axum::routing::get(routes::get_goal)
                .put(routes::update_goal)
                .delete(routes::delete_goal),
        )
```

**Step 3: Build and verify**

Run: `cargo build --workspace --lib`
Expected: Compiles with zero errors

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: Zero warnings

**Step 4: Commit**

```bash
git add crates/openfang-api/src/routes.rs crates/openfang-api/src/server.rs
git commit -m "feat: add Goals CRUD API endpoints"
```

---

### Task 4: Add Goals Dashboard Page (JS)

**Files:**
- Create: `crates/openfang-api/static/js/pages/goals.js`

**Step 1: Create the Goals page Alpine.js component**

```javascript
// OpenFang Goals Page — hierarchical goal tracking
'use strict';

function goalsPage() {
  return {
    tab: 'tree',
    goals: [],
    loading: true,
    loadError: '',

    // Create form
    showCreateForm: false,
    newGoal: {
      title: '',
      description: '',
      level: 'task',
      status: 'planned',
      parent_id: '',
      owner_agent_id: '',
      progress: 0
    },
    creating: false,

    // Edit
    editGoal: null,
    editForm: {},
    saving: false,

    // ── Lifecycle ──

    async loadData() {
      this.loading = true;
      this.loadError = '';
      try {
        var data = await OpenFangAPI.get('/api/goals');
        this.goals = data.goals || [];
      } catch(e) {
        this.goals = [];
        this.loadError = e.message || 'Could not load goals.';
      }
      this.loading = false;
    },

    // ── Tree helpers ──

    rootGoals() {
      return this.goals.filter(function(g) { return !g.parent_id; });
    },

    childrenOf(parentId) {
      return this.goals.filter(function(g) { return g.parent_id === parentId; });
    },

    goalDepth(goalId) {
      var depth = 0;
      var current = this.goals.find(function(g) { return g.id === goalId; });
      while (current && current.parent_id && depth < 10) {
        depth++;
        var pid = current.parent_id;
        current = this.goals.find(function(g) { return g.id === pid; });
      }
      return depth;
    },

    // Build flattened tree order for rendering
    treeOrder() {
      var result = [];
      var self = this;
      function walk(parentId, depth) {
        var children = self.goals.filter(function(g) {
          return parentId ? g.parent_id === parentId : !g.parent_id;
        });
        // Sort: missions first, then by level, then alphabetical
        var levelOrder = { mission: 0, strategy: 1, objective: 2, task: 3 };
        children.sort(function(a, b) {
          var la = levelOrder[a.level] || 4;
          var lb = levelOrder[b.level] || 4;
          if (la !== lb) return la - lb;
          return a.title.localeCompare(b.title);
        });
        for (var i = 0; i < children.length; i++) {
          result.push({ goal: children[i], depth: depth });
          walk(children[i].id, depth + 1);
        }
      }
      walk(null, 0);
      return result;
    },

    // ── Board helpers (kanban) ──

    goalsByStatus(status) {
      return this.goals.filter(function(g) { return g.status === status; });
    },

    // ── CRUD ──

    async createGoal() {
      if (!this.newGoal.title.trim()) {
        OpenFangToast.warn('Please enter a goal title');
        return;
      }
      this.creating = true;
      try {
        var body = {
          title: this.newGoal.title,
          description: this.newGoal.description || null,
          level: this.newGoal.level,
          status: this.newGoal.status,
          parent_id: this.newGoal.parent_id || null,
          owner_agent_id: this.newGoal.owner_agent_id || null,
          progress: parseInt(this.newGoal.progress, 10) || 0
        };
        await OpenFangAPI.post('/api/goals', body);
        this.showCreateForm = false;
        this.newGoal = { title: '', description: '', level: 'task', status: 'planned', parent_id: '', owner_agent_id: '', progress: 0 };
        OpenFangToast.success('Goal created');
        await this.loadData();
      } catch(e) {
        OpenFangToast.error('Failed to create goal: ' + (e.message || e));
      }
      this.creating = false;
    },

    openEdit(goal) {
      this.editGoal = goal;
      this.editForm = {
        title: goal.title,
        description: goal.description || '',
        level: goal.level,
        status: goal.status,
        parent_id: goal.parent_id || '',
        owner_agent_id: goal.owner_agent_id || '',
        progress: goal.progress
      };
    },

    async saveEdit() {
      if (!this.editGoal) return;
      this.saving = true;
      try {
        var body = {};
        if (this.editForm.title !== this.editGoal.title) body.title = this.editForm.title;
        if (this.editForm.description !== (this.editGoal.description || '')) body.description = this.editForm.description || null;
        if (this.editForm.level !== this.editGoal.level) body.level = this.editForm.level;
        if (this.editForm.status !== this.editGoal.status) body.status = this.editForm.status;
        if (this.editForm.parent_id !== (this.editGoal.parent_id || '')) body.parent_id = this.editForm.parent_id || null;
        if (this.editForm.owner_agent_id !== (this.editGoal.owner_agent_id || '')) body.owner_agent_id = this.editForm.owner_agent_id || null;
        if (parseInt(this.editForm.progress, 10) !== this.editGoal.progress) body.progress = parseInt(this.editForm.progress, 10);
        await OpenFangAPI.put('/api/goals/' + this.editGoal.id, body);
        this.editGoal = null;
        OpenFangToast.success('Goal updated');
        await this.loadData();
      } catch(e) {
        OpenFangToast.error('Failed to update goal: ' + (e.message || e));
      }
      this.saving = false;
    },

    deleteGoal(goal) {
      var self = this;
      OpenFangToast.confirm('Delete Goal', 'Delete "' + goal.title + '"? Children will become root goals.', async function() {
        try {
          await OpenFangAPI.del('/api/goals/' + goal.id);
          OpenFangToast.success('Goal deleted');
          await self.loadData();
        } catch(e) {
          OpenFangToast.error('Failed to delete goal: ' + (e.message || e));
        }
      });
    },

    async setStatus(goal, status) {
      try {
        var progress = status === 'completed' ? 100 : goal.progress;
        await OpenFangAPI.put('/api/goals/' + goal.id, { status: status, progress: progress });
        await this.loadData();
      } catch(e) {
        OpenFangToast.error('Failed to update status: ' + (e.message || e));
      }
    },

    // ── Display helpers ──

    levelBadgeClass(level) {
      var map = { mission: 'badge-info', strategy: 'badge-created', objective: 'badge-warn', task: 'badge-dim' };
      return map[level] || 'badge-dim';
    },

    statusBadgeClass(status) {
      var map = { planned: 'badge-dim', active: 'badge-info', completed: 'badge-success', paused: 'badge-warn' };
      return map[status] || 'badge-dim';
    },

    levelIcon(level) {
      var map = { mission: '\u{1F3AF}', strategy: '\u{1F9ED}', objective: '\u{1F4CC}', task: '\u{2705}' };
      return map[level] || '\u{1F4CB}';
    },

    goalTitle(id) {
      if (!id) return '(none)';
      for (var i = 0; i < this.goals.length; i++) {
        if (this.goals[i].id === id) return this.goals[i].title;
      }
      return id.substring(0, 8) + '...';
    },

    get availableAgents() {
      return Alpine.store('app').agents || [];
    },

    agentName(agentId) {
      if (!agentId) return '(unassigned)';
      var agents = this.availableAgents;
      for (var i = 0; i < agents.length; i++) {
        if (agents[i].id === agentId) return agents[i].name;
      }
      return agentId.substring(0, 8) + '...';
    },

    relativeTime(ts) {
      if (!ts) return '';
      try {
        var diff = Date.now() - new Date(ts).getTime();
        if (isNaN(diff)) return '';
        if (diff < 60000) return 'just now';
        if (diff < 3600000) return Math.floor(diff / 60000) + 'm ago';
        if (diff < 86400000) return Math.floor(diff / 3600000) + 'h ago';
        return Math.floor(diff / 86400000) + 'd ago';
      } catch(e) { return ''; }
    },

    // Stats
    completedCount() { return this.goals.filter(function(g) { return g.status === 'completed'; }).length; },
    activeCount() { return this.goals.filter(function(g) { return g.status === 'active'; }).length; },
    avgProgress() {
      if (!this.goals.length) return 0;
      var sum = 0;
      for (var i = 0; i < this.goals.length; i++) sum += this.goals[i].progress;
      return Math.round(sum / this.goals.length);
    }
  };
}
```

**Step 2: Commit**

```bash
git add crates/openfang-api/static/js/pages/goals.js
git commit -m "feat: add Goals page Alpine.js component"
```

---

### Task 5: Add Goals HTML Template and Wire Navigation

**Files:**
- Modify: `crates/openfang-api/static/index_body.html`
- Modify: `crates/openfang-api/static/js/app.js`
- Modify: `crates/openfang-api/src/webchat.rs`

**Step 1: Add nav item in `index_body.html`**

In the "Automation" nav section (around line 108-125), add a Goals nav item after the Scheduler link. Find the closing `</div>` of the Scheduler link and add before `</div></template></div>`:

After line 122 (the `</a>` closing the Scheduler nav item), insert:

```html
            <a class="nav-item" :class="{ active: page === 'goals' }" @click="navigate('goals')" :aria-current="page === 'goals' ? 'page' : false">
              <span class="nav-icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="m9 12 2 2 4-4"/></svg></span>
              <span class="nav-label">Goals</span>
            </a>
```

**Step 2: Add page template in `index_body.html`**

Find the line `<!-- Page: Channels -->` (line ~1785) and insert the Goals page template BEFORE it:

```html
    <!-- Page: Goals -->
    <template x-if="page === 'goals'">
      <div x-data="goalsPage()">
        <div class="page-header">
          <h2>Goals <span class="badge badge-dim" x-show="goals.length" x-text="completedCount() + '/' + goals.length + ' completed'" style="margin-left:4px"></span></h2>
          <button class="btn btn-primary btn-sm" @click="showCreateForm = true">+ New Goal</button>
        </div>
        <div class="tabs" role="tablist">
          <div class="tab" role="tab" :class="{ active: tab === 'tree' }" @click="tab = 'tree'">Goal Tree</div>
          <div class="tab" role="tab" :class="{ active: tab === 'board' }" @click="tab = 'board'">Board</div>
          <div class="tab" role="tab" :class="{ active: tab === 'timeline' }" @click="tab = 'timeline'">Timeline</div>
        </div>
        <div class="page-body" x-init="loadData()">
          <div x-show="loading" class="loading-state"><div class="spinner"></div><span>Loading goals...</span></div>
          <div x-show="!loading && loadError" class="error-state">
            <span class="error-icon">!</span>
            <p x-text="loadError"></p>
            <button class="btn btn-ghost btn-sm" @click="loadData()">Retry</button>
          </div>
          <div x-show="!loading && !loadError">

            <!-- Stats Row -->
            <div class="grid grid-cols-4" style="gap:12px;margin-bottom:20px" x-show="goals.length">
              <div class="card stat-card"><div class="stat-label">Total</div><div class="stat-value" x-text="goals.length"></div></div>
              <div class="card stat-card"><div class="stat-label">Active</div><div class="stat-value" x-text="activeCount()"></div></div>
              <div class="card stat-card"><div class="stat-label">Completed</div><div class="stat-value" x-text="completedCount()"></div></div>
              <div class="card stat-card"><div class="stat-label">Avg Progress</div><div class="stat-value" x-text="avgProgress() + '%'"></div></div>
            </div>

            <!-- TAB: Tree View -->
            <div x-show="tab === 'tree'">
              <div class="table-wrap" x-show="goals.length">
                <table>
                  <thead><tr><th>Goal</th><th>Level</th><th>Status</th><th>Owner</th><th>Progress</th><th>Actions</th></tr></thead>
                  <tbody>
                    <template x-for="item in treeOrder()" :key="item.goal.id">
                      <tr>
                        <td>
                          <div :style="'padding-left:' + (item.depth * 24) + 'px'" class="flex items-center gap-2">
                            <span x-text="levelIcon(item.goal.level)"></span>
                            <span class="font-bold" x-text="item.goal.title"></span>
                          </div>
                          <div :style="'padding-left:' + (item.depth * 24 + 28) + 'px'" class="text-xs text-dim" x-show="item.goal.description" x-text="(item.goal.description || '').substring(0, 80)"></div>
                        </td>
                        <td><span class="badge" :class="levelBadgeClass(item.goal.level)" x-text="item.goal.level"></span></td>
                        <td><span class="badge" :class="statusBadgeClass(item.goal.status)" x-text="item.goal.status"></span></td>
                        <td class="text-xs" x-text="agentName(item.goal.owner_agent_id)"></td>
                        <td>
                          <div style="display:flex;align-items:center;gap:6px">
                            <div style="width:60px;height:6px;background:var(--border);border-radius:3px;overflow:hidden">
                              <div :style="'width:' + item.goal.progress + '%;height:100%;background:var(--accent);border-radius:3px;transition:width 0.3s'"></div>
                            </div>
                            <span class="text-xs" x-text="item.goal.progress + '%'"></span>
                          </div>
                        </td>
                        <td>
                          <div class="flex gap-1">
                            <button class="btn btn-ghost btn-sm" @click="openEdit(item.goal)">Edit</button>
                            <button class="btn btn-danger btn-sm" @click="deleteGoal(item.goal)">Del</button>
                          </div>
                        </td>
                      </tr>
                    </template>
                  </tbody>
                </table>
              </div>
              <div class="empty-state" x-show="!goals.length">
                <div class="empty-state-icon">
                  <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="m9 12 2 2 4-4"/></svg>
                </div>
                <h3>No goals yet</h3>
                <p>Create your first goal to start tracking what your agents are working toward.</p>
                <button class="btn btn-primary mt-4" @click="showCreateForm = true">+ Create Goal</button>
              </div>
            </div>

            <!-- TAB: Board (Kanban) -->
            <div x-show="tab === 'board'">
              <div style="display:grid;grid-template-columns:repeat(4,1fr);gap:12px">
                <template x-for="status in ['planned','active','completed','paused']" :key="status">
                  <div>
                    <div style="font-weight:600;font-size:13px;text-transform:capitalize;margin-bottom:8px;display:flex;align-items:center;gap:6px">
                      <span class="badge" :class="statusBadgeClass(status)" x-text="status"></span>
                      <span class="text-xs text-dim" x-text="goalsByStatus(status).length"></span>
                    </div>
                    <div style="display:flex;flex-direction:column;gap:8px">
                      <template x-for="goal in goalsByStatus(status)" :key="goal.id">
                        <div class="card" style="padding:10px;cursor:pointer" @click="openEdit(goal)">
                          <div class="flex items-center gap-2 mb-1">
                            <span x-text="levelIcon(goal.level)" style="font-size:0.9rem"></span>
                            <span class="font-bold text-sm" x-text="goal.title"></span>
                          </div>
                          <div class="text-xs text-dim" x-show="goal.description" x-text="(goal.description || '').substring(0, 60)"></div>
                          <div style="display:flex;align-items:center;gap:6px;margin-top:6px">
                            <div style="flex:1;height:4px;background:var(--border);border-radius:2px;overflow:hidden">
                              <div :style="'width:' + goal.progress + '%;height:100%;background:var(--accent);border-radius:2px'"></div>
                            </div>
                            <span class="text-xs text-dim" x-text="goal.progress + '%'"></span>
                          </div>
                          <div class="text-xs text-dim" style="margin-top:4px" x-text="agentName(goal.owner_agent_id)"></div>
                        </div>
                      </template>
                      <div class="text-xs text-dim" style="text-align:center;padding:12px" x-show="!goalsByStatus(status).length">No goals</div>
                    </div>
                  </div>
                </template>
              </div>
            </div>

            <!-- TAB: Timeline -->
            <div x-show="tab === 'timeline'">
              <div class="table-wrap" x-show="goals.length">
                <table>
                  <thead><tr><th>Updated</th><th>Goal</th><th>Level</th><th>Status</th><th>Progress</th></tr></thead>
                  <tbody>
                    <template x-for="goal in [...goals].sort((a,b) => new Date(b.updated_at) - new Date(a.updated_at))" :key="goal.id">
                      <tr style="cursor:pointer" @click="openEdit(goal)">
                        <td class="text-xs" style="white-space:nowrap" x-text="relativeTime(goal.updated_at)" :title="new Date(goal.updated_at).toLocaleString()"></td>
                        <td>
                          <span class="font-bold" x-text="goal.title"></span>
                          <div class="text-xs text-dim" x-show="goal.parent_id" x-text="'\u2514 ' + goalTitle(goal.parent_id)"></div>
                        </td>
                        <td><span class="badge" :class="levelBadgeClass(goal.level)" x-text="goal.level"></span></td>
                        <td><span class="badge" :class="statusBadgeClass(goal.status)" x-text="goal.status"></span></td>
                        <td>
                          <div style="display:flex;align-items:center;gap:6px">
                            <div style="width:60px;height:6px;background:var(--border);border-radius:3px;overflow:hidden">
                              <div :style="'width:' + goal.progress + '%;height:100%;background:var(--accent);border-radius:3px'"></div>
                            </div>
                            <span class="text-xs" x-text="goal.progress + '%'"></span>
                          </div>
                        </td>
                      </tr>
                    </template>
                  </tbody>
                </table>
              </div>
              <div class="empty-state" x-show="!goals.length">
                <h4>No goals yet</h4>
                <p class="hint">Create goals to track what your agents are working toward.</p>
              </div>
            </div>

            <!-- Create Goal Modal -->
            <template x-if="showCreateForm">
              <div class="modal-overlay" @click.self="showCreateForm = false" @keydown.escape.window="showCreateForm = false">
                <div class="modal">
                  <div class="modal-header">
                    <h3>Create Goal</h3>
                    <button class="modal-close" @click="showCreateForm = false">&times;</button>
                  </div>
                  <div class="form-group">
                    <label>Title</label>
                    <input class="form-input" x-model="newGoal.title" placeholder="e.g. Ship v1.0 to production">
                  </div>
                  <div class="form-group">
                    <label>Description</label>
                    <textarea class="form-textarea" x-model="newGoal.description" placeholder="What does achieving this goal look like?" rows="2"></textarea>
                  </div>
                  <div style="display:grid;grid-template-columns:1fr 1fr;gap:12px">
                    <div class="form-group">
                      <label>Level</label>
                      <select class="form-select" x-model="newGoal.level">
                        <option value="mission">Mission</option>
                        <option value="strategy">Strategy</option>
                        <option value="objective">Objective</option>
                        <option value="task">Task</option>
                      </select>
                    </div>
                    <div class="form-group">
                      <label>Status</label>
                      <select class="form-select" x-model="newGoal.status">
                        <option value="planned">Planned</option>
                        <option value="active">Active</option>
                      </select>
                    </div>
                  </div>
                  <div class="form-group">
                    <label>Parent Goal</label>
                    <select class="form-select" x-model="newGoal.parent_id">
                      <option value="">(none - root goal)</option>
                      <template x-for="g in goals" :key="g.id">
                        <option :value="g.id" x-text="levelIcon(g.level) + ' ' + g.title"></option>
                      </template>
                    </select>
                  </div>
                  <div class="form-group">
                    <label>Assign to Agent</label>
                    <select class="form-select" x-model="newGoal.owner_agent_id">
                      <option value="">(unassigned)</option>
                      <template x-for="a in availableAgents" :key="a.id">
                        <option :value="a.id" x-text="a.name"></option>
                      </template>
                    </select>
                  </div>
                  <div class="form-group">
                    <label>Progress: <span x-text="newGoal.progress + '%'"></span></label>
                    <input type="range" min="0" max="100" step="5" x-model="newGoal.progress" style="width:100%">
                  </div>
                  <button class="btn btn-primary btn-block mt-4" @click="createGoal()" :disabled="creating">
                    <span x-show="!creating">Create Goal</span>
                    <span x-show="creating">Creating...</span>
                  </button>
                </div>
              </div>
            </template>

            <!-- Edit Goal Modal -->
            <template x-if="editGoal">
              <div class="modal-overlay" @click.self="editGoal = null" @keydown.escape.window="editGoal = null">
                <div class="modal">
                  <div class="modal-header">
                    <h3>Edit Goal</h3>
                    <button class="modal-close" @click="editGoal = null">&times;</button>
                  </div>
                  <div class="form-group">
                    <label>Title</label>
                    <input class="form-input" x-model="editForm.title">
                  </div>
                  <div class="form-group">
                    <label>Description</label>
                    <textarea class="form-textarea" x-model="editForm.description" rows="2"></textarea>
                  </div>
                  <div style="display:grid;grid-template-columns:1fr 1fr;gap:12px">
                    <div class="form-group">
                      <label>Level</label>
                      <select class="form-select" x-model="editForm.level">
                        <option value="mission">Mission</option>
                        <option value="strategy">Strategy</option>
                        <option value="objective">Objective</option>
                        <option value="task">Task</option>
                      </select>
                    </div>
                    <div class="form-group">
                      <label>Status</label>
                      <select class="form-select" x-model="editForm.status">
                        <option value="planned">Planned</option>
                        <option value="active">Active</option>
                        <option value="completed">Completed</option>
                        <option value="paused">Paused</option>
                      </select>
                    </div>
                  </div>
                  <div class="form-group">
                    <label>Parent Goal</label>
                    <select class="form-select" x-model="editForm.parent_id">
                      <option value="">(none - root goal)</option>
                      <template x-for="g in goals.filter(g => g.id !== editGoal.id)" :key="g.id">
                        <option :value="g.id" x-text="levelIcon(g.level) + ' ' + g.title"></option>
                      </template>
                    </select>
                  </div>
                  <div class="form-group">
                    <label>Assign to Agent</label>
                    <select class="form-select" x-model="editForm.owner_agent_id">
                      <option value="">(unassigned)</option>
                      <template x-for="a in availableAgents" :key="a.id">
                        <option :value="a.id" x-text="a.name"></option>
                      </template>
                    </select>
                  </div>
                  <div class="form-group">
                    <label>Progress: <span x-text="editForm.progress + '%'"></span></label>
                    <input type="range" min="0" max="100" step="5" x-model="editForm.progress" style="width:100%">
                  </div>
                  <div class="flex gap-2 mt-4">
                    <button class="btn btn-primary" style="flex:1" @click="saveEdit()" :disabled="saving">
                      <span x-show="!saving">Save Changes</span>
                      <span x-show="saving">Saving...</span>
                    </button>
                    <button class="btn btn-danger" @click="var g = editGoal; editGoal = null; deleteGoal(g)">Delete</button>
                  </div>
                </div>
              </div>
            </template>

          </div>
        </div>
      </div>
    </template>
```

**Step 3: Register 'goals' in valid pages in `app.js`**

In `crates/openfang-api/static/js/app.js` line 221, add `'goals'` to the `validPages` array:

Change:
```javascript
var validPages = ['overview','agents','sessions','approvals','comms','workflows','scheduler','channels','skills','hands','analytics','logs','runtime','settings','wizard'];
```
To:
```javascript
var validPages = ['overview','agents','sessions','approvals','comms','workflows','scheduler','goals','channels','skills','hands','analytics','logs','runtime','settings','wizard'];
```

**Step 4: Add goals.js include in `webchat.rs`**

In `crates/openfang-api/src/webchat.rs`, after the `hands.js` include (line 111-112), add:

```rust
    "\n",
    include_str!("../static/js/pages/goals.js"),
```

**Step 5: Build and verify**

Run: `cargo build --workspace --lib`
Expected: Compiles

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: Zero warnings

**Step 6: Commit**

```bash
git add crates/openfang-api/static/index_body.html crates/openfang-api/static/js/app.js crates/openfang-api/src/webchat.rs crates/openfang-api/static/js/pages/goals.js
git commit -m "feat: add Goals dashboard page with tree, board, and timeline views"
```

---

### Task 6: Run Full Test Suite and Clippy

**Step 1: Run full workspace tests**

Run: `cargo test --workspace`
Expected: All 1767+ tests PASS

**Step 2: Run clippy**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: Zero warnings

**Step 3: Fix any issues found**

If there are compilation errors or test failures, fix them before proceeding.

**Step 4: Final commit if any fixes were needed**

```bash
git add -A
git commit -m "fix: resolve any issues from full test suite"
```

---

## Summary of Changes

| File | Action | Description |
|------|--------|-------------|
| `crates/openfang-memory/src/migration.rs` | Modify | Add v8 migration with `goals` table |
| `crates/openfang-memory/src/goals.rs` | Create | GoalStore with full CRUD + tests |
| `crates/openfang-memory/src/lib.rs` | Modify | Register `goals` module |
| `crates/openfang-memory/src/substrate.rs` | Modify | Add GoalStore field + `goals()` accessor |
| `crates/openfang-api/src/routes.rs` | Modify | 5 goal route handlers |
| `crates/openfang-api/src/server.rs` | Modify | Register `/api/goals` routes |
| `crates/openfang-api/static/js/pages/goals.js` | Create | Alpine.js page component |
| `crates/openfang-api/static/index_body.html` | Modify | Nav item + page template |
| `crates/openfang-api/static/js/app.js` | Modify | Add `goals` to validPages |
| `crates/openfang-api/src/webchat.rs` | Modify | Include goals.js in embedded HTML |
