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
    /// Override `last_refill` so tests can simulate time passing without
    /// actually sleeping.  Only compiled in test builds.
    #[cfg(test)]
    fn set_last_refill(&mut self, t: Instant) {
        self.last_refill = t;
    }
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

    /// Returns the raw stored token count WITHOUT applying elapsed-time refill.
    /// Only compiled in test builds; used to assert exact post-call state.
    #[cfg(test)]
    fn raw_tokens(&self, key: &str) -> Option<f64> {
        self.buckets.get(key).map(|b| b.tokens)
    }

    /// Back-date the `last_refill` timestamp for `key` by `elapsed` so that
    /// the next `check*` call will apply the equivalent refill.  Creates a
    /// fresh bucket if none exists yet.  Only compiled in test builds.
    #[cfg(test)]
    pub(crate) fn force_last_refill_offset(&self, key: &str, elapsed: Duration) {
        // Use checked_sub so a test passing a very large `elapsed` saturates
        // at "epoch start" rather than panicking on Instant underflow.
        let now = Instant::now();
        let past = now.checked_sub(elapsed).unwrap_or(now);
        self.buckets
            .entry(key.to_string())
            .or_insert_with(|| TokenBucket::new(self.config.max_burst))
            .set_last_refill(past);
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

    /// Expose inner limiters for test seams only.
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn per_user_limiter(&self) -> &RateLimiter {
        &self.per_user
    }

    #[cfg(test)]
    pub(crate) fn per_endpoint_limiter(&self) -> &RateLimiter {
        &self.per_endpoint
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn global_limiter(&self) -> &RateLimiter {
        &self.global
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Test 1: single check within capacity succeeds
    // -----------------------------------------------------------------------

    /// A fresh limiter with 10-token burst allows a cost-1 call and
    /// decrements the bucket to exactly 9 tokens.
    #[test]
    fn single_check_within_capacity_succeeds() {
        let limiter = RateLimiter::new(RateLimitConfig::per_second(10));

        let result = limiter.check_with_cost("alice", 1.0);

        assert!(result.is_ok(), "first call should succeed");
        // Raw tokens must be 9 — no wall-clock elapsed because we called
        // immediately after construction.
        let raw = limiter.raw_tokens("alice").expect("bucket must exist");
        assert!(
            (raw - 9.0).abs() < 0.01,
            "expected ~9.0 tokens remaining, got {raw}"
        );
    }

    // -----------------------------------------------------------------------
    // Test 2: exhaust capacity → next call returns Err with non-zero retry_after
    // -----------------------------------------------------------------------

    /// After draining all 10 tokens, a subsequent request must be rejected
    /// with a `retry_after` that is strictly positive.
    #[test]
    fn exhaust_capacity_returns_retry_after() {
        let limiter = RateLimiter::new(RateLimitConfig::per_second(10));

        // Consume all 10 tokens in one shot.
        assert!(
            limiter.check_with_cost("bob", 10.0).is_ok(),
            "draining the bucket should succeed"
        );

        // Any further cost should fail immediately.
        let err = limiter
            .check_with_cost("bob", 1.0)
            .expect_err("should be rejected after exhaustion");

        match err {
            RateLimitError::Exceeded { key, retry_after } => {
                assert_eq!(key, "bob");
                assert!(
                    retry_after > Duration::ZERO,
                    "retry_after must be strictly positive, got {retry_after:?}"
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Test 3: retry_after is proportional to deficit / refill_rate
    // -----------------------------------------------------------------------

    /// When the bucket has `available` tokens and we ask for `cost > available`,
    /// the returned `retry_after` must equal `(cost - available) / rate`.
    /// This pins the arithmetic and guards against division-rounding anomalies.
    #[test]
    fn retry_after_is_proportional_to_deficit() {
        // 4 tokens/s, burst=4 → starts full.
        let config = RateLimitConfig {
            tokens_per_second: 4.0,
            max_burst: 4.0,
            window: Duration::from_secs(1),
        };
        let limiter = RateLimiter::new(config);

        // Consume 3 tokens, leaving 1.
        limiter.check_with_cost("carol", 3.0).unwrap();

        // Ask for cost=3 when only 1 remains → deficit = 2.
        // Expected wait = 2 / 4 = 0.5 s.
        let err = limiter
            .check_with_cost("carol", 3.0)
            .expect_err("should be rejected");

        match err {
            RateLimitError::Exceeded { retry_after, .. } => {
                let secs = retry_after.as_secs_f64();
                // deficit = 3 - raw_tokens_after_first_check
                // We called check_with_cost immediately, so raw ≈ 1.0
                // wait ≈ 2.0 / 4.0 = 0.5 s
                assert!(
                    (secs - 0.5).abs() < 0.05,
                    "expected retry_after ≈ 0.5 s, got {secs:.4} s"
                );
                assert!(
                    secs > 0.0,
                    "retry_after must never be zero (overflow/rounding guard)"
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Test 4: zero-cost call succeeds without decrementing tokens
    // -----------------------------------------------------------------------

    /// `check_with_cost(0.0)` is a no-op; the bucket should retain all its
    /// tokens.
    #[test]
    fn zero_cost_call_succeeds_without_decrement() {
        let limiter = RateLimiter::new(RateLimitConfig::per_second(10));

        let result = limiter.check_with_cost("dave", 0.0);

        assert!(result.is_ok(), "zero-cost call must succeed");
        let raw = limiter.raw_tokens("dave").expect("bucket must exist");
        assert!(
            (raw - 10.0).abs() < 0.01,
            "tokens should be unchanged (≈10), got {raw}"
        );
    }

    // -----------------------------------------------------------------------
    // Test 5: MultiKeyRateLimiter — all tiers must pass; per-user tier rejects
    // -----------------------------------------------------------------------

    /// With per-user burst=5 and per-endpoint burst=10, five calls exhaust the
    /// per-user bucket.  The sixth call must be rejected and the error key must
    /// identify the per-user key ("alice"), not the endpoint key.
    #[test]
    fn multi_key_all_tiers_must_pass() {
        // Global is generous so it never interferes.
        let multi = MultiKeyRateLimiter::new(
            RateLimitConfig::per_second(1000), // global — effectively unlimited
            RateLimitConfig::per_second(5),    // per-user burst=5
            RateLimitConfig::per_second(10),   // per-endpoint burst=10
        );

        // Five calls must all succeed.
        for i in 0..5 {
            assert!(
                multi.check_all("alice", "/foo").is_ok(),
                "call {i} should succeed"
            );
        }

        // Sixth call must fail — per-user bucket exhausted.
        let err = multi
            .check_all("alice", "/foo")
            .expect_err("6th call must be rejected");

        match &err {
            RateLimitError::Exceeded { key, retry_after } => {
                assert_eq!(
                    key, "alice",
                    "the failing tier must be the per-user one (key='alice'), got key='{key}'"
                );
                assert!(
                    *retry_after > Duration::ZERO,
                    "retry_after must be positive"
                );
            }
        }

        // The per-endpoint bucket should still have 5 tokens (only 5 requests
        // made it through to the endpoint tier).
        let endpoint_raw = multi
            .per_endpoint_limiter()
            .raw_tokens("/foo")
            .expect("endpoint bucket must exist");
        assert!(
            (endpoint_raw - 5.0).abs() < 0.1,
            "per-endpoint bucket should have ~5 tokens left, got {endpoint_raw}"
        );
    }

    // -----------------------------------------------------------------------
    // Test 6: tier ordering — per-endpoint bucket is not over-decremented
    //         when a different user exhausts the per-user tier
    // -----------------------------------------------------------------------

    /// Exhaust alice's per-user bucket (5 calls).  Then bob (fresh per-user
    /// bucket) calls the same endpoint.  Because per-endpoint has only been
    /// decremented 5 times (not 6), it still has 5 tokens left and bob's
    /// call must succeed.
    ///
    /// This also verifies that alice's rejected 6th call did NOT decrement
    /// the per-endpoint counter (no double-decrement on failure).
    #[test]
    fn multi_key_tier_order_does_not_starve_lower_tiers() {
        let multi = MultiKeyRateLimiter::new(
            RateLimitConfig::per_second(1000), // global — effectively unlimited
            RateLimitConfig::per_second(5),    // per-user burst=5
            RateLimitConfig::per_second(10),   // per-endpoint burst=10
        );

        // Five successful calls from alice (exhausts alice's per-user bucket).
        for i in 0..5 {
            assert!(
                multi.check_all("alice", "/bar").is_ok(),
                "alice call {i} should succeed"
            );
        }

        // Alice's 6th call is rejected at the per-user tier.
        // The per-endpoint bucket must NOT have been decremented by this call.
        assert!(
            multi.check_all("alice", "/bar").is_err(),
            "alice's 6th call should be rejected"
        );

        // Per-endpoint bucket should still have 5 tokens (decremented by
        // alice's 5 successful calls, not the rejected one).
        let ep_after_alice_rejection = multi
            .per_endpoint_limiter()
            .raw_tokens("/bar")
            .expect("endpoint bucket must exist");
        assert!(
            (ep_after_alice_rejection - 5.0).abs() < 0.1,
            "per-endpoint should have ~5 tokens after alice's rejection, got {ep_after_alice_rejection}"
        );

        // Bob (fresh per-user bucket) calls the same endpoint — must succeed
        // because per-endpoint still has tokens.
        assert!(
            multi.check_all("bob", "/bar").is_ok(),
            "bob's first call must succeed (per-user fresh + per-endpoint has budget)"
        );

        // Now per-endpoint has 4 tokens left.
        let ep_after_bob = multi
            .per_endpoint_limiter()
            .raw_tokens("/bar")
            .expect("endpoint bucket must exist");
        assert!(
            (ep_after_bob - 4.0).abs() < 0.1,
            "per-endpoint should have ~4 tokens after bob's call, got {ep_after_bob}"
        );
    }

    // -----------------------------------------------------------------------
    // Test 7: refill restores tokens after time passes (clock-seam test)
    // -----------------------------------------------------------------------

    /// Exhaust the bucket, then back-date `last_refill` by 1 second so the
    /// next call sees 10 new tokens from the refill.  Assert the call succeeds.
    #[test]
    fn refill_restores_tokens_after_elapsed_time() {
        let limiter = RateLimiter::new(RateLimitConfig::per_second(10));

        // Drain completely.
        limiter.check_with_cost("eve", 10.0).unwrap();
        assert!(
            limiter.check_with_cost("eve", 1.0).is_err(),
            "should be exhausted"
        );

        // Simulate 1 full second of elapsed time via the test seam.
        limiter.force_last_refill_offset("eve", Duration::from_secs(1));

        // After refill, 10 new tokens are available.
        assert!(
            limiter.check_with_cost("eve", 10.0).is_ok(),
            "after simulated 1-second refill, a full-bucket draw must succeed"
        );
    }

    // -----------------------------------------------------------------------
    // Test 8: boundary — exact cost equal to available tokens is accepted
    // -----------------------------------------------------------------------

    /// `tokens >= cost` (not `>`) means an exact-boundary draw must succeed.
    /// This test pins the off-by-one boundary and would catch the mutation
    /// `tokens > cost`.
    #[test]
    fn exact_boundary_cost_is_accepted() {
        let limiter = RateLimiter::new(RateLimitConfig::per_second(10));

        // A cost exactly equal to the full burst must succeed.
        let result = limiter.check_with_cost("frank", 10.0);
        assert!(
            result.is_ok(),
            "cost == max_burst must be accepted (>= boundary), got {result:?}"
        );

        // Now the bucket is empty — asking for anything more must fail.
        assert!(
            limiter.check_with_cost("frank", 0.001).is_err(),
            "bucket empty: even tiny cost must be rejected"
        );
    }
}
