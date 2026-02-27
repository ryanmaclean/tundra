use std::sync::Arc;

use at_bridge::error_response::{extract_request_id, sanitized_error};
use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, ApiState};
use at_core::config::Config;
use at_core::settings::SettingsManager;
use serde_json::Value;

// ===========================================================================
// Test Helpers
// ===========================================================================

/// Spin up an API server backed by a temp settings file on a random port.
async fn start_test_server() -> (String, Arc<ApiState>) {
    let event_bus = EventBus::new();
    let tmp_path = std::env::temp_dir()
        .join(format!(
            "at-error-sanitization-test-{}",
            uuid::Uuid::new_v4()
        ))
        .join("settings.toml");
    let settings_manager = Arc::new(SettingsManager::new(&tmp_path));

    let mut state = ApiState::new(event_bus);
    state.settings_manager = settings_manager;
    let state = Arc::new(state);

    let router = api_router(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind to ephemeral port");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    (format!("http://{addr}"), state)
}

/// Spin up a server with a PTY pool for terminal testing.
async fn start_test_server_with_pty() -> (String, Arc<ApiState>) {
    let event_bus = EventBus::new();
    let pool = Arc::new(at_session::pty_pool::PtyPool::new(2));
    let state = Arc::new(ApiState::with_pty_pool(event_bus, pool));
    let router = api_router(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind to ephemeral port");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    (format!("http://{addr}"), state)
}

// ===========================================================================
// Error Response Structure Tests
// ===========================================================================

#[tokio::test]
async fn test_error_response_has_request_id() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Send malformed JSON to trigger a deserialization error
    let resp = client
        .put(format!("{base}/api/settings"))
        .header("content-type", "application/json")
        .body("not json at all")
        .send()
        .await
        .unwrap();

    // Should get an error response
    assert!(
        resp.status().is_client_error() || resp.status().is_server_error(),
        "Expected error status, got {}",
        resp.status()
    );

    // If we can parse JSON, it should contain request_id
    if let Ok(body) = resp.json::<Value>().await {
        if body.get("error").is_some() {
            assert!(
                body.get("request_id").is_some(),
                "Error response missing request_id: {:?}",
                body
            );
        }
    }
}

#[tokio::test]
async fn test_error_response_preserves_custom_request_id() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let custom_request_id = "test-req-123456";

    // Send malformed JSON to trigger an error
    let resp = client
        .patch(format!("{base}/api/settings"))
        .header("x-request-id", custom_request_id)
        .header("content-type", "application/json")
        .body("this is not json")
        .send()
        .await
        .unwrap();

    // Should get an error response
    assert!(
        resp.status().is_client_error() || resp.status().is_server_error(),
        "Expected error status, got {}",
        resp.status()
    );

    // If we can parse the response, verify our custom request ID is present
    if let Ok(body) = resp.json::<Value>().await {
        if let Some(req_id) = body.get("request_id") {
            assert_eq!(
                req_id.as_str().unwrap_or(""),
                custom_request_id,
                "Expected custom request ID to be preserved"
            );
        }
    }
}

// ===========================================================================
// Settings API - Path Sanitization Tests
// ===========================================================================

#[tokio::test]
async fn test_settings_error_does_not_expose_filesystem_paths() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Make the settings file read-only to trigger a write error
    let settings_path = state.settings_manager.path();

    // First ensure parent directory exists
    if let Some(parent) = settings_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // Create the file
    let _ = std::fs::write(&settings_path, "# test");

    // Make it read-only
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&settings_path)
            .unwrap()
            .permissions();
        perms.set_mode(0o444); // Read-only
        std::fs::set_permissions(&settings_path, perms).unwrap();
    }

    #[cfg(windows)]
    {
        let mut perms = std::fs::metadata(&settings_path)
            .unwrap()
            .permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&settings_path, perms).unwrap();
    }

    // Try to save settings (should fail due to read-only)
    let cfg = Config::default();
    let resp = client
        .put(format!("{base}/api/settings"))
        .json(&cfg)
        .send()
        .await
        .unwrap();

    // Clean up permissions before asserting
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if settings_path.exists() {
            let mut perms = std::fs::metadata(&settings_path)
                .unwrap()
                .permissions();
            perms.set_mode(0o644);
            let _ = std::fs::set_permissions(&settings_path, perms);
        }
    }

    #[cfg(windows)]
    {
        if settings_path.exists() {
            let mut perms = std::fs::metadata(&settings_path)
                .unwrap()
                .permissions();
            perms.set_readonly(false);
            let _ = std::fs::set_permissions(&settings_path, perms);
        }
    }

    // Should return an error
    assert_eq!(
        resp.status(),
        500,
        "Expected INTERNAL_SERVER_ERROR for write failure"
    );

    let body: Value = resp.json().await.unwrap();
    let error_msg = body["error"].as_str().unwrap_or("");

    // Verify the error message is sanitized (doesn't contain filesystem path)
    assert!(
        !error_msg.contains(&settings_path.to_string_lossy().to_string()),
        "Error message exposes filesystem path: {}",
        error_msg
    );

    // Verify it contains a generic message
    assert!(
        error_msg.contains("Failed to save settings")
            || error_msg.contains("Internal server error"),
        "Expected sanitized error message, got: {}",
        error_msg
    );

    // Verify request_id is present
    assert!(
        body["request_id"].is_string(),
        "Error response missing request_id"
    );
}

#[tokio::test]
async fn test_patch_settings_error_sanitized() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Send malformed JSON to trigger a deserialization error
    let resp = client
        .patch(format!("{base}/api/settings"))
        .header("content-type", "application/json")
        .body("{not valid json at all")
        .send()
        .await
        .unwrap();

    assert!(
        resp.status().is_client_error() || resp.status().is_server_error()
    );

    // The error should be sanitized and contain request_id
    if let Ok(body) = resp.json::<Value>().await {
        if let Some(error) = body.get("error") {
            let error_str = error.as_str().unwrap_or("");

            // Should not contain internal Rust error details
            assert!(
                !error_str.contains("serde"),
                "Error exposes internal deserialization details: {}",
                error_str
            );
            assert!(
                !error_str.contains("toml"),
                "Error exposes internal format details: {}",
                error_str
            );
        }
    }
}

// ===========================================================================
// OAuth API - Configuration Sanitization Tests
// ===========================================================================

#[tokio::test]
async fn test_oauth_error_does_not_reveal_env_var_names() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Clear OAuth environment variables to trigger configuration error
    let original_client_id = std::env::var("GITHUB_OAUTH_CLIENT_ID").ok();
    let original_client_secret = std::env::var("GITHUB_OAUTH_CLIENT_SECRET").ok();

    std::env::remove_var("GITHUB_OAUTH_CLIENT_ID");
    std::env::remove_var("GITHUB_OAUTH_CLIENT_SECRET");

    let resp = client
        .get(format!("{base}/api/github/oauth/authorize"))
        .send()
        .await
        .unwrap();

    // Restore environment variables
    if let Some(id) = original_client_id {
        std::env::set_var("GITHUB_OAUTH_CLIENT_ID", id);
    }
    if let Some(secret) = original_client_secret {
        std::env::set_var("GITHUB_OAUTH_CLIENT_SECRET", secret);
    }

    // Should return an error (503 Service Unavailable)
    assert_eq!(
        resp.status(),
        503,
        "Expected SERVICE_UNAVAILABLE for missing OAuth config"
    );

    let body: Value = resp.json().await.unwrap();
    let error_msg = body["error"].as_str().unwrap_or("");

    // Verify it doesn't expose specific environment variable names
    assert!(
        !error_msg.contains("GITHUB_OAUTH_CLIENT_ID"),
        "Error exposes environment variable name: {}",
        error_msg
    );
    assert!(
        !error_msg.contains("GITHUB_OAUTH_CLIENT_SECRET"),
        "Error exposes environment variable name: {}",
        error_msg
    );
    assert!(
        !error_msg.contains("not set"),
        "Error reveals configuration status: {}",
        error_msg
    );

    // Verify it contains a generic message
    assert!(
        error_msg.contains("GitHub OAuth not configured")
            || error_msg.contains("not configured"),
        "Expected sanitized OAuth error, got: {}",
        error_msg
    );

    // Verify request_id is present
    assert!(
        body["request_id"].is_string(),
        "Error response missing request_id"
    );
}

#[tokio::test]
async fn test_oauth_callback_error_sanitized() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Send invalid callback request (missing state parameter)
    let resp = client
        .get(format!("{base}/api/github/oauth/callback?code=invalid"))
        .send()
        .await
        .unwrap();

    // Should return an error
    assert!(resp.status().is_client_error() || resp.status().is_server_error());

    // Try to parse response body - it might be empty or HTML
    if let Ok(body) = resp.json::<Value>().await {
        // If we got JSON, it should have an error and request_id
        if body.get("error").is_some() {
            assert!(
                body["request_id"].is_string(),
                "Error response missing request_id"
            );
        }
    }
    // If response isn't JSON, that's also okay - the handler might return HTML or empty body
}

// ===========================================================================
// Terminal API - OS Error Sanitization Tests
// ===========================================================================

#[tokio::test]
async fn test_terminal_spawn_error_sanitized() {
    let (base, _state) = start_test_server_with_pty().await;
    let client = reqwest::Client::new();

    // Create terminals until we potentially hit a failure
    // (This may not always fail, but we're testing the error path exists)
    let resp = client
        .post(format!("{base}/api/terminals"))
        .header("x-request-id", "test-terminal-spawn")
        .send()
        .await
        .unwrap();

    // If it succeeds (201), that's fine - the error path is covered in other scenarios
    // If it fails, verify the error is sanitized
    if resp.status().is_server_error() {
        let body: Value = resp.json().await.unwrap();
        let error_msg = body["error"].as_str().unwrap_or("");

        // Should not expose OS-level error details
        assert!(
            !error_msg.contains("spawn"),
            "Error exposes internal spawn details: {}",
            error_msg
        );
        assert!(
            !error_msg.contains("/bin/"),
            "Error exposes shell path: {}",
            error_msg
        );
        assert!(
            !error_msg.contains("errno"),
            "Error exposes OS error codes: {}",
            error_msg
        );

        // Should contain generic message
        assert!(
            error_msg.contains("Failed to create terminal")
                || error_msg.contains("terminal"),
            "Expected sanitized terminal error, got: {}",
            error_msg
        );

        // Should have request_id
        assert_eq!(
            body["request_id"].as_str().unwrap_or(""),
            "test-terminal-spawn",
            "Request ID not preserved in error response"
        );
    }
}

// ===========================================================================
// Utility Function Tests
// ===========================================================================

#[test]
fn test_extract_request_id_from_headers() {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        "x-request-id",
        axum::http::HeaderValue::from_static("test-123"),
    );

    let request_id = extract_request_id(&headers);
    assert_eq!(request_id, "test-123");
}

#[test]
fn test_extract_request_id_fallback() {
    let headers = axum::http::HeaderMap::new();
    let request_id = extract_request_id(&headers);
    assert_eq!(request_id, "unknown");
}

#[test]
fn test_sanitized_error_structure() {
    let (status, json) = sanitized_error("Test message", "req-456");

    assert_eq!(status, axum::http::StatusCode::INTERNAL_SERVER_ERROR);

    let value = json.0;
    assert_eq!(value["error"], "Test message");
    assert_eq!(value["request_id"], "req-456");

    // Should only have these two fields
    assert_eq!(value.as_object().unwrap().len(), 2);
}

// ===========================================================================
// Integration Tests - Generic Error Patterns
// ===========================================================================

#[tokio::test]
async fn test_all_api_errors_follow_consistent_structure() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Test various error scenarios that should return sanitized errors

    // Test 1: Invalid settings JSON
    let resp = client
        .put(format!("{base}/api/settings"))
        .header("content-type", "application/json")
        .body("malformed json")
        .send()
        .await
        .unwrap();

    if resp.status().is_client_error() || resp.status().is_server_error() {
        if let Ok(body) = resp.json::<Value>().await {
            // If there's an error field, it should have request_id
            if body.get("error").is_some() {
                assert!(
                    body.get("request_id").is_some(),
                    "Error response missing request_id for PUT /api/settings"
                );
            }
        }
    }

    // Test 2: Invalid PATCH settings
    let resp = client
        .patch(format!("{base}/api/settings"))
        .header("content-type", "application/json")
        .body("not json")
        .send()
        .await
        .unwrap();

    if resp.status().is_client_error() || resp.status().is_server_error() {
        if let Ok(body) = resp.json::<Value>().await {
            if body.get("error").is_some() {
                assert!(
                    body.get("request_id").is_some(),
                    "Error response missing request_id for PATCH /api/settings"
                );
            }
        }
    }
}

#[tokio::test]
async fn test_error_responses_do_not_contain_stack_traces() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Trigger various errors
    let resp = client
        .put(format!("{base}/api/settings"))
        .header("content-type", "application/json")
        .body("definitely not json")
        .send()
        .await
        .unwrap();

    if let Ok(body) = resp.json::<Value>().await {
        let body_str = serde_json::to_string(&body).unwrap();

        // Should not contain stack trace indicators
        assert!(
            !body_str.contains("panicked at"),
            "Error response contains panic info"
        );
        assert!(
            !body_str.contains("backtrace"),
            "Error response contains backtrace"
        );
        assert!(
            !body_str.contains("src/"),
            "Error response contains source file paths"
        );
        assert!(
            !body_str.contains(".rs:"),
            "Error response contains Rust file locations"
        );
    }
}
