//! Project management API routes.
//!
//! Provides endpoints for managing projects (workspaces/repositories), including
//! CRUD operations and project activation for workspace switching.
//!
//! All routes are prefixed with `/api/projects` and must be merged into the main
//! router using `.merge()` or `.nest()`.

use axum::{
    routing::{get, post, put},
    Router,
};
use std::sync::Arc;

use crate::http_api::{
    activate_project, create_project, delete_project, list_projects, update_project, ApiState,
};

/// Build the projects sub-router.
///
/// Includes all project-related endpoints:
/// - Project CRUD (list, create, update, delete)
/// - Project activation for workspace switching
///
/// All routes are mounted under `/api/projects` — the caller is responsible for
/// merging this into the top-level router.
pub fn projects_router() -> Router<Arc<ApiState>> {
    Router::new()
        .route("/api/projects", get(list_projects).post(create_project))
        .route(
            "/api/projects/{id}",
            put(update_project).delete(delete_project),
        )
        .route("/api/projects/{id}/activate", post(activate_project))
}
