//! Integration tests for WebSocket Origin header validation.
//!
//! These tests verify that the WebSocket endpoints properly validate the Origin
//! header to prevent cross-site WebSocket hijacking attacks.

use std::sync::Arc;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, ApiState};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;

/// Spin up an API server on a random port, return the base URL.
async fn start_test_server() -> (String, Arc<ApiState>) {
    let event_bus = EventBus::new();
    let state = Arc::new(ApiState::new(event_bus).with_relaxed_rate_limits());
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

/// Spin up an API server with PTY pool support for terminal tests.
async fn start_test_server_with_pty() -> (String, Arc<ApiState>) {
    let event_bus = EventBus::new();
    let pool = Arc::new(at_session::pty_pool::PtyPool::new(4));
    let state = Arc::new(ApiState::with_pty_pool(event_bus, pool).with_relaxed_rate_limits());
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

/// Helper to create a terminal for testing /ws/terminal/{id} endpoint.
/// Returns (base_url, terminal_id, state) where base_url includes a running server with PTY support.
async fn create_terminal_with_server() -> (String, String, Arc<ApiState>) {
    let (base, state) = start_test_server_with_pty().await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/terminals", base))
        .send()
        .await
        .expect("failed to create terminal");

    assert_eq!(resp.status(), 201, "failed to create terminal");

    let body: serde_json::Value = resp.json().await.expect("failed to parse response");
    let terminal_id = body["id"]
        .as_str()
        .expect("terminal id not found")
        .to_string();

    (base, terminal_id, state)
}

// ---------------------------------------------------------------------------
// /ws endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ws_valid_localhost_origin() {
    let (base, _state) = start_test_server().await;
    let ws_url = base.replace("http://", "ws://") + "/ws";

    let mut request = ws_url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("origin", HeaderValue::from_static("http://localhost:3000"));

    let result = tokio_tungstenite::connect_async(request).await;
    assert!(result.is_ok(), "Valid localhost origin should be accepted");
}

#[tokio::test]
async fn test_ws_valid_127_0_0_1_origin() {
    let (base, _state) = start_test_server().await;
    let ws_url = base.replace("http://", "ws://") + "/ws";

    let mut request = ws_url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("origin", HeaderValue::from_static("http://127.0.0.1:8080"));

    let result = tokio_tungstenite::connect_async(request).await;
    assert!(result.is_ok(), "Valid 127.0.0.1 origin should be accepted");
}

#[tokio::test]
async fn test_ws_valid_ipv6_localhost_origin() {
    let (base, _state) = start_test_server().await;
    let ws_url = base.replace("http://", "ws://") + "/ws";

    let mut request = ws_url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("origin", HeaderValue::from_static("http://[::1]:9000"));

    let result = tokio_tungstenite::connect_async(request).await;
    assert!(
        result.is_ok(),
        "Valid IPv6 localhost origin should be accepted"
    );
}

#[tokio::test]
async fn test_ws_invalid_external_origin() {
    let (base, _state) = start_test_server().await;
    let ws_url = base.replace("http://", "ws://") + "/ws";

    let mut request = ws_url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("origin", HeaderValue::from_static("http://evil.com"));

    let result = tokio_tungstenite::connect_async(request).await;
    assert!(result.is_err(), "External origin should be rejected");

    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("403") || err_str.contains("Forbidden"),
        "Expected 403 Forbidden, got: {}",
        err_str
    );
}

#[tokio::test]
async fn test_ws_missing_origin_header() {
    let (base, _state) = start_test_server().await;
    let ws_url = base.replace("http://", "ws://") + "/ws";

    // Note: tokio_tungstenite may add its own origin header, so we need to explicitly test
    // In a real browser scenario, the Origin header would always be present for cross-origin requests
    let mut request = ws_url.into_client_request().unwrap();
    // Remove any default origin header that might have been added
    request.headers_mut().remove("origin");

    let result = tokio_tungstenite::connect_async(request).await;
    // Missing origin should be rejected
    assert!(result.is_err(), "Missing origin header should be rejected");

    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("403") || err_str.contains("Forbidden"),
        "Expected 403 Forbidden, got: {}",
        err_str
    );
}

#[tokio::test]
async fn test_ws_malicious_origin_with_localhost_in_path() {
    let (base, _state) = start_test_server().await;
    let ws_url = base.replace("http://", "ws://") + "/ws";

    let mut request = ws_url.into_client_request().unwrap();
    request.headers_mut().insert(
        "origin",
        HeaderValue::from_static("http://evil.com/localhost"),
    );

    let result = tokio_tungstenite::connect_async(request).await;
    assert!(
        result.is_err(),
        "Origin with localhost in path should be rejected"
    );

    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("403") || err_str.contains("Forbidden"),
        "Expected 403 Forbidden, got: {}",
        err_str
    );
}

// ---------------------------------------------------------------------------
// /api/events/ws endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_events_ws_valid_localhost_origin() {
    let (base, _state) = start_test_server().await;
    let ws_url = base.replace("http://", "ws://") + "/api/events/ws";

    let mut request = ws_url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("origin", HeaderValue::from_static("http://localhost"));

    let result = tokio_tungstenite::connect_async(request).await;
    assert!(result.is_ok(), "Valid localhost origin should be accepted");
}

#[tokio::test]
async fn test_events_ws_invalid_external_origin() {
    let (base, _state) = start_test_server().await;
    let ws_url = base.replace("http://", "ws://") + "/api/events/ws";

    let mut request = ws_url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("origin", HeaderValue::from_static("http://attacker.com"));

    let result = tokio_tungstenite::connect_async(request).await;
    assert!(result.is_err(), "External origin should be rejected");

    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("403") || err_str.contains("Forbidden"),
        "Expected 403 Forbidden, got: {}",
        err_str
    );
}

#[tokio::test]
async fn test_events_ws_missing_origin_header() {
    let (base, _state) = start_test_server().await;
    let ws_url = base.replace("http://", "ws://") + "/api/events/ws";

    let mut request = ws_url.into_client_request().unwrap();
    request.headers_mut().remove("origin");

    let result = tokio_tungstenite::connect_async(request).await;
    assert!(result.is_err(), "Missing origin header should be rejected");

    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("403") || err_str.contains("Forbidden"),
        "Expected 403 Forbidden, got: {}",
        err_str
    );
}

// ---------------------------------------------------------------------------
// /ws/terminal/{id} endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_terminal_ws_valid_localhost_origin() {
    let (base, terminal_id, _state) = create_terminal_with_server().await;
    let ws_url = format!(
        "{}/ws/terminal/{}",
        base.replace("http://", "ws://"),
        terminal_id
    );

    let mut request = ws_url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("origin", HeaderValue::from_static("https://localhost:8443"));

    let result = tokio_tungstenite::connect_async(request).await;
    assert!(result.is_ok(), "Valid localhost origin should be accepted");
}

#[tokio::test]
async fn test_terminal_ws_valid_127_0_0_1_origin() {
    let (base, terminal_id, _state) = create_terminal_with_server().await;
    let ws_url = format!(
        "{}/ws/terminal/{}",
        base.replace("http://", "ws://"),
        terminal_id
    );

    let mut request = ws_url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("origin", HeaderValue::from_static("https://127.0.0.1"));

    let result = tokio_tungstenite::connect_async(request).await;
    assert!(result.is_ok(), "Valid 127.0.0.1 origin should be accepted");
}

#[tokio::test]
async fn test_terminal_ws_invalid_external_origin() {
    let (base, terminal_id, _state) = create_terminal_with_server().await;
    let ws_url = format!(
        "{}/ws/terminal/{}",
        base.replace("http://", "ws://"),
        terminal_id
    );

    let mut request = ws_url.into_client_request().unwrap();
    request.headers_mut().insert(
        "origin",
        HeaderValue::from_static("http://malicious.example.com"),
    );

    let result = tokio_tungstenite::connect_async(request).await;
    assert!(result.is_err(), "External origin should be rejected");

    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("403") || err_str.contains("Forbidden"),
        "Expected 403 Forbidden, got: {}",
        err_str
    );
}

#[tokio::test]
async fn test_terminal_ws_missing_origin_header() {
    let (base, terminal_id, _state) = create_terminal_with_server().await;
    let ws_url = format!(
        "{}/ws/terminal/{}",
        base.replace("http://", "ws://"),
        terminal_id
    );

    let mut request = ws_url.into_client_request().unwrap();
    request.headers_mut().remove("origin");

    let result = tokio_tungstenite::connect_async(request).await;
    assert!(result.is_err(), "Missing origin header should be rejected");

    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("403") || err_str.contains("Forbidden"),
        "Expected 403 Forbidden, got: {}",
        err_str
    );
}

#[tokio::test]
async fn test_terminal_ws_subdomain_origin_rejected() {
    let (base, terminal_id, _state) = create_terminal_with_server().await;
    let ws_url = format!(
        "{}/ws/terminal/{}",
        base.replace("http://", "ws://"),
        terminal_id
    );

    let mut request = ws_url.into_client_request().unwrap();
    request.headers_mut().insert(
        "origin",
        HeaderValue::from_static("http://fake.localhost.evil.com"),
    );

    let result = tokio_tungstenite::connect_async(request).await;
    assert!(
        result.is_err(),
        "Origin with fake localhost subdomain should be rejected"
    );

    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("403") || err_str.contains("Forbidden"),
        "Expected 403 Forbidden, got: {}",
        err_str
    );
}

// ---------------------------------------------------------------------------
// Security-focused tests - testing attack scenarios
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_security_cross_site_websocket_hijacking_blocked() {
    // This test simulates an attacker's webpage trying to connect to the local daemon
    let (base, _state) = start_test_server().await;
    let ws_url = base.replace("http://", "ws://") + "/ws";

    // Attacker's origin
    let mut request = ws_url.into_client_request().unwrap();
    request.headers_mut().insert(
        "origin",
        HeaderValue::from_static("https://attacker-site.com"),
    );

    let result = tokio_tungstenite::connect_async(request).await;
    assert!(
        result.is_err(),
        "Cross-site WebSocket hijacking attempt should be blocked"
    );
}

#[tokio::test]
async fn test_security_terminal_hijacking_blocked() {
    // This test simulates an attacker trying to hijack a terminal session
    let (base, terminal_id, _state) = create_terminal_with_server().await;
    let ws_url = format!(
        "{}/ws/terminal/{}",
        base.replace("http://", "ws://"),
        terminal_id
    );

    // Attacker's origin attempting to connect to the terminal
    let mut request = ws_url.into_client_request().unwrap();
    request.headers_mut().insert(
        "origin",
        HeaderValue::from_static("http://phishing-site.com"),
    );

    let result = tokio_tungstenite::connect_async(request).await;
    assert!(
        result.is_err(),
        "Terminal hijacking attempt should be blocked"
    );
}

#[tokio::test]
async fn test_https_localhost_origins_accepted() {
    // Test that HTTPS localhost connections work (important for production)
    let (base, _state) = start_test_server().await;
    let ws_url = base.replace("http://", "ws://") + "/ws";

    let test_origins = vec![
        "https://localhost",
        "https://localhost:443",
        "https://localhost:8443",
        "https://127.0.0.1",
        "https://127.0.0.1:443",
        "https://[::1]",
        "https://[::1]:443",
    ];

    for origin in test_origins {
        let mut request = ws_url.clone().into_client_request().unwrap();
        request
            .headers_mut()
            .insert("origin", HeaderValue::from_str(origin).unwrap());

        let result = tokio_tungstenite::connect_async(request).await;
        assert!(
            result.is_ok(),
            "HTTPS localhost origin {} should be accepted",
            origin
        );
    }
}
