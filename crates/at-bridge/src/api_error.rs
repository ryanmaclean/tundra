//! HTTP API error types.
//!
//! Provides a unified `ApiError` enum for consistent error responses across
//! the HTTP API layer. Implements Axum's `IntoResponse` trait to automatically
//! convert errors into appropriate HTTP responses.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors that can occur in the HTTP API layer.
#[derive(Debug, Error)]
pub enum ApiError {
    /// The requested resource was not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// The request was malformed or invalid.
    #[error("bad request: {0}")]
    BadRequest(String),

    /// An internal server error occurred.
    #[error("internal error: {0}")]
    InternalError(String),
}

// ---------------------------------------------------------------------------
// IntoResponse implementation
// ---------------------------------------------------------------------------

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::InternalError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = Json(json!({
            "error": error_message
        }));

        (status, body).into_response()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_not_found_response() {
        let error = ApiError::NotFound("task not found".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        assert!(body_str.contains("\"error\""));
        assert!(body_str.contains("task not found"));
    }

    #[tokio::test]
    async fn test_bad_request_response() {
        let error = ApiError::BadRequest("invalid input".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        assert!(body_str.contains("\"error\""));
        assert!(body_str.contains("invalid input"));
    }

    #[tokio::test]
    async fn test_internal_error_response() {
        let error = ApiError::InternalError("database connection failed".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        assert!(body_str.contains("\"error\""));
        assert!(body_str.contains("database connection failed"));
    }
}
