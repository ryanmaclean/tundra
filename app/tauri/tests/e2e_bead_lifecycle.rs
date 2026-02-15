//! End-to-end integration test: full bead lifecycle through the stack.
//!
//! Tests the flow: Seed agents → Hook bead → Sling → Done
//! Exercises: at-core (types, cache), at-tauri (commands, state), at-bridge (event bus)

use at_core::types::{Agent, AgentRole, AgentStatus, CliType, BeadStatus, Lane};
use at_tauri::commands;
use at_tauri::state::AppState;
use chrono::Utc;
use uuid::Uuid;

/// Helper: seed an agent into the cache so commands that look up by name succeed.
async fn seed_agent(state: &AppState, name: &str, role: AgentRole, cli: CliType) {
    let agent = Agent {
        id: Uuid::new_v4(),
        name: name.to_string(),
        role,
        cli_type: cli,
        model: Some("claude-sonnet-4".to_string()),
        status: AgentStatus::Active,
        rig: Some("test-rig".to_string()),
        pid: Some(12345),
        session_id: Some("sess-001".to_string()),
        created_at: Utc::now(),
        last_seen: Utc::now(),
        metadata: None,
    };
    state.cache.upsert_agent(&agent).await.expect("seed agent");
}

#[tokio::test]
async fn test_full_bead_lifecycle_through_tauri_commands() {
    let state = AppState::with_in_memory_cache().await;

    // 1. Verify empty state
    let status = commands::get_status(&state).await.unwrap();
    assert_eq!(status.beads_total, 0);
    assert_eq!(status.agents_active, 0);

    // 2. Seed agents (required for hook/sling)
    seed_agent(&state, "mayor-1", AgentRole::Mayor, CliType::Claude).await;
    seed_agent(&state, "polecat-1", AgentRole::Polecat, CliType::Codex).await;

    // 3. Hook a bead (creates it in Hooked state)
    let hooked = commands::hook_bead(
        &state,
        "Fix authentication bug".to_string(),
        "mayor-1".to_string(),
        Some("critical".to_string()),
    )
    .await
    .unwrap();
    assert_eq!(hooked.status, BeadStatus::Hooked);
    assert_eq!(hooked.title, "Fix authentication bug");
    assert_eq!(hooked.lane, Lane::Critical);

    // 4. Verify it appears in status
    let status = commands::get_status(&state).await.unwrap();
    assert_eq!(status.beads_total, 1);

    // 5. List all beads - should have 1
    let all_beads = commands::list_beads(&state, None).await.unwrap();
    assert_eq!(all_beads.len(), 1);
    assert_eq!(all_beads[0].title, "Fix authentication bug");

    // 6. List beads filtered by status
    let hooked_beads = commands::list_beads(&state, Some("hooked".to_string()))
        .await
        .unwrap();
    assert_eq!(hooked_beads.len(), 1);
    let backlog_beads = commands::list_beads(&state, Some("backlog".to_string()))
        .await
        .unwrap();
    assert_eq!(backlog_beads.len(), 0);

    // 7. Sling the bead to an agent
    let slung = commands::sling_bead(
        &state,
        hooked.id.to_string(),
        "polecat-1".to_string(),
    )
    .await
    .unwrap();
    assert_eq!(slung.status, BeadStatus::Slung);

    // 8. Move to Review
    let reviewed = commands::review_bead(&state, slung.id.to_string())
        .await
        .unwrap();
    assert_eq!(reviewed.status, BeadStatus::Review);

    // 9. Complete the bead (Review -> Done)
    let done = commands::done_bead(&state, reviewed.id.to_string(), false)
        .await
        .unwrap();
    assert_eq!(done.status, BeadStatus::Done);

    // 9. Verify KPI snapshot reflects completion
    let kpi = commands::get_kpi(&state).await.unwrap();
    assert_eq!(kpi.done, 1);
    assert_eq!(kpi.total_beads, 1);
}

#[tokio::test]
async fn test_bead_failure_lifecycle() {
    let state = AppState::with_in_memory_cache().await;
    seed_agent(&state, "crew-1", AgentRole::Crew, CliType::Claude).await;

    let hooked = commands::hook_bead(
        &state,
        "Refactor database layer".to_string(),
        "crew-1".to_string(),
        None,
    )
    .await
    .unwrap();

    let slung = commands::sling_bead(&state, hooked.id.to_string(), "crew-1".to_string())
        .await
        .unwrap();
    assert_eq!(slung.status, BeadStatus::Slung);

    let failed = commands::done_bead(&state, slung.id.to_string(), true)
        .await
        .unwrap();
    assert_eq!(failed.status, BeadStatus::Failed);

    let kpi = commands::get_kpi(&state).await.unwrap();
    assert_eq!(kpi.failed, 1);
    assert_eq!(kpi.done, 0);
}

#[tokio::test]
async fn test_multiple_beads_across_lanes() {
    let state = AppState::with_in_memory_cache().await;
    seed_agent(&state, "mayor-1", AgentRole::Mayor, CliType::Claude).await;
    seed_agent(&state, "crew-1", AgentRole::Crew, CliType::Codex).await;
    seed_agent(&state, "polecat-1", AgentRole::Polecat, CliType::Gemini).await;

    let critical = commands::hook_bead(
        &state,
        "Critical fix".to_string(),
        "mayor-1".to_string(),
        Some("critical".to_string()),
    )
    .await
    .unwrap();

    let _standard = commands::hook_bead(
        &state,
        "Standard task".to_string(),
        "crew-1".to_string(),
        None,
    )
    .await
    .unwrap();

    let experimental = commands::hook_bead(
        &state,
        "Experiment".to_string(),
        "polecat-1".to_string(),
        Some("experimental".to_string()),
    )
    .await
    .unwrap();

    let all = commands::list_beads(&state, None).await.unwrap();
    assert_eq!(all.len(), 3);

    // Complete critical: sling -> review -> done
    commands::sling_bead(&state, critical.id.to_string(), "mayor-1".to_string())
        .await
        .unwrap();
    commands::review_bead(&state, critical.id.to_string())
        .await
        .unwrap();
    commands::done_bead(&state, critical.id.to_string(), false)
        .await
        .unwrap();

    // Fail experimental: sling -> failed (Slung->Failed is valid)
    commands::sling_bead(
        &state,
        experimental.id.to_string(),
        "polecat-1".to_string(),
    )
    .await
    .unwrap();
    commands::done_bead(&state, experimental.id.to_string(), true)
        .await
        .unwrap();

    let kpi = commands::get_kpi(&state).await.unwrap();
    assert_eq!(kpi.total_beads, 3);
    assert_eq!(kpi.done, 1);
    assert_eq!(kpi.failed, 1);
    assert_eq!(kpi.hooked, 1);
}

#[tokio::test]
async fn test_bridge_event_bus_integration() {
    use at_bridge::event_bus::EventBus;
    use at_bridge::protocol::BridgeMessage;

    let bus = EventBus::new();
    let rx = bus.subscribe();

    bus.publish(BridgeMessage::GetStatus);
    let msg = rx.recv_async().await.unwrap();
    assert!(matches!(msg, BridgeMessage::GetStatus));

    bus.publish(BridgeMessage::NudgeAgent {
        agent_name: "mayor-1".to_string(),
        message: "Check backlog".to_string(),
    });
    let msg = rx.recv_async().await.unwrap();
    assert!(matches!(msg, BridgeMessage::NudgeAgent { .. }));
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

    sm.transition(AgentEvent::Pause).unwrap();
    assert_eq!(sm.state(), AgentState::Paused);

    sm.transition(AgentEvent::Resume).unwrap();
    assert_eq!(sm.state(), AgentState::Active);

    sm.transition(AgentEvent::Stop).unwrap();
    assert_eq!(sm.state(), AgentState::Stopping);

    sm.transition(AgentEvent::Stop).unwrap();
    assert_eq!(sm.state(), AgentState::Stopped);

    assert!(sm.history().len() >= 6);
}

#[tokio::test]
async fn test_get_nonexistent_bead_returns_not_found() {
    let state = AppState::with_in_memory_cache().await;
    let fake_id = Uuid::new_v4().to_string();
    let result = commands::get_bead(&state, fake_id).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_invalid_bead_transition_rejected() {
    let state = AppState::with_in_memory_cache().await;
    seed_agent(&state, "crew-1", AgentRole::Crew, CliType::Claude).await;

    let hooked = commands::hook_bead(
        &state,
        "Test bead".to_string(),
        "crew-1".to_string(),
        None,
    )
    .await
    .unwrap();

    // Try to mark as done directly from hooked (should fail — must sling first)
    let result = commands::done_bead(&state, hooked.id.to_string(), false).await;
    assert!(result.is_err());
}
