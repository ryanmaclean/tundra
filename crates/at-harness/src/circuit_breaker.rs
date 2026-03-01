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
}
