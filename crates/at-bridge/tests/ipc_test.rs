use at_bridge::event_bus::EventBus;
use at_bridge::ipc::IpcHandler;
use at_bridge::protocol::BridgeMessage;
use std::collections::HashMap;
use uuid::Uuid;

fn make_handler() -> IpcHandler {
    IpcHandler::new_stub(EventBus::new())
}

#[tokio::test]
async fn test_handle_get_status() {
    let handler = make_handler();
    let resp = handler
        .handle_message(BridgeMessage::GetStatus)
        .await
        .unwrap();
    match resp {
        BridgeMessage::StatusUpdate(s) => {
            assert_eq!(s.version, env!("CARGO_PKG_VERSION"));
            // uptime will be near-zero but no longer hardcoded 0
        }
        other => panic!("unexpected response: {:?}", other),
    }
}

#[tokio::test]
async fn test_handle_list_beads() {
    let handler = make_handler();
    let resp = handler
        .handle_message(BridgeMessage::ListBeads { status: None })
        .await
        .unwrap();
    match resp {
        BridgeMessage::BeadList(beads) => assert!(beads.is_empty()),
        other => panic!("unexpected response: {:?}", other),
    }
}

#[tokio::test]
async fn test_handle_list_agents() {
    let handler = make_handler();
    let resp = handler
        .handle_message(BridgeMessage::ListAgents)
        .await
        .unwrap();
    match resp {
        BridgeMessage::AgentList(agents) => assert!(agents.is_empty()),
        other => panic!("unexpected response: {:?}", other),
    }
}

#[tokio::test]
async fn test_handle_get_kpi() {
    let handler = make_handler();
    let resp = handler.handle_message(BridgeMessage::GetKpi).await.unwrap();
    match resp {
        BridgeMessage::KpiUpdate(kpi) => {
            assert_eq!(kpi.total_beads, 0);
            assert_eq!(kpi.active_agents, 0);
        }
        other => panic!("unexpected response: {:?}", other),
    }
}

#[tokio::test]
async fn test_handle_sling_bead_publishes_event() {
    let bus = EventBus::new();
    let rx = bus.subscribe();
    let handler = IpcHandler::new_stub(bus);

    let bead_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let resp = handler
        .handle_message(BridgeMessage::SlingBead { bead_id, agent_id })
        .await
        .unwrap();

    // Response should be an Event.
    assert!(matches!(resp, BridgeMessage::Event(_)));

    // Event bus should have received the same message.
    let published = rx.try_recv().expect("event should have been published");
    assert!(matches!(*published, BridgeMessage::Event(_)));
}

#[tokio::test]
async fn test_handle_hook_bead_publishes_event() {
    let bus = EventBus::new();
    let rx = bus.subscribe();
    let handler = IpcHandler::new_stub(bus);

    let resp = handler
        .handle_message(BridgeMessage::HookBead {
            title: "test bead".into(),
            agent_name: "crew-1".into(),
        })
        .await
        .unwrap();

    assert!(matches!(resp, BridgeMessage::Event(_)));
    assert!(rx.try_recv().is_ok());
}

#[tokio::test]
async fn test_handle_done_bead_publishes_event() {
    let bus = EventBus::new();
    let rx = bus.subscribe();
    let handler = IpcHandler::new_stub(bus);

    let resp = handler
        .handle_message(BridgeMessage::DoneBead {
            bead_id: Uuid::new_v4(),
            failed: false,
        })
        .await
        .unwrap();

    assert!(matches!(resp, BridgeMessage::Event(_)));
    assert!(rx.try_recv().is_ok());
}

#[tokio::test]
async fn test_handle_nudge_agent_publishes_event() {
    let bus = EventBus::new();
    let rx = bus.subscribe();
    let handler = IpcHandler::new_stub(bus);

    let resp = handler
        .handle_message(BridgeMessage::NudgeAgent {
            agent_name: "deacon-1".into(),
            message: "wake up".into(),
        })
        .await
        .unwrap();

    assert!(matches!(resp, BridgeMessage::Event(_)));
    assert!(rx.try_recv().is_ok());
}

#[tokio::test]
async fn test_backend_message_returns_error() {
    let handler = make_handler();

    // Backend-to-frontend messages should not be handled as incoming requests.
    let result = handler
        .handle_message(BridgeMessage::Error {
            code: "TEST".into(),
            message: "test".into(),
        })
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_bead_created_message_returns_error() {
    use at_core::types::{Bead, Lane};

    let handler = make_handler();

    // BeadCreated is a backend-to-frontend message and should not be handled as incoming request.
    let bead = Bead::new("test-bead", Lane::Standard);
    let result = handler
        .handle_message(BridgeMessage::BeadCreated(bead))
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_bead_updated_message_returns_error() {
    use at_core::types::{Bead, Lane};

    let handler = make_handler();

    // BeadUpdated is a backend-to-frontend message and should not be handled as incoming request.
    let bead = Bead::new("test-bead", Lane::Standard);
    let result = handler
        .handle_message(BridgeMessage::BeadUpdated(bead))
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_handle_list_beads_with_populated_data() {
    use at_core::types::{Bead, BeadStatus, Lane};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    let beads = vec![
        Bead::new("bead-1", Lane::Standard),
        {
            let mut b = Bead::new("bead-2", Lane::Standard);
            b.status = BeadStatus::Done;
            b
        },
        {
            let mut b = Bead::new("bead-3", Lane::Standard);
            b.status = BeadStatus::Failed;
            b
        },
    ];
    let beads = Arc::new(RwLock::new(beads.into_iter().map(|b| (b.id, b)).collect::<HashMap<_, _>>()));
    let agents = Arc::new(RwLock::new(HashMap::new()));
    let handler = IpcHandler::new(EventBus::new(), beads, agents, std::time::Instant::now());

    // List all beads
    let resp = handler
        .handle_message(BridgeMessage::ListBeads { status: None })
        .await
        .unwrap();
    match resp {
        BridgeMessage::BeadList(list) => assert_eq!(list.len(), 3),
        other => panic!("unexpected response: {:?}", other),
    }

    // Filter by status
    let resp = handler
        .handle_message(BridgeMessage::ListBeads {
            status: Some("done".to_string()),
        })
        .await
        .unwrap();
    match resp {
        BridgeMessage::BeadList(list) => {
            assert_eq!(list.len(), 1);
            assert_eq!(list[0].title, "bead-2");
        }
        other => panic!("unexpected response: {:?}", other),
    }
}

#[tokio::test]
async fn test_handle_get_status_with_populated_data() {
    use at_core::types::{Bead, BeadStatus, Lane};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    let beads = vec![
        Bead::new("bead-1", Lane::Standard), // Backlog (active)
        {
            let mut b = Bead::new("bead-2", Lane::Standard);
            b.status = BeadStatus::Done; // not active
            b
        },
    ];
    let beads = Arc::new(RwLock::new(beads.into_iter().map(|b| (b.id, b)).collect::<HashMap<_, _>>()));
    let agents = Arc::new(RwLock::new(HashMap::new()));
    let handler = IpcHandler::new(EventBus::new(), beads, agents, std::time::Instant::now());

    let resp = handler
        .handle_message(BridgeMessage::GetStatus)
        .await
        .unwrap();
    match resp {
        BridgeMessage::StatusUpdate(s) => {
            assert_eq!(s.beads_active, 1); // only the Backlog bead
            assert_eq!(s.agents_active, 0);
        }
        other => panic!("unexpected response: {:?}", other),
    }
}

#[tokio::test]
async fn test_handle_get_kpi_with_populated_data() {
    use at_core::types::{Bead, BeadStatus, Lane};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    let beads = vec![
        Bead::new("b1", Lane::Standard), // Backlog
        {
            let mut b = Bead::new("b2", Lane::Standard);
            b.status = BeadStatus::Hooked;
            b
        },
        {
            let mut b = Bead::new("b3", Lane::Standard);
            b.status = BeadStatus::Slung;
            b
        },
        {
            let mut b = Bead::new("b4", Lane::Standard);
            b.status = BeadStatus::Done;
            b
        },
        {
            let mut b = Bead::new("b5", Lane::Standard);
            b.status = BeadStatus::Failed;
            b
        },
    ];
    let beads = Arc::new(RwLock::new(beads.into_iter().map(|b| (b.id, b)).collect::<HashMap<_, _>>()));
    let agents = Arc::new(RwLock::new(HashMap::new()));
    let handler = IpcHandler::new(EventBus::new(), beads, agents, std::time::Instant::now());

    let resp = handler.handle_message(BridgeMessage::GetKpi).await.unwrap();
    match resp {
        BridgeMessage::KpiUpdate(kpi) => {
            assert_eq!(kpi.total_beads, 5);
            assert_eq!(kpi.backlog, 1);
            assert_eq!(kpi.hooked, 1);
            assert_eq!(kpi.slung, 1);
            assert_eq!(kpi.done, 1);
            assert_eq!(kpi.failed, 1);
        }
        other => panic!("unexpected response: {:?}", other),
    }
}
