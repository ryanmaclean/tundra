//! Worktree management API routes.
//!
//! Provides endpoints for managing git worktrees: listing, creating branches,
//! merge operations, merge preview, conflict resolution, and cleanup.
//!
//! All routes are prefixed with `/api/worktrees` and must be merged into the main
//! router using `.merge()` or `.nest()`.

use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;

use crate::http_api::{
    delete_worktree, list_worktrees, merge_preview, merge_worktree, resolve_conflict, ApiState,
};

/// Build the worktrees sub-router.
///
/// Includes all worktree-related endpoints:
/// - List all git worktrees
/// - Delete a worktree
/// - Merge worktree to main branch
/// - Preview merge conflicts before merging
/// - Resolve conflicts during merge
///
/// All routes are mounted under `/api/worktrees` — the caller is responsible for
/// merging this into the top-level router.
pub fn worktrees_router() -> Router<Arc<ApiState>> {
    Router::new()
        // List worktrees
        .route("/api/worktrees", get(list_worktrees))
        // Worktree operations by ID
        .route(
            "/api/worktrees/{id}",
            axum::routing::delete(delete_worktree),
        )
        // Merge operations
        .route("/api/worktrees/{id}/merge", post(merge_worktree))
        .route("/api/worktrees/{id}/merge-preview", get(merge_preview))
        // Conflict resolution
        .route("/api/worktrees/{id}/resolve", post(resolve_conflict))
}
