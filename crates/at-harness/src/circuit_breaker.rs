use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

/// Errors that can occur when executing a call through a circuit breaker.
///
/// Circuit breakers protect downstream services from cascading failures by
/// temporarily blocking calls when error rates exceed thresholds. This enum
/// represents the various failure modes that can occur during protected execution.
///
/// # Examples
///
/// ```rust
/// use at_harness::circuit_breaker::{CircuitBreaker, CircuitBreakerError, CircuitBreakerConfig};
///
/// async fn handle_circuit_breaker() {
///     let breaker = CircuitBreaker::new(CircuitBreakerConfig::default());
///
///     match breaker.call(|| async { Ok::<_, String>("result") }).await {
///         Err(CircuitBreakerError::Open) => {
///             println!("Circuit is open, service unavailable");
///         }
///         Err(CircuitBreakerError::Timeout(duration)) => {
///             println!("Call timed out after {:?}", duration);
///         }
///         Err(CircuitBreakerError::Inner(msg)) => {
///             println!("Inner operation failed: {}", msg);
///         }
///         Ok(_) => {}
///     }
/// }
/// ```
#[derive(Debug, thiserror::Error)]
pub enum CircuitBreakerError {
    /// The circuit breaker is open and refusing calls.
    ///
    /// This occurs when the failure threshold has been exceeded and the circuit
    /// has transitioned to the **Open** state. Calls are rejected immediately
    /// without being executed to protect the downstream service.
    ///
    /// The circuit will automatically transition to **HalfOpen** after the
    /// configured timeout period, at which point limited calls will be allowed
    /// through to test if the service has recovered.
    #[error("circuit is open – refusing call")]
    Open,

    /// The call exceeded the configured timeout duration.
    ///
    /// The wrapped operation did not complete within the `call_timeout` period
    /// specified in [`CircuitBreakerConfig`]. This counts as a failure and
    /// increments the circuit breaker's failure counter.
    ///
    /// The contained [`Duration`] indicates how long the circuit breaker waited
    /// before timing out the call.
    #[error("call timed out after {0:?}")]
    Timeout(Duration),

    /// The inner operation returned an error.
    ///
    /// The call was allowed through the circuit breaker but the wrapped
    /// operation itself failed. This counts as a failure and increments
    /// the circuit breaker's failure counter.
    ///
    /// The contained string provides the error message from the underlying
    /// operation.
    #[error("inner error: {0}")]
    Inner(String),
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation – all calls pass through.
    Closed,
    /// Too many failures – calls are rejected immediately.
    Open,
    /// Testing recovery – limited calls are allowed through.
    HalfOpen,
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before the circuit opens.
    pub failure_threshold: u32,
    /// Number of consecutive successes in half-open before closing.
    pub success_threshold: u32,
    /// How long the circuit stays open before transitioning to half-open.
    pub timeout: Duration,
    /// Maximum duration for an individual call.
    pub call_timeout: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 2,
            timeout: Duration::from_secs(60),
            call_timeout: Duration::from_secs(30),
        }
    }
}

// ---------------------------------------------------------------------------
// Inner state (behind Mutex)
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct InnerState {
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
    last_failure_time: Option<Instant>,
}

// ---------------------------------------------------------------------------
// CircuitBreaker
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    inner: Arc<Mutex<InnerState>>,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            inner: Arc::new(Mutex::new(InnerState {
                state: CircuitState::Closed,
                failure_count: 0,
                success_count: 0,
                last_failure_time: None,
            })),
        }
    }

    /// Returns the current state of the circuit breaker.
    pub async fn state(&self) -> CircuitState {
        let guard = self.inner.lock().await;
        guard.state
    }

    /// Returns the current failure count.
    pub async fn failure_count(&self) -> u32 {
        let guard = self.inner.lock().await;
        guard.failure_count
    }

    /// Returns the current success count (relevant in half-open).
    pub async fn success_count(&self) -> u32 {
        let guard = self.inner.lock().await;
        guard.success_count
    }

    /// Execute `f` through the circuit breaker.
    ///
    /// If the circuit is **Open** and the timeout has not elapsed the call is
    /// rejected immediately.  If the timeout *has* elapsed the circuit moves
    /// to **HalfOpen** and the call is allowed through.
    pub async fn call<F, Fut, T, E>(&self, f: F) -> Result<T, CircuitBreakerError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        // --- pre-flight check ---
        {
            let mut guard = self.inner.lock().await;
            match guard.state {
                CircuitState::Open => {
                    // Check whether the timeout has elapsed.
                    if let Some(last) = guard.last_failure_time {
                        if last.elapsed() >= self.config.timeout {
                            info!("circuit breaker transitioning Open -> HalfOpen");
                            guard.state = CircuitState::HalfOpen;
                            guard.success_count = 0;
                        } else {
                            return Err(CircuitBreakerError::Open);
                        }
                    } else {
                        return Err(CircuitBreakerError::Open);
                    }
                }
                CircuitState::Closed | CircuitState::HalfOpen => { /* allow */ }
            }
        }

        // --- execute with timeout ---
        let result = tokio::time::timeout(self.config.call_timeout, f()).await;

        match result {
            Ok(Ok(value)) => {
                self.record_success().await;
                Ok(value)
            }
            Ok(Err(e)) => {
                self.record_failure().await;
                Err(CircuitBreakerError::Inner(e.to_string()))
            }
            Err(_elapsed) => {
                self.record_failure().await;
                Err(CircuitBreakerError::Timeout(self.config.call_timeout))
            }
        }
    }

    // ----- helpers -----

    async fn record_success(&self) {
        let mut guard = self.inner.lock().await;
        match guard.state {
            CircuitState::HalfOpen => {
                guard.success_count += 1;
                if guard.success_count >= self.config.success_threshold {
                    info!("circuit breaker transitioning HalfOpen -> Closed");
                    guard.state = CircuitState::Closed;
                    guard.failure_count = 0;
                    guard.success_count = 0;
                }
            }
            CircuitState::Closed => {
                // Reset failure streak on success.
                guard.failure_count = 0;
            }
            CircuitState::Open => { /* shouldn't happen */ }
        }
    }

    async fn record_failure(&self) {
        let mut guard = self.inner.lock().await;
        guard.failure_count += 1;
        guard.last_failure_time = Some(Instant::now());

        match guard.state {
            CircuitState::Closed => {
                if guard.failure_count >= self.config.failure_threshold {
                    warn!(
                        failures = guard.failure_count,
                        "circuit breaker transitioning Closed -> Open"
                    );
                    guard.state = CircuitState::Open;
                }
            }
            CircuitState::HalfOpen => {
                warn!("circuit breaker transitioning HalfOpen -> Open (failure during probe)");
                guard.state = CircuitState::Open;
                guard.success_count = 0;
            }
            CircuitState::Open => { /* already open */ }
        }
    }

    /// Manually reset the circuit breaker to the **Closed** state.
    pub async fn reset(&self) {
        let mut guard = self.inner.lock().await;
        guard.state = CircuitState::Closed;
        guard.failure_count = 0;
        guard.success_count = 0;
        guard.last_failure_time = None;
    }

    /// Test seam: force the circuit into Open state with a backdated
    /// `last_failure_time` so tests can control whether the timeout has
    /// elapsed without real sleeps.
    ///
    /// `elapsed` is how much time should appear to have passed since the
    /// circuit opened.  Use a value >= `config.timeout` to make the breaker
    /// treat the timeout as having expired.
    #[cfg(test)]
    pub(crate) async fn force_open_elapsed(&self, elapsed: Duration) {
        let mut guard = self.inner.lock().await;
        guard.state = CircuitState::Open;
        guard.last_failure_time = Instant::now().checked_sub(elapsed);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn config_with(
        failure_threshold: u32,
        success_threshold: u32,
        timeout_secs: u64,
    ) -> CircuitBreakerConfig {
        CircuitBreakerConfig {
            failure_threshold,
            success_threshold,
            timeout: Duration::from_secs(timeout_secs),
            call_timeout: Duration::from_secs(5),
        }
    }

    /// Helper closure that records how many times it was invoked.
    macro_rules! ok_closure {
        ($counter:expr, $val:expr) => {{
            let c = Arc::clone(&$counter);
            move || {
                let c2 = Arc::clone(&c);
                async move {
                    c2.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, String>($val)
                }
            }
        }};
    }

    macro_rules! err_closure {
        ($counter:expr) => {{
            let c = Arc::clone(&$counter);
            move || {
                let c2 = Arc::clone(&c);
                async move {
                    c2.fetch_add(1, Ordering::SeqCst);
                    Err::<i32, _>("injected failure".to_string())
                }
            }
        }};
    }

    // -----------------------------------------------------------------------
    // Test 1: Closed breaker passes calls through
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn closed_breaker_passes_calls_through() {
        let breaker = CircuitBreaker::new(config_with(3, 2, 60));
        let invocations = Arc::new(AtomicUsize::new(0));

        let result = breaker.call(ok_closure!(invocations, 42)).await;

        assert_eq!(result.unwrap(), 42, "expected the closure's return value");
        assert_eq!(
            invocations.load(Ordering::SeqCst),
            1,
            "closure must be invoked exactly once"
        );
        assert_eq!(
            breaker.state().await,
            CircuitState::Closed,
            "state must stay Closed after a success"
        );
    }

    // -----------------------------------------------------------------------
    // Test 2: N failures trip the breaker; (N+1)th call is short-circuited
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn failure_threshold_trips_breaker_to_open() {
        let threshold = 3u32;
        let breaker = CircuitBreaker::new(config_with(threshold, 2, 60));
        let invocations = Arc::new(AtomicUsize::new(0));

        // Drive exactly `threshold` failures.
        for _ in 0..threshold {
            let c = Arc::clone(&invocations);
            let _ = breaker.call(err_closure!(c)).await;
        }

        assert_eq!(
            breaker.state().await,
            CircuitState::Open,
            "breaker must be Open after failure_threshold failures"
        );
        assert_eq!(
            invocations.load(Ordering::SeqCst),
            threshold as usize,
            "closure must have been invoked for each failure"
        );

        // (N+1)th call: breaker is open, closure must NOT be invoked.
        let extra_invocations = Arc::new(AtomicUsize::new(0));
        let extra = Arc::clone(&extra_invocations);
        let result = breaker
            .call(move || {
                let c = Arc::clone(&extra);
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok::<i32, String>(0)
                }
            })
            .await;

        assert!(
            matches!(result, Err(CircuitBreakerError::Open)),
            "expected CircuitBreakerError::Open, got {result:?}"
        );
        assert_eq!(
            extra_invocations.load(Ordering::SeqCst),
            0,
            "closure must NOT be invoked when circuit is Open"
        );
    }

    // -----------------------------------------------------------------------
    // Test 3: Open breaker rejects calls until timeout; transitions to HalfOpen
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn open_breaker_rejects_calls_until_timeout() {
        let timeout_secs = 10u64;
        let breaker = CircuitBreaker::new(config_with(1, 2, timeout_secs));

        // Trip the breaker.
        let _ = breaker
            .call(|| async { Err::<i32, _>("boom".to_string()) })
            .await;
        assert_eq!(breaker.state().await, CircuitState::Open);

        // Immediate call within the timeout window — closure must NOT run.
        let invocations = Arc::new(AtomicUsize::new(0));
        let result = breaker.call(ok_closure!(invocations, 1)).await;
        assert!(
            matches!(result, Err(CircuitBreakerError::Open)),
            "expected Open error while within timeout, got {result:?}"
        );
        assert_eq!(
            invocations.load(Ordering::SeqCst),
            0,
            "closure must not run while circuit is Open and timeout has not elapsed"
        );

        // Simulate timeout elapsed by backdating last_failure_time.
        breaker
            .force_open_elapsed(Duration::from_secs(timeout_secs + 1))
            .await;

        // Next call should transition to HalfOpen and invoke the closure.
        let invocations2 = Arc::new(AtomicUsize::new(0));
        let result2 = breaker.call(ok_closure!(invocations2, 99)).await;
        assert!(
            result2.is_ok(),
            "expected Ok after Open→HalfOpen transition, got {result2:?}"
        );
        assert_eq!(
            invocations2.load(Ordering::SeqCst),
            1,
            "closure must run after timeout elapses and circuit moves to HalfOpen"
        );
        // The state should now be HalfOpen (one success out of success_threshold=2).
        assert_eq!(
            breaker.state().await,
            CircuitState::HalfOpen,
            "state should be HalfOpen after first successful probe (success_threshold=2)"
        );
    }

    // -----------------------------------------------------------------------
    // Test 4: HalfOpen with enough successes recovers to Closed
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn half_open_success_recovers_to_closed() {
        let success_threshold = 2u32;
        let timeout_secs = 10u64;
        let breaker = CircuitBreaker::new(config_with(1, success_threshold, timeout_secs));

        // Trip the breaker, then simulate timeout elapsed.
        let _ = breaker
            .call(|| async { Err::<i32, _>("boom".to_string()) })
            .await;
        breaker
            .force_open_elapsed(Duration::from_secs(timeout_secs + 1))
            .await;

        // Send `success_threshold` successful calls; the last one should close the circuit.
        let invocations = Arc::new(AtomicUsize::new(0));
        for i in 0..success_threshold {
            let c = Arc::clone(&invocations);
            let result = breaker.call(ok_closure!(c, i as i32)).await;
            assert!(result.is_ok(), "probe call {i} should succeed");
        }

        assert_eq!(
            invocations.load(Ordering::SeqCst),
            success_threshold as usize,
            "all probe closures must have been invoked"
        );
        assert_eq!(
            breaker.state().await,
            CircuitState::Closed,
            "breaker must be Closed after success_threshold successful probes"
        );

        // A subsequent failure in Closed state should NOT immediately reopen
        // (needs to reach failure_threshold=1 — but note threshold is 1 here,
        // so one failure *does* trip it; use threshold=3 for that sub-assertion).
        // Verify failure_count was reset to 0 on Closed transition.
        assert_eq!(
            breaker.failure_count().await,
            0,
            "failure_count must be reset to 0 when circuit closes"
        );
    }

    // -----------------------------------------------------------------------
    // Test 5: HalfOpen failure re-opens the breaker; subsequent call rejected
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn half_open_failure_reopens_breaker() {
        let timeout_secs = 10u64;
        let breaker = CircuitBreaker::new(config_with(1, 3, timeout_secs));

        // Trip the breaker, simulate timeout, enter HalfOpen via a force.
        let _ = breaker
            .call(|| async { Err::<i32, _>("boom".to_string()) })
            .await;
        breaker
            .force_open_elapsed(Duration::from_secs(timeout_secs + 1))
            .await;

        // One failing probe call while in HalfOpen.
        let probe_invocations = Arc::new(AtomicUsize::new(0));
        let probe = Arc::clone(&probe_invocations);
        let result = breaker
            .call(move || {
                let c = Arc::clone(&probe);
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err::<i32, _>("probe failure".to_string())
                }
            })
            .await;

        assert!(
            matches!(result, Err(CircuitBreakerError::Inner(_))),
            "expected Inner error from failing probe, got {result:?}"
        );
        assert_eq!(
            probe_invocations.load(Ordering::SeqCst),
            1,
            "probe closure must have been invoked"
        );
        assert_eq!(
            breaker.state().await,
            CircuitState::Open,
            "breaker must return to Open after a probe failure in HalfOpen"
        );

        // The timeout window has reset: an immediate call must be rejected
        // without invoking the closure (new last_failure_time is fresh).
        let follow_up_invocations = Arc::new(AtomicUsize::new(0));
        let result2 = breaker.call(ok_closure!(follow_up_invocations, 0)).await;
        assert!(
            matches!(result2, Err(CircuitBreakerError::Open)),
            "follow-up call must be rejected while circuit is Open, got {result2:?}"
        );
        assert_eq!(
            follow_up_invocations.load(Ordering::SeqCst),
            0,
            "follow-up closure must NOT be invoked when circuit is Open"
        );
    }
}
