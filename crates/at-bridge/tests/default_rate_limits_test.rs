//! The production limiter (`ApiState::new`): global 600/min, per-client
//! 120/min, per-client-per-route 30/min (loopback peers skip the per-client
//! tiers, see `rate_limit_middleware`). Pinned so a change is deliberate.
//!
//! The values are not arbitrary: one local TUI sends 156 requests/min, all
//! from loopback and all counted against the global tier, so the earlier
//! 100/20/10 defaults returned 429 to a single TUI. A page polling one route
//! every 5 s sends 12 requests/min to that route, above the old 10/min.

use std::sync::Arc;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::state::{
    RATE_LIMIT_CLIENT_PER_MINUTE, RATE_LIMIT_GLOBAL_PER_MINUTE, RATE_LIMIT_ROUTE_PER_MINUTE,
    TUI_REQUESTS_PER_MINUTE,
};
use at_bridge::http_api::{api_router_with_auth, ApiState};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

const ROUTE_POLLS_PER_MINUTE: u64 = 60 / 5;

#[test]
fn default_limits_are_the_documented_values() {
    assert_eq!(RATE_LIMIT_GLOBAL_PER_MINUTE, 600);
    assert_eq!(RATE_LIMIT_CLIENT_PER_MINUTE, 120);
    assert_eq!(RATE_LIMIT_ROUTE_PER_MINUTE, 30);
}

#[test]
fn global_limit_clears_the_tui_poll_budget() {
    assert_eq!(TUI_REQUESTS_PER_MINUTE, 156);
    // One TUI must fit, and so must several together with other loopback
    // clients; the earlier global limit of 100 did not fit even one.
    assert!(RATE_LIMIT_GLOBAL_PER_MINUTE >= 3 * TUI_REQUESTS_PER_MINUTE);
}

#[test]
fn route_limit_clears_five_second_polling() {
    assert!(RATE_LIMIT_ROUTE_PER_MINUTE > ROUTE_POLLS_PER_MINUTE);
    assert!(RATE_LIMIT_ROUTE_PER_MINUTE < RATE_LIMIT_CLIENT_PER_MINUTE);
    assert!(RATE_LIMIT_CLIENT_PER_MINUTE < RATE_LIMIT_GLOBAL_PER_MINUTE);
}

#[tokio::test]
async fn default_api_endpoint_limit_returns_429_with_retry_after() {
    let state = Arc::new(ApiState::new(EventBus::new()));
    let app = api_router_with_auth(state, Some("rate-limit-fixture".into()), vec![]);
    for _ in 0..RATE_LIMIT_ROUTE_PER_MINUTE {
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
async fn default_api_client_limit_is_one_twenty() {
    let state = ApiState::new(EventBus::new());
    for index in 0..RATE_LIMIT_CLIENT_PER_MINUTE {
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
async fn default_api_global_limit_is_six_hundred() {
    let state = ApiState::new(EventBus::new());
    for index in 0..RATE_LIMIT_GLOBAL_PER_MINUTE {
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
