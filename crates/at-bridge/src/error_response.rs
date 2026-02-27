//! Error response sanitization utilities for API endpoints.
//!
//! This module provides helper functions for creating sanitized error responses
//! that prevent information disclosure while maintaining error correlation via
//! request IDs.
//!
//! # Security Rationale
//!
//! Raw error messages can expose sensitive internal information including:
//! - Filesystem paths and directory structures
//! - OS-level error details and software versions
//! - Environment variable names and configuration presence
//! - Database connection strings and internal service details
//!
//! By using these utilities, API handlers return generic error messages to
//! clients while logging detailed error information internally with correlation
//! IDs for debugging.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};

/// Creates a sanitized error response with a generic message and request ID.
///
/// # Arguments
///
/// * `message` - User-friendly error message (should not contain sensitive details)
/// * `request_id` - Correlation ID from X-Request-Id header for log correlation
///
/// # Returns
///
/// Returns `(StatusCode::INTERNAL_SERVER_ERROR, Json<Value>)` tuple containing:
/// - `error`: The sanitized error message
/// - `request_id`: The correlation ID for debugging
///
/// # Example
///
/// ```rust,ignore
/// // In an API handler:
/// let request_id = extract_request_id(&request);
/// tracing::error!(request_id = %request_id, error = %e, "Failed to spawn terminal");
/// return sanitized_error("Failed to create terminal", &request_id).into_response();
/// ```
pub fn sanitized_error(message: &str, request_id: &str) -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({
            "error": message,
            "request_id": request_id,
        })),
    )
}

/// Creates a sanitized error response with a custom status code.
///
/// # Arguments
///
/// * `status` - HTTP status code to return
/// * `message` - User-friendly error message (should not contain sensitive details)
/// * `request_id` - Correlation ID from X-Request-Id header for log correlation
///
/// # Returns
///
/// Returns `(StatusCode, Json<Value>)` tuple with the specified status code.
///
/// # Example
///
/// ```rust,ignore
/// // For a bad request error:
/// return sanitized_error_with_status(
///     StatusCode::BAD_REQUEST,
///     "Invalid configuration",
///     &request_id
/// ).into_response();
/// ```
pub fn sanitized_error_with_status(
    status: StatusCode,
    message: &str,
    request_id: &str,
) -> (StatusCode, Json<Value>) {
    (
        status,
        Json(json!({
            "error": message,
            "request_id": request_id,
        })),
    )
}

/// Creates a generic 500 Internal Server Error response.
///
/// Use this when you want to hide all error details and only provide
/// a correlation ID for debugging.
///
/// # Arguments
///
/// * `request_id` - Correlation ID from X-Request-Id header for log correlation
///
/// # Returns
///
/// Returns `(StatusCode::INTERNAL_SERVER_ERROR, Json<Value>)` tuple.
///
/// # Example
///
/// ```rust,ignore
/// let request_id = extract_request_id(&request);
/// tracing::error!(request_id = %request_id, error = %e, "Database connection failed");
/// return sanitized_500(&request_id).into_response();
/// ```
pub fn sanitized_500(request_id: &str) -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({
            "error": "Internal server error",
            "request_id": request_id,
        })),
    )
}

/// Creates a 503 Service Unavailable error response with a custom message.
///
/// # Arguments
///
/// * `message` - User-friendly message explaining the unavailability
/// * `request_id` - Correlation ID from X-Request-Id header for log correlation
///
/// # Returns
///
/// Returns `(StatusCode::SERVICE_UNAVAILABLE, Json<Value>)` tuple.
///
/// # Example
///
/// ```rust,ignore
/// return sanitized_503("Service temporarily unavailable", &request_id).into_response();
/// ```
pub fn sanitized_503(message: &str, request_id: &str) -> (StatusCode, Json<Value>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({
            "error": message,
            "request_id": request_id,
        })),
    )
}

/// Extracts the request ID from request headers.
///
/// If the `x-request-id` header is present, returns its value.
/// Otherwise returns "unknown" as a fallback.
///
/// # Arguments
///
/// * `headers` - HTTP headers from the request
///
/// # Returns
///
/// The request ID as a String, or "unknown" if not found.
///
/// # Example
///
/// ```rust,ignore
/// use axum::extract::Request;
///
/// async fn handler(request: Request) -> impl IntoResponse {
///     let request_id = extract_request_id(request.headers());
///     // ... use request_id in error responses
/// }
/// ```
pub fn extract_request_id(headers: &axum::http::HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

// ---------------------------------------------------------------------------
// Response wrapper type for convenience
// ---------------------------------------------------------------------------

/// Wrapper type that automatically converts to an Axum response.
///
/// This allows returning error tuples directly from handlers.
pub struct SanitizedErrorResponse {
    status: StatusCode,
    body: Json<Value>,
}

impl SanitizedErrorResponse {
    /// Creates a new sanitized error response.
    pub fn new(status: StatusCode, message: &str, request_id: &str) -> Self {
        Self {
            status,
            body: Json(json!({
                "error": message,
                "request_id": request_id,
            })),
        }
    }
}

impl IntoResponse for SanitizedErrorResponse {
    fn into_response(self) -> Response {
        (self.status, self.body).into_response()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitized_error() {
        let (status, body) = sanitized_error("Test error", "test-id-123");
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);

        let json_value = body.0;
        assert_eq!(json_value["error"], "Test error");
        assert_eq!(json_value["request_id"], "test-id-123");
    }

    #[test]
    fn test_sanitized_error_with_status() {
        let (status, body) =
            sanitized_error_with_status(StatusCode::BAD_REQUEST, "Bad input", "req-456");
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let json_value = body.0;
        assert_eq!(json_value["error"], "Bad input");
        assert_eq!(json_value["request_id"], "req-456");
    }

    #[test]
    fn test_sanitized_500() {
        let (status, body) = sanitized_500("req-789");
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);

        let json_value = body.0;
        assert_eq!(json_value["error"], "Internal server error");
        assert_eq!(json_value["request_id"], "req-789");
    }

    #[test]
    fn test_sanitized_503() {
        let (status, body) = sanitized_503("Service down", "req-abc");
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);

        let json_value = body.0;
        assert_eq!(json_value["error"], "Service down");
        assert_eq!(json_value["request_id"], "req-abc");
    }

    #[test]
    fn test_extract_request_id_present() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            "x-request-id",
            axum::http::HeaderValue::from_static("test-123"),
        );

        let request_id = extract_request_id(&headers);
        assert_eq!(request_id, "test-123");
    }

    #[test]
    fn test_extract_request_id_missing() {
        let headers = axum::http::HeaderMap::new();
        let request_id = extract_request_id(&headers);
        assert_eq!(request_id, "unknown");
    }

    #[test]
    fn test_sanitized_error_response_type() {
        let response = SanitizedErrorResponse::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Test message",
            "req-xyz",
        );

        // Test that it can convert to a response
        let _axum_response = response.into_response();
    }
}
