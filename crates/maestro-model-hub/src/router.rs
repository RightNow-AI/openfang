//! Model Router — wraps ModelHub with fallback chains and cost tracking.
//!
//! The router adds three layers on top of the raw ModelHub:
//! 1. **Fallback chain** — if the primary model fails, try the next in the chain
//! 2. **Cost tracking** — accumulate spend per agent/session, enforce budgets
//! 3. **Circuit breaker** — temporarily skip models that have been failing

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, warn};

use crate::{ModelHub, TaskRequirements};

/// A routing decision returned to the caller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    /// The selected model ID.
    pub model_id: String,
    /// The provider (openai, anthropic, google, etc.)
    pub provider: String,
    /// Estimated cost for this request (USD).
    pub estimated_cost: f64,
    /// Whether this is a fallback selection.
    pub is_fallback: bool,
    /// Fallback depth (0 = primary, 1 = first fallback, etc.)
    pub fallback_depth: usize,
}

/// Cost budget configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    /// Maximum spend per agent per hour (USD). None = unlimited.
    pub per_agent_hourly: Option<f64>,
    /// Maximum spend per session (USD). None = unlimited.
    pub per_session: Option<f64>,
    /// Maximum total spend (USD). None = unlimited.
    pub total_max: Option<f64>,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            per_agent_hourly: None,
            per_session: None,
            total_max: None,
        }
    }
}

/// Circuit breaker state for a model.
#[derive(Debug)]
struct CircuitBreaker {
    failures: u32,
    last_failure: Option<Instant>,
    open_until: Option<Instant>,
}

impl CircuitBreaker {
    fn new() -> Self {
        Self { failures: 0, last_failure: None, open_until: None }
    }

    fn is_open(&self) -> bool {
        if let Some(until) = self.open_until {
            Instant::now() < until
        } else {
            false
        }
    }

    fn record_failure(&mut self) {
        self.failures += 1;
        self.last_failure = Some(Instant::now());
        // Open circuit after 3 failures for 60 seconds
        if self.failures >= 3 {
            self.open_until = Some(Instant::now() + Duration::from_secs(60));
            warn!("Circuit breaker opened");
        }
    }

    fn record_success(&mut self) {
        self.failures = 0;
        self.open_until = None;
    }
}

/// Cost accumulator for a single agent or session.
#[derive(Debug, Default)]
struct CostAccumulator {
    total: f64,
    hourly_start: Option<Instant>,
    hourly_total: f64,
}

impl CostAccumulator {
    fn add(&mut self, cost: f64) {
        self.total += cost;
        // Reset hourly counter if an hour has passed
        if let Some(start) = self.hourly_start {
            if start.elapsed() > Duration::from_secs(3600) {
                self.hourly_total = 0.0;
                self.hourly_start = Some(Instant::now());
            }
        } else {
            self.hourly_start = Some(Instant::now());
        }
        self.hourly_total += cost;
    }

    fn exceeds_budget(&self, budget: &BudgetConfig) -> bool {
        if let Some(max) = budget.per_agent_hourly {
            if self.hourly_total >= max { return true; }
        }
        if let Some(max) = budget.total_max {
            if self.total >= max { return true; }
        }
        false
    }
}

/// Model Router with fallback chains, cost tracking, and circuit breakers.
pub struct ModelRouter {
    hub: Arc<ModelHub>,
    /// Ordered fallback chains: capability → [model_id, ...]
    fallback_chains: RwLock<HashMap<String, Vec<String>>>,
    /// Circuit breakers per model
    circuit_breakers: DashMap<String, CircuitBreaker>,
    /// Cost accumulators per agent_id
    agent_costs: DashMap<String, CostAccumulator>,
    /// Global cost accumulator
    global_cost: RwLock<CostAccumulator>,
    /// Budget configuration
    budget: BudgetConfig,
}

impl ModelRouter {
    pub fn new(hub: Arc<ModelHub>, budget: BudgetConfig) -> Self {
        Self {
            hub,
            fallback_chains: RwLock::new(HashMap::new()),
            circuit_breakers: DashMap::new(),
            agent_costs: DashMap::new(),
            global_cost: RwLock::new(CostAccumulator::default()),
            budget,
        }
    }

    /// Register a fallback chain for a capability type.
    pub async fn set_fallback_chain(&self, capability: impl Into<String>, chain: Vec<String>) {
        let mut chains = self.fallback_chains.write().await;
        chains.insert(capability.into(), chain);
    }

    /// Route a request to the best available model.
    ///
    /// Returns a `RoutingDecision` with the selected model and estimated cost.
    /// If the primary model's circuit breaker is open, falls back to the next
    /// model in the chain.
    pub async fn route(
        &self,
        requirements: &TaskRequirements,
        agent_id: Option<&str>,
        estimated_input_tokens: u32,
        estimated_output_tokens: u32,
    ) -> Option<RoutingDecision> {
        // Check budget before routing
        if let Some(agent_id) = agent_id {
            if let Some(acc) = self.agent_costs.get(agent_id) {
                if acc.exceeds_budget(&self.budget) {
                    warn!(agent_id, "Agent has exceeded budget, routing blocked");
                    return None;
                }
            }
        }

        // Get fallback chain for this capability
        let chains = self.fallback_chains.read().await;
        let chain = chains.get(&requirements.primary_capability).cloned();
        drop(chains);

        // Build candidate list: fallback chain first, then hub selection
        let mut candidates: Vec<String> = Vec::new();
        if let Some(chain) = chain {
            candidates.extend(chain);
        }

        // Add hub-selected model if not already in candidates
        if let Some(hub_pick) = self.hub.select(requirements) {
            if !candidates.contains(&hub_pick) {
                candidates.insert(0, hub_pick);
            }
        }

        // Try each candidate in order, skipping open circuit breakers
        for (depth, model_id) in candidates.iter().enumerate() {
            // Check circuit breaker
            if let Some(cb) = self.circuit_breakers.get(model_id) {
                if cb.is_open() {
                    debug!(model_id, "Circuit breaker open, skipping");
                    continue;
                }
            }

            // Get model capabilities for cost estimation
            if let Some(model) = self.hub.get_model(model_id) {
                let estimated_cost = (estimated_input_tokens as f64 / 1_000_000.0)
                    * model.cost_per_1m_input
                    + (estimated_output_tokens as f64 / 1_000_000.0)
                    * model.cost_per_1m_output;

                // Check per-session budget
                if let Some(max_session) = self.budget.per_session {
                    if estimated_cost > max_session {
                        debug!(model_id, "Estimated cost exceeds session budget, trying cheaper model");
                        continue;
                    }
                }

                return Some(RoutingDecision {
                    model_id: model_id.clone(),
                    provider: model.provider.clone(),
                    estimated_cost,
                    is_fallback: depth > 0,
                    fallback_depth: depth,
                });
            }
        }

        None
    }

    /// Record a successful model call (resets circuit breaker, tracks cost).
    pub async fn record_success(&self, model_id: &str, agent_id: Option<&str>, actual_cost: f64) {
        // Reset circuit breaker
        if let Some(mut cb) = self.circuit_breakers.get_mut(model_id) {
            cb.record_success();
        }

        // Track cost
        if let Some(agent_id) = agent_id {
            self.agent_costs.entry(agent_id.to_string())
                .or_default()
                .add(actual_cost);
        }
        self.global_cost.write().await.add(actual_cost);
    }

    /// Record a failed model call (increments circuit breaker).
    pub fn record_failure(&self, model_id: &str) {
        self.circuit_breakers
            .entry(model_id.to_string())
            .or_insert_with(CircuitBreaker::new)
            .record_failure();
    }

    /// Get total spend for an agent.
    pub fn agent_spend(&self, agent_id: &str) -> f64 {
        self.agent_costs.get(agent_id).map(|a| a.total).unwrap_or(0.0)
    }

    /// Get total global spend.
    pub async fn global_spend(&self) -> f64 {
        self.global_cost.read().await.total
    }

    /// Get spend summary for all agents.
    pub fn spend_summary(&self) -> HashMap<String, f64> {
        self.agent_costs.iter()
            .map(|e| (e.key().clone(), e.value().total))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::default_registry;

    #[tokio::test]
    async fn test_route_selects_model() {
        let hub = Arc::new(default_registry());
        let router = ModelRouter::new(hub, BudgetConfig::default());

        let req = TaskRequirements {
            primary_capability: "coding".to_string(),
            min_context: 8_000,
            needs_tools: false,
            needs_vision: false,
            max_cost_per_1m: None,
            prefer_speed: false,
        };

        let decision = router.route(&req, None, 1_000, 500).await;
        assert!(decision.is_some());
        let d = decision.unwrap();
        assert!(!d.model_id.is_empty());
    }

    #[tokio::test]
    async fn test_budget_enforcement() {
        let hub = Arc::new(default_registry());
        let budget = BudgetConfig {
            per_agent_hourly: Some(0.001), // Very low budget
            ..Default::default()
        };
        let router = ModelRouter::new(hub, budget);

        // Simulate the agent exceeding budget
        router.agent_costs.entry("agent-1".to_string())
            .or_default()
            .add(0.002); // Exceeds 0.001 hourly limit

        let req = TaskRequirements {
            primary_capability: "coding".to_string(),
            min_context: 1_000,
            needs_tools: false,
            needs_vision: false,
            max_cost_per_1m: None,
            prefer_speed: false,
        };

        let decision = router.route(&req, Some("agent-1"), 1_000, 500).await;
        assert!(decision.is_none(), "Should block when budget exceeded");
    }
}
