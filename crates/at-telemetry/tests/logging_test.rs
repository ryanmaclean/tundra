use at_telemetry::logging;

#[test]
fn test_init_logging_human() {
    // Should not panic; second call is a safe no-op.
    logging::init_logging("test-service", "debug");
    logging::init_logging("test-service", "info");

    tracing::info!(key = "value", "human-readable log line");
}

#[test]
fn test_init_logging_json() {
    // Because the global subscriber is already set by the first test that runs,
    // this will silently no-op -- which is exactly the behaviour we want.
    logging::init_logging_json("test-service-json", "info");

    tracing::info!(key = "value", "json log line");
}

#[test]
fn test_default_level_fallback() {
    // Ensure we don't panic when RUST_LOG is not set and we rely on the default.
    std::env::remove_var("RUST_LOG");
    logging::init_logging("fallback-test", "warn");
}
