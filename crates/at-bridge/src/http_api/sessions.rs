use axum::{
    extract::State,
    response::IntoResponse,
    routing::{get, put},
    Json, Router,
};
use std::sync::Arc;

use at_core::session_store::SessionState;

use super::state::ApiState;

/// GET /api/sessions/ui -- retrieve the most recent UI session state.
pub(crate) async fn get_ui_session(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    match state.session_store.list_sessions().await {
        Ok(sessions) => {
            if let Some(session) = sessions.into_iter().next() {
                (axum::http::StatusCode::OK, Json(serde_json::json!(session)))
            } else {
                (axum::http::StatusCode::OK, Json(serde_json::json!(null)))
            }
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

/// PUT /api/sessions/ui -- save or update UI session state.
pub(crate) async fn save_ui_session(
    State(state): State<Arc<ApiState>>,
    Json(mut session): Json<SessionState>,
) -> impl IntoResponse {
    session.last_active_at = chrono::Utc::now();
    match state.session_store.save_session(&session).await {
        Ok(()) => (axum::http::StatusCode::OK, Json(serde_json::json!(session))),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

/// GET /api/sessions/ui/list -- retrieve all saved UI sessions.
pub(crate) async fn list_ui_sessions(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    match state.session_store.list_sessions().await {
        Ok(sessions) => (
            axum::http::StatusCode::OK,
            Json(serde_json::json!(sessions)),
        ),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

/// Builds the sessions sub-router.
///
/// This router handles UI session state management endpoints.
pub fn sessions_router() -> Router<Arc<ApiState>> {
    Router::new()
        .route("/api/sessions/ui", get(get_ui_session).put(save_ui_session))
        .route("/api/sessions/ui/list", get(list_ui_sessions))
}
