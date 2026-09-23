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

    /// Refill, then report whether `cost` tokens are available *without*
    /// consuming them.  On failure returns the duration until enough tokens
    /// will have accumulated.
    fn peek(&mut self, cost: f64, tokens_per_second: f64, max_burst: f64) -> Result<(), Duration> {
        self.refill(tokens_per_second, max_burst);
        if self.tokens >= cost {
            Ok(())
        } else {
            Err(retry_after(cost - self.tokens, tokens_per_second))
        }
    }
}

/// Convert a token deficit into a retry duration.
///
/// A zero (or negative / non-finite) refill rate would produce `inf`, which
/// makes `Duration::from_secs_f64` panic.  Saturate to `Duration::MAX` instead:
/// a bucket that never refills never admits the request.
fn retry_after(deficit: f64, tokens_per_second: f64) -> Duration {
    if tokens_per_second <= 0.0 || !tokens_per_second.is_finite() {
        return Duration::MAX;
    }
    Duration::try_from_secs_f64(deficit / tokens_per_second).unwrap_or(Duration::MAX)
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
        let mut bucket = self.bucket(key);
        self.peek(&mut bucket, key, cost)?;
        bucket.tokens -= cost;
        Ok(())
    }

    /// Get (creating if needed) the bucket for `key`.  The returned guard
    /// holds the DashMap shard lock until dropped.
    fn bucket(&self, key: &str) -> dashmap::mapref::one::RefMut<'_, String, TokenBucket> {
        self.buckets
            .entry(key.to_string())
            .or_insert_with(|| TokenBucket::new(self.config.max_burst))
    }

    /// Refill `bucket` and verify `cost` tokens are available, without
    /// spending them.
    fn peek(&self, bucket: &mut TokenBucket, key: &str, cost: f64) -> Result<(), RateLimitError> {
        bucket
            .peek(cost, self.config.tokens_per_second, self.config.max_burst)
            .map_err(|retry_after| {
                warn!(key, ?retry_after, "rate limit exceeded");
                RateLimitError::Exceeded {
                    key: key.to_string(),
                    retry_after,
                }
            })
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
    ///
    /// Tokens are only spent when **every** tier admits the request, so a
    /// rejected request never drains a shared bucket (see
    /// [`Self::check_tiers`]).
    pub fn check_all(&self, user_key: &str, endpoint_key: &str) -> Result<(), RateLimitError> {
        self.check_tiers(Some(user_key), Some(endpoint_key), 1.0)
    }

    /// Check all three tiers with a custom cost.
    pub fn check_all_with_cost(
        &self,
        user_key: &str,
        endpoint_key: &str,
        cost: f64,
    ) -> Result<(), RateLimitError> {
        self.check_tiers(Some(user_key), Some(endpoint_key), cost)
    }

    /// Check-then-commit across the tiers.
    ///
    /// `None` for `user_key` / `endpoint_key` skips that tier (e.g. for
    /// exempt clients); the global tier always applies.
    ///
    /// Tiers are checked from most to least specific (per-user, per-endpoint,
    /// global) while holding every bucket's shard guard, and tokens are only
    /// deducted once all tiers pass.  Guards are always acquired in the same
    /// order, so concurrent callers cannot deadlock.
    pub fn check_tiers(
        &self,
        user_key: Option<&str>,
        endpoint_key: Option<&str>,
        cost: f64,
    ) -> Result<(), RateLimitError> {
        let mut user = user_key.map(|k| (k, self.per_user.bucket(k)));
        let mut endpoint = endpoint_key.map(|k| (k, self.per_endpoint.bucket(k)));
        let mut global = self.global.bucket("global");

        if let Some((k, b)) = user.as_mut() {
            self.per_user.peek(b, k, cost)?;
        }
        if let Some((k, b)) = endpoint.as_mut() {
            self.per_endpoint.peek(b, k, cost)?;
        }
        self.global.peek(&mut global, "global", cost)?;

        if let Some((_, b)) = user.as_mut() {
            b.tokens -= cost;
        }
        if let Some((_, b)) = endpoint.as_mut() {
            b.tokens -= cost;
        }
        global.tokens -= cost;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_requests_do_not_drain_global_bucket() {
        // Production-shaped config: 100 global, 20 per user, 1000 per endpoint.
        let limiter = MultiKeyRateLimiter::new(
            RateLimitConfig::per_minute(100),
            RateLimitConfig::per_minute(20),
            RateLimitConfig::per_minute(1000),
        );
        let mut rejected = 0;
        for _ in 0..100 {
            if limiter.check_all("attacker", "/api/beads").is_err() {
                rejected += 1;
            }
        }
        assert_eq!(rejected, 80);
        // Only the 20 admitted requests spent global tokens.
        assert!(limiter.global.remaining("global") >= 79.0);
        assert!(limiter.check_all("innocent", "/api/beads").is_ok());
    }

    #[test]
    fn endpoint_rejection_does_not_drain_user_bucket() {
        let limiter = MultiKeyRateLimiter::new(
            RateLimitConfig::per_minute(1000),
            RateLimitConfig::per_minute(10),
            RateLimitConfig::per_minute(2),
        );
        assert!(limiter.check_all("u", "/a").is_ok());
        assert!(limiter.check_all("u", "/a").is_ok());
        for _ in 0..20 {
            assert!(limiter.check_all("u", "/a").is_err());
        }
        // 2 spent, 8 left for other endpoints.
        for i in 0..8 {
            assert!(limiter.check_all("u", &format!("/b{i}")).is_ok());
        }
        assert!(limiter.check_all("u", "/c").is_err());
    }

    #[test]
    fn skipped_tiers_are_not_consulted() {
        let limiter = MultiKeyRateLimiter::new(
            RateLimitConfig::per_minute(1000),
            RateLimitConfig::per_minute(1),
            RateLimitConfig::per_minute(1),
        );
        for _ in 0..10 {
            assert!(limiter.check_tiers(None, None, 1.0).is_ok());
        }
        assert!(limiter.global.remaining("global") < 991.0);
    }

    #[test]
    fn zero_rate_config_denies_without_panicking() {
        let limiter = RateLimiter::new(RateLimitConfig::per_minute(0));
        match limiter.check("k") {
            Err(RateLimitError::Exceeded { retry_after, .. }) => {
                assert_eq!(retry_after, Duration::MAX)
            }
            Ok(()) => panic!("zero-capacity limiter must deny"),
        }
    }
}
