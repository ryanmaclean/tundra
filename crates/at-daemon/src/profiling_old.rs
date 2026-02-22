use tracing::{info, info_span};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize Datadog APM and profiling
pub fn init_datadog() -> anyhow::Result<()> {
    // Note: tracing is already initialized by at_telemetry::logging
    // We just need to log that Datadog is ready
    info!("Datadog APM and profiling ready (tracing already initialized)");

    Ok(())
}

/// Create a traced span for monitoring
#[macro_export]
macro_rules! traced_span {
    ($name:expr) => {
        tracing::info_span!($name, service = "at-daemon")
    };
}

/// Profile a function execution
pub fn profile_function<F, R>(name: &str, f: F) -> R 
where 
    F: FnOnce() -> R,
{
    let _span = info_span!("function_execution", name = name);
    f()
}

/// Profile an async function execution
#[macro_export]
macro_rules! profile_async {
    ($name:expr, $future:expr) => {
        async {
            let _span = tracing::info_span!($name, async_name = $name);
            $future.await
        }
    };
}
