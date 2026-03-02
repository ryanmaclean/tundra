use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use uuid::Uuid;

use super::state::ApiState;
use super::types::NotificationQuery;

/// GET /api/notifications -- retrieve notifications with optional filtering.
pub(crate) async fn list_notifications(
    State(state): State<Arc<ApiState>>,
    Query(params): Query<NotificationQuery>,
) -> impl IntoResponse {
    let store = state.notification_store.read().await;
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    if params.unread == Some(true) {
        let unread: Vec<_> = store
            .list_unread()
            .into_iter()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect();
        Json(serde_json::json!(unread))
    } else {
        let all: Vec<_> = store.list_all(limit, offset).into_iter().cloned().collect();
        Json(serde_json::json!(all))
    }
}

/// GET /api/notifications/count -- retrieve notification counts.
pub(crate) async fn notification_count(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let store = state.notification_store.read().await;
    Json(serde_json::json!({
        "unread": store.unread_count(),
        "total": store.total_count(),
    }))
}

/// POST /api/notifications/{id}/read -- mark a single notification as read.
pub(crate) async fn mark_notification_read(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut store = state.notification_store.write().await;
    if store.mark_read(id) {
        (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({"status": "read", "id": id.to_string()})),
        )
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "notification not found"})),
        )
    }
}

/// POST /api/notifications/read-all -- mark all notifications as read.
pub(crate) async fn mark_all_notifications_read(
    State(state): State<Arc<ApiState>>,
) -> impl IntoResponse {
    let mut store = state.notification_store.write().await;
    store.mark_all_read();
    Json(serde_json::json!({"status": "all_read"}))
}

/// DELETE /api/notifications/{id} -- delete a notification.
pub(crate) async fn delete_notification(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let mut store = state.notification_store.write().await;
    if store.delete(id) {
        (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({"status": "deleted", "id": id.to_string()})),
        )
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "notification not found"})),
        )
    }
}
