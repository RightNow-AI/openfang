//! PostgreSQL implementation of the usage tracking store.

use crate::backends::UsageBackend;
use crate::usage::{DailyBreakdown, ModelUsage, UsageRecord, UsageSummary};
use deadpool_postgres::Pool;
use openfang_types::agent::AgentId;
use openfang_types::error::{OpenFangError, OpenFangResult};

pub struct PgUsageStore {
    pool: Pool,
}

impl PgUsageStore {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    fn block_on_pg<F, T>(&self, f: F) -> OpenFangResult<T>
    where
        F: std::future::Future<Output = OpenFangResult<T>>,
    {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(f)
        })
    }
}

impl UsageBackend for PgUsageStore {
    fn record(&self, record: &UsageRecord) -> OpenFangResult<()> {
        let record = record.clone();
        self.block_on_pg(async {
            let client = self.pool.get().await
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            client.execute(
                "INSERT INTO usage_events (agent_id, model, input_tokens, output_tokens, cost_usd, tool_calls)
                 VALUES ($1, $2, $3, $4, $5, $6)",
                &[
                    &record.agent_id.0.to_string(), &record.model,
                    &(record.input_tokens as i64), &(record.output_tokens as i64),
                    &record.cost_usd, &(record.tool_calls as i64),
                ],
            ).await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(())
        })
    }

    fn query_hourly(&self, agent_id: AgentId) -> OpenFangResult<f64> {
        self.block_on_pg(async {
            let client = self.pool.get().await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let row = client.query_one(
                "SELECT COALESCE(SUM(cost_usd), 0) FROM usage_events WHERE agent_id = $1 AND timestamp > NOW() - INTERVAL '1 hour'",
                &[&agent_id.0.to_string()],
            ).await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(row.get(0))
        })
    }

    fn query_daily(&self, agent_id: AgentId) -> OpenFangResult<f64> {
        self.block_on_pg(async {
            let client = self.pool.get().await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let row = client.query_one(
                "SELECT COALESCE(SUM(cost_usd), 0) FROM usage_events WHERE agent_id = $1 AND timestamp >= CURRENT_DATE",
                &[&agent_id.0.to_string()],
            ).await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(row.get(0))
        })
    }

    fn query_monthly(&self, agent_id: AgentId) -> OpenFangResult<f64> {
        self.block_on_pg(async {
            let client = self.pool.get().await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let row = client.query_one(
                "SELECT COALESCE(SUM(cost_usd), 0) FROM usage_events WHERE agent_id = $1 AND timestamp >= date_trunc('month', CURRENT_DATE)",
                &[&agent_id.0.to_string()],
            ).await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(row.get(0))
        })
    }

    fn query_global_hourly(&self) -> OpenFangResult<f64> {
        self.block_on_pg(async {
            let client = self.pool.get().await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let row = client.query_one(
                "SELECT COALESCE(SUM(cost_usd), 0) FROM usage_events WHERE timestamp > NOW() - INTERVAL '1 hour'", &[],
            ).await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(row.get(0))
        })
    }

    fn query_global_monthly(&self) -> OpenFangResult<f64> {
        self.block_on_pg(async {
            let client = self.pool.get().await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let row = client.query_one(
                "SELECT COALESCE(SUM(cost_usd), 0) FROM usage_events WHERE timestamp >= date_trunc('month', CURRENT_DATE)", &[],
            ).await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(row.get(0))
        })
    }

    fn query_summary(&self, agent_id: Option<AgentId>) -> OpenFangResult<UsageSummary> {
        self.block_on_pg(async {
            let client = self.pool.get().await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let (sql, params): (&str, Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>>) = match agent_id {
                Some(id) => (
                    "SELECT COALESCE(SUM(input_tokens),0)::bigint, COALESCE(SUM(output_tokens),0)::bigint, COALESCE(SUM(cost_usd),0)::float8, COUNT(*)::bigint, COALESCE(SUM(tool_calls),0)::bigint FROM usage_events WHERE agent_id = $1",
                    vec![Box::new(id.0.to_string())],
                ),
                None => (
                    "SELECT COALESCE(SUM(input_tokens),0)::bigint, COALESCE(SUM(output_tokens),0)::bigint, COALESCE(SUM(cost_usd),0)::float8, COUNT(*)::bigint, COALESCE(SUM(tool_calls),0)::bigint FROM usage_events",
                    vec![],
                ),
            };
            let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = params.iter().map(|b| b.as_ref() as _).collect();
            let row = client.query_one(sql, &param_refs).await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(UsageSummary {
                total_input_tokens: row.get::<_, i64>(0) as u64,
                total_output_tokens: row.get::<_, i64>(1) as u64,
                total_cost_usd: row.get(2),
                call_count: row.get::<_, i64>(3) as u64,
                total_tool_calls: row.get::<_, i64>(4) as u64,
            })
        })
    }

    fn query_by_model(&self) -> OpenFangResult<Vec<ModelUsage>> {
        self.block_on_pg(async {
            let client = self.pool.get().await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let rows = client.query(
                "SELECT model, SUM(cost_usd)::float8, SUM(input_tokens)::bigint, SUM(output_tokens)::bigint, COUNT(*)::bigint
                 FROM usage_events GROUP BY model ORDER BY SUM(cost_usd) DESC", &[],
            ).await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(rows.iter().map(|r| ModelUsage {
                model: r.get(0),
                total_cost_usd: r.get(1),
                total_input_tokens: r.get::<_, i64>(2) as u64,
                total_output_tokens: r.get::<_, i64>(3) as u64,
                call_count: r.get::<_, i64>(4) as u64,
            }).collect())
        })
    }

    fn query_daily_breakdown(&self, days: u32) -> OpenFangResult<Vec<DailyBreakdown>> {
        self.block_on_pg(async {
            let client = self.pool.get().await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let rows = client.query(
                "SELECT timestamp::date::text, SUM(cost_usd)::float8, SUM(input_tokens + output_tokens)::bigint, COUNT(*)::bigint
                 FROM usage_events WHERE timestamp >= CURRENT_DATE - $1::integer * INTERVAL '1 day'
                 GROUP BY timestamp::date ORDER BY timestamp::date DESC",
                &[&(days as i32)],
            ).await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(rows.iter().map(|r| DailyBreakdown {
                date: r.get(0),
                cost_usd: r.get(1),
                tokens: r.get::<_, i64>(2) as u64,
                calls: r.get::<_, i64>(3) as u64,
            }).collect())
        })
    }

    fn query_first_event_date(&self) -> OpenFangResult<Option<String>> {
        self.block_on_pg(async {
            let client = self.pool.get().await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let row = client.query_opt("SELECT MIN(timestamp)::text FROM usage_events", &[])
                .await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(row.and_then(|r| r.get(0)))
        })
    }

    fn query_today_cost(&self) -> OpenFangResult<f64> {
        self.block_on_pg(async {
            let client = self.pool.get().await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let row = client.query_one(
                "SELECT COALESCE(SUM(cost_usd), 0) FROM usage_events WHERE timestamp >= CURRENT_DATE", &[],
            ).await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(row.get(0))
        })
    }

    fn cleanup_old(&self, days: u32) -> OpenFangResult<usize> {
        self.block_on_pg(async {
            let client = self.pool.get().await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            let deleted = client.execute(
                "DELETE FROM usage_events WHERE timestamp < NOW() - $1::integer * INTERVAL '1 day'",
                &[&(days as i32)],
            ).await.map_err(|e| OpenFangError::Memory(e.to_string()))?;
            Ok(deleted as usize)
        })
    }
}
