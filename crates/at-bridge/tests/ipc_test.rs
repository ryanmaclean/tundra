use at_bridge::event_bus::EventBus;
use at_bridge::ipc::IpcHandler;
use at_bridge::protocol::BridgeMessage;
use uuid::Uuid;

fn make_handler() -> IpcHandler {
    IpcHandler::new(EventBus::new())
}

#[test]
fn test_handle_get_status() {
    let handler = make_handler();
    let resp = handler.handle_message(BridgeMessage::GetStatus).unwrap();
    match resp {
        BridgeMessage::StatusUpdate(s) => {
            assert_eq!(s.version, env!("CARGO_PKG_VERSION"));
            assert_eq!(s.uptime_seconds, 0);
        }
        other => panic!("unexpected response: {:?}", other),
    }
}

#[test]
fn test_handle_list_beads() {
    let handler = make_handler();
    let resp = handler
        .handle_message(BridgeMessage::ListBeads { status: None })
        .unwrap();
    match resp {
        BridgeMessage::BeadList(beads) => assert!(beads.is_empty()),
        other => panic!("unexpected response: {:?}", other),
    }
}

#[test]
fn test_handle_list_agents() {
    let handler = make_handler();
    let resp = handler
        .handle_message(BridgeMessage::ListAgents)
        .unwrap();
    match resp {
        BridgeMessage::AgentList(agents) => assert!(agents.is_empty()),
        other => panic!("unexpected response: {:?}", other),
    }
}

#[test]
fn test_handle_get_kpi() {
    let handler = make_handler();
    let resp = handler.handle_message(BridgeMessage::GetKpi).unwrap();
    match resp {
        BridgeMessage::KpiUpdate(kpi) => {
            assert_eq!(kpi.total_beads, 0);
            assert_eq!(kpi.active_agents, 0);
        }
        other => panic!("unexpected response: {:?}", other),
    }
}

#[test]
fn test_handle_sling_bead_publishes_event() {
    let bus = EventBus::new();
    let rx = bus.subscribe();
    let handler = IpcHandler::new(bus);

    let bead_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let resp = handler
        .handle_message(BridgeMessage::SlingBead { bead_id, agent_id })
        .unwrap();

    // Response should be an Event.
    assert!(matches!(resp, BridgeMessage::Event(_)));

    // Event bus should have received the same message.
    let published = rx.try_recv().expect("event should have been published");
    assert!(matches!(published, BridgeMessage::Event(_)));
}

#[test]
fn test_handle_hook_bead_publishes_event() {
    let bus = EventBus::new();
    let rx = bus.subscribe();
    let handler = IpcHandler::new(bus);

    let resp = handler
        .handle_message(BridgeMessage::HookBead {
            title: "test bead".into(),
            agent_name: "crew-1".into(),
        })
        .unwrap();

    assert!(matches!(resp, BridgeMessage::Event(_)));
    assert!(rx.try_recv().is_ok());
}

#[test]
fn test_handle_done_bead_publishes_event() {
    let bus = EventBus::new();
    let rx = bus.subscribe();
    let handler = IpcHandler::new(bus);

    let resp = handler
        .handle_message(BridgeMessage::DoneBead {
            bead_id: Uuid::new_v4(),
            failed: false,
        })
        .unwrap();

    assert!(matches!(resp, BridgeMessage::Event(_)));
    assert!(rx.try_recv().is_ok());
}

#[test]
fn test_handle_nudge_agent_publishes_event() {
    let bus = EventBus::new();
    let rx = bus.subscribe();
    let handler = IpcHandler::new(bus);

    let resp = handler
        .handle_message(BridgeMessage::NudgeAgent {
            agent_name: "deacon-1".into(),
            message: "wake up".into(),
        })
        .unwrap();

    assert!(matches!(resp, BridgeMessage::Event(_)));
    assert!(rx.try_recv().is_ok());
}

#[test]
fn test_backend_message_returns_error() {
    let handler = make_handler();

    // Backend-to-frontend messages should not be handled as incoming requests.
    let result = handler.handle_message(BridgeMessage::Error {
        code: "TEST".into(),
        message: "test".into(),
    });
    assert!(result.is_err());
}
