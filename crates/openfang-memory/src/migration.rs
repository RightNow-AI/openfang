//! SQLite schema creation and migration.
//!
//! Creates all tables needed by the memory substrate on first boot.

use chrono::Utc;
use rusqlite::{params, Connection};

/// Current schema version.
const SCHEMA_VERSION: u32 = 15;

/// Run all migrations to bring the database up to date.
pub fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
    let current_version = get_schema_version(conn);

    if current_version < 1 {
        migrate_v1(conn)?;
    }

    if current_version < 2 {
        migrate_v2(conn)?;
    }

    if current_version < 3 {
        migrate_v3(conn)?;
    }

    if current_version < 4 {
        migrate_v4(conn)?;
    }

    if current_version < 5 {
        migrate_v5(conn)?;
    }

    if current_version < 6 {
        migrate_v6(conn)?;
    }

    if current_version < 7 {
        migrate_v7(conn)?;
    }

    if current_version < 8 {
        migrate_v8(conn)?;
    }

    if current_version < 9 {
        migrate_v9(conn)?;
    }

    if current_version < 10 {
        migrate_v10(conn)?;
    }

    if current_version < 11 {
        migrate_v11(conn)?;
    }

    if current_version < 12 {
        migrate_v12(conn)?;
    }

    if current_version < 13 {
        migrate_v13(conn)?;
    }

    if current_version < 14 {
        migrate_v14(conn)?;
    }

    if current_version < 15 {
        migrate_v15(conn)?;
    }

    set_schema_version(conn, SCHEMA_VERSION)?;
    Ok(())
}

/// Get the current schema version from the database.
fn get_schema_version(conn: &Connection) -> u32 {
    conn.pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap_or(0)
}

/// Check if a column exists in a table (SQLite has no ADD COLUMN IF NOT EXISTS).
fn column_exists(conn: &Connection, table: &str, column: &str) -> bool {
    let sql = format!("PRAGMA table_info({})", table);
    let Ok(mut stmt) = conn.prepare(&sql) else {
        return false;
    };
    let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(1)) else {
        return false;
    };
    let names: Vec<String> = rows.filter_map(|r| r.ok()).collect();
    names.iter().any(|n| n == column)
}

/// Set the schema version in the database.
fn set_schema_version(conn: &Connection, version: u32) -> Result<(), rusqlite::Error> {
    conn.pragma_update(None, "user_version", version)
}

/// Version 1: Create all core tables.
fn migrate_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        -- Agent registry
        CREATE TABLE IF NOT EXISTS agents (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            manifest BLOB NOT NULL,
            state TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        -- Session history
        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            agent_id TEXT NOT NULL,
            messages BLOB NOT NULL,
            context_window_tokens INTEGER DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        -- Event log
        CREATE TABLE IF NOT EXISTS events (
            id TEXT PRIMARY KEY,
            source_agent TEXT NOT NULL,
            target TEXT NOT NULL,
            payload BLOB NOT NULL,
            timestamp TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);
        CREATE INDEX IF NOT EXISTS idx_events_source ON events(source_agent);

        -- Key-value store (per-agent)
        CREATE TABLE IF NOT EXISTS kv_store (
            agent_id TEXT NOT NULL,
            key TEXT NOT NULL,
            value BLOB NOT NULL,
            version INTEGER NOT NULL DEFAULT 1,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (agent_id, key)
        );

        -- Task queue
        CREATE TABLE IF NOT EXISTS task_queue (
            id TEXT PRIMARY KEY,
            agent_id TEXT NOT NULL,
            task_type TEXT NOT NULL,
            payload BLOB NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            priority INTEGER NOT NULL DEFAULT 0,
            scheduled_at TEXT,
            created_at TEXT NOT NULL,
            completed_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_task_status_priority ON task_queue(status, priority DESC);

        -- Semantic memories
        CREATE TABLE IF NOT EXISTS memories (
            id TEXT PRIMARY KEY,
            agent_id TEXT NOT NULL,
            content TEXT NOT NULL,
            source TEXT NOT NULL,
            scope TEXT NOT NULL DEFAULT 'episodic',
            confidence REAL NOT NULL DEFAULT 1.0,
            metadata TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL,
            accessed_at TEXT NOT NULL,
            access_count INTEGER NOT NULL DEFAULT 0,
            deleted INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_memories_agent ON memories(agent_id);
        CREATE INDEX IF NOT EXISTS idx_memories_scope ON memories(scope);

        -- Knowledge graph entities
        CREATE TABLE IF NOT EXISTS entities (
            id TEXT PRIMARY KEY,
            entity_type TEXT NOT NULL,
            name TEXT NOT NULL,
            properties TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        -- Knowledge graph relations
        CREATE TABLE IF NOT EXISTS relations (
            id TEXT PRIMARY KEY,
            source_entity TEXT NOT NULL,
            relation_type TEXT NOT NULL,
            target_entity TEXT NOT NULL,
            properties TEXT NOT NULL DEFAULT '{}',
            confidence REAL NOT NULL DEFAULT 1.0,
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_relations_source ON relations(source_entity);
        CREATE INDEX IF NOT EXISTS idx_relations_target ON relations(target_entity);
        CREATE INDEX IF NOT EXISTS idx_relations_type ON relations(relation_type);

        -- Migration tracking
        CREATE TABLE IF NOT EXISTS migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL,
            description TEXT
        );

        INSERT OR IGNORE INTO migrations (version, applied_at, description)
        VALUES (1, datetime('now'), 'Initial schema');
        ",
    )?;
    Ok(())
}

/// Version 2: Add collaboration columns to task_queue for agent task delegation.
fn migrate_v2(conn: &Connection) -> Result<(), rusqlite::Error> {
    // SQLite requires one ALTER TABLE per statement; check before adding
    let cols = [
        ("title", "TEXT DEFAULT ''"),
        ("description", "TEXT DEFAULT ''"),
        ("assigned_to", "TEXT DEFAULT ''"),
        ("created_by", "TEXT DEFAULT ''"),
        ("result", "TEXT DEFAULT ''"),
    ];
    for (name, typedef) in &cols {
        if !column_exists(conn, "task_queue", name) {
            conn.execute(
                &format!("ALTER TABLE task_queue ADD COLUMN {} {}", name, typedef),
                [],
            )?;
        }
    }

    conn.execute(
        "INSERT OR IGNORE INTO migrations (version, applied_at, description) VALUES (2, datetime('now'), 'Add collaboration columns to task_queue')",
        [],
    )?;

    Ok(())
}

/// Version 3: Add embedding column to memories table for vector search.
fn migrate_v3(conn: &Connection) -> Result<(), rusqlite::Error> {
    if !column_exists(conn, "memories", "embedding") {
        conn.execute(
            "ALTER TABLE memories ADD COLUMN embedding BLOB DEFAULT NULL",
            [],
        )?;
    }
    conn.execute(
        "INSERT OR IGNORE INTO migrations (version, applied_at, description) VALUES (3, datetime('now'), 'Add embedding column to memories')",
        [],
    )?;
    Ok(())
}

/// Version 4: Add usage_events table for cost tracking and metering.
fn migrate_v4(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS usage_events (
            id TEXT PRIMARY KEY,
            agent_id TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            model TEXT NOT NULL,
            input_tokens INTEGER NOT NULL DEFAULT 0,
            output_tokens INTEGER NOT NULL DEFAULT 0,
            cost_usd REAL NOT NULL DEFAULT 0.0,
            tool_calls INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_usage_agent_time ON usage_events(agent_id, timestamp);
        CREATE INDEX IF NOT EXISTS idx_usage_timestamp ON usage_events(timestamp);

        INSERT OR IGNORE INTO migrations (version, applied_at, description)
        VALUES (4, datetime('now'), 'Add usage_events table for cost tracking');
        ",
    )?;
    Ok(())
}

/// Version 5: Add canonical_sessions table for cross-channel persistent memory.
fn migrate_v5(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS canonical_sessions (
            agent_id TEXT PRIMARY KEY,
            messages BLOB NOT NULL,
            compaction_cursor INTEGER NOT NULL DEFAULT 0,
            compacted_summary TEXT,
            updated_at TEXT NOT NULL
        );

        INSERT OR IGNORE INTO migrations (version, applied_at, description)
        VALUES (5, datetime('now'), 'Add canonical_sessions for cross-channel memory');
        ",
    )?;
    Ok(())
}

/// Version 6: Add label column to sessions table.
fn migrate_v6(conn: &Connection) -> Result<(), rusqlite::Error> {
    // Check if column already exists before ALTER (SQLite has no ADD COLUMN IF NOT EXISTS)
    if !column_exists(conn, "sessions", "label") {
        conn.execute("ALTER TABLE sessions ADD COLUMN label TEXT", [])?;
    }
    conn.execute(
        "INSERT OR IGNORE INTO migrations (version, applied_at, description) VALUES (6, datetime('now'), 'Add label column to sessions for human-readable labels')",
        [],
    )?;
    Ok(())
}

/// Version 7: Add paired_devices table for device pairing persistence.
fn migrate_v7(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS paired_devices (
            device_id TEXT PRIMARY KEY,
            display_name TEXT NOT NULL,
            platform TEXT NOT NULL,
            paired_at TEXT NOT NULL,
            last_seen TEXT NOT NULL,
            push_token TEXT
        );

        INSERT OR IGNORE INTO migrations (version, applied_at, description)
        VALUES (7, datetime('now'), 'Add paired_devices table for device pairing');
        ",
    )?;
    Ok(())
}

/// Version 8: Add audit_entries table for persistent Merkle audit trail.
fn migrate_v8(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS audit_entries (
            seq INTEGER PRIMARY KEY,
            timestamp TEXT NOT NULL,
            agent_id TEXT NOT NULL,
            action TEXT NOT NULL,
            detail TEXT NOT NULL,
            outcome TEXT NOT NULL,
            prev_hash TEXT NOT NULL,
            hash TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_audit_agent ON audit_entries(agent_id);
        CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_entries(timestamp);
        CREATE INDEX IF NOT EXISTS idx_audit_action ON audit_entries(action);

        INSERT OR IGNORE INTO migrations (version, applied_at, description)
        VALUES (8, datetime('now'), 'Add audit_entries table for persistent Merkle audit trail');
        ",
    )?;
    Ok(())
}

/// Version 9: Add planner tables for the Personal Chief of Staff slice.
fn migrate_v9(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS planner_inbox_items (
            id TEXT PRIMARY KEY,
            text TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at TEXT NOT NULL,
            clarified_at TEXT,
            task_id TEXT,
            project_id TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_planner_inbox_created ON planner_inbox_items(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_planner_inbox_status ON planner_inbox_items(status);

        CREATE TABLE IF NOT EXISTS planner_projects (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            outcome TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS planner_tasks (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            status TEXT NOT NULL,
            priority TEXT NOT NULL,
            effort_minutes INTEGER,
            energy TEXT NOT NULL,
            project_id TEXT,
            due_at TEXT,
            scheduled_for TEXT,
            blocked_by TEXT NOT NULL DEFAULT '[]',
            next_action TEXT NOT NULL,
            source_inbox_item_id TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_planner_tasks_status ON planner_tasks(status);
        CREATE INDEX IF NOT EXISTS idx_planner_tasks_schedule ON planner_tasks(scheduled_for);

        CREATE TABLE IF NOT EXISTS planner_today_plans (
            date TEXT PRIMARY KEY,
            payload TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS planner_routines (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            trigger TEXT NOT NULL,
            thread_label TEXT NOT NULL,
            active INTEGER NOT NULL DEFAULT 1,
            last_run_at TEXT,
            next_run_at TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS planner_reviews (
            id TEXT PRIMARY KEY,
            scope TEXT NOT NULL,
            created_at TEXT NOT NULL,
            summary TEXT NOT NULL,
            wins TEXT NOT NULL DEFAULT '[]',
            misses TEXT NOT NULL DEFAULT '[]',
            adjustments TEXT NOT NULL DEFAULT '[]'
        );

        INSERT OR IGNORE INTO migrations (version, applied_at, description)
        VALUES (9, datetime('now'), 'Add planner tables for Personal Chief of Staff');
        ",
    )?;
    Ok(())
}

/// Version 10: Add planner agent preference persistence.
fn migrate_v10(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS planner_agent_preferences (
            agent_id TEXT PRIMARY KEY,
            enabled INTEGER NOT NULL DEFAULT 1,
            updated_at TEXT NOT NULL
        );

        INSERT OR IGNORE INTO migrations (version, applied_at, description)
        VALUES (10, datetime('now'), 'Add planner agent preference persistence');
        ",
    )?;
    Ok(())
}

/// Version 11: Strip persisted planner recommendation fields from saved today plans.
fn migrate_v11(conn: &Connection) -> Result<(), rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT date, payload FROM planner_today_plans")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut updates = Vec::new();
    for row in rows {
        let (date, payload) = row?;
        if let Some(cleaned_payload) = strip_persisted_planner_recommendations(&payload) {
            updates.push((date, cleaned_payload));
        }
    }

    for (date, payload) in updates {
        conn.execute(
            "UPDATE planner_today_plans SET payload = ?2, updated_at = ?3 WHERE date = ?1",
            params![date, payload, Utc::now().to_rfc3339()],
        )?;
    }

    conn.execute(
        "INSERT OR IGNORE INTO migrations (version, applied_at, description) VALUES (11, datetime('now'), 'Strip persisted planner recommendation fields from today plans')",
        [],
    )?;
    Ok(())
}

/// Version 12: Add agency profile persistence for imported profile catalog entries.
fn migrate_v12(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS agent_profiles (
            id TEXT PRIMARY KEY,
            source_path TEXT NOT NULL,
            payload TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_agent_profiles_updated ON agent_profiles(updated_at DESC);

        INSERT OR IGNORE INTO migrations (version, applied_at, description)
        VALUES (12, datetime('now'), 'Add agency profile persistence for imported profiles');
        ",
    )?;
    Ok(())
}

/// Version 13: Add persisted OAuth users and revocable JWT-backed sessions.
fn migrate_v13(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS auth_users (
            id TEXT PRIMARY KEY,
            provider TEXT NOT NULL,
            provider_user_id TEXT NOT NULL,
            login TEXT,
            name TEXT,
            email TEXT,
            avatar_url TEXT,
            role TEXT NOT NULL DEFAULT 'user',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            last_login_at TEXT NOT NULL
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_auth_users_provider_subject
            ON auth_users(provider, provider_user_id);

        CREATE TABLE IF NOT EXISTS auth_sessions (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            provider TEXT NOT NULL,
            subject TEXT NOT NULL,
            issued_at TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            revoked_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_auth_sessions_user_id ON auth_sessions(user_id);
        CREATE INDEX IF NOT EXISTS idx_auth_sessions_expiry ON auth_sessions(expires_at);

        INSERT OR IGNORE INTO migrations (version, applied_at, description)
        VALUES (13, datetime('now'), 'Add persisted OAuth users and revocable auth sessions');
        ",
    )?;
    Ok(())
}

/// Version 14: Work items — canonical unit-of-work foundation.
fn migrate_v14(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS work_items (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            work_type TEXT NOT NULL DEFAULT 'agent_task',
            source TEXT NOT NULL DEFAULT 'api',
            status TEXT NOT NULL DEFAULT 'pending',
            approval_status TEXT NOT NULL DEFAULT 'not_required',
            assigned_agent_id TEXT,
            assigned_agent_name TEXT,
            result TEXT,
            error TEXT,
            iterations INTEGER NOT NULL DEFAULT 0,
            priority INTEGER NOT NULL DEFAULT 128,
            scheduled_at TEXT,
            started_at TEXT,
            completed_at TEXT,
            deadline TEXT,
            requires_approval INTEGER NOT NULL DEFAULT 0,
            approved_by TEXT,
            approved_at TEXT,
            approval_note TEXT,
            payload TEXT NOT NULL DEFAULT '{}',
            tags TEXT NOT NULL DEFAULT '[]',
            created_by TEXT,
            idempotency_key TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            retry_count INTEGER NOT NULL DEFAULT 0,
            max_retries INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_work_items_status ON work_items(status);
        CREATE INDEX IF NOT EXISTS idx_work_items_agent ON work_items(assigned_agent_id);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_work_items_idempotency
            ON work_items(idempotency_key)
            WHERE idempotency_key IS NOT NULL;

        CREATE TABLE IF NOT EXISTS work_events (
            id TEXT PRIMARY KEY,
            work_item_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            from_status TEXT,
            to_status TEXT,
            actor TEXT,
            detail TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY (work_item_id) REFERENCES work_items(id)
        );
        CREATE INDEX IF NOT EXISTS idx_work_events_item ON work_events(work_item_id);

        CREATE TABLE IF NOT EXISTS approval_records (
            id TEXT PRIMARY KEY,
            work_item_id TEXT NOT NULL,
            decision TEXT NOT NULL,
            actor TEXT NOT NULL,
            note TEXT,
            decided_at TEXT NOT NULL,
            FOREIGN KEY (work_item_id) REFERENCES work_items(id)
        );
        CREATE INDEX IF NOT EXISTS idx_approval_records_item ON approval_records(work_item_id);

        INSERT OR IGNORE INTO migrations (version, applied_at, description)
        VALUES (14, datetime('now'), 'Add work_items, work_events, approval_records tables');
        ",
    )?;
    Ok(())
}

/// Version 15: Add parent_id to work_items for subagent delegation chains.
fn migrate_v15(conn: &Connection) -> Result<(), rusqlite::Error> {
    if !column_exists(conn, "work_items", "parent_id") {
        conn.execute_batch("ALTER TABLE work_items ADD COLUMN parent_id TEXT;")?;
    }
    conn.execute_batch(
        "
        CREATE INDEX IF NOT EXISTS idx_work_items_parent
            ON work_items(parent_id)
            WHERE parent_id IS NOT NULL;
        INSERT OR IGNORE INTO migrations (version, applied_at, description)
        VALUES (15, datetime('now'), 'Add parent_id to work_items for subagent delegation');
        ",
    )?;
    Ok(())
}

fn strip_persisted_planner_recommendations(payload: &str) -> Option<String> {
    let mut value = serde_json::from_str::<serde_json::Value>(payload).ok()?;
    if !remove_agent_recommendations(&mut value) {
        return None;
    }
    serde_json::to_string(&value).ok()
}

fn remove_agent_recommendations(value: &mut serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            let mut changed = map.remove("agent_recommendation").is_some();
            for child in map.values_mut() {
                changed |= remove_agent_recommendations(child);
            }
            changed
        }
        serde_json::Value::Array(items) => items
            .iter_mut()
            .fold(false, |changed, child| changed | remove_agent_recommendations(child)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    #[test]
    fn test_migration_creates_tables() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        // Verify tables exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert!(tables.contains(&"agents".to_string()));
        assert!(tables.contains(&"sessions".to_string()));
        assert!(tables.contains(&"kv_store".to_string()));
        assert!(tables.contains(&"memories".to_string()));
        assert!(tables.contains(&"entities".to_string()));
        assert!(tables.contains(&"relations".to_string()));
        assert!(tables.contains(&"agent_profiles".to_string()));
        assert!(tables.contains(&"auth_users".to_string()));
        assert!(tables.contains(&"auth_sessions".to_string()));
    }

    #[test]
    fn test_migration_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap(); // Should not error
    }

    #[test]
    fn test_migration_strips_persisted_planner_recommendations() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let payload = serde_json::json!({
            "date": "2026-03-10",
            "daily_outcome": "Ship auth update",
            "must_do": [{
                "id": "task-1",
                "title": "Need a security review of auth flow",
                "status": "todo",
                "priority": "high",
                "effort_minutes": 30,
                "energy": "high",
                "project_id": null,
                "due_at": null,
                "scheduled_for": null,
                "blocked_by": [],
                "next_action": "Need a security review of auth flow",
                "source_inbox_item_id": "inbox-1",
                "agent_recommendation": {
                    "agent_id": "assistant",
                    "name": "Assistant",
                    "reason": "Legacy fallback",
                    "confidence": "low"
                },
                "created_at": "2026-03-10T09:00:00Z",
                "updated_at": "2026-03-10T09:00:00Z"
            }],
            "should_do": [],
            "could_do": [],
            "blockers": [],
            "focus_suggestion": null,
            "rebuilt_at": "2026-03-10T09:00:00Z"
        })
        .to_string();

        conn.execute(
            "INSERT INTO planner_today_plans (date, payload, updated_at) VALUES (?1, ?2, ?3)",
            params!["2026-03-10", payload, "2026-03-10T09:00:00Z"],
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 10).unwrap();

        run_migrations(&conn).unwrap();

        let updated_payload: String = conn
            .query_row(
                "SELECT payload FROM planner_today_plans WHERE date = ?1",
                params!["2026-03-10"],
                |row| row.get(0),
            )
            .unwrap();

        assert!(!updated_payload.contains("assistant"));
        assert!(!updated_payload.contains("agent_recommendation"));
    }
}
