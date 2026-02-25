//! Terminal management API routes.
//!
//! Provides endpoints for managing PTY terminal sessions, including lifecycle
//! operations (create, list, delete), settings management, and WebSocket connection
//! for interactive terminal I/O.
//!
//! All routes are prefixed with `/api/terminals` or `/ws/terminal` and must be
//! merged into the main router using `.merge()` or `.nest()`.

use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;

use crate::http_api::ApiState;
use crate::terminal_ws::{
    auto_name_terminal, create_terminal, delete_terminal, list_persistent_terminals,
    list_terminals, terminal_ws, update_terminal_settings,
};

/// Build the terminals sub-router.
///
/// Includes all terminal-related endpoints:
/// - Terminal lifecycle (create, list, delete)
/// - Terminal settings management (update settings, auto-naming)
/// - Persistent terminal queries
/// - WebSocket connection for interactive I/O
///
/// All routes are mounted under `/api/terminals` and `/ws/terminal` — the caller
/// is responsible for merging this into the top-level router.
pub fn terminals_router() -> Router<Arc<ApiState>> {
    Router::new()
        .route("/api/terminals", get(list_terminals).post(create_terminal))
        .route(
            "/api/terminals/{id}",
            axum::routing::delete(delete_terminal),
        )
        .route(
            "/api/terminals/{id}/settings",
            axum::routing::patch(update_terminal_settings),
        )
        .route(
            "/api/terminals/{id}/auto-name",
            post(auto_name_terminal),
        )
        .route(
            "/api/terminals/persistent",
            get(list_persistent_terminals),
        )
        .route("/ws/terminal/{id}", get(terminal_ws))
}
