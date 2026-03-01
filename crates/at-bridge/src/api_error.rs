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
/// of the form `{"error": "<message>"}`. This enum implements `IntoResponse`
/// for seamless integration with Axum handlers.
///
/// # Examples
///
/// ```rust,ignore
/// use axum::{Json, extract::Path};
/// use uuid::Uuid;
/// use at_bridge::api_error::ApiError;
///
/// async fn get_widget(Path(id): Path<Uuid>) -> Result<Json<Widget>, ApiError> {
///     let widget = find_widget(id)
///         .ok_or_else(|| ApiError::NotFound("widget not found".into()))?;
///     Ok(Json(widget))
/// }
///
/// async fn create_widget(Json(data): Json<WidgetData>) -> Result<Json<Widget>, ApiError> {
///     if data.name.is_empty() {
///         return Err(ApiError::BadRequest("name is required".into()));
///     }
///     // ...
/// }
/// ```
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// HTTP 404 Not Found - The requested resource does not exist.
    ///
    /// This occurs when:
    /// - A resource ID doesn't match any existing entity
    /// - An endpoint path doesn't exist (though Axum typically handles this)
    /// - A query returns no results when one was expected
    ///
    /// The contained string should identify what resource was not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// HTTP 400 Bad Request - The request is malformed or invalid.
    ///
    /// This occurs when:
    /// - Required fields are missing from the request body
    /// - Field values fail validation (out of range, wrong format, etc.)
    /// - Query parameters are invalid
    /// - The request cannot be processed due to client error
    ///
    /// The contained string should explain what's wrong with the request.
    #[error("bad request: {0}")]
    BadRequest(String),

    /// HTTP 401 Unauthorized - Authentication is required or has failed.
    ///
    /// This occurs when:
    /// - No authentication credentials are provided
    /// - The provided credentials are invalid
    /// - The authentication token has expired
    /// - The authentication method is not supported
    ///
    /// The contained string should explain the authentication issue without
    /// leaking sensitive security details.
    #[error("unauthorized: {0}")]
    Unauthorized(String),

    /// HTTP 503 Service Unavailable - The service cannot handle the request.
    ///
    /// This occurs when:
    /// - A required backend service is down
    /// - The database is unreachable
    /// - The system is overloaded or in maintenance mode
    /// - A temporary condition prevents request processing
    ///
    /// The contained string should explain what service is unavailable.
    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),

    /// HTTP 500 Internal Server Error - An unexpected error occurred.
    ///
    /// This occurs when:
    /// - An unhandled exception happens during request processing
    /// - A programming error is encountered (assertion failure, panic)
    /// - An unexpected state is reached
    /// - Database or I/O operations fail unexpectedly
    ///
    /// The contained string should provide error details while being careful
    /// not to leak sensitive implementation details to clients.
    #[error("internal error: {0}")]
    Internal(String),

    /// HTTP 409 Conflict - The request conflicts with current resource state.
    ///
    /// This occurs when:
    /// - Creating a resource that already exists (duplicate ID/name)
    /// - Concurrent modification conflicts (optimistic locking failure)
    /// - The operation violates a uniqueness constraint
    /// - State transitions are invalid (e.g., starting an already-started task)
    ///
    /// The contained string should explain what conflict occurred.
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
        let (status, body) = error_response(ApiError::Unauthorized("invalid token".into())).await;
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
        let (status, body) = error_response(ApiError::Internal("unexpected failure".into())).await;
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
            assert!(body["error"].is_string(), "'error' field must be a string");
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
