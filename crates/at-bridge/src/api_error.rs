//! Unified API error type with consistent JSON responses.
//!
//! `ApiError` replaces ad-hoc `(StatusCode, Json)` error tuples with a single
//! enum that implements `IntoResponse`. Handlers can return
//! `Result<impl IntoResponse, ApiError>` for cleaner error handling while
//! remaining fully compatible with existing `impl IntoResponse` signatures.
//!
//! # Example
//! ```ignore
//! async fn get_widget(Path(id): Path<Uuid>) -> Result<Json<Widget>, ApiError> {
//!     let widget = find_widget(id).ok_or_else(|| ApiError::NotFound("widget not found".into()))?;
//!     Ok(Json(widget))
//! }
//! ```

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Unified error type for HTTP API handlers.
///
/// Each variant maps to a specific HTTP status code and produces a JSON body
/// of the form `{"error": "<message>"}`.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// 404 Not Found
    #[error("not found: {0}")]
    NotFound(String),

    /// 400 Bad Request
    #[error("bad request: {0}")]
    BadRequest(String),

    /// 401 Unauthorized
    #[error("unauthorized: {0}")]
    Unauthorized(String),

    /// 503 Service Unavailable
    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),

    /// 500 Internal Server Error
    #[error("internal error: {0}")]
    Internal(String),

    /// 409 Conflict
    #[error("conflict: {0}")]
    Conflict(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            ApiError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            ApiError::ServiceUnavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg.clone()),
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            ApiError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
        };
        (status, Json(json!({"error": message}))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    /// Helper to extract status code and JSON body from an ApiError response.
    async fn error_response(err: ApiError) -> (StatusCode, serde_json::Value) {
        let response = err.into_response();
        let status = response.status();
        let body_bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        (status, body)
    }

    #[tokio::test]
    async fn not_found_returns_404() {
        let (status, body) = error_response(ApiError::NotFound("task not found".into())).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"], "task not found");
    }

    #[tokio::test]
    async fn bad_request_returns_400() {
        let (status, body) = error_response(ApiError::BadRequest("missing field".into())).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "missing field");
    }

    #[tokio::test]
    async fn unauthorized_returns_401() {
        let (status, body) =
            error_response(ApiError::Unauthorized("invalid token".into())).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"], "invalid token");
    }

    #[tokio::test]
    async fn service_unavailable_returns_503() {
        let (status, body) =
            error_response(ApiError::ServiceUnavailable("database offline".into())).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["error"], "database offline");
    }

    #[tokio::test]
    async fn internal_returns_500() {
        let (status, body) =
            error_response(ApiError::Internal("unexpected failure".into())).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["error"], "unexpected failure");
    }

    #[tokio::test]
    async fn conflict_returns_409() {
        let (status, body) =
            error_response(ApiError::Conflict("resource already exists".into())).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"], "resource already exists");
    }

    #[tokio::test]
    async fn error_body_always_has_error_field() {
        // Verify every variant produces a JSON body with an "error" key.
        let variants: Vec<ApiError> = vec![
            ApiError::NotFound("a".into()),
            ApiError::BadRequest("b".into()),
            ApiError::Unauthorized("c".into()),
            ApiError::ServiceUnavailable("d".into()),
            ApiError::Internal("e".into()),
            ApiError::Conflict("f".into()),
        ];
        for variant in variants {
            let (_, body) = error_response(variant).await;
            assert!(
                body.get("error").is_some(),
                "response body must contain 'error' field"
            );
            assert!(
                body["error"].is_string(),
                "'error' field must be a string"
            );
        }
    }

    #[test]
    fn display_impl_matches_error_message() {
        let err = ApiError::NotFound("widget not found".into());
        assert_eq!(err.to_string(), "not found: widget not found");

        let err = ApiError::BadRequest("invalid input".into());
        assert_eq!(err.to_string(), "bad request: invalid input");
    }
}
