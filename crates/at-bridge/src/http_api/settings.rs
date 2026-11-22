use axum::{extract::State, response::IntoResponse, Json};
use std::sync::Arc;

use at_core::config::{Config, ConfigError};
use axum::http::StatusCode;

use super::merge_json;
use super::state::ApiState;

/// Map a settings load/validation error to an HTTP error response.
///
/// A settings file that exists but cannot be parsed or fails validation is
/// a 409 Conflict (the file on disk must be fixed first); I/O failures are
/// 500. The response names the file so the caller can locate it.
pub(crate) fn settings_error_response(
    e: &ConfigError,
    path: &std::path::Path,
) -> (StatusCode, Json<serde_json::Value>) {
    let status = match e {
        ConfigError::Parse(_) | ConfigError::Validation(_) => StatusCode::CONFLICT,
        ConfigError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(serde_json::json!({
            "error": format!("settings file is invalid or unreadable: {e}"),
            "path": path.display().to_string(),
        })),
    )
}

/// GET /api/settings -- retrieve the current application configuration.
///
/// Returns the full Config object including all sections (general, security, UI,
/// bridge, agents, integrations, kanban, etc.). If no saved configuration exists,
/// returns the default configuration.
///
/// **Response:** 200 OK with Config JSON object; 409 if the settings file
/// exists but is unparseable or invalid; 500 if it cannot be read.
pub(crate) async fn get_settings(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    match state.settings_manager.load_for_update() {
        Ok(cfg) => (StatusCode::OK, Json(serde_json::json!(cfg))),
        Err(e) => settings_error_response(&e, state.settings_manager.path()),
    }
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
    save_response(&state, &cfg)
}

/// Validate and persist `cfg`, returning the saved config or an error:
/// 400 for a config that fails validation, 500 if the write fails.
fn save_response(state: &ApiState, cfg: &Config) -> (StatusCode, Json<serde_json::Value>) {
    if let Err(e) = cfg.validate() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e.to_string()})),
        );
    }
    match state.settings_manager.save(cfg) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!(cfg))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
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
/// If the settings file on disk is unparseable or invalid, nothing is saved
/// (merging into defaults would wipe the user's settings) and 409 is returned.
///
/// **Request Body:** Partial Config JSON object with only the fields to update.
/// **Response:** 200 OK with updated Config, 400 if merge creates invalid config,
/// 409 if the existing settings file is invalid, 500 if load/save fails.
pub(crate) async fn patch_settings(
    State(state): State<Arc<ApiState>>,
    Json(partial): Json<serde_json::Value>,
) -> impl IntoResponse {
    let mut current = match state.settings_manager.load_for_update() {
        Ok(cfg) => cfg,
        Err(e) => return settings_error_response(&e, state.settings_manager.path()),
    };
    let mut current_val = match serde_json::to_value(&current) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
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
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e.to_string()})),
            );
        }
    };

    save_response(&state, &current)
}
