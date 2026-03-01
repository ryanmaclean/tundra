use dashmap::DashMap;
use std::time::{Duration, Instant};
use tracing::warn;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

/// Errors that can occur when enforcing rate limits.
///
/// Rate limiters use a token bucket algorithm to control request rates across
/// different keys (e.g., users, endpoints, global limits). This error indicates
/// that a rate limit has been exceeded and provides timing information for retry.
///
/// # Examples
///
/// ```rust
/// use at_harness::rate_limiter::{RateLimiter, RateLimitConfig, RateLimitError};
///
/// fn handle_rate_limit() {
///     let limiter = RateLimiter::new(RateLimitConfig::per_second(10));
///
///     match limiter.check("user_123") {
///         Err(RateLimitError::Exceeded { key, retry_after }) => {
///             println!("Rate limit exceeded for '{}', retry after {:?}", key, retry_after);
///             // Implement exponential backoff or wait for retry_after duration
///         }
///         Ok(()) => {
///             // Request allowed, proceed with operation
///         }
///     }
/// }
/// ```
#[derive(Debug, thiserror::Error)]
pub enum RateLimitError {
    /// The rate limit was exceeded for the specified key.
    ///
    /// The token bucket for this key has insufficient tokens to allow the request.
    /// This indicates that the request rate has exceeded the configured limit.
    ///
    /// Callers should implement retry logic that respects the `retry_after` duration,
    /// typically using exponential backoff strategies to avoid overwhelming the system.
    ///
    /// # Fields
    ///
    /// - `key`: The rate limit key that was exceeded (e.g., user ID, endpoint name)
    /// - `retry_after`: Duration to wait before the next request would be allowed
    #[error("rate limit exceeded for key `{key}` – retry after {retry_after:?}")]
    Exceeded {
        /// The rate limit key that exceeded its quota.
        key: String,
        /// Duration to wait before retrying.
        retry_after: Duration,
    },
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Tokens added per second.
    pub tokens_per_second: f64,
    /// Maximum burst size (bucket capacity).
    pub max_burst: f64,
    /// Window duration (informational, used for helper constructors).
    pub window: Duration,
}

impl RateLimitConfig {
    /// Allow `count` requests per second.
    pub fn per_second(count: u64) -> Self {
        Self {
            tokens_per_second: count as f64,
            max_burst: count as f64,
            window: Duration::from_secs(1),
        }
    }

    /// Allow `count` requests per minute.
    pub fn per_minute(count: u64) -> Self {
        Self {
            tokens_per_second: count as f64 / 60.0,
            max_burst: count as f64,
            window: Duration::from_secs(60),
        }
    }

    /// Allow `count` requests per hour.
    pub fn per_hour(count: u64) -> Self {
        Self {
            tokens_per_second: count as f64 / 3600.0,
            max_burst: count as f64,
            window: Duration::from_secs(3600),
        }
    }

    /// Override the max burst capacity.
    pub fn with_burst(mut self, burst: u64) -> Self {
        self.max_burst = burst as f64;
        self
    }
}

// ---------------------------------------------------------------------------
// Bucket (per-key state)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
}

impl TokenBucket {
    fn new(max_burst: f64) -> Self {
        Self {
            tokens: max_burst,
            last_refill: Instant::now(),
        }
    }

    /// Refill tokens based on elapsed time, capped at `max_burst`.
    fn refill(&mut self, tokens_per_second: f64, max_burst: f64) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * tokens_per_second).min(max_burst);
        self.last_refill = now;
    }

    /// Try to consume `cost` tokens.  Returns `Ok(())` or an error with retry
    /// duration.
    fn try_consume(
        &mut self,
        cost: f64,
        tokens_per_second: f64,
        max_burst: f64,
    ) -> Result<(), Duration> {
        self.refill(tokens_per_second, max_burst);
        if self.tokens >= cost {
            self.tokens -= cost;
            Ok(())
        } else {
            let deficit = cost - self.tokens;
            let wait = Duration::from_secs_f64(deficit / tokens_per_second);
            Err(wait)
        }
    }
}

// ---------------------------------------------------------------------------
// RateLimiter
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct RateLimiter {
    config: RateLimitConfig,
    buckets: DashMap<String, TokenBucket>,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            buckets: DashMap::new(),
        }
    }

    /// Check whether a single-cost request is allowed for `key`.
    pub fn check(&self, key: &str) -> Result<(), RateLimitError> {
        self.check_with_cost(key, 1.0)
    }

    /// Check whether a request with the given `cost` is allowed for `key`.
    pub fn check_with_cost(&self, key: &str, cost: f64) -> Result<(), RateLimitError> {
        let mut bucket = self
            .buckets
            .entry(key.to_string())
            .or_insert_with(|| TokenBucket::new(self.config.max_burst));

        match bucket.try_consume(cost, self.config.tokens_per_second, self.config.max_burst) {
            Ok(()) => Ok(()),
            Err(retry_after) => {
                warn!(key, ?retry_after, "rate limit exceeded");
                Err(RateLimitError::Exceeded {
                    key: key.to_string(),
                    retry_after,
                })
            }
        }
    }

    /// Returns the approximate number of tokens remaining for `key`.
    pub fn remaining(&self, key: &str) -> f64 {
        match self.buckets.get(key) {
            Some(bucket) => {
                let elapsed = bucket.last_refill.elapsed().as_secs_f64();
                (bucket.tokens + elapsed * self.config.tokens_per_second).min(self.config.max_burst)
            }
            None => self.config.max_burst,
        }
    }
}

// ---------------------------------------------------------------------------
// MultiKeyRateLimiter
// ---------------------------------------------------------------------------

/// Enforces multiple rate-limit tiers: global, per-user, and per-endpoint.
#[derive(Debug)]
pub struct MultiKeyRateLimiter {
    global: RateLimiter,
    per_user: RateLimiter,
    per_endpoint: RateLimiter,
}

impl MultiKeyRateLimiter {
    pub fn new(
        global_config: RateLimitConfig,
        per_user_config: RateLimitConfig,
        per_endpoint_config: RateLimitConfig,
    ) -> Self {
        Self {
            global: RateLimiter::new(global_config),
            per_user: RateLimiter::new(per_user_config),
            per_endpoint: RateLimiter::new(per_endpoint_config),
        }
    }

    /// Check all three tiers.  Returns the first error encountered.
    pub fn check_all(&self, user_key: &str, endpoint_key: &str) -> Result<(), RateLimitError> {
        self.global.check("global")?;
        self.per_user.check(user_key)?;
        self.per_endpoint.check(endpoint_key)?;
        Ok(())
    }

    /// Check all three tiers with a custom cost.
    pub fn check_all_with_cost(
        &self,
        user_key: &str,
        endpoint_key: &str,
        cost: f64,
    ) -> Result<(), RateLimitError> {
        self.global.check_with_cost("global", cost)?;
        self.per_user.check_with_cost(user_key, cost)?;
        self.per_endpoint.check_with_cost(endpoint_key, cost)?;
        Ok(())
    }
}
