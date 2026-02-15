use tracing_subscriber::{EnvFilter, fmt};

/// Initialize logging with human-readable output format.
///
/// Uses the `RUST_LOG` environment variable if set, otherwise falls back
/// to `default_level` (e.g. "info", "debug", "at_core=debug,warn").
///
/// Safe to call multiple times (e.g. in tests) -- subsequent calls are no-ops.
pub fn init_logging(service_name: &str, default_level: &str) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_level));

    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(true)
        .with_line_number(true)
        .with_level(true)
        .try_init()
        .ok();

    tracing::info!(service = service_name, "logging initialised (human-readable)");
}

/// Initialize logging with JSON output format (suitable for Vector / Loki / ELK).
///
/// Uses the `RUST_LOG` environment variable if set, otherwise falls back
/// to `default_level`.
///
/// Safe to call multiple times -- subsequent calls are no-ops.
pub fn init_logging_json(service_name: &str, default_level: &str) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_level));

    fmt()
        .json()
        .with_env_filter(filter)
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .with_level(true)
        .try_init()
        .ok();

    tracing::info!(service = service_name, "logging initialised (json)");
}
