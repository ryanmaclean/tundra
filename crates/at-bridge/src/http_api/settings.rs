use axum::{extract::State, response::IntoResponse, Json};
use std::sync::Arc;

use at_core::config::Config;

use super::merge_json;
use super::state::ApiState;

/// GET /api/settings -- retrieve the current application configuration.
///
/// Returns the full Config object including all sections (general, security, UI,
/// bridge, agents, integrations, kanban, etc.). If no saved configuration exists,
/// returns the default configuration.
///
/// **Response:** 200 OK with Config JSON object.
pub(crate) async fn get_settings(State(state): State<Arc<ApiState>>) -> Json<Config> {
    let cfg = state.settings_manager.load_or_default();
    Json(cfg)
}

/// PUT /api/settings -- replace the entire application configuration.
///
/// Replaces the entire configuration with the provided Config object and persists it to disk.
/// All sections of the config must be provided; any omitted sections will be reset to their
/// default values. Use PATCH /api/settings for partial updates.
///
/// **Request Body:** Complete Config JSON object.
/// **Response:** 200 OK with saved Config, 500 if save fails.
pub(crate) async fn put_settings(
    State(state): State<Arc<ApiState>>,
    Json(cfg): Json<Config>,
) -> impl IntoResponse {
    match state.settings_manager.save(&cfg) {
        Ok(()) => (axum::http::StatusCode::OK, Json(serde_json::json!(cfg))),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

/// PATCH /api/settings -- partially update the application configuration.
///
/// Merges the provided partial configuration into the existing configuration and persists
/// the updated result to disk. Only the fields present in the request body are updated;
/// all other fields retain their current values.
///
/// **Request Body:** Partial Config JSON object with only the fields to update.
/// **Response:** 200 OK with updated Config, 400 if merge creates invalid config, 500 if save fails.
pub(crate) async fn patch_settings(
    State(state): State<Arc<ApiState>>,
    Json(partial): Json<serde_json::Value>,
) -> impl IntoResponse {
    let mut current = state.settings_manager.load_or_default();
    let mut current_val = match serde_json::to_value(&current) {
        Ok(v) => v,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            );
        }
    };

    // Merge partial into current
    merge_json(&mut current_val, &partial);

    current = match serde_json::from_value(current_val) {
        Ok(c) => c,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e.to_string()})),
            );
        }
    };

    match state.settings_manager.save(&current) {
        Ok(()) => (axum::http::StatusCode::OK, Json(serde_json::json!(current))),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}
