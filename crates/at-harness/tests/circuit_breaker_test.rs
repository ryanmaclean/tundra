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

/// Finding #9: HalfOpen must admit at most `half_open_max_calls` concurrent
/// probes; extra callers are rejected with `Open` instead of hammering a
/// still-failing downstream.
#[tokio::test]
async fn half_open_limits_concurrent_probes() {
    let cb = CircuitBreaker::new(fast_config());
    assert_eq!(cb.half_open_max_calls(), 1);

    for _ in 0..3 {
        let _ = cb.call(|| async { Err::<i32, _>("fail") }).await;
    }
    assert_eq!(cb.state().await, CircuitState::Open);
    tokio::time::sleep(Duration::from_millis(150)).await;

    // First probe: held open until we release it.
    let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
    let probe_cb = cb.clone();
    let probe = tokio::spawn(async move {
        probe_cb
            .call(|| async move {
                let _ = release_rx.await;
                Ok::<_, String>(1)
            })
            .await
    });

    // Wait until the probe has been admitted.
    for _ in 0..100 {
        if cb.half_open_in_flight() == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(cb.state().await, CircuitState::HalfOpen);
    assert_eq!(cb.half_open_in_flight(), 1);

    // Concurrent callers while the probe is in flight are rejected.
    for _ in 0..5 {
        let res = cb.call(|| async { Ok::<_, String>(2) }).await;
        assert!(matches!(res, Err(CircuitBreakerError::Open)));
    }

    release_tx.send(()).unwrap();
    assert_eq!(probe.await.unwrap().unwrap(), 1);
    assert_eq!(cb.half_open_in_flight(), 0);

    // Slot freed: next sequential probe is admitted and closes the circuit.
    assert_eq!(cb.call(|| async { Ok::<_, String>(3) }).await.unwrap(), 3);
    assert_eq!(cb.state().await, CircuitState::Closed);
}

/// A cancelled (dropped) probe future must release its half-open slot.
#[tokio::test]
async fn half_open_slot_released_on_cancel() {
    let cb = CircuitBreaker::new(fast_config()).with_half_open_max_calls(2);
    assert_eq!(cb.half_open_max_calls(), 2);

    for _ in 0..3 {
        let _ = cb.call(|| async { Err::<i32, _>("fail") }).await;
    }
    tokio::time::sleep(Duration::from_millis(150)).await;

    let hang_cb = cb.clone();
    let hung = tokio::spawn(async move {
        hang_cb
            .call(std::future::pending::<Result<i32, String>>)
            .await
    });
    for _ in 0..100 {
        if cb.half_open_in_flight() == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(cb.half_open_in_flight(), 1);

    hung.abort();
    let _ = hung.await;
    assert_eq!(cb.half_open_in_flight(), 0);
    assert_eq!(cb.state().await, CircuitState::HalfOpen);
}
