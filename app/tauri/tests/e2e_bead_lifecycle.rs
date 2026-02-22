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
