use at_bridge::protocol::*;
use uuid::Uuid;

/// Helper: serialize a message to JSON and deserialize it back, asserting the
/// round-trip produces an equivalent value (via Debug representation).
fn roundtrip(msg: &BridgeMessage) {
    let json = serde_json::to_string(msg).expect("serialize");
    let back: BridgeMessage = serde_json::from_str(&json).expect("deserialize");
    // Compare Debug output as a simple structural equality check.
    assert_eq!(format!("{:?}", msg), format!("{:?}", back));
}

#[test]
fn test_get_status_roundtrip() {
    roundtrip(&BridgeMessage::GetStatus);
}

#[test]
fn test_list_beads_with_status_roundtrip() {
    roundtrip(&BridgeMessage::ListBeads {
        status: Some("backlog".into()),
    });
}

#[test]
fn test_list_beads_without_status_roundtrip() {
    roundtrip(&BridgeMessage::ListBeads { status: None });
}

#[test]
fn test_list_agents_roundtrip() {
    roundtrip(&BridgeMessage::ListAgents);
}

#[test]
fn test_sling_bead_roundtrip() {
    roundtrip(&BridgeMessage::SlingBead {
        bead_id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
    });
}

#[test]
fn test_hook_bead_roundtrip() {
    roundtrip(&BridgeMessage::HookBead {
        title: "Fix the widget".into(),
        agent_name: "deacon-1".into(),
    });
}

#[test]
fn test_done_bead_roundtrip() {
    roundtrip(&BridgeMessage::DoneBead {
        bead_id: Uuid::new_v4(),
        failed: false,
    });
    roundtrip(&BridgeMessage::DoneBead {
        bead_id: Uuid::new_v4(),
        failed: true,
    });
}

#[test]
fn test_nudge_agent_roundtrip() {
    roundtrip(&BridgeMessage::NudgeAgent {
        agent_name: "crew-3".into(),
        message: "hurry up".into(),
    });
}

#[test]
fn test_get_kpi_roundtrip() {
    roundtrip(&BridgeMessage::GetKpi);
}

#[test]
fn test_status_update_roundtrip() {
    roundtrip(&BridgeMessage::StatusUpdate(StatusPayload {
        version: "0.1.0".into(),
        uptime_seconds: 42,
        agents_active: 3,
        beads_active: 7,
    }));
}

#[test]
fn test_bead_list_roundtrip() {
    roundtrip(&BridgeMessage::BeadList(Vec::new()));
}

#[test]
fn test_agent_list_roundtrip() {
    roundtrip(&BridgeMessage::AgentList(Vec::new()));
}

#[test]
fn test_kpi_update_roundtrip() {
    roundtrip(&BridgeMessage::KpiUpdate(KpiPayload {
        total_beads: 100,
        backlog: 20,
        hooked: 10,
        slung: 30,
        review: 15,
        done: 20,
        failed: 5,
        active_agents: 4,
    }));
}

#[test]
fn test_agent_output_roundtrip() {
    roundtrip(&BridgeMessage::AgentOutput {
        agent_id: Uuid::new_v4(),
        output: "hello world".into(),
    });
}

#[test]
fn test_error_roundtrip() {
    roundtrip(&BridgeMessage::Error {
        code: "NOT_FOUND".into(),
        message: "bead not found".into(),
    });
}

#[test]
fn test_event_roundtrip() {
    roundtrip(&BridgeMessage::Event(EventPayload {
        event_type: "bead_hooked".into(),
        agent_id: Some(Uuid::new_v4()),
        bead_id: Some(Uuid::new_v4()),
        message: "something happened".into(),
        timestamp: chrono::Utc::now(),
    }));
}

#[test]
fn test_json_uses_snake_case_tags() {
    let json = serde_json::to_value(&BridgeMessage::GetStatus).unwrap();
    assert_eq!(json["type"], "get_status");

    let json = serde_json::to_value(&BridgeMessage::ListBeads { status: None }).unwrap();
    assert_eq!(json["type"], "list_beads");
}
