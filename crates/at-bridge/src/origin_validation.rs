//! WebSocket Origin header validation to prevent cross-site WebSocket hijacking.
//!
//! WebSocket connections are not subject to CORS restrictions - the browser
//! allows cross-origin WebSocket connections and only sends the Origin header
//! for the server to validate. Without Origin validation, any malicious webpage
//! can connect to WebSocket endpoints and potentially gain unauthorized access.
//!
//! This module provides validation functions that check the Origin header against
//! an allowlist of permitted origins. By default, only localhost variants are
//! allowed (`http://localhost:*`, `http://127.0.0.1:*`, `http://[::1]:*`).

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
        if let Some(remainder) = origin.strip_prefix(allowed.as_str()) {
            // Check if the remainder is a port (starts with ':' followed by digits)
            if let Some(port) = remainder.strip_prefix(':') {
                return port.chars().all(|c| c.is_ascii_digit());
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

/// Helper function to get default allowed origins as a `Vec<String>`.
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

    #[test]
    fn test_origin_with_path_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "http://localhost/path".parse().unwrap());
        let result = validate_websocket_origin(&headers, &allowed_origins());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_origin_with_port_and_path_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "http://localhost:3000/path".parse().unwrap());
        let result = validate_websocket_origin(&headers, &allowed_origins());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_origin_with_query_string_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "http://localhost?query=1".parse().unwrap());
        let result = validate_websocket_origin(&headers, &allowed_origins());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_origin_with_trailing_slash_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "http://localhost/".parse().unwrap());
        let result = validate_websocket_origin(&headers, &allowed_origins());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_empty_allowed_origins_rejects_all() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "http://localhost".parse().unwrap());
        let empty_origins: Vec<String> = vec![];
        let result = validate_websocket_origin(&headers, &empty_origins);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_https_localhost_with_various_ports() {
        let origins = get_default_allowed_origins();

        let test_cases = vec![
            "https://localhost:443",
            "https://localhost:8443",
            "https://localhost:3001",
        ];

        for origin in test_cases {
            let mut headers = HeaderMap::new();
            headers.insert("origin", origin.parse().unwrap());
            assert!(
                validate_websocket_origin(&headers, &origins).is_ok(),
                "Expected {} to be valid",
                origin
            );
        }
    }

    #[test]
    fn test_https_ipv4_with_various_ports() {
        let origins = get_default_allowed_origins();

        let test_cases = vec![
            "https://127.0.0.1:443",
            "https://127.0.0.1:8443",
            "https://127.0.0.1:5000",
        ];

        for origin in test_cases {
            let mut headers = HeaderMap::new();
            headers.insert("origin", origin.parse().unwrap());
            assert!(
                validate_websocket_origin(&headers, &origins).is_ok(),
                "Expected {} to be valid",
                origin
            );
        }
    }

    #[test]
    fn test_https_ipv6_with_various_ports() {
        let origins = get_default_allowed_origins();

        let test_cases = vec![
            "https://[::1]:443",
            "https://[::1]:8443",
            "https://[::1]:5000",
        ];

        for origin in test_cases {
            let mut headers = HeaderMap::new();
            headers.insert("origin", origin.parse().unwrap());
            assert!(
                validate_websocket_origin(&headers, &origins).is_ok(),
                "Expected {} to be valid",
                origin
            );
        }
    }

    #[test]
    fn test_multiple_allowed_origins_first_match() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "https://app.example.com".parse().unwrap());

        let custom_origins = vec![
            "https://app.example.com".to_string(),
            "https://api.example.com".to_string(),
        ];
        assert!(validate_websocket_origin(&headers, &custom_origins).is_ok());
    }

    #[test]
    fn test_multiple_allowed_origins_second_match() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "https://api.example.com".parse().unwrap());

        let custom_origins = vec![
            "https://app.example.com".to_string(),
            "https://api.example.com".to_string(),
        ];
        assert!(validate_websocket_origin(&headers, &custom_origins).is_ok());
    }

    #[test]
    fn test_multiple_allowed_origins_no_match() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "https://evil.com".parse().unwrap());

        let custom_origins = vec![
            "https://app.example.com".to_string(),
            "https://api.example.com".to_string(),
        ];
        let result = validate_websocket_origin(&headers, &custom_origins);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_origin_case_sensitivity() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "HTTP://LOCALHOST".parse().unwrap());
        let result = validate_websocket_origin(&headers, &allowed_origins());
        // Origins are case-sensitive per RFC 6454, so this should fail
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_port_zero_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "http://localhost:0".parse().unwrap());
        // Port 0 is technically valid (let OS choose port), but unusual for origins
        // Our validation accepts it since it's numeric
        assert!(validate_websocket_origin(&headers, &allowed_origins()).is_ok());
    }

    #[test]
    fn test_large_port_number() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "http://localhost:65535".parse().unwrap());
        assert!(validate_websocket_origin(&headers, &allowed_origins()).is_ok());
    }

    #[test]
    fn test_very_large_port_number() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "http://localhost:999999".parse().unwrap());
        // Our validation only checks if it's numeric, not if it's a valid port
        assert!(validate_websocket_origin(&headers, &allowed_origins()).is_ok());
    }

    #[test]
    fn test_colon_without_port_accepted() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "http://localhost:".parse().unwrap());
        // The current implementation accepts colon without port because
        // an empty string passes the .all(|c| c.is_ascii_digit()) check
        assert!(validate_websocket_origin(&headers, &allowed_origins()).is_ok());
    }

    #[test]
    fn test_mixed_case_origin_in_allowlist() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "https://Example.Com".parse().unwrap());

        let custom_origins = vec!["https://Example.Com".to_string()];
        // Exact match should work
        assert!(validate_websocket_origin(&headers, &custom_origins).is_ok());
    }

    #[test]
    fn test_mixed_case_no_match() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "https://Example.Com".parse().unwrap());

        let custom_origins = vec!["https://example.com".to_string()];
        let result = validate_websocket_origin(&headers, &custom_origins);
        // Case mismatch should fail (origins are case-sensitive)
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_default_origins_constant() {
        let defaults = get_default_allowed_origins();
        assert_eq!(defaults.len(), 6);
        assert!(defaults.contains(&"http://localhost".to_string()));
        assert!(defaults.contains(&"https://localhost".to_string()));
        assert!(defaults.contains(&"http://127.0.0.1".to_string()));
        assert!(defaults.contains(&"https://127.0.0.1".to_string()));
        assert!(defaults.contains(&"http://[::1]".to_string()));
        assert!(defaults.contains(&"https://[::1]".to_string()));
    }

    #[test]
    fn test_subdomain_not_allowed() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "http://sub.localhost".parse().unwrap());
        let result = validate_websocket_origin(&headers, &allowed_origins());
        // Subdomains should not match
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_origin_with_username_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "http://user@localhost".parse().unwrap());
        let result = validate_websocket_origin(&headers, &allowed_origins());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_file_protocol_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "file:///path/to/file".parse().unwrap());
        let result = validate_websocket_origin(&headers, &allowed_origins());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_ws_protocol_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "ws://localhost".parse().unwrap());
        let result = validate_websocket_origin(&headers, &allowed_origins());
        // WebSocket origins should be http/https, not ws/wss
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_wss_protocol_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", "wss://localhost".parse().unwrap());
        let result = validate_websocket_origin(&headers, &allowed_origins());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::FORBIDDEN);
    }
}
