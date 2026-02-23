use at_agents::state_machine::{AgentEvent, AgentState, AgentStateMachine};

#[test]
fn valid_idle_to_spawning_to_active() {
    let mut sm = AgentStateMachine::new();
    assert_eq!(sm.state(), AgentState::Idle);

    let s = sm.transition(AgentEvent::Start).unwrap();
    assert_eq!(s, AgentState::Spawning);
    assert_eq!(sm.state(), AgentState::Spawning);

    let s = sm.transition(AgentEvent::Spawned).unwrap();
    assert_eq!(s, AgentState::Active);
    assert_eq!(sm.state(), AgentState::Active);
}

#[test]
fn invalid_idle_to_active() {
    let mut sm = AgentStateMachine::new();
    let result = sm.transition(AgentEvent::Spawned);
    assert!(result.is_err());
    // State should remain Idle after a rejected transition.
    assert_eq!(sm.state(), AgentState::Idle);
}

#[test]
fn full_lifecycle_idle_to_stopped() {
    let mut sm = AgentStateMachine::new();

    sm.transition(AgentEvent::Start).unwrap(); // Idle -> Spawning
    sm.transition(AgentEvent::Spawned).unwrap(); // Spawning -> Active
    sm.transition(AgentEvent::Stop).unwrap(); // Active -> Stopping
    let s = sm.transition(AgentEvent::Stop).unwrap(); // Stopping -> Stopped
    assert_eq!(s, AgentState::Stopped);

    assert_eq!(sm.history().len(), 4);
}

#[test]
fn failure_and_recovery() {
    let mut sm = AgentStateMachine::new();

    sm.transition(AgentEvent::Start).unwrap(); // Idle -> Spawning
    sm.transition(AgentEvent::Spawned).unwrap(); // Spawning -> Active
    sm.transition(AgentEvent::Fail).unwrap(); // Active -> Failed
    assert_eq!(sm.state(), AgentState::Failed);

    sm.transition(AgentEvent::Recover).unwrap(); // Failed -> Idle
    assert_eq!(sm.state(), AgentState::Idle);
}

#[test]
fn pause_and_resume() {
    let mut sm = AgentStateMachine::new();

    sm.transition(AgentEvent::Start).unwrap();
    sm.transition(AgentEvent::Spawned).unwrap();
    sm.transition(AgentEvent::Pause).unwrap();
    assert_eq!(sm.state(), AgentState::Paused);

    sm.transition(AgentEvent::Resume).unwrap();
    assert_eq!(sm.state(), AgentState::Active);
}

#[test]
fn can_transition_checks() {
    let sm = AgentStateMachine::new();
    assert!(sm.can_transition(AgentEvent::Start));
    assert!(!sm.can_transition(AgentEvent::Spawned));
    assert!(!sm.can_transition(AgentEvent::Stop));
}

#[test]
fn spawning_fail_goes_to_failed() {
    let mut sm = AgentStateMachine::new();
    sm.transition(AgentEvent::Start).unwrap();
    let s = sm.transition(AgentEvent::Fail).unwrap();
    assert_eq!(s, AgentState::Failed);
}

#[test]
fn paused_stop_goes_to_stopping() {
    let mut sm = AgentStateMachine::new();
    sm.transition(AgentEvent::Start).unwrap();
    sm.transition(AgentEvent::Spawned).unwrap();
    sm.transition(AgentEvent::Pause).unwrap();
    let s = sm.transition(AgentEvent::Stop).unwrap();
    assert_eq!(s, AgentState::Stopping);
}
