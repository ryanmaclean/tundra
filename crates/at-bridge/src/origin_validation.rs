//! WebSocket Origin header validation to prevent cross-site WebSocket hijacking.
//!
//! WebSocket connections are not subject to CORS restrictions - the browser
//! allows cross-origin WebSocket connections and only sends the Origin header
//! for the server to validate. Without Origin validation, any malicious webpage
//! can connect to WebSocket endpoints and potentially gain unauthorized access.
//!
//! This module provides validation functions that check the Origin header against
//! an allowlist of permitted origins. By default, only localhost variants are
//! allowed (http://localhost:*, http://127.0.0.1:*, http://[::1]:*).

use axum::http::{HeaderMap, StatusCode};

/// Default allowed origins for WebSocket connections (localhost variants only).
pub const DEFAULT_ALLOWED_ORIGINS: &[&str] = &[
    "http://localhost",
    "https://localhost",
    "http://127.0.0.1",
    "https://127.0.0.1",
    "http://[::1]",
    "https://[::1]",
];

/// Validates the Origin header of a WebSocket upgrade request against an allowlist.
///
/// # Arguments
///
/// * `headers` - The HTTP headers from the upgrade request
/// * `allowed_origins` - List of permitted origin patterns
///
/// # Returns
///
/// * `Ok(())` if the Origin header is present and matches an allowed origin
/// * `Err(StatusCode::FORBIDDEN)` if the Origin is missing, invalid, or not in the allowlist
///
/// # Security
///
/// This function performs the following checks:
/// 1. Verifies that the Origin header is present
/// 2. Validates that the Origin header value is valid UTF-8
/// 3. Checks if the Origin matches any allowed origin (exact match or prefix match with port)
///
/// # Examples
///
/// ```ignore
/// use axum::http::HeaderMap;
/// use at_bridge::origin_validation::{validate_websocket_origin, DEFAULT_ALLOWED_ORIGINS};
///
/// let mut headers = HeaderMap::new();
/// headers.insert("origin", "http://localhost:3000".parse().unwrap());
///
/// let allowed: Vec<String> = DEFAULT_ALLOWED_ORIGINS.iter().map(|s| s.to_string()).collect();
/// assert!(validate_websocket_origin(&headers, &allowed).is_ok());
/// ```
pub fn validate_websocket_origin(
    headers: &HeaderMap,
    allowed_origins: &[String],
) -> Result<(), StatusCode> {
    // Extract the Origin header
    let origin = headers
        .get("origin")
        .ok_or(StatusCode::FORBIDDEN)?
        .to_str()
        .map_err(|_| StatusCode::FORBIDDEN)?;

    // Check if the origin matches any allowed origin
    let is_allowed = allowed_origins.iter().any(|allowed| {
        // Exact match
        if origin == allowed {
            return true;
        }

        // Prefix match with port (e.g., "http://localhost:3000" matches "http://localhost")
        if origin.starts_with(allowed) {
            let remainder = &origin[allowed.len()..];
            // Check if the remainder is a port (starts with ':' followed by digits)
            if remainder.starts_with(':') {
                return remainder[1..].chars().all(|c| c.is_ascii_digit());
            }
        }

        false
    });

    if is_allowed {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

/// Helper function to get default allowed origins as a Vec<String>.
///
/// Returns a vector of default localhost origin patterns that are safe
/// to allow for local development and production use.
pub fn get_default_allowed_origins() -> Vec<String> {
    DEFAULT_ALLOWED_ORIGINS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    fn allowed_origins() -> Vec<String> {
        get_default_allowed_origins()
    }

    #[test]
    fn test_valid_localhost_http() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "http://localhost".parse().unwrap());
        assert!(validate_websocket_origin(&headers, &allowed_origins()).is_ok());
    }

    #[test]
    fn test_valid_localhost_https() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "https://localhost".parse().unwrap());
        assert!(validate_websocket_origin(&headers, &allowed_origins()).is_ok());
    }

    #[test]
    fn test_valid_localhost_with_port() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "http://localhost:3000".parse().unwrap());
        assert!(validate_websocket_origin(&headers, &allowed_origins()).is_ok());
    }

    #[test]
    fn test_valid_127_0_0_1() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "http://127.0.0.1".parse().unwrap());
        assert!(validate_websocket_origin(&headers, &allowed_origins()).is_ok());
    }

    #[test]
    fn test_valid_127_0_0_1_with_port() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "http://127.0.0.1:8080".parse().unwrap());
        assert!(validate_websocket_origin(&headers, &allowed_origins()).is_ok());
    }

    #[test]
    fn test_valid_ipv6_localhost() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "http://[::1]".parse().unwrap());
        assert!(validate_websocket_origin(&headers, &allowed_origins()).is_ok());
    }

    #[test]
    fn test_valid_ipv6_localhost_with_port() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "http://[::1]:9000".parse().unwrap());
        assert!(validate_websocket_origin(&headers, &allowed_origins()).is_ok());
    }

    #[test]
    fn test_invalid_external_domain() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "http://evil.com".parse().unwrap());
        let result = validate_websocket_origin(&headers, &allowed_origins());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_invalid_external_domain_with_localhost_in_path() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "http://evil.com/localhost".parse().unwrap());
        let result = validate_websocket_origin(&headers, &allowed_origins());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_missing_origin_header() {
        let headers = HeaderMap::new();
        let result = validate_websocket_origin(&headers, &allowed_origins());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_invalid_origin_subdomain() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "http://fake.localhost.evil.com".parse().unwrap());
        let result = validate_websocket_origin(&headers, &allowed_origins());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_custom_allowed_origins() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "https://example.com".parse().unwrap());

        let custom_origins = vec!["https://example.com".to_string()];
        assert!(validate_websocket_origin(&headers, &custom_origins).is_ok());
    }

    #[test]
    fn test_custom_allowed_origins_with_port() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "https://example.com:8443".parse().unwrap());

        let custom_origins = vec!["https://example.com".to_string()];
        assert!(validate_websocket_origin(&headers, &custom_origins).is_ok());
    }

    #[test]
    fn test_port_with_non_numeric_suffix_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "http://localhost:abc".parse().unwrap());
        let result = validate_websocket_origin(&headers, &allowed_origins());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::FORBIDDEN);
    }
}
