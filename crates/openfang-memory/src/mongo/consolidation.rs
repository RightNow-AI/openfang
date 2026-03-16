//! MongoDB memory consolidation and decay logic.

use chrono::Utc;
use bson::doc;
use mongodb::Collection;
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::memory::ConsolidationReport;

/// Memory consolidation engine backed by MongoDB.
#[derive(Clone)]
pub struct MongoConsolidationEngine {
    memories: Collection<bson::Document>,
    decay_rate: f32,
}

impl MongoConsolidationEngine {
    pub fn new(db: mongodb::Database, decay_rate: f32) -> Self {
        Self {
            memories: db.collection("memories"),
            decay_rate,
        }
    }

    /// Run a consolidation cycle: decay old memories.
    pub async fn consolidate(&self) -> OpenFangResult<ConsolidationReport> {
        let start = std::time::Instant::now();

        // Decay confidence of memories not accessed in the last 7 days
        let cutoff = bson::DateTime::from_chrono(
            Utc::now() - chrono::Duration::days(7),
        );
        let decay_factor = 1.0 - self.decay_rate as f64;

        let filter = doc! {
            "deleted": false,
            "accessed_at": { "$lt": cutoff },
            "confidence": { "$gt": 0.1 },
        };

        // Use an aggregation pipeline update to compute new confidence in-place
        let update = vec![doc! {
            "$set": {
                "confidence": {
                    "$max": [0.1, { "$multiply": ["$confidence", decay_factor] }]
                }
            }
        }];

        let result = self
            .memories
            .update_many(filter, update)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(ConsolidationReport {
            memories_merged: 0,
            memories_decayed: result.modified_count,
            duration_ms,
        })
    }
}
