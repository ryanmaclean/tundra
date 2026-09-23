use std::future::Future;
use std::sync::atomic::{AtomicU32, Ordering};
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

/// Default number of concurrent probe calls admitted while **HalfOpen**.
pub const DEFAULT_HALF_OPEN_MAX_CALLS: u32 = 1;

/// RAII slot for one in-flight half-open probe. Releases the slot on drop so
/// that cancelled (dropped) futures and panics cannot leak capacity.
struct ProbeSlot(Arc<AtomicU32>);

impl Drop for ProbeSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

// ---------------------------------------------------------------------------
// CircuitBreaker
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    inner: Arc<Mutex<InnerState>>,
    /// Probe calls currently executing that were admitted while HalfOpen.
    /// Incremented only while holding `inner`; decremented by [`ProbeSlot`].
    half_open_in_flight: Arc<AtomicU32>,
    /// Maximum concurrent probe calls admitted while HalfOpen (>= 1).
    half_open_max_calls: u32,
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
            half_open_in_flight: Arc::new(AtomicU32::new(0)),
            half_open_max_calls: DEFAULT_HALF_OPEN_MAX_CALLS,
        }
    }

    /// Set how many probe calls may run concurrently while **HalfOpen**
    /// (default [`DEFAULT_HALF_OPEN_MAX_CALLS`]). Extra callers are rejected
    /// with [`CircuitBreakerError::Open`]. Values below 1 are clamped to 1.
    pub fn with_half_open_max_calls(mut self, max: u32) -> Self {
        self.half_open_max_calls = max.max(1);
        self
    }

    /// Maximum concurrent probe calls admitted while **HalfOpen**.
    pub fn half_open_max_calls(&self) -> u32 {
        self.half_open_max_calls
    }

    /// Number of half-open probe calls currently in flight.
    pub fn half_open_in_flight(&self) -> u32 {
        self.half_open_in_flight.load(Ordering::Acquire)
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
    /// to **HalfOpen**.  While **HalfOpen**, at most `half_open_max_calls`
    /// probe calls run concurrently; further callers get
    /// [`CircuitBreakerError::Open`] until a probe finishes.
    pub async fn call<F, Fut, T, E>(&self, f: F) -> Result<T, CircuitBreakerError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        // --- pre-flight check ---
        // Held until the end of this function (after the outcome is recorded)
        // so a new probe cannot slip in before this probe's result lands.
        let _probe_slot = {
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
                CircuitState::Closed | CircuitState::HalfOpen => {}
            }
            if guard.state == CircuitState::HalfOpen {
                // Increments only happen under `inner`, so check-then-add
                // cannot over-admit.
                if self.half_open_in_flight.load(Ordering::Acquire) >= self.half_open_max_calls {
                    return Err(CircuitBreakerError::Open);
                }
                self.half_open_in_flight.fetch_add(1, Ordering::AcqRel);
                Some(ProbeSlot(Arc::clone(&self.half_open_in_flight)))
            } else {
                None
            }
        };

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
