//! Event -> notification conversion happens exactly once per event,
//! regardless of how many `/api/events/ws` clients are connected.

use std::sync::Arc;
use std::time::Duration;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, ApiState};
use serde_json::Value;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;

async fn start_server(start_notifications: bool) -> (String, Arc<ApiState>) {
    let state = Arc::new(ApiState::new(EventBus::new()).with_relaxed_rate_limits());
    if start_notifications {
        state.start_notification_task();
    }
    let router = api_router(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (format!("http://{addr}"), state)
}

async fn notification_total(base: &str) -> u64 {
    let v: Value = reqwest::get(format!("{base}/api/notifications/count"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    v["total"].as_u64().unwrap()
}

async fn wait_for_total(base: &str, expected: u64) -> u64 {
    let mut total = 0;
    for _ in 0..40 {
        total = notification_total(base).await;
        if total >= expected {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    // Give any duplicate writers a chance to show up.
    tokio::time::sleep(Duration::from_millis(150)).await;
    total.max(notification_total(base).await)
}

async fn create_bead(base: &str) {
    let resp = reqwest::Client::new()
        .post(format!("{base}/api/beads"))
        .json(&serde_json::json!({"title": "notify me"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
}

#[tokio::test]
async fn one_notification_per_event_with_many_ws_clients() {
    let (base, _state) = start_server(true).await;
    let ws_url = base.replace("http://", "ws://") + "/api/events/ws";

    let mut clients = Vec::new();
    for _ in 0..3 {
        let mut req = ws_url.as_str().into_client_request().unwrap();
        req.headers_mut()
            .insert("origin", HeaderValue::from_static("http://localhost"));
        let (ws, _) = tokio_tungstenite::connect_async(req).await.unwrap();
        clients.push(ws);
    }
    tokio::time::sleep(Duration::from_millis(50)).await;

    let before = notification_total(&base).await;
    create_bead(&base).await;
    let after = wait_for_total(&base, before + 1).await;
    assert_eq!(
        after - before,
        1,
        "BeadCreated must produce exactly one notification with 3 WS clients"
    );
    drop(clients);
}

#[tokio::test]
async fn notifications_recorded_with_no_ws_clients() {
    let (base, _state) = start_server(true).await;
    let before = notification_total(&base).await;
    create_bead(&base).await;
    let after = wait_for_total(&base, before + 1).await;
    assert_eq!(after - before, 1);
}

#[tokio::test]
async fn start_notification_task_is_idempotent() {
    let (base, state) = start_server(true).await;
    state.start_notification_task();
    state.start_notification_task();
    let before = notification_total(&base).await;
    create_bead(&base).await;
    let after = wait_for_total(&base, before + 1).await;
    assert_eq!(after - before, 1);
}
