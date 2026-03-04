//! Metering engine — tracks LLM cost and enforces spending quotas.

use openfang_memory::usage::{ModelUsage, UsageRecord, UsageStore, UsageSummary};
use openfang_types::agent::{AgentId, ResourceQuota};
use openfang_types::error::{OpenFangError, OpenFangResult};
use std::sync::Arc;

/// The metering engine tracks usage cost and enforces quota limits.
pub struct MeteringEngine {
    /// Persistent usage store (SQLite-backed).
    store: Arc<UsageStore>,
}

impl MeteringEngine {
    /// Create a new metering engine with the given usage store.
    pub fn new(store: Arc<UsageStore>) -> Self {
        Self { store }
    }

    /// Record a usage event (persists to SQLite).
    pub fn record(&self, record: &UsageRecord) -> OpenFangResult<()> {
        self.store.record(record)
    }

    /// Check if an agent is within its spending quotas (hourly, daily, monthly).
    /// Returns Ok(()) if under all quotas, or QuotaExceeded error if over any.
    pub fn check_quota(&self, agent_id: AgentId, quota: &ResourceQuota) -> OpenFangResult<()> {
        // Hourly check
        if quota.max_cost_per_hour_usd > 0.0 {
            let hourly_cost = self.store.query_hourly(agent_id)?;
            if hourly_cost >= quota.max_cost_per_hour_usd {
                return Err(OpenFangError::QuotaExceeded(format!(
                    "Agent {} exceeded hourly cost quota: ${:.4} / ${:.4}",
                    agent_id, hourly_cost, quota.max_cost_per_hour_usd
                )));
            }
        }

        // Daily check
        if quota.max_cost_per_day_usd > 0.0 {
            let daily_cost = self.store.query_daily(agent_id)?;
            if daily_cost >= quota.max_cost_per_day_usd {
                return Err(OpenFangError::QuotaExceeded(format!(
                    "Agent {} exceeded daily cost quota: ${:.4} / ${:.4}",
                    agent_id, daily_cost, quota.max_cost_per_day_usd
                )));
            }
        }

        // Monthly check
        if quota.max_cost_per_month_usd > 0.0 {
            let monthly_cost = self.store.query_monthly(agent_id)?;
            if monthly_cost >= quota.max_cost_per_month_usd {
                return Err(OpenFangError::QuotaExceeded(format!(
                    "Agent {} exceeded monthly cost quota: ${:.4} / ${:.4}",
                    agent_id, monthly_cost, quota.max_cost_per_month_usd
                )));
            }
        }

        Ok(())
    }

    /// Check global budget limits (across all agents).
    pub fn check_global_budget(
        &self,
        budget: &openfang_types::config::BudgetConfig,
    ) -> OpenFangResult<()> {
        if budget.max_hourly_usd > 0.0 {
            let cost = self.store.query_global_hourly()?;
            if cost >= budget.max_hourly_usd {
                return Err(OpenFangError::QuotaExceeded(format!(
                    "Global hourly budget exceeded: ${:.4} / ${:.4}",
                    cost, budget.max_hourly_usd
                )));
            }
        }

        if budget.max_daily_usd > 0.0 {
            let cost = self.store.query_today_cost()?;
            if cost >= budget.max_daily_usd {
                return Err(OpenFangError::QuotaExceeded(format!(
                    "Global daily budget exceeded: ${:.4} / ${:.4}",
                    cost, budget.max_daily_usd
                )));
            }
        }

        if budget.max_monthly_usd > 0.0 {
            let cost = self.store.query_global_monthly()?;
            if cost >= budget.max_monthly_usd {
                return Err(OpenFangError::QuotaExceeded(format!(
                    "Global monthly budget exceeded: ${:.4} / ${:.4}",
                    cost, budget.max_monthly_usd
                )));
            }
        }

        Ok(())
    }

    /// Get budget status — current spend vs limits for all time windows.
    pub fn budget_status(&self, budget: &openfang_types::config::BudgetConfig) -> BudgetStatus {
        let hourly = self.store.query_global_hourly().unwrap_or(0.0);
        let daily = self.store.query_today_cost().unwrap_or(0.0);
        let monthly = self.store.query_global_monthly().unwrap_or(0.0);

        BudgetStatus {
            hourly_spend: hourly,
            hourly_limit: budget.max_hourly_usd,
            hourly_pct: if budget.max_hourly_usd > 0.0 {
                hourly / budget.max_hourly_usd
            } else {
                0.0
            },
            daily_spend: daily,
            daily_limit: budget.max_daily_usd,
            daily_pct: if budget.max_daily_usd > 0.0 {
                daily / budget.max_daily_usd
            } else {
                0.0
            },
            monthly_spend: monthly,
            monthly_limit: budget.max_monthly_usd,
            monthly_pct: if budget.max_monthly_usd > 0.0 {
                monthly / budget.max_monthly_usd
            } else {
                0.0
            },
            alert_threshold: budget.alert_threshold,
        }
    }

    /// Get a usage summary, optionally filtered by agent.
    pub fn get_summary(&self, agent_id: Option<AgentId>) -> OpenFangResult<UsageSummary> {
        self.store.query_summary(agent_id)
    }

    /// Get usage grouped by model.
    pub fn get_by_model(&self) -> OpenFangResult<Vec<ModelUsage>> {
        self.store.query_by_model()
    }

    /// Estimate the cost of an LLM call based on model and token counts.
    ///
    /// Pricing table (approximate, per million tokens):
    ///
    /// | Model Family          | Input $/M | Output $/M |
    /// |-----------------------|-----------|------------|
    /// | claude-haiku          |     0.25  |      1.25  |
    /// | claude-sonnet-4-6     |     3.00  |     15.00  |
    /// | claude-opus-4-6       |     5.00  |     25.00  |
    /// | claude-opus (legacy)  |    15.00  |     75.00  |
    /// | gpt-5.2(-pro)         |     1.75  |     14.00  |
    /// | gpt-5(.1)             |     1.25  |     10.00  |
    /// | gpt-5-mini            |     0.25  |      2.00  |
    /// | gpt-5-nano            |     0.05  |      0.40  |
    /// | gpt-4o                |     2.50  |     10.00  |
    /// | gpt-4o-mini           |     0.15  |      0.60  |
    /// | gpt-4.1               |     2.00  |      8.00  |
    /// | gpt-4.1-mini          |     0.40  |      1.60  |
    /// | gpt-4.1-nano          |     0.10  |      0.40  |
    /// | o3-mini               |     1.10  |      4.40  |
    /// | gemini-3.1            |     2.50  |     15.00  |
    /// | gemini-3              |     0.50  |      3.00  |
    /// | gemini-2.5-flash-lite |     0.04  |      0.15  |
    /// | gemini-2.5-pro        |     1.25  |     10.00  |
    /// | gemini-2.5-flash      |     0.15  |      0.60  |
    /// | gemini-2.0-flash      |     0.10  |      0.40  |
    /// | deepseek-chat/v3      |     0.27  |      1.10  |
    /// | deepseek-reasoner/r1  |     0.55  |      2.19  |
    /// | llama-4-maverick      |     0.50  |      0.77  |
    /// | llama-4-scout         |     0.11  |      0.34  |
    /// | llama/mixtral (groq)  |     0.05  |      0.10  |
    /// | grok-4.1              |     0.20  |      0.50  |
    /// | grok-4                |     3.00  |     15.00  |
    /// | grok-3                |     3.00  |     15.00  |
    /// | qwen                  |     0.20  |      0.60  |
    /// | mistral-large         |     2.00  |      6.00  |
    /// | mistral-small         |     0.10  |      0.30  |
    /// | command-r-plus        |     2.50  |     10.00  |
    /// | Default (unknown)     |     1.00  |      3.00  |
    pub fn estimate_cost(model: &str, input_tokens: u64, output_tokens: u64) -> f64 {
        let model_lower = model.to_lowercase();
        let (input_per_m, output_per_m) = estimate_cost_rates(&model_lower);

        let input_cost = (input_tokens as f64 / 1_000_000.0) * input_per_m;
        let output_cost = (output_tokens as f64 / 1_000_000.0) * output_per_m;
        input_cost + output_cost
    }

    /// Estimate cost using the model catalog as the pricing source.
    ///
    /// Falls back to the default rate ($1/$3 per million) if the model is not
    /// found in the catalog.
    pub fn estimate_cost_with_catalog(
        catalog: &openfang_runtime::model_catalog::ModelCatalog,
        model: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) -> f64 {
        let (input_per_m, output_per_m) = catalog.pricing(model).unwrap_or((1.0, 3.0));
        let input_cost = (input_tokens as f64 / 1_000_000.0) * input_per_m;
        let output_cost = (output_tokens as f64 / 1_000_000.0) * output_per_m;
        input_cost + output_cost
    }

    /// Clean up old usage records.
    pub fn cleanup(&self, days: u32) -> OpenFangResult<usize> {
        self.store.cleanup_old(days)
    }
}

/// Budget status snapshot — current spend vs limits for all time windows.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BudgetStatus {
    pub hourly_spend: f64,
    pub hourly_limit: f64,
    pub hourly_pct: f64,
    pub daily_spend: f64,
    pub daily_limit: f64,
    pub daily_pct: f64,
    pub monthly_spend: f64,
    pub monthly_limit: f64,
    pub monthly_pct: f64,
    pub alert_threshold: f64,
}

/// Returns (input_per_million, output_per_million) pricing for a model.
///
/// Order matters: more specific patterns must come before generic ones
/// (e.g. "gpt-4o-mini" before "gpt-4o", "gpt-4.1-mini" before "gpt-4.1").
fn estimate_cost_rates(model: &str) -> (f64, f64) {
    // ── Anthropic ──────────────────────────────────────────────
    if model.contains("haiku") {
        return (0.25, 1.25);
    }
    if model.contains("opus-4-6") || model.contains("claude-opus-4-6") {
        return (5.0, 25.0);
    }
    if model.contains("opus") {
        return (15.0, 75.0);
    }
    if model.contains("sonnet-4-6") || model.contains("claude-sonnet-4-6") {
        return (3.0, 15.0);
    }
    if model.contains("sonnet") {
        return (3.0, 15.0);
    }

    // ── OpenAI ─────────────────────────────────────────────────
    if model.contains("gpt-5.2-pro") {
        return (1.75, 14.0);
    }
    if model.contains("gpt-5.2") {
        return (1.75, 14.0);
    }
    if model.contains("gpt-5.1") {
        return (1.25, 10.0);
    }
    if model.contains("gpt-5-nano") {
        return (0.05, 0.40);
    }
    if model.contains("gpt-5-mini") {
        return (0.25, 2.0);
    }
    if model.contains("gpt-5") {
        return (1.25, 10.0);
    }
    if model.contains("gpt-4o-mini") {
        return (0.15, 0.60);
    }
    if model.contains("gpt-4o") {
        return (2.50, 10.0);
    }
    if model.contains("gpt-4.1-nano") {
        return (0.10, 0.40);
    }
    if model.contains("gpt-4.1-mini") {
        return (0.40, 1.60);
    }
    if model.contains("gpt-4.1") {
        return (2.00, 8.00);
    }
    if model.contains("o4-mini") {
        return (1.10, 4.40);
    }
    if model.contains("o3-mini") {
        return (1.10, 4.40);
    }
    if model.contains("o3") {
        return (2.00, 8.00);
    }
    // Generic gpt-4 fallback
    if model.contains("gpt-4") {
        return (2.50, 10.0);
    }

    // ── Google Gemini ──────────────────────────────────────────
    if model.contains("gemini-3.1") {
        return (2.50, 15.0);
    }
    if model.contains("gemini-3") {
        return (0.50, 3.0);
    }
    if model.contains("gemini-2.5-flash-lite") {
        return (0.04, 0.15);
    }
    if model.contains("gemini-2.5-pro") {
        return (1.25, 10.0);
    }
    if model.contains("gemini-2.5-flash") {
        return (0.15, 0.60);
    }
    if model.contains("gemini-2.0-flash") || model.contains("gemini-flash") {
        return (0.10, 0.40);
    }
    // Generic gemini fallback
    if model.contains("gemini") {
        return (0.15, 0.60);
    }

    // ── DeepSeek ───────────────────────────────────────────────
    if model.contains("deepseek-reasoner") || model.contains("deepseek-r1") {
        return (0.55, 2.19);
    }
    if model.contains("deepseek") {
        return (0.27, 1.10);
    }

    // ── Cerebras (ultra-fast, cheap) ── must come before llama ─
    if model.contains("cerebras") {
        return (0.06, 0.06);
    }

    // ── SambaNova ── must come before llama ──────────────────────
    if model.contains("sambanova") {
        return (0.06, 0.06);
    }

    // ── Replicate ── must come before llama ─────────────────────
    if model.contains("replicate") {
        return (0.40, 0.40);
    }

    // ── Open-source (Groq, Together, etc.) ─────────────────────
    if model.contains("llama-4-maverick") {
        return (0.50, 0.77);
    }
    if model.contains("llama-4-scout") {
        return (0.11, 0.34);
    }
    if model.contains("llama") || model.contains("mixtral") {
        return (0.05, 0.10);
    }
    // ── Qwen (Alibaba) ──────────────────────────────────────────
    if model.contains("qwen-max") {
        return (4.00, 12.00);
    }
    if model.contains("qwen-vl") {
        return (1.50, 4.50);
    }
    if model.contains("qwen-plus") {
        return (0.80, 2.00);
    }
    if model.contains("qwen-turbo") {
        return (0.30, 0.60);
    }
    if model.contains("qwen") {
        return (0.20, 0.60);
    }

    // ── MiniMax ──────────────────────────────────────────────────
    if model.contains("minimax") {
        return (1.00, 3.00);
    }

    // ── Zhipu / GLM ─────────────────────────────────────────────
    if model.contains("glm-4-flash") {
        return (0.10, 0.10);
    }
    if model.contains("glm") {
        return (1.50, 5.00);
    }
    if model.contains("codegeex") {
        return (0.10, 0.10);
    }

    // ── Moonshot / Kimi ─────────────────────────────────────────
    if model.contains("moonshot") || model.contains("kimi") {
        return (0.80, 0.80);
    }

    // ── Baidu ERNIE ─────────────────────────────────────────────
    if model.contains("ernie") {
        return (2.00, 6.00);
    }

    // ── AWS Bedrock ─────────────────────────────────────────────
    if model.contains("nova-pro") {
        return (0.80, 3.20);
    }
    if model.contains("nova-lite") {
        return (0.06, 0.24);
    }

    // ── Mistral ────────────────────────────────────────────────
    if model.contains("mistral-large") {
        return (2.00, 6.00);
    }
    if model.contains("mistral-small") || model.contains("mistral") {
        return (0.10, 0.30);
    }

    // ── Cohere ─────────────────────────────────────────────────
    if model.contains("command-r-plus") {
        return (2.50, 10.0);
    }
    if model.contains("command-r") {
        return (0.15, 0.60);
    }

    // ── Perplexity ──────────────────────────────────────────────
    if model.contains("sonar-pro") {
        return (3.0, 15.0);
    }
    if model.contains("sonar") {
        return (1.0, 5.0);
    }

    // ── xAI / Grok ──────────────────────────────────────────────
    if model.contains("grok-4-1") {
        return (0.20, 0.50);
    }
    if model.contains("grok-4") {
        return (3.0, 15.0);
    }
    if model.contains("grok-3-mini") || model.contains("grok-2-mini") || model.contains("grok-mini")
    {
        return (0.30, 0.50);
    }
    if model.contains("grok-3") {
        return (3.0, 15.0);
    }
    if model.contains("grok") {
        return (2.0, 10.0);
    }

    // ── AI21 / Jamba ────────────────────────────────────────────
    if model.contains("jamba") {
        return (2.0, 8.0);
    }

    // ── Default (conservative) ─────────────────────────────────
    (1.0, 3.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_memory::MemorySubstrate;

    fn setup() -> MeteringEngine {
        let substrate = MemorySubstrate::open_in_memory(0.1).unwrap();
        let store = Arc::new(UsageStore::new(substrate.usage_conn()));
        MeteringEngine::new(store)
    }

    #[test]
    fn test_record_and_check_quota_under() {
        let engine = setup();
        let agent_id = AgentId::new();
        let quota = ResourceQuota {
            max_cost_per_hour_usd: 1.0,
            ..Default::default()
        };

        engine
            .record(&UsageRecord {
                agent_id,
                model: "claude-haiku".to_string(),
                input_tokens: 100,
                output_tokens: 50,
                cost_usd: 0.001,
                tool_calls: 0,
            })
            .unwrap();

        assert!(engine.check_quota(agent_id, &quota).is_ok());
    }

    #[test]
    fn test_check_quota_exceeded() {
        let engine = setup();
        let agent_id = AgentId::new();
        let quota = ResourceQuota {
            max_cost_per_hour_usd: 0.01,
            ..Default::default()
        };

        engine
            .record(&UsageRecord {
                agent_id,
                model: "claude-sonnet".to_string(),
                input_tokens: 10000,
                output_tokens: 5000,
                cost_usd: 0.05,
                tool_calls: 0,
            })
            .unwrap();

        let result = engine.check_quota(agent_id, &quota);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("exceeded hourly cost quota"));
    }

    #[test]
    fn test_check_quota_zero_limit_skipped() {
        let engine = setup();
        let agent_id = AgentId::new();
        let quota = ResourceQuota {
            max_cost_per_hour_usd: 0.0,
            ..Default::default()
        };

        // Even with high usage, a zero limit means no enforcement
        engine
            .record(&UsageRecord {
                agent_id,
                model: "claude-opus".to_string(),
                input_tokens: 100000,
                output_tokens: 50000,
                cost_usd: 100.0,
                tool_calls: 0,
            })
            .unwrap();

        assert!(engine.check_quota(agent_id, &quota).is_ok());
    }

    #[test]
    fn test_estimate_cost_haiku() {
        let cost = MeteringEngine::estimate_cost("claude-haiku-4-5-20251001", 1_000_000, 1_000_000);
        assert!((cost - 1.50).abs() < 0.01); // $0.25 + $1.25
    }

    #[test]
    fn test_estimate_cost_sonnet() {
        let cost = MeteringEngine::estimate_cost("claude-sonnet-4-20250514", 1_000_000, 1_000_000);
        assert!((cost - 18.0).abs() < 0.01); // $3.00 + $15.00
    }

    #[test]
    fn test_estimate_cost_opus() {
        let cost = MeteringEngine::estimate_cost("claude-opus-4-20250514", 1_000_000, 1_000_000);
        assert!((cost - 90.0).abs() < 0.01); // $15.00 + $75.00
    }

    #[test]
    fn test_estimate_cost_gpt4o() {
        let cost = MeteringEngine::estimate_cost("gpt-4o-2024-11-20", 1_000_000, 1_000_000);
        assert!((cost - 12.50).abs() < 0.01); // $2.50 + $10.00
    }

    #[test]
    fn test_estimate_cost_gpt4o_mini() {
        let cost = MeteringEngine::estimate_cost("gpt-4o-mini", 1_000_000, 1_000_000);
        assert!((cost - 0.75).abs() < 0.01); // $0.15 + $0.60
    }

    #[test]
    fn test_estimate_cost_gpt41() {
        let cost = MeteringEngine::estimate_cost("gpt-4.1", 1_000_000, 1_000_000);
        assert!((cost - 10.0).abs() < 0.01); // $2.00 + $8.00
    }

    #[test]
    fn test_estimate_cost_gpt41_mini() {
        let cost = MeteringEngine::estimate_cost("gpt-4.1-mini", 1_000_000, 1_000_000);
        assert!((cost - 2.0).abs() < 0.01); // $0.40 + $1.60
    }

    #[test]
    fn test_estimate_cost_gpt41_nano() {
        let cost = MeteringEngine::estimate_cost("gpt-4.1-nano", 1_000_000, 1_000_000);
        assert!((cost - 0.50).abs() < 0.01); // $0.10 + $0.40
    }

    #[test]
    fn test_estimate_cost_o3_mini() {
        let cost = MeteringEngine::estimate_cost("o3-mini", 1_000_000, 1_000_000);
        assert!((cost - 5.50).abs() < 0.01); // $1.10 + $4.40
    }

    #[test]
    fn test_estimate_cost_gemini_20_flash() {
        let cost = MeteringEngine::estimate_cost("gemini-2.0-flash", 1_000_000, 1_000_000);
        assert!((cost - 0.50).abs() < 0.01); // $0.10 + $0.40
    }

    #[test]
    fn test_estimate_cost_gemini_25_pro() {
        let cost = MeteringEngine::estimate_cost("gemini-2.5-pro", 1_000_000, 1_000_000);
        assert!((cost - 11.25).abs() < 0.01); // $1.25 + $10.00
    }

    #[test]
    fn test_estimate_cost_gemini_25_flash() {
        let cost = MeteringEngine::estimate_cost("gemini-2.5-flash", 1_000_000, 1_000_000);
        assert!((cost - 0.75).abs() < 0.01); // $0.15 + $0.60
    }

    #[test]
    fn test_estimate_cost_deepseek_chat() {
        let cost = MeteringEngine::estimate_cost("deepseek-chat", 1_000_000, 1_000_000);
        assert!((cost - 1.37).abs() < 0.01); // $0.27 + $1.10
    }

    #[test]
    fn test_estimate_cost_deepseek_reasoner() {
        let cost = MeteringEngine::estimate_cost("deepseek-reasoner", 1_000_000, 1_000_000);
        assert!((cost - 2.74).abs() < 0.01); // $0.55 + $2.19
    }

    #[test]
    fn test_estimate_cost_llama() {
        let cost = MeteringEngine::estimate_cost("llama-3.3-70b-versatile", 1_000_000, 1_000_000);
        assert!((cost - 0.15).abs() < 0.01); // $0.05 + $0.10
    }

    #[test]
    fn test_estimate_cost_mixtral() {
        let cost = MeteringEngine::estimate_cost("mixtral-8x7b", 1_000_000, 1_000_000);
        assert!((cost - 0.15).abs() < 0.01); // $0.05 + $0.10
    }

    #[test]
    fn test_estimate_cost_qwen() {
        let cost = MeteringEngine::estimate_cost("qwen-2.5-72b", 1_000_000, 1_000_000);
        assert!((cost - 0.80).abs() < 0.01); // $0.20 + $0.60
    }

    #[test]
    fn test_estimate_cost_mistral_large() {
        let cost = MeteringEngine::estimate_cost("mistral-large-latest", 1_000_000, 1_000_000);
        assert!((cost - 8.0).abs() < 0.01); // $2.00 + $6.00
    }

    #[test]
    fn test_estimate_cost_mistral_small() {
        let cost = MeteringEngine::estimate_cost("mistral-small-latest", 1_000_000, 1_000_000);
        assert!((cost - 0.40).abs() < 0.01); // $0.10 + $0.30
    }

    #[test]
    fn test_estimate_cost_command_r_plus() {
        let cost = MeteringEngine::estimate_cost("command-r-plus", 1_000_000, 1_000_000);
        assert!((cost - 12.50).abs() < 0.01); // $2.50 + $10.00
    }

    #[test]
    fn test_estimate_cost_unknown() {
        let cost = MeteringEngine::estimate_cost("my-custom-model", 1_000_000, 1_000_000);
        assert!((cost - 4.0).abs() < 0.01); // $1.00 + $3.00
    }

    #[test]
    fn test_estimate_cost_grok() {
        let cost = MeteringEngine::estimate_cost("grok-2", 1_000_000, 1_000_000);
        assert!((cost - 12.0).abs() < 0.01); // $2.00 + $10.00
    }

    #[test]
    fn test_estimate_cost_grok_mini() {
        let cost = MeteringEngine::estimate_cost("grok-2-mini", 1_000_000, 1_000_000);
        assert!((cost - 0.80).abs() < 0.01); // $0.30 + $0.50
    }

    #[test]
    fn test_estimate_cost_sonar_pro() {
        let cost = MeteringEngine::estimate_cost("sonar-pro", 1_000_000, 1_000_000);
        assert!((cost - 18.0).abs() < 0.01); // $3.00 + $15.00
    }

    #[test]
    fn test_estimate_cost_jamba() {
        let cost = MeteringEngine::estimate_cost("jamba-1.5-large", 1_000_000, 1_000_000);
        assert!((cost - 10.0).abs() < 0.01); // $2.00 + $8.00
    }

    #[test]
    fn test_estimate_cost_cerebras() {
        let cost = MeteringEngine::estimate_cost("cerebras/llama3.3-70b", 1_000_000, 1_000_000);
        assert!((cost - 0.12).abs() < 0.01); // $0.06 + $0.06
    }

    #[test]
    fn test_estimate_cost_with_catalog() {
        let catalog = openfang_runtime::model_catalog::ModelCatalog::new();
        // Sonnet: $3/M input, $15/M output
        let cost = MeteringEngine::estimate_cost_with_catalog(
            &catalog,
            "claude-sonnet-4-20250514",
            1_000_000,
            1_000_000,
        );
        assert!((cost - 18.0).abs() < 0.01);
    }

    #[test]
    fn test_estimate_cost_with_catalog_alias() {
        let catalog = openfang_runtime::model_catalog::ModelCatalog::new();
        // "sonnet" alias should resolve to same pricing
        let cost =
            MeteringEngine::estimate_cost_with_catalog(&catalog, "sonnet", 1_000_000, 1_000_000);
        assert!((cost - 18.0).abs() < 0.01);
    }

    #[test]
    fn test_estimate_cost_with_catalog_unknown_uses_default() {
        let catalog = openfang_runtime::model_catalog::ModelCatalog::new();
        // Unknown model falls back to $1/$3
        let cost = MeteringEngine::estimate_cost_with_catalog(
            &catalog,
            "totally-unknown-model",
            1_000_000,
            1_000_000,
        );
        assert!((cost - 4.0).abs() < 0.01);
    }

    #[test]
    fn test_get_summary() {
        let engine = setup();
        let agent_id = AgentId::new();

        engine
            .record(&UsageRecord {
                agent_id,
                model: "haiku".to_string(),
                input_tokens: 500,
                output_tokens: 200,
                cost_usd: 0.005,
                tool_calls: 3,
            })
            .unwrap();

        let summary = engine.get_summary(Some(agent_id)).unwrap();
        assert_eq!(summary.call_count, 1);
        assert_eq!(summary.total_input_tokens, 500);
    }
}
