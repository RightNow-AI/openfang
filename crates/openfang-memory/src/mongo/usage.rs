//! MongoDB usage tracking store — records LLM usage events for cost monitoring.

use bson::doc;
use chrono::{Datelike, Utc};
use futures::TryStreamExt;
use mongodb::Collection;
use openfang_types::agent::AgentId;
use openfang_types::error::{OpenFangError, OpenFangResult};

use crate::usage::{DailyBreakdown, ModelUsage, UsageRecord, UsageSummary};

/// Usage store backed by MongoDB.
#[derive(Clone)]
pub struct MongoUsageStore {
    events: Collection<bson::Document>,
}

impl MongoUsageStore {
    pub fn new(db: mongodb::Database) -> Self {
        Self {
            events: db.collection("usage_events"),
        }
    }

    /// Record a usage event.
    pub async fn record(&self, record: &UsageRecord) -> OpenFangResult<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = bson::DateTime::from_chrono(Utc::now());
        let doc = doc! {
            "_id": &id,
            "agent_id": record.agent_id.0.to_string(),
            "timestamp": now,
            "model": &record.model,
            "input_tokens": record.input_tokens as i64,
            "output_tokens": record.output_tokens as i64,
            "cost_usd": record.cost_usd,
            "tool_calls": record.tool_calls as i64,
        };
        self.events
            .insert_one(doc)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Query total cost in the last hour for an agent.
    pub async fn query_hourly(&self, agent_id: AgentId) -> OpenFangResult<f64> {
        let cutoff =
            bson::DateTime::from_chrono(Utc::now() - chrono::Duration::hours(1));
        self.sum_cost(doc! { "agent_id": agent_id.0.to_string(), "timestamp": { "$gt": cutoff } })
            .await
    }

    /// Query total cost today for an agent.
    pub async fn query_daily(&self, agent_id: AgentId) -> OpenFangResult<f64> {
        let today = Utc::now().date_naive().and_hms_opt(0, 0, 0).unwrap();
        let cutoff = bson::DateTime::from_chrono(
            chrono::DateTime::<Utc>::from_naive_utc_and_offset(today, Utc),
        );
        self.sum_cost(doc! { "agent_id": agent_id.0.to_string(), "timestamp": { "$gte": cutoff } })
            .await
    }

    /// Query total cost in the current calendar month for an agent.
    pub async fn query_monthly(&self, agent_id: AgentId) -> OpenFangResult<f64> {
        let now = Utc::now();
        let first_of_month = now.date_naive().with_day(1).unwrap().and_hms_opt(0, 0, 0).unwrap();
        let cutoff = bson::DateTime::from_chrono(
            chrono::DateTime::<Utc>::from_naive_utc_and_offset(first_of_month, Utc),
        );
        self.sum_cost(doc! { "agent_id": agent_id.0.to_string(), "timestamp": { "$gte": cutoff } })
            .await
    }

    /// Query total cost across all agents for the current hour.
    pub async fn query_global_hourly(&self) -> OpenFangResult<f64> {
        let cutoff =
            bson::DateTime::from_chrono(Utc::now() - chrono::Duration::hours(1));
        self.sum_cost(doc! { "timestamp": { "$gt": cutoff } }).await
    }

    /// Query total cost across all agents for the current calendar month.
    pub async fn query_global_monthly(&self) -> OpenFangResult<f64> {
        let now = Utc::now();
        let first_of_month = now.date_naive().with_day(1).unwrap().and_hms_opt(0, 0, 0).unwrap();
        let cutoff = bson::DateTime::from_chrono(
            chrono::DateTime::<Utc>::from_naive_utc_and_offset(first_of_month, Utc),
        );
        self.sum_cost(doc! { "timestamp": { "$gte": cutoff } }).await
    }

    /// Query usage summary, optionally filtered by agent.
    pub async fn query_summary(&self, agent_id: Option<AgentId>) -> OpenFangResult<UsageSummary> {
        let match_stage = match agent_id {
            Some(aid) => doc! { "$match": { "agent_id": aid.0.to_string() } },
            None => doc! { "$match": {} },
        };
        let group_stage = doc! {
            "$group": {
                "_id": bson::Bson::Null,
                "total_input_tokens": { "$sum": "$input_tokens" },
                "total_output_tokens": { "$sum": "$output_tokens" },
                "total_cost_usd": { "$sum": "$cost_usd" },
                "call_count": { "$sum": 1 },
                "total_tool_calls": { "$sum": "$tool_calls" },
            }
        };

        let mut cursor = self
            .events
            .aggregate(vec![match_stage, group_stage])
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        if let Some(d) = cursor
            .try_next()
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?
        {
            Ok(UsageSummary {
                total_input_tokens: d.get_i64("total_input_tokens").unwrap_or(0) as u64,
                total_output_tokens: d.get_i64("total_output_tokens").unwrap_or(0) as u64,
                total_cost_usd: d.get_f64("total_cost_usd").unwrap_or(0.0),
                call_count: d.get_i32("call_count").unwrap_or(0) as u64,
                total_tool_calls: d.get_i64("total_tool_calls").unwrap_or(0) as u64,
            })
        } else {
            Ok(UsageSummary {
                total_input_tokens: 0,
                total_output_tokens: 0,
                total_cost_usd: 0.0,
                call_count: 0,
                total_tool_calls: 0,
            })
        }
    }

    /// Query usage grouped by model.
    pub async fn query_by_model(&self) -> OpenFangResult<Vec<ModelUsage>> {
        let pipeline = vec![
            doc! {
                "$group": {
                    "_id": "$model",
                    "total_cost_usd": { "$sum": "$cost_usd" },
                    "total_input_tokens": { "$sum": "$input_tokens" },
                    "total_output_tokens": { "$sum": "$output_tokens" },
                    "call_count": { "$sum": 1 },
                }
            },
            doc! { "$sort": { "total_cost_usd": -1 } },
        ];

        let mut cursor = self
            .events
            .aggregate(pipeline)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut results = Vec::new();
        while let Some(d) = cursor
            .try_next()
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?
        {
            results.push(ModelUsage {
                model: d.get_str("_id").unwrap_or("unknown").to_string(),
                total_cost_usd: d.get_f64("total_cost_usd").unwrap_or(0.0),
                total_input_tokens: d.get_i64("total_input_tokens").unwrap_or(0) as u64,
                total_output_tokens: d.get_i64("total_output_tokens").unwrap_or(0) as u64,
                call_count: d.get_i32("call_count").unwrap_or(0) as u64,
            });
        }
        Ok(results)
    }

    /// Query daily usage breakdown for the last N days.
    pub async fn query_daily_breakdown(&self, days: u32) -> OpenFangResult<Vec<DailyBreakdown>> {
        let cutoff = bson::DateTime::from_chrono(
            Utc::now() - chrono::Duration::days(days as i64),
        );
        let pipeline = vec![
            doc! { "$match": { "timestamp": { "$gt": cutoff } } },
            doc! {
                "$group": {
                    "_id": { "$dateToString": { "format": "%Y-%m-%d", "date": "$timestamp" } },
                    "cost_usd": { "$sum": "$cost_usd" },
                    "tokens": { "$sum": { "$add": ["$input_tokens", "$output_tokens"] } },
                    "calls": { "$sum": 1 },
                }
            },
            doc! { "$sort": { "_id": 1 } },
        ];

        let mut cursor = self
            .events
            .aggregate(pipeline)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut results = Vec::new();
        while let Some(d) = cursor
            .try_next()
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?
        {
            results.push(DailyBreakdown {
                date: d.get_str("_id").unwrap_or("").to_string(),
                cost_usd: d.get_f64("cost_usd").unwrap_or(0.0),
                tokens: d.get_i64("tokens").unwrap_or(0) as u64,
                calls: d.get_i32("calls").unwrap_or(0) as u64,
            });
        }
        Ok(results)
    }

    /// Query the timestamp of the earliest usage event.
    pub async fn query_first_event_date(&self) -> OpenFangResult<Option<String>> {
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "timestamp": 1 })
            .limit(1)
            .projection(doc! { "timestamp": 1 })
            .build();
        let mut cursor = self
            .events
            .find(doc! {})
            .with_options(opts)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        if let Some(d) = cursor
            .try_next()
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?
        {
            let ts = d
                .get_datetime("timestamp")
                .ok()
                .map(|dt| dt.to_chrono().to_rfc3339());
            Ok(ts)
        } else {
            Ok(None)
        }
    }

    /// Query today's total cost across all agents.
    pub async fn query_today_cost(&self) -> OpenFangResult<f64> {
        let today = Utc::now().date_naive().and_hms_opt(0, 0, 0).unwrap();
        let cutoff = bson::DateTime::from_chrono(
            chrono::DateTime::<Utc>::from_naive_utc_and_offset(today, Utc),
        );
        self.sum_cost(doc! { "timestamp": { "$gte": cutoff } }).await
    }

    /// Delete usage events older than the given number of days.
    pub async fn cleanup_old(&self, days: u32) -> OpenFangResult<usize> {
        let cutoff = bson::DateTime::from_chrono(
            Utc::now() - chrono::Duration::days(days as i64),
        );
        let result = self
            .events
            .delete_many(doc! { "timestamp": { "$lt": cutoff } })
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(result.deleted_count as usize)
    }

    /// Helper: sum cost_usd matching a filter using aggregation.
    async fn sum_cost(&self, filter: bson::Document) -> OpenFangResult<f64> {
        let pipeline = vec![
            doc! { "$match": filter },
            doc! {
                "$group": {
                    "_id": bson::Bson::Null,
                    "total": { "$sum": "$cost_usd" },
                }
            },
        ];
        let mut cursor = self
            .events
            .aggregate(pipeline)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        if let Some(d) = cursor
            .try_next()
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?
        {
            Ok(d.get_f64("total").unwrap_or(0.0))
        } else {
            Ok(0.0)
        }
    }
}
