//! PostgreSQL schema creation and migration.
//!
//! Mirrors the SQLite migration structure (v1-v9) with PostgreSQL types:
//! - `BLOB` → `BYTEA`
//! - `TEXT` timestamps → `TIMESTAMPTZ`
//! - `INTEGER` booleans → `BOOLEAN`
//! - `TEXT` JSON properties → `JSONB`
//! - `BLOB` embeddings → `vector` (pgvector)
//! - Auto-increment → `BIGSERIAL`

use deadpool_postgres::Pool;
use openfang_types::error::{OpenFangError, OpenFangResult};

/// Current schema version.
const SCHEMA_VERSION: i32 = 9;

/// Run all migrations to bring the database up to date.
pub async fn run_migrations(pool: &Pool) -> OpenFangResult<()> {
    let client = pool
        .get()
        .await
        .map_err(|e| OpenFangError::Memory(format!("Failed to get PG connection: {e}")))?;

    // Enable pgvector extension
    client
        .execute("CREATE EXTENSION IF NOT EXISTS vector", &[])
        .await
        .map_err(|e| OpenFangError::Memory(format!("Failed to enable pgvector: {e}")))?;

    // Ensure schema_version tracking table exists
    client
        .execute(
            "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL DEFAULT 0)",
            &[],
        )
        .await
        .map_err(|e| OpenFangError::Memory(format!("PG migration failed: {e}")))?;

    // Initialize version row if empty
    let count: i64 = client
        .query_one("SELECT COUNT(*) FROM schema_version", &[])
        .await
        .map_err(|e| OpenFangError::Memory(e.to_string()))?
        .get(0);
    if count == 0 {
        client
            .execute("INSERT INTO schema_version (version) VALUES (0)", &[])
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
    }

    let current_version: i32 = client
        .query_one("SELECT version FROM schema_version LIMIT 1", &[])
        .await
        .map_err(|e| OpenFangError::Memory(e.to_string()))?
        .get(0);

    if current_version < 1 {
        migrate_v1(&client).await?;
    }
    if current_version < 2 {
        migrate_v2(&client).await?;
    }
    if current_version < 3 {
        migrate_v3(&client).await?;
    }
    if current_version < 4 {
        migrate_v4(&client).await?;
    }
    if current_version < 5 {
        migrate_v5(&client).await?;
    }
    if current_version < 6 {
        migrate_v6(&client).await?;
    }
    if current_version < 7 {
        migrate_v7(&client).await?;
    }
    if current_version < 8 {
        migrate_v8(&client).await?;
    }
    if current_version < 9 {
        migrate_v9(&client).await?;
    }

    // Update version
    client
        .execute(
            "UPDATE schema_version SET version = $1",
            &[&SCHEMA_VERSION],
        )
        .await
        .map_err(|e| OpenFangError::Memory(format!("PG migration failed: {e}")))?;

    Ok(())
}

type PgClient = deadpool_postgres::Object;

/// Version 1: Create all core tables.
async fn migrate_v1(client: &PgClient) -> OpenFangResult<()> {
    client
        .batch_execute(
            "
            -- Agent registry
            CREATE TABLE IF NOT EXISTS agents (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                manifest BYTEA NOT NULL,
                state TEXT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );

            -- Session history
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                messages BYTEA NOT NULL,
                context_window_tokens BIGINT NOT NULL DEFAULT 0,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );

            -- Event log
            CREATE TABLE IF NOT EXISTS events (
                id TEXT PRIMARY KEY,
                source_agent TEXT NOT NULL,
                target TEXT NOT NULL,
                payload BYTEA NOT NULL,
                timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);
            CREATE INDEX IF NOT EXISTS idx_events_source ON events(source_agent);

            -- Key-value store (per-agent)
            CREATE TABLE IF NOT EXISTS kv_store (
                agent_id TEXT NOT NULL,
                key TEXT NOT NULL,
                value BYTEA NOT NULL,
                version INTEGER NOT NULL DEFAULT 1,
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                PRIMARY KEY (agent_id, key)
            );

            -- Task queue
            CREATE TABLE IF NOT EXISTS task_queue (
                id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                task_type TEXT NOT NULL,
                payload BYTEA NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                priority INTEGER NOT NULL DEFAULT 0,
                scheduled_at TEXT,
                created_at TEXT NOT NULL DEFAULT '',
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
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                accessed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                access_count BIGINT NOT NULL DEFAULT 0,
                deleted BOOLEAN NOT NULL DEFAULT FALSE
            );
            CREATE INDEX IF NOT EXISTS idx_memories_agent ON memories(agent_id);
            CREATE INDEX IF NOT EXISTS idx_memories_scope ON memories(scope);

            -- Knowledge graph entities
            CREATE TABLE IF NOT EXISTS entities (
                id TEXT PRIMARY KEY,
                entity_type TEXT NOT NULL,
                name TEXT NOT NULL,
                properties JSONB NOT NULL DEFAULT '{}',
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );

            -- Knowledge graph relations
            CREATE TABLE IF NOT EXISTS relations (
                id TEXT PRIMARY KEY,
                source_entity TEXT NOT NULL REFERENCES entities(id),
                relation_type TEXT NOT NULL,
                target_entity TEXT NOT NULL REFERENCES entities(id),
                properties JSONB NOT NULL DEFAULT '{}',
                confidence REAL NOT NULL DEFAULT 1.0,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_relations_source ON relations(source_entity);
            CREATE INDEX IF NOT EXISTS idx_relations_target ON relations(target_entity);
            CREATE INDEX IF NOT EXISTS idx_relations_type ON relations(relation_type);

            -- Migration tracking
            CREATE TABLE IF NOT EXISTS migrations (
                version INTEGER PRIMARY KEY,
                applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                description TEXT
            );

            INSERT INTO migrations (version, applied_at, description)
            VALUES (1, NOW(), 'Initial schema')
            ON CONFLICT (version) DO NOTHING;
            ",
        )
        .await
        .map_err(|e| OpenFangError::Memory(format!("PG migration v1 failed: {e}")))?;
    Ok(())
}

/// Version 2: Add collaboration columns to task_queue.
async fn migrate_v2(client: &PgClient) -> OpenFangResult<()> {
    // PostgreSQL supports ADD COLUMN IF NOT EXISTS
    client
        .batch_execute(
            "
            ALTER TABLE task_queue ADD COLUMN IF NOT EXISTS title TEXT DEFAULT '';
            ALTER TABLE task_queue ADD COLUMN IF NOT EXISTS description TEXT DEFAULT '';
            ALTER TABLE task_queue ADD COLUMN IF NOT EXISTS assigned_to TEXT DEFAULT '';
            ALTER TABLE task_queue ADD COLUMN IF NOT EXISTS created_by TEXT DEFAULT '';
            ALTER TABLE task_queue ADD COLUMN IF NOT EXISTS result TEXT DEFAULT '';
            CREATE INDEX IF NOT EXISTS idx_task_queue_status ON task_queue(status);

            INSERT INTO migrations (version, applied_at, description)
            VALUES (2, NOW(), 'Add collaboration columns to task_queue')
            ON CONFLICT (version) DO NOTHING;
            ",
        )
        .await
        .map_err(|e| OpenFangError::Memory(format!("PG migration v2 failed: {e}")))?;
    Ok(())
}

/// Version 3: Add embedding column to memories table.
async fn migrate_v3(client: &PgClient) -> OpenFangResult<()> {
    client
        .batch_execute(
            "
            ALTER TABLE memories ADD COLUMN IF NOT EXISTS embedding vector;

            INSERT INTO migrations (version, applied_at, description)
            VALUES (3, NOW(), 'Add embedding column to memories')
            ON CONFLICT (version) DO NOTHING;
            ",
        )
        .await
        .map_err(|e| OpenFangError::Memory(format!("PG migration v3 failed: {e}")))?;
    Ok(())
}

/// Version 4: Add usage_events table for cost tracking.
async fn migrate_v4(client: &PgClient) -> OpenFangResult<()> {
    client
        .batch_execute(
            "
            CREATE TABLE IF NOT EXISTS usage_events (
                id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
                agent_id TEXT NOT NULL,
                timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                model TEXT NOT NULL,
                input_tokens BIGINT NOT NULL DEFAULT 0,
                output_tokens BIGINT NOT NULL DEFAULT 0,
                cost_usd DOUBLE PRECISION NOT NULL DEFAULT 0,
                tool_calls BIGINT NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_usage_agent_time ON usage_events(agent_id, timestamp);
            CREATE INDEX IF NOT EXISTS idx_usage_timestamp ON usage_events(timestamp);

            INSERT INTO migrations (version, applied_at, description)
            VALUES (4, NOW(), 'Add usage_events table for cost tracking')
            ON CONFLICT (version) DO NOTHING;
            ",
        )
        .await
        .map_err(|e| OpenFangError::Memory(format!("PG migration v4 failed: {e}")))?;
    Ok(())
}

/// Version 5: Add canonical_sessions table for cross-channel memory.
async fn migrate_v5(client: &PgClient) -> OpenFangResult<()> {
    client
        .batch_execute(
            "
            CREATE TABLE IF NOT EXISTS canonical_sessions (
                agent_id TEXT PRIMARY KEY,
                messages BYTEA NOT NULL,
                compaction_cursor INTEGER NOT NULL DEFAULT 0,
                compacted_summary TEXT,
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );

            INSERT INTO migrations (version, applied_at, description)
            VALUES (5, NOW(), 'Add canonical_sessions for cross-channel memory')
            ON CONFLICT (version) DO NOTHING;
            ",
        )
        .await
        .map_err(|e| OpenFangError::Memory(format!("PG migration v5 failed: {e}")))?;
    Ok(())
}

/// Version 6: Add label column to sessions table.
async fn migrate_v6(client: &PgClient) -> OpenFangResult<()> {
    client
        .batch_execute(
            "
            ALTER TABLE sessions ADD COLUMN IF NOT EXISTS label TEXT;

            INSERT INTO migrations (version, applied_at, description)
            VALUES (6, NOW(), 'Add label column to sessions')
            ON CONFLICT (version) DO NOTHING;
            ",
        )
        .await
        .map_err(|e| OpenFangError::Memory(format!("PG migration v6 failed: {e}")))?;
    Ok(())
}

/// Version 7: Add paired_devices table.
async fn migrate_v7(client: &PgClient) -> OpenFangResult<()> {
    client
        .batch_execute(
            "
            CREATE TABLE IF NOT EXISTS paired_devices (
                device_id TEXT PRIMARY KEY,
                display_name TEXT NOT NULL DEFAULT '',
                platform TEXT NOT NULL DEFAULT '',
                paired_at TEXT NOT NULL DEFAULT '',
                last_seen TEXT NOT NULL DEFAULT '',
                push_token TEXT
            );

            INSERT INTO migrations (version, applied_at, description)
            VALUES (7, NOW(), 'Add paired_devices table')
            ON CONFLICT (version) DO NOTHING;
            ",
        )
        .await
        .map_err(|e| OpenFangError::Memory(format!("PG migration v7 failed: {e}")))?;
    Ok(())
}

/// Version 8: Add audit_entries table for Merkle audit trail.
async fn migrate_v8(client: &PgClient) -> OpenFangResult<()> {
    client
        .batch_execute(
            "
            CREATE TABLE IF NOT EXISTS audit_entries (
                seq BIGSERIAL PRIMARY KEY,
                timestamp TEXT NOT NULL,
                agent_id TEXT NOT NULL DEFAULT '',
                action TEXT NOT NULL DEFAULT '',
                detail TEXT NOT NULL DEFAULT '',
                outcome TEXT NOT NULL DEFAULT '',
                prev_hash TEXT NOT NULL DEFAULT '',
                hash TEXT NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_audit_agent ON audit_entries(agent_id);
            CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_entries(timestamp);
            CREATE INDEX IF NOT EXISTS idx_audit_action ON audit_entries(action);
            CREATE INDEX IF NOT EXISTS idx_audit_agent_time ON audit_entries(agent_id, timestamp);

            INSERT INTO migrations (version, applied_at, description)
            VALUES (8, NOW(), 'Add audit_entries table for Merkle audit trail')
            ON CONFLICT (version) DO NOTHING;
            ",
        )
        .await
        .map_err(|e| OpenFangError::Memory(format!("PG migration v8 failed: {e}")))?;
    Ok(())
}

/// Version 9: Add agent identity columns.
///
/// Note: SQLite v9 creates sqlite-vec virtual table. PostgreSQL uses pgvector
/// (enabled in v3 via `vector` column type) so no equivalent is needed here.
/// Instead, v9 adds the agent identity columns that were created inline in
/// the original PG schema.
async fn migrate_v9(client: &PgClient) -> OpenFangResult<()> {
    client
        .batch_execute(
            "
            ALTER TABLE agents ADD COLUMN IF NOT EXISTS session_id TEXT NOT NULL DEFAULT '';
            ALTER TABLE agents ADD COLUMN IF NOT EXISTS identity TEXT NOT NULL DEFAULT '{}';

            INSERT INTO migrations (version, applied_at, description)
            VALUES (9, NOW(), 'Add agent session_id and identity columns')
            ON CONFLICT (version) DO NOTHING;
            ",
        )
        .await
        .map_err(|e| OpenFangError::Memory(format!("PG migration v9 failed: {e}")))?;
    Ok(())
}
