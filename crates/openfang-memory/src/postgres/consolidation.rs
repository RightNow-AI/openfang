//! PostgreSQL implementation of memory consolidation (confidence decay).

use crate::backends::ConsolidationBackend;
use deadpool_postgres::Pool;
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::memory::ConsolidationReport;

pub struct PgConsolidationEngine {
    pool: Pool,
    /// Decay rate: how much to reduce confidence per consolidation cycle.
    decay_rate: f32,
}

impl PgConsolidationEngine {
    pub fn new(pool: Pool) -> Self {
        Self {
            pool,
            decay_rate: 0.1,
        }
    }

    pub fn with_decay_rate(mut self, rate: f32) -> Self {
        self.decay_rate = rate;
        self
    }

    fn block_on_pg<F, T>(&self, f: F) -> OpenFangResult<T>
    where
        F: std::future::Future<Output = OpenFangResult<T>>,
    {
        tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(f))
    }
}

impl ConsolidationBackend for PgConsolidationEngine {
    fn consolidate(&self) -> OpenFangResult<ConsolidationReport> {
        let start = std::time::Instant::now();
        let decay_factor: f32 = 1.0 - self.decay_rate;
        let cutoff = chrono::Utc::now() - chrono::Duration::days(7);

        let decayed = self.block_on_pg(async {
            let client = self
                .pool
                .get()
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let rows = client
                .execute(
                    "UPDATE memories SET confidence = GREATEST(0.1::real, confidence * $1)
                     WHERE deleted = FALSE AND accessed_at < $2 AND confidence > 0.1",
                    &[&decay_factor, &cutoff],
                )
                .await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(rows)
        })?;

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(ConsolidationReport {
            memories_merged: 0,
            memories_decayed: decayed,
            duration_ms,
        })
    }
}
