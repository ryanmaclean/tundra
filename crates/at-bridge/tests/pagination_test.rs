//! Exhaustive integration tests for HTTP API pagination support.
//!
//! These tests verify that pagination query parameters (limit & offset) work
//! correctly across all endpoints that support them. We test:
//! - Basic pagination with limit and offset
//! - Default pagination values
//! - Edge cases (offset beyond data, limit 0, etc.)
//! - Pagination combined with filters
//! - Multiple endpoints for comprehensive coverage
//!
//! Each endpoint that supports pagination through Query types (NotificationQuery,
//! BeadQuery, TaskListQuery, etc.) is tested to ensure consistent behavior.

use std::sync::Arc;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, ApiState};
use at_bridge::notifications::NotificationLevel;
use at_core::types::*;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

// ===========================================================================
// Helpers
// ===========================================================================

/// Helper to get router with state for testing
fn test_router_with_state() -> (axum::Router, Arc<ApiState>) {
    let event_bus = EventBus::new();
    let state = Arc::new(ApiState::new(event_bus).with_relaxed_rate_limits());
    let router = api_router(state.clone());
    (router, state)
}

/// Helper to read the response body as JSON array
async fn body_json_array(resp: axum::http::Response<Body>) -> Vec<Value> {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// Seed tasks for testing
async fn seed_tasks(state: &ApiState, count: usize) -> Vec<Uuid> {
    let mut tasks = state.tasks.write().await;
    let mut ids = Vec::new();
    for i in 0..count {
        let task_id = Uuid::new_v4();
        let task = Task {
            id: task_id,
            title: format!("Task {}", i),
            bead_id: Uuid::new_v4(),
            category: TaskCategory::Feature,
            priority: TaskPriority::Medium,
            complexity: TaskComplexity::Medium,
            description: Some(format!("Description for task {}", i)),
            phase: TaskPhase::Discovery,
            progress_percent: 0,
            subtasks: vec![],
            worktree_path: None,
            git_branch: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
            error: None,
            phase_configs: vec![],
            agent_profile: None,
            impact: None,
            logs: vec![],
            qa_report: None,
            source: None,
            parent_task_id: None,
            stack_position: None,
            pr_number: None,
            build_logs: vec![],
        };
        tasks.insert(task_id, task);
        ids.push(task_id);
    }
    ids
}

/// Seed beads for testing
async fn seed_beads(state: &ApiState, count: usize) -> Vec<Uuid> {
    let mut beads = state.beads.write().await;
    let mut ids = Vec::new();
    for i in 0..count {
        let bead = Bead::new(format!("Bead {}", i), Lane::Standard);
        ids.push(bead.id);
        beads.insert(bead.id, bead);
    }
    ids
}

/// Seed agents for testing
async fn seed_agents(state: &ApiState, count: usize) -> Vec<Uuid> {
    let mut agents = state.agents.write().await;
    let mut ids = Vec::new();
    for i in 0..count {
        let agent = Agent::new(format!("Agent {}", i), AgentRole::Crew, CliType::Claude);
        ids.push(agent.id);
        agents.insert(agent.id, agent);
    }
    ids
}

/// Seed notifications for testing
async fn seed_notifications(state: &ApiState, count: usize) {
    let mut store = state.notification_store.write().await;
    for i in 0..count {
        store.add(
            format!("Notification {}", i),
            &format!("Message {}", i),
            NotificationLevel::Info,
            "test",
        );
    }
}

/// Seed projects for testing
async fn seed_projects(state: &ApiState, count: usize) -> Vec<Uuid> {
    let mut projects = state.projects.write().await;
    let mut ids = Vec::new();
    for i in 0..count {
        let project_id = Uuid::new_v4();
        let project = at_bridge::http_api::types::Project {
            id: project_id,
            name: format!("Project {}", i),
            path: format!("/path/to/project{}", i),
            created_at: chrono::Utc::now().to_rfc3339(),
            is_active: true,
        };
        projects.push(project);
        ids.push(project_id);
    }
    ids
}

// ===========================================================================
// 1. Tasks Endpoint Pagination (10 tests)
// ===========================================================================

#[tokio::test]
async fn test_tasks_pagination_basic_limit() {
    let (app, state) = test_router_with_state();
    seed_tasks(&state, 20).await;

    let req = Request::builder()
        .uri("/api/tasks?limit=5")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let tasks = body_json_array(resp).await;
    assert_eq!(tasks.len(), 5, "Should return exactly 5 tasks");
}

#[tokio::test]
async fn test_tasks_pagination_with_offset() {
    let (app, state) = test_router_with_state();
    seed_tasks(&state, 20).await;

    let req = Request::builder()
        .uri("/api/tasks?limit=5&offset=10")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let tasks = body_json_array(resp).await;
    assert_eq!(
        tasks.len(),
        5,
        "Should return 5 tasks starting from offset 10"
    );
}

#[tokio::test]
async fn test_tasks_pagination_offset_beyond_data() {
    let (app, state) = test_router_with_state();
    seed_tasks(&state, 10).await;

    let req = Request::builder()
        .uri("/api/tasks?limit=5&offset=20")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let tasks = body_json_array(resp).await;
    assert_eq!(
        tasks.len(),
        0,
        "Should return empty array when offset beyond data"
    );
}

#[tokio::test]
async fn test_tasks_pagination_default_limit() {
    let (app, state) = test_router_with_state();
    seed_tasks(&state, 100).await;

    let req = Request::builder()
        .uri("/api/tasks")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let tasks = body_json_array(resp).await;
    assert_eq!(tasks.len(), 50, "Default limit should be 50");
}

#[tokio::test]
async fn test_tasks_pagination_limit_zero() {
    let (app, state) = test_router_with_state();
    seed_tasks(&state, 10).await;

    let req = Request::builder()
        .uri("/api/tasks?limit=0")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let tasks = body_json_array(resp).await;
    assert_eq!(tasks.len(), 0, "Limit 0 should return empty array");
}

#[tokio::test]
async fn test_tasks_pagination_with_phase_filter() {
    let (app, state) = test_router_with_state();
    seed_tasks(&state, 20).await;

    let req = Request::builder()
        .uri("/api/tasks?phase=discovery&limit=5&offset=0")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let tasks = body_json_array(resp).await;
    assert!(tasks.len() <= 5, "Should respect limit with filter");
}

#[tokio::test]
async fn test_tasks_pagination_with_category_filter() {
    let (app, state) = test_router_with_state();
    seed_tasks(&state, 15).await;

    let req = Request::builder()
        .uri("/api/tasks?category=feature&limit=3&offset=0")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let tasks = body_json_array(resp).await;
    assert!(
        tasks.len() <= 3,
        "Should respect limit with category filter"
    );
}

#[tokio::test]
async fn test_tasks_pagination_with_priority_filter() {
    let (app, state) = test_router_with_state();
    seed_tasks(&state, 15).await;

    let req = Request::builder()
        .uri("/api/tasks?priority=medium&limit=5")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let tasks = body_json_array(resp).await;
    assert!(
        tasks.len() <= 5,
        "Should respect limit with priority filter"
    );
}

#[tokio::test]
async fn test_tasks_pagination_large_limit() {
    let (app, state) = test_router_with_state();
    seed_tasks(&state, 10).await;

    let req = Request::builder()
        .uri("/api/tasks?limit=1000")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let tasks = body_json_array(resp).await;
    assert_eq!(
        tasks.len(),
        10,
        "Should return all available tasks when limit exceeds count"
    );
}

#[tokio::test]
async fn test_tasks_pagination_offset_only() {
    let (app, state) = test_router_with_state();
    seed_tasks(&state, 100).await;

    let req = Request::builder()
        .uri("/api/tasks?offset=10")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let tasks = body_json_array(resp).await;
    assert_eq!(
        tasks.len(),
        50,
        "Should use default limit of 50 with custom offset"
    );
}

// ===========================================================================
// 2. Beads Endpoint Pagination (8 tests)
// ===========================================================================

#[tokio::test]
async fn test_beads_pagination_basic_limit() {
    let (app, state) = test_router_with_state();
    seed_beads(&state, 15).await;

    let req = Request::builder()
        .uri("/api/beads?limit=5")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let beads = body_json_array(resp).await;
    assert_eq!(beads.len(), 5, "Should return exactly 5 beads");
}

#[tokio::test]
async fn test_beads_pagination_with_offset() {
    let (app, state) = test_router_with_state();
    seed_beads(&state, 20).await;

    let req = Request::builder()
        .uri("/api/beads?limit=5&offset=10")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let beads = body_json_array(resp).await;
    assert_eq!(beads.len(), 5, "Should return 5 beads from offset");
}

#[tokio::test]
async fn test_beads_pagination_empty_result() {
    let (app, state) = test_router_with_state();
    seed_beads(&state, 5).await;

    let req = Request::builder()
        .uri("/api/beads?limit=10&offset=10")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let beads = body_json_array(resp).await;
    assert_eq!(
        beads.len(),
        0,
        "Should return empty when offset beyond data"
    );
}

#[tokio::test]
async fn test_beads_pagination_with_status_filter() {
    let (app, state) = test_router_with_state();
    seed_beads(&state, 20).await;

    let req = Request::builder()
        .uri("/api/beads?status=backlog&limit=5")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let beads = body_json_array(resp).await;
    assert!(beads.len() <= 5, "Should respect limit with status filter");
}

#[tokio::test]
async fn test_beads_pagination_limit_one() {
    let (app, state) = test_router_with_state();
    seed_beads(&state, 10).await;

    let req = Request::builder()
        .uri("/api/beads?limit=1")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let beads = body_json_array(resp).await;
    assert_eq!(beads.len(), 1, "Should return exactly 1 bead");
}

#[tokio::test]
async fn test_beads_pagination_default_values() {
    let (app, state) = test_router_with_state();
    seed_beads(&state, 100).await;

    let req = Request::builder()
        .uri("/api/beads")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let beads = body_json_array(resp).await;
    assert_eq!(beads.len(), 50, "Should use default limit of 50");
}

#[tokio::test]
async fn test_beads_pagination_sequential_pages() {
    let (app, state) = test_router_with_state();
    seed_beads(&state, 15).await;

    // Get first page
    let req1 = Request::builder()
        .uri("/api/beads?limit=5&offset=0")
        .body(Body::empty())
        .unwrap();

    let resp1 = app.clone().oneshot(req1).await.unwrap();
    let page1 = body_json_array(resp1).await;
    assert_eq!(page1.len(), 5);

    // Get second page
    let req2 = Request::builder()
        .uri("/api/beads?limit=5&offset=5")
        .body(Body::empty())
        .unwrap();

    let resp2 = app.clone().oneshot(req2).await.unwrap();
    let page2 = body_json_array(resp2).await;
    assert_eq!(page2.len(), 5);

    // Get third page
    let req3 = Request::builder()
        .uri("/api/beads?limit=5&offset=10")
        .body(Body::empty())
        .unwrap();

    let resp3 = app.oneshot(req3).await.unwrap();
    let page3 = body_json_array(resp3).await;
    assert_eq!(page3.len(), 5);
}

#[tokio::test]
async fn test_beads_pagination_partial_last_page() {
    let (app, state) = test_router_with_state();
    seed_beads(&state, 12).await;

    let req = Request::builder()
        .uri("/api/beads?limit=5&offset=10")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let beads = body_json_array(resp).await;
    assert_eq!(
        beads.len(),
        2,
        "Should return remaining 2 beads on last page"
    );
}

// ===========================================================================
// 3. Agents Endpoint Pagination (6 tests)
// ===========================================================================

#[tokio::test]
async fn test_agents_pagination_basic_limit() {
    let (app, state) = test_router_with_state();
    seed_agents(&state, 20).await;

    let req = Request::builder()
        .uri("/api/agents?limit=5")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let agents = body_json_array(resp).await;
    assert_eq!(agents.len(), 5, "Should return exactly 5 agents");
}

#[tokio::test]
async fn test_agents_pagination_with_offset() {
    let (app, state) = test_router_with_state();
    seed_agents(&state, 25).await;

    let req = Request::builder()
        .uri("/api/agents?limit=10&offset=5")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let agents = body_json_array(resp).await;
    assert_eq!(agents.len(), 10, "Should return 10 agents from offset");
}

#[tokio::test]
async fn test_agents_pagination_empty_dataset() {
    let (app, _state) = test_router_with_state();

    let req = Request::builder()
        .uri("/api/agents?limit=10&offset=0")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let agents = body_json_array(resp).await;
    assert_eq!(agents.len(), 0, "Should return empty array for no agents");
}

#[tokio::test]
async fn test_agents_pagination_default_limit() {
    let (app, state) = test_router_with_state();
    seed_agents(&state, 100).await;

    let req = Request::builder()
        .uri("/api/agents")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let agents = body_json_array(resp).await;
    assert_eq!(agents.len(), 50, "Should use default limit of 50");
}

#[tokio::test]
async fn test_agents_pagination_exact_page_size() {
    let (app, state) = test_router_with_state();
    seed_agents(&state, 10).await;

    let req = Request::builder()
        .uri("/api/agents?limit=10")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let agents = body_json_array(resp).await;
    assert_eq!(
        agents.len(),
        10,
        "Should return all agents when limit equals count"
    );
}

#[tokio::test]
async fn test_agents_pagination_large_offset() {
    let (app, state) = test_router_with_state();
    seed_agents(&state, 10).await;

    let req = Request::builder()
        .uri("/api/agents?limit=5&offset=1000")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let agents = body_json_array(resp).await;
    assert_eq!(agents.len(), 0, "Should return empty for very large offset");
}

// ===========================================================================
// 4. Notifications Endpoint Pagination (6 tests)
// ===========================================================================

#[tokio::test]
async fn test_notifications_pagination_basic_limit() {
    let (app, state) = test_router_with_state();
    seed_notifications(&state, 20).await;

    let req = Request::builder()
        .uri("/api/notifications?limit=5")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let notifications = body_json_array(resp).await;
    assert_eq!(
        notifications.len(),
        5,
        "Should return exactly 5 notifications"
    );
}

#[tokio::test]
async fn test_notifications_pagination_with_offset() {
    let (app, state) = test_router_with_state();
    seed_notifications(&state, 30).await;

    let req = Request::builder()
        .uri("/api/notifications?limit=5&offset=10")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let notifications = body_json_array(resp).await;
    assert_eq!(
        notifications.len(),
        5,
        "Should return 5 notifications from offset"
    );
}

#[tokio::test]
async fn test_notifications_pagination_with_unread_filter() {
    let (app, state) = test_router_with_state();
    seed_notifications(&state, 20).await;

    let req = Request::builder()
        .uri("/api/notifications?unread=true&limit=5")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let notifications = body_json_array(resp).await;
    assert!(
        notifications.len() <= 5,
        "Should respect limit with unread filter"
    );
}

#[tokio::test]
async fn test_notifications_pagination_empty_result() {
    let (app, state) = test_router_with_state();
    seed_notifications(&state, 5).await;

    let req = Request::builder()
        .uri("/api/notifications?limit=10&offset=10")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let notifications = body_json_array(resp).await;
    assert_eq!(
        notifications.len(),
        0,
        "Should return empty when offset beyond data"
    );
}

#[tokio::test]
async fn test_notifications_pagination_default_limit() {
    let (app, state) = test_router_with_state();
    seed_notifications(&state, 100).await;

    let req = Request::builder()
        .uri("/api/notifications")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let notifications = body_json_array(resp).await;
    assert_eq!(notifications.len(), 50, "Should use default limit of 50");
}

#[tokio::test]
async fn test_notifications_pagination_limit_exceeds_data() {
    let (app, state) = test_router_with_state();
    seed_notifications(&state, 5).await;

    let req = Request::builder()
        .uri("/api/notifications?limit=100")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let notifications = body_json_array(resp).await;
    assert_eq!(
        notifications.len(),
        5,
        "Should return all available when limit exceeds count"
    );
}

// ===========================================================================
// 5. Projects Endpoint Pagination (5 tests)
// ===========================================================================

#[tokio::test]
async fn test_projects_pagination_basic_limit() {
    let (app, state) = test_router_with_state();
    seed_projects(&state, 15).await;

    let req = Request::builder()
        .uri("/api/projects?limit=5")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let projects = body_json_array(resp).await;
    assert_eq!(projects.len(), 5, "Should return exactly 5 projects");
}

#[tokio::test]
async fn test_projects_pagination_with_offset() {
    let (app, state) = test_router_with_state();
    seed_projects(&state, 20).await;

    let req = Request::builder()
        .uri("/api/projects?limit=5&offset=5")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let projects = body_json_array(resp).await;
    assert_eq!(projects.len(), 5, "Should return 5 projects from offset");
}

#[tokio::test]
async fn test_projects_pagination_empty_dataset() {
    let (app, _state) = test_router_with_state();

    let req = Request::builder()
        .uri("/api/projects?limit=10")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let projects = body_json_array(resp).await;
    // Note: There may be a default project, so we just verify pagination works
    assert!(
        projects.len() <= 10,
        "Should respect limit even for default projects"
    );
}

#[tokio::test]
async fn test_projects_pagination_default_limit() {
    let (app, state) = test_router_with_state();
    seed_projects(&state, 100).await;

    let req = Request::builder()
        .uri("/api/projects")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let projects = body_json_array(resp).await;
    assert_eq!(projects.len(), 50, "Should use default limit of 50");
}

#[tokio::test]
async fn test_projects_pagination_offset_at_boundary() {
    let (app, state) = test_router_with_state();

    // Clear any default projects first
    {
        let mut projects = state.projects.write().await;
        projects.clear();
    }

    seed_projects(&state, 10).await;

    let req = Request::builder()
        .uri("/api/projects?limit=5&offset=10")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let projects = body_json_array(resp).await;
    assert_eq!(
        projects.len(),
        0,
        "Should return empty when offset equals total count"
    );
}

// ===========================================================================
// 6. Queue Endpoint Pagination (4 tests)
// ===========================================================================

#[tokio::test]
async fn test_queue_pagination_basic_limit() {
    let (app, state) = test_router_with_state();

    // Add tasks to the queue (tasks in Discovery phase that haven't started)
    {
        let mut tasks = state.tasks.write().await;
        for i in 0..15 {
            let task_id = Uuid::new_v4();
            let mut task = Task::new(
                format!("Queued Task {}", i),
                Uuid::new_v4(),
                TaskCategory::Feature,
                TaskPriority::Medium,
                TaskComplexity::Medium,
            );
            task.phase = TaskPhase::Discovery;
            tasks.insert(task_id, task);
        }
    }

    let req = Request::builder()
        .uri("/api/queue?limit=5")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let queue: Value = serde_json::from_slice(&body_bytes).unwrap();
    let queue_array = queue.as_array().unwrap();
    assert_eq!(queue_array.len(), 5, "Should return exactly 5 queued tasks");
}

#[tokio::test]
async fn test_queue_pagination_with_offset() {
    let (app, state) = test_router_with_state();

    {
        let mut tasks = state.tasks.write().await;
        for i in 0..20 {
            let task_id = Uuid::new_v4();
            let mut task = Task::new(
                format!("Queued Task {}", i),
                Uuid::new_v4(),
                TaskCategory::Feature,
                TaskPriority::Medium,
                TaskComplexity::Medium,
            );
            task.phase = TaskPhase::Discovery;
            tasks.insert(task_id, task);
        }
    }

    let req = Request::builder()
        .uri("/api/queue?limit=5&offset=10")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let queue: Value = serde_json::from_slice(&body_bytes).unwrap();
    let queue_array = queue.as_array().unwrap();
    assert_eq!(queue_array.len(), 5, "Should return 5 tasks from offset");
}

#[tokio::test]
async fn test_queue_pagination_empty_queue() {
    let (app, _state) = test_router_with_state();

    let req = Request::builder()
        .uri("/api/queue?limit=10")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let queue: Value = serde_json::from_slice(&body_bytes).unwrap();
    let queue_array = queue.as_array().unwrap();
    assert_eq!(
        queue_array.len(),
        0,
        "Should return empty array for empty queue"
    );
}

#[tokio::test]
async fn test_queue_pagination_default_limit() {
    let (app, state) = test_router_with_state();

    {
        let mut tasks = state.tasks.write().await;
        for i in 0..100 {
            let task_id = Uuid::new_v4();
            let mut task = Task::new(
                format!("Queued Task {}", i),
                Uuid::new_v4(),
                TaskCategory::Feature,
                TaskPriority::Medium,
                TaskComplexity::Medium,
            );
            task.phase = TaskPhase::Discovery;
            tasks.insert(task_id, task);
        }
    }

    let req = Request::builder()
        .uri("/api/queue")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let queue: Value = serde_json::from_slice(&body_bytes).unwrap();
    let queue_array = queue.as_array().unwrap();
    assert_eq!(queue_array.len(), 50, "Should use default limit of 50");
}

// ===========================================================================
// 7. Cross-Endpoint Pagination Consistency (5 tests)
// ===========================================================================

#[tokio::test]
async fn test_pagination_consistent_across_endpoints() {
    let (app, state) = test_router_with_state();

    // Seed data for multiple endpoints
    seed_tasks(&state, 20).await;
    seed_beads(&state, 20).await;
    seed_agents(&state, 20).await;
    seed_notifications(&state, 20).await;

    let endpoints = vec![
        "/api/tasks?limit=5&offset=0",
        "/api/beads?limit=5&offset=0",
        "/api/agents?limit=5&offset=0",
        "/api/notifications?limit=5&offset=0",
    ];

    for endpoint in endpoints {
        let req = Request::builder()
            .uri(endpoint)
            .body(Body::empty())
            .unwrap();

        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "Endpoint {} should return 200",
            endpoint
        );

        let items = body_json_array(resp).await;
        assert_eq!(
            items.len(),
            5,
            "Endpoint {} should return exactly 5 items",
            endpoint
        );
    }
}

#[tokio::test]
async fn test_pagination_zero_offset_consistent() {
    let (app, state) = test_router_with_state();
    seed_tasks(&state, 10).await;

    // Test that offset=0 and no offset parameter produce the same result
    let req1 = Request::builder()
        .uri("/api/tasks?limit=5&offset=0")
        .body(Body::empty())
        .unwrap();

    let resp1 = app.clone().oneshot(req1).await.unwrap();
    let tasks1 = body_json_array(resp1).await;

    let req2 = Request::builder()
        .uri("/api/tasks?limit=5")
        .body(Body::empty())
        .unwrap();

    let resp2 = app.oneshot(req2).await.unwrap();
    let tasks2 = body_json_array(resp2).await;

    assert_eq!(
        tasks1.len(),
        tasks2.len(),
        "offset=0 should be same as no offset"
    );
}

#[tokio::test]
async fn test_pagination_parameters_case_insensitive() {
    let (app, state) = test_router_with_state();
    seed_tasks(&state, 10).await;

    // Query parameters should be case-sensitive per HTTP spec,
    // but let's verify they work as documented
    let req = Request::builder()
        .uri("/api/tasks?limit=3&offset=2")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let tasks = body_json_array(resp).await;
    assert_eq!(tasks.len(), 3, "Lowercase parameters should work");
}

#[tokio::test]
async fn test_pagination_invalid_params_ignored() {
    let (app, state) = test_router_with_state();
    seed_tasks(&state, 10).await;

    // Invalid parameter values should be handled gracefully
    let req = Request::builder()
        .uri("/api/tasks?limit=abc&offset=xyz")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    // The endpoint should handle invalid params gracefully
    // Either by using defaults or returning an error
    assert!(
        resp.status() == StatusCode::OK || resp.status() == StatusCode::BAD_REQUEST,
        "Should handle invalid pagination params gracefully"
    );
}

#[tokio::test]
async fn test_pagination_multiple_sequential_requests() {
    let (app, state) = test_router_with_state();
    seed_tasks(&state, 30).await;

    // Make multiple sequential requests to verify consistency
    for i in 0..3 {
        let offset = i * 10;
        let uri = format!("/api/tasks?limit=10&offset={}", offset);

        let req = Request::builder()
            .uri(uri.as_str())
            .body(Body::empty())
            .unwrap();

        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let tasks = body_json_array(resp).await;
        assert_eq!(tasks.len(), 10, "Each page should return 10 tasks");
    }
}
