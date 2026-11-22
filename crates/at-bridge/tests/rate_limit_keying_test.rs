//! End-to-end rate-limit keying with the production `ApiState` limits.
//!
//! Regression for: every local client shared one 20 req/min "unknown" bucket,
//! so a single TUI refresh (~13 GETs) nearly emptied it and the second
//! refresh produced 429s.

use std::net::SocketAddr;
use std::sync::Arc;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, ApiState};
use at_bridge::rate_limit_middleware::RateLimitPolicy;

async fn serve(state: Arc<ApiState>, with_connect_info: bool) -> String {
    let router = api_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        if with_connect_info {
            axum::serve(
                listener,
                router.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .unwrap();
        } else {
            axum::serve(listener, router).await.unwrap();
        }
    });
    format!("http://{addr}")
}

const TUI_ENDPOINTS: &[&str] = &[
    "/api/status",
    "/api/beads",
    "/api/agents",
    "/api/kpi",
    "/api/tasks",
    "/api/bootstrap",
    "/api/notifications",
    "/api/notifications/count",
    "/api/worktrees",
    "/api/mcp/servers",
    "/api/kanban/columns",
    "/api/settings",
    "/api/projects",
];

#[tokio::test]
async fn tui_polling_from_loopback_is_not_rate_limited() {
    // Production limits (ApiState::new), NOT with_relaxed_rate_limits().
    let state = Arc::new(ApiState::new(EventBus::new()));
    let base = serve(state, true).await;
    let client = reqwest::Client::new();

    // Five TUI refresh cycles = 65 requests from one loopback client.
    for cycle in 0..5 {
        for ep in TUI_ENDPOINTS {
            let resp = client.get(format!("{base}{ep}")).send().await.unwrap();
            assert_ne!(
                resp.status(),
                reqwest::StatusCode::TOO_MANY_REQUESTS,
                "cycle {cycle} {ep} was rate limited"
            );
        }
    }
}

#[tokio::test]
async fn non_exempt_client_is_still_limited_per_route() {
    let mut state = ApiState::new(EventBus::new());
    state.rate_limit_policy = RateLimitPolicy {
        exempt_loopback: false,
        trust_proxy_headers: false,
    };
    let base = serve(Arc::new(state), true).await;
    let client = reqwest::Client::new();

    // Per-client-per-route tier is 120/min; the 121st hit on one route is 429.
    let mut limited = false;
    for _ in 0..121 {
        let resp = client
            .get(format!("{base}/api/status"))
            .send()
            .await
            .unwrap();
        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            limited = true;
            break;
        }
    }
    assert!(
        limited,
        "per-route tier should still apply to non-exempt clients"
    );

    // A different route from the same client is unaffected.
    let resp = client.get(format!("{base}/api/kpi")).send().await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
}
