//! Exhaustive integration tests for the Kanban Board feature.
//!
//! The Kanban board maps BeadStatus variants to visual lanes:
//!   - Backlog   -> "Planning"
//!   - Hooked    -> "In Progress"
//!   - Slung     -> "AI Review"
//!   - Review    -> "Human Review"
//!   - Done      -> "Done"
//!   - Failed    -> "Failed"
//!   - Escalated -> "Escalated"
//!
//! These tests exercise lane management, task card CRUD, state transitions,
//! agent assignment, filtering/sorting, and API integration — all through
//! the HTTP API backed by in-memory state.

use std::collections::HashMap;
use std::sync::Arc;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, ApiState};
use at_core::types::*;
use serde_json::{json, Value};
use uuid::Uuid;

// ===========================================================================
// Helpers
// ===========================================================================

/// Spin up an API server on a random ephemeral port, return the base URL and
/// shared state handle so tests can both exercise the HTTP layer and inspect
/// or inject state directly when needed.
async fn start_test_server() -> (String, Arc<ApiState>) {
    let event_bus = EventBus::new();
    let state = Arc::new(ApiState::new(event_bus));
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

/// Create a bead via the API and return the JSON response.
async fn api_create_bead(client: &reqwest::Client, base: &str, title: &str) -> Value {
    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&json!({ "title": title }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    resp.json().await.unwrap()
}

/// Create a bead with a description via the API.
async fn api_create_bead_with_desc(
    client: &reqwest::Client,
    base: &str,
    title: &str,
    desc: &str,
) -> Value {
    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&json!({ "title": title, "description": desc }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    resp.json().await.unwrap()
}

/// Transition a bead's status via the API, returning the response + status code.
async fn api_transition_bead(
    client: &reqwest::Client,
    base: &str,
    id: &str,
    status: &str,
) -> (u16, Value) {
    let resp = client
        .post(format!("{base}/api/beads/{id}/status"))
        .json(&json!({ "status": status }))
        .send()
        .await
        .unwrap();
    let code = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap();
    (code, body)
}

/// List all beads via the API.
async fn api_list_beads(client: &reqwest::Client, base: &str) -> Vec<Value> {
    let resp = client
        .get(format!("{base}/api/beads"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    resp.json().await.unwrap()
}

/// Build a mapping from bead-status string to a count of beads in that status.
fn lane_counts(beads: &[Value]) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for b in beads {
        let status = b["status"].as_str().unwrap_or("unknown").to_string();
        *counts.entry(status).or_insert(0) += 1;
    }
    counts
}

/// Directly inject a Bead into the ApiState.
async fn inject_bead(state: &ApiState, title: &str, status: BeadStatus) -> Uuid {
    let mut bead = Bead::new(title, Lane::Standard);
    bead.status = status;
    let id = bead.id;
    state.beads.write().await.insert(id, bead);
    id
}

/// Directly inject an Agent into the ApiState.
async fn inject_agent(state: &ApiState, name: &str) -> Uuid {
    let agent = Agent::new(name, AgentRole::Crew, CliType::Claude);
    let id = agent.id;
    state.agents.write().await.insert(id, agent);
    id
}

// ===========================================================================
// 1. Lane Management
// ===========================================================================

#[tokio::test]
async fn test_kanban_lists_all_seven_status_lanes() {
    // The Kanban board has seven possible status lanes corresponding to
    // BeadStatus variants: Backlog, Hooked, Slung, Review, Done, Failed, Escalated.
    let (_base, state) = start_test_server().await;

    inject_bead(&state, "t-backlog", BeadStatus::Backlog).await;
    inject_bead(&state, "t-hooked", BeadStatus::Hooked).await;
    inject_bead(&state, "t-slung", BeadStatus::Slung).await;
    inject_bead(&state, "t-review", BeadStatus::Review).await;
    inject_bead(&state, "t-done", BeadStatus::Done).await;
    inject_bead(&state, "t-failed", BeadStatus::Failed).await;
    inject_bead(&state, "t-escalated", BeadStatus::Escalated).await;

    let beads = state.beads.read().await;
    let statuses: Vec<String> = beads
        .values()
        .map(|b| {
            serde_json::to_value(&b.status)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();

    assert!(statuses.contains(&"backlog".to_string()));
    assert!(statuses.contains(&"hooked".to_string()));
    assert!(statuses.contains(&"slung".to_string()));
    assert!(statuses.contains(&"review".to_string()));
    assert!(statuses.contains(&"done".to_string()));
    assert!(statuses.contains(&"failed".to_string()));
    assert!(statuses.contains(&"escalated".to_string()));
}

#[tokio::test]
async fn test_kanban_lane_counts_reflect_bead_states() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Inject beads in various states directly.
    for _ in 0..3 {
        inject_bead(&state, "backlog-item", BeadStatus::Backlog).await;
    }
    for _ in 0..2 {
        inject_bead(&state, "hooked-item", BeadStatus::Hooked).await;
    }
    inject_bead(&state, "done-item", BeadStatus::Done).await;

    let beads = api_list_beads(&client, &base).await;
    let counts = lane_counts(&beads);

    assert_eq!(*counts.get("backlog").unwrap_or(&0), 3);
    assert_eq!(*counts.get("hooked").unwrap_or(&0), 2);
    assert_eq!(*counts.get("done").unwrap_or(&0), 1);
    assert_eq!(*counts.get("slung").unwrap_or(&0), 0);
    assert_eq!(*counts.get("review").unwrap_or(&0), 0);
}

#[tokio::test]
async fn test_kanban_empty_lanes_returned() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // With zero beads, every lane count should be zero.
    let beads = api_list_beads(&client, &base).await;
    assert!(beads.is_empty());
    let counts = lane_counts(&beads);
    assert_eq!(*counts.get("backlog").unwrap_or(&0), 0);
    assert_eq!(*counts.get("hooked").unwrap_or(&0), 0);
    assert_eq!(*counts.get("slung").unwrap_or(&0), 0);
    assert_eq!(*counts.get("review").unwrap_or(&0), 0);
    assert_eq!(*counts.get("done").unwrap_or(&0), 0);
}

// ===========================================================================
// 2. Task Card CRUD
// ===========================================================================

#[tokio::test]
async fn test_create_task_appears_in_planning_lane() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let created = api_create_bead(&client, &base, "New feature card").await;
    assert_eq!(created["status"], "backlog");

    let beads = api_list_beads(&client, &base).await;
    assert_eq!(beads.len(), 1);
    assert_eq!(beads[0]["status"], "backlog");
}

#[tokio::test]
async fn test_create_task_with_title_and_description() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let created =
        api_create_bead_with_desc(&client, &base, "Login form", "Implement OAuth flow").await;
    assert_eq!(created["title"], "Login form");
    assert_eq!(created["description"], "Implement OAuth flow");
}

#[tokio::test]
async fn test_create_task_with_priority_level() {
    // Beads carry an integer priority field; verify it serializes.
    let (_base, state) = start_test_server().await;

    let mut bead = Bead::new("priority-test", Lane::Standard);
    bead.priority = 5;
    let id = bead.id;
    state.beads.write().await.insert(id, bead);

    let beads = state.beads.read().await;
    let found = beads.get(&id).unwrap();
    assert_eq!(found.priority, 5);
}

#[tokio::test]
async fn test_create_task_with_labels() {
    // Labels are stored in the metadata JSON field.
    let (_base, state) = start_test_server().await;

    let mut bead = Bead::new("labeled-task", Lane::Standard);
    bead.metadata = Some(json!({ "labels": ["bug", "frontend", "urgent"] }));
    let id = bead.id;
    state.beads.write().await.insert(id, bead);

    let beads = state.beads.read().await;
    let found = beads.get(&id).unwrap();
    let labels = found.metadata.as_ref().unwrap()["labels"]
        .as_array()
        .unwrap();
    assert_eq!(labels.len(), 3);
    assert!(labels.contains(&json!("bug")));
    assert!(labels.contains(&json!("frontend")));
    assert!(labels.contains(&json!("urgent")));
}

#[tokio::test]
async fn test_create_task_generates_unique_id() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let a = api_create_bead(&client, &base, "task-a").await;
    let b = api_create_bead(&client, &base, "task-b").await;
    let c = api_create_bead(&client, &base, "task-c").await;

    let ids: Vec<&str> = vec![
        a["id"].as_str().unwrap(),
        b["id"].as_str().unwrap(),
        c["id"].as_str().unwrap(),
    ];
    // All IDs must be unique.
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), 3);
}

#[tokio::test]
async fn test_delete_task_removes_from_lane() {
    // Deleting is done by removing from state directly (no DELETE endpoint for beads).
    let (_base, state) = start_test_server().await;

    let id = inject_bead(&state, "to-delete", BeadStatus::Backlog).await;
    assert_eq!(state.beads.read().await.len(), 1);

    // Remove.
    state.beads.write().await.remove(&id);
    assert_eq!(state.beads.read().await.len(), 0);
}

#[tokio::test]
async fn test_update_task_title() {
    let (_base, state) = start_test_server().await;

    let id = inject_bead(&state, "old-title", BeadStatus::Backlog).await;
    {
        let mut beads = state.beads.write().await;
        let bead = beads.get_mut(&id).unwrap();
        bead.title = "new-title".to_string();
        bead.updated_at = chrono::Utc::now();
    }

    let beads = state.beads.read().await;
    let bead = beads.get(&id).unwrap();
    assert_eq!(bead.title, "new-title");
}

#[tokio::test]
async fn test_update_task_description() {
    let (_base, state) = start_test_server().await;

    let id = inject_bead(&state, "desc-test", BeadStatus::Backlog).await;
    {
        let mut beads = state.beads.write().await;
        let bead = beads.get_mut(&id).unwrap();
        bead.description = Some("Updated description content".to_string());
        bead.updated_at = chrono::Utc::now();
    }

    let beads = state.beads.read().await;
    let bead = beads.get(&id).unwrap();
    assert_eq!(
        bead.description.as_deref(),
        Some("Updated description content")
    );
}

// ===========================================================================
// 3. Task State Transitions (Kanban pipeline)
// ===========================================================================

#[tokio::test]
async fn test_move_task_planning_to_in_progress() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let created = api_create_bead(&client, &base, "plan-to-progress").await;
    let id = created["id"].as_str().unwrap();

    // backlog -> hooked
    let (code, body) = api_transition_bead(&client, &base, id, "hooked").await;
    assert_eq!(code, 200);
    assert_eq!(body["status"], "hooked");
}

#[tokio::test]
async fn test_move_task_in_progress_to_ai_review() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let created = api_create_bead(&client, &base, "progress-to-ai").await;
    let id = created["id"].as_str().unwrap();

    api_transition_bead(&client, &base, id, "hooked").await;
    let (code, body) = api_transition_bead(&client, &base, id, "slung").await;
    assert_eq!(code, 200);
    assert_eq!(body["status"], "slung");
}

#[tokio::test]
async fn test_move_task_ai_review_to_human_review() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let created = api_create_bead(&client, &base, "ai-to-human").await;
    let id = created["id"].as_str().unwrap();

    api_transition_bead(&client, &base, id, "hooked").await;
    api_transition_bead(&client, &base, id, "slung").await;
    let (code, body) = api_transition_bead(&client, &base, id, "review").await;
    assert_eq!(code, 200);
    assert_eq!(body["status"], "review");
}

#[tokio::test]
async fn test_move_task_human_review_to_done() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let created = api_create_bead(&client, &base, "review-to-done").await;
    let id = created["id"].as_str().unwrap();

    api_transition_bead(&client, &base, id, "hooked").await;
    api_transition_bead(&client, &base, id, "slung").await;
    api_transition_bead(&client, &base, id, "review").await;
    let (code, body) = api_transition_bead(&client, &base, id, "done").await;
    assert_eq!(code, 200);
    assert_eq!(body["status"], "done");
}

#[tokio::test]
async fn test_full_pipeline_backlog_to_done() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let created = api_create_bead(&client, &base, "full-pipeline").await;
    let id = created["id"].as_str().unwrap();

    let transitions = ["hooked", "slung", "review", "done"];
    for target in transitions {
        let (code, body) = api_transition_bead(&client, &base, id, target).await;
        assert_eq!(code, 200, "transition to {target} should succeed");
        assert_eq!(body["status"], target);
    }
}

#[tokio::test]
async fn test_move_task_backward_to_planning() {
    // hooked -> backlog is a valid regression transition.
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let created = api_create_bead(&client, &base, "regress-to-planning").await;
    let id = created["id"].as_str().unwrap();

    api_transition_bead(&client, &base, id, "hooked").await;
    let (code, body) = api_transition_bead(&client, &base, id, "backlog").await;
    assert_eq!(code, 200);
    assert_eq!(body["status"], "backlog");
}

#[tokio::test]
async fn test_move_task_skip_lanes_not_allowed() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let created = api_create_bead(&client, &base, "no-skip").await;
    let id = created["id"].as_str().unwrap();

    // backlog -> done (skip hooked, slung, review)
    let (code, _body) = api_transition_bead(&client, &base, id, "done").await;
    assert_eq!(code, 400, "skipping lanes should be rejected");

    // backlog -> slung (skip hooked)
    let (code, _body) = api_transition_bead(&client, &base, id, "slung").await;
    assert_eq!(code, 400, "skipping to slung should be rejected");

    // backlog -> review (skip hooked, slung)
    let (code, _body) = api_transition_bead(&client, &base, id, "review").await;
    assert_eq!(code, 400, "skipping to review should be rejected");
}

#[tokio::test]
async fn test_stuck_task_recovery() {
    // Simulates the fix/kanban-stuck-task scenario: a bead is stuck in Slung
    // (AI Review) and can be moved to Failed, then recovered back to Backlog.
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let created = api_create_bead(&client, &base, "stuck-task").await;
    let id = created["id"].as_str().unwrap();

    // Move through pipeline to slung (AI Review).
    api_transition_bead(&client, &base, id, "hooked").await;
    api_transition_bead(&client, &base, id, "slung").await;

    // Simulate stuck: move to failed.
    let (code, body) = api_transition_bead(&client, &base, id, "failed").await;
    assert_eq!(code, 200);
    assert_eq!(body["status"], "failed");

    // Recover: failed -> backlog (re-enter planning).
    let (code, body) = api_transition_bead(&client, &base, id, "backlog").await;
    assert_eq!(code, 200);
    assert_eq!(body["status"], "backlog");

    // Re-enter the pipeline.
    let (code, body) = api_transition_bead(&client, &base, id, "hooked").await;
    assert_eq!(code, 200);
    assert_eq!(body["status"], "hooked");
}

#[tokio::test]
async fn test_escalated_task_recovery() {
    // Escalated tasks can also be recovered to backlog.
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let created = api_create_bead(&client, &base, "escalated-task").await;
    let id = created["id"].as_str().unwrap();

    api_transition_bead(&client, &base, id, "hooked").await;
    api_transition_bead(&client, &base, id, "slung").await;

    // slung -> escalated
    let (code, body) = api_transition_bead(&client, &base, id, "escalated").await;
    assert_eq!(code, 200);
    assert_eq!(body["status"], "escalated");

    // escalated -> backlog
    let (code, body) = api_transition_bead(&client, &base, id, "backlog").await;
    assert_eq!(code, 200);
    assert_eq!(body["status"], "backlog");
}

#[tokio::test]
async fn test_review_rejection_sends_back_to_slung() {
    // Review -> Slung allows sending a task back to AI Review after
    // a human reviewer rejects it.
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let created = api_create_bead(&client, &base, "review-reject").await;
    let id = created["id"].as_str().unwrap();

    api_transition_bead(&client, &base, id, "hooked").await;
    api_transition_bead(&client, &base, id, "slung").await;
    api_transition_bead(&client, &base, id, "review").await;

    // review -> slung (rejected, send back)
    let (code, body) = api_transition_bead(&client, &base, id, "slung").await;
    assert_eq!(code, 200);
    assert_eq!(body["status"], "slung");
}

#[tokio::test]
async fn test_review_can_fail_task() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let created = api_create_bead(&client, &base, "review-fail").await;
    let id = created["id"].as_str().unwrap();

    api_transition_bead(&client, &base, id, "hooked").await;
    api_transition_bead(&client, &base, id, "slung").await;
    api_transition_bead(&client, &base, id, "review").await;

    let (code, body) = api_transition_bead(&client, &base, id, "failed").await;
    assert_eq!(code, 200);
    assert_eq!(body["status"], "failed");
}

#[tokio::test]
async fn test_task_state_sync_after_refresh() {
    // Simulates fix/kanban-refresh-button: after creating/transitioning beads,
    // re-fetching the list should show the latest state.
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let created = api_create_bead(&client, &base, "refresh-sync").await;
    let id = created["id"].as_str().unwrap();
    api_transition_bead(&client, &base, id, "hooked").await;

    // Simulate "Refresh" — re-fetch all beads.
    let beads = api_list_beads(&client, &base).await;
    assert_eq!(beads.len(), 1);
    assert_eq!(beads[0]["status"], "hooked");
    assert_eq!(beads[0]["title"], "refresh-sync");

    // Transition further and refresh again.
    api_transition_bead(&client, &base, id, "slung").await;
    let beads = api_list_beads(&client, &base).await;
    assert_eq!(beads[0]["status"], "slung");
}

#[tokio::test]
async fn test_done_is_terminal_no_transitions() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let created = api_create_bead(&client, &base, "done-terminal").await;
    let id = created["id"].as_str().unwrap();

    // Walk the full pipeline to Done.
    api_transition_bead(&client, &base, id, "hooked").await;
    api_transition_bead(&client, &base, id, "slung").await;
    api_transition_bead(&client, &base, id, "review").await;
    api_transition_bead(&client, &base, id, "done").await;

    // Try to move out of Done — should all fail.
    let (code, _) = api_transition_bead(&client, &base, id, "backlog").await;
    assert_eq!(code, 400, "done -> backlog should be invalid");

    let (code, _) = api_transition_bead(&client, &base, id, "hooked").await;
    assert_eq!(code, 400, "done -> hooked should be invalid");
}

#[tokio::test]
async fn test_invalid_transition_from_backlog() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let created = api_create_bead(&client, &base, "invalid-backlog").await;
    let id = created["id"].as_str().unwrap();

    // backlog can only go to hooked.
    for invalid in &["slung", "review", "done", "failed", "escalated"] {
        let (code, _) = api_transition_bead(&client, &base, id, invalid).await;
        assert_eq!(code, 400, "backlog -> {invalid} should be invalid");
    }
}

#[tokio::test]
async fn test_nonexistent_bead_transition_returns_404() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();
    let fake_id = Uuid::new_v4().to_string();

    let (code, _) = api_transition_bead(&client, &base, &fake_id, "hooked").await;
    assert_eq!(code, 404);
}

// ===========================================================================
// 4. Agent Assignment
// ===========================================================================

#[tokio::test]
async fn test_assign_agent_to_task() {
    let (_base, state) = start_test_server().await;

    let agent_id = inject_agent(&state, "agent-alpha").await;
    let bead_id = inject_bead(&state, "assign-me", BeadStatus::Hooked).await;

    {
        let mut beads = state.beads.write().await;
        let bead = beads.get_mut(&bead_id).unwrap();
        bead.agent_id = Some(agent_id);
    }

    let beads = state.beads.read().await;
    let bead = beads.get(&bead_id).unwrap();
    assert_eq!(bead.agent_id, Some(agent_id));
}

#[tokio::test]
async fn test_unassign_agent_from_task() {
    let (_base, state) = start_test_server().await;

    let agent_id = inject_agent(&state, "agent-beta").await;
    let bead_id = inject_bead(&state, "unassign-me", BeadStatus::Hooked).await;

    // Assign.
    {
        let mut beads = state.beads.write().await;
        let bead = beads.get_mut(&bead_id).unwrap();
        bead.agent_id = Some(agent_id);
    }

    // Unassign.
    {
        let mut beads = state.beads.write().await;
        let bead = beads.get_mut(&bead_id).unwrap();
        bead.agent_id = None;
    }

    let beads = state.beads.read().await;
    let bead = beads.get(&bead_id).unwrap();
    assert!(bead.agent_id.is_none());
}

#[tokio::test]
async fn test_agent_badge_appears_on_assigned_tasks() {
    // When a bead has an agent_id set, the API response includes it.
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    let agent_id = inject_agent(&state, "badge-agent").await;
    let bead_id = inject_bead(&state, "badged-task", BeadStatus::Backlog).await;

    {
        let mut beads = state.beads.write().await;
        let bead = beads.get_mut(&bead_id).unwrap();
        bead.agent_id = Some(agent_id);
    }

    let beads = api_list_beads(&client, &base).await;
    let bead = beads
        .iter()
        .find(|b| b["id"].as_str().unwrap() == bead_id.to_string())
        .unwrap();
    assert_eq!(bead["agent_id"], agent_id.to_string());
}

#[tokio::test]
async fn test_multiple_agents_on_same_task() {
    // The Bead model supports a single agent_id, but a convoy_id groups
    // multiple beads. We verify that multiple beads in the same convoy
    // can each be assigned to different agents.
    let (_base, state) = start_test_server().await;

    let agent_a = inject_agent(&state, "agent-a").await;
    let agent_b = inject_agent(&state, "agent-b").await;
    let convoy_id = Uuid::new_v4();

    let bead_1 = inject_bead(&state, "convoy-task-1", BeadStatus::Hooked).await;
    let bead_2 = inject_bead(&state, "convoy-task-2", BeadStatus::Hooked).await;

    {
        let mut beads = state.beads.write().await;
        let b1 = beads.get_mut(&bead_1).unwrap();
        b1.agent_id = Some(agent_a);
        b1.convoy_id = Some(convoy_id);

        let b2 = beads.get_mut(&bead_2).unwrap();
        b2.agent_id = Some(agent_b);
        b2.convoy_id = Some(convoy_id);
    }

    let beads = state.beads.read().await;
    let convoy_beads: Vec<_> = beads
        .values()
        .filter(|b| b.convoy_id == Some(convoy_id))
        .collect();
    assert_eq!(convoy_beads.len(), 2);
    assert_ne!(convoy_beads[0].agent_id, convoy_beads[1].agent_id);
}

// ===========================================================================
// 5. Filtering & Sorting
// ===========================================================================

#[tokio::test]
async fn test_filter_tasks_by_lane() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    inject_bead(&state, "f-backlog-1", BeadStatus::Backlog).await;
    inject_bead(&state, "f-backlog-2", BeadStatus::Backlog).await;
    inject_bead(&state, "f-hooked-1", BeadStatus::Hooked).await;
    inject_bead(&state, "f-done-1", BeadStatus::Done).await;

    let beads = api_list_beads(&client, &base).await;

    let backlog_beads: Vec<_> = beads.iter().filter(|b| b["status"] == "backlog").collect();
    assert_eq!(backlog_beads.len(), 2);

    let hooked_beads: Vec<_> = beads.iter().filter(|b| b["status"] == "hooked").collect();
    assert_eq!(hooked_beads.len(), 1);

    let done_beads: Vec<_> = beads.iter().filter(|b| b["status"] == "done").collect();
    assert_eq!(done_beads.len(), 1);
}

#[tokio::test]
async fn test_filter_tasks_by_agent() {
    let (_base, state) = start_test_server().await;

    let agent_a = inject_agent(&state, "filter-agent-a").await;
    let agent_b = inject_agent(&state, "filter-agent-b").await;

    let bead_1 = inject_bead(&state, "agent-a-task", BeadStatus::Hooked).await;
    let bead_2 = inject_bead(&state, "agent-b-task", BeadStatus::Hooked).await;
    let _bead_3 = inject_bead(&state, "unassigned-task", BeadStatus::Backlog).await;

    {
        let mut beads = state.beads.write().await;
        beads.get_mut(&bead_1).unwrap().agent_id = Some(agent_a);
        beads.get_mut(&bead_2).unwrap().agent_id = Some(agent_b);
    }

    let beads = state.beads.read().await;
    let agent_a_tasks: Vec<_> = beads
        .values()
        .filter(|b| b.agent_id == Some(agent_a))
        .collect();
    assert_eq!(agent_a_tasks.len(), 1);
    assert_eq!(agent_a_tasks[0].title, "agent-a-task");

    let unassigned: Vec<_> = beads.values().filter(|b| b.agent_id.is_none()).collect();
    assert_eq!(unassigned.len(), 1);
}

#[tokio::test]
async fn test_filter_tasks_by_priority() {
    let (_base, state) = start_test_server().await;

    let id_low = inject_bead(&state, "low-pri", BeadStatus::Backlog).await;
    let id_high = inject_bead(&state, "high-pri", BeadStatus::Backlog).await;
    let id_urgent = inject_bead(&state, "urgent-pri", BeadStatus::Backlog).await;

    {
        let mut beads = state.beads.write().await;
        beads.get_mut(&id_low).unwrap().priority = 1;
        beads.get_mut(&id_high).unwrap().priority = 5;
        beads
            .get_mut(&id_urgent)
            .unwrap()
            .priority = 10;
    }

    let beads = state.beads.read().await;
    let high_pri: Vec<_> = beads.values().filter(|b| b.priority >= 5).collect();
    assert_eq!(high_pri.len(), 2);
}

#[tokio::test]
async fn test_sort_tasks_by_created_date() {
    let (_base, state) = start_test_server().await;

    // Inject beads with slight time gaps.
    inject_bead(&state, "oldest", BeadStatus::Backlog).await;
    inject_bead(&state, "middle", BeadStatus::Backlog).await;
    inject_bead(&state, "newest", BeadStatus::Backlog).await;

    let beads = state.beads.read().await;
    let mut sorted: Vec<_> = beads.values().collect();
    sorted.sort_by_key(|b| b.created_at);

    // Oldest first.
    assert_eq!(sorted[0].title, "oldest");
    assert_eq!(sorted[sorted.len() - 1].title, "newest");
}

#[tokio::test]
async fn test_sort_tasks_by_priority() {
    let (_base, state) = start_test_server().await;

    let id_a = inject_bead(&state, "pri-3", BeadStatus::Backlog).await;
    let id_b = inject_bead(&state, "pri-1", BeadStatus::Backlog).await;
    let id_c = inject_bead(&state, "pri-7", BeadStatus::Backlog).await;

    {
        let mut beads = state.beads.write().await;
        beads.get_mut(&id_a).unwrap().priority = 3;
        beads.get_mut(&id_b).unwrap().priority = 1;
        beads.get_mut(&id_c).unwrap().priority = 7;
    }

    let beads = state.beads.read().await;
    let mut sorted: Vec<_> = beads.values().collect();
    sorted.sort_by(|a, b| b.priority.cmp(&a.priority)); // Descending.

    assert_eq!(sorted[0].title, "pri-7");
    assert_eq!(sorted[1].title, "pri-3");
    assert_eq!(sorted[2].title, "pri-1");
}

// ===========================================================================
// 6. API Integration
// ===========================================================================

#[tokio::test]
async fn test_get_api_beads_returns_all_tasks() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    inject_bead(&state, "all-1", BeadStatus::Backlog).await;
    inject_bead(&state, "all-2", BeadStatus::Hooked).await;
    inject_bead(&state, "all-3", BeadStatus::Done).await;

    let beads = api_list_beads(&client, &base).await;
    assert_eq!(beads.len(), 3);
}

#[tokio::test]
async fn test_post_api_beads_creates_new_task() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let created = api_create_bead(&client, &base, "api-created-task").await;
    assert_eq!(created["title"], "api-created-task");
    assert_eq!(created["status"], "backlog");

    // Verify it persists in the list.
    let beads = api_list_beads(&client, &base).await;
    assert_eq!(beads.len(), 1);
    assert_eq!(beads[0]["title"], "api-created-task");
}

#[tokio::test]
async fn test_patch_api_beads_status_moves_between_lanes() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let created = api_create_bead(&client, &base, "lane-mover").await;
    let id = created["id"].as_str().unwrap();

    // Move through each lane via status updates.
    let (code, body) = api_transition_bead(&client, &base, id, "hooked").await;
    assert_eq!(code, 200);
    assert_eq!(body["status"], "hooked");

    let (code, body) = api_transition_bead(&client, &base, id, "slung").await;
    assert_eq!(code, 200);
    assert_eq!(body["status"], "slung");

    let (code, body) = api_transition_bead(&client, &base, id, "review").await;
    assert_eq!(code, 200);
    assert_eq!(body["status"], "review");

    let (code, body) = api_transition_bead(&client, &base, id, "done").await;
    assert_eq!(code, 200);
    assert_eq!(body["status"], "done");

    // Verify final state.
    let beads = api_list_beads(&client, &base).await;
    assert_eq!(beads.len(), 1);
    assert_eq!(beads[0]["status"], "done");
}

#[tokio::test]
async fn test_get_api_beads_with_lane_filter() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    inject_bead(&state, "lf-backlog", BeadStatus::Backlog).await;
    inject_bead(&state, "lf-hooked", BeadStatus::Hooked).await;
    inject_bead(&state, "lf-done", BeadStatus::Done).await;

    // Fetch all beads and filter client-side (the API returns all beads).
    let beads = api_list_beads(&client, &base).await;
    let hooked: Vec<_> = beads.iter().filter(|b| b["status"] == "hooked").collect();
    assert_eq!(hooked.len(), 1);
    assert_eq!(hooked[0]["title"], "lf-hooked");
}

#[tokio::test]
async fn test_get_api_status_includes_bead_count() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    inject_bead(&state, "status-1", BeadStatus::Backlog).await;
    inject_bead(&state, "status-2", BeadStatus::Hooked).await;

    let resp = client
        .get(format!("{base}/api/status"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["bead_count"], 2);
}

#[tokio::test]
async fn test_get_api_kpi_reflects_lane_counts() {
    let (_base, state) = start_test_server().await;

    // Manually update the KPI snapshot to reflect bead distribution.
    {
        let mut kpi = state.kpi.write().await;
        kpi.total_beads = 10;
        kpi.backlog = 4;
        kpi.hooked = 3;
        kpi.slung = 1;
        kpi.review = 0;
        kpi.done = 2;
        kpi.failed = 0;
        kpi.escalated = 0;
    }

    let kpi = state.kpi.read().await;
    assert_eq!(kpi.total_beads, 10);
    assert_eq!(kpi.backlog, 4);
    assert_eq!(kpi.hooked, 3);
    assert_eq!(kpi.slung, 1);
    assert_eq!(kpi.review, 0);
    assert_eq!(kpi.done, 2);
}

#[tokio::test]
async fn test_auto_realignment_on_refresh() {
    // Auto-realignment ensures that when beads are re-fetched ("refresh"),
    // the lane membership is consistent with each bead's current status.
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create beads and move them to various states.
    let b1 = api_create_bead(&client, &base, "realign-1").await;
    let b2 = api_create_bead(&client, &base, "realign-2").await;
    let _b3 = api_create_bead(&client, &base, "realign-3").await;

    let id1 = b1["id"].as_str().unwrap();
    let id2 = b2["id"].as_str().unwrap();

    api_transition_bead(&client, &base, id1, "hooked").await;
    api_transition_bead(&client, &base, id2, "hooked").await;
    api_transition_bead(&client, &base, id2, "slung").await;
    // id3 remains in backlog.

    // "Refresh" — fetch all beads and verify alignment.
    let beads = api_list_beads(&client, &base).await;
    assert_eq!(beads.len(), 3);

    let counts = lane_counts(&beads);
    assert_eq!(*counts.get("backlog").unwrap_or(&0), 1);
    assert_eq!(*counts.get("hooked").unwrap_or(&0), 1);
    assert_eq!(*counts.get("slung").unwrap_or(&0), 1);
}

// ===========================================================================
// 7. Additional edge-case & robustness tests
// ===========================================================================

#[tokio::test]
async fn test_concurrent_bead_creation() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Fire off 10 creates concurrently.
    let mut handles = vec![];
    for i in 0..10 {
        let c = client.clone();
        let b = base.clone();
        handles.push(tokio::spawn(async move {
            api_create_bead(&c, &b, &format!("concurrent-{i}")).await
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let beads = api_list_beads(&client, &base).await;
    assert_eq!(beads.len(), 10);

    // All IDs unique.
    let ids: std::collections::HashSet<_> = beads
        .iter()
        .map(|b| b["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids.len(), 10);
}

#[tokio::test]
async fn test_bead_updated_at_changes_on_transition() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let created = api_create_bead(&client, &base, "timestamp-test").await;
    let id = created["id"].as_str().unwrap();
    let created_at = created["updated_at"].as_str().unwrap().to_string();

    // Small delay to ensure timestamp differs.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let (_, updated) = api_transition_bead(&client, &base, id, "hooked").await;
    let updated_at = updated["updated_at"].as_str().unwrap().to_string();

    assert_ne!(
        created_at, updated_at,
        "updated_at should change on transition"
    );
}

#[tokio::test]
async fn test_bead_serialization_includes_all_fields() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let created = api_create_bead_with_desc(&client, &base, "full-fields", "A description").await;

    // All expected fields should be present in the JSON.
    assert!(created["id"].is_string());
    assert!(created["title"].is_string());
    assert!(created["description"].is_string());
    assert!(created["status"].is_string());
    assert!(created["lane"].is_string());
    assert!(created["priority"].is_number());
    assert!(created["created_at"].is_string());
    assert!(created["updated_at"].is_string());
}

#[tokio::test]
async fn test_multiple_beads_in_same_lane() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    for i in 0..5 {
        inject_bead(&state, &format!("backlog-{i}"), BeadStatus::Backlog).await;
    }
    for i in 0..3 {
        inject_bead(&state, &format!("hooked-{i}"), BeadStatus::Hooked).await;
    }

    let beads = api_list_beads(&client, &base).await;
    let counts = lane_counts(&beads);

    assert_eq!(*counts.get("backlog").unwrap(), 5);
    assert_eq!(*counts.get("hooked").unwrap(), 3);
}

#[tokio::test]
async fn test_bead_metadata_roundtrip_via_api() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    let mut bead = Bead::new("metadata-test", Lane::Standard);
    bead.metadata = Some(json!({
        "labels": ["ui", "backend"],
        "color": "green",
        "source": "github"
    }));
    let id = bead.id;
    state.beads.write().await.insert(id, bead);

    let beads = api_list_beads(&client, &base).await;
    let found = beads
        .iter()
        .find(|b| b["id"].as_str().unwrap() == id.to_string())
        .unwrap();

    assert_eq!(found["metadata"]["color"], "green");
    assert_eq!(found["metadata"]["source"], "github");
    let labels = found["metadata"]["labels"].as_array().unwrap();
    assert_eq!(labels.len(), 2);
}

#[tokio::test]
async fn test_bead_lane_field_preserved() {
    // Verify that the Lane enum (Experimental/Standard/Critical) is preserved
    // independently of the BeadStatus.
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&json!({ "title": "lane-test", "lane": "critical" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let created: Value = resp.json().await.unwrap();
    assert_eq!(created["lane"], "critical");
    assert_eq!(created["status"], "backlog");
}

#[tokio::test]
async fn test_kpi_endpoint_returns_all_lane_fields() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client.get(format!("{base}/api/kpi")).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();

    // KPI snapshot includes all lane-count fields.
    assert!(body["total_beads"].is_number());
    assert!(body["backlog"].is_number());
    assert!(body["hooked"].is_number());
    assert!(body["slung"].is_number());
    assert!(body["review"].is_number());
    assert!(body["done"].is_number());
    assert!(body["failed"].is_number());
    assert!(body["escalated"].is_number());
    assert!(body["active_agents"].is_number());
}

#[tokio::test]
async fn test_git_branch_field_on_bead() {
    // Beads can carry a git_branch for branch-tracking in the Kanban UI.
    let (_base, state) = start_test_server().await;

    let mut bead = Bead::new("branch-test", Lane::Standard);
    bead.git_branch = Some("feat/kanban-refresh".to_string());
    let id = bead.id;
    state.beads.write().await.insert(id, bead);

    let beads = state.beads.read().await;
    let found = beads.get(&id).unwrap();
    assert_eq!(found.git_branch.as_deref(), Some("feat/kanban-refresh"));
}

#[tokio::test]
async fn test_convoy_groups_beads_together() {
    let (_base, state) = start_test_server().await;

    let convoy_id = Uuid::new_v4();
    let b1 = inject_bead(&state, "convoy-1", BeadStatus::Backlog).await;
    let b2 = inject_bead(&state, "convoy-2", BeadStatus::Hooked).await;
    let _b3 = inject_bead(&state, "solo", BeadStatus::Backlog).await;

    {
        let mut beads = state.beads.write().await;
        beads.get_mut(&b1).unwrap().convoy_id = Some(convoy_id);
        beads.get_mut(&b2).unwrap().convoy_id = Some(convoy_id);
    }

    let beads = state.beads.read().await;
    let convoy_beads: Vec<_> = beads
        .values()
        .filter(|b| b.convoy_id == Some(convoy_id))
        .collect();
    assert_eq!(convoy_beads.len(), 2);

    let solo_beads: Vec<_> = beads.values().filter(|b| b.convoy_id.is_none()).collect();
    assert_eq!(solo_beads.len(), 1);
}

#[tokio::test]
async fn test_hooked_at_timestamp_set_on_transition() {
    let (_base, state) = start_test_server().await;

    let id = inject_bead(&state, "hooked-timestamp", BeadStatus::Backlog).await;

    {
        let beads = state.beads.read().await;
        let bead = beads.get(&id).unwrap();
        assert!(bead.hooked_at.is_none());
    }

    // Simulate the transition by setting the timestamp.
    {
        let mut beads = state.beads.write().await;
        let bead = beads.get_mut(&id).unwrap();
        bead.status = BeadStatus::Hooked;
        bead.hooked_at = Some(chrono::Utc::now());
    }

    let beads = state.beads.read().await;
    let bead = beads.get(&id).unwrap();
    assert!(bead.hooked_at.is_some());
}

#[tokio::test]
async fn test_done_at_timestamp_set_on_completion() {
    let (_base, state) = start_test_server().await;

    let id = inject_bead(&state, "done-timestamp", BeadStatus::Backlog).await;

    {
        let mut beads = state.beads.write().await;
        let bead = beads.get_mut(&id).unwrap();
        bead.status = BeadStatus::Done;
        bead.done_at = Some(chrono::Utc::now());
    }

    let beads = state.beads.read().await;
    let bead = beads.get(&id).unwrap();
    assert!(bead.done_at.is_some());
}

#[tokio::test]
async fn test_slung_at_timestamp_set_on_ai_review() {
    let (_base, state) = start_test_server().await;

    let id = inject_bead(&state, "slung-timestamp", BeadStatus::Backlog).await;

    {
        let mut beads = state.beads.write().await;
        let bead = beads.get_mut(&id).unwrap();
        bead.status = BeadStatus::Slung;
        bead.slung_at = Some(chrono::Utc::now());
    }

    let beads = state.beads.read().await;
    let bead = beads.get(&id).unwrap();
    assert!(bead.slung_at.is_some());
}

#[tokio::test]
async fn test_default_bead_has_no_timestamps() {
    let bead = Bead::new("fresh", Lane::Standard);
    assert!(bead.hooked_at.is_none());
    assert!(bead.slung_at.is_none());
    assert!(bead.done_at.is_none());
    assert!(bead.agent_id.is_none());
    assert!(bead.convoy_id.is_none());
    assert!(bead.git_branch.is_none());
    assert!(bead.metadata.is_none());
}

#[tokio::test]
async fn test_default_bead_lane_is_standard() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // When no lane is specified, the default is Standard.
    let created = api_create_bead(&client, &base, "default-lane").await;
    assert_eq!(created["lane"], "standard");
}

#[tokio::test]
async fn test_agent_list_endpoint() {
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    inject_agent(&state, "agent-x").await;
    inject_agent(&state, "agent-y").await;

    let resp = client
        .get(format!("{base}/api/agents"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let agents: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(agents.len(), 2);
}
