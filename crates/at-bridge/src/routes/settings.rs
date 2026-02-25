//! Settings and credentials API routes.
//!
//! Provides endpoints for application configuration management and
//! credential provider status checking.
//!
//! All routes are prefixed with `/api/settings` or `/api/credentials`
//! and must be merged into the main router using `.merge()`.

use axum::{
    routing::{get, patch, post, put},
    Router,
};
use std::sync::Arc;

use crate::http_api::{
    get_credentials_status, get_settings, patch_settings, put_settings, toggle_direct_mode,
    ApiState,
};

/// Build the settings and credentials sub-router.
///
/// Includes all settings and credentials endpoints:
/// - Settings CRUD (get, update full, update partial)
/// - Direct mode toggle
/// - Credential provider status
///
/// All routes are mounted under `/api/settings` and `/api/credentials` —
/// the caller is responsible for merging this into the top-level router.
pub fn settings_router() -> Router<Arc<ApiState>> {
    Router::new()
        // Settings endpoints
        .route("/api/settings", get(get_settings))
        .route("/api/settings", put(put_settings))
        .route("/api/settings", patch(patch_settings))
        .route("/api/settings/direct-mode", post(toggle_direct_mode))
        // Credentials endpoints
        .route("/api/credentials/status", get(get_credentials_status))
}
