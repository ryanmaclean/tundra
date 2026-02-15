use at_harness::rate_limiter::{MultiKeyRateLimiter, RateLimitConfig, RateLimitError, RateLimiter};

#[test]
fn allows_requests_within_limit() {
    let limiter = RateLimiter::new(RateLimitConfig::per_second(10));

    for _ in 0..10 {
        assert!(limiter.check("user-1").is_ok());
    }
}

#[test]
fn rejects_when_exhausted() {
    let limiter = RateLimiter::new(RateLimitConfig::per_second(5));

    // Exhaust the bucket
    for _ in 0..5 {
        limiter.check("user-1").unwrap();
    }

    let result = limiter.check("user-1");
    assert!(result.is_err());
    assert!(matches!(result, Err(RateLimitError::Exceeded { .. })));
}

#[test]
fn separate_keys_have_separate_buckets() {
    let limiter = RateLimiter::new(RateLimitConfig::per_second(2));

    limiter.check("user-a").unwrap();
    limiter.check("user-a").unwrap();
    // user-a is exhausted
    assert!(limiter.check("user-a").is_err());
    // user-b is independent
    assert!(limiter.check("user-b").is_ok());
}

#[test]
fn per_minute_config() {
    let config = RateLimitConfig::per_minute(60);
    // 60 per minute = 1 per second
    assert!((config.tokens_per_second - 1.0).abs() < f64::EPSILON);
    assert!((config.max_burst - 60.0).abs() < f64::EPSILON);
}

#[test]
fn per_hour_config() {
    let config = RateLimitConfig::per_hour(3600);
    // 3600 per hour = 1 per second
    assert!((config.tokens_per_second - 1.0).abs() < f64::EPSILON);
}

#[test]
fn with_burst_override() {
    let config = RateLimitConfig::per_second(10).with_burst(20);
    assert!((config.max_burst - 20.0).abs() < f64::EPSILON);

    let limiter = RateLimiter::new(config);
    // Should allow 20 burst requests
    for _ in 0..20 {
        assert!(limiter.check("user-1").is_ok());
    }
    assert!(limiter.check("user-1").is_err());
}

#[test]
fn check_with_cost() {
    let limiter = RateLimiter::new(RateLimitConfig::per_second(10));

    // Use 5 tokens at once
    assert!(limiter.check_with_cost("user-1", 5.0).is_ok());
    // Use another 5
    assert!(limiter.check_with_cost("user-1", 5.0).is_ok());
    // Bucket empty
    assert!(limiter.check_with_cost("user-1", 1.0).is_err());
}

#[test]
fn remaining_tokens() {
    let limiter = RateLimiter::new(RateLimitConfig::per_second(10));

    // Before any usage, should be full
    let rem = limiter.remaining("user-1");
    assert!((rem - 10.0).abs() < 1.0);

    limiter.check("user-1").unwrap();
    let rem = limiter.remaining("user-1");
    // Should be around 9 (plus tiny refill)
    assert!(rem < 10.0);
    assert!(rem >= 8.5);
}

#[test]
fn multi_key_limiter() {
    let limiter = MultiKeyRateLimiter::new(
        RateLimitConfig::per_second(100),   // global
        RateLimitConfig::per_second(5),     // per user
        RateLimitConfig::per_second(50),    // per endpoint
    );

    for _ in 0..5 {
        assert!(limiter.check_all("user-1", "/api/chat").is_ok());
    }

    // User limit exhausted
    assert!(limiter.check_all("user-1", "/api/chat").is_err());

    // Different user still works
    assert!(limiter.check_all("user-2", "/api/chat").is_ok());
}

#[test]
fn error_message_includes_key() {
    let limiter = RateLimiter::new(RateLimitConfig::per_second(1));
    limiter.check("my-key").unwrap();
    let err = limiter.check("my-key").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("my-key"), "error should contain key name: {msg}");
}
