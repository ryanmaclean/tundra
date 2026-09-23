use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
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
    ///
    /// Also returned in **HalfOpen** when
    /// [`CircuitBreakerConfig::half_open_max_calls`] probes are already in
    /// flight: excess calls are refused until a probe completes.
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
    /// Maximum number of probe calls admitted concurrently while
    /// **HalfOpen**. Further calls are rejected with
    /// [`CircuitBreakerError::Open`] until an in-flight probe completes; each
    /// probe outcome then decides the state (any failure reopens, and
    /// `success_threshold` successes close). A value of `0` is treated as `1`.
    pub half_open_max_calls: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 2,
            timeout: Duration::from_secs(60),
            call_timeout: Duration::from_secs(30),
            half_open_max_calls: 1,
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
    /// Probes admitted in the current HalfOpen window and not yet completed.
    half_open_in_flight: u32,
    /// Bumped on every state transition so a probe's completion only counts
    /// against the HalfOpen window that admitted it.
    generation: u64,
}

impl InnerState {
    fn transition(&mut self, to: CircuitState) {
        self.state = to;
        self.success_count = 0;
        self.half_open_in_flight = 0;
        self.generation = self.generation.wrapping_add(1);
    }
}

/// A HalfOpen probe slot. Released when the probe completes or, if the
/// calling future is cancelled mid-probe, on drop -- so an abandoned probe
/// can never wedge the breaker in HalfOpen.
struct ProbePermit<'a> {
    inner: &'a Mutex<InnerState>,
    generation: u64,
    armed: bool,
}

impl ProbePermit<'_> {
    /// Release the slot under an already-held lock. Returns whether this
    /// probe still belongs to the current HalfOpen window (i.e. whether its
    /// outcome should decide the state).
    fn complete(&mut self, guard: &mut InnerState) -> bool {
        self.armed = false;
        let current = guard.state == CircuitState::HalfOpen && guard.generation == self.generation;
        if current {
            guard.half_open_in_flight = guard.half_open_in_flight.saturating_sub(1);
        }
        current
    }
}

impl Drop for ProbePermit<'_> {
    fn drop(&mut self) {
        if self.armed {
            let mut guard = lock(self.inner);
            self.complete(&mut guard);
        }
    }
}

fn lock(inner: &Mutex<InnerState>) -> MutexGuard<'_, InnerState> {
    inner.lock().unwrap_or_else(|e| e.into_inner())
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
                half_open_in_flight: 0,
                generation: 0,
            })),
        }
    }

    /// Returns the current state of the circuit breaker.
    pub async fn state(&self) -> CircuitState {
        lock(&self.inner).state
    }

    /// Returns the current failure count.
    pub async fn failure_count(&self) -> u32 {
        lock(&self.inner).failure_count
    }

    /// Returns the current success count (relevant in half-open).
    pub async fn success_count(&self) -> u32 {
        lock(&self.inner).success_count
    }

    /// Returns the number of HalfOpen probes currently in flight (always 0
    /// outside HalfOpen).
    pub async fn half_open_in_flight(&self) -> u32 {
        lock(&self.inner).half_open_in_flight
    }

    /// Execute `f` through the circuit breaker.
    ///
    /// If the circuit is **Open** and the timeout has not elapsed the call is
    /// rejected immediately.  If the timeout *has* elapsed the circuit moves
    /// to **HalfOpen** and the call is admitted as a probe.  In **HalfOpen**
    /// at most [`CircuitBreakerConfig::half_open_max_calls`] probes run at
    /// once; further calls are rejected with [`CircuitBreakerError::Open`].
    pub async fn call<F, Fut, T, E>(&self, f: F) -> Result<T, CircuitBreakerError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        // --- pre-flight check (admission) ---
        let mut permit = self.admit()?;

        // --- execute with timeout ---
        let result = tokio::time::timeout(self.config.call_timeout, f()).await;

        let mut guard = lock(&self.inner);
        let is_current_probe = permit.as_mut().is_some_and(|p| p.complete(&mut guard));
        match result {
            Ok(Ok(value)) => {
                self.record_success(&mut guard, is_current_probe);
                Ok(value)
            }
            Ok(Err(e)) => {
                self.record_failure(&mut guard, is_current_probe);
                Err(CircuitBreakerError::Inner(e.to_string()))
            }
            Err(_elapsed) => {
                self.record_failure(&mut guard, is_current_probe);
                Err(CircuitBreakerError::Timeout(self.config.call_timeout))
            }
        }
    }

    // ----- helpers -----

    /// Decide whether a call may run. `Ok(Some(_))` admits a HalfOpen probe,
    /// `Ok(None)` admits a normal Closed-state call.
    fn admit(&self) -> Result<Option<ProbePermit<'_>>, CircuitBreakerError> {
        let mut guard = lock(&self.inner);
        if guard.state == CircuitState::Open {
            match guard.last_failure_time {
                Some(last) if last.elapsed() >= self.config.timeout => {
                    info!("circuit breaker transitioning Open -> HalfOpen");
                    guard.transition(CircuitState::HalfOpen);
                }
                _ => return Err(CircuitBreakerError::Open),
            }
        }
        match guard.state {
            CircuitState::Closed => Ok(None),
            CircuitState::HalfOpen => {
                if guard.half_open_in_flight >= self.config.half_open_max_calls.max(1) {
                    return Err(CircuitBreakerError::Open);
                }
                guard.half_open_in_flight += 1;
                Ok(Some(ProbePermit {
                    inner: &self.inner,
                    generation: guard.generation,
                    armed: true,
                }))
            }
            CircuitState::Open => Err(CircuitBreakerError::Open),
        }
    }

    fn record_success(&self, guard: &mut InnerState, is_current_probe: bool) {
        match guard.state {
            CircuitState::HalfOpen if is_current_probe => {
                guard.success_count += 1;
                if guard.success_count >= self.config.success_threshold {
                    info!("circuit breaker transitioning HalfOpen -> Closed");
                    guard.transition(CircuitState::Closed);
                    guard.failure_count = 0;
                }
            }
            CircuitState::Closed => {
                // Reset failure streak on success.
                guard.failure_count = 0;
            }
            // A call admitted before the current HalfOpen window (or while
            // Open) carries no information about the probe; ignore it.
            CircuitState::HalfOpen | CircuitState::Open => {}
        }
    }

    fn record_failure(&self, guard: &mut InnerState, is_current_probe: bool) {
        match guard.state {
            CircuitState::Closed => {
                guard.failure_count += 1;
                guard.last_failure_time = Some(Instant::now());
                if guard.failure_count >= self.config.failure_threshold {
                    warn!(
                        failures = guard.failure_count,
                        "circuit breaker transitioning Closed -> Open"
                    );
                    guard.transition(CircuitState::Open);
                }
            }
            CircuitState::HalfOpen if is_current_probe => {
                guard.failure_count += 1;
                guard.last_failure_time = Some(Instant::now());
                warn!("circuit breaker transitioning HalfOpen -> Open (failure during probe)");
                guard.transition(CircuitState::Open);
            }
            // Stale result from a call admitted before this HalfOpen window:
            // don't let it reopen the circuit mid-probe.
            CircuitState::HalfOpen => {}
            CircuitState::Open => {
                // Late failure while already open: extend the open window.
                guard.failure_count += 1;
                guard.last_failure_time = Some(Instant::now());
            }
        }
    }

    /// Manually reset the circuit breaker to the **Closed** state.
    pub async fn reset(&self) {
        let mut guard = lock(&self.inner);
        guard.transition(CircuitState::Closed);
        guard.failure_count = 0;
        guard.last_failure_time = None;
    }
}
