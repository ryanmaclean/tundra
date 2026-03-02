use std::sync::Arc;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, ApiState};

/// Spin up an API server on a random port, return the base URL.
async fn start_test_server() -> String {
    let event_bus = EventBus::new();
    let state = Arc::new(ApiState::new(event_bus).with_relaxed_rate_limits());
    let router = api_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind to ephemeral port");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    format!("http://{addr}")
}

#[tokio::test]
async fn test_cross_origin_headers_present() {
    let base = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/status")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let headers = resp.headers();

    // Verify WASM threading headers
    assert_eq!(
        headers.get("Cross-Origin-Opener-Policy").unwrap(),
        "same-origin"
    );
    assert_eq!(
        headers.get("Cross-Origin-Embedder-Policy").unwrap(),
        "credentialless"
    );
    assert_eq!(
        headers.get("Cross-Origin-Resource-Policy").unwrap(),
        "same-origin"
    );
}

#[tokio::test]
async fn test_x_content_type_options_header() {
    let base = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/status")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let headers = resp.headers();
    assert_eq!(headers.get("X-Content-Type-Options").unwrap(), "nosniff");
}

#[tokio::test]
async fn test_x_frame_options_header() {
    let base = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/status")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let headers = resp.headers();
    assert_eq!(headers.get("X-Frame-Options").unwrap(), "DENY");
}

#[tokio::test]
async fn test_strict_transport_security_header() {
    let base = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/status")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let headers = resp.headers();
    assert_eq!(
        headers.get("Strict-Transport-Security").unwrap(),
        "max-age=63072000; includeSubDomains"
    );
}

#[tokio::test]
async fn test_cache_control_header() {
    let base = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/status")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let headers = resp.headers();
    assert_eq!(
        headers.get("Cache-Control").unwrap(),
        "no-store, no-cache, must-revalidate, private"
    );
}

#[tokio::test]
async fn test_x_xss_protection_header() {
    let base = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/status")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let headers = resp.headers();
    assert_eq!(headers.get("X-XSS-Protection").unwrap(), "1; mode=block");
}

#[tokio::test]
async fn test_referrer_policy_header() {
    let base = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/status")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let headers = resp.headers();
    assert_eq!(
        headers.get("Referrer-Policy").unwrap(),
        "strict-origin-when-cross-origin"
    );
}

#[tokio::test]
async fn test_all_security_headers_on_get_endpoint() {
    let base = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/beads")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let headers = resp.headers();

    // Cross-origin headers
    assert!(headers.contains_key("Cross-Origin-Opener-Policy"));
    assert!(headers.contains_key("Cross-Origin-Embedder-Policy"));
    assert!(headers.contains_key("Cross-Origin-Resource-Policy"));

    // Security headers
    assert!(headers.contains_key("X-Content-Type-Options"));
    assert!(headers.contains_key("X-Frame-Options"));
    assert!(headers.contains_key("Strict-Transport-Security"));
    assert!(headers.contains_key("Cache-Control"));
    assert!(headers.contains_key("X-XSS-Protection"));
    assert!(headers.contains_key("Referrer-Policy"));
}

#[tokio::test]
async fn test_all_security_headers_on_post_endpoint() {
    let base = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&serde_json::json!({
            "title": "Security test bead"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let headers = resp.headers();

    // Cross-origin headers
    assert!(headers.contains_key("Cross-Origin-Opener-Policy"));
    assert!(headers.contains_key("Cross-Origin-Embedder-Policy"));
    assert!(headers.contains_key("Cross-Origin-Resource-Policy"));

    // Security headers
    assert!(headers.contains_key("X-Content-Type-Options"));
    assert!(headers.contains_key("X-Frame-Options"));
    assert!(headers.contains_key("Strict-Transport-Security"));
    assert!(headers.contains_key("Cache-Control"));
    assert!(headers.contains_key("X-XSS-Protection"));
    assert!(headers.contains_key("Referrer-Policy"));
}

#[tokio::test]
async fn test_security_headers_on_404_response() {
    let base = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/nonexistent"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    let headers = resp.headers();

    // Security headers should be present even on error responses
    assert!(headers.contains_key("X-Content-Type-Options"));
    assert!(headers.contains_key("X-Frame-Options"));
    assert!(headers.contains_key("Strict-Transport-Security"));
    assert!(headers.contains_key("Cache-Control"));
}

#[tokio::test]
async fn test_security_headers_on_websocket_upgrade_attempt() {
    let base = start_test_server().await;

    // Attempt to connect to WebSocket endpoint without upgrade headers
    // This will fail but should still return security headers
    let resp = reqwest::get(format!("{base}/ws")).await.unwrap();

    let headers = resp.headers();

    // Security headers should be present even on WebSocket endpoint
    assert!(headers.contains_key("X-Content-Type-Options"));
    assert!(headers.contains_key("X-Frame-Options"));
    assert!(headers.contains_key("Strict-Transport-Security"));
}
