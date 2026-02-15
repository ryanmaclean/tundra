use at_core::types::{Agent, AgentRole, CliType};
use at_tauri::bridge::BridgeManager;
use at_tauri::commands::*;
use at_tauri::state::AppState;

async fn make_state() -> AppState {
    AppState::with_in_memory_cache().await
}

/// Seed a test agent into the cache so bead commands can reference it.
async fn seed_agent(state: &AppState, name: &str) -> Agent {
    let agent = Agent::new(name, AgentRole::Crew, CliType::Claude);
    state.cache.upsert_agent(&agent).await.unwrap();
    agent
}

#[tokio::test]
async fn test_get_status_empty() {
    let state = make_state().await;
    let resp = get_status(&state).await.unwrap();
    assert_eq!(resp.agents_active, 0);
    assert_eq!(resp.beads_total, 0);
    assert_eq!(resp.beads_active, 0);
}

#[tokio::test]
async fn test_hook_and_list_beads() {
    let state = make_state().await;
    seed_agent(&state, "alpha").await;

    let bead = hook_bead(&state, "Task A".into(), "alpha".into(), None)
        .await
        .unwrap();
    assert_eq!(bead.title, "Task A");

    let all = list_beads(&state, None).await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].title, "Task A");

    let hooked = list_beads(&state, Some("hooked".into())).await.unwrap();
    assert_eq!(hooked.len(), 1);
}

#[tokio::test]
async fn test_sling_bead() {
    let state = make_state().await;
    seed_agent(&state, "beta").await;

    let bead = hook_bead(&state, "Task B".into(), "beta".into(), None)
        .await
        .unwrap();

    let slung = sling_bead(&state, bead.id.to_string(), "beta".into())
        .await
        .unwrap();
    assert_eq!(slung.status, at_core::types::BeadStatus::Slung);
    assert!(slung.slung_at.is_some());
}

#[tokio::test]
async fn test_done_bead() {
    let state = make_state().await;
    seed_agent(&state, "gamma").await;

    // hook -> sling -> done (via review would be the normal path, but
    // Slung -> Failed is a valid transition per BeadStatus::can_transition_to)
    let bead = hook_bead(&state, "Task C".into(), "gamma".into(), None)
        .await
        .unwrap();
    let slung = sling_bead(&state, bead.id.to_string(), "gamma".into())
        .await
        .unwrap();

    // Mark as failed (Slung -> Failed is valid).
    let done = done_bead(&state, slung.id.to_string(), true)
        .await
        .unwrap();
    assert_eq!(done.status, at_core::types::BeadStatus::Failed);
    assert!(done.done_at.is_some());
}

#[tokio::test]
async fn test_list_agents_empty() {
    let state = make_state().await;
    let agents = list_agents(&state).await.unwrap();
    assert!(agents.is_empty());
}

#[tokio::test]
async fn test_bridge_subscribe_publish() {
    let bus = at_bridge::event_bus::EventBus::new();
    let mgr = BridgeManager::new(bus);

    let rx = mgr.subscribe();
    assert_eq!(mgr.subscriber_count(), 1);

    mgr.publish(at_bridge::protocol::BridgeMessage::GetStatus)
        .unwrap();

    let msg = rx.recv().unwrap();
    assert!(matches!(msg, at_bridge::protocol::BridgeMessage::GetStatus));
}
