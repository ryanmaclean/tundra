use axum::{
    body::Body,
    extract::Request,
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

/// Generate an OpenTelemetry-compatible trace ID (32 hex characters).
pub fn generate_trace_id() -> String {
    let id = Uuid::new_v4();
    // OTel trace IDs are 32 hex chars (128 bits) -- a UUID without hyphens is
    // exactly 32 hex chars.
    id.as_simple().to_string()
}

/// Generate a span ID (16 hex characters).
pub fn generate_span_id() -> String {
    let id = Uuid::new_v4();
    // Take the first 16 hex chars (64 bits) for span IDs.
    id.as_simple().to_string()[..16].to_string()
}

/// Axum middleware that injects `X-Request-Id` headers and creates a tracing
/// span for each request.
///
/// If the incoming request already has an `X-Request-Id` header, that value is
/// reused. Otherwise a new trace ID is generated.
///
/// The response always includes the `X-Request-Id` header for correlation.
pub async fn request_id_middleware(mut request: Request<Body>, next: Next) -> Response {
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(generate_trace_id);

    // Insert/overwrite so downstream handlers can read it
    request.headers_mut().insert(
        "x-request-id",
        request_id.parse().unwrap_or_else(|_| {
            axum::http::HeaderValue::from_static("unknown")
        }),
    );

    let method = request.method().to_string();
    let path = request.uri().path().to_string();

    let span = tracing::info_span!(
        "http_request",
        trace_id = %request_id,
        method = %method,
        path = %path,
    );

    let _guard = span.enter();
    tracing::debug!(trace_id = %request_id, "processing request");

    let mut response = next.run(request).await;

    // Attach the request ID to the response
    if let Ok(val) = request_id.parse() {
        response.headers_mut().insert("x-request-id", val);
    }

    response
}

/// Create a named span for a common operation, returning the span and its
/// trace ID for logging correlation.
pub fn create_operation_span(operation: &str) -> (tracing::Span, String) {
    let trace_id = generate_trace_id();
    let span_id = generate_span_id();
    let span = tracing::info_span!(
        "operation",
        trace_id = %trace_id,
        span_id = %span_id,
        operation = %operation,
    );
    (span, trace_id)
}

/// Create a child span under an existing trace ID.
pub fn create_child_span(trace_id: &str, operation: &str) -> tracing::Span {
    let span_id = generate_span_id();
    tracing::info_span!(
        "operation",
        trace_id = %trace_id,
        span_id = %span_id,
        operation = %operation,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_id_format() {
        let id = generate_trace_id();
        assert_eq!(id.len(), 32);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_span_id_format() {
        let id = generate_span_id();
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_operation_span_creation() {
        let (span, trace_id) = create_operation_span("test_op");
        assert_eq!(trace_id.len(), 32);
        // Span should be valid
        let _guard = span.enter();
    }

    #[test]
    fn test_child_span_creation() {
        let trace_id = generate_trace_id();
        let span = create_child_span(&trace_id, "child_op");
        let _guard = span.enter();
    }
}
