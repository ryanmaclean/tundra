use std::sync::Arc;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, ApiState};
use at_bridge::protocol::BridgeMessage;
use at_core::config::Config;
use at_core::settings::SettingsManager;
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

/// Spin up an API server backed by a temp settings file and preloaded config.
async fn start_test_server_with_config(config: Config) -> (String, Arc<ApiState>) {
    let event_bus = EventBus::new();
    let tmp_path = std::env::temp_dir()
        .join(format!("at-http-api-test-{}", uuid::Uuid::new_v4()))
        .join("settings.toml");
    let settings_manager = Arc::new(SettingsManager::new(&tmp_path));
    settings_manager.save(&config).expect("save test settings");

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

#[tokio::test]
async fn test_get_status() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/status")).await.unwrap();
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

    let resp = reqwest::get(format!("{base}/api/beads")).await.unwrap();
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
    let resp = reqwest::get(format!("{base}/api/beads")).await.unwrap();
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
async fn test_delete_bead() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create a bead
    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&serde_json::json!({
            "title": "Delete me",
            "description": "This bead will be deleted"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let created: Value = resp.json().await.unwrap();
    let id = created["id"].as_str().unwrap();

    // Delete the bead
    let resp = client
        .delete(format!("{base}/api/beads/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let deleted: Value = resp.json().await.unwrap();
    assert_eq!(deleted["status"], "deleted");

    // Verify it's gone
    let resp = reqwest::get(format!("{base}/api/beads")).await.unwrap();
    let beads: Vec<Value> = resp.json().await.unwrap();
    assert!(beads.is_empty());
}

#[tokio::test]
async fn test_list_agents_empty() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/agents")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = resp.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn test_get_kpi() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/kpi")).await.unwrap();
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

    let request = tokio_tungstenite::tungstenite::http::Request::builder()
        .uri(&ws_url)
        .header("Host", "localhost")
        .header("Origin", "http://localhost")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tokio_tungstenite::tungstenite::handshake::client::generate_key(),
        )
        .body(())
        .unwrap();
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(request)
        .await
        .expect("failed to connect to websocket");

    // Give the WebSocket subscription a moment to register
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Publish an event through the event bus
    state.event_bus.publish(BridgeMessage::GetStatus);

    // Receive the event on the WS
    let msg = tokio::time::timeout(std::time::Duration::from_secs(2), ws_stream.next())
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

    let resp = reqwest::get(format!("{base}/api/tasks/{id}"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["title"], "Implement login");
}

#[tokio::test]
async fn test_get_task_not_found() {
    let (base, _state) = start_test_server().await;
    let fake_id = uuid::Uuid::new_v4();
    let resp = reqwest::get(format!("{base}/api/tasks/{fake_id}"))
        .await
        .unwrap();
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

    let resp = reqwest::get(format!("{base}/api/tasks/{id}/logs"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Vec<Value> = resp.json().await.unwrap();
    assert!(body.is_empty()); // No logs on a fresh task
}

#[tokio::test]
async fn test_list_tasks_with_filters_by_phase() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create tasks with different phases
    let bead_id = uuid::Uuid::new_v4();

    // Task 1: discovery phase (default)
    client
        .post(format!("{base}/api/tasks"))
        .json(&json!({
            "title": "Task 1",
            "bead_id": bead_id,
            "category": "feature",
            "priority": "high",
            "complexity": "medium"
        }))
        .send()
        .await
        .unwrap();

    // Task 2: context_gathering phase
    let resp = client
        .post(format!("{base}/api/tasks"))
        .json(&json!({
            "title": "Task 2",
            "bead_id": bead_id,
            "category": "feature",
            "priority": "medium",
            "complexity": "medium"
        }))
        .send()
        .await
        .unwrap();
    let task2: Value = resp.json().await.unwrap();
    let task2_id = task2["id"].as_str().unwrap();

    // Update task 2 to context_gathering phase (valid: discovery -> context_gathering)
    client
        .post(format!("{base}/api/tasks/{task2_id}/phase"))
        .json(&json!({ "phase": "context_gathering" }))
        .send()
        .await
        .unwrap();

    // Task 3: spec_creation phase
    let resp = client
        .post(format!("{base}/api/tasks"))
        .json(&json!({
            "title": "Task 3",
            "bead_id": bead_id,
            "category": "bug_fix",
            "priority": "urgent",
            "complexity": "small"
        }))
        .send()
        .await
        .unwrap();
    let task3: Value = resp.json().await.unwrap();
    let task3_id = task3["id"].as_str().unwrap();

    // Update task 3 to spec_creation phase
    client
        .post(format!("{base}/api/tasks/{task3_id}/phase"))
        .json(&json!({ "phase": "context_gathering" }))
        .send()
        .await
        .unwrap();

    client
        .post(format!("{base}/api/tasks/{task3_id}/phase"))
        .json(&json!({ "phase": "spec_creation" }))
        .send()
        .await
        .unwrap();

    // Filter by phase=discovery (should return 1 task)
    let resp = reqwest::get(format!("{base}/api/tasks?phase=discovery"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["title"], "Task 1");
    assert_eq!(body[0]["phase"], "discovery");

    // Filter by phase=context_gathering (should return 1 task)
    let resp = reqwest::get(format!("{base}/api/tasks?phase=context_gathering"))
        .await
        .unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["title"], "Task 2");
    assert_eq!(body[0]["phase"], "context_gathering");

    // Filter by phase=spec_creation (should return 1 task)
    let resp = reqwest::get(format!("{base}/api/tasks?phase=spec_creation"))
        .await
        .unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["title"], "Task 3");
    assert_eq!(body[0]["phase"], "spec_creation");

    // Filter by phase=complete (should return 0 tasks)
    let resp = reqwest::get(format!("{base}/api/tasks?phase=complete"))
        .await
        .unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 0);

    // No filter (should return all 3 tasks)
    let resp = reqwest::get(format!("{base}/api/tasks")).await.unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 3);
}

#[tokio::test]
async fn test_list_tasks_with_filters_by_category() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create tasks with different categories
    client
        .post(format!("{base}/api/tasks"))
        .json(&json!({
            "title": "Feature Task",
            "bead_id": uuid::Uuid::new_v4(),
            "category": "feature",
            "priority": "high",
            "complexity": "medium"
        }))
        .send()
        .await
        .unwrap();

    client
        .post(format!("{base}/api/tasks"))
        .json(&json!({
            "title": "Bug Fix Task",
            "bead_id": uuid::Uuid::new_v4(),
            "category": "bug_fix",
            "priority": "urgent",
            "complexity": "small"
        }))
        .send()
        .await
        .unwrap();

    client
        .post(format!("{base}/api/tasks"))
        .json(&json!({
            "title": "Documentation Task",
            "bead_id": uuid::Uuid::new_v4(),
            "category": "documentation",
            "priority": "low",
            "complexity": "trivial"
        }))
        .send()
        .await
        .unwrap();

    // Filter by category=feature
    let resp = reqwest::get(format!("{base}/api/tasks?category=feature"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["title"], "Feature Task");
    assert_eq!(body[0]["category"], "feature");

    // Filter by category=bug_fix
    let resp = reqwest::get(format!("{base}/api/tasks?category=bug_fix"))
        .await
        .unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["title"], "Bug Fix Task");
    assert_eq!(body[0]["category"], "bug_fix");

    // Filter by category=documentation
    let resp = reqwest::get(format!("{base}/api/tasks?category=documentation"))
        .await
        .unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["title"], "Documentation Task");

    // Filter by category=security (should return 0 tasks)
    let resp = reqwest::get(format!("{base}/api/tasks?category=security"))
        .await
        .unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 0);
}

#[tokio::test]
async fn test_list_tasks_with_filters_by_priority() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create tasks with different priorities
    client
        .post(format!("{base}/api/tasks"))
        .json(&json!({
            "title": "Low Priority Task",
            "bead_id": uuid::Uuid::new_v4(),
            "category": "feature",
            "priority": "low",
            "complexity": "medium"
        }))
        .send()
        .await
        .unwrap();

    client
        .post(format!("{base}/api/tasks"))
        .json(&json!({
            "title": "High Priority Task",
            "bead_id": uuid::Uuid::new_v4(),
            "category": "bug_fix",
            "priority": "high",
            "complexity": "large"
        }))
        .send()
        .await
        .unwrap();

    client
        .post(format!("{base}/api/tasks"))
        .json(&json!({
            "title": "Urgent Priority Task",
            "bead_id": uuid::Uuid::new_v4(),
            "category": "security",
            "priority": "urgent",
            "complexity": "medium"
        }))
        .send()
        .await
        .unwrap();

    // Filter by priority=low
    let resp = reqwest::get(format!("{base}/api/tasks?priority=low"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["title"], "Low Priority Task");
    assert_eq!(body[0]["priority"], "low");

    // Filter by priority=high
    let resp = reqwest::get(format!("{base}/api/tasks?priority=high"))
        .await
        .unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["title"], "High Priority Task");
    assert_eq!(body[0]["priority"], "high");

    // Filter by priority=urgent
    let resp = reqwest::get(format!("{base}/api/tasks?priority=urgent"))
        .await
        .unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["title"], "Urgent Priority Task");
    assert_eq!(body[0]["priority"], "urgent");

    // Filter by priority=medium (should return 0 tasks)
    let resp = reqwest::get(format!("{base}/api/tasks?priority=medium"))
        .await
        .unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 0);
}

#[tokio::test]
async fn test_list_tasks_with_filters_multiple_combined() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let bead_id = uuid::Uuid::new_v4();

    // Create diverse tasks
    client
        .post(format!("{base}/api/tasks"))
        .json(&json!({
            "title": "High Priority Feature",
            "bead_id": bead_id,
            "category": "feature",
            "priority": "high",
            "complexity": "medium"
        }))
        .send()
        .await
        .unwrap();

    client
        .post(format!("{base}/api/tasks"))
        .json(&json!({
            "title": "High Priority Bug",
            "bead_id": bead_id,
            "category": "bug_fix",
            "priority": "high",
            "complexity": "small"
        }))
        .send()
        .await
        .unwrap();

    client
        .post(format!("{base}/api/tasks"))
        .json(&json!({
            "title": "Low Priority Feature",
            "bead_id": bead_id,
            "category": "feature",
            "priority": "low",
            "complexity": "trivial"
        }))
        .send()
        .await
        .unwrap();

    // Filter by category=feature AND priority=high (should return 1 task)
    let resp = reqwest::get(format!("{base}/api/tasks?category=feature&priority=high"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["title"], "High Priority Feature");
    assert_eq!(body[0]["category"], "feature");
    assert_eq!(body[0]["priority"], "high");

    // Filter by category=feature AND priority=low (should return 1 task)
    let resp = reqwest::get(format!("{base}/api/tasks?category=feature&priority=low"))
        .await
        .unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["title"], "Low Priority Feature");

    // Filter by priority=high (should return 2 tasks)
    let resp = reqwest::get(format!("{base}/api/tasks?priority=high"))
        .await
        .unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 2);

    // Filter by category=feature AND priority=urgent (should return 0 tasks)
    let resp = reqwest::get(format!("{base}/api/tasks?category=feature&priority=urgent"))
        .await
        .unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 0);

    // Filter by all three: phase=discovery AND category=feature AND priority=high
    let resp = reqwest::get(format!("{base}/api/tasks?phase=discovery&category=feature&priority=high"))
        .await
        .unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["title"], "High Priority Feature");
}

#[tokio::test]
async fn test_list_tasks_with_filters_case_insensitive() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let bead_id = uuid::Uuid::new_v4();

    client
        .post(format!("{base}/api/tasks"))
        .json(&json!({
            "title": "Test Task",
            "bead_id": bead_id,
            "category": "feature",
            "priority": "high",
            "complexity": "medium"
        }))
        .send()
        .await
        .unwrap();

    // Test case-insensitive filtering with various casings
    let test_cases = vec![
        "discovery",
        "Discovery",
        "DISCOVERY",
        "DiScOvErY",
    ];

    for phase_value in test_cases {
        let resp = reqwest::get(format!("{base}/api/tasks?phase={phase_value}"))
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: Vec<Value> = resp.json().await.unwrap();
        assert_eq!(body.len(), 1, "Failed for phase value: {}", phase_value);
        assert_eq!(body[0]["title"], "Test Task");
    }

    // Test category case-insensitivity
    let category_test_cases = vec!["feature", "Feature", "FEATURE", "FEaTuRe"];
    for category_value in category_test_cases {
        let resp = reqwest::get(format!("{base}/api/tasks?category={category_value}"))
            .await
            .unwrap();
        let body: Vec<Value> = resp.json().await.unwrap();
        assert_eq!(body.len(), 1, "Failed for category value: {}", category_value);
    }

    // Test priority case-insensitivity
    let priority_test_cases = vec!["high", "High", "HIGH", "HiGh"];
    for priority_value in priority_test_cases {
        let resp = reqwest::get(format!("{base}/api/tasks?priority={priority_value}"))
            .await
            .unwrap();
        let body: Vec<Value> = resp.json().await.unwrap();
        assert_eq!(body.len(), 1, "Failed for priority value: {}", priority_value);
    }
}

#[tokio::test]
async fn test_list_tasks_with_filters_no_filters_returns_all() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let bead_id = uuid::Uuid::new_v4();

    // Create 5 tasks with various attributes
    for i in 1..=5 {
        client
            .post(format!("{base}/api/tasks"))
            .json(&json!({
                "title": format!("Task {}", i),
                "bead_id": bead_id,
                "category": if i % 2 == 0 { "feature" } else { "bug_fix" },
                "priority": if i <= 2 { "high" } else { "low" },
                "complexity": "medium"
            }))
            .send()
            .await
            .unwrap();
    }

    // No filters - should return all 5 tasks
    let resp = reqwest::get(format!("{base}/api/tasks")).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 5);
}

#[tokio::test]
async fn test_kanban_columns_get_and_patch() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{base}/api/kanban/columns"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let columns = body["columns"].as_array().unwrap();
    assert_eq!(columns.len(), 8);
    assert_eq!(columns[0]["id"], "backlog");
    assert_eq!(columns[0]["label"], "Backlog");
    assert_eq!(columns[7]["id"], "error");
    assert_eq!(columns[7]["label"], "Error");

    let custom = json!({
        "columns": [
            {"id": "backlog", "label": "To Do", "width_px": 200},
            {"id": "done", "label": "Done", "width_px": 150}
        ]
    });
    let resp = client
        .patch(format!("{base}/api/kanban/columns"))
        .json(&custom)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let columns = body["columns"].as_array().unwrap();
    assert_eq!(columns.len(), 2);
    assert_eq!(columns[0]["label"], "To Do");

    let resp = client
        .patch(format!("{base}/api/kanban/columns"))
        .json(&json!({ "columns": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

// ---------------------------------------------------------------------------
// MCP servers endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_mcp_servers_returns_ok() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/mcp/servers"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = resp.json().await.unwrap();
    assert!(!body.is_empty(), "MCP servers list should not be empty");
}

#[tokio::test]
async fn test_list_mcp_servers_response_structure() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/mcp/servers"))
        .await
        .unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();

    // Each server should have name, status, and tools fields
    for server in &body {
        assert!(
            server["name"].is_string(),
            "server should have a name string"
        );
        assert!(
            server["status"].is_string(),
            "server should have a status string"
        );
        assert!(
            server["tools"].is_array(),
            "server should have a tools array"
        );
    }

    // Verify known servers are present
    let names: Vec<&str> = body.iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"Context7"));
    assert!(names.contains(&"Graphiti Memory"));
    assert!(names.contains(&"Filesystem"));
}

#[tokio::test]
async fn test_list_mcp_servers_has_active_and_inactive() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/mcp/servers"))
        .await
        .unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();

    let statuses: Vec<&str> = body.iter().map(|s| s["status"].as_str().unwrap()).collect();
    assert!(
        statuses.contains(&"active"),
        "should have at least one active server"
    );
    assert!(
        statuses.contains(&"inactive"),
        "should have at least one inactive server"
    );
}

// ---------------------------------------------------------------------------
// Worktrees endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_worktrees_returns_ok() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/worktrees")).await.unwrap();
    // This should return 200 since we're in a git repo
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert!(body.is_array(), "worktrees response should be an array");
}

#[tokio::test]
async fn test_list_worktrees_response_structure() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/worktrees")).await.unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();

    // In a git repo, at least one worktree (the main one) should exist
    if !body.is_empty() {
        let entry = &body[0];
        assert!(entry["id"].is_string(), "worktree should have id");
        assert!(entry["path"].is_string(), "worktree should have path");
        assert!(entry["branch"].is_string(), "worktree should have branch");
        assert!(entry["status"].is_string(), "worktree should have status");
        assert_eq!(entry["status"], "active");
    }
}

// ---------------------------------------------------------------------------
// GitHub PRs endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_github_prs_requires_config() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/github/prs"))
        .await
        .unwrap();
    // Without proper config, returns 503 (no token) or 400 (token but no owner/repo)
    assert!(
        resp.status() == 503 || resp.status() == 400,
        "Expected 503 or 400, got {}",
        resp.status()
    );

    let body: Value = resp.json().await.unwrap();
    let err = body["error"].as_str().unwrap();
    assert!(err.contains("token") || err.contains("owner") || err.contains("repo"));
}

#[tokio::test]
async fn test_list_github_prs_accepts_query_params() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!(
        "{base}/api/github/prs?state=open&page=1&per_page=10"
    ))
    .await
    .unwrap();
    // Without proper config, returns 503 (no token) or 400 (token but no owner/repo)
    assert!(
        resp.status() == 503 || resp.status() == 400,
        "Expected 503 or 400, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn test_list_github_prs_invalid_state_param() {
    let (base, _state) = start_test_server().await;

    // Invalid state param should be gracefully ignored
    let resp = reqwest::get(format!("{base}/api/github/prs?state=bogus"))
        .await
        .unwrap();
    // Returns 503 (no token) or 400 (token but no owner/repo)
    assert!(
        resp.status() == 503 || resp.status() == 400,
        "Expected 503 or 400, got {}",
        resp.status()
    );
}

// ---------------------------------------------------------------------------
// Import GitHub issue endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_import_github_issue_requires_config() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/github/issues/42/import"))
        .send()
        .await
        .unwrap();
    // Without proper config, returns 503 (no token) or 400 (token but no owner/repo)
    assert!(
        resp.status() == 503 || resp.status() == 400,
        "Expected 503 or 400, got {}",
        resp.status()
    );

    let body: Value = resp.json().await.unwrap();
    let err = body["error"].as_str().unwrap();
    assert!(err.contains("token") || err.contains("owner") || err.contains("repo"));
}

#[tokio::test]
async fn test_import_github_issue_different_numbers() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Issue number 1 - returns 503 (no token) or 400 (token but no owner/repo)
    let resp = client
        .post(format!("{base}/api/github/issues/1/import"))
        .send()
        .await
        .unwrap();
    assert!(
        resp.status() == 503 || resp.status() == 400,
        "Expected 503 or 400, got {}",
        resp.status()
    );

    // Issue number 9999
    let resp = client
        .post(format!("{base}/api/github/issues/9999/import"))
        .send()
        .await
        .unwrap();
    assert!(
        resp.status() == 503 || resp.status() == 400,
        "Expected 503 or 400, got {}",
        resp.status()
    );
}

// ---------------------------------------------------------------------------
// GitLab & Linear integration endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_gitlab_issues_requires_token_env() {
    let mut cfg = Config::default();
    cfg.integrations.gitlab_token_env = "AT_TEST_MISSING_GITLAB_TOKEN".into();
    cfg.integrations.gitlab_project_id = Some("42".into());
    let (base, _state) = start_test_server_with_config(cfg).await;

    let resp = reqwest::get(format!("{base}/api/gitlab/issues"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 503);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["env_var"], "AT_TEST_MISSING_GITLAB_TOKEN");
}

#[tokio::test]
async fn test_list_gitlab_mrs_requires_project_id_when_not_configured() {
    let mut cfg = Config::default();
    // PATH is guaranteed in test process, so token lookup succeeds and we test project-id validation.
    cfg.integrations.gitlab_token_env = "PATH".into();
    cfg.integrations.gitlab_project_id = None;
    let (base, _state) = start_test_server_with_config(cfg).await;

    let resp = reqwest::get(format!("{base}/api/gitlab/merge-requests"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    let body: Value = resp.json().await.unwrap();
    assert!(
        body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("project ID"),
        "expected project ID error, got: {body}"
    );
}

#[tokio::test]
async fn test_review_gitlab_mr_requires_project_id_when_not_configured() {
    let mut cfg = Config::default();
    cfg.integrations.gitlab_token_env = "PATH".into();
    cfg.integrations.gitlab_project_id = None;
    let (base, _state) = start_test_server_with_config(cfg).await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/gitlab/merge-requests/7/review"))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    let body: Value = resp.json().await.unwrap();
    assert!(
        body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("project ID"),
        "expected project ID error, got: {body}"
    );
}

#[tokio::test]
async fn test_list_linear_issues_requires_token_env() {
    let mut cfg = Config::default();
    cfg.integrations.linear_api_key_env = "AT_TEST_MISSING_LINEAR_API_KEY".into();
    cfg.integrations.linear_team_id = Some("TEAM".into());
    let (base, _state) = start_test_server_with_config(cfg).await;

    let resp = reqwest::get(format!("{base}/api/linear/issues"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 503);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["env_var"], "AT_TEST_MISSING_LINEAR_API_KEY");
}

#[tokio::test]
async fn test_list_linear_issues_requires_team_id_when_not_configured() {
    let mut cfg = Config::default();
    cfg.integrations.linear_api_key_env = "PATH".into();
    cfg.integrations.linear_team_id = None;
    let (base, _state) = start_test_server_with_config(cfg).await;

    let resp = reqwest::get(format!("{base}/api/linear/issues"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    let body: Value = resp.json().await.unwrap();
    assert!(
        body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("team_id"),
        "expected team_id error, got: {body}"
    );
}

// ---------------------------------------------------------------------------
// Stop agent endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_stop_agent_not_found() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let resp = client
        .post(format!("{base}/api/agents/{fake_id}/stop"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "agent not found");
}

#[tokio::test]
async fn test_stop_agent_success() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Inject an agent into state
    let agent = at_core::types::Agent::new(
        "test-agent",
        at_core::types::AgentRole::Crew,
        at_core::types::CliType::Claude,
    );
    let agent_id = agent.id;
    {
        let mut agents = state.agents.write().await;
        agents.insert(agent_id, agent);
    }

    let resp = client
        .post(format!("{base}/api/agents/{agent_id}/stop"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "stopped");
    assert_eq!(body["id"], agent_id.to_string());
}

#[tokio::test]
async fn test_stop_agent_already_stopped() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    let mut agent = at_core::types::Agent::new(
        "stopped-agent",
        at_core::types::AgentRole::Crew,
        at_core::types::CliType::Claude,
    );
    agent.status = at_core::types::AgentStatus::Stopped;
    let agent_id = agent.id;
    {
        let mut agents = state.agents.write().await;
        agents.insert(agent_id, agent);
    }

    // Stopping an already stopped agent should still succeed
    let resp = client
        .post(format!("{base}/api/agents/{agent_id}/stop"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "stopped");
}

#[tokio::test]
async fn test_stop_agent_changes_status_from_active() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    let mut agent = at_core::types::Agent::new(
        "active-agent",
        at_core::types::AgentRole::Crew,
        at_core::types::CliType::Claude,
    );
    agent.status = at_core::types::AgentStatus::Active;
    let agent_id = agent.id;
    {
        let mut agents = state.agents.write().await;
        agents.insert(agent_id, agent);
    }

    let resp = client
        .post(format!("{base}/api/agents/{agent_id}/stop"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "stopped");

    // Verify the agent list reflects the change
    let resp = reqwest::get(format!("{base}/api/agents")).await.unwrap();
    let agents: Vec<Value> = resp.json().await.unwrap();
    let agent_entry = agents
        .iter()
        .find(|a| a["id"] == agent_id.to_string())
        .unwrap();
    assert_eq!(agent_entry["status"], "stopped");
}

// ---------------------------------------------------------------------------
// Costs endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_costs_returns_ok() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/costs")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert!(body["input_tokens"].is_number());
    assert!(body["output_tokens"].is_number());
    assert!(body["sessions"].is_array());
}

#[tokio::test]
async fn test_get_costs_default_values() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/costs")).await.unwrap();
    let body: Value = resp.json().await.unwrap();

    assert_eq!(body["input_tokens"], 0);
    assert_eq!(body["output_tokens"], 0);
    let sessions = body["sessions"].as_array().unwrap();
    assert!(sessions.is_empty());
}

// ---------------------------------------------------------------------------
// Agent sessions endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_agent_sessions_empty() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/sessions")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = resp.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn test_list_agent_sessions_with_agents() {
    let (base, state) = start_test_server().await;

    // Inject agents
    {
        let mut agents = state.agents.write().await;
        let agent1 = at_core::types::Agent::new(
            "session-agent-1",
            at_core::types::AgentRole::Crew,
            at_core::types::CliType::Claude,
        );
        let agent2 = at_core::types::Agent::new(
            "session-agent-2",
            at_core::types::AgentRole::Mayor,
            at_core::types::CliType::Codex,
        );
        agents.insert(agent1.id, agent1);
        agents.insert(agent2.id, agent2);
    }

    let resp = reqwest::get(format!("{base}/api/sessions")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 2);
}

#[tokio::test]
async fn test_list_agent_sessions_response_structure() {
    let (base, state) = start_test_server().await;

    {
        let mut agents = state.agents.write().await;
        let mut agent = at_core::types::Agent::new(
            "struct-agent",
            at_core::types::AgentRole::Crew,
            at_core::types::CliType::Claude,
        );
        agent.session_id = Some("test-session-123".to_string());
        agents.insert(agent.id, agent);
    }

    let resp = reqwest::get(format!("{base}/api/sessions")).await.unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();

    assert_eq!(body.len(), 1);
    let session = &body[0];
    assert!(session["id"].is_string());
    assert_eq!(session["id"], "test-session-123");
    assert_eq!(session["agent_name"], "struct-agent");
    assert!(session["cli_type"].is_string());
    assert!(session["status"].is_string());
    assert!(session["duration"].is_string());
}

// ---------------------------------------------------------------------------
// Convoys endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_convoys_returns_ok() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/convoys")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = resp.json().await.unwrap();
    assert!(body.is_empty(), "convoys should be empty by default");
}

#[tokio::test]
async fn test_list_convoys_response_is_array() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/convoys")).await.unwrap();
    let body: Value = resp.json().await.unwrap();
    assert!(body.is_array());
}

// ---------------------------------------------------------------------------
// Insights sessions messages endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_session_messages_not_found() {
    let (base, _state) = start_test_server().await;

    let fake_id = uuid::Uuid::new_v4();
    let resp = reqwest::get(format!("{base}/api/insights/sessions/{fake_id}/messages"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "session not found");
}

#[tokio::test]
async fn test_get_session_messages_empty_session() {
    let (base, state) = start_test_server().await;

    // Create a session via the engine
    let session_id;
    {
        let mut engine = state.insights_engine.write().await;
        let session = engine.create_session("Test Session", "claude-3");
        session_id = session.id;
    }

    let resp = reqwest::get(format!(
        "{base}/api/insights/sessions/{session_id}/messages"
    ))
    .await
    .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = resp.json().await.unwrap();
    assert!(body.is_empty(), "new session should have no messages");
}

#[tokio::test]
async fn test_get_session_messages_with_messages() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create a session and add a message
    let session_id;
    {
        let mut engine = state.insights_engine.write().await;
        let session = engine.create_session("Chat Session", "claude-3");
        session_id = session.id;
    }

    // Add a message via the POST endpoint
    let resp = client
        .post(format!(
            "{base}/api/insights/sessions/{session_id}/messages"
        ))
        .json(&json!({"content": "Hello from test"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    // Get messages
    let resp = reqwest::get(format!(
        "{base}/api/insights/sessions/{session_id}/messages"
    ))
    .await
    .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["content"], "Hello from test");
    assert_eq!(body[0]["role"], "user");
}

#[tokio::test]
async fn test_add_message_to_nonexistent_session() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let resp = client
        .post(format!("{base}/api/insights/sessions/{fake_id}/messages"))
        .json(&json!({"content": "Hello"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_insight_session_crud_flow() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // List sessions - empty
    let resp = reqwest::get(format!("{base}/api/insights/sessions"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let sessions: Vec<Value> = resp.json().await.unwrap();
    assert!(sessions.is_empty());

    // Create a session
    let resp = client
        .post(format!("{base}/api/insights/sessions"))
        .json(&json!({"title": "My Analysis", "model": "claude-3"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let session: Value = resp.json().await.unwrap();
    let session_id = session["id"].as_str().unwrap();

    // List sessions - should have one
    let resp = reqwest::get(format!("{base}/api/insights/sessions"))
        .await
        .unwrap();
    let sessions: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(sessions.len(), 1);

    // Add messages
    let resp = client
        .post(format!(
            "{base}/api/insights/sessions/{session_id}/messages"
        ))
        .json(&json!({"content": "What is the project about?"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let resp = client
        .post(format!(
            "{base}/api/insights/sessions/{session_id}/messages"
        ))
        .json(&json!({"content": "Follow-up question"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    // Get messages - should have 2
    let resp = reqwest::get(format!(
        "{base}/api/insights/sessions/{session_id}/messages"
    ))
    .await
    .unwrap();
    assert_eq!(resp.status(), 200);
    let messages: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(messages.len(), 2);

    // Delete session
    let resp = client
        .delete(format!("{base}/api/insights/sessions/{session_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Messages should return 404 now
    let resp = reqwest::get(format!(
        "{base}/api/insights/sessions/{session_id}/messages"
    ))
    .await
    .unwrap();
    assert_eq!(resp.status(), 404);
}

// ---------------------------------------------------------------------------
// Queue endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_queue_empty() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/queue")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = resp.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn test_list_queue_with_tasks() {
    let (base, state) = start_test_server().await;

    // Add tasks with different priorities (all in Discovery phase, not started)
    {
        let mut tasks = state.tasks.write().await;
        let t1 = at_core::types::Task::new(
            "Low priority task",
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::Low,
            at_core::types::TaskComplexity::Small,
        );
        let t2 = at_core::types::Task::new(
            "Urgent task",
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::BugFix,
            at_core::types::TaskPriority::Urgent,
            at_core::types::TaskComplexity::Medium,
        );
        let t3 = at_core::types::Task::new(
            "High priority task",
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::High,
            at_core::types::TaskComplexity::Small,
        );
        tasks.insert(t1.id, t1);
        tasks.insert(t2.id, t2);
        tasks.insert(t3.id, t3);
    }

    let resp = reqwest::get(format!("{base}/api/queue")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 3);
    // Should be sorted: Urgent first, then High, then Low
    assert_eq!(body[0]["title"], "Urgent task");
    assert_eq!(body[1]["title"], "High priority task");
    assert_eq!(body[2]["title"], "Low priority task");
    // Position should be 1-indexed
    assert_eq!(body[0]["position"], 1);
    assert_eq!(body[1]["position"], 2);
    assert_eq!(body[2]["position"], 3);
}

#[tokio::test]
async fn test_list_queue_excludes_started_tasks() {
    let (base, state) = start_test_server().await;

    {
        let mut tasks = state.tasks.write().await;
        let mut started_task = at_core::types::Task::new(
            "Started task",
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::High,
            at_core::types::TaskComplexity::Small,
        );
        started_task.started_at = Some(chrono::Utc::now());
        tasks.insert(started_task.id, started_task);

        let queued_task = at_core::types::Task::new(
            "Queued task",
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::Medium,
            at_core::types::TaskComplexity::Small,
        );
        tasks.insert(queued_task.id, queued_task);
    }

    let resp = reqwest::get(format!("{base}/api/queue")).await.unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["title"], "Queued task");
}

#[tokio::test]
async fn test_reorder_queue_empty_ids() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/queue/reorder"))
        .json(&json!({"task_ids": []}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_reorder_queue_not_found() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let resp = client
        .post(format!("{base}/api/queue/reorder"))
        .json(&json!({"task_ids": [fake_id]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_reorder_queue_success() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    let (id1, id2);
    {
        let mut tasks = state.tasks.write().await;
        let t1 = at_core::types::Task::new(
            "Task A",
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::Medium,
            at_core::types::TaskComplexity::Small,
        );
        let t2 = at_core::types::Task::new(
            "Task B",
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::High,
            at_core::types::TaskComplexity::Small,
        );
        id1 = t1.id;
        id2 = t2.id;
        tasks.insert(id1, t1);
        tasks.insert(id2, t2);
    }

    let resp = client
        .post(format!("{base}/api/queue/reorder"))
        .json(&json!({"task_ids": [id2, id1]}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn test_prioritize_task_not_found() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let resp = client
        .post(format!("{base}/api/queue/{fake_id}/prioritize"))
        .json(&json!({"priority": "urgent"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_prioritize_task_success() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    let task_id;
    {
        let mut tasks = state.tasks.write().await;
        let task = at_core::types::Task::new(
            "Priority bump",
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::Low,
            at_core::types::TaskComplexity::Small,
        );
        task_id = task.id;
        tasks.insert(task_id, task);
    }

    let resp = client
        .post(format!("{base}/api/queue/{task_id}/prioritize"))
        .json(&json!({"priority": "urgent"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["priority"], "urgent");
    assert_eq!(body["id"], task_id.to_string());
}

// ---------------------------------------------------------------------------
// Merge endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_merge_worktree_not_found() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/worktrees/nonexistent-branch/merge"))
        .send()
        .await
        .unwrap();
    // The branch won't be found among worktrees
    assert_eq!(resp.status(), 404);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "worktree not found");
}

#[tokio::test]
async fn test_merge_preview_not_found() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!(
        "{base}/api/worktrees/nonexistent-branch/merge-preview"
    ))
    .await
    .unwrap();
    assert_eq!(resp.status(), 404);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "worktree not found");
}

#[tokio::test]
async fn test_resolve_conflict_invalid_strategy() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/worktrees/some-id/resolve"))
        .json(&json!({"strategy": "invalid", "file": "test.rs"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    let body: Value = resp.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("invalid strategy"));
}

#[tokio::test]
async fn test_resolve_conflict_valid_strategies() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    for strategy in &["ours", "theirs", "manual"] {
        let resp = client
            .post(format!("{base}/api/worktrees/test-id/resolve"))
            .json(&json!({"strategy": strategy, "file": "some/path.rs"}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["status"], "resolved");
        assert_eq!(body["strategy"], *strategy);
    }
}

// ---------------------------------------------------------------------------
// Direct mode endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_toggle_direct_mode_enable() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/settings/direct-mode"))
        .json(&json!({"enabled": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["direct_mode"], true);
}

#[tokio::test]
async fn test_toggle_direct_mode_disable() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/settings/direct-mode"))
        .json(&json!({"enabled": false}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["direct_mode"], false);
}

// ---------------------------------------------------------------------------
// CLI availability endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_cli_available_returns_json_array() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/cli/available"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 4, "should return entries for all 4 CLI types");

    // Verify all expected CLI names are present
    let names: Vec<&str> = body.iter().map(|e| e["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"claude"));
    assert!(names.contains(&"codex"));
    assert!(names.contains(&"gemini"));
    assert!(names.contains(&"opencode"));
}

#[tokio::test]
async fn test_list_cli_available_entry_structure() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/cli/available"))
        .await
        .unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();

    for entry in &body {
        assert!(entry["name"].is_string(), "entry should have a name string");
        assert!(
            entry["detected"].is_boolean(),
            "entry should have a detected boolean"
        );
        // path is either null or a string
        assert!(
            entry["path"].is_null() || entry["path"].is_string(),
            "path should be null or string"
        );
    }
}

#[tokio::test]
async fn test_list_cli_available_detected_has_path() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/cli/available"))
        .await
        .unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();

    for entry in &body {
        if entry["detected"].as_bool().unwrap() {
            assert!(
                entry["path"].is_string(),
                "detected CLI should have a path: {:?}",
                entry
            );
        } else {
            assert!(
                entry["path"].is_null(),
                "undetected CLI should have null path: {:?}",
                entry
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Execute pipeline with cli_type tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_execute_pipeline_task_not_found_with_cli_type() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let resp = client
        .post(format!("{base}/api/tasks/{fake_id}/execute"))
        .json(&json!({"cli_type": "codex"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_execute_pipeline_accepts_cli_type_param() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create a task and advance it to a phase that can transition to coding.
    let task_id;
    {
        let mut tasks = state.tasks.write().await;
        let mut task = at_core::types::Task::new(
            "CLI type test",
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::Medium,
            at_core::types::TaskComplexity::Small,
        );
        // Advance through phases so it can enter Coding
        task.set_phase(at_core::types::TaskPhase::ContextGathering);
        task.set_phase(at_core::types::TaskPhase::SpecCreation);
        task.set_phase(at_core::types::TaskPhase::Planning);
        task_id = task.id;
        tasks.insert(task_id, task);
    }

    // Execute with cli_type = codex -- should return 202 Accepted
    let resp = client
        .post(format!("{base}/api/tasks/{task_id}/execute"))
        .json(&json!({"cli_type": "codex"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "started");
}

#[tokio::test]
async fn test_execute_pipeline_without_cli_type_defaults() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    let task_id;
    {
        let mut tasks = state.tasks.write().await;
        let mut task = at_core::types::Task::new(
            "Default CLI test",
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::Medium,
            at_core::types::TaskComplexity::Small,
        );
        task.set_phase(at_core::types::TaskPhase::ContextGathering);
        task.set_phase(at_core::types::TaskPhase::SpecCreation);
        task.set_phase(at_core::types::TaskPhase::Planning);
        task_id = task.id;
        tasks.insert(task_id, task);
    }

    // Execute without body -- should still return 202 (defaults to Claude)
    let resp = client
        .post(format!("{base}/api/tasks/{task_id}/execute"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "started");
}

// ---------------------------------------------------------------------------
// Build log types unit tests
// ---------------------------------------------------------------------------

#[test]
fn test_build_log_entry_serialization() {
    use at_core::types::{BuildLogEntry, BuildStream, TaskPhase};

    let entry = BuildLogEntry {
        timestamp: chrono::Utc::now(),
        stream: BuildStream::Stdout,
        line: "cargo build --release".to_string(),
        phase: TaskPhase::Coding,
    };

    let json_str = serde_json::to_string(&entry).unwrap();
    assert!(json_str.contains("\"stream\":\"stdout\""));
    assert!(json_str.contains("\"line\":\"cargo build --release\""));
    assert!(json_str.contains("\"phase\":\"coding\""));

    // Roundtrip
    let deserialized: BuildLogEntry = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.stream, BuildStream::Stdout);
    assert_eq!(deserialized.line, "cargo build --release");
    assert_eq!(deserialized.phase, TaskPhase::Coding);
}

#[test]
fn test_build_stream_variants() {
    use at_core::types::BuildStream;

    let stdout: BuildStream = serde_json::from_str("\"stdout\"").unwrap();
    assert_eq!(stdout, BuildStream::Stdout);

    let stderr: BuildStream = serde_json::from_str("\"stderr\"").unwrap();
    assert_eq!(stderr, BuildStream::Stderr);
}

#[test]
fn test_task_build_logs_default_empty() {
    let task = at_core::types::Task::new(
        "Build log test",
        uuid::Uuid::new_v4(),
        at_core::types::TaskCategory::Feature,
        at_core::types::TaskPriority::Medium,
        at_core::types::TaskComplexity::Small,
    );
    assert!(task.build_logs.is_empty());
}

#[test]
fn test_task_add_build_log() {
    use at_core::types::BuildStream;

    let mut task = at_core::types::Task::new(
        "Build log test",
        uuid::Uuid::new_v4(),
        at_core::types::TaskCategory::Feature,
        at_core::types::TaskPriority::Medium,
        at_core::types::TaskComplexity::Small,
    );

    task.add_build_log(BuildStream::Stdout, "compiling crate-a");
    task.add_build_log(BuildStream::Stderr, "warning: unused variable");
    task.add_build_log(BuildStream::Stdout, "compiling crate-b");

    assert_eq!(task.build_logs.len(), 3);
    assert_eq!(task.build_logs[0].stream, BuildStream::Stdout);
    assert_eq!(task.build_logs[0].line, "compiling crate-a");
    assert_eq!(task.build_logs[1].stream, BuildStream::Stderr);
    assert_eq!(task.build_logs[1].line, "warning: unused variable");
}

#[test]
fn test_task_with_build_logs_serde_roundtrip() {
    use at_core::types::BuildStream;

    let mut task = at_core::types::Task::new(
        "Serde test",
        uuid::Uuid::new_v4(),
        at_core::types::TaskCategory::Feature,
        at_core::types::TaskPriority::Low,
        at_core::types::TaskComplexity::Trivial,
    );
    task.add_build_log(BuildStream::Stdout, "hello");
    task.add_build_log(BuildStream::Stderr, "err: oops");

    let json_str = serde_json::to_string(&task).unwrap();
    let deserialized: at_core::types::Task = serde_json::from_str(&json_str).unwrap();

    assert_eq!(deserialized.build_logs.len(), 2);
    assert_eq!(deserialized.build_logs[0].line, "hello");
    assert_eq!(deserialized.build_logs[1].stream, BuildStream::Stderr);
}

#[test]
fn test_task_without_build_logs_deserializes_with_default() {
    // Simulate JSON from an older version that lacks the build_logs field.
    let json_str = json!({
        "id": uuid::Uuid::new_v4(),
        "title": "Legacy task",
        "description": null,
        "bead_id": uuid::Uuid::new_v4(),
        "phase": "discovery",
        "progress_percent": 0,
        "subtasks": [],
        "worktree_path": null,
        "git_branch": null,
        "category": "feature",
        "priority": "low",
        "complexity": "small",
        "impact": null,
        "agent_profile": null,
        "phase_configs": [],
        "created_at": "2026-02-21T00:00:00Z",
        "updated_at": "2026-02-21T00:00:00Z",
        "started_at": null,
        "completed_at": null,
        "error": null,
        "logs": [],
        "qa_report": null
    });

    let task: at_core::types::Task = serde_json::from_value(json_str).unwrap();
    assert!(
        task.build_logs.is_empty(),
        "build_logs should default to empty vec"
    );
}

// ---------------------------------------------------------------------------
// Build log API endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_build_logs_empty() {
    let (base, state) = start_test_server().await;

    let task_id;
    {
        let mut tasks = state.tasks.write().await;
        let task = at_core::types::Task::new(
            "Build logs test",
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::Medium,
            at_core::types::TaskComplexity::Small,
        );
        task_id = task.id;
        tasks.insert(task_id, task);
    }

    let resp = reqwest::get(format!("{base}/api/tasks/{task_id}/build-logs"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = resp.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn test_get_build_logs_not_found() {
    let (base, _state) = start_test_server().await;

    let fake_id = uuid::Uuid::new_v4();
    let resp = reqwest::get(format!("{base}/api/tasks/{fake_id}/build-logs"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_get_build_logs_with_entries() {
    let (base, state) = start_test_server().await;
    use at_core::types::BuildStream;

    let task_id;
    {
        let mut tasks = state.tasks.write().await;
        let mut task = at_core::types::Task::new(
            "Build logs populated",
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::Medium,
            at_core::types::TaskComplexity::Small,
        );
        task.add_build_log(BuildStream::Stdout, "Compiling at-core v0.1.0");
        task.add_build_log(BuildStream::Stderr, "warning: unused import");
        task.add_build_log(BuildStream::Stdout, "Finished dev profile");
        task_id = task.id;
        tasks.insert(task_id, task);
    }

    let resp = reqwest::get(format!("{base}/api/tasks/{task_id}/build-logs"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 3);
    assert_eq!(body[0]["stream"], "stdout");
    assert_eq!(body[0]["line"], "Compiling at-core v0.1.0");
    assert_eq!(body[1]["stream"], "stderr");
    assert_eq!(body[1]["line"], "warning: unused import");
    assert_eq!(body[2]["line"], "Finished dev profile");
}

#[tokio::test]
async fn test_get_build_logs_since_filter() {
    let (base, state) = start_test_server().await;
    use at_core::types::BuildStream;

    let task_id;
    let mid_timestamp;
    {
        let mut tasks = state.tasks.write().await;
        let mut task = at_core::types::Task::new(
            "Since filter test",
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::Medium,
            at_core::types::TaskComplexity::Small,
        );
        task.add_build_log(BuildStream::Stdout, "first line");
        // Record a timestamp between entries.
        // Use to_rfc3339_opts with Z suffix to avoid URL-encoding issues with '+'.
        mid_timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true);
        // Small sleep to ensure timestamp difference.
        std::thread::sleep(std::time::Duration::from_millis(10));
        task.add_build_log(BuildStream::Stdout, "second line");
        task_id = task.id;
        tasks.insert(task_id, task);
    }

    let resp = reqwest::get(format!(
        "{base}/api/tasks/{task_id}/build-logs?since={mid_timestamp}"
    ))
    .await
    .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 1, "only the second line should be returned");
    assert_eq!(body[0]["line"], "second line");
}

#[tokio::test]
async fn test_get_build_logs_invalid_since() {
    let (base, state) = start_test_server().await;

    let task_id;
    {
        let mut tasks = state.tasks.write().await;
        let task = at_core::types::Task::new(
            "Invalid since test",
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::Medium,
            at_core::types::TaskComplexity::Small,
        );
        task_id = task.id;
        tasks.insert(task_id, task);
    }

    let resp = reqwest::get(format!(
        "{base}/api/tasks/{task_id}/build-logs?since=not-a-timestamp"
    ))
    .await
    .unwrap();
    assert_eq!(resp.status(), 400);
}

// ---------------------------------------------------------------------------
// Build status endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_build_status_empty() {
    let (base, state) = start_test_server().await;

    let task_id;
    {
        let mut tasks = state.tasks.write().await;
        let task = at_core::types::Task::new(
            "Status test",
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::Medium,
            at_core::types::TaskComplexity::Small,
        );
        task_id = task.id;
        tasks.insert(task_id, task);
    }

    let resp = reqwest::get(format!("{base}/api/tasks/{task_id}/build-status"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["phase"], "discovery");
    assert_eq!(body["total_lines"], 0);
    assert_eq!(body["stdout_lines"], 0);
    assert_eq!(body["stderr_lines"], 0);
    assert_eq!(body["error_count"], 0);
    assert!(body["last_line"].is_null());
}

#[tokio::test]
async fn test_get_build_status_not_found() {
    let (base, _state) = start_test_server().await;

    let fake_id = uuid::Uuid::new_v4();
    let resp = reqwest::get(format!("{base}/api/tasks/{fake_id}/build-status"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_get_build_status_with_logs() {
    let (base, state) = start_test_server().await;
    use at_core::types::BuildStream;

    let task_id;
    {
        let mut tasks = state.tasks.write().await;
        let mut task = at_core::types::Task::new(
            "Status with logs",
            uuid::Uuid::new_v4(),
            at_core::types::TaskCategory::Feature,
            at_core::types::TaskPriority::Medium,
            at_core::types::TaskComplexity::Small,
        );
        task.set_phase(at_core::types::TaskPhase::Coding);
        task.add_build_log(BuildStream::Stdout, "compiling...");
        task.add_build_log(BuildStream::Stderr, "warn: something");
        task.add_build_log(BuildStream::Stderr, "error: failed");
        task.add_build_log(BuildStream::Stdout, "done");
        task_id = task.id;
        tasks.insert(task_id, task);
    }

    let resp = reqwest::get(format!("{base}/api/tasks/{task_id}/build-status"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["phase"], "coding");
    assert_eq!(body["total_lines"], 4);
    assert_eq!(body["stdout_lines"], 2);
    assert_eq!(body["stderr_lines"], 2);
    assert_eq!(body["error_count"], 2);
    assert_eq!(body["last_line"], "done");
    assert!(body["progress_percent"].as_u64().unwrap() > 0);
}
