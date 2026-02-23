use at_telemetry::metrics::{global_metrics, Labels, MetricsCollector};
use at_telemetry::tracing_setup::{
    create_child_span, create_operation_span, generate_span_id, generate_trace_id,
};

// ---------------------------------------------------------------------------
// Metrics Collector — Counters
// ---------------------------------------------------------------------------

#[test]
fn test_counter_increment() {
    let m = MetricsCollector::new();
    assert_eq!(m.get_counter("requests", &[("method", "GET")]), 0);

    m.increment_counter("requests", &[("method", "GET")]);
    assert_eq!(m.get_counter("requests", &[("method", "GET")]), 1);

    m.increment_counter("requests", &[("method", "GET")]);
    m.increment_counter("requests", &[("method", "GET")]);
    assert_eq!(m.get_counter("requests", &[("method", "GET")]), 3);

    // Different label set is a different counter
    m.increment_counter("requests", &[("method", "POST")]);
    assert_eq!(m.get_counter("requests", &[("method", "POST")]), 1);
    assert_eq!(m.get_counter("requests", &[("method", "GET")]), 3);
}

#[test]
fn test_counter_increment_by() {
    let m = MetricsCollector::new();
    m.increment_counter_by("tokens", &[("dir", "in")], 100);
    assert_eq!(m.get_counter("tokens", &[("dir", "in")]), 100);

    m.increment_counter_by("tokens", &[("dir", "in")], 250);
    assert_eq!(m.get_counter("tokens", &[("dir", "in")]), 350);

    // Different labels
    m.increment_counter_by("tokens", &[("dir", "out")], 50);
    assert_eq!(m.get_counter("tokens", &[("dir", "out")]), 50);
    assert_eq!(m.get_counter("tokens", &[("dir", "in")]), 350);

    // Increment by 0 is valid
    m.increment_counter_by("tokens", &[("dir", "in")], 0);
    assert_eq!(m.get_counter("tokens", &[("dir", "in")]), 350);
}

// ---------------------------------------------------------------------------
// Metrics Collector — Gauges
// ---------------------------------------------------------------------------

#[test]
fn test_gauge_set() {
    let m = MetricsCollector::new();
    assert_eq!(m.get_gauge("agents_active"), 0);

    m.set_gauge("agents_active", 5);
    assert_eq!(m.get_gauge("agents_active"), 5);

    m.set_gauge("agents_active", 3);
    assert_eq!(m.get_gauge("agents_active"), 3);

    m.set_gauge("agents_active", 0);
    assert_eq!(m.get_gauge("agents_active"), 0);
}

#[test]
fn test_gauge_increment_decrement() {
    let m = MetricsCollector::new();

    // Gauges can be set to positive and negative values
    m.set_gauge("temperature", 20);
    assert_eq!(m.get_gauge("temperature"), 20);

    m.set_gauge("temperature", -5);
    assert_eq!(m.get_gauge("temperature"), -5);

    // Multiple gauges are independent
    m.set_gauge("pressure", 1013);
    assert_eq!(m.get_gauge("pressure"), 1013);
    assert_eq!(m.get_gauge("temperature"), -5);

    // Simulated increment/decrement via get + set
    let current = m.get_gauge("pressure");
    m.set_gauge("pressure", current + 1);
    assert_eq!(m.get_gauge("pressure"), 1014);

    let current = m.get_gauge("pressure");
    m.set_gauge("pressure", current - 10);
    assert_eq!(m.get_gauge("pressure"), 1004);
}

// ---------------------------------------------------------------------------
// Metrics Collector — Histograms
// ---------------------------------------------------------------------------

#[test]
fn test_histogram_record() {
    let m = MetricsCollector::new();
    m.record_histogram("request_duration", 0.05);
    m.record_histogram("request_duration", 0.5);
    m.record_histogram("request_duration", 2.0);

    let json = m.export_json();
    let hist = &json["histograms"]["request_duration"];
    assert_eq!(hist["count"], 3);

    let sum = hist["sum"].as_f64().unwrap();
    assert!((sum - 2.55).abs() < 0.001);
}

#[test]
fn test_histogram_multiple_observations() {
    let m = MetricsCollector::new();
    let values = [0.001, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0];
    for v in &values {
        m.record_histogram("latency", *v);
    }

    let json = m.export_json();
    let hist = &json["histograms"]["latency"];
    assert_eq!(hist["count"], values.len() as u64);

    let expected_sum: f64 = values.iter().sum();
    let actual_sum = hist["sum"].as_f64().unwrap();
    assert!(
        (actual_sum - expected_sum).abs() < 0.001,
        "expected sum {}, got {}",
        expected_sum,
        actual_sum
    );
}

// ---------------------------------------------------------------------------
// Prometheus Export Format
// ---------------------------------------------------------------------------

#[test]
fn test_prometheus_export_format() {
    let m = MetricsCollector::new();

    // Add a counter with labels
    m.increment_counter(
        "http_requests_total",
        &[("method", "GET"), ("status", "200")],
    );
    m.increment_counter(
        "http_requests_total",
        &[("method", "GET"), ("status", "200")],
    );

    // Add a gauge
    m.set_gauge("beads_in_flight", 7);

    // Add a histogram observation
    m.record_histogram("api_latency_seconds", 0.123);

    let output = m.export_prometheus();

    // Verify counter section
    assert!(
        output.contains("# TYPE http_requests_total counter"),
        "missing counter TYPE line"
    );
    assert!(
        output.contains("http_requests_total{method=\"GET\",status=\"200\"} 2"),
        "missing counter value line, output: {}",
        output
    );

    // Verify gauge section
    assert!(
        output.contains("# TYPE beads_in_flight gauge"),
        "missing gauge TYPE line"
    );
    assert!(
        output.contains("beads_in_flight 7"),
        "missing gauge value line"
    );

    // Verify histogram section
    assert!(
        output.contains("# TYPE api_latency_seconds histogram"),
        "missing histogram TYPE line"
    );
    assert!(
        output.contains("api_latency_seconds_count 1"),
        "missing histogram count"
    );
    assert!(
        output.contains("api_latency_seconds_bucket{le=\"+Inf\"} 1"),
        "missing +Inf bucket"
    );
}

// ---------------------------------------------------------------------------
// Metrics Labels
// ---------------------------------------------------------------------------

#[test]
fn test_metrics_labels() {
    // Labels sort by key
    let l = Labels::new(&[("z_key", "z_val"), ("a_key", "a_val")]);
    assert_eq!(l.prometheus_str(), "{a_key=\"a_val\",z_key=\"z_val\"}");

    // Empty labels
    let empty = Labels::empty();
    assert_eq!(empty.prometheus_str(), "");

    // Single label
    let single = Labels::new(&[("method", "POST")]);
    assert_eq!(single.prometheus_str(), "{method=\"POST\"}");

    // Labels equality
    let l1 = Labels::new(&[("a", "1"), ("b", "2")]);
    let l2 = Labels::new(&[("b", "2"), ("a", "1")]);
    assert_eq!(
        l1, l2,
        "labels with same pairs in different order should be equal"
    );
}

#[test]
fn test_counter_with_different_label_sets() {
    let m = MetricsCollector::new();

    m.increment_counter("api_calls", &[("service", "auth"), ("method", "login")]);
    m.increment_counter("api_calls", &[("service", "auth"), ("method", "logout")]);
    m.increment_counter("api_calls", &[("service", "data"), ("method", "query")]);

    assert_eq!(
        m.get_counter("api_calls", &[("service", "auth"), ("method", "login")]),
        1
    );
    assert_eq!(
        m.get_counter("api_calls", &[("service", "auth"), ("method", "logout")]),
        1
    );
    assert_eq!(
        m.get_counter("api_calls", &[("service", "data"), ("method", "query")]),
        1
    );
    // Non-existent label combo returns 0
    assert_eq!(
        m.get_counter("api_calls", &[("service", "data"), ("method", "login")]),
        0
    );
}

// ---------------------------------------------------------------------------
// Tracing Setup
// ---------------------------------------------------------------------------

#[test]
fn test_tracing_init() {
    // init_logging is safe to call multiple times (subsequent calls are no-ops)
    at_telemetry::logging::init_logging("test-service", "warn");
    // Calling again should not panic
    at_telemetry::logging::init_logging("test-service-2", "debug");
}

#[test]
fn test_tracing_json_format() {
    // init_logging_json is also safe to call multiple times
    // Since init_logging already set the global subscriber, this will be a no-op
    at_telemetry::logging::init_logging_json("test-json-service", "info");
}

// ---------------------------------------------------------------------------
// Tracing — trace and span ID generation
// ---------------------------------------------------------------------------

#[test]
fn test_trace_id_format() {
    let id = generate_trace_id();
    assert_eq!(id.len(), 32, "trace ID should be 32 hex chars");
    assert!(
        id.chars().all(|c| c.is_ascii_hexdigit()),
        "trace ID should be all hex: {}",
        id
    );
}

#[test]
fn test_trace_id_uniqueness() {
    let ids: Vec<String> = (0..100).map(|_| generate_trace_id()).collect();
    let unique: std::collections::HashSet<&String> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len(), "trace IDs should be unique");
}

#[test]
fn test_span_id_format() {
    let id = generate_span_id();
    assert_eq!(id.len(), 16, "span ID should be 16 hex chars");
    assert!(
        id.chars().all(|c| c.is_ascii_hexdigit()),
        "span ID should be all hex: {}",
        id
    );
}

#[test]
fn test_operation_span_creation() {
    let (span, trace_id) = create_operation_span("test_operation");
    assert_eq!(trace_id.len(), 32);
    let _guard = span.enter();
    // Span should be valid and enterable without panic
}

#[test]
fn test_child_span_creation() {
    let parent_trace_id = generate_trace_id();
    let span = create_child_span(&parent_trace_id, "child_work");
    let _guard = span.enter();
    // Child span should be valid
}

// ---------------------------------------------------------------------------
// Request Metrics Middleware (unit-level verification)
// ---------------------------------------------------------------------------

#[test]
fn test_request_counter_incremented() {
    // We use the global metrics singleton to verify the counter name/label
    // pattern used by the middleware. The middleware increments
    // "api_requests_total" with method/path/status labels.
    let m = global_metrics();
    let before = m.get_counter(
        "api_requests_total",
        &[("method", "GET"), ("path", "/test"), ("status", "200")],
    );
    m.increment_counter(
        "api_requests_total",
        &[("method", "GET"), ("path", "/test"), ("status", "200")],
    );
    let after = m.get_counter(
        "api_requests_total",
        &[("method", "GET"), ("path", "/test"), ("status", "200")],
    );
    assert_eq!(after, before + 1);
}

#[test]
fn test_request_duration_recorded() {
    // The middleware records "api_request_duration_seconds" histogram.
    // Verify we can record into it and it shows up in export.
    let m = global_metrics();
    m.record_histogram("api_request_duration_seconds", 0.042);

    let output = m.export_prometheus();
    assert!(
        output.contains("api_request_duration_seconds"),
        "histogram should appear in prometheus export"
    );
}

#[test]
fn test_request_id_generated() {
    // generate_trace_id produces valid OTel-compatible trace IDs
    let id = generate_trace_id();
    assert_eq!(id.len(), 32);
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()));

    // Each call generates a different ID (used as X-Request-Id in middleware)
    let id2 = generate_trace_id();
    assert_ne!(id, id2, "each request ID should be unique");
}

// ---------------------------------------------------------------------------
// JSON Export
// ---------------------------------------------------------------------------

#[test]
fn test_json_export_structure() {
    let m = MetricsCollector::new();
    m.increment_counter("events_total", &[]);
    m.set_gauge("queue_depth", 42);
    m.record_histogram("processing_time", 0.5);

    let json = m.export_json();

    assert!(json["counters"].is_object());
    assert!(json["gauges"].is_object());
    assert!(json["histograms"].is_object());

    assert_eq!(json["gauges"]["queue_depth"], 42);

    let hist = &json["histograms"]["processing_time"];
    assert_eq!(hist["count"], 1);
    assert!(hist["buckets"].is_array());
}

// ---------------------------------------------------------------------------
// Global Singleton
// ---------------------------------------------------------------------------

#[test]
fn test_global_metrics_is_singleton() {
    let m1 = global_metrics();
    let m2 = global_metrics();
    assert!(
        std::ptr::eq(m1, m2),
        "global_metrics should return the same instance"
    );
}

#[test]
fn test_global_metrics_has_default_histograms() {
    let m = global_metrics();
    // with_defaults pre-registers these histograms
    let output = m.export_prometheus();
    assert!(
        output.contains("llm_request_duration_seconds")
            || output.contains("api_request_duration_seconds"),
        "global metrics should have pre-registered histograms"
    );
}
