//! SurrealDB-backed usage store.

use openfang_types::agent::AgentId;
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::usage::{DailyBreakdown, ModelUsage, UsageRecord, UsageSummary};
use surrealdb::engine::local::Db;
use surrealdb::Surreal;

/// SurrealDB-backed usage store — provides the same interface as the SQLite UsageStore.
///
/// Queries the `usage_records` table in SurrealDB for aggregated usage data.
/// Time-windowed queries (hourly, daily, monthly) use RFC 3339 date comparisons.
pub struct SurrealUsageStore {
    db: Option<Surreal<Db>>,
}

impl Clone for SurrealUsageStore {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
        }
    }
}

impl SurrealUsageStore {
    /// Create a new SurrealDB usage store backed by the given connection.
    pub fn with_db(db: Surreal<Db>) -> Self {
        Self { db: Some(db) }
    }

    /// Create an empty (no-op) usage store.
    pub fn new() -> Self {
        Self { db: None }
    }

    /// Helper: run an async block on the current tokio runtime.
    fn block_on<F: std::future::Future<Output = OpenFangResult<T>>, T>(&self, f: F) -> OpenFangResult<T> {
        tokio::runtime::Handle::current().block_on(f)
    }

    /// Get a reference to the database, returning an error if not initialized.
    fn db(&self) -> OpenFangResult<&Surreal<Db>> {
        self.db.as_ref().ok_or_else(|| {
            OpenFangError::Memory("SurrealUsageStore: database not initialized".to_string())
        })
    }

    /// Compute an RFC 3339 cutoff string for "now minus N hours".
    fn hours_ago(hours: i64) -> String {
        (chrono::Utc::now() - chrono::Duration::hours(hours)).to_rfc3339()
    }

    /// Compute an RFC 3339 cutoff string for "start of today (UTC)".
    fn start_of_today() -> String {
        chrono::Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .to_rfc3339()
    }

    /// Compute an RFC 3339 cutoff string for "N days ago at midnight UTC".
    fn days_ago(days: i64) -> String {
        (chrono::Utc::now() - chrono::Duration::days(days))
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .to_rfc3339()
    }

    /// Compute an RFC 3339 cutoff string for "start of this month (UTC)".
    fn start_of_month() -> String {
        let now = chrono::Utc::now();
        chrono::NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .to_rfc3339()
    }

    // -----------------------------------------------------------------------
    // Write operations
    // -----------------------------------------------------------------------

    /// Record a usage event.
    pub fn record(&self, record: &UsageRecord) -> OpenFangResult<()> {
        let db = match &self.db {
            Some(db) => db.clone(),
            None => return Ok(()),
        };
        let agent_id = record.agent_id.0.to_string();
        let model = record.model.clone();
        let input_tokens = record.input_tokens;
        let output_tokens = record.output_tokens;
        let cost_usd = record.cost_usd;
        let tool_calls = record.tool_calls;

        self.block_on(async {
            let id = uuid::Uuid::new_v4().to_string();
            let _: Option<serde_json::Value> = db
                .create(("usage_records", id.clone()))
                .content(serde_json::json!({
                    "agent_id": agent_id,
                    "model": model,
                    "input_tokens": input_tokens,
                    "output_tokens": output_tokens,
                    "cost_usd": cost_usd,
                    "tool_calls": tool_calls,
                    "created_at": chrono::Utc::now().to_rfc3339(),
                }))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Usage record insert failed: {}", e)))?;
            Ok(())
        })
    }

    // -----------------------------------------------------------------------
    // Per-agent time-windowed cost queries
    // -----------------------------------------------------------------------

    /// Query hourly cost for an agent (last 1 hour).
    pub fn query_hourly(&self, agent_id: AgentId) -> OpenFangResult<f64> {
        let db = match &self.db {
            Some(db) => db.clone(),
            None => return Ok(0.0),
        };
        let aid = agent_id.0.to_string();
        let cutoff = Self::hours_ago(1);

        self.block_on(async {
            let results: Vec<serde_json::Value> = db
                .query("SELECT math::sum(cost_usd) AS total FROM usage_records WHERE agent_id = $aid AND created_at >= $cutoff GROUP ALL")
                .bind(("aid", aid))
                .bind(("cutoff", cutoff))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Hourly query failed: {}", e)))?
                .take(0)
                .unwrap_or_default();

            Ok(results.first()
                .and_then(|r| r.get("total"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0))
        })
    }

    /// Query daily cost for an agent (since start of today UTC).
    pub fn query_daily(&self, agent_id: AgentId) -> OpenFangResult<f64> {
        let db = match &self.db {
            Some(db) => db.clone(),
            None => return Ok(0.0),
        };
        let aid = agent_id.0.to_string();
        let cutoff = Self::start_of_today();

        self.block_on(async {
            let results: Vec<serde_json::Value> = db
                .query("SELECT math::sum(cost_usd) AS total FROM usage_records WHERE agent_id = $aid AND created_at >= $cutoff GROUP ALL")
                .bind(("aid", aid))
                .bind(("cutoff", cutoff))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Daily query failed: {}", e)))?
                .take(0)
                .unwrap_or_default();

            Ok(results.first()
                .and_then(|r| r.get("total"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0))
        })
    }

    /// Query monthly cost for an agent (since start of this month UTC).
    pub fn query_monthly(&self, agent_id: AgentId) -> OpenFangResult<f64> {
        let db = match &self.db {
            Some(db) => db.clone(),
            None => return Ok(0.0),
        };
        let aid = agent_id.0.to_string();
        let cutoff = Self::start_of_month();

        self.block_on(async {
            let results: Vec<serde_json::Value> = db
                .query("SELECT math::sum(cost_usd) AS total FROM usage_records WHERE agent_id = $aid AND created_at >= $cutoff GROUP ALL")
                .bind(("aid", aid))
                .bind(("cutoff", cutoff))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Monthly query failed: {}", e)))?
                .take(0)
                .unwrap_or_default();

            Ok(results.first()
                .and_then(|r| r.get("total"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0))
        })
    }

    // -----------------------------------------------------------------------
    // Global time-windowed cost queries
    // -----------------------------------------------------------------------

    /// Query global hourly cost (last 1 hour, all agents).
    pub fn query_global_hourly(&self) -> OpenFangResult<f64> {
        let db = match &self.db {
            Some(db) => db.clone(),
            None => return Ok(0.0),
        };
        let cutoff = Self::hours_ago(1);

        self.block_on(async {
            let results: Vec<serde_json::Value> = db
                .query("SELECT math::sum(cost_usd) AS total FROM usage_records WHERE created_at >= $cutoff GROUP ALL")
                .bind(("cutoff", cutoff))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Global hourly query failed: {}", e)))?
                .take(0)
                .unwrap_or_default();

            Ok(results.first()
                .and_then(|r| r.get("total"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0))
        })
    }

    /// Query global monthly cost (since start of this month, all agents).
    pub fn query_global_monthly(&self) -> OpenFangResult<f64> {
        let db = match &self.db {
            Some(db) => db.clone(),
            None => return Ok(0.0),
        };
        let cutoff = Self::start_of_month();

        self.block_on(async {
            let results: Vec<serde_json::Value> = db
                .query("SELECT math::sum(cost_usd) AS total FROM usage_records WHERE created_at >= $cutoff GROUP ALL")
                .bind(("cutoff", cutoff))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Global monthly query failed: {}", e)))?
                .take(0)
                .unwrap_or_default();

            Ok(results.first()
                .and_then(|r| r.get("total"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0))
        })
    }

    // -----------------------------------------------------------------------
    // Aggregated queries
    // -----------------------------------------------------------------------

    /// Get a usage summary, optionally filtered by agent.
    pub fn query_summary(&self, agent_id: Option<AgentId>) -> OpenFangResult<UsageSummary> {
        let db = match &self.db {
            Some(db) => db.clone(),
            None => return Ok(UsageSummary::default()),
        };

        self.block_on(async {
            let (sql, bindings) = if let Some(aid) = agent_id {
                (
                    "SELECT math::sum(input_tokens) AS ti, math::sum(output_tokens) AS to_val, math::sum(cost_usd) AS tc, count() AS cc, math::sum(tool_calls) AS tt FROM usage_records WHERE agent_id = $aid GROUP ALL".to_string(),
                    Some(("aid", aid.0.to_string())),
                )
            } else {
                (
                    "SELECT math::sum(input_tokens) AS ti, math::sum(output_tokens) AS to_val, math::sum(cost_usd) AS tc, count() AS cc, math::sum(tool_calls) AS tt FROM usage_records GROUP ALL".to_string(),
                    None,
                )
            };

            let results: Vec<serde_json::Value> = if let Some((key, val)) = bindings {
                db.query(&sql)
                    .bind((key, val))
                    .await
                    .map_err(|e| OpenFangError::Memory(format!("Usage summary query failed: {}", e)))?
                    .take(0)
                    .unwrap_or_default()
            } else {
                db.query(&sql)
                    .await
                    .map_err(|e| OpenFangError::Memory(format!("Usage summary query failed: {}", e)))?
                    .take(0)
                    .unwrap_or_default()
            };

            if let Some(row) = results.first() {
                Ok(UsageSummary {
                    total_input_tokens: row.get("ti").and_then(|v| v.as_u64()).unwrap_or(0),
                    total_output_tokens: row.get("to_val").and_then(|v| v.as_u64()).unwrap_or(0),
                    total_cost_usd: row.get("tc").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    call_count: row.get("cc").and_then(|v| v.as_u64()).unwrap_or(0),
                    total_tool_calls: row.get("tt").and_then(|v| v.as_u64()).unwrap_or(0),
                })
            } else {
                Ok(UsageSummary::default())
            }
        })
    }

    /// Get usage grouped by model.
    pub fn query_by_model(&self) -> OpenFangResult<Vec<ModelUsage>> {
        let db = match &self.db {
            Some(db) => db.clone(),
            None => return Ok(Vec::new()),
        };

        self.block_on(async {
            let results: Vec<serde_json::Value> = db
                .query(r#"
                    SELECT
                        model,
                        math::sum(input_tokens) AS ti,
                        math::sum(output_tokens) AS to_val,
                        math::sum(cost_usd) AS tc,
                        count() AS cc
                    FROM usage_records
                    GROUP BY model
                "#)
                .await
                .map_err(|e| OpenFangError::Memory(format!("Usage by model query failed: {}", e)))?
                .take(0)
                .unwrap_or_default();

            Ok(results.iter().filter_map(|row| {
                Some(ModelUsage {
                    model: row.get("model")?.as_str()?.to_string(),
                    total_cost_usd: row.get("tc").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    total_input_tokens: row.get("ti").and_then(|v| v.as_u64()).unwrap_or(0),
                    total_output_tokens: row.get("to_val").and_then(|v| v.as_u64()).unwrap_or(0),
                    call_count: row.get("cc").and_then(|v| v.as_u64()).unwrap_or(0),
                })
            }).collect())
        })
    }

    /// Get daily usage breakdown for the last N days.
    pub fn query_daily_breakdown(&self, days: u32) -> OpenFangResult<Vec<DailyBreakdown>> {
        let db = match &self.db {
            Some(db) => db.clone(),
            None => return Ok(Vec::new()),
        };
        let cutoff = Self::days_ago(days as i64);

        self.block_on(async {
            let results: Vec<serde_json::Value> = db
                .query(r#"
                    SELECT
                        string::slice(created_at, 0, 10) AS date,
                        math::sum(cost_usd) AS tc,
                        math::sum(input_tokens) + math::sum(output_tokens) AS tokens,
                        count() AS cc
                    FROM usage_records
                    WHERE created_at >= $cutoff
                    GROUP BY string::slice(created_at, 0, 10)
                    ORDER BY date DESC
                "#)
                .bind(("cutoff", cutoff))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Daily breakdown query failed: {}", e)))?
                .take(0)
                .unwrap_or_default();

            Ok(results.iter().filter_map(|row| {
                Some(DailyBreakdown {
                    date: row.get("date")?.as_str()?.to_string(),
                    cost_usd: row.get("tc").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    tokens: row.get("tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    calls: row.get("cc").and_then(|v| v.as_u64()).unwrap_or(0),
                })
            }).collect())
        })
    }

    /// Get the date of the first usage event.
    pub fn query_first_event_date(&self) -> OpenFangResult<Option<String>> {
        let db = match &self.db {
            Some(db) => db.clone(),
            None => return Ok(None),
        };

        self.block_on(async {
            let results: Vec<serde_json::Value> = db
                .query("SELECT created_at FROM usage_records ORDER BY created_at ASC LIMIT 1")
                .await
                .map_err(|e| OpenFangError::Memory(format!("First event date query failed: {}", e)))?
                .take(0)
                .unwrap_or_default();

            Ok(results.first()
                .and_then(|r| r.get("created_at"))
                .and_then(|v| v.as_str())
                .map(|s| s[..10].to_string()))
        })
    }

    /// Get today's total cost across all agents.
    pub fn query_today_cost(&self) -> OpenFangResult<f64> {
        let db = match &self.db {
            Some(db) => db.clone(),
            None => return Ok(0.0),
        };
        let cutoff = Self::start_of_today();

        self.block_on(async {
            let results: Vec<serde_json::Value> = db
                .query("SELECT math::sum(cost_usd) AS total FROM usage_records WHERE created_at >= $cutoff GROUP ALL")
                .bind(("cutoff", cutoff))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Today cost query failed: {}", e)))?
                .take(0)
                .unwrap_or_default();

            Ok(results.first()
                .and_then(|r| r.get("total"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0))
        })
    }

    /// Clean up usage records older than N days.
    pub fn cleanup_old(&self, days: u32) -> OpenFangResult<usize> {
        let db = match &self.db {
            Some(db) => db.clone(),
            None => return Ok(0),
        };
        let cutoff = Self::days_ago(days as i64);

        self.block_on(async {
            // Count before delete
            let count_result: Vec<serde_json::Value> = db
                .query("SELECT count() AS cnt FROM usage_records WHERE created_at < $cutoff GROUP ALL")
                .bind(("cutoff", cutoff.clone()))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Cleanup count failed: {}", e)))?
                .take(0)
                .unwrap_or_default();

            let count = count_result.first()
                .and_then(|r| r.get("cnt"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;

            // Delete old records
            db.query("DELETE usage_records WHERE created_at < $cutoff")
                .bind(("cutoff", cutoff))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Cleanup delete failed: {}", e)))?;

            Ok(count)
        })
    }
}

// Need chrono traits for year()/month()
use chrono::Datelike;
