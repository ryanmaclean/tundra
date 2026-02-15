use at_core::types::*;

#[test]
fn bead_status_valid_transitions() {
    assert!(BeadStatus::Backlog.can_transition_to(&BeadStatus::Hooked));
    assert!(BeadStatus::Hooked.can_transition_to(&BeadStatus::Slung));
    assert!(BeadStatus::Hooked.can_transition_to(&BeadStatus::Backlog));
    assert!(BeadStatus::Slung.can_transition_to(&BeadStatus::Review));
    assert!(BeadStatus::Slung.can_transition_to(&BeadStatus::Failed));
    assert!(BeadStatus::Slung.can_transition_to(&BeadStatus::Escalated));
    assert!(BeadStatus::Review.can_transition_to(&BeadStatus::Done));
    assert!(BeadStatus::Review.can_transition_to(&BeadStatus::Slung));
    assert!(BeadStatus::Review.can_transition_to(&BeadStatus::Failed));
    assert!(BeadStatus::Failed.can_transition_to(&BeadStatus::Backlog));
    assert!(BeadStatus::Escalated.can_transition_to(&BeadStatus::Backlog));
}

#[test]
fn bead_status_invalid_transitions() {
    assert!(!BeadStatus::Backlog.can_transition_to(&BeadStatus::Done));
    assert!(!BeadStatus::Done.can_transition_to(&BeadStatus::Backlog));
    assert!(!BeadStatus::Hooked.can_transition_to(&BeadStatus::Review));
    assert!(!BeadStatus::Review.can_transition_to(&BeadStatus::Hooked));
}

#[test]
fn bead_creation() {
    let bead = Bead::new("test task", Lane::Standard);
    assert_eq!(bead.title, "test task");
    assert_eq!(bead.status, BeadStatus::Backlog);
    assert_eq!(bead.lane, Lane::Standard);
    assert_eq!(bead.priority, 0);
    assert!(bead.description.is_none());
    assert!(bead.agent_id.is_none());
}

#[test]
fn agent_status_glyph() {
    assert_eq!(AgentStatus::Active.glyph(), "@");
    assert_eq!(AgentStatus::Idle.glyph(), "*");
    assert_eq!(AgentStatus::Pending.glyph(), "!");
    assert_eq!(AgentStatus::Unknown.glyph(), "?");
    assert_eq!(AgentStatus::Stopped.glyph(), "x");
}

#[test]
fn lane_ordering() {
    assert!(Lane::Experimental < Lane::Standard);
    assert!(Lane::Standard < Lane::Critical);
    assert!(Lane::Experimental < Lane::Critical);
}

#[test]
fn serialization_roundtrip() {
    let bead = Bead::new("roundtrip", Lane::Critical);
    let json = serde_json::to_string(&bead).expect("serialize");
    let back: Bead = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.title, "roundtrip");
    assert_eq!(back.lane, Lane::Critical);
    assert_eq!(back.status, BeadStatus::Backlog);

    let agent = Agent::new("agent-1", AgentRole::Mayor, CliType::Claude);
    let json = serde_json::to_string(&agent).expect("serialize agent");
    let back: Agent = serde_json::from_str(&json).expect("deserialize agent");
    assert_eq!(back.name, "agent-1");
    assert_eq!(back.role, AgentRole::Mayor);
    assert_eq!(back.cli_type, CliType::Claude);
}
