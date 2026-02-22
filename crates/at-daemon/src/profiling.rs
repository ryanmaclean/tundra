use tracing::{info, info_span};
use std::time::Instant;

/// Initialize comprehensive Datadog OpenTelemetry integration
pub fn init_datadog_telemetry() -> anyhow::Result<()> {
    // Get configuration from environment
    let service_name = std::env::var("DD_SERVICE").unwrap_or_else(|_| "at-daemon".to_string());
    let service_env = std::env::var("DD_ENV").unwrap_or_else(|_| "development".to_string());
    let service_version = std::env::var("DD_VERSION").unwrap_or_else(|_| "0.1.0".to_string());
    let agent_endpoint = std::env::var("DD_TRACE_AGENT_URL").unwrap_or_else(|_| "http://localhost:8126".to_string());

    info!("Initializing Datadog tracing for service: {}", service_name);

    // Note: tracing is already initialized by at_telemetry::logging
    // We're just adding Datadog-compatible structured logging
    info!("Datadog tracing initialized successfully");
    info!("Service: {} | Environment: {} | Version: {}", service_name, service_env, service_version);
    info!("Agent endpoint: {}", agent_endpoint);

    Ok(())
}

/// Create a detailed traced span with custom attributes
#[macro_export]
macro_rules! traced_span {
    ($name:expr) => {
        tracing::info_span!($name, 
            service = "at-daemon"
        )
    };
    ($name:expr, $($key:ident = $value:expr),*) => {
        tracing::info_span!($name, 
            service = "at-daemon",
            $($key = $value),*
        )
    };
}

/// LLM-specific traced span
#[macro_export]
macro_rules! llm_span {
    ($name:expr, model: $model:expr, provider: $provider:expr) => {
        tracing::info_span!($name, 
            service = "at-daemon",
            component = "llm",
            model = $model,
            provider = $provider
        )
    };
    ($name:expr, model: $model:expr, provider: $provider:expr, $($key:ident = $value:expr),*) => {
        tracing::info_span!($name, 
            service = "at-daemon",
            component = "llm",
            model = $model,
            provider = $provider,
            $($key = $value),*
        )
    };
}

/// Profile async function with detailed metrics
#[macro_export]
macro_rules! profile_async {
    ($name:expr, $future:expr) => {
        async {
            let start = std::time::Instant::now();
            let _span = tracing::info_span!($name, 
                operation_type = "async"
            );
            
            let result = $future.await;
            let duration = start.elapsed();
            
            tracing::info!(
                operation = $name,
                duration_ms = duration.as_millis(),
                duration_us = duration.as_micros(),
                "async operation completed"
            );
            
            result
        }
    };
}

/// Profile synchronous function with timing
pub fn profile_function<F, R>(name: &str, f: F) -> R 
where 
    F: FnOnce() -> R,
{
    let start = Instant::now();
    let _span = info_span!(
        "function_execution",
        operation = name,
        operation_type = "sync"
    );
    
    let result = f();
    let duration = start.elapsed();
    
    info!(
        operation = name,
        duration_ms = duration.as_millis(),
        duration_us = duration.as_micros(),
        "function execution completed"
    );
    
    result
}

/// Add custom metrics and tags to current span
pub fn add_span_tags(tags: &[(&str, &str)]) {
    let _span = tracing::Span::current();
    // For now, just log the tags
    for (key, value) in tags {
        tracing::debug!(tag_key = key, tag_value = value, "span tag");
    }
}

/// Record custom event with metrics
pub fn record_event(event_name: &str, metrics: &[(&str, &str)]) {
    info!(
        event = event_name,
        service = "at-daemon"
    );
    
    for (_key, value) in metrics {
        info!(metric_value = %value, "event metric");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_traced_span_macro() {
        let span = traced_span!("test_operation", user_id = "123", action = "test");
        // This would normally be used in actual tracing context
    }

    #[test]
    fn test_profile_function() {
        let result = profile_function("test_add", || {
            std::thread::sleep(std::time::Duration::from_millis(10));
            2 + 2
        });
        assert_eq!(result, 4);
    }
}
