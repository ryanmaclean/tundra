//! Kanban board and Planning Poker API routes.
//!
//! Provides endpoints for Kanban board configuration, column management,
//! task ordering, and Planning Poker session management.
//!
//! All routes are prefixed with `/api/kanban` and must be merged into the main
//! router using `.merge()` or `.nest()`.

use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;

use crate::http_api::{
    get_kanban_columns, get_planning_poker_session, lock_column, patch_kanban_columns,
    reveal_planning_poker, save_task_ordering, simulate_planning_poker, start_planning_poker,
    submit_planning_poker_vote, ApiState,
};

/// Build the kanban sub-router.
///
/// Includes all kanban-related endpoints:
/// - Kanban column configuration (get, update)
/// - Column locking
/// - Task ordering persistence
/// - Planning Poker session management (start, vote, reveal, simulate, get)
///
/// All routes are mounted under `/api/kanban` — the caller is responsible for
/// merging this into the top-level router.
pub fn kanban_router() -> Router<Arc<ApiState>> {
    Router::new()
        // Kanban column configuration
        .route(
            "/api/kanban/columns",
            get(get_kanban_columns).patch(patch_kanban_columns),
        )
        .route("/api/kanban/columns/lock", post(lock_column))
        // Task ordering
        .route("/api/kanban/ordering", post(save_task_ordering))
        // Planning Poker
        .route("/api/kanban/poker/start", post(start_planning_poker))
        .route("/api/kanban/poker/vote", post(submit_planning_poker_vote))
        .route("/api/kanban/poker/reveal", post(reveal_planning_poker))
        .route("/api/kanban/poker/simulate", post(simulate_planning_poker))
        .route(
            "/api/kanban/poker/{bead_id}",
            get(get_planning_poker_session),
        )
}
