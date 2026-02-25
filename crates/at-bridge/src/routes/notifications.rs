//! Notification management API routes.
//!
//! Provides endpoints for listing, reading, and managing in-app notifications.
//! Supports notification counting, marking as read (single or bulk), and deletion.
//!
//! All routes are prefixed with `/api/notifications` and must be merged into the main
//! router using `.merge()` or `.nest()`.

use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;

use crate::http_api::{
    delete_notification, list_notifications, mark_all_notifications_read, mark_notification_read,
    notification_count, ApiState,
};

/// Build the notifications sub-router.
///
/// Includes all notification-related endpoints:
/// - List notifications (with pagination and filtering)
/// - Get notification counts (unread/total)
/// - Mark single notification as read
/// - Mark all notifications as read
/// - Delete notification
///
/// All routes are mounted under `/api/notifications` — the caller is responsible for
/// merging this into the top-level router.
pub fn notifications_router() -> Router<Arc<ApiState>> {
    Router::new()
        .route("/api/notifications", get(list_notifications))
        .route("/api/notifications/count", get(notification_count))
        .route("/api/notifications/{id}/read", post(mark_notification_read))
        .route(
            "/api/notifications/read-all",
            post(mark_all_notifications_read),
        )
        .route(
            "/api/notifications/{id}",
            axum::routing::delete(delete_notification),
        )
}
