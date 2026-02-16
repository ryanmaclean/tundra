use std::sync::Arc;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, ApiState};
use at_bridge::protocol::BridgeMessage;
use serde_json::{json, Value};

/// Spin up an API server on a random port, return the base URL.
async fn start_test_server() -> (String, Arc<ApiState>) {
    let event_bus = EventBus::new();
    let state = Arc::new(ApiState::new(event_bus));
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

#[tokio::test]
async fn test_get_status() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/status"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert!(body["version"].is_string());
    assert_eq!(body["agent_count"], 0);
    assert_eq!(body["bead_count"], 0);
    assert!(body["uptime_seconds"].is_number());
}

#[tokio::test]
async fn test_list_beads_empty() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/beads"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = resp.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn test_create_and_list_beads() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create a bead
    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&serde_json::json!({
            "title": "Test bead",
            "description": "A test description"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let created: Value = resp.json().await.unwrap();
    assert_eq!(created["title"], "Test bead");
    assert_eq!(created["description"], "A test description");
    assert_eq!(created["status"], "backlog");
    assert!(created["id"].is_string());

    // List beads and verify
    let resp = reqwest::get(format!("{base}/api/beads"))
        .await
        .unwrap();
    let beads: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(beads.len(), 1);
    assert_eq!(beads[0]["title"], "Test bead");
}

#[tokio::test]
async fn test_update_bead_status() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create a bead (starts as backlog)
    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&serde_json::json!({ "title": "Status test" }))
        .send()
        .await
        .unwrap();
    let created: Value = resp.json().await.unwrap();
    let id = created["id"].as_str().unwrap();

    // Valid transition: backlog -> hooked
    let resp = client
        .post(format!("{base}/api/beads/{id}/status"))
        .json(&serde_json::json!({ "status": "hooked" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let updated: Value = resp.json().await.unwrap();
    assert_eq!(updated["status"], "hooked");

    // Invalid transition: hooked -> done (should fail)
    let resp = client
        .post(format!("{base}/api/beads/{id}/status"))
        .json(&serde_json::json!({ "status": "done" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_update_bead_status_not_found() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let resp = client
        .post(format!("{base}/api/beads/{fake_id}/status"))
        .json(&serde_json::json!({ "status": "hooked" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_list_agents_empty() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/agents"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = resp.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn test_get_kpi() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/kpi"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["total_beads"], 0);
    assert_eq!(body["active_agents"], 0);
}

#[tokio::test]
async fn test_websocket_receives_events() {
    use futures_util::StreamExt;

    let (base, state) = start_test_server().await;
    let ws_url = base.replace("http://", "ws://") + "/ws";

    let (mut ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("failed to connect to websocket");

    // Give the WebSocket subscription a moment to register
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Publish an event through the event bus
    state
        .event_bus
        .publish(BridgeMessage::GetStatus);

    // Receive the event on the WS
    let msg = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        ws_stream.next(),
    )
    .await
    .expect("timed out waiting for ws message")
    .expect("stream ended")
    .expect("ws error");

    let text = msg.into_text().expect("expected text message");
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["type"], "get_status");
}

// ---------------------------------------------------------------------------
// Task CRUD tests
// ---------------------------------------------------------------------------

fn task_payload() -> Value {
    json!({
        "title": "Implement login",
        "bead_id": uuid::Uuid::new_v4(),
        "category": "feature",
        "priority": "high",
        "complexity": "medium"
    })
}

#[tokio::test]
async fn test_create_task() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/tasks"))
        .json(&task_payload())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["title"], "Implement login");
    assert_eq!(body["phase"], "discovery");
    assert_eq!(body["category"], "feature");
    assert_eq!(body["priority"], "high");
    assert_eq!(body["complexity"], "medium");
    assert!(body["id"].is_string());
}

#[tokio::test]
async fn test_list_tasks() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Empty initially
    let resp = reqwest::get(format!("{base}/api/tasks")).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: Vec<Value> = resp.json().await.unwrap();
    assert!(body.is_empty());

    // Create one
    client
        .post(format!("{base}/api/tasks"))
        .json(&task_payload())
        .send()
        .await
        .unwrap();

    let resp = reqwest::get(format!("{base}/api/tasks")).await.unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 1);
}

#[tokio::test]
async fn test_get_task() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/tasks"))
        .json(&task_payload())
        .send()
        .await
        .unwrap();
    let created: Value = resp.json().await.unwrap();
    let id = created["id"].as_str().unwrap();

    let resp = reqwest::get(format!("{base}/api/tasks/{id}")).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["title"], "Implement login");
}

#[tokio::test]
async fn test_get_task_not_found() {
    let (base, _state) = start_test_server().await;
    let fake_id = uuid::Uuid::new_v4();
    let resp = reqwest::get(format!("{base}/api/tasks/{fake_id}")).await.unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_update_task_phase() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/tasks"))
        .json(&task_payload())
        .send()
        .await
        .unwrap();
    let created: Value = resp.json().await.unwrap();
    let id = created["id"].as_str().unwrap();

    // discovery -> context_gathering (valid)
    let resp = client
        .post(format!("{base}/api/tasks/{id}/phase"))
        .json(&json!({"phase": "context_gathering"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["phase"], "context_gathering");
}

#[tokio::test]
async fn test_update_task_phase_invalid() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/tasks"))
        .json(&task_payload())
        .send()
        .await
        .unwrap();
    let created: Value = resp.json().await.unwrap();
    let id = created["id"].as_str().unwrap();

    // discovery -> coding (invalid, skips steps)
    let resp = client
        .post(format!("{base}/api/tasks/{id}/phase"))
        .json(&json!({"phase": "coding"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_get_task_logs() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/tasks"))
        .json(&task_payload())
        .send()
        .await
        .unwrap();
    let created: Value = resp.json().await.unwrap();
    let id = created["id"].as_str().unwrap();

    let resp = reqwest::get(format!("{base}/api/tasks/{id}/logs")).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: Vec<Value> = resp.json().await.unwrap();
    assert!(body.is_empty()); // No logs on a fresh task
}
