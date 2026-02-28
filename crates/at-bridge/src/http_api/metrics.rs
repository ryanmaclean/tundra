use axum::{
    response::IntoResponse,
    Json,
};

use at_telemetry::metrics::global_metrics;

/// GET /api/metrics -- exports telemetry metrics in Prometheus text format.
pub(crate) async fn get_metrics_prometheus() -> impl IntoResponse {
    let body = global_metrics().export_prometheus();
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
}

/// GET /api/metrics/json -- exports telemetry metrics in JSON format.
pub(crate) async fn get_metrics_json() -> impl IntoResponse {
    Json(global_metrics().export_json())
}
