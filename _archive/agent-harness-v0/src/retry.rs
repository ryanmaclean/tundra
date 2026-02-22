use std::time::Duration;
use tracing::{debug, warn};

use crate::provider::ProviderError;

/// Configuration for retry behavior.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts.
    pub max_retries: u32,
    /// Initial backoff duration.
    pub initial_backoff: Duration,
    /// Maximum backoff duration.
    pub max_backoff: Duration,
    /// Backoff multiplier.
    pub multiplier: f64,
    /// Whether to add jitter to backoff.
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(30),
            multiplier: 2.0,
            jitter: true,
        }
    }
}

impl RetryConfig {
    /// Calculate backoff duration for a given attempt.
    fn backoff_for(&self, attempt: u32) -> Duration {
        let base = self.initial_backoff.as_millis() as f64 * self.multiplier.powi(attempt as i32);
        let capped = base.min(self.max_backoff.as_millis() as f64);

        let ms = if self.jitter {
            // Simple jitter: 50-100% of the backoff.
            let jitter_factor = 0.5 + (rand_simple() * 0.5);
            capped * jitter_factor
        } else {
            capped
        };

        Duration::from_millis(ms as u64)
    }
}

/// Execute an async operation with retry logic.
pub async fn with_retry<F, Fut, T>(
    config: &RetryConfig,
    operation_name: &str,
    mut f: F,
) -> Result<T, ProviderError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, ProviderError>>,
{
    let mut last_error = None;

    for attempt in 0..=config.max_retries {
        match f().await {
            Ok(result) => {
                if attempt > 0 {
                    debug!("{} succeeded on attempt {}", operation_name, attempt + 1);
                }
                return Ok(result);
            }
            Err(e) => {
                if !e.is_retryable() || attempt == config.max_retries {
                    return Err(e);
                }

                let backoff = if let ProviderError::RateLimited { retry_after_ms } = &e {
                    Duration::from_millis(*retry_after_ms)
                } else {
                    config.backoff_for(attempt)
                };

                warn!(
                    "{} attempt {} failed: {}. Retrying in {:?}",
                    operation_name,
                    attempt + 1,
                    e,
                    backoff
                );

                tokio::time::sleep(backoff).await;
                last_error = Some(e);
            }
        }
    }

    Err(last_error.unwrap())
}

/// Simple pseudo-random f64 in [0, 1) using timestamp.
fn rand_simple() -> f64 {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    (nanos % 10000) as f64 / 10000.0
}
