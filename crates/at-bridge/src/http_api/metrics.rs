use axum::{
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use std::sync::Arc;

use at_telemetry::metrics::global_metrics;

use crate::http_api::ApiState;

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

/// Creates the metrics sub-router.
///
/// Provides telemetry metrics export endpoints in multiple formats:
/// - GET `/` - Prometheus text format
/// - GET `/json` - JSON format
pub fn metrics_router() -> Router<Arc<ApiState>> {
    Router::new()
        .route("/", get(get_metrics_prometheus))
        .route("/json", get(get_metrics_json))
}
