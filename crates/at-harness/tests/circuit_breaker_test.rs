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
        half_open_max_calls: 1,
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
        half_open_max_calls: 1,
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
        half_open_max_calls: 1,
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
        half_open_max_calls: 1,
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

// ---------------------------------------------------------------------------
// HalfOpen probe limiting
// ---------------------------------------------------------------------------

fn probe_config(max_probes: u32, success_threshold: u32) -> CircuitBreakerConfig {
    CircuitBreakerConfig {
        failure_threshold: 1,
        success_threshold,
        timeout: Duration::from_millis(30),
        call_timeout: Duration::from_secs(5),
        half_open_max_calls: max_probes,
    }
}

/// Trip the breaker and wait until the next call will move it to HalfOpen.
async fn trip_and_cool(cb: &CircuitBreaker) {
    let _ = cb.call(|| async { Err::<i32, _>("boom") }).await;
    assert_eq!(cb.state().await, CircuitState::Open);
    tokio::time::sleep(Duration::from_millis(50)).await;
}

type ProbeGate = tokio::sync::oneshot::Sender<Result<i32, String>>;
type ProbeTask = tokio::task::JoinHandle<Result<i32, CircuitBreakerError>>;

/// Start a probe that blocks until the returned sender fires its outcome.
fn gated_probe(cb: &CircuitBreaker) -> (ProbeGate, ProbeTask) {
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<i32, String>>();
    let cb = cb.clone();
    let task = tokio::spawn(async move {
        cb.call(|| async move { rx.await.unwrap_or_else(|_| Err("gate dropped".into())) })
            .await
    });
    (tx, task)
}

async fn wait_in_flight(cb: &CircuitBreaker, n: u32) {
    for _ in 0..200 {
        if cb.half_open_in_flight().await == n {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!(
        "expected {n} probes in flight, got {}",
        cb.half_open_in_flight().await
    );
}

#[test]
fn default_half_open_max_calls_is_one() {
    assert_eq!(CircuitBreakerConfig::default().half_open_max_calls, 1);
}

#[tokio::test]
async fn half_open_admits_single_probe_by_default() {
    let cb = CircuitBreaker::new(probe_config(1, 2));
    trip_and_cool(&cb).await;

    let (gate, probe) = gated_probe(&cb);
    wait_in_flight(&cb, 1).await;
    assert_eq!(cb.state().await, CircuitState::HalfOpen);

    // A second call while the probe is outstanding is refused, not executed.
    let executed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = executed.clone();
    let res = cb
        .call(|| async move {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok::<_, String>(0)
        })
        .await;
    assert!(matches!(res, Err(CircuitBreakerError::Open)));
    assert!(!executed.load(std::sync::atomic::Ordering::SeqCst));

    // Probe succeeds: 1 of 2 successes, still HalfOpen, slot freed.
    gate.send(Ok(7)).unwrap();
    assert_eq!(probe.await.unwrap().unwrap(), 7);
    assert_eq!(cb.state().await, CircuitState::HalfOpen);
    assert_eq!(cb.half_open_in_flight().await, 0);
    assert_eq!(cb.success_count().await, 1);

    // Next probe is admitted and closes the circuit.
    assert_eq!(cb.call(|| async { Ok::<_, String>(8) }).await.unwrap(), 8);
    assert_eq!(cb.state().await, CircuitState::Closed);
    assert_eq!(cb.half_open_in_flight().await, 0);
}

#[tokio::test]
async fn half_open_admits_configured_number_of_probes() {
    let cb = CircuitBreaker::new(probe_config(3, 3));
    trip_and_cool(&cb).await;

    let probes: Vec<_> = (0..3).map(|_| gated_probe(&cb)).collect();
    wait_in_flight(&cb, 3).await;

    let res = cb.call(|| async { Ok::<_, String>(0) }).await;
    assert!(matches!(res, Err(CircuitBreakerError::Open)));

    for (gate, _) in &probes {
        assert!(!gate.is_closed());
    }
    for (i, (gate, task)) in probes.into_iter().enumerate() {
        gate.send(Ok(i as i32)).unwrap();
        assert_eq!(task.await.unwrap().unwrap(), i as i32);
    }
    assert_eq!(cb.state().await, CircuitState::Closed);
}

#[tokio::test]
async fn zero_max_probes_treated_as_one() {
    let cb = CircuitBreaker::new(probe_config(0, 1));
    trip_and_cool(&cb).await;

    let (gate, probe) = gated_probe(&cb);
    wait_in_flight(&cb, 1).await;
    assert!(matches!(
        cb.call(|| async { Ok::<_, String>(0) }).await,
        Err(CircuitBreakerError::Open)
    ));
    gate.send(Ok(1)).unwrap();
    probe.await.unwrap().unwrap();
    assert_eq!(cb.state().await, CircuitState::Closed);
}

#[tokio::test]
async fn failed_probe_reopens_and_stale_probe_is_ignored() {
    let cb = CircuitBreaker::new(probe_config(2, 1));
    trip_and_cool(&cb).await;

    let (gate_a, probe_a) = gated_probe(&cb);
    let (gate_b, probe_b) = gated_probe(&cb);
    wait_in_flight(&cb, 2).await;

    gate_a.send(Err("still down".into())).unwrap();
    assert!(matches!(
        probe_a.await.unwrap(),
        Err(CircuitBreakerError::Inner(_))
    ));
    assert_eq!(cb.state().await, CircuitState::Open);
    assert_eq!(cb.half_open_in_flight().await, 0);

    // B was admitted in the window A just closed. Let the circuit cool and
    // open a *new* HalfOpen window with probe C before B completes.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let (gate_c, probe_c) = gated_probe(&cb);
    wait_in_flight(&cb, 1).await;
    assert_eq!(cb.state().await, CircuitState::HalfOpen);

    // B's success is stale: it must neither close the circuit (it would, as
    // success_threshold is 1) nor free C's probe slot.
    gate_b.send(Ok(1)).unwrap();
    assert_eq!(probe_b.await.unwrap().unwrap(), 1);
    assert_eq!(cb.state().await, CircuitState::HalfOpen);
    assert_eq!(cb.half_open_in_flight().await, 1);
    assert_eq!(cb.success_count().await, 0);

    // C is the current probe; its success decides.
    gate_c.send(Ok(2)).unwrap();
    assert_eq!(probe_c.await.unwrap().unwrap(), 2);
    assert_eq!(cb.state().await, CircuitState::Closed);
}

#[tokio::test]
async fn cancelled_probe_releases_its_slot() {
    let cb = CircuitBreaker::new(probe_config(1, 1));
    trip_and_cool(&cb).await;

    let (_gate, probe) = gated_probe(&cb);
    wait_in_flight(&cb, 1).await;
    probe.abort();
    assert!(probe.await.unwrap_err().is_cancelled());

    // Abandoned probe must not wedge the breaker in HalfOpen.
    assert_eq!(cb.half_open_in_flight().await, 0);
    assert_eq!(cb.state().await, CircuitState::HalfOpen);
    assert_eq!(cb.call(|| async { Ok::<_, String>(5) }).await.unwrap(), 5);
    assert_eq!(cb.state().await, CircuitState::Closed);
}

#[tokio::test]
async fn timed_out_probe_reopens() {
    let cb = CircuitBreaker::new(CircuitBreakerConfig {
        call_timeout: Duration::from_millis(20),
        ..probe_config(1, 1)
    });
    trip_and_cool(&cb).await;

    let res = cb
        .call(|| async {
            tokio::time::sleep(Duration::from_millis(200)).await;
            Ok::<_, String>(1)
        })
        .await;
    assert!(matches!(res, Err(CircuitBreakerError::Timeout(_))));
    assert_eq!(cb.state().await, CircuitState::Open);
    assert_eq!(cb.half_open_in_flight().await, 0);
}
