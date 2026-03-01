use std::sync::Arc;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, ApiState};
use serde_json::json;

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

/// Generate a large JSON payload of approximately the specified size.
fn generate_large_payload(size_bytes: usize) -> serde_json::Value {
    let padding = "x".repeat(size_bytes.saturating_sub(100));
    json!({
        "title": "Test payload",
        "description": "Large payload for body limit testing",
        "padding": padding
    })
}

#[tokio::test]
async fn test_body_limit_under_2mb_succeeds() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create a payload under 2MB (~1MB)
    let payload = generate_large_payload(1024 * 1024);

    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&payload)
        .send()
        .await
        .unwrap();

    // Should succeed (201 Created)
    assert_eq!(resp.status(), 201);
    let created: serde_json::Value = resp.json().await.unwrap();
    assert!(created["id"].is_string());
}

#[tokio::test]
async fn test_body_limit_over_2mb_rejected() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create a payload over 2MB (~3MB)
    let payload = generate_large_payload(3 * 1024 * 1024);

    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&payload)
        .send()
        .await
        .unwrap();

    // Should be rejected with 413 Payload Too Large
    assert_eq!(resp.status(), 413);
}

#[tokio::test]
async fn test_task_creation_respects_2mb_limit() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create a payload just over 2MB for task creation
    // Task creation uses the default 2MB limit
    let payload = generate_large_payload(2 * 1024 * 1024 + 100 * 1024);

    let resp = client
        .post(format!("{base}/api/tasks"))
        .json(&payload)
        .send()
        .await
        .unwrap();

    // Should be rejected with 413 (exceeds default 2MB limit)
    assert_eq!(resp.status(), 413);
}

#[tokio::test]
async fn test_bead_creation_over_2mb_rejected() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Try to create a bead with a payload over 2MB
    // This verifies the default 2MB limit applies to POST /api/beads
    let payload = generate_large_payload(2 * 1024 * 1024 + 200 * 1024);

    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&payload)
        .send()
        .await
        .unwrap();

    // Should be rejected with 413 (exceeds default 2MB limit)
    assert_eq!(resp.status(), 413);
}

#[tokio::test]
async fn test_simple_endpoint_accepts_100kb() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // First, create a bead to update
    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&json!({
            "title": "Test bead for status update",
            "description": "Test"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let bead: serde_json::Value = resp.json().await.unwrap();
    let bead_id = bead["id"].as_str().unwrap();

    // Create a small payload (~100KB, under 256KB limit)
    let padding = "x".repeat(100 * 1024);
    let payload = json!({
        "status": "hooked",
        "padding": padding
    });

    let resp = client
        .post(format!("{base}/api/beads/{bead_id}/status"))
        .json(&payload)
        .send()
        .await
        .unwrap();

    // Should succeed (200 OK)
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_simple_endpoint_rejects_512kb() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // First, create a bead to update
    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&json!({
            "title": "Test bead for status update",
            "description": "Test"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let bead: serde_json::Value = resp.json().await.unwrap();
    let bead_id = bead["id"].as_str().unwrap();

    // Create a payload over 256KB (~512KB)
    let padding = "x".repeat(512 * 1024);
    let payload = json!({
        "status": "hooked",
        "padding": padding
    });

    let resp = client
        .post(format!("{base}/api/beads/{bead_id}/status"))
        .json(&payload)
        .send()
        .await
        .unwrap();

    // Should be rejected with 413 Payload Too Large
    assert_eq!(resp.status(), 413);
}

#[tokio::test]
async fn test_body_limit_at_2mb_boundary() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create a payload at exactly 2MB (slightly under to account for JSON overhead)
    let payload = generate_large_payload(2 * 1024 * 1024 - 200);

    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&payload)
        .send()
        .await
        .unwrap();

    // Should succeed at the boundary
    assert_eq!(resp.status(), 201);
}

#[tokio::test]
async fn test_body_limit_just_over_2mb() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create a payload just over 2MB
    let payload = generate_large_payload(2 * 1024 * 1024 + 1024);

    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&payload)
        .send()
        .await
        .unwrap();

    // Should be rejected
    assert_eq!(resp.status(), 413);
}

#[tokio::test]
async fn test_multiple_endpoints_with_256kb_limit() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create two beads to test status updates
    let resp1 = client
        .post(format!("{base}/api/beads"))
        .json(&json!({"title": "Test bead 1"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp1.status(), 201);
    let bead1: serde_json::Value = resp1.json().await.unwrap();
    let bead1_id = bead1["id"].as_str().unwrap();

    let resp2 = client
        .post(format!("{base}/api/beads"))
        .json(&json!({"title": "Test bead 2"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp2.status(), 201);
    let bead2: serde_json::Value = resp2.json().await.unwrap();
    let bead2_id = bead2["id"].as_str().unwrap();

    // Test first 256KB-limited endpoint with oversized payload
    let padding = "x".repeat(512 * 1024);
    let resp = client
        .post(format!("{base}/api/beads/{bead1_id}/status"))
        .json(&json!({
            "status": "hooked",
            "padding": padding
        }))
        .send()
        .await
        .unwrap();

    // Should be rejected with 413 (body too large for 256KB limit)
    assert_eq!(resp.status(), 413);

    // Test another bead status update with oversized payload
    let padding2 = "x".repeat(512 * 1024);
    let resp = client
        .post(format!("{base}/api/beads/{bead2_id}/status"))
        .json(&json!({
            "status": "hooked",
            "padding": padding2
        }))
        .send()
        .await
        .unwrap();

    // Should also be rejected with 413
    assert_eq!(resp.status(), 413);
}

#[tokio::test]
async fn test_get_requests_not_affected() {
    let (base, _state) = start_test_server().await;

    // GET requests should not be affected by body limits
    let resp = reqwest::get(format!("{base}/api/status"))
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let resp = reqwest::get(format!("{base}/api/beads"))
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
}
