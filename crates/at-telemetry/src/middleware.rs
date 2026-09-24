use axum::{
    body::Body,
    extract::{MatchedPath, Request},
    middleware::Next,
    response::Response,
};
use std::time::Instant;

use crate::metrics::global_metrics;

/// `path` label value used when the request did not match any route.
pub const UNMATCHED_PATH_LABEL: &str = "unmatched";

/// Axum middleware that records API request metrics.
///
/// For each request it records:
/// - `api_requests_total` counter with labels `method`, `path`, `status`
/// - `api_request_duration_seconds` histogram
///
/// The `path` label is the matched route template (e.g. `/api/tasks/{id}`),
/// never the concrete request path, so per-resource URLs share one series
/// and label cardinality stays bounded by the number of routes. Requests that
/// match no route are labelled [`UNMATCHED_PATH_LABEL`]. Attach this with
/// `Router::layer` so it runs after routing and sees [`MatchedPath`].
pub async fn metrics_middleware(request: Request<Body>, next: Next) -> Response {
    let method = request.method().to_string();
    let path = request
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_owned())
        .unwrap_or_else(|| UNMATCHED_PATH_LABEL.to_owned());
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http::StatusCode, middleware, routing::get, Router};
    use tower::ServiceExt;

    async fn send(app: &Router, uri: &str) -> StatusCode {
        app.clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn path_label_is_route_template_not_raw_path() {
        // Route prefix is unique to this test: the collector is process-global.
        let app = Router::new()
            .route("/mw-card-test/tasks/{id}", get(|| async { "ok" }))
            .layer(middleware::from_fn(metrics_middleware));

        for id in ["a1", "b2", "c3"] {
            assert_eq!(
                send(&app, &format!("/mw-card-test/tasks/{id}")).await,
                StatusCode::OK
            );
        }

        let m = global_metrics();
        let templ = [
            ("method", "GET"),
            ("path", "/mw-card-test/tasks/{id}"),
            ("status", "200"),
        ];
        assert_eq!(m.get_counter("api_requests_total", &templ), 3);
        for id in ["a1", "b2", "c3"] {
            let raw = format!("/mw-card-test/tasks/{id}");
            let labels = [("method", "GET"), ("path", raw.as_str()), ("status", "200")];
            assert_eq!(m.get_counter("api_requests_total", &labels), 0);
        }
        let export = m.export_prometheus();
        assert!(!export.contains("/mw-card-test/tasks/a1"));
    }

    #[tokio::test]
    async fn unmatched_requests_share_one_label() {
        let app = Router::new()
            .route("/mw-unmatched-test/known", get(|| async { "ok" }))
            .layer(middleware::from_fn(metrics_middleware));

        let before = global_metrics().get_counter(
            "api_requests_total",
            &[
                ("method", "GET"),
                ("path", UNMATCHED_PATH_LABEL),
                ("status", "404"),
            ],
        );
        for uri in ["/mw-unmatched-test/x1", "/mw-unmatched-test/x2"] {
            assert_eq!(send(&app, uri).await, StatusCode::NOT_FOUND);
        }
        let after = global_metrics().get_counter(
            "api_requests_total",
            &[
                ("method", "GET"),
                ("path", UNMATCHED_PATH_LABEL),
                ("status", "404"),
            ],
        );
        // Other tests may also hit the unmatched series concurrently.
        assert!(after - before >= 2);
        assert!(!global_metrics()
            .export_prometheus()
            .contains("/mw-unmatched-test/x1"));
    }
}
