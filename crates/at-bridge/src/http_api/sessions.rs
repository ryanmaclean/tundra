use axum::{
    extract::{Query, State},
    response::IntoResponse,
    Json,
};
use std::sync::Arc;

use at_core::session_store::SessionState;

use super::state::ApiState;
use super::types::SessionQuery;

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
pub(crate) async fn list_ui_sessions(
    State(state): State<Arc<ApiState>>,
    Query(params): Query<SessionQuery>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    match state.session_store.list_sessions().await {
        Ok(sessions) => {
            let paginated: Vec<_> = sessions.into_iter().skip(offset).take(limit).collect();
            (
                axum::http::StatusCode::OK,
                Json(serde_json::json!(paginated)),
            )
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}
