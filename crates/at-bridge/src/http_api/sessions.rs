use axum::{
    extract::State,
    response::IntoResponse,
    Json,
};
use std::sync::Arc;

use at_core::session_store::SessionState;

use crate::api_error::ApiError;
use super::state::ApiState;

/// GET /api/sessions/ui -- retrieve the most recent UI session state.
pub(crate) async fn get_ui_session(State(state): State<Arc<ApiState>>) -> Result<impl IntoResponse, ApiError> {
    let sessions = state.session_store.list_sessions().await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if let Some(session) = sessions.into_iter().next() {
        Ok((axum::http::StatusCode::OK, Json(serde_json::json!(session))))
    } else {
        Ok((axum::http::StatusCode::OK, Json(serde_json::json!(null))))
    }
}

/// PUT /api/sessions/ui -- save or update UI session state.
pub(crate) async fn save_ui_session(
    State(state): State<Arc<ApiState>>,
    Json(mut session): Json<SessionState>,
) -> Result<impl IntoResponse, ApiError> {
    session.last_active_at = chrono::Utc::now();
    state.session_store.save_session(&session).await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok((axum::http::StatusCode::OK, Json(serde_json::json!(session))))
}

/// GET /api/sessions/ui/list -- retrieve all saved UI sessions.
pub(crate) async fn list_ui_sessions(State(state): State<Arc<ApiState>>) -> Result<impl IntoResponse, ApiError> {
    let sessions = state.session_store.list_sessions().await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok((
        axum::http::StatusCode::OK,
        Json(serde_json::json!(sessions)),
    ))
}
