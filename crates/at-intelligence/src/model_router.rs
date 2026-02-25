//! Model routing and cascading for cost-efficient LLM orchestration.
//!
//! Routes requests to the most appropriate model based on:
//! - Task complexity (simple → cheap model, complex → premium model)
//! - Token budget constraints
//! - Quality requirements
//! - Cost optimization targets
//!
//! Supports cascading: try a cheaper model first, escalate if confidence is low.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::cost_tracker::{CostTracker, ModelPricing};
use crate::llm::{LlmConfig, LlmError, LlmMessage, LlmProvider, LlmResponse};
use crate::token_cache::TokenCache;

// ---------------------------------------------------------------------------
// Routing Strategy
// ---------------------------------------------------------------------------

/// How to select a model for a given request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingStrategy {
    /// Always use a fixed model.
    Fixed { model: String },
    /// Route based on estimated task complexity.
    ComplexityBased,
    /// Route to minimize cost while meeting a quality threshold.
    CostOptimized { min_quality: f64 },
    /// Try cheaper models first, escalate if response quality is low.
    Cascade,
}

impl Default for RoutingStrategy {
    fn default() -> Self {
        Self::CostOptimized { min_quality: 0.8 }
    }
}

// ---------------------------------------------------------------------------
// Task Complexity Estimate
// ---------------------------------------------------------------------------

/// Estimated complexity for model routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComplexityLevel {
    Trivial,
    Simple,
    Moderate,
    Complex,
    Expert,
}

impl ComplexityLevel {
    /// Minimum quality score needed for this complexity level.
    pub fn min_quality(&self) -> f64 {
        match self {
            Self::Trivial => 0.5,
            Self::Simple => 0.65,
            Self::Moderate => 0.80,
            Self::Complex => 0.90,
            Self::Expert => 0.95,
        }
    }
}

/// Estimate complexity from prompt characteristics.
pub fn estimate_complexity(messages: &[LlmMessage]) -> ComplexityLevel {
    let total_chars: usize = messages.iter().map(|m| m.content.len()).sum();
    let msg_count = messages.len();

    // Heuristic based on prompt size and structure
    if total_chars < 100 && msg_count <= 2 {
        ComplexityLevel::Trivial
    } else if total_chars < 500 && msg_count <= 4 {
        ComplexityLevel::Simple
    } else if total_chars < 2000 {
        ComplexityLevel::Moderate
    } else if total_chars < 8000 {
        ComplexityLevel::Complex
    } else {
        ComplexityLevel::Expert
    }
}

// ---------------------------------------------------------------------------
// Route Decision
// ---------------------------------------------------------------------------

/// The result of a routing decision.
#[derive(Debug, Clone)]
pub struct RouteDecision {
    /// The selected model ID.
    pub model: String,
    /// The provider for the selected model.
    pub provider: String,
    /// Why this model was selected.
    pub reason: String,
    /// Estimated cost for this request.
    pub estimated_cost: f64,
    /// Quality score of the selected model.
    pub quality_score: f64,
}

// ---------------------------------------------------------------------------
// ModelRouter
// ---------------------------------------------------------------------------

/// Routes LLM requests to the optimal model based on strategy and constraints.
pub struct ModelRouter {
    strategy: RoutingStrategy,
    cost_tracker: CostTracker,
    cache: TokenCache,
    /// Model tiers ordered from cheapest to most expensive.
    model_tiers: Arc<RwLock<Vec<ModelPricing>>>,
}

impl ModelRouter {
    pub fn new(strategy: RoutingStrategy, cost_tracker: CostTracker, cache: TokenCache) -> Self {
        let mut tiers = crate::cost_tracker::default_pricing_table();
        // Sort by cost (cheapest first)
        tiers.sort_by(|a, b| {
            a.output_cost_per_1m
                .partial_cmp(&b.output_cost_per_1m)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Self {
            strategy,
            cost_tracker,
            cache,
            model_tiers: Arc::new(RwLock::new(tiers)),
        }
    }

    /// Select the best model for the given messages and config.
    pub async fn route(&self, messages: &[LlmMessage], _config: &LlmConfig) -> RouteDecision {
        match &self.strategy {
            RoutingStrategy::Fixed { model } => self.route_fixed(model).await,
            RoutingStrategy::ComplexityBased => self.route_by_complexity(messages).await,
            RoutingStrategy::CostOptimized { min_quality } => {
                self.route_cost_optimized(messages, *min_quality).await
            }
            RoutingStrategy::Cascade => self.route_cascade(messages).await,
        }
    }

    /// Execute a request with automatic routing, caching, and cost tracking.
    pub async fn execute(
        &self,
        provider: &dyn LlmProvider,
        messages: &[LlmMessage],
        config: &LlmConfig,
        budget_key: Option<&str>,
    ) -> Result<(LlmResponse, RouteDecision), LlmError> {
        // Check cache first
        if let Some(cached) = self.cache.get(messages, config).await {
            let decision = RouteDecision {
                model: cached.model.clone(),
                provider: "cache".into(),
                reason: "cache hit".into(),
                estimated_cost: 0.0,
                quality_score: 1.0,
            };
            return Ok((cached, decision));
        }

        // Route to best model
        let decision = self.route(messages, config).await;

        // Check budget
        if let Some(key) = budget_key {
            let check = self
                .cost_tracker
                .check_budget(key, config.max_tokens as u64, decision.estimated_cost)
                .await;
            if !check.is_allowed() {
                return Err(LlmError::ApiError {
                    status: 429,
                    message: "token budget exceeded".into(),
                });
            }
        }

        // Execute with the routed model
        let routed_config = LlmConfig {
            model: decision.model.clone(),
            ..config.clone()
        };

        let start = std::time::Instant::now();
        let response = provider.complete(messages, &routed_config).await?;
        let latency_ms = start.elapsed().as_millis() as u64;

        // Calculate actual cost
        let cost = self
            .cost_tracker
            .calculate_cost(
                &response.model,
                response.input_tokens,
                response.output_tokens,
            )
            .await;

        // Record in cost tracker
        let record = crate::cost_tracker::RequestRecord {
            model: response.model.clone(),
            provider: decision.provider.clone(),
            input_tokens: response.input_tokens,
            output_tokens: response.output_tokens,
            cost_usd: cost,
            latency_ms,
            cache_hit: false,
            task_id: None,
            agent_id: None,
            timestamp: chrono::Utc::now(),
        };
        self.cost_tracker.record_request(record).await;

        // Consume budget
        if let Some(key) = budget_key {
            self.cost_tracker
                .consume_budget(key, response.input_tokens + response.output_tokens, cost)
                .await;
        }

        // Store in cache using the original config so future lookups hit
        self.cache.put(messages, config, &response).await;

        Ok((response, decision))
    }

    /// Add a model to the routing tiers.
    pub async fn add_model(&self, pricing: ModelPricing) {
        let mut tiers = self.model_tiers.write().await;
        tiers.push(pricing.clone());
        tiers.sort_by(|a, b| {
            a.output_cost_per_1m
                .partial_cmp(&b.output_cost_per_1m)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        self.cost_tracker.set_pricing(pricing).await;
    }

    // -- Routing strategies --

    async fn route_fixed(&self, model: &str) -> RouteDecision {
        let tiers = self.model_tiers.read().await;
        let pricing = tiers.iter().find(|p| p.model == model);

        RouteDecision {
            model: model.to_string(),
            provider: pricing.map(|p| p.provider.clone()).unwrap_or_default(),
            reason: "fixed model".into(),
            estimated_cost: pricing.map(|p| p.calculate_cost(1000, 500)).unwrap_or(0.0),
            quality_score: pricing.map(|p| p.quality_score).unwrap_or(0.5),
        }
    }

    async fn route_by_complexity(&self, messages: &[LlmMessage]) -> RouteDecision {
        let complexity = estimate_complexity(messages);
        let min_quality = complexity.min_quality();
        self.route_cost_optimized(messages, min_quality).await
    }

    async fn route_cost_optimized(
        &self,
        _messages: &[LlmMessage],
        min_quality: f64,
    ) -> RouteDecision {
        let tiers = self.model_tiers.read().await;

        // Find the cheapest model that meets the quality threshold
        for pricing in tiers.iter() {
            if pricing.quality_score >= min_quality {
                return RouteDecision {
                    model: pricing.model.clone(),
                    provider: pricing.provider.clone(),
                    reason: format!(
                        "cheapest model meeting quality threshold {:.0}%",
                        min_quality * 100.0
                    ),
                    estimated_cost: pricing.calculate_cost(1000, 500),
                    quality_score: pricing.quality_score,
                };
            }
        }

        // Fallback to the highest quality model
        let best = tiers.last().cloned().unwrap_or(ModelPricing {
            model: "claude-sonnet-4-20250514".into(),
            provider: "anthropic".into(),
            input_cost_per_1m: 3.0,
            output_cost_per_1m: 15.0,
            quality_score: 0.92,
            context_window: 200_000,
        });

        let estimated_cost = best.calculate_cost(1000, 500);
        let quality_score = best.quality_score;
        RouteDecision {
            model: best.model,
            provider: best.provider,
            reason: "fallback to highest quality".into(),
            estimated_cost,
            quality_score,
        }
    }

    async fn route_cascade(&self, messages: &[LlmMessage]) -> RouteDecision {
        let complexity = estimate_complexity(messages);
        // For cascade, start with the cheapest model that has a reasonable chance
        let min_quality = match complexity {
            ComplexityLevel::Trivial | ComplexityLevel::Simple => 0.5,
            ComplexityLevel::Moderate => 0.7,
            ComplexityLevel::Complex => 0.85,
            ComplexityLevel::Expert => 0.90,
        };
        self.route_cost_optimized(messages, min_quality).await
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{LlmMessage, LlmResponse, MockProvider};
    use crate::token_cache::TokenCacheConfig;

    fn make_router(strategy: RoutingStrategy) -> ModelRouter {
        ModelRouter::new(
            strategy,
            CostTracker::default(),
            TokenCache::new(TokenCacheConfig::default()),
        )
    }

    // -- Complexity estimation --

    #[test]
    fn estimate_trivial_complexity() {
        let messages = vec![LlmMessage::user("Hi")];
        assert_eq!(estimate_complexity(&messages), ComplexityLevel::Trivial);
    }

    #[test]
    fn estimate_simple_complexity() {
        let messages = vec![LlmMessage::user("Tell me about Rust programming language")];
        assert!(estimate_complexity(&messages) <= ComplexityLevel::Simple);
    }

    #[test]
    fn estimate_moderate_complexity() {
        let long_msg = "x".repeat(600);
        let messages = vec![LlmMessage::user(long_msg)];
        assert_eq!(estimate_complexity(&messages), ComplexityLevel::Moderate);
    }

    #[test]
    fn estimate_complex_complexity() {
        let long_msg = "x".repeat(3000);
        let messages = vec![LlmMessage::user(long_msg)];
        assert_eq!(estimate_complexity(&messages), ComplexityLevel::Complex);
    }

    #[test]
    fn estimate_expert_complexity() {
        let long_msg = "x".repeat(10000);
        let messages = vec![LlmMessage::user(long_msg)];
        assert_eq!(estimate_complexity(&messages), ComplexityLevel::Expert);
    }

    // -- Routing strategies --

    #[tokio::test]
    async fn route_fixed_returns_specified_model() {
        let router = make_router(RoutingStrategy::Fixed {
            model: "gpt-4o".into(),
        });
        let messages = vec![LlmMessage::user("Hello")];
        let config = LlmConfig::default();

        let decision = router.route(&messages, &config).await;
        assert_eq!(decision.model, "gpt-4o");
        assert_eq!(decision.reason, "fixed model");
    }

    #[tokio::test]
    async fn route_cost_optimized_picks_cheapest_meeting_quality() {
        let router = make_router(RoutingStrategy::CostOptimized { min_quality: 0.85 });
        let messages = vec![LlmMessage::user("Hello")];
        let config = LlmConfig::default();

        let decision = router.route(&messages, &config).await;
        // Should pick the cheapest model with quality >= 0.85
        assert!(decision.quality_score >= 0.85);
        assert!(decision.reason.contains("cheapest"));
    }

    #[tokio::test]
    async fn route_cost_optimized_high_threshold_picks_premium() {
        let router = make_router(RoutingStrategy::CostOptimized { min_quality: 0.95 });
        let messages = vec![LlmMessage::user("Hello")];
        let config = LlmConfig::default();

        let decision = router.route(&messages, &config).await;
        assert!(decision.quality_score >= 0.95);
    }

    #[tokio::test]
    async fn route_complexity_based_trivial_picks_cheap() {
        let router = make_router(RoutingStrategy::ComplexityBased);
        let messages = vec![LlmMessage::user("Hi")];
        let config = LlmConfig::default();

        let decision = router.route(&messages, &config).await;
        // Trivial tasks should route to cheapest model
        assert!(decision.estimated_cost < 0.1);
    }

    #[tokio::test]
    async fn route_complexity_based_expert_picks_premium() {
        let router = make_router(RoutingStrategy::ComplexityBased);
        let long_msg = "x".repeat(10000);
        let messages = vec![LlmMessage::user(long_msg)];
        let config = LlmConfig::default();

        let decision = router.route(&messages, &config).await;
        assert!(decision.quality_score >= 0.90);
    }

    #[tokio::test]
    async fn route_cascade_starts_cheap() {
        let router = make_router(RoutingStrategy::Cascade);
        let messages = vec![LlmMessage::user("Simple question")];
        let config = LlmConfig::default();

        let decision = router.route(&messages, &config).await;
        // Cascade should start with a cheap model for simple queries
        assert!(decision.estimated_cost < 0.1);
    }

    // -- Execute with caching --

    #[tokio::test]
    async fn execute_caches_response() {
        let router = make_router(RoutingStrategy::Fixed {
            model: "test-model".into(),
        });
        let provider = MockProvider::new();
        let messages = vec![LlmMessage::user("Hello")];
        let config = LlmConfig::default();

        // First call — miss
        let (resp1, dec1) = router
            .execute(&provider, &messages, &config, None)
            .await
            .unwrap();
        assert_ne!(dec1.reason, "cache hit");

        // Second call — cache hit
        let (resp2, dec2) = router
            .execute(&provider, &messages, &config, None)
            .await
            .unwrap();
        assert_eq!(dec2.reason, "cache hit");
        assert_eq!(resp1.content, resp2.content);
    }

    #[tokio::test]
    async fn execute_respects_budget() {
        let router = make_router(RoutingStrategy::Fixed {
            model: "test-model".into(),
        });
        let provider = MockProvider::new();
        let messages = vec![LlmMessage::user("Hello")];
        let config = LlmConfig::default();

        // Set a budget that allows exactly 1 request
        router
            .cost_tracker
            .set_budget(
                "task-1".into(),
                crate::cost_tracker::TokenBudget::new(100_000, 10.0, 1),
            )
            .await;

        // First call succeeds
        let result = router
            .execute(&provider, &messages, &config, Some("task-1"))
            .await;
        assert!(result.is_ok());

        // Second call should fail (max_requests=1 exhausted)
        let messages2 = vec![LlmMessage::user("Another question")];
        let result = router
            .execute(&provider, &messages2, &config, Some("task-1"))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn execute_tracks_cost() {
        let router = make_router(RoutingStrategy::Fixed {
            model: "test-model".into(),
        });
        let provider = MockProvider::new();
        let messages = vec![LlmMessage::user("Hello")];
        let config = LlmConfig::default();

        router
            .execute(&provider, &messages, &config, None)
            .await
            .unwrap();

        assert_eq!(router.cost_tracker.request_count().await, 1);
    }

    // -- Add model --

    #[tokio::test]
    async fn add_model_expands_tiers() {
        let router = make_router(RoutingStrategy::CostOptimized { min_quality: 0.99 });

        router
            .add_model(ModelPricing {
                model: "super-model".into(),
                provider: "custom".into(),
                input_cost_per_1m: 0.01,
                output_cost_per_1m: 0.02,
                quality_score: 0.99,
                context_window: 1_000_000,
            })
            .await;

        let messages = vec![LlmMessage::user("Hello")];
        let config = LlmConfig::default();
        let decision = router.route(&messages, &config).await;
        assert_eq!(decision.model, "super-model");
    }

    // -- Strategy serialization --

    #[test]
    fn routing_strategy_serialization() {
        let strategy = RoutingStrategy::CostOptimized { min_quality: 0.85 };
        let json = serde_json::to_string(&strategy).unwrap();
        let deser: RoutingStrategy = serde_json::from_str(&json).unwrap();
        assert_eq!(deser, strategy);
    }

    #[test]
    fn complexity_level_ordering() {
        assert!(ComplexityLevel::Trivial < ComplexityLevel::Simple);
        assert!(ComplexityLevel::Simple < ComplexityLevel::Moderate);
        assert!(ComplexityLevel::Moderate < ComplexityLevel::Complex);
        assert!(ComplexityLevel::Complex < ComplexityLevel::Expert);
    }

    #[test]
    fn complexity_min_quality_increases() {
        let levels = [
            ComplexityLevel::Trivial,
            ComplexityLevel::Simple,
            ComplexityLevel::Moderate,
            ComplexityLevel::Complex,
            ComplexityLevel::Expert,
        ];
        for w in levels.windows(2) {
            assert!(w[0].min_quality() < w[1].min_quality());
        }
    }
}
