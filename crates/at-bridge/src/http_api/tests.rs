use super::*;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use crate::event_bus::EventBus;
use at_core::types::{
    Agent, Bead, CliType, Lane, Task, TaskCategory, TaskComplexity, TaskPhase, TaskPriority,
};
use state::ApiState;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use uuid::Uuid;

/// Build a test router with fresh state.
fn test_app() -> (axum::Router, Arc<ApiState>) {
    let event_bus = EventBus::new();
    let state = Arc::new(ApiState::new(event_bus));
    let app = router::api_router(state.clone());
    (app, state)
}

#[tokio::test]
async fn test_trigger_sync_returns_ok() {
    let (app, _state) = test_app();

    let req = Request::builder()
        .method("POST")
        .uri("/api/github/sync")
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    assert!(
        status == StatusCode::SERVICE_UNAVAILABLE || status == StatusCode::BAD_REQUEST,
        "Expected 503 or 400, got {}",
        status
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let err = json["error"].as_str().unwrap();
    assert!(err.contains("token") || err.contains("owner") || err.contains("repo"));
}

#[tokio::test]
async fn test_list_github_issues_requires_config() {
    let (app, _state) = test_app();
    let req = Request::builder()
        .method("GET")
        .uri("/api/github/issues")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    assert!(
        status == StatusCode::SERVICE_UNAVAILABLE || status == StatusCode::BAD_REQUEST,
        "Expected 503 or 400, got {}",
        status
    );
}

#[tokio::test]
async fn test_list_github_issues_accepts_query_params() {
    let (app, _state) = test_app();
    let req = Request::builder()
        .method("GET")
        .uri("/api/github/issues?state=open&page=1&per_page=10")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    assert!(
        status == StatusCode::SERVICE_UNAVAILABLE || status == StatusCode::BAD_REQUEST,
        "Expected 503 or 400, got {}",
        status
    );
}

#[tokio::test]
async fn test_get_sync_status_default() {
    let (app, _state) = test_app();

    let req = Request::builder()
        .method("GET")
        .uri("/api/github/sync/status")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["is_syncing"], false);
    assert!(json["last_sync_time"].is_null());
}

#[tokio::test]
async fn test_create_pr_task_not_found() {
    let (app, _state) = test_app();

    let fake_id = Uuid::new_v4();
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/github/pr/{}", fake_id))
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_create_pr_for_existing_task_no_branch() {
    let (app, state) = test_app();

    let task = Task::new(
        "Test task",
        Uuid::new_v4(),
        TaskCategory::Feature,
        TaskPriority::Medium,
        TaskComplexity::Small,
    );
    let task_id = task.id;
    {
        let mut tasks = state.tasks.write().await;
        tasks.insert(task_id, task);
    }

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/github/pr/{}", task_id))
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"].as_str().unwrap().contains("branch"));
}

#[tokio::test]
async fn test_create_pr_for_existing_task_with_branch_no_token() {
    let (app, state) = test_app();

    let mut task = Task::new(
        "Test task",
        Uuid::new_v4(),
        TaskCategory::Feature,
        TaskPriority::Medium,
        TaskComplexity::Small,
    );
    task.git_branch = Some("feature/test-branch".to_string());
    let task_id = task.id;
    {
        let mut tasks = state.tasks.write().await;
        tasks.insert(task_id, task);
    }

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/github/pr/{}", task_id))
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    assert!(status == StatusCode::SERVICE_UNAVAILABLE || status == StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let err = json["error"].as_str().unwrap();
    assert!(err.contains("token") || err.contains("owner") || err.contains("repo"));
}

#[tokio::test]
async fn test_create_pr_stacked_base_branch_accepted() {
    let (app, state) = test_app();

    let mut task = Task::new(
        "Stacked PR task",
        Uuid::new_v4(),
        TaskCategory::Feature,
        TaskPriority::Medium,
        TaskComplexity::Small,
    );
    task.git_branch = Some("feature/child".to_string());
    let task_id = task.id;
    {
        let mut tasks = state.tasks.write().await;
        tasks.insert(task_id, task);
    }

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/github/pr/{}", task_id))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"base_branch":"feature/parent"}"#))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    assert!(
        status == StatusCode::SERVICE_UNAVAILABLE || status == StatusCode::BAD_REQUEST,
        "Expected 503 or 400, got {}",
        status
    );
}

// -----------------------------------------------------------------------
// Notification endpoint tests
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_notifications_empty() {
    let (app, _state) = test_app();

    let req = Request::builder()
        .method("GET")
        .uri("/api/notifications")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(json.is_empty());
}

#[tokio::test]
async fn test_notification_count_empty() {
    let (app, _state) = test_app();

    let req = Request::builder()
        .method("GET")
        .uri("/api/notifications/count")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["unread"], 0);
    assert_eq!(json["total"], 0);
}

#[tokio::test]
async fn test_notification_crud() {
    let (_app, state) = test_app();

    let notif_id;
    {
        let mut store = state.notification_store.write().await;
        notif_id = store.add(
            "Test Alert",
            "Something happened",
            crate::notifications::NotificationLevel::Info,
            "system",
        );
    }

    let app = router::api_router(state.clone());
    let req = Request::builder()
        .method("GET")
        .uri("/api/notifications")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.len(), 1);
    assert_eq!(json[0]["title"], "Test Alert");

    let app = router::api_router(state.clone());
    let req = Request::builder()
        .method("GET")
        .uri("/api/notifications/count")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["unread"], 1);

    let app = router::api_router(state.clone());
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/notifications/{}/read", notif_id))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let app = router::api_router(state.clone());
    let req = Request::builder()
        .method("GET")
        .uri("/api/notifications/count")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["unread"], 0);
    assert_eq!(json["total"], 1);

    let app = router::api_router(state.clone());
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/notifications/{}", notif_id))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let app = router::api_router(state.clone());
    let req = Request::builder()
        .method("GET")
        .uri("/api/notifications/count")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["total"], 0);
}

#[tokio::test]
async fn test_mark_all_read() {
    let (_app, state) = test_app();

    {
        let mut store = state.notification_store.write().await;
        store.add(
            "n1",
            "m1",
            crate::notifications::NotificationLevel::Info,
            "system",
        );
        store.add(
            "n2",
            "m2",
            crate::notifications::NotificationLevel::Warning,
            "system",
        );
        store.add(
            "n3",
            "m3",
            crate::notifications::NotificationLevel::Error,
            "system",
        );
    }

    let app = router::api_router(state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/api/notifications/read-all")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let app = router::api_router(state.clone());
    let req = Request::builder()
        .method("GET")
        .uri("/api/notifications/count")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["unread"], 0);
    assert_eq!(json["total"], 3);
}

#[tokio::test]
async fn test_notification_unread_filter() {
    let (_app, state) = test_app();

    let id1;
    {
        let mut store = state.notification_store.write().await;
        id1 = store.add(
            "n1",
            "m1",
            crate::notifications::NotificationLevel::Info,
            "system",
        );
        store.add(
            "n2",
            "m2",
            crate::notifications::NotificationLevel::Warning,
            "system",
        );
        store.mark_read(id1);
    }

    let app = router::api_router(state.clone());
    let req = Request::builder()
        .method("GET")
        .uri("/api/notifications?unread=true")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.len(), 1);
    assert_eq!(json[0]["title"], "n2");
}

#[tokio::test]
async fn test_notification_pagination() {
    let (_app, state) = test_app();

    {
        let mut store = state.notification_store.write().await;
        for i in 0..10 {
            store.add(
                format!("n{i}"),
                "msg",
                crate::notifications::NotificationLevel::Info,
                "system",
            );
        }
    }

    let app = router::api_router(state.clone());
    let req = Request::builder()
        .method("GET")
        .uri("/api/notifications?limit=3&offset=0")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.len(), 3);
    assert_eq!(json[0]["title"], "n9");
}

#[tokio::test]
async fn test_bead_pagination() {
    let (_app, state) = test_app();

    {
        let mut beads = state.beads.write().await;
        for i in 0..10 {
            let b = Bead::new(format!("bead{i}"), Lane::Standard);
            beads.insert(b.id, b);
        }
    }

    let app = router::api_router(state.clone());
    let req = Request::builder()
        .method("GET")
        .uri("/api/beads?limit=3&offset=0")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.len(), 3);
    assert!(json[0]["title"].as_str().unwrap().starts_with("bead"));
}

#[tokio::test]
async fn test_agent_pagination() {
    let (_app, state) = test_app();

    {
        let mut agents = state.agents.write().await;
        for i in 0..10 {
            let a = Agent::new(
                format!("agent{i}"),
                at_core::types::AgentRole::Crew,
                CliType::Claude,
            );
            agents.insert(a.id, a);
        }
    }

    let app = router::api_router(state.clone());
    let req = Request::builder()
        .method("GET")
        .uri("/api/agents?limit=3&offset=0")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.len(), 3);
    assert!(json[0]["name"].as_str().unwrap().starts_with("agent"));
}

#[tokio::test]
async fn test_delete_notification_not_found() {
    let (app, _state) = test_app();

    let fake_id = Uuid::new_v4();
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/notifications/{}", fake_id))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_mark_read_not_found() {
    let (app, _state) = test_app();

    let fake_id = Uuid::new_v4();
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/notifications/{}/read", fake_id))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// -----------------------------------------------------------------------
// Execute pipeline endpoint tests
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_execute_pipeline_task_not_found() {
    let (app, _state) = test_app();
    let fake_id = Uuid::new_v4();

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/tasks/{}/execute", fake_id))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_execute_pipeline_wrong_phase() {
    let (app, state) = test_app();

    let task = Task::new(
        "Test task",
        Uuid::new_v4(),
        TaskCategory::Feature,
        TaskPriority::Medium,
        TaskComplexity::Small,
    );
    let task_id = task.id;
    state.tasks.write().await.insert(task_id, task);

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/tasks/{}/execute", task_id))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_execute_pipeline_accepts_from_planning() {
    let (app, state) = test_app();

    let mut task = Task::new(
        "Test task",
        Uuid::new_v4(),
        TaskCategory::Feature,
        TaskPriority::Medium,
        TaskComplexity::Small,
    );
    task.set_phase(TaskPhase::Planning);
    let task_id = task.id;
    state.tasks.write().await.insert(task_id, task);

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/tasks/{}/execute", task_id))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "started");

    let tasks = state.tasks.read().await;
    let t = tasks.get(&task_id).unwrap();
    assert_eq!(t.phase, TaskPhase::Coding);
}

#[tokio::test]
async fn test_pipeline_queue_status_endpoint() {
    let (app, state) = test_app();

    state.pipeline_waiting.store(2, Ordering::SeqCst);
    state.pipeline_running.store(1, Ordering::SeqCst);

    let req = Request::builder()
        .method("GET")
        .uri("/api/pipeline/queue")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["limit"], state.pipeline_max_concurrent as u64);
    assert_eq!(json["waiting"], 2);
    assert_eq!(json["running"], 1);
    assert!(json["available_permits"].as_u64().is_some());
}

#[tokio::test]
async fn test_list_attachments_empty() {
    let (app, _) = test_app();
    let id = Uuid::new_v4();
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/tasks/{id}/attachments"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_save_and_get_draft() {
    let (app, _) = test_app();
    let draft_id = Uuid::new_v4();
    let draft = serde_json::json!({
        "id": draft_id,
        "title": "Test draft",
        "description": "A draft task",
        "category": null,
        "priority": null,
        "files": [],
        "updated_at": ""
    });
    let req = Request::builder()
        .method("POST")
        .uri("/api/tasks/drafts")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&draft).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_list_drafts() {
    let (app, _) = test_app();
    let req = Request::builder()
        .method("GET")
        .uri("/api/tasks/drafts")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_lock_column() {
    let (app, _) = test_app();
    let body = serde_json::json!({"column_id": "done", "locked": true});
    let req = Request::builder()
        .method("POST")
        .uri("/api/kanban/columns/lock")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_save_task_ordering() {
    let (app, _) = test_app();
    let body = serde_json::json!({
        "column_id": "backlog",
        "task_ids": [Uuid::new_v4(), Uuid::new_v4()]
    });
    let req = Request::builder()
        .method("POST")
        .uri("/api/kanban/ordering")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_planning_poker_roundtrip() {
    let (app, _state) = test_app();

    let create_body = serde_json::json!({
        "title": "Estimate API migration",
        "description": "Planning poker bead",
        "lane": "standard"
    });
    let create_req = Request::builder()
        .method("POST")
        .uri("/api/beads")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
        .unwrap();
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let create_bytes = axum::body::to_bytes(create_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(&create_bytes).unwrap();
    let bead_id = created["id"].as_str().unwrap();

    let start_body = serde_json::json!({ "bead_id": bead_id });
    let start_req = Request::builder()
        .method("POST")
        .uri("/api/kanban/poker/start")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&start_body).unwrap()))
        .unwrap();
    let start_resp = app.clone().oneshot(start_req).await.unwrap();
    assert_eq!(start_resp.status(), StatusCode::CREATED);

    let vote_a = serde_json::json!({
        "bead_id": bead_id,
        "voter": "alice",
        "card": "5"
    });
    let vote_a_req = Request::builder()
        .method("POST")
        .uri("/api/kanban/poker/vote")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&vote_a).unwrap()))
        .unwrap();
    let vote_a_resp = app.clone().oneshot(vote_a_req).await.unwrap();
    assert_eq!(vote_a_resp.status(), StatusCode::OK);

    let vote_b = serde_json::json!({
        "bead_id": bead_id,
        "voter": "bob",
        "card": "8"
    });
    let vote_b_req = Request::builder()
        .method("POST")
        .uri("/api/kanban/poker/vote")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&vote_b).unwrap()))
        .unwrap();
    let vote_b_resp = app.clone().oneshot(vote_b_req).await.unwrap();
    assert_eq!(vote_b_resp.status(), StatusCode::OK);

    let get_req = Request::builder()
        .method("GET")
        .uri(format!("/api/kanban/poker/{bead_id}"))
        .body(Body::empty())
        .unwrap();
    let get_resp = app.clone().oneshot(get_req).await.unwrap();
    assert_eq!(get_resp.status(), StatusCode::OK);
    let get_bytes = axum::body::to_bytes(get_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let get_json: serde_json::Value = serde_json::from_slice(&get_bytes).unwrap();
    assert_eq!(get_json["phase"], "voting");
    assert_eq!(get_json["revealed"], false);
    assert_eq!(get_json["vote_count"], 2);
    assert_eq!(get_json["votes"][0]["has_voted"], true);
    assert_eq!(get_json["votes"][1]["has_voted"], true);
    assert!(get_json["votes"][0]["card"].is_null());
    assert!(get_json["votes"][1]["card"].is_null());

    let reveal_body = serde_json::json!({ "bead_id": bead_id });
    let reveal_req = Request::builder()
        .method("POST")
        .uri("/api/kanban/poker/reveal")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&reveal_body).unwrap()))
        .unwrap();
    let reveal_resp = app.clone().oneshot(reveal_req).await.unwrap();
    assert_eq!(reveal_resp.status(), StatusCode::OK);
    let reveal_bytes = axum::body::to_bytes(reveal_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let reveal_json: serde_json::Value = serde_json::from_slice(&reveal_bytes).unwrap();
    assert_eq!(reveal_json["phase"], "revealed");
    assert_eq!(reveal_json["revealed"], true);
    assert_eq!(reveal_json["vote_count"], 2);
    assert_eq!(reveal_json["votes"][0]["card"], "5");
    assert_eq!(reveal_json["votes"][1]["card"], "8");
    assert!(reveal_json["consensus_card"].is_null());
    assert_eq!(reveal_json["stats"]["numeric_vote_count"], 2);
    assert_eq!(reveal_json["stats"]["min"], 5.0);
    assert_eq!(reveal_json["stats"]["max"], 8.0);
}

#[tokio::test]
async fn test_planning_poker_requires_existing_bead() {
    let (app, _) = test_app();
    let body = serde_json::json!({ "bead_id": Uuid::new_v4() });
    let req = Request::builder()
        .method("POST")
        .uri("/api/kanban/poker/start")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_planning_poker_participants_answered_first() {
    let (app, _state) = test_app();
    let create_body = serde_json::json!({
        "title": "Estimate queue",
        "description": "Planning poker ordering",
        "lane": "standard"
    });
    let create_req = Request::builder()
        .method("POST")
        .uri("/api/beads")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
        .unwrap();
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    let create_bytes = axum::body::to_bytes(create_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(&create_bytes).unwrap();
    let bead_id = created["id"].as_str().unwrap();

    let start_body = serde_json::json!({
        "bead_id": bead_id,
        "participants": ["zoe", "amy", "liam"]
    });
    let start_req = Request::builder()
        .method("POST")
        .uri("/api/kanban/poker/start")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&start_body).unwrap()))
        .unwrap();
    let start_resp = app.clone().oneshot(start_req).await.unwrap();
    assert_eq!(start_resp.status(), StatusCode::CREATED);

    let vote_body = serde_json::json!({
        "bead_id": bead_id,
        "voter": "liam",
        "card": "3"
    });
    let vote_req = Request::builder()
        .method("POST")
        .uri("/api/kanban/poker/vote")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&vote_body).unwrap()))
        .unwrap();
    let vote_resp = app.clone().oneshot(vote_req).await.unwrap();
    assert_eq!(vote_resp.status(), StatusCode::OK);

    let get_req = Request::builder()
        .method("GET")
        .uri(format!("/api/kanban/poker/{bead_id}"))
        .body(Body::empty())
        .unwrap();
    let get_resp = app.clone().oneshot(get_req).await.unwrap();
    let get_bytes = axum::body::to_bytes(get_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let get_json: serde_json::Value = serde_json::from_slice(&get_bytes).unwrap();
    assert_eq!(get_json["votes"][0]["voter"], "liam");
    assert_eq!(get_json["votes"][0]["has_voted"], true);
    assert_eq!(get_json["votes"][1]["voter"], "amy");
    assert_eq!(get_json["votes"][2]["voter"], "zoe");
    assert_eq!(get_json["votes"][1]["has_voted"], false);
    assert_eq!(get_json["votes"][2]["has_voted"], false);
}

#[tokio::test]
async fn test_planning_poker_custom_deck_validation() {
    let (app, _state) = test_app();
    let create_body = serde_json::json!({
        "title": "Estimate parser",
        "description": "Deck validation",
        "lane": "standard"
    });
    let create_req = Request::builder()
        .method("POST")
        .uri("/api/beads")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
        .unwrap();
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    let create_bytes = axum::body::to_bytes(create_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(&create_bytes).unwrap();
    let bead_id = created["id"].as_str().unwrap();

    let start_body = serde_json::json!({
        "bead_id": bead_id,
        "custom_deck": ["A", "B"]
    });
    let start_req = Request::builder()
        .method("POST")
        .uri("/api/kanban/poker/start")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&start_body).unwrap()))
        .unwrap();
    let start_resp = app.clone().oneshot(start_req).await.unwrap();
    assert_eq!(start_resp.status(), StatusCode::CREATED);

    let bad_vote = serde_json::json!({
        "bead_id": bead_id,
        "voter": "dev",
        "card": "C"
    });
    let bad_vote_req = Request::builder()
        .method("POST")
        .uri("/api/kanban/poker/vote")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&bad_vote).unwrap()))
        .unwrap();
    let bad_vote_resp = app.clone().oneshot(bad_vote_req).await.unwrap();
    assert_eq!(bad_vote_resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_planning_poker_unknown_deck_preset() {
    let (app, _state) = test_app();
    let create_body = serde_json::json!({
        "title": "Estimate auth",
        "description": "Unknown preset",
        "lane": "standard"
    });
    let create_req = Request::builder()
        .method("POST")
        .uri("/api/beads")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
        .unwrap();
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    let create_bytes = axum::body::to_bytes(create_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(&create_bytes).unwrap();
    let bead_id = created["id"].as_str().unwrap();

    let start_body = serde_json::json!({
        "bead_id": bead_id,
        "deck_preset": "not-a-deck"
    });
    let start_req = Request::builder()
        .method("POST")
        .uri("/api/kanban/poker/start")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&start_body).unwrap()))
        .unwrap();
    let start_resp = app.clone().oneshot(start_req).await.unwrap();
    assert_eq!(start_resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_planning_poker_vote_after_reveal_conflict() {
    let (app, _state) = test_app();
    let create_body = serde_json::json!({
        "title": "Estimate ws backlog",
        "description": "Vote after reveal",
        "lane": "standard"
    });
    let create_req = Request::builder()
        .method("POST")
        .uri("/api/beads")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
        .unwrap();
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    let create_bytes = axum::body::to_bytes(create_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(&create_bytes).unwrap();
    let bead_id = created["id"].as_str().unwrap();

    let start_body = serde_json::json!({ "bead_id": bead_id });
    let start_req = Request::builder()
        .method("POST")
        .uri("/api/kanban/poker/start")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&start_body).unwrap()))
        .unwrap();
    let start_resp = app.clone().oneshot(start_req).await.unwrap();
    assert_eq!(start_resp.status(), StatusCode::CREATED);

    let reveal_body = serde_json::json!({ "bead_id": bead_id });
    let reveal_req = Request::builder()
        .method("POST")
        .uri("/api/kanban/poker/reveal")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&reveal_body).unwrap()))
        .unwrap();
    let reveal_resp = app.clone().oneshot(reveal_req).await.unwrap();
    assert_eq!(reveal_resp.status(), StatusCode::OK);

    let vote_body = serde_json::json!({
        "bead_id": bead_id,
        "voter": "qa",
        "card": "5"
    });
    let vote_req = Request::builder()
        .method("POST")
        .uri("/api/kanban/poker/vote")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&vote_body).unwrap()))
        .unwrap();
    let vote_resp = app.clone().oneshot(vote_req).await.unwrap();
    assert_eq!(vote_resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_planning_poker_simulate_virtual_agents() {
    let (app, _state) = test_app();
    let create_body = serde_json::json!({
        "title": "Estimate retries",
        "description": "Virtual poker simulation",
        "lane": "standard"
    });
    let create_req = Request::builder()
        .method("POST")
        .uri("/api/beads")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
        .unwrap();
    let create_resp = app.clone().oneshot(create_req).await.unwrap();
    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let create_bytes = axum::body::to_bytes(create_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(&create_bytes).unwrap();
    let bead_id = created["id"].as_str().unwrap();

    let simulate_body = serde_json::json!({
        "bead_id": bead_id,
        "agent_count": 4,
        "focus_card": "8",
        "auto_reveal": true
    });
    let simulate_req = Request::builder()
        .method("POST")
        .uri("/api/kanban/poker/simulate")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&simulate_body).unwrap()))
        .unwrap();
    let simulate_resp = app.clone().oneshot(simulate_req).await.unwrap();
    assert_eq!(simulate_resp.status(), StatusCode::OK);
    let simulate_bytes = axum::body::to_bytes(simulate_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let simulate_json: serde_json::Value = serde_json::from_slice(&simulate_bytes).unwrap();
    assert_eq!(simulate_json["phase"], "revealed");
    assert_eq!(simulate_json["vote_count"], 4);
    assert_eq!(simulate_json["votes"].as_array().unwrap().len(), 4);
    for vote in simulate_json["votes"].as_array().unwrap() {
        assert_eq!(vote["has_voted"], true);
        assert!(vote["card"].is_string());
    }
}

#[tokio::test]
async fn test_convert_idea_auto_simulates_planning_poker() {
    let (app, _state) = test_app();

    let generate_body = serde_json::json!({
        "category": "performance",
        "context": "profiling cache misses in parser"
    });
    let generate_req = Request::builder()
        .method("POST")
        .uri("/api/ideation/generate")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&generate_body).unwrap()))
        .unwrap();
    let generate_resp = app.clone().oneshot(generate_req).await.unwrap();
    assert_eq!(generate_resp.status(), StatusCode::CREATED);
    let generate_bytes = axum::body::to_bytes(generate_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let generated: serde_json::Value = serde_json::from_slice(&generate_bytes).unwrap();
    let idea_id = generated["ideas"][0]["id"].as_str().unwrap();

    let convert_req = Request::builder()
        .method("POST")
        .uri(format!("/api/ideation/ideas/{idea_id}/convert"))
        .body(Body::empty())
        .unwrap();
    let convert_resp = app.clone().oneshot(convert_req).await.unwrap();
    assert_eq!(convert_resp.status(), StatusCode::OK);
    let convert_bytes = axum::body::to_bytes(convert_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let converted: serde_json::Value = serde_json::from_slice(&convert_bytes).unwrap();
    let bead_id = converted["id"].as_str().unwrap();
    assert!(converted["planning_poker"].is_object());
    assert_eq!(converted["planning_poker"]["phase"], "revealed");
    assert_eq!(converted["planning_poker"]["vote_count"], 5);

    let session_req = Request::builder()
        .method("GET")
        .uri(format!("/api/kanban/poker/{bead_id}"))
        .body(Body::empty())
        .unwrap();
    let session_resp = app.clone().oneshot(session_req).await.unwrap();
    assert_eq!(session_resp.status(), StatusCode::OK);
    let session_bytes = axum::body::to_bytes(session_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let session: serde_json::Value = serde_json::from_slice(&session_bytes).unwrap();
    assert_eq!(session["phase"], "revealed");
    assert_eq!(session["vote_count"], 5);
}

#[tokio::test]
async fn test_file_watch() {
    let (app, _) = test_app();
    let body = serde_json::json!({"path": "/tmp/test", "recursive": true});
    let req = Request::builder()
        .method("POST")
        .uri("/api/files/watch")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_competitor_analysis() {
    let (app, _) = test_app();
    let body = serde_json::json!({
        "competitor_name": "CompetitorX",
        "competitor_url": "https://competitor.com",
        "focus_areas": ["pricing", "features"]
    });
    let req = Request::builder()
        .method("POST")
        .uri("/api/roadmap/competitor-analysis")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_app_update_check() {
    let (app, _) = test_app();
    let req = Request::builder()
        .method("GET")
        .uri("/api/notifications/app-update")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
