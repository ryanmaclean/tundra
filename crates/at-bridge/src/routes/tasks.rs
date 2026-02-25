//! Task management API routes.
//!
//! Provides endpoints for CRUD operations on tasks, task execution pipelines,
//! build logs and status tracking, task archiving, attachments, and draft auto-save.
//!
//! All routes are prefixed with `/api/tasks` and must be merged into the main
//! router using `.merge()` or `.nest()`.

use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;

use crate::http_api::{
    add_attachment, archive_task, create_task, delete_attachment, delete_task, delete_task_draft,
    execute_task_pipeline, get_build_logs, get_build_status, get_task, get_task_draft,
    get_task_logs, list_archived_tasks, list_attachments, list_task_drafts, list_tasks,
    save_task_draft, unarchive_task, update_task, update_task_phase, ApiState,
};

/// Build the tasks sub-router.
///
/// Includes all task-related endpoints:
/// - Task CRUD (list, create, get, update, delete)
/// - Task phase management
/// - Task execution pipeline
/// - Build logs and status
/// - Task archiving
/// - Task attachments
/// - Task draft auto-save
///
/// All routes are mounted under `/api/tasks` — the caller is responsible for
/// merging this into the top-level router.
pub fn tasks_router() -> Router<Arc<ApiState>> {
    Router::new()
        // Core task CRUD
        .route("/api/tasks", get(list_tasks).post(create_task))
        .route(
            "/api/tasks/{id}",
            get(get_task).put(update_task).delete(delete_task),
        )
        // Task phase and execution
        .route("/api/tasks/{id}/phase", post(update_task_phase))
        .route("/api/tasks/{id}/logs", get(get_task_logs))
        .route("/api/tasks/{id}/execute", post(execute_task_pipeline))
        // Build tracking
        .route("/api/tasks/{id}/build-logs", get(get_build_logs))
        .route("/api/tasks/{id}/build-status", get(get_build_status))
        // Archiving
        .route("/api/tasks/{id}/archive", post(archive_task))
        .route("/api/tasks/{id}/unarchive", post(unarchive_task))
        .route("/api/tasks/archived", get(list_archived_tasks))
        // Attachments
        .route(
            "/api/tasks/{task_id}/attachments",
            get(list_attachments).post(add_attachment),
        )
        .route(
            "/api/tasks/{task_id}/attachments/{id}",
            axum::routing::delete(delete_attachment),
        )
        // Draft auto-save
        .route(
            "/api/tasks/drafts",
            get(list_task_drafts).post(save_task_draft),
        )
        .route(
            "/api/tasks/drafts/{id}",
            get(get_task_draft).delete(delete_task_draft),
        )
}
