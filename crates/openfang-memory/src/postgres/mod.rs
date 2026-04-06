//! PostgreSQL backend for the OpenFang memory layer.
//!
//! Uses `tokio-postgres` with `deadpool-postgres` for connection pooling
//! and `pgvector` for vector similarity search.
//!
//! Enable with `cargo build --features postgres`.

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

pub use audit::PgAuditStore;
pub use consolidation::PgConsolidationEngine;
pub use knowledge::PgKnowledgeStore;
pub use migration::run_migrations;
pub use paired_devices::PgPairedDevicesStore;
pub use semantic::PgSemanticStore;
pub use session::PgSessionStore;
pub use structured::PgStructuredStore;
pub use task_queue::PgTaskQueueStore;
pub use usage::PgUsageStore;

use deadpool_postgres::{Config, Pool, Runtime};
use openfang_types::error::{OpenFangError, OpenFangResult};
use tokio_postgres::NoTls;

/// Create a connection pool from a PostgreSQL URL.
pub fn create_pool(url: &str, pool_size: u32) -> OpenFangResult<Pool> {
    let mut cfg = Config::new();
    cfg.url = Some(url.to_string());
    cfg.pool = Some(deadpool_postgres::PoolConfig::new(pool_size as usize));
    cfg.create_pool(Some(Runtime::Tokio1), NoTls)
        .map_err(|e| OpenFangError::Memory(format!("Failed to create PostgreSQL pool: {e}")))
}

/// Convenience factory that creates all PostgreSQL-backed stores from a single pool.
pub struct PgBackend {
    pool: Pool,
}

impl PgBackend {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub fn structured(&self) -> PgStructuredStore {
        PgStructuredStore::new(self.pool.clone())
    }

    pub fn semantic(&self) -> PgSemanticStore {
        PgSemanticStore::new(self.pool.clone())
    }

    pub fn knowledge(&self) -> PgKnowledgeStore {
        PgKnowledgeStore::new(self.pool.clone())
    }

    pub fn session(&self) -> PgSessionStore {
        PgSessionStore::new(self.pool.clone())
    }

    pub fn usage(&self) -> PgUsageStore {
        PgUsageStore::new(self.pool.clone())
    }

    pub fn paired_devices(&self) -> PgPairedDevicesStore {
        PgPairedDevicesStore::new(self.pool.clone())
    }

    pub fn task_queue(&self) -> PgTaskQueueStore {
        PgTaskQueueStore::new(self.pool.clone())
    }

    pub fn consolidation(&self) -> PgConsolidationEngine {
        PgConsolidationEngine::new(self.pool.clone())
    }

    pub fn audit(&self) -> PgAuditStore {
        PgAuditStore::new(self.pool.clone())
    }
}
