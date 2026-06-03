//! SQLite schema creation and migration.
//!
//! Creates all tables needed by the memory substrate on first boot.

use rusqlite::Connection;

/// Current schema version.
const SCHEMA_VERSION: u32 = 10;

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

/// Version 9: Tag every session with a user and a parent-session link.
///
/// - `user_id` (TEXT, NOT NULL, default = nil UUID) — owning user. Existing
///   v8 rows inherit the nil-UUID sentinel; the kernel's
///   `bootstrap_default_user` rewrite migrates them to the persistent
///   default user on first boot after upgrade.
/// - `parent_session_id` (TEXT, nullable) — set when a session is forked off
///   another (e.g. a hand session linked back to its caller). No production
///   code path forks sessions yet, so the column is unused on existing rows.
///
/// Indexes on `(agent_id, user_id)` and `parent_session_id` keep the per-user
/// lookups and child-session fan-out cheap.
fn migrate_v9(conn: &Connection) -> Result<(), rusqlite::Error> {
    if !column_exists(conn, "sessions", "user_id") {
        conn.execute(
            "ALTER TABLE sessions ADD COLUMN user_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000'",
            [],
        )?;
    }
    if !column_exists(conn, "sessions", "parent_session_id") {
        conn.execute("ALTER TABLE sessions ADD COLUMN parent_session_id TEXT", [])?;
    }
    conn.execute_batch(
        "
        CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(agent_id, user_id);
        CREATE INDEX IF NOT EXISTS idx_sessions_parent ON sessions(parent_session_id);

        INSERT OR IGNORE INTO migrations (version, applied_at, description)
        VALUES (9, datetime('now'), 'Tag sessions with user_id and parent_session_id');
        ",
    )?;
    Ok(())
}

/// Version 10: Structured-memory storage tables.
///
/// Adds three tables — `session_extractions`, `user_memory_topics`,
/// `user_agent_memory_topics` — that back the opt-in structured memory
/// system. None of these tables are populated by default agents: the
/// producer (`extract_structured` / dreamer) is gated on
/// `MemoryConfig::is_structured()` and lands in a later PR. The tables are
/// created unconditionally so the storage and control-API surface is
/// available the moment an agent opts in.
///
/// Schema choices:
///
/// - `session_extractions` carries denormalized `user_id` and `agent_id`
///   columns from the start. The audit endpoint filters by owning user
///   and surfaces a `session_deleted` flag derived from a LEFT JOIN
///   against `sessions`; keeping the ids on the row means audit
///   attribution survives a session delete instead of orphaning the row.
/// - `user_memory_topics` ships with `expires_at` (optional ISO timestamp;
///   expired rows are pruned at read time) and `embedding` (BLOB of packed
///   little-endian f32 values for optional cosine-similarity retrieval).
///   The producer populates `embedding` asynchronously, so the column
///   stays nullable.
/// - `user_agent_memory_topics` is the per-(user, agent) sibling of
///   `user_memory_topics`. Topics here are scoped to "how this user
///   interacts with this specific agent" and do not bleed across agents.
///
/// Indexes:
/// - `idx_extractions_session` for the per-session loader.
/// - `idx_extractions_user_created` for the audit endpoint's
///   `(user_id, created_at)` ordering.
/// - `idx_user_memory_user` for the per-user topic index.
/// - `idx_user_agent_memory` for the per-(user, agent) topic index.
fn migrate_v10(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS session_extractions (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            user_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
            agent_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
            facts TEXT NOT NULL DEFAULT '[]',
            preferences TEXT NOT NULL DEFAULT '[]',
            decisions TEXT NOT NULL DEFAULT '[]',
            tasks TEXT NOT NULL DEFAULT '[]',
            open_items TEXT NOT NULL DEFAULT '[]',
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_extractions_session
            ON session_extractions(session_id);
        CREATE INDEX IF NOT EXISTS idx_extractions_user_created
            ON session_extractions(user_id, created_at);

        CREATE TABLE IF NOT EXISTS user_memory_topics (
            user_id TEXT NOT NULL,
            topic TEXT NOT NULL,
            summary TEXT NOT NULL DEFAULT '',
            content TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL,
            expires_at TEXT DEFAULT NULL,
            embedding BLOB DEFAULT NULL,
            PRIMARY KEY (user_id, topic)
        );
        CREATE INDEX IF NOT EXISTS idx_user_memory_user
            ON user_memory_topics(user_id);

        CREATE TABLE IF NOT EXISTS user_agent_memory_topics (
            user_id TEXT NOT NULL,
            agent_id TEXT NOT NULL,
            topic TEXT NOT NULL,
            summary TEXT NOT NULL DEFAULT '',
            content TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL,
            PRIMARY KEY (user_id, agent_id, topic)
        );
        CREATE INDEX IF NOT EXISTS idx_user_agent_memory
            ON user_agent_memory_topics(user_id, agent_id);

        INSERT OR IGNORE INTO migrations (version, applied_at, description)
        VALUES (10, datetime('now'), 'Structured memory storage tables (session_extractions, user_memory_topics, user_agent_memory_topics)');
        ",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn test_migration_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap(); // Should not error
    }

    #[test]
    fn test_migration_v9_adds_session_user_columns() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        assert!(column_exists(&conn, "sessions", "user_id"));
        assert!(column_exists(&conn, "sessions", "parent_session_id"));
    }

    #[test]
    fn test_migration_v9_indexes_present() {
        // The (agent_id, user_id) and parent_session_id indexes underpin the
        // per-user and child-session lookups added in PR 1. If either is
        // missing, the queries fall back to full table scans.
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        let names: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='index'")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(names.contains(&"idx_sessions_user".to_string()));
        assert!(names.contains(&"idx_sessions_parent".to_string()));
    }

    /// A v8 database (sessions table without `user_id` or `parent_session_id`)
    /// must upgrade cleanly to v9: existing rows survive, the new columns are
    /// present, and the user_id default sentinel is the nil UUID.
    #[test]
    fn test_migration_v8_to_v9_upgrade_preserves_existing_rows() {
        let conn = Connection::open_in_memory().unwrap();

        // Build the v8 schema by stopping the migration runner one step early.
        migrate_v1(&conn).unwrap();
        migrate_v2(&conn).unwrap();
        migrate_v3(&conn).unwrap();
        migrate_v4(&conn).unwrap();
        migrate_v5(&conn).unwrap();
        migrate_v6(&conn).unwrap();
        migrate_v7(&conn).unwrap();
        migrate_v8(&conn).unwrap();
        set_schema_version(&conn, 8).unwrap();

        assert!(!column_exists(&conn, "sessions", "user_id"));
        assert!(!column_exists(&conn, "sessions", "parent_session_id"));

        // Insert a pre-v9 session row (no user_id, no parent_session_id).
        conn.execute(
            "INSERT INTO sessions (id, agent_id, messages, context_window_tokens, created_at, updated_at)
             VALUES ('sess-1', 'agent-1', X'', 0, datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();

        // Now run the full pipeline — v9 should apply on top of v8.
        run_migrations(&conn).unwrap();

        assert!(column_exists(&conn, "sessions", "user_id"));
        assert!(column_exists(&conn, "sessions", "parent_session_id"));

        // The pre-existing row survives and gets the nil-UUID default for
        // user_id, NULL for parent_session_id.
        let (uid, parent): (String, Option<String>) = conn
            .query_row(
                "SELECT user_id, parent_session_id FROM sessions WHERE id = 'sess-1'",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .unwrap();
        assert_eq!(uid, "00000000-0000-0000-0000-000000000000");
        assert!(parent.is_none());
    }

    // ── v10: Structured-memory storage ──────────────────────────────────

    #[test]
    fn test_migration_v10_creates_storage_tables() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(tables.contains(&"session_extractions".to_string()));
        assert!(tables.contains(&"user_memory_topics".to_string()));
        assert!(tables.contains(&"user_agent_memory_topics".to_string()));
    }

    #[test]
    fn test_migration_v10_session_extractions_columns_denormalized() {
        // user_id and agent_id are denormalized onto the row from the start —
        // no v15 backfill needed because PR 2 ships the table this way.
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        assert!(column_exists(&conn, "session_extractions", "user_id"));
        assert!(column_exists(&conn, "session_extractions", "agent_id"));
        assert!(column_exists(&conn, "session_extractions", "session_id"));
        assert!(column_exists(&conn, "session_extractions", "facts"));
        assert!(column_exists(&conn, "session_extractions", "preferences"));
        assert!(column_exists(&conn, "session_extractions", "decisions"));
        assert!(column_exists(&conn, "session_extractions", "tasks"));
        assert!(column_exists(&conn, "session_extractions", "open_items"));
        assert!(column_exists(&conn, "session_extractions", "created_at"));
    }

    #[test]
    fn test_migration_v10_user_memory_topics_columns() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        // expires_at and embedding are present from the start so prune-at-read
        // and similarity-search code paths compile and run.
        assert!(column_exists(&conn, "user_memory_topics", "user_id"));
        assert!(column_exists(&conn, "user_memory_topics", "topic"));
        assert!(column_exists(&conn, "user_memory_topics", "summary"));
        assert!(column_exists(&conn, "user_memory_topics", "content"));
        assert!(column_exists(&conn, "user_memory_topics", "updated_at"));
        assert!(column_exists(&conn, "user_memory_topics", "expires_at"));
        assert!(column_exists(&conn, "user_memory_topics", "embedding"));
    }

    #[test]
    fn test_migration_v10_user_agent_memory_topics_columns() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        assert!(column_exists(&conn, "user_agent_memory_topics", "user_id"));
        assert!(column_exists(&conn, "user_agent_memory_topics", "agent_id"));
        assert!(column_exists(&conn, "user_agent_memory_topics", "topic"));
        assert!(column_exists(&conn, "user_agent_memory_topics", "summary"));
        assert!(column_exists(&conn, "user_agent_memory_topics", "content"));
        assert!(column_exists(
            &conn,
            "user_agent_memory_topics",
            "updated_at"
        ));
    }

    #[test]
    fn test_migration_v10_indexes_present() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        let names: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='index'")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(names.contains(&"idx_extractions_session".to_string()));
        assert!(names.contains(&"idx_extractions_user_created".to_string()));
        assert!(names.contains(&"idx_user_memory_user".to_string()));
        assert!(names.contains(&"idx_user_agent_memory".to_string()));
    }

    /// A v9 database (PR 1 tip) must upgrade cleanly to v10: the three new
    /// tables appear without touching pre-existing rows.
    #[test]
    fn test_migration_v9_to_v10_upgrade_preserves_existing_rows() {
        let conn = Connection::open_in_memory().unwrap();

        // Stop at v9 (PR 1 baseline).
        migrate_v1(&conn).unwrap();
        migrate_v2(&conn).unwrap();
        migrate_v3(&conn).unwrap();
        migrate_v4(&conn).unwrap();
        migrate_v5(&conn).unwrap();
        migrate_v6(&conn).unwrap();
        migrate_v7(&conn).unwrap();
        migrate_v8(&conn).unwrap();
        migrate_v9(&conn).unwrap();
        set_schema_version(&conn, 9).unwrap();

        // Insert a v9 session row — nothing here should be touched by v10.
        conn.execute(
            "INSERT INTO sessions (id, agent_id, messages, context_window_tokens, user_id, created_at, updated_at) \
             VALUES ('sess-v9', 'agent-v9', X'', 0, '11111111-1111-1111-1111-111111111111', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();

        // Apply v10.
        run_migrations(&conn).unwrap();

        // New tables exist.
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(tables.contains(&"session_extractions".to_string()));
        assert!(tables.contains(&"user_memory_topics".to_string()));
        assert!(tables.contains(&"user_agent_memory_topics".to_string()));

        // Pre-existing session row untouched.
        let uid: String = conn
            .query_row(
                "SELECT user_id FROM sessions WHERE id = 'sess-v9'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(uid, "11111111-1111-1111-1111-111111111111");
    }
}
