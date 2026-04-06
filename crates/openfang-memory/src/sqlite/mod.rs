//! SQLite backend implementations for the OpenFang memory layer.

pub mod audit;
pub mod consolidation;
pub mod knowledge;
pub mod migration;
pub mod paired_devices;
pub mod semantic;
pub mod session;
pub mod structured;
pub mod task_queue;
pub mod usage;

pub use audit::SqliteAuditStore;
pub use consolidation::ConsolidationEngine;
pub use knowledge::KnowledgeStore;
pub use paired_devices::SqlitePairedDevicesStore;
pub use semantic::SemanticStore;
pub use session::SessionStore;
pub use structured::StructuredStore;
pub use task_queue::SqliteTaskQueueStore;
pub use usage::UsageStore;

use openfang_types::error::{OpenFangError, OpenFangResult};
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Factory that opens a single SQLite connection and hands out typed stores.
pub struct SqliteBackend {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteBackend {
    /// Open (or create) a database file, register sqlite-vec, apply PRAGMAs
    /// and run migrations.
    pub fn open(db_path: &Path) -> OpenFangResult<Self> {
        // Register sqlite-vec as auto-extension before opening the connection.
        // This is process-wide and idempotent — safe to call multiple times.
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute::<
                *const (),
                unsafe extern "C" fn(
                    *mut rusqlite::ffi::sqlite3,
                    *mut *const u8,
                    *const rusqlite::ffi::sqlite3_api_routines,
                ) -> i32,
            >(sqlite_vec::sqlite3_vec_init as *const ())));
        }
        let conn = Connection::open(db_path).map_err(|e| OpenFangError::Memory(e.to_string()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        migration::run_migrations(&conn).map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Open an in-memory database (for testing).
    pub fn open_in_memory() -> OpenFangResult<Self> {
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute::<
                *const (),
                unsafe extern "C" fn(
                    *mut rusqlite::ffi::sqlite3,
                    *mut *const u8,
                    *const rusqlite::ffi::sqlite3_api_routines,
                ) -> i32,
            >(sqlite_vec::sqlite3_vec_init as *const ())));
        }
        let conn =
            Connection::open_in_memory().map_err(|e| OpenFangError::Memory(e.to_string()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        migration::run_migrations(&conn).map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Return a clone of the shared connection handle.
    pub fn conn(&self) -> Arc<Mutex<Connection>> {
        Arc::clone(&self.conn)
    }

    pub fn structured(&self) -> StructuredStore {
        StructuredStore::new(Arc::clone(&self.conn))
    }

    pub fn semantic(&self) -> SemanticStore {
        SemanticStore::new(Arc::clone(&self.conn))
    }

    pub fn knowledge(&self) -> KnowledgeStore {
        KnowledgeStore::new(Arc::clone(&self.conn))
    }

    pub fn session(&self) -> SessionStore {
        SessionStore::new(Arc::clone(&self.conn))
    }

    pub fn usage(&self) -> UsageStore {
        UsageStore::new(Arc::clone(&self.conn))
    }

    pub fn paired_devices(&self) -> SqlitePairedDevicesStore {
        SqlitePairedDevicesStore::new(Arc::clone(&self.conn))
    }

    pub fn task_queue(&self) -> SqliteTaskQueueStore {
        SqliteTaskQueueStore::new(Arc::clone(&self.conn))
    }

    pub fn consolidation(&self, decay_rate: f32) -> ConsolidationEngine {
        ConsolidationEngine::new(Arc::clone(&self.conn), decay_rate)
    }

    pub fn audit(&self) -> SqliteAuditStore {
        SqliteAuditStore::new(Arc::clone(&self.conn))
    }
}
