use std::sync::Arc;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router_with_auth, ApiState};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn default_api_endpoint_limit_returns_429_with_retry_after() {
    let state = Arc::new(ApiState::new(EventBus::new()));
    let app = api_router_with_auth(state, Some("rate-limit-fixture".into()), vec![]);
    for _ in 0..10 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/status")
                    .header("x-api-key", "rate-limit-fixture")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/status")
                .header("x-api-key", "rate-limit-fixture")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry: u64 = response.headers()["retry-after"]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(retry > 0);
}

#[tokio::test]
async fn default_api_user_limit_is_twenty() {
    let state = ApiState::new(EventBus::new());
    for index in 0..20 {
        assert!(state
            .rate_limiter
            .check_all("one-user", &format!("/endpoint/{index}"))
            .is_ok());
    }
    assert!(state
        .rate_limiter
        .check_all("one-user", "/endpoint/exhausted")
        .is_err());
}

#[tokio::test]
async fn default_api_global_limit_is_one_hundred() {
    let state = ApiState::new(EventBus::new());
    for index in 0..100 {
        assert!(state
            .rate_limiter
            .check_all(&format!("user-{index}"), &format!("/endpoint/{index}"))
            .is_ok());
    }
    assert!(state
        .rate_limiter
        .check_all("new-user", "/endpoint/exhausted")
        .is_err());
}
