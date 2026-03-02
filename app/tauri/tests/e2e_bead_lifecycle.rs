//! End-to-end integration tests for the embedded daemon architecture.
//!
//! Tests that the daemon can start in embedded mode and serve API requests.

use at_core::config::Config;
use at_daemon::daemon::Daemon;

#[tokio::test]
async fn test_daemon_start_embedded_returns_port() {
    let mut config = Config::default();
    config.cache.path = ":memory:".to_string();

    let daemon = Daemon::new(config).await.expect("create daemon");
    let port = daemon.start_embedded().await.expect("start embedded");

    // Port must be non-zero (OS-assigned)
    assert!(port > 0, "expected a non-zero port, got {port}");

    // API should be reachable
    let url = format!("http://localhost:{port}/api/status");
    let resp = reqwest::get(&url).await;
    assert!(resp.is_ok(), "API server should be reachable at {url}");
    let resp = resp.unwrap();
    assert_eq!(resp.status(), 200);

    daemon.shutdown();
}

#[tokio::test]
async fn test_daemon_embedded_different_ports() {
    // Two daemons should get different ports.
    let mut config1 = Config::default();
    config1.cache.path = ":memory:".to_string();
    let mut config2 = Config::default();
    config2.cache.path = ":memory:".to_string();

    let daemon1 = Daemon::new(config1).await.expect("create daemon1");
    let daemon2 = Daemon::new(config2).await.expect("create daemon2");

    let port1 = daemon1.start_embedded().await.expect("start embedded 1");
    let port2 = daemon2.start_embedded().await.expect("start embedded 2");

    assert_ne!(port1, port2, "two daemons must get different ports");

    daemon1.shutdown();
    daemon2.shutdown();
}

#[tokio::test]
async fn test_bridge_event_bus_integration() {
    use at_bridge::event_bus::EventBus;
    use at_bridge::protocol::BridgeMessage;

    let bus = EventBus::new();
    let rx = bus.subscribe();

    bus.publish(BridgeMessage::GetStatus);
    let msg = rx.recv_async().await.unwrap();
    assert!(matches!(*msg, BridgeMessage::GetStatus));
}

#[tokio::test]
async fn test_agent_state_machine_lifecycle() {
    use at_agents::state_machine::{AgentEvent, AgentState, AgentStateMachine};

    let mut sm = AgentStateMachine::new();
    assert_eq!(sm.state(), AgentState::Idle);

    sm.transition(AgentEvent::Start).unwrap();
    assert_eq!(sm.state(), AgentState::Spawning);

    sm.transition(AgentEvent::Spawned).unwrap();
    assert_eq!(sm.state(), AgentState::Active);

    sm.transition(AgentEvent::Stop).unwrap();
    assert_eq!(sm.state(), AgentState::Stopping);

    sm.transition(AgentEvent::Stop).unwrap();
    assert_eq!(sm.state(), AgentState::Stopped);
}

#[tokio::test]
async fn test_e2e_bead_creation_and_notification() {
    use at_bridge::event_bus::EventBus;
    use at_bridge::protocol::BridgeMessage;
    use at_core::types::BeadStatus;

    // Create daemon with in-memory storage
    let mut config = Config::default();
    config.cache.path = ":memory:".to_string();

    let daemon = Daemon::new(config).await.expect("create daemon");
    let port = daemon.start_embedded().await.expect("start embedded");

    // Subscribe to event bus to monitor BeadCreated and BeadUpdated events
    let event_bus = daemon.event_bus();
    let rx = event_bus.subscribe();

    let client = reqwest::Client::new();
    let base_url = format!("http://localhost:{port}");

    // Step 1: Create bead via API (simulating Tauri IPC)
    let create_payload = serde_json::json!({
        "title": "E2E Test Task",
        "description": "Testing end-to-end bead lifecycle"
    });
    let resp = client
        .post(&format!("{base_url}/api/beads"))
        .json(&create_payload)
        .send()
        .await
        .expect("create bead");
    assert_eq!(resp.status(), 201, "bead creation should return 201 Created");

    let bead: serde_json::Value = resp.json().await.expect("parse created bead");
    let bead_id = bead["id"].as_str().expect("bead id").to_string();
    assert_eq!(bead["title"], "E2E Test Task");
    assert_eq!(bead["status"], "Pending");

    // Step 2: Verify bead appears in list (simulating UI query)
    let resp = client
        .get(&format!("{base_url}/api/beads"))
        .send()
        .await
        .expect("list beads");
    assert_eq!(resp.status(), 200);
    let beads: Vec<serde_json::Value> = resp.json().await.expect("parse beads");
    assert_eq!(beads.len(), 1, "should have 1 bead after creation");
    assert_eq!(beads[0]["id"], bead_id);

    // Drain BeadCreated event from event bus
    let msg = rx.recv_async().await.expect("receive BeadCreated event");
    assert!(
        matches!(&*msg, BridgeMessage::BeadCreated(_)),
        "should receive BeadCreated event"
    );

    // Step 3: Mark bead as Done (simulating UI action)
    let update_payload = serde_json::json!({
        "status": "Done"
    });
    let resp = client
        .patch(&format!("{base_url}/api/beads/{bead_id}"))
        .json(&update_payload)
        .send()
        .await
        .expect("update bead status");
    assert!(
        resp.status().is_success(),
        "bead status update should succeed"
    );

    let updated_bead: serde_json::Value = resp.json().await.expect("parse updated bead");
    assert_eq!(updated_bead["id"], bead_id);
    assert_eq!(updated_bead["status"], "Done");

    // Step 4: Verify BeadUpdated event is published (this triggers notification)
    let msg = rx.recv_async().await.expect("receive BeadUpdated event");
    if let BridgeMessage::BeadUpdated(bead) = msg.as_ref() {
        assert_eq!(bead.id.to_string(), bead_id);
        assert_eq!(bead.status, BeadStatus::Done);
        assert_eq!(bead.title, "E2E Test Task");
        // When status is Done, the notification listener would send a notification
        // with title "Bead Completed" and body "✓ E2E Test Task"
    } else {
        panic!("expected BeadUpdated event after status change");
    }

    daemon.shutdown();
}
