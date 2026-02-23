use std::sync::Arc;

use at_bridge::auth::AuthLayer;
use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router_with_auth, ApiState};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use axum::Router;
use serde_json::Value;
use tower::ServiceExt;

/// Build a minimal test router with the auth layer applied.
fn auth_router(api_key: Option<String>) -> Router {
    Router::new()
        .route("/ping", get(|| async { "pong" }))
        .route("/echo", post(|body: String| async move { body }))
        .layer(AuthLayer::new(api_key))
}

/// Build the full API router with auth for integration-style tests.
fn full_api_router(api_key: Option<String>) -> Router {
    let event_bus = EventBus::new();
    let state = Arc::new(ApiState::new(event_bus));
    api_router_with_auth(state, api_key)
}

/// Helper to read the response body as bytes.
async fn body_bytes(resp: axum::http::Response<Body>) -> Vec<u8> {
    axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec()
}

// ===========================================================================
// Development Mode (no API key configured)
// ===========================================================================

#[tokio::test]
async fn test_no_api_key_allows_all_requests() {
    let app = auth_router(None);

    let req = Request::builder().uri("/ping").body(Body::empty()).unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_bytes(resp).await;
    assert_eq!(body, b"pong");
}

#[tokio::test]
async fn test_dev_mode_allows_get() {
    let app = full_api_router(None);

    let req = Request::builder()
        .uri("/api/status")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_dev_mode_allows_post() {
    let app = auth_router(None);

    let req = Request::builder()
        .method("POST")
        .uri("/echo")
        .header("content-type", "text/plain")
        .body(Body::from("hello world"))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_bytes(resp).await;
    assert_eq!(body, b"hello world");
}

// ===========================================================================
// API Key Authentication
// ===========================================================================

#[tokio::test]
async fn test_valid_x_api_key_header_allowed() {
    let app = auth_router(Some("my-secret-key".into()));

    let req = Request::builder()
        .uri("/ping")
        .header("X-API-Key", "my-secret-key")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_bytes(resp).await;
    assert_eq!(body, b"pong");
}

#[tokio::test]
async fn test_valid_bearer_token_allowed() {
    let app = auth_router(Some("my-secret-key".into()));

    let req = Request::builder()
        .uri("/ping")
        .header("Authorization", "Bearer my-secret-key")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_invalid_x_api_key_rejected_401() {
    let app = auth_router(Some("correct-key".into()));

    let req = Request::builder()
        .uri("/ping")
        .header("X-API-Key", "wrong-key")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_invalid_bearer_token_rejected_401() {
    let app = auth_router(Some("correct-key".into()));

    let req = Request::builder()
        .uri("/ping")
        .header("Authorization", "Bearer wrong-key")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_missing_auth_header_rejected_401() {
    let app = auth_router(Some("correct-key".into()));

    let req = Request::builder().uri("/ping").body(Body::empty()).unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_empty_api_key_rejected_401() {
    let app = auth_router(Some("correct-key".into()));

    let req = Request::builder()
        .uri("/ping")
        .header("X-API-Key", "")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ===========================================================================
// Edge Cases
// ===========================================================================

#[tokio::test]
async fn test_api_key_case_sensitive() {
    let app = auth_router(Some("CaseSensitiveKey".into()));

    // Wrong case should fail
    let req = Request::builder()
        .uri("/ping")
        .header("X-API-Key", "casesensitivekey")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Exact case should succeed
    let app = auth_router(Some("CaseSensitiveKey".into()));
    let req = Request::builder()
        .uri("/ping")
        .header("X-API-Key", "CaseSensitiveKey")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_multiple_auth_headers() {
    // When both X-API-Key and Authorization are present, X-API-Key takes priority
    let app = auth_router(Some("the-key".into()));

    // Correct X-API-Key, wrong Bearer -> should still pass (X-API-Key wins)
    let req = Request::builder()
        .uri("/ping")
        .header("X-API-Key", "the-key")
        .header("Authorization", "Bearer wrong-key")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_auth_preserves_request_body() {
    let app = auth_router(Some("secret".into()));

    let payload = "important data payload";
    let req = Request::builder()
        .method("POST")
        .uri("/echo")
        .header("X-API-Key", "secret")
        .header("content-type", "text/plain")
        .body(Body::from(payload))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_bytes(resp).await;
    assert_eq!(String::from_utf8(body).unwrap(), payload);
}

#[tokio::test]
async fn test_auth_error_response_is_json() {
    let app = auth_router(Some("secret".into()));

    let req = Request::builder().uri("/ping").body(Body::empty()).unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Verify the error body is JSON
    let body = body_bytes(resp).await;
    let json: Value =
        serde_json::from_slice(&body).expect("401 response body should be valid JSON");
    assert_eq!(json["error"], "unauthorized");
}
