//! Cross-crate API integration tests.
//!
//! These tests exercise higher-level workflows across the bridge layer:
//! bead creation -> KPI reflection, agent lifecycle, settings persistence,
//! and event bus propagation through the HTTP API.

use std::sync::Arc;
use std::time::Duration;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, ApiState};
use at_bridge::protocol::BridgeMessage;
use at_core::settings::SettingsManager;
use at_core::types::{Agent, AgentRole, CliType};
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Spin up an API server on a random port, return the base URL and shared state.
async fn start_test_server() -> (String, Arc<ApiState>) {
    let event_bus = EventBus::new();
    let mut api_state = ApiState::new(event_bus);
    // Use an isolated settings file per test server to avoid cross-test races.
    let settings_path = std::env::temp_dir()
        .join(format!("at-bridge-api-test-{}", uuid::Uuid::new_v4()))
        .join("settings.toml");
    api_state.settings_manager = Arc::new(SettingsManager::new(settings_path));
    let state = Arc::new(api_state);
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

fn task_payload(bead_id: &str) -> Value {
    json!({
        "title": "Cross-crate test task",
        "bead_id": bead_id,
        "category": "feature",
        "priority": "medium",
        "complexity": "small"
    })
}

// ===========================================================================
// Bead creation -> KPI reflection
// ===========================================================================

#[tokio::test]
async fn test_create_bead_appears_in_status_count() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Check initial status.
    let resp = client
        .get(format!("{base}/api/status"))
        .send()
        .await
        .unwrap();
    let status: Value = resp.json().await.unwrap();
    assert_eq!(status["bead_count"], 0);

    // Create a bead.
    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&json!({ "title": "KPI test bead" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    // Status should now show 1 bead.
    let resp = client
        .get(format!("{base}/api/status"))
        .send()
        .await
        .unwrap();
    let status: Value = resp.json().await.unwrap();
    assert_eq!(status["bead_count"], 1);
}

#[tokio::test]
async fn test_create_multiple_beads_reflected_in_status() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    for i in 0..5 {
        let resp = client
            .post(format!("{base}/api/beads"))
            .json(&json!({ "title": format!("bead-{i}") }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201);
    }

    let resp = client
        .get(format!("{base}/api/status"))
        .send()
        .await
        .unwrap();
    let status: Value = resp.json().await.unwrap();
    assert_eq!(status["bead_count"], 5);
}

#[tokio::test]
async fn test_bead_status_transitions_full_lifecycle() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create bead (starts as backlog).
    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&json!({ "title": "Lifecycle bead" }))
        .send()
        .await
        .unwrap();
    let bead: Value = resp.json().await.unwrap();
    let id = bead["id"].as_str().unwrap();
    assert_eq!(bead["status"], "backlog");

    // backlog -> hooked
    let resp = client
        .post(format!("{base}/api/beads/{id}/status"))
        .json(&json!({ "status": "hooked" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // hooked -> slung
    let resp = client
        .post(format!("{base}/api/beads/{id}/status"))
        .json(&json!({ "status": "slung" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // slung -> review
    let resp = client
        .post(format!("{base}/api/beads/{id}/status"))
        .json(&json!({ "status": "review" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // review -> done
    let resp = client
        .post(format!("{base}/api/beads/{id}/status"))
        .json(&json!({ "status": "done" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Verify final state.
    let resp = client
        .get(format!("{base}/api/beads"))
        .send()
        .await
        .unwrap();
    let beads: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(beads[0]["status"], "done");
}

// ===========================================================================
// Agent lifecycle (create -> status -> stop)
// ===========================================================================

#[tokio::test]
async fn test_agent_lifecycle_via_state_injection() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Inject an agent directly into state.
    let agent = Agent::new("test-worker", AgentRole::Crew, CliType::Claude);
    let agent_id = agent.id;
    state.agents.write().await.push(agent);

    // List agents.
    let resp = client
        .get(format!("{base}/api/agents"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let agents: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0]["name"], "test-worker");
    assert_eq!(agents[0]["status"], "pending");

    // Nudge the agent.
    let resp = client
        .post(format!("{base}/api/agents/{agent_id}/nudge"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Stop the agent.
    let resp = client
        .post(format!("{base}/api/agents/{agent_id}/stop"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let stopped: Value = resp.json().await.unwrap();
    assert_eq!(stopped["status"], "stopped");

    // Verify the status endpoint reflects the agent.
    let resp = client
        .get(format!("{base}/api/status"))
        .send()
        .await
        .unwrap();
    let status: Value = resp.json().await.unwrap();
    assert_eq!(status["agent_count"], 1);
}

#[tokio::test]
async fn test_agent_nudge_not_found() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();
    let fake_id = uuid::Uuid::new_v4();

    let resp = client
        .post(format!("{base}/api/agents/{fake_id}/nudge"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_agent_stop_not_found() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();
    let fake_id = uuid::Uuid::new_v4();

    let resp = client
        .post(format!("{base}/api/agents/{fake_id}/stop"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// ===========================================================================
// Settings persistence (save -> reload -> verify)
// ===========================================================================

#[tokio::test]
async fn test_settings_put_and_get_cycle() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // GET default settings.
    let resp = client
        .get(format!("{base}/api/settings"))
        .send()
        .await
        .unwrap();
    let defaults: Value = resp.json().await.unwrap();

    // PUT modified settings back.
    let resp = client
        .put(format!("{base}/api/settings"))
        .json(&defaults)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // GET again and verify round-trip.
    let resp = client
        .get(format!("{base}/api/settings"))
        .send()
        .await
        .unwrap();
    let reloaded: Value = resp.json().await.unwrap();
    assert_eq!(defaults["agents"], reloaded["agents"]);
    assert_eq!(defaults["cache"], reloaded["cache"]);
}

#[tokio::test]
async fn test_settings_patch_preserves_other_fields() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // GET initial settings to know the cache section.
    let resp = client
        .get(format!("{base}/api/settings"))
        .send()
        .await
        .unwrap();
    let initial: Value = resp.json().await.unwrap();
    let initial_cache = initial["cache"].clone();

    // PATCH only the agents section.
    let resp = client
        .patch(format!("{base}/api/settings"))
        .json(&json!({ "agents": { "max_concurrent": 42 } }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let patched: Value = resp.json().await.unwrap();

    // Cache section should be preserved.
    assert_eq!(patched["cache"], initial_cache);
    assert_eq!(patched["agents"]["max_concurrent"], 42);
}

// ===========================================================================
// Event bus propagation (action -> event received via WS)
// ===========================================================================

#[tokio::test]
async fn test_bead_creation_publishes_event_on_bus() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Subscribe to events BEFORE creating a bead.
    let rx = state.event_bus.subscribe();

    // Create a bead (the handler publishes a BeadCreated event).
    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&json!({ "title": "Event test bead" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    // Give time for event propagation.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Drain events and find the BeadCreated message.
    let mut found_bead_created = false;
    while let Ok(msg) = rx.try_recv() {
        if matches!(&*msg, BridgeMessage::BeadCreated(_)) {
            found_bead_created = true;
        }
    }
    assert!(
        found_bead_created,
        "creating a bead should publish BeadCreated event"
    );
}

#[tokio::test]
async fn test_task_update_publishes_event_on_bus() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create a task first.
    let resp = client
        .post(format!("{base}/api/tasks"))
        .json(&json!({
            "title": "Event task",
            "bead_id": uuid::Uuid::new_v4(),
            "category": "feature",
            "priority": "medium",
            "complexity": "small"
        }))
        .send()
        .await
        .unwrap();
    let created: Value = resp.json().await.unwrap();
    let task_id = created["id"].as_str().unwrap();

    // Subscribe BEFORE update.
    let rx = state.event_bus.subscribe();

    // Update the task.
    let resp = client
        .put(format!("{base}/api/tasks/{task_id}"))
        .json(&json!({ "title": "Updated event task" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut found_task_update = false;
    while let Ok(msg) = rx.try_recv() {
        if matches!(&*msg, BridgeMessage::TaskUpdate(_)) {
            found_task_update = true;
        }
    }
    assert!(
        found_task_update,
        "updating a task should publish TaskUpdate event"
    );
}

#[tokio::test]
async fn test_event_bus_multiple_subscribers() {
    let event_bus = EventBus::new();

    let rx1 = event_bus.subscribe();
    let rx2 = event_bus.subscribe();

    assert_eq!(event_bus.subscriber_count(), 2);

    event_bus.publish(BridgeMessage::GetStatus);

    // Both should receive the message.
    let msg1 = rx1.try_recv();
    let msg2 = rx2.try_recv();
    assert!(msg1.is_ok());
    assert!(msg2.is_ok());
}

#[tokio::test]
async fn test_event_bus_dropped_subscriber_pruned() {
    let event_bus = EventBus::new();

    let rx1 = event_bus.subscribe();
    let _rx2 = event_bus.subscribe();
    assert_eq!(event_bus.subscriber_count(), 2);

    drop(rx1);

    // Publishing will prune the dropped subscriber.
    event_bus.publish(BridgeMessage::GetStatus);
    assert_eq!(event_bus.subscriber_count(), 1);
}

// ===========================================================================
// Bead + Task cross-entity workflow
// ===========================================================================

#[tokio::test]
async fn test_create_bead_then_task_linked_by_bead_id() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create a bead.
    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&json!({ "title": "Parent bead" }))
        .send()
        .await
        .unwrap();
    let bead: Value = resp.json().await.unwrap();
    let bead_id = bead["id"].as_str().unwrap();

    // Create a task linked to the bead.
    let resp = client
        .post(format!("{base}/api/tasks"))
        .json(&task_payload(bead_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let task: Value = resp.json().await.unwrap();
    assert_eq!(task["bead_id"], bead_id);
}

// ===========================================================================
// Kanban columns
// ===========================================================================

#[tokio::test]
async fn test_kanban_columns_default_and_update() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // GET default columns (8 columns).
    let resp = client
        .get(format!("{base}/api/kanban/columns"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let columns = body["columns"].as_array().unwrap();
    assert_eq!(columns.len(), 8);

    // PATCH to custom columns.
    let custom = json!({
        "columns": [
            {"id": "todo", "label": "To Do"},
            {"id": "doing", "label": "Doing"},
            {"id": "done", "label": "Done"}
        ]
    });
    let resp = client
        .patch(format!("{base}/api/kanban/columns"))
        .json(&custom)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // GET should reflect the update.
    let resp = client
        .get(format!("{base}/api/kanban/columns"))
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    let columns = body["columns"].as_array().unwrap();
    assert_eq!(columns.len(), 3);
    assert_eq!(columns[0]["label"], "To Do");
}

// ===========================================================================
// Notification integration
// ===========================================================================

#[tokio::test]
async fn test_notification_store_add_and_read_via_api() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Add a notification via state injection.
    let notif_id;
    {
        let mut store = state.notification_store.write().await;
        notif_id = store.add(
            "Integration Alert",
            "Cross-crate test",
            at_bridge::notifications::NotificationLevel::Warning,
            "api_test",
        );
    }

    // Count endpoint should show 1 unread.
    let resp = client
        .get(format!("{base}/api/notifications/count"))
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["unread"], 1);
    assert_eq!(body["total"], 1);

    // Mark as read.
    let resp = client
        .post(format!("{base}/api/notifications/{notif_id}/read"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Now 0 unread.
    let resp = client
        .get(format!("{base}/api/notifications/count"))
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["unread"], 0);
    assert_eq!(body["total"], 1);
}

// ===========================================================================
// Notification generation from BeadCreated/BeadUpdated events
// ===========================================================================

#[tokio::test]
async fn test_bead_created_event_generates_notification() {
    use at_bridge::notifications::{notification_from_event, NotificationLevel};
    use at_core::types::{Bead, Lane};

    // Create a BeadCreated event.
    let bead = Bead::new("Test Bead", Lane::Standard);
    let bead_id = bead.id;
    let event = BridgeMessage::BeadCreated(bead);

    // Convert to notification.
    let result = notification_from_event(&event);

    // Verify notification is generated.
    assert!(
        result.is_some(),
        "BeadCreated event should generate a notification"
    );

    let (title, message, level, source, action_url) = result.unwrap();
    assert_eq!(title, "Bead Created");
    assert_eq!(message, "Created bead: Test Bead");
    assert_eq!(level, NotificationLevel::Success);
    assert_eq!(source, "system");
    assert_eq!(action_url, Some(format!("/beads/{}", bead_id)));
}

#[tokio::test]
async fn test_bead_updated_event_generates_notification() {
    use at_bridge::notifications::{notification_from_event, NotificationLevel};
    use at_core::types::{Bead, Lane};

    // Create a BeadUpdated event.
    let bead = Bead::new("Updated Bead", Lane::Critical);
    let bead_id = bead.id;
    let event = BridgeMessage::BeadUpdated(bead);

    // Convert to notification.
    let result = notification_from_event(&event);

    // Verify notification is generated.
    assert!(
        result.is_some(),
        "BeadUpdated event should generate a notification"
    );

    let (title, message, level, source, action_url) = result.unwrap();
    assert_eq!(title, "Bead Updated");
    assert_eq!(message, "Updated bead: Updated Bead");
    assert_eq!(level, NotificationLevel::Info);
    assert_eq!(source, "system");
    assert_eq!(action_url, Some(format!("/beads/{}", bead_id)));
}

#[tokio::test]
async fn test_bead_creation_flow_with_notification() {
    use at_bridge::notifications::notification_from_event;

    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Subscribe to events BEFORE creating a bead.
    let rx = state.event_bus.subscribe();

    // Create a bead.
    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&json!({ "title": "Notification test bead" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    // Give time for event propagation.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Find the BeadCreated event and verify notification can be generated.
    let mut found_valid_notification = false;
    while let Ok(msg) = rx.try_recv() {
        if let BridgeMessage::BeadCreated(_) = &*msg {
            let notification = notification_from_event(&msg);
            assert!(
                notification.is_some(),
                "BeadCreated event should generate notification"
            );
            let (title, _, _, _, _) = notification.unwrap();
            assert_eq!(title, "Bead Created");
            found_valid_notification = true;
        }
    }
    assert!(
        found_valid_notification,
        "Should have received and validated BeadCreated notification"
    );
}

#[tokio::test]
async fn test_bead_update_flow_with_notification() {
    use at_bridge::notifications::notification_from_event;

    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create a bead first.
    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&json!({ "title": "Update notification bead" }))
        .send()
        .await
        .unwrap();
    let bead: Value = resp.json().await.unwrap();
    let id = bead["id"].as_str().unwrap();

    // Subscribe to events BEFORE updating.
    let rx = state.event_bus.subscribe();

    // Update bead status.
    let resp = client
        .post(format!("{base}/api/beads/{id}/status"))
        .json(&json!({ "status": "hooked" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Give time for event propagation.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Find the BeadUpdated event and verify notification can be generated.
    let mut found_valid_notification = false;
    while let Ok(msg) = rx.try_recv() {
        if let BridgeMessage::BeadUpdated(_) = &*msg {
            let notification = notification_from_event(&msg);
            assert!(
                notification.is_some(),
                "BeadUpdated event should generate notification"
            );
            let (title, _, _, _, _) = notification.unwrap();
            assert_eq!(title, "Bead Updated");
            found_valid_notification = true;
        }
    }
    assert!(
        found_valid_notification,
        "Should have received and validated BeadUpdated notification"
    );
}
