use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, warn};

#[derive(Debug, Error)]
pub enum RateLimitError {
    #[error("Rate limit exceeded for {key}. Retry after {retry_after_ms}ms")]
    Exceeded { key: String, retry_after_ms: u64 },
    #[error("Invalid rate limit configuration: {0}")]
    InvalidConfig(String),
}

/// Token bucket for rate limiting
#[derive(Debug, Clone)]
struct TokenBucket {
    tokens: f64,
    capacity: f64,
    refill_rate: f64, // tokens per second
    last_refill: Instant,
}

impl TokenBucket {
    fn new(capacity: f64, refill_rate: f64) -> Self {
        Self {
            tokens: capacity,
            capacity,
            refill_rate,
            last_refill: Instant::now(),
        }
    }
    
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        
        let new_tokens = elapsed * self.refill_rate;
        self.tokens = (self.tokens + new_tokens).min(self.capacity);
        self.last_refill = now;
    }
    
    fn try_consume(&mut self, tokens: f64) -> Result<(), u64> {
        self.refill();
        
        if self.tokens >= tokens {
            self.tokens -= tokens;
            Ok(())
        } else {
            // Calculate retry after time
            let tokens_needed = tokens - self.tokens;
            let retry_after_secs = tokens_needed / self.refill_rate;
            let retry_after_ms = (retry_after_secs * 1000.0) as u64;
            Err(retry_after_ms)
        }
    }
    
    fn available_tokens(&self) -> f64 {
        self.tokens
    }
}

/// Rate limiter configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per time window
    pub max_requests: u32,
    /// Time window duration
    pub window: Duration,
    /// Burst capacity (allows temporary spikes)
    pub burst_capacity: Option<u32>,
}

impl RateLimitConfig {
    pub fn new(max_requests: u32, window: Duration) -> Self {
        Self {
            max_requests,
            window,
            burst_capacity: None,
        }
    }
    
    pub fn with_burst(mut self, burst: u32) -> Self {
        self.burst_capacity = Some(burst);
        self
    }
    
    pub fn per_second(max_requests: u32) -> Self {
        Self::new(max_requests, Duration::from_secs(1))
    }
    
    pub fn per_minute(max_requests: u32) -> Self {
        Self::new(max_requests, Duration::from_secs(60))
    }
    
    pub fn per_hour(max_requests: u32) -> Self {
        Self::new(max_requests, Duration::from_secs(3600))
    }
}

/// Rate limiter with token bucket algorithm
pub struct RateLimiter {
    buckets: Arc<RwLock<HashMap<String, TokenBucket>>>,
    default_config: RateLimitConfig,
    custom_configs: Arc<RwLock<HashMap<String, RateLimitConfig>>>,
}

impl RateLimiter {
    pub fn new(default_config: RateLimitConfig) -> Self {
        debug!("RateLimiter initialized with default config: {:?}", default_config);
        Self {
            buckets: Arc::new(RwLock::new(HashMap::new())),
            default_config,
            custom_configs: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Set custom rate limit for a specific key
    pub async fn set_limit(&self, key: &str, config: RateLimitConfig) {
        let mut configs = self.custom_configs.write().await;
        configs.insert(key.to_string(), config);
        debug!("Set custom rate limit for key '{}': {:?}", key, configs.get(key));
    }
    
    /// Check if request is allowed
    pub async fn check(&self, key: &str) -> Result<(), RateLimitError> {
        self.check_with_cost(key, 1.0).await
    }
    
    /// Check if request is allowed with custom cost
    pub async fn check_with_cost(&self, key: &str, cost: f64) -> Result<(), RateLimitError> {
        let config = {
            let configs = self.custom_configs.read().await;
            configs.get(key).cloned().unwrap_or_else(|| self.default_config.clone())
        };
        
        let capacity = config.burst_capacity.unwrap_or(config.max_requests) as f64;
        let refill_rate = config.max_requests as f64 / config.window.as_secs_f64();
        
        let mut buckets = self.buckets.write().await;
        let bucket = buckets
            .entry(key.to_string())
            .or_insert_with(|| TokenBucket::new(capacity, refill_rate));
        
        match bucket.try_consume(cost) {
            Ok(()) => {
                debug!("Rate limit check passed for '{}' (cost: {}, remaining: {:.2})", 
                    key, cost, bucket.available_tokens());
                Ok(())
            }
            Err(retry_after_ms) => {
                warn!("Rate limit exceeded for '{}' (cost: {}, retry after: {}ms)", 
                    key, cost, retry_after_ms);
                Err(RateLimitError::Exceeded {
                    key: key.to_string(),
                    retry_after_ms,
                })
            }
        }
    }
    
    /// Get remaining capacity for a key
    pub async fn remaining(&self, key: &str) -> f64 {
        let buckets = self.buckets.read().await;
        buckets.get(key).map(|b| b.available_tokens()).unwrap_or(0.0)
    }
    
    /// Reset rate limit for a key
    pub async fn reset(&self, key: &str) {
        let mut buckets = self.buckets.write().await;
        buckets.remove(key);
        debug!("Reset rate limit for key '{}'", key);
    }
    
    /// Clear all rate limits
    pub async fn clear_all(&self) {
        let mut buckets = self.buckets.write().await;
        buckets.clear();
        debug!("Cleared all rate limits");
    }
}

/// Rate limiter with per-user and per-endpoint limits
pub struct MultiKeyRateLimiter {
    global_limiter: RateLimiter,
    user_limiter: RateLimiter,
    endpoint_limiter: RateLimiter,
}

impl MultiKeyRateLimiter {
    pub fn new(
        global_config: RateLimitConfig,
        user_config: RateLimitConfig,
        endpoint_config: RateLimitConfig,
    ) -> Self {
        Self {
            global_limiter: RateLimiter::new(global_config),
            user_limiter: RateLimiter::new(user_config),
            endpoint_limiter: RateLimiter::new(endpoint_config),
        }
    }
    
    /// Check all rate limits (global, user, endpoint)
    pub async fn check_all(
        &self,
        user_id: &str,
        endpoint: &str,
    ) -> Result<(), RateLimitError> {
        // Check global limit
        self.global_limiter.check("global").await?;
        
        // Check user limit
        self.user_limiter.check(user_id).await?;
        
        // Check endpoint limit
        self.endpoint_limiter.check(endpoint).await?;
        
        Ok(())
    }
    
    /// Check with custom cost
    pub async fn check_all_with_cost(
        &self,
        user_id: &str,
        endpoint: &str,
        cost: f64,
    ) -> Result<(), RateLimitError> {
        self.global_limiter.check_with_cost("global", cost).await?;
        self.user_limiter.check_with_cost(user_id, cost).await?;
        self.endpoint_limiter.check_with_cost(endpoint, cost).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[test]
    fn test_token_bucket_basic() {
        let mut bucket = TokenBucket::new(10.0, 1.0);
        
        // Should succeed
        assert!(bucket.try_consume(5.0).is_ok());
        assert!((bucket.available_tokens() - 5.0).abs() < 0.1);
        
        // Should succeed
        assert!(bucket.try_consume(5.0).is_ok());
        assert!(bucket.available_tokens() < 0.1); // Close to 0
        
        // Should fail
        assert!(bucket.try_consume(1.0).is_err());
    }

    #[tokio::test]
    async fn test_rate_limiter_basic() {
        let config = RateLimitConfig::per_second(5);
        let limiter = RateLimiter::new(config);
        
        // First 5 requests should succeed
        for i in 0..5 {
            assert!(limiter.check("test-key").await.is_ok(), "Request {} failed", i);
        }
        
        // 6th request should fail
        assert!(limiter.check("test-key").await.is_err());
    }

    #[tokio::test]
    async fn test_rate_limiter_refill() {
        let config = RateLimitConfig::new(2, Duration::from_millis(100));
        let limiter = RateLimiter::new(config);
        
        // Use up tokens
        assert!(limiter.check("test-key").await.is_ok());
        assert!(limiter.check("test-key").await.is_ok());
        assert!(limiter.check("test-key").await.is_err());
        
        // Wait for refill
        sleep(Duration::from_millis(150)).await;
        
        // Should succeed now
        assert!(limiter.check("test-key").await.is_ok());
    }

    #[tokio::test]
    async fn test_rate_limiter_custom_config() {
        let default_config = RateLimitConfig::per_second(5);
        let limiter = RateLimiter::new(default_config);
        
        // Set custom limit for specific key
        let custom_config = RateLimitConfig::per_second(10);
        limiter.set_limit("premium-user", custom_config).await;
        
        // Premium user should have higher limit
        for _ in 0..10 {
            assert!(limiter.check("premium-user").await.is_ok());
        }
        assert!(limiter.check("premium-user").await.is_err());
        
        // Regular user should have default limit
        for _ in 0..5 {
            assert!(limiter.check("regular-user").await.is_ok());
        }
        assert!(limiter.check("regular-user").await.is_err());
    }

    #[tokio::test]
    async fn test_rate_limiter_with_cost() {
        let config = RateLimitConfig::per_second(10);
        let limiter = RateLimiter::new(config);
        
        // Consume 3 tokens
        assert!(limiter.check_with_cost("test-key", 3.0).await.is_ok());
        
        // Consume 5 tokens
        assert!(limiter.check_with_cost("test-key", 5.0).await.is_ok());
        
        // Try to consume 3 more tokens (should fail, only 2 remaining)
        assert!(limiter.check_with_cost("test-key", 3.0).await.is_err());
    }

    #[tokio::test]
    async fn test_rate_limiter_reset() {
        let config = RateLimitConfig::per_second(2);
        let limiter = RateLimiter::new(config);
        
        // Use up tokens
        assert!(limiter.check("test-key").await.is_ok());
        assert!(limiter.check("test-key").await.is_ok());
        assert!(limiter.check("test-key").await.is_err());
        
        // Reset
        limiter.reset("test-key").await;
        
        // Should succeed now
        assert!(limiter.check("test-key").await.is_ok());
    }

    #[tokio::test]
    async fn test_multi_key_rate_limiter() {
        let global_config = RateLimitConfig::per_second(100);
        let user_config = RateLimitConfig::per_second(10);
        let endpoint_config = RateLimitConfig::per_second(20);
        
        let limiter = MultiKeyRateLimiter::new(global_config, user_config, endpoint_config);
        
        // Should succeed within all limits
        for _ in 0..10 {
            assert!(limiter.check_all("user1", "/api/chat").await.is_ok());
        }
        
        // Should fail user limit
        assert!(limiter.check_all("user1", "/api/chat").await.is_err());
        
        // Different user should still work
        assert!(limiter.check_all("user2", "/api/chat").await.is_ok());
    }

    #[tokio::test]
    async fn test_rate_limit_config_builders() {
        let per_sec = RateLimitConfig::per_second(10);
        assert_eq!(per_sec.max_requests, 10);
        assert_eq!(per_sec.window, Duration::from_secs(1));
        
        let per_min = RateLimitConfig::per_minute(60);
        assert_eq!(per_min.max_requests, 60);
        assert_eq!(per_min.window, Duration::from_secs(60));
        
        let per_hour = RateLimitConfig::per_hour(3600);
        assert_eq!(per_hour.max_requests, 3600);
        assert_eq!(per_hour.window, Duration::from_secs(3600));
        
        let with_burst = RateLimitConfig::per_second(10).with_burst(20);
        assert_eq!(with_burst.burst_capacity, Some(20));
    }
}
