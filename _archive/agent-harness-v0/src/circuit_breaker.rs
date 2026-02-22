use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{warn, info, debug};

#[derive(Debug, Clone, PartialEq)]
pub enum CircuitState {
    Closed,    // Normal operation
    Open,      // Circuit is open, calls fail fast
    HalfOpen,  // Testing if the service has recovered
}

#[derive(Debug, thiserror::Error)]
pub enum CircuitBreakerError {
    #[error("Circuit breaker is open - calls are blocked")]
    Open,
    #[error("Circuit breaker timeout")]
    Timeout,
}

/// Circuit breaker pattern implementation for API resilience
pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitState>>,
    failure_count: AtomicU32,
    success_count: AtomicU32,
    last_failure_time: AtomicU64,
    config: CircuitBreakerConfig,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening the circuit
    pub failure_threshold: u32,
    /// Number of successes before closing the circuit (in half-open state)
    pub success_threshold: u32,
    /// How long to wait before transitioning from open to half-open
    pub timeout: Duration,
    /// How long to wait for a response before timing out
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

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            failure_count: AtomicU32::new(0),
            success_count: AtomicU32::new(0),
            last_failure_time: AtomicU64::new(0),
            config,
        }
    }

    pub fn new_default() -> Self {
        Self::new(CircuitBreakerConfig::default())
    }

    /// Execute a function with circuit breaker protection
    pub async fn call<F, T, E>(&self, f: F) -> Result<T, CircuitBreakerError>
    where
        F: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Debug,
    {
        // Check circuit state
        let state = {
            let state_guard = self.state.read().await;
            state_guard.clone()
        };

        match state {
            CircuitState::Open => {
                // Check if we should transition to half-open
                if self.should_attempt_reset() {
                    self.transition_to_half_open().await;
                } else {
                    return Err(CircuitBreakerError::Open);
                }
            }
            CircuitState::Closed | CircuitState::HalfOpen => {
                // Proceed with the call
            }
        }

        // Execute the call with timeout
        let result = tokio::time::timeout(self.config.call_timeout, f).await;

        match result {
            Ok(Ok(success)) => {
                self.on_success().await;
                Ok(success)
            }
            Ok(Err(error)) => {
                warn!("Call failed: {:?}", error);
                self.on_failure().await;
                Err(CircuitBreakerError::Open)
            }
            Err(_) => {
                warn!("Call timed out after {:?}", self.config.call_timeout);
                self.on_failure().await;
                Err(CircuitBreakerError::Timeout)
            }
        }
    }

    async fn on_success(&self) {
        // Reset failure count on success
        self.failure_count.store(0, Ordering::Relaxed);
        
        let mut state = self.state.write().await;
        match *state {
            CircuitState::HalfOpen => {
                let success_count = self.success_count.fetch_add(1, Ordering::Relaxed) + 1;
                debug!("Success in half-open state: {}/{}", success_count, self.config.success_threshold);
                
                if success_count >= self.config.success_threshold {
                    info!("Circuit breaker closing after {} successes", success_count);
                    *state = CircuitState::Closed;
                    self.success_count.store(0, Ordering::Relaxed);
                }
            }
            CircuitState::Closed => {
                // Already in good state, nothing to do
            }
            CircuitState::Open => {
                // This shouldn't happen, but handle it gracefully
                warn!("Unexpected success in open state");
                *state = CircuitState::Closed;
            }
        }
    }

    async fn on_failure(&self) {
        let failure_count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.last_failure_time.store(now, Ordering::Relaxed);

        debug!("Failure count: {}/{}", failure_count, self.config.failure_threshold);

        let mut state = self.state.write().await;
        match *state {
            CircuitState::Closed => {
                if failure_count >= self.config.failure_threshold {
                    warn!("Circuit breaker opening after {} failures", failure_count);
                    *state = CircuitState::Open;
                    self.success_count.store(0, Ordering::Relaxed);
                }
            }
            CircuitState::HalfOpen => {
                warn!("Circuit breaker opening again due to failure in half-open state");
                *state = CircuitState::Open;
                self.success_count.store(0, Ordering::Relaxed);
            }
            CircuitState::Open => {
                // Already open, nothing to do
            }
        }
    }

    async fn transition_to_half_open(&self) {
        let mut state = self.state.write().await;
        if *state == CircuitState::Open {
            info!("Circuit breaker transitioning to half-open state");
            *state = CircuitState::HalfOpen;
            self.success_count.store(0, Ordering::Relaxed);
        }
    }

    fn should_attempt_reset(&self) -> bool {
        let last_failure = self.last_failure_time.load(Ordering::Relaxed);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        now - last_failure > self.config.timeout.as_secs()
    }

    /// Get current circuit state
    pub async fn state(&self) -> CircuitState {
        self.state.read().await.clone()
    }

    /// Get current metrics
    pub async fn metrics(&self) -> CircuitBreakerMetrics {
        CircuitBreakerMetrics {
            state: self.state.read().await.clone(),
            failure_count: self.failure_count.load(Ordering::Relaxed),
            success_count: self.success_count.load(Ordering::Relaxed),
            last_failure_time: self.last_failure_time.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerMetrics {
    pub state: CircuitState,
    pub failure_count: u32,
    pub success_count: u32,
    pub last_failure_time: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_circuit_breaker_basic_flow() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout: Duration::from_millis(50),
            call_timeout: Duration::from_millis(50),
        };

        let circuit = Arc::new(CircuitBreaker::new(config));

        // Initially closed
        assert_eq!(circuit.state().await, CircuitState::Closed);

        // Test basic success
        let result = circuit.call::<_, String, &str>(async {
            Ok("success".to_string())
        }).await;
        assert!(result.is_ok());

        // Test basic failure
        let result = circuit.call::<_, &str, &str>(async {
            Err("failure")
        }).await;
        assert!(result.is_err());

        // Cause failures to open the circuit
        for _ in 0..3 {
            let result: Result<&str, CircuitBreakerError> = circuit.call::<_, &str, &str>(async {
                Err("simulated failure")
            }).await;
            assert!(result.is_err());
        }

        // Circuit should be open
        assert_eq!(circuit.state().await, CircuitState::Open);

        // Calls should be blocked
        let result: Result<&str, CircuitBreakerError> = circuit.call::<_, &str, &str>(async {
            Ok("should not be called")
        }).await;
        assert!(matches!(result, Err(CircuitBreakerError::Open)));

        // Test metrics
        let metrics = circuit.metrics().await;
        assert_eq!(metrics.state, CircuitState::Open);
        assert!(metrics.failure_count >= 3);
    }

    #[tokio::test]
    async fn test_circuit_breaker_timeout() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 1,
            timeout: Duration::from_millis(100),
            call_timeout: Duration::from_millis(10),
        };

        let circuit = Arc::new(CircuitBreaker::new(config));

        // Test timeout
        let result = circuit.call::<_, &str, &str>(async {
            sleep(Duration::from_millis(20)).await;
            Ok("should timeout")
        }).await;

        assert!(matches!(result, Err(CircuitBreakerError::Timeout)));
    }
}
