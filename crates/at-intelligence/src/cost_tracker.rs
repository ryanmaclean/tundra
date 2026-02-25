//! Cost tracking, token budgeting, and LETS metrics for LLM orchestration.
//!
//! Tracks per-model pricing, per-task/agent token budgets, and computes
//! quality/cost/accuracy tradeoffs. Provides LETS metrics (Latency,
//! Efficiency, Throughput, Scalability) for monitoring agent swarms.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// Model Pricing
// ---------------------------------------------------------------------------

/// Per-model pricing in USD per 1M tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub model: String,
    pub provider: String,
    pub input_cost_per_1m: f64,
    pub output_cost_per_1m: f64,
    /// Relative quality score (0.0–1.0) for routing decisions.
    pub quality_score: f64,
    /// Context window size in tokens.
    pub context_window: u64,
}

impl ModelPricing {
    /// Calculate cost for a request with the given token counts.
    pub fn calculate_cost(&self, input_tokens: u64, output_tokens: u64) -> f64 {
        (input_tokens as f64 / 1_000_000.0) * self.input_cost_per_1m
            + (output_tokens as f64 / 1_000_000.0) * self.output_cost_per_1m
    }
}

/// Default pricing table for common models (approximate 2025-2026 pricing).
pub fn default_pricing_table() -> Vec<ModelPricing> {
    vec![
        // Anthropic
        ModelPricing {
            model: "claude-opus-4-20250514".into(),
            provider: "anthropic".into(),
            input_cost_per_1m: 15.0,
            output_cost_per_1m: 75.0,
            quality_score: 0.98,
            context_window: 200_000,
        },
        ModelPricing {
            model: "claude-sonnet-4-20250514".into(),
            provider: "anthropic".into(),
            input_cost_per_1m: 3.0,
            output_cost_per_1m: 15.0,
            quality_score: 0.92,
            context_window: 200_000,
        },
        ModelPricing {
            model: "claude-haiku-4-20250514".into(),
            provider: "anthropic".into(),
            input_cost_per_1m: 0.80,
            output_cost_per_1m: 4.0,
            quality_score: 0.82,
            context_window: 200_000,
        },
        // OpenAI
        ModelPricing {
            model: "gpt-4o".into(),
            provider: "openai".into(),
            input_cost_per_1m: 2.50,
            output_cost_per_1m: 10.0,
            quality_score: 0.90,
            context_window: 128_000,
        },
        ModelPricing {
            model: "gpt-4o-mini".into(),
            provider: "openai".into(),
            input_cost_per_1m: 0.15,
            output_cost_per_1m: 0.60,
            quality_score: 0.78,
            context_window: 128_000,
        },
        ModelPricing {
            model: "o3-mini".into(),
            provider: "openai".into(),
            input_cost_per_1m: 1.10,
            output_cost_per_1m: 4.40,
            quality_score: 0.88,
            context_window: 200_000,
        },
    ]
}

// ---------------------------------------------------------------------------
// Request Record
// ---------------------------------------------------------------------------

/// A single LLM request/response record for cost tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestRecord {
    pub model: String,
    pub provider: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    pub latency_ms: u64,
    pub cache_hit: bool,
    pub task_id: Option<String>,
    pub agent_id: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// Token Budget
// ---------------------------------------------------------------------------

/// A token budget for a task or agent with enforcement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBudget {
    /// Maximum total tokens (input + output) allowed.
    pub max_tokens: u64,
    /// Tokens consumed so far.
    pub consumed_tokens: u64,
    /// Maximum cost in USD allowed.
    pub max_cost_usd: f64,
    /// Cost consumed so far.
    pub consumed_cost_usd: f64,
    /// Maximum number of requests allowed.
    pub max_requests: u32,
    /// Requests made so far.
    pub request_count: u32,
}

impl TokenBudget {
    pub fn new(max_tokens: u64, max_cost_usd: f64, max_requests: u32) -> Self {
        Self {
            max_tokens,
            consumed_tokens: 0,
            max_cost_usd,
            consumed_cost_usd: 0.0,
            max_requests,
            request_count: 0,
        }
    }

    /// Check if the budget allows another request of the estimated size.
    pub fn can_afford(&self, estimated_tokens: u64, estimated_cost: f64) -> BudgetCheck {
        if self.request_count >= self.max_requests {
            return BudgetCheck::Denied {
                reason: "max requests exceeded".into(),
            };
        }
        if self.consumed_tokens + estimated_tokens > self.max_tokens {
            return BudgetCheck::Denied {
                reason: format!(
                    "would exceed token budget ({} + {} > {})",
                    self.consumed_tokens, estimated_tokens, self.max_tokens
                ),
            };
        }
        if self.consumed_cost_usd + estimated_cost > self.max_cost_usd {
            return BudgetCheck::Denied {
                reason: format!(
                    "would exceed cost budget (${:.4} + ${:.4} > ${:.4})",
                    self.consumed_cost_usd, estimated_cost, self.max_cost_usd
                ),
            };
        }
        // Warn if we're above 80% of any limit
        let token_pct = (self.consumed_tokens + estimated_tokens) as f64 / self.max_tokens as f64;
        let cost_pct = (self.consumed_cost_usd + estimated_cost) / self.max_cost_usd;
        if token_pct > 0.8 || cost_pct > 0.8 {
            BudgetCheck::Warning {
                token_pct,
                cost_pct,
            }
        } else {
            BudgetCheck::Allowed
        }
    }

    /// Record consumption after a successful request.
    pub fn consume(&mut self, tokens: u64, cost: f64) {
        self.consumed_tokens += tokens;
        self.consumed_cost_usd += cost;
        self.request_count += 1;
    }

    /// Percentage of token budget consumed.
    pub fn token_utilization(&self) -> f64 {
        if self.max_tokens == 0 {
            return 0.0;
        }
        self.consumed_tokens as f64 / self.max_tokens as f64
    }

    /// Percentage of cost budget consumed.
    pub fn cost_utilization(&self) -> f64 {
        if self.max_cost_usd == 0.0 {
            return 0.0;
        }
        self.consumed_cost_usd / self.max_cost_usd
    }
}

/// Result of a budget check.
#[derive(Debug, Clone, PartialEq)]
pub enum BudgetCheck {
    Allowed,
    Warning { token_pct: f64, cost_pct: f64 },
    Denied { reason: String },
}

impl BudgetCheck {
    pub fn is_allowed(&self) -> bool {
        !matches!(self, BudgetCheck::Denied { .. })
    }
}

// ---------------------------------------------------------------------------
// LETS Metrics
// ---------------------------------------------------------------------------

/// LETS metrics snapshot for monitoring agent orchestration.
/// Latency, Efficiency, Throughput, Scalability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LetsMetrics {
    /// Average time-to-first-token in ms.
    pub latency_ttft_ms: f64,
    /// Average total request latency in ms.
    pub latency_total_ms: f64,
    /// P95 request latency in ms.
    pub latency_p95_ms: f64,
    /// Token efficiency: output tokens per input token.
    pub efficiency_ratio: f64,
    /// Cache hit rate (0.0–1.0).
    pub efficiency_cache_hit_rate: f64,
    /// Average cost per request in USD.
    pub efficiency_cost_per_request: f64,
    /// Total tokens generated per second across all agents.
    pub throughput_tps: f64,
    /// Requests completed per minute.
    pub throughput_rpm: f64,
    /// Number of active concurrent agents.
    pub scalability_active_agents: u32,
    /// Percentage of budget consumed.
    pub scalability_budget_utilization: f64,
    /// Timestamp of this snapshot.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// Quality-Cost-Accuracy (QCA) Score
// ---------------------------------------------------------------------------

/// Composite score combining quality, cost, and accuracy for model selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QcaScore {
    /// Quality score (0.0–1.0) — model capability.
    pub quality: f64,
    /// Cost score (0.0–1.0) — lower cost = higher score.
    pub cost: f64,
    /// Accuracy score (0.0–1.0) — task completion accuracy.
    pub accuracy: f64,
    /// Weighted composite score.
    pub composite: f64,
}

impl QcaScore {
    /// Compute a weighted composite score.
    /// Default weights: quality=0.3, cost=0.4, accuracy=0.3
    pub fn compute(quality: f64, cost: f64, accuracy: f64) -> Self {
        Self::compute_weighted(quality, cost, accuracy, 0.3, 0.4, 0.3)
    }

    pub fn compute_weighted(
        quality: f64,
        cost: f64,
        accuracy: f64,
        w_quality: f64,
        w_cost: f64,
        w_accuracy: f64,
    ) -> Self {
        let total_weight = w_quality + w_cost + w_accuracy;
        let composite = if total_weight > 0.0 {
            (quality * w_quality + cost * w_cost + accuracy * w_accuracy) / total_weight
        } else {
            0.0
        };
        Self {
            quality,
            cost,
            accuracy,
            composite,
        }
    }
}

// ---------------------------------------------------------------------------
// CostTracker
// ---------------------------------------------------------------------------

/// Thread-safe cost tracker for all LLM requests across the system.
/// Ring-buffer backed using `VecDeque` for O(1) eviction.
#[derive(Clone)]
pub struct CostTracker {
    pricing: Arc<RwLock<HashMap<String, ModelPricing>>>,
    records: Arc<RwLock<VecDeque<RequestRecord>>>,
    max_records: usize,
    budgets: Arc<RwLock<HashMap<String, TokenBudget>>>,
    latencies: Arc<RwLock<VecDeque<u64>>>,
    max_latencies: usize,
}

impl CostTracker {
    pub fn new(max_records: usize, max_latencies: usize) -> Self {
        Self::with_capacity(max_records, max_latencies)
    }

    pub fn with_capacity(max_records: usize, max_latencies: usize) -> Self {
        let mut pricing_map = HashMap::new();
        for p in default_pricing_table() {
            pricing_map.insert(p.model.clone(), p);
        }
        Self {
            pricing: Arc::new(RwLock::new(pricing_map)),
            records: Arc::new(RwLock::new(VecDeque::new())),
            max_records,
            budgets: Arc::new(RwLock::new(HashMap::new())),
            latencies: Arc::new(RwLock::new(VecDeque::new())),
            max_latencies,
        }
    }

    /// Add or update model pricing.
    pub async fn set_pricing(&self, pricing: ModelPricing) {
        let mut map = self.pricing.write().await;
        map.insert(pricing.model.clone(), pricing);
    }

    /// Get pricing for a model.
    pub async fn get_pricing(&self, model: &str) -> Option<ModelPricing> {
        self.pricing.read().await.get(model).cloned()
    }

    /// Calculate cost for a request (returns 0.0 if model not in pricing table).
    pub async fn calculate_cost(&self, model: &str, input_tokens: u64, output_tokens: u64) -> f64 {
        match self.pricing.read().await.get(model) {
            Some(p) => p.calculate_cost(input_tokens, output_tokens),
            None => 0.0,
        }
    }

    /// Record a completed request.
    pub async fn record_request(&self, record: RequestRecord) {
        let mut latencies = self.latencies.write().await;
        latencies.push_back(record.latency_ms);
        // Ring buffer: evict oldest when over capacity (O(1) with VecDeque).
        while latencies.len() > self.max_latencies {
            latencies.pop_front();
        }

        let mut records = self.records.write().await;
        records.push_back(record);
        // Ring buffer: evict oldest when over capacity (O(1) with VecDeque).
        while records.len() > self.max_records {
            records.pop_front();
        }
    }

    /// Set a token budget for a task or agent.
    pub async fn set_budget(&self, key: String, budget: TokenBudget) {
        let mut budgets = self.budgets.write().await;
        budgets.insert(key, budget);
    }

    /// Check budget for a key. Returns `BudgetCheck::Allowed` if no budget is set.
    pub async fn check_budget(
        &self,
        key: &str,
        estimated_tokens: u64,
        estimated_cost: f64,
    ) -> BudgetCheck {
        let budgets = self.budgets.read().await;
        match budgets.get(key) {
            Some(budget) => budget.can_afford(estimated_tokens, estimated_cost),
            None => BudgetCheck::Allowed,
        }
    }

    /// Consume budget for a key. No-op if no budget is set.
    pub async fn consume_budget(&self, key: &str, tokens: u64, cost: f64) {
        let mut budgets = self.budgets.write().await;
        if let Some(budget) = budgets.get_mut(key) {
            budget.consume(tokens, cost);
        }
    }

    /// Get total cost across all recorded requests.
    pub async fn total_cost(&self) -> f64 {
        self.records.read().await.iter().map(|r| r.cost_usd).sum()
    }

    /// Get total tokens across all recorded requests.
    pub async fn total_tokens(&self) -> u64 {
        self.records
            .read()
            .await
            .iter()
            .map(|r| r.input_tokens + r.output_tokens)
            .sum()
    }

    /// Get cost breakdown by model.
    pub async fn cost_by_model(&self) -> HashMap<String, f64> {
        let records = self.records.read().await;
        let mut by_model: HashMap<String, f64> = HashMap::new();
        for r in records.iter() {
            *by_model.entry(r.model.clone()).or_default() += r.cost_usd;
        }
        by_model
    }

    /// Compute LETS metrics from recorded data.
    pub async fn compute_lets_metrics(&self, active_agents: u32) -> LetsMetrics {
        let records = self.records.read().await;
        let latencies = self.latencies.read().await;

        let total_requests = records.len() as f64;
        let total_cost: f64 = records.iter().map(|r| r.cost_usd).sum();
        let total_input: u64 = records.iter().map(|r| r.input_tokens).sum();
        let total_output: u64 = records.iter().map(|r| r.output_tokens).sum();
        let cache_hits = records.iter().filter(|r| r.cache_hit).count() as f64;

        // Latency
        let avg_latency = if latencies.is_empty() {
            0.0
        } else {
            latencies.iter().sum::<u64>() as f64 / latencies.len() as f64
        };

        let p95_latency = if latencies.is_empty() {
            0.0
        } else {
            let mut sorted: Vec<u64> = latencies.iter().copied().collect();
            sorted.sort_unstable();
            let idx = ((sorted.len() as f64) * 0.95) as usize;
            sorted[idx.min(sorted.len() - 1)] as f64
        };

        // Efficiency
        let efficiency_ratio = if total_input > 0 {
            total_output as f64 / total_input as f64
        } else {
            0.0
        };
        let cache_hit_rate = if total_requests > 0.0 {
            cache_hits / total_requests
        } else {
            0.0
        };
        let cost_per_request = if total_requests > 0.0 {
            total_cost / total_requests
        } else {
            0.0
        };

        // Throughput — computed over the time window of all records
        let (tps, rpm) = if records.len() >= 2 {
            let first = records.front().unwrap().timestamp;
            let last = records.back().unwrap().timestamp;
            let duration_secs = (last - first).num_seconds().max(1) as f64;
            let tps = total_output as f64 / duration_secs;
            let rpm = total_requests / (duration_secs / 60.0);
            (tps, rpm)
        } else {
            (0.0, 0.0)
        };

        // Budget utilization
        let budgets = self.budgets.read().await;
        let budget_util = if budgets.is_empty() {
            0.0
        } else {
            budgets.values().map(|b| b.cost_utilization()).sum::<f64>() / budgets.len() as f64
        };

        LetsMetrics {
            latency_ttft_ms: avg_latency * 0.3, // approximate TTFT as fraction of total
            latency_total_ms: avg_latency,
            latency_p95_ms: p95_latency,
            efficiency_ratio,
            efficiency_cache_hit_rate: cache_hit_rate,
            efficiency_cost_per_request: cost_per_request,
            throughput_tps: tps,
            throughput_rpm: rpm,
            scalability_active_agents: active_agents,
            scalability_budget_utilization: budget_util,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Number of recorded requests.
    pub async fn request_count(&self) -> usize {
        self.records.read().await.len()
    }
}

impl Default for CostTracker {
    fn default() -> Self {
        Self::new(10_000, 100_000)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    // -- ModelPricing --

    #[test]
    fn model_pricing_cost_calculation() {
        let pricing = ModelPricing {
            model: "test-model".into(),
            provider: "test".into(),
            input_cost_per_1m: 3.0,
            output_cost_per_1m: 15.0,
            quality_score: 0.9,
            context_window: 200_000,
        };
        // 1M input + 1M output = $3 + $15 = $18
        let cost = pricing.calculate_cost(1_000_000, 1_000_000);
        assert!((cost - 18.0).abs() < 0.001);

        // 1000 input + 500 output
        let cost = pricing.calculate_cost(1000, 500);
        assert!((cost - 0.0105).abs() < 0.0001);
    }

    #[test]
    fn model_pricing_zero_tokens() {
        let pricing = &default_pricing_table()[0];
        assert_eq!(pricing.calculate_cost(0, 0), 0.0);
    }

    #[test]
    fn default_pricing_table_has_entries() {
        let table = default_pricing_table();
        assert!(table.len() >= 4);
        // Should have both anthropic and openai
        assert!(table.iter().any(|p| p.provider == "anthropic"));
        assert!(table.iter().any(|p| p.provider == "openai"));
    }

    // -- TokenBudget --

    #[test]
    fn budget_allows_within_limits() {
        let budget = TokenBudget::new(10_000, 1.0, 100);
        assert!(budget.can_afford(1000, 0.05).is_allowed());
    }

    #[test]
    fn budget_denies_over_tokens() {
        let budget = TokenBudget::new(1000, 10.0, 100);
        let check = budget.can_afford(1500, 0.01);
        assert!(!check.is_allowed());
    }

    #[test]
    fn budget_denies_over_cost() {
        let budget = TokenBudget::new(1_000_000, 0.50, 100);
        let check = budget.can_afford(100, 0.60);
        assert!(!check.is_allowed());
    }

    #[test]
    fn budget_denies_over_requests() {
        let mut budget = TokenBudget::new(1_000_000, 100.0, 2);
        budget.consume(100, 0.01);
        budget.consume(100, 0.01);
        let check = budget.can_afford(100, 0.01);
        assert!(!check.is_allowed());
    }

    #[test]
    fn budget_warns_at_80_percent() {
        let budget = TokenBudget::new(1000, 1.0, 100);
        let check = budget.can_afford(850, 0.01);
        match check {
            BudgetCheck::Warning { token_pct, .. } => {
                assert!(token_pct > 0.8);
            }
            other => panic!("expected Warning, got {other:?}"),
        }
    }

    #[test]
    fn budget_consume_and_utilization() {
        let mut budget = TokenBudget::new(10_000, 5.0, 50);
        budget.consume(2500, 1.25);
        assert!((budget.token_utilization() - 0.25).abs() < 0.001);
        assert!((budget.cost_utilization() - 0.25).abs() < 0.001);
        assert_eq!(budget.request_count, 1);
    }

    // -- QCA Score --

    #[test]
    fn qca_score_default_weights() {
        let score = QcaScore::compute(0.9, 0.8, 0.7);
        // (0.9*0.3 + 0.8*0.4 + 0.7*0.3) / 1.0 = 0.27 + 0.32 + 0.21 = 0.80
        assert!((score.composite - 0.80).abs() < 0.001);
    }

    #[test]
    fn qca_score_custom_weights() {
        let score = QcaScore::compute_weighted(1.0, 0.0, 1.0, 0.5, 0.0, 0.5);
        assert!((score.composite - 1.0).abs() < 0.001);
    }

    #[test]
    fn qca_score_zero_weights() {
        let score = QcaScore::compute_weighted(1.0, 1.0, 1.0, 0.0, 0.0, 0.0);
        assert_eq!(score.composite, 0.0);
    }

    // -- CostTracker --

    #[tokio::test]
    async fn tracker_starts_empty() {
        let tracker = CostTracker::new(10_000, 100_000);
        assert_eq!(tracker.total_cost().await, 0.0);
        assert_eq!(tracker.total_tokens().await, 0);
        assert_eq!(tracker.request_count().await, 0);
    }

    #[tokio::test]
    async fn tracker_records_request() {
        let tracker = CostTracker::new(10_000, 100_000);
        tracker
            .record_request(RequestRecord {
                model: "claude-sonnet-4-20250514".into(),
                provider: "anthropic".into(),
                input_tokens: 1000,
                output_tokens: 500,
                cost_usd: 0.0105,
                latency_ms: 250,
                cache_hit: false,
                task_id: None,
                agent_id: None,
                timestamp: Utc::now(),
            })
            .await;

        assert_eq!(tracker.request_count().await, 1);
        assert!((tracker.total_cost().await - 0.0105).abs() < 0.0001);
        assert_eq!(tracker.total_tokens().await, 1500);
    }

    #[tokio::test]
    async fn tracker_cost_by_model() {
        let tracker = CostTracker::new(10_000, 100_000);
        for (model, cost) in [("model-a", 0.10), ("model-b", 0.20), ("model-a", 0.15)] {
            tracker
                .record_request(RequestRecord {
                    model: model.into(),
                    provider: "test".into(),
                    input_tokens: 100,
                    output_tokens: 50,
                    cost_usd: cost,
                    latency_ms: 100,
                    cache_hit: false,
                    task_id: None,
                    agent_id: None,
                    timestamp: Utc::now(),
                })
                .await;
        }

        let by_model = tracker.cost_by_model().await;
        assert!((by_model["model-a"] - 0.25).abs() < 0.001);
        assert!((by_model["model-b"] - 0.20).abs() < 0.001);
    }

    #[tokio::test]
    async fn tracker_budget_enforcement() {
        let tracker = CostTracker::new(10_000, 100_000);
        tracker
            .set_budget("task-1".into(), TokenBudget::new(5000, 0.50, 10))
            .await;

        assert!(tracker
            .check_budget("task-1", 1000, 0.10)
            .await
            .is_allowed());

        // Consume most of the budget
        tracker.consume_budget("task-1", 4500, 0.45).await;

        // Should be denied now
        assert!(!tracker
            .check_budget("task-1", 1000, 0.10)
            .await
            .is_allowed());
    }

    #[tokio::test]
    async fn tracker_no_budget_allows_all() {
        let tracker = CostTracker::new(10_000, 100_000);
        assert!(tracker
            .check_budget("no-budget-key", 999_999, 999.0)
            .await
            .is_allowed());
    }

    #[tokio::test]
    async fn tracker_compute_lets_metrics() {
        let tracker = CostTracker::new(10_000, 100_000);
        let now = Utc::now();

        for i in 0..5 {
            tracker
                .record_request(RequestRecord {
                    model: "test".into(),
                    provider: "test".into(),
                    input_tokens: 100,
                    output_tokens: 50,
                    cost_usd: 0.01,
                    latency_ms: 200 + i * 50,
                    cache_hit: i == 0,
                    task_id: None,
                    agent_id: None,
                    timestamp: now + chrono::Duration::seconds(i as i64),
                })
                .await;
        }

        let metrics = tracker.compute_lets_metrics(3).await;
        assert!(metrics.latency_total_ms > 0.0);
        assert!(metrics.latency_p95_ms >= metrics.latency_total_ms);
        assert!((metrics.efficiency_cache_hit_rate - 0.2).abs() < 0.001);
        assert!(metrics.efficiency_cost_per_request > 0.0);
        assert_eq!(metrics.scalability_active_agents, 3);
    }

    #[tokio::test]
    async fn tracker_custom_pricing() {
        let tracker = CostTracker::new(10_000, 100_000);
        tracker
            .set_pricing(ModelPricing {
                model: "custom-model".into(),
                provider: "custom".into(),
                input_cost_per_1m: 1.0,
                output_cost_per_1m: 2.0,
                quality_score: 0.85,
                context_window: 100_000,
            })
            .await;

        let cost = tracker
            .calculate_cost("custom-model", 1_000_000, 1_000_000)
            .await;
        assert!((cost - 3.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn tracker_unknown_model_zero_cost() {
        let tracker = CostTracker::new(10_000, 100_000);
        let cost = tracker.calculate_cost("nonexistent", 1000, 1000).await;
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn request_record_serialization() {
        let record = RequestRecord {
            model: "test".into(),
            provider: "test".into(),
            input_tokens: 100,
            output_tokens: 50,
            cost_usd: 0.01,
            latency_ms: 200,
            cache_hit: false,
            task_id: Some("task-1".into()),
            agent_id: None,
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&record).unwrap();
        let deser: RequestRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.model, "test");
        assert_eq!(deser.input_tokens, 100);
    }

    #[test]
    fn lets_metrics_serialization() {
        let metrics = LetsMetrics {
            latency_ttft_ms: 50.0,
            latency_total_ms: 200.0,
            latency_p95_ms: 350.0,
            efficiency_ratio: 0.5,
            efficiency_cache_hit_rate: 0.3,
            efficiency_cost_per_request: 0.01,
            throughput_tps: 100.0,
            throughput_rpm: 60.0,
            scalability_active_agents: 4,
            scalability_budget_utilization: 0.45,
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&metrics).unwrap();
        let deser: LetsMetrics = serde_json::from_str(&json).unwrap();
        assert!((deser.latency_total_ms - 200.0).abs() < 0.001);
        assert_eq!(deser.scalability_active_agents, 4);
    }
}
