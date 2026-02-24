//! HTTP API error types.
//!
//! Provides a unified `ApiError` enum for consistent error responses across
//! the HTTP API layer. Implements Axum's `IntoResponse` trait to automatically
//! convert errors into appropriate HTTP responses.

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
