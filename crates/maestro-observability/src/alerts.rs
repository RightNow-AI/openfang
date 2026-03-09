//! Alert engine — rule evaluation and notification dispatch.

use crate::{AlertAction, AlertCondition, AlertRule};
use crate::traces::TraceStore;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// A fired alert event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertEvent {
    pub rule_id: String,
    pub rule_name: String,
    pub fired_at: DateTime<Utc>,
    pub condition_value: f64,
    pub message: String,
}

/// The alert engine — manages rules and evaluates them against the trace store.
#[derive(Clone)]
pub struct AlertEngine {
    rules: Arc<RwLock<Vec<AlertRule>>>,
    history: Arc<RwLock<Vec<AlertEvent>>>,
    cooldowns: Arc<DashMap<String, DateTime<Utc>>>,
    cooldown_secs: u64,
    http: reqwest::Client,
}

impl AlertEngine {
    pub fn new(cooldown_secs: u64) -> Self {
        Self {
            rules: Arc::new(RwLock::new(Vec::new())),
            history: Arc::new(RwLock::new(Vec::new())),
            cooldowns: Arc::new(DashMap::new()),
            cooldown_secs,
            http: reqwest::Client::new(),
        }
    }

    pub async fn upsert_rule(&self, rule: AlertRule) {
        let mut rules = self.rules.write().await;
        if let Some(pos) = rules.iter().position(|r| r.id == rule.id) {
            rules[pos] = rule;
        } else {
            rules.push(rule);
        }
    }

    pub async fn remove_rule(&self, rule_id: &str) {
        let mut rules = self.rules.write().await;
        rules.retain(|r| r.id != rule_id);
    }

    pub async fn rules(&self) -> Vec<AlertRule> {
        self.rules.read().await.clone()
    }

    pub async fn history(&self, limit: usize) -> Vec<AlertEvent> {
        let h = self.history.read().await;
        h.iter().rev().take(limit).cloned().collect()
    }

    pub async fn evaluate(&self, store: &TraceStore) -> Vec<AlertEvent> {
        let rules = self.rules.read().await.clone();
        let mut fired = Vec::new();
        for rule in rules.iter().filter(|r| r.enabled) {
            if self.is_in_cooldown(&rule.id) { continue; }
            let (triggered, value, message) = match &rule.condition {
                AlertCondition::ErrorRate { threshold, window_secs } => {
                    let rate = store.error_rate(*window_secs).await;
                    (rate > *threshold, rate, format!("Error rate {:.1}% exceeds threshold {:.1}%", rate * 100.0, threshold * 100.0))
                }
                AlertCondition::LatencyExceeded { threshold_ms } => {
                    let p99 = store.p99_latency_ms(300).await;
                    (p99 > *threshold_ms, p99 as f64, format!("P99 latency {}ms exceeds threshold {}ms", p99, threshold_ms))
                }
                AlertCondition::CostExceeded { budget_usd, period_secs } => {
                    let cost = store.total_cost_usd(*period_secs).await;
                    (cost > *budget_usd, cost, format!("Cost ${:.4} exceeds budget ${:.4}", cost, budget_usd))
                }
                AlertCondition::TokenLimit { limit, period_secs } => {
                    let tokens = store.total_tokens(*period_secs).await;
                    (tokens > *limit, tokens as f64, format!("Token usage {} exceeds limit {}", tokens, limit))
                }
            };
            if triggered {
                let event = AlertEvent {
                    rule_id: rule.id.clone(),
                    rule_name: rule.name.clone(),
                    fired_at: Utc::now(),
                    condition_value: value,
                    message: message.clone(),
                };
                warn!(rule_id = %rule.id, "Alert fired: {}", message);
                self.execute_action(&rule.action, &event).await;
                self.cooldowns.insert(rule.id.clone(), Utc::now());
                let mut h = self.history.write().await;
                h.push(event.clone());
                if h.len() > 1000 { h.drain(0..100); }
                drop(h);
                fired.push(event);
            }
        }
        fired
    }

    fn is_in_cooldown(&self, rule_id: &str) -> bool {
        if let Some(last_fired) = self.cooldowns.get(rule_id) {
            (Utc::now() - *last_fired).num_seconds() < self.cooldown_secs as i64
        } else { false }
    }

    async fn execute_action(&self, action: &AlertAction, event: &AlertEvent) {
        match action {
            AlertAction::Log => {
                info!(rule_id = %event.rule_id, message = %event.message, "Alert action: logged");
            }
            AlertAction::Webhook { url } => {
                let payload = serde_json::json!({
                    "rule_id": event.rule_id, "rule_name": event.rule_name,
                    "fired_at": event.fired_at, "condition_value": event.condition_value,
                    "message": event.message,
                });
                match self.http.post(url).json(&payload).send().await {
                    Ok(resp) => info!(rule_id = %event.rule_id, status = %resp.status(), "Alert webhook delivered"),
                    Err(e) => error!(rule_id = %event.rule_id, error = %e, "Alert webhook delivery failed"),
                }
            }
            AlertAction::ThrottleAgent { agent_id } => {
                warn!(rule_id = %event.rule_id, agent_id = %agent_id, "Alert: throttle agent");
            }
            AlertAction::PauseAgent { agent_id } => {
                warn!(rule_id = %event.rule_id, agent_id = %agent_id, "Alert: pause agent");
            }
        }
    }

    pub fn start_evaluation_loop(
        self: Arc<Self>,
        store: TraceStore,
        interval_secs: u64,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));
            loop {
                interval.tick().await;
                let fired = self.evaluate(&store).await;
                if !fired.is_empty() {
                    info!(count = fired.len(), "Alert evaluation: {} alerts fired", fired.len());
                }
            }
        })
    }
}

impl Default for AlertEngine {
    fn default() -> Self { Self::new(300) }
}
