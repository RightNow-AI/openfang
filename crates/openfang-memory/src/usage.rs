//! Usage tracking store — records LLM usage events for cost monitoring.

use chrono::Utc;
use openfang_types::agent::AgentId;
use openfang_types::error::{OpenFangError, OpenFangResult};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// A single usage event recording an LLM call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    /// Which agent made the call.
    pub agent_id: AgentId,
    /// Model used.
    pub model: String,
    /// Input tokens consumed.
    pub input_tokens: u64,
    /// Output tokens consumed.
    pub output_tokens: u64,
    /// Estimated cost in USD.
    pub cost_usd: f64,
    /// Number of tool calls in this interaction.
    pub tool_calls: u32,
}

/// Summary of usage over a period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSummary {
    /// Total input tokens.
    pub total_input_tokens: u64,
    /// Total output tokens.
    pub total_output_tokens: u64,
    /// Total estimated cost in USD.
    pub total_cost_usd: f64,
    /// Total number of calls.
    pub call_count: u64,
    /// Total tool calls.
    pub total_tool_calls: u64,
}

/// Usage grouped by model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelUsage {
    /// Model name.
    pub model: String,
    /// Total cost for this model.
    pub total_cost_usd: f64,
    /// Total input tokens.
    pub total_input_tokens: u64,
    /// Total output tokens.
    pub total_output_tokens: u64,
    /// Number of calls.
    pub call_count: u64,
}

/// Daily usage breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyBreakdown {
    /// Date string (YYYY-MM-DD).
    pub date: String,
    /// Total cost for this day.
    pub cost_usd: f64,
    /// Total tokens (input + output).
    pub tokens: u64,
    /// Number of API calls.
    pub calls: u64,
}

/// Internal backend selector for the usage store.
enum UsageBackend {
    Sqlite(Arc<Mutex<Connection>>),
    Mongo(crate::mongo::usage::MongoUsageStore),
}

/// Sync block-on helper for MongoDB async calls.
fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(f))
}

/// Usage store with dual SQLite/MongoDB backend support.
pub struct UsageStore {
    backend: UsageBackend,
}

// Manual Clone since we can't derive across the enum easily.
impl Clone for UsageStore {
    fn clone(&self) -> Self {
        match &self.backend {
            UsageBackend::Sqlite(conn) => Self {
                backend: UsageBackend::Sqlite(Arc::clone(conn)),
            },
            UsageBackend::Mongo(store) => Self {
                backend: UsageBackend::Mongo(store.clone()),
            },
        }
    }
}

impl UsageStore {
    /// Create a new usage store wrapping an SQLite connection.
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self {
            backend: UsageBackend::Sqlite(conn),
        }
    }

    /// Create a new usage store wrapping a MongoDB usage store.
    pub fn from_mongo(store: crate::mongo::usage::MongoUsageStore) -> Self {
        Self {
            backend: UsageBackend::Mongo(store),
        }
    }

    /// Record a usage event.
    pub fn record(&self, record: &UsageRecord) -> OpenFangResult<()> {
        match &self.backend {
            UsageBackend::Sqlite(conn) => {
                let conn = conn
                    .lock()
                    .map_err(|e| OpenFangError::Internal(e.to_string()))?;
                let id = uuid::Uuid::new_v4().to_string();
                let now = Utc::now().to_rfc3339();
                conn.execute(
                    "INSERT INTO usage_events (id, agent_id, timestamp, model, input_tokens, output_tokens, cost_usd, tool_calls)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    rusqlite::params![
                        id,
                        record.agent_id.0.to_string(),
                        now,
                        record.model,
                        record.input_tokens as i64,
                        record.output_tokens as i64,
                        record.cost_usd,
                        record.tool_calls as i64,
                    ],
                )
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
                Ok(())
            }
            UsageBackend::Mongo(store) => block_on(store.record(record)),
        }
    }

    /// Query total cost in the last hour for an agent.
    pub fn query_hourly(&self, agent_id: AgentId) -> OpenFangResult<f64> {
        match &self.backend {
            UsageBackend::Sqlite(conn) => {
                let conn = conn
                    .lock()
                    .map_err(|e| OpenFangError::Internal(e.to_string()))?;
                let cost: f64 = conn
                    .query_row(
                        "SELECT COALESCE(SUM(cost_usd), 0.0) FROM usage_events
                         WHERE agent_id = ?1 AND timestamp > datetime('now', '-1 hour')",
                        rusqlite::params![agent_id.0.to_string()],
                        |row| row.get(0),
                    )
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;
                Ok(cost)
            }
            UsageBackend::Mongo(store) => block_on(store.query_hourly(agent_id)),
        }
    }

    /// Query total cost today for an agent.
    pub fn query_daily(&self, agent_id: AgentId) -> OpenFangResult<f64> {
        match &self.backend {
            UsageBackend::Sqlite(conn) => {
                let conn = conn
                    .lock()
                    .map_err(|e| OpenFangError::Internal(e.to_string()))?;
                let cost: f64 = conn
                    .query_row(
                        "SELECT COALESCE(SUM(cost_usd), 0.0) FROM usage_events
                         WHERE agent_id = ?1 AND timestamp > datetime('now', 'start of day')",
                        rusqlite::params![agent_id.0.to_string()],
                        |row| row.get(0),
                    )
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;
                Ok(cost)
            }
            UsageBackend::Mongo(store) => block_on(store.query_daily(agent_id)),
        }
    }

    /// Query total cost in the current calendar month for an agent.
    pub fn query_monthly(&self, agent_id: AgentId) -> OpenFangResult<f64> {
        match &self.backend {
            UsageBackend::Sqlite(conn) => {
                let conn = conn
                    .lock()
                    .map_err(|e| OpenFangError::Internal(e.to_string()))?;
                let cost: f64 = conn
                    .query_row(
                        "SELECT COALESCE(SUM(cost_usd), 0.0) FROM usage_events
                         WHERE agent_id = ?1 AND timestamp > datetime('now', 'start of month')",
                        rusqlite::params![agent_id.0.to_string()],
                        |row| row.get(0),
                    )
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;
                Ok(cost)
            }
            UsageBackend::Mongo(store) => block_on(store.query_monthly(agent_id)),
        }
    }

    /// Query total cost across all agents for the current hour.
    pub fn query_global_hourly(&self) -> OpenFangResult<f64> {
        match &self.backend {
            UsageBackend::Sqlite(conn) => {
                let conn = conn
                    .lock()
                    .map_err(|e| OpenFangError::Internal(e.to_string()))?;
                let cost: f64 = conn
                    .query_row(
                        "SELECT COALESCE(SUM(cost_usd), 0.0) FROM usage_events
                         WHERE timestamp > datetime('now', '-1 hour')",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;
                Ok(cost)
            }
            UsageBackend::Mongo(store) => block_on(store.query_global_hourly()),
        }
    }

    /// Query total cost across all agents for the current calendar month.
    pub fn query_global_monthly(&self) -> OpenFangResult<f64> {
        match &self.backend {
            UsageBackend::Sqlite(conn) => {
                let conn = conn
                    .lock()
                    .map_err(|e| OpenFangError::Internal(e.to_string()))?;
                let cost: f64 = conn
                    .query_row(
                        "SELECT COALESCE(SUM(cost_usd), 0.0) FROM usage_events
                         WHERE timestamp > datetime('now', 'start of month')",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;
                Ok(cost)
            }
            UsageBackend::Mongo(store) => block_on(store.query_global_monthly()),
        }
    }

    /// Query usage summary, optionally filtered by agent.
    pub fn query_summary(&self, agent_id: Option<AgentId>) -> OpenFangResult<UsageSummary> {
        match &self.backend {
            UsageBackend::Sqlite(conn) => {
                let conn = conn
                    .lock()
                    .map_err(|e| OpenFangError::Internal(e.to_string()))?;

                let (sql, params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = match agent_id {
                    Some(aid) => (
                        "SELECT COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0),
                                COALESCE(SUM(cost_usd), 0.0), COUNT(*), COALESCE(SUM(tool_calls), 0)
                         FROM usage_events WHERE agent_id = ?1",
                        vec![Box::new(aid.0.to_string())],
                    ),
                    None => (
                        "SELECT COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0),
                                COALESCE(SUM(cost_usd), 0.0), COUNT(*), COALESCE(SUM(tool_calls), 0)
                         FROM usage_events",
                        vec![],
                    ),
                };

                let params_refs: Vec<&dyn rusqlite::types::ToSql> =
                    params.iter().map(|p| p.as_ref()).collect();

                let summary = conn
                    .query_row(sql, params_refs.as_slice(), |row| {
                        Ok(UsageSummary {
                            total_input_tokens: row.get::<_, i64>(0)? as u64,
                            total_output_tokens: row.get::<_, i64>(1)? as u64,
                            total_cost_usd: row.get(2)?,
                            call_count: row.get::<_, i64>(3)? as u64,
                            total_tool_calls: row.get::<_, i64>(4)? as u64,
                        })
                    })
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;

                Ok(summary)
            }
            UsageBackend::Mongo(store) => block_on(store.query_summary(agent_id)),
        }
    }

    /// Query usage grouped by model.
    pub fn query_by_model(&self) -> OpenFangResult<Vec<ModelUsage>> {
        match &self.backend {
            UsageBackend::Sqlite(conn) => {
                let conn = conn
                    .lock()
                    .map_err(|e| OpenFangError::Internal(e.to_string()))?;

                let mut stmt = conn
                    .prepare(
                        "SELECT model, COALESCE(SUM(cost_usd), 0.0), COALESCE(SUM(input_tokens), 0),
                                COALESCE(SUM(output_tokens), 0), COUNT(*)
                         FROM usage_events GROUP BY model ORDER BY SUM(cost_usd) DESC",
                    )
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;

                let rows = stmt
                    .query_map([], |row| {
                        Ok(ModelUsage {
                            model: row.get(0)?,
                            total_cost_usd: row.get(1)?,
                            total_input_tokens: row.get::<_, i64>(2)? as u64,
                            total_output_tokens: row.get::<_, i64>(3)? as u64,
                            call_count: row.get::<_, i64>(4)? as u64,
                        })
                    })
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;

                let mut results = Vec::new();
                for row in rows {
                    results.push(row.map_err(|e| OpenFangError::Memory(e.to_string()))?);
                }
                Ok(results)
            }
            UsageBackend::Mongo(store) => block_on(store.query_by_model()),
        }
    }

    /// Query daily usage breakdown for the last N days.
    pub fn query_daily_breakdown(&self, days: u32) -> OpenFangResult<Vec<DailyBreakdown>> {
        match &self.backend {
            UsageBackend::Sqlite(conn) => {
                let conn = conn
                    .lock()
                    .map_err(|e| OpenFangError::Internal(e.to_string()))?;

                let mut stmt = conn
                    .prepare(&format!(
                        "SELECT date(timestamp) as day,
                                    COALESCE(SUM(cost_usd), 0.0),
                                    COALESCE(SUM(input_tokens) + SUM(output_tokens), 0),
                                    COUNT(*)
                             FROM usage_events
                             WHERE timestamp > datetime('now', '-{days} days')
                             GROUP BY day
                             ORDER BY day ASC"
                    ))
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;

                let rows = stmt
                    .query_map([], |row| {
                        Ok(DailyBreakdown {
                            date: row.get(0)?,
                            cost_usd: row.get(1)?,
                            tokens: row.get::<_, i64>(2)? as u64,
                            calls: row.get::<_, i64>(3)? as u64,
                        })
                    })
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;

                let mut results = Vec::new();
                for row in rows {
                    results.push(row.map_err(|e| OpenFangError::Memory(e.to_string()))?);
                }
                Ok(results)
            }
            UsageBackend::Mongo(store) => block_on(store.query_daily_breakdown(days)),
        }
    }

    /// Query the timestamp of the earliest usage event.
    pub fn query_first_event_date(&self) -> OpenFangResult<Option<String>> {
        match &self.backend {
            UsageBackend::Sqlite(conn) => {
                let conn = conn
                    .lock()
                    .map_err(|e| OpenFangError::Internal(e.to_string()))?;
                let result: Option<String> = conn
                    .query_row("SELECT MIN(timestamp) FROM usage_events", [], |row| {
                        row.get(0)
                    })
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;
                Ok(result)
            }
            UsageBackend::Mongo(store) => block_on(store.query_first_event_date()),
        }
    }

    /// Query today's total cost across all agents.
    pub fn query_today_cost(&self) -> OpenFangResult<f64> {
        match &self.backend {
            UsageBackend::Sqlite(conn) => {
                let conn = conn
                    .lock()
                    .map_err(|e| OpenFangError::Internal(e.to_string()))?;
                let cost: f64 = conn
                    .query_row(
                        "SELECT COALESCE(SUM(cost_usd), 0.0) FROM usage_events
                         WHERE timestamp > datetime('now', 'start of day')",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;
                Ok(cost)
            }
            UsageBackend::Mongo(store) => block_on(store.query_today_cost()),
        }
    }

    /// Delete usage events older than the given number of days.
    pub fn cleanup_old(&self, days: u32) -> OpenFangResult<usize> {
        match &self.backend {
            UsageBackend::Sqlite(conn) => {
                let conn = conn
                    .lock()
                    .map_err(|e| OpenFangError::Internal(e.to_string()))?;
                let deleted = conn
                    .execute(
                        &format!(
                            "DELETE FROM usage_events WHERE timestamp < datetime('now', '-{days} days')"
                        ),
                        [],
                    )
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;
                Ok(deleted)
            }
            UsageBackend::Mongo(store) => block_on(store.cleanup_old(days)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::run_migrations;

    fn setup() -> UsageStore {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        UsageStore::new(Arc::new(Mutex::new(conn)))
    }

    #[test]
    fn test_record_and_query_summary() {
        let store = setup();
        let agent_id = AgentId::new();

        store
            .record(&UsageRecord {
                agent_id,
                model: "claude-haiku".to_string(),
                input_tokens: 100,
                output_tokens: 50,
                cost_usd: 0.001,
                tool_calls: 2,
            })
            .unwrap();

        store
            .record(&UsageRecord {
                agent_id,
                model: "claude-sonnet".to_string(),
                input_tokens: 500,
                output_tokens: 200,
                cost_usd: 0.01,
                tool_calls: 1,
            })
            .unwrap();

        let summary = store.query_summary(Some(agent_id)).unwrap();
        assert_eq!(summary.call_count, 2);
        assert_eq!(summary.total_input_tokens, 600);
        assert_eq!(summary.total_output_tokens, 250);
        assert!((summary.total_cost_usd - 0.011).abs() < 0.0001);
        assert_eq!(summary.total_tool_calls, 3);
    }

    #[test]
    fn test_query_summary_all_agents() {
        let store = setup();
        let a1 = AgentId::new();
        let a2 = AgentId::new();

        store
            .record(&UsageRecord {
                agent_id: a1,
                model: "haiku".to_string(),
                input_tokens: 100,
                output_tokens: 50,
                cost_usd: 0.001,
                tool_calls: 0,
            })
            .unwrap();

        store
            .record(&UsageRecord {
                agent_id: a2,
                model: "sonnet".to_string(),
                input_tokens: 200,
                output_tokens: 100,
                cost_usd: 0.005,
                tool_calls: 1,
            })
            .unwrap();

        let summary = store.query_summary(None).unwrap();
        assert_eq!(summary.call_count, 2);
        assert_eq!(summary.total_input_tokens, 300);
    }

    #[test]
    fn test_query_by_model() {
        let store = setup();
        let agent_id = AgentId::new();

        for _ in 0..3 {
            store
                .record(&UsageRecord {
                    agent_id,
                    model: "haiku".to_string(),
                    input_tokens: 100,
                    output_tokens: 50,
                    cost_usd: 0.001,
                    tool_calls: 0,
                })
                .unwrap();
        }

        store
            .record(&UsageRecord {
                agent_id,
                model: "sonnet".to_string(),
                input_tokens: 500,
                output_tokens: 200,
                cost_usd: 0.01,
                tool_calls: 1,
            })
            .unwrap();

        let by_model = store.query_by_model().unwrap();
        assert_eq!(by_model.len(), 2);
        // sonnet should be first (highest cost)
        assert_eq!(by_model[0].model, "sonnet");
        assert_eq!(by_model[1].model, "haiku");
        assert_eq!(by_model[1].call_count, 3);
    }

    #[test]
    fn test_query_hourly() {
        let store = setup();
        let agent_id = AgentId::new();

        store
            .record(&UsageRecord {
                agent_id,
                model: "haiku".to_string(),
                input_tokens: 100,
                output_tokens: 50,
                cost_usd: 0.05,
                tool_calls: 0,
            })
            .unwrap();

        let hourly = store.query_hourly(agent_id).unwrap();
        assert!((hourly - 0.05).abs() < 0.001);
    }

    #[test]
    fn test_query_daily() {
        let store = setup();
        let agent_id = AgentId::new();

        store
            .record(&UsageRecord {
                agent_id,
                model: "haiku".to_string(),
                input_tokens: 100,
                output_tokens: 50,
                cost_usd: 0.123,
                tool_calls: 0,
            })
            .unwrap();

        let daily = store.query_daily(agent_id).unwrap();
        assert!((daily - 0.123).abs() < 0.001);
    }

    #[test]
    fn test_cleanup_old() {
        let store = setup();
        let agent_id = AgentId::new();

        store
            .record(&UsageRecord {
                agent_id,
                model: "haiku".to_string(),
                input_tokens: 100,
                output_tokens: 50,
                cost_usd: 0.001,
                tool_calls: 0,
            })
            .unwrap();

        // Cleanup events older than 1 day should not remove today's events
        let deleted = store.cleanup_old(1).unwrap();
        assert_eq!(deleted, 0);

        let summary = store.query_summary(None).unwrap();
        assert_eq!(summary.call_count, 1);
    }

    #[test]
    fn test_empty_summary() {
        let store = setup();
        let summary = store.query_summary(None).unwrap();
        assert_eq!(summary.call_count, 0);
        assert_eq!(summary.total_cost_usd, 0.0);
    }
}
