use at_harness::circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError, CircuitState,
};
use std::time::Duration;

fn fast_config() -> CircuitBreakerConfig {
    CircuitBreakerConfig {
        failure_threshold: 3,
        success_threshold: 2,
        timeout: Duration::from_millis(100),
        call_timeout: Duration::from_secs(5),
    }
}

#[tokio::test]
async fn starts_closed() {
    let cb = CircuitBreaker::new(fast_config());
    assert_eq!(cb.state().await, CircuitState::Closed);
}

#[tokio::test]
async fn stays_closed_on_success() {
    let cb = CircuitBreaker::new(fast_config());
    let res: Result<i32, CircuitBreakerError> = cb.call(|| async { Ok::<_, String>(42) }).await;
    assert_eq!(res.unwrap(), 42);
    assert_eq!(cb.state().await, CircuitState::Closed);
}

#[tokio::test]
async fn opens_after_threshold_failures() {
    let cb = CircuitBreaker::new(fast_config());

    for _ in 0..3 {
        let _ = cb.call(|| async { Err::<i32, _>("fail") }).await;
    }

    assert_eq!(cb.state().await, CircuitState::Open);
}

#[tokio::test]
async fn rejects_calls_when_open() {
    let cb = CircuitBreaker::new(fast_config());

    // Trip the breaker
    for _ in 0..3 {
        let _ = cb.call(|| async { Err::<i32, _>("fail") }).await;
    }

    let result = cb.call(|| async { Ok::<_, String>(1) }).await;
    assert!(matches!(result, Err(CircuitBreakerError::Open)));
}

#[tokio::test]
async fn transitions_to_half_open_after_timeout() {
    let cb = CircuitBreaker::new(fast_config());

    // Trip the breaker
    for _ in 0..3 {
        let _ = cb.call(|| async { Err::<i32, _>("fail") }).await;
    }
    assert_eq!(cb.state().await, CircuitState::Open);

    // Wait for the timeout
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Next call should be allowed (transitions to HalfOpen then executes)
    let result = cb.call(|| async { Ok::<_, String>(99) }).await;
    assert_eq!(result.unwrap(), 99);
}

#[tokio::test]
async fn recovers_from_half_open_to_closed() {
    let config = CircuitBreakerConfig {
        failure_threshold: 2,
        success_threshold: 2,
        timeout: Duration::from_millis(50),
        call_timeout: Duration::from_secs(5),
    };
    let cb = CircuitBreaker::new(config);

    // Trip the breaker
    for _ in 0..2 {
        let _ = cb.call(|| async { Err::<i32, _>("fail") }).await;
    }
    assert_eq!(cb.state().await, CircuitState::Open);

    // Wait for timeout
    tokio::time::sleep(Duration::from_millis(80)).await;

    // Two successes should close the circuit
    let _ = cb.call(|| async { Ok::<_, String>(1) }).await;
    let _ = cb.call(|| async { Ok::<_, String>(2) }).await;

    assert_eq!(cb.state().await, CircuitState::Closed);
}

#[tokio::test]
async fn failure_in_half_open_reopens() {
    let config = CircuitBreakerConfig {
        failure_threshold: 2,
        success_threshold: 2,
        timeout: Duration::from_millis(50),
        call_timeout: Duration::from_secs(5),
    };
    let cb = CircuitBreaker::new(config);

    // Trip the breaker
    for _ in 0..2 {
        let _ = cb.call(|| async { Err::<i32, _>("fail") }).await;
    }

    // Wait for timeout, then fail again
    tokio::time::sleep(Duration::from_millis(80)).await;
    let _ = cb.call(|| async { Err::<i32, _>("still failing") }).await;

    assert_eq!(cb.state().await, CircuitState::Open);
}

#[tokio::test]
async fn manual_reset() {
    let cb = CircuitBreaker::new(fast_config());

    // Trip the breaker
    for _ in 0..3 {
        let _ = cb.call(|| async { Err::<i32, _>("fail") }).await;
    }
    assert_eq!(cb.state().await, CircuitState::Open);

    cb.reset().await;
    assert_eq!(cb.state().await, CircuitState::Closed);
    assert_eq!(cb.failure_count().await, 0);
}

#[tokio::test]
async fn timeout_counts_as_failure() {
    let config = CircuitBreakerConfig {
        failure_threshold: 1,
        success_threshold: 1,
        timeout: Duration::from_millis(50),
        call_timeout: Duration::from_millis(10),
    };
    let cb = CircuitBreaker::new(config);

    let result = cb
        .call(|| async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok::<_, String>(1)
        })
        .await;

    assert!(matches!(result, Err(CircuitBreakerError::Timeout(_))));
    assert_eq!(cb.state().await, CircuitState::Open);
}
