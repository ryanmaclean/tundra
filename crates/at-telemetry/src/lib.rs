//! Telemetry and observability infrastructure for auto-tundra services.
//!
//! This crate provides a unified observability layer combining logging, metrics,
//! and distributed tracing. It integrates with the `tracing` ecosystem for
//! structured logging and spans, exposes Prometheus-compatible metrics, and
//! provides OpenTelemetry-compatible trace/span ID generation for correlation
//! across services.
//!
//! Key components:
//! - **Logging**: Human-readable and JSON-formatted output via `tracing-subscriber`
//! - **Metrics**: Thread-safe counters, gauges, and histograms with Prometheus export
//! - **Middleware**: Axum middleware for automatic request metrics and trace ID injection
//! - **Tracing**: OpenTelemetry-compatible trace/span ID generation and correlation

pub mod logging;
pub mod metrics;
pub mod middleware;
pub mod tracing_setup;
