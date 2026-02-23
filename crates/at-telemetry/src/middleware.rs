use axum::{body::Body, extract::Request, middleware::Next, response::Response};
use std::time::Instant;

use crate::metrics::global_metrics;

/// Axum middleware that records API request metrics.
///
/// For each request it records:
/// - `api_requests_total` counter with labels `method`, `path`, `status`
/// - `api_request_duration_seconds` histogram
pub async fn metrics_middleware(request: Request<Body>, next: Next) -> Response {
    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let start = Instant::now();

    let response = next.run(request).await;

    let duration = start.elapsed().as_secs_f64();
    let status = response.status().as_u16().to_string();

    let m = global_metrics();
    m.increment_counter(
        "api_requests_total",
        &[("method", &method), ("path", &path), ("status", &status)],
    );
    m.record_histogram("api_request_duration_seconds", duration);

    response
}
