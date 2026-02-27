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
        // Return standardized JSON: { "error": "...", "request_id": "..." }
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
        // Return standardized JSON with custom status code
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
        // Return generic error message to hide all implementation details
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
        // Return service unavailable status for temporary outages
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
        .get("x-request-id") // Try to get the x-request-id header
        .and_then(|v| v.to_str().ok()) // Convert HeaderValue to &str, ignoring non-UTF8 values
        .map(|s| s.to_string()) // Convert &str to owned String
        .unwrap_or_else(|| "unknown".to_string()) // Fall back to "unknown" if header is missing or invalid
}

// ---------------------------------------------------------------------------
// Response wrapper type for convenience
// ---------------------------------------------------------------------------

/// Wrapper type that automatically converts to an Axum response.
///
/// This allows returning error tuples directly from handlers without
/// manually calling `.into_response()` on the tuple.
///
/// # Example
///
/// ```rust,ignore
/// fn create_error(request_id: &str) -> SanitizedErrorResponse {
///     SanitizedErrorResponse::new(
///         StatusCode::INTERNAL_SERVER_ERROR,
///         "Operation failed",
///         request_id
///     )
/// }
/// ```
pub struct SanitizedErrorResponse {
    /// HTTP status code for the error response
    status: StatusCode,
    /// JSON body containing the error message and request ID
    body: Json<Value>,
}

impl SanitizedErrorResponse {
    /// Creates a new sanitized error response.
    ///
    /// # Arguments
    ///
    /// * `status` - HTTP status code for the error
    /// * `message` - User-friendly error message (must not contain sensitive details)
    /// * `request_id` - Correlation ID for log tracing
    ///
    /// # Returns
    ///
    /// A `SanitizedErrorResponse` that can be returned from Axum handlers.
    pub fn new(status: StatusCode, message: &str, request_id: &str) -> Self {
        Self {
            status,
            // Create JSON body with standardized error structure:
            // { "error": "<message>", "request_id": "<id>" }
            body: Json(json!({
                "error": message,
                "request_id": request_id,
            })),
        }
    }
}

impl IntoResponse for SanitizedErrorResponse {
    /// Converts the sanitized error into an Axum HTTP response.
    ///
    /// This implementation allows `SanitizedErrorResponse` to be returned
    /// directly from Axum handlers via the `IntoResponse` trait.
    fn into_response(self) -> Response {
        // Convert the (StatusCode, Json) tuple into an Axum response
        (self.status, self.body).into_response()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Unit tests for error response sanitization functions.
///
/// These tests verify that:
/// - Error responses have the correct HTTP status codes
/// - JSON structure matches the expected format (error + request_id)
/// - Request ID extraction handles both present and missing headers
/// - The wrapper type correctly implements IntoResponse
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitized_error() {
        // Test that sanitized_error creates a 500 response with proper JSON structure
        let (status, body) = sanitized_error("Test error", "test-id-123");
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);

        // Verify JSON structure contains both error message and request ID
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
        // Verify fallback behavior when x-request-id header is not present
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
