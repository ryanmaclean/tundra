use std::sync::Arc;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, ApiState};
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

#[tokio::test]
async fn project_creation_preserves_utc_wire_and_state_timestamp() {
    let state = Arc::new(ApiState::new(EventBus::new()).with_relaxed_rate_limits());
    let app = api_router(state.clone());
    let request = Request::builder()
        .method("POST")
        .uri("/api/projects")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"name": "example", "path": "/tmp/example"}).to_string(),
        ))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    let text = body["created_at"].as_str().unwrap();
    let parsed = chrono::DateTime::parse_from_rfc3339(text).unwrap();
    assert_eq!(text, parsed.with_timezone(&chrono::Utc).to_rfc3339());
    let projects = state.projects.read().await;
    let stored = projects
        .values()
        .find(|p| p.id.to_string() == body["id"].as_str().unwrap())
        .unwrap();
    assert_eq!(stored.created_at, text);
}

fn assert_legacy_utc(value: &Value) -> &str {
    let text = value.as_str().unwrap();
    let parsed = chrono::DateTime::parse_from_rfc3339(text).unwrap();
    assert_eq!(text, parsed.with_timezone(&chrono::Utc).to_rfc3339());
    text
}

#[tokio::test]
async fn default_project_and_attachment_timestamps_keep_wire_format() {
    let state = Arc::new(ApiState::new(EventBus::new()).with_relaxed_rate_limits());
    let initial =
        serde_json::to_value(state.projects.read().await.values().next().unwrap()).unwrap();
    assert_legacy_utc(&initial["created_at"]);
    let task_id = uuid::Uuid::new_v4();
    let request = Request::builder()
        .method("POST")
        .uri(format!("/api/tasks/{task_id}/attachments"))
        .header("content-type", "application/json")
        .body(Body::from(json!({"filename": "example.txt"}).to_string()))
        .unwrap();
    let response = api_router(state.clone()).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    let timestamp = assert_legacy_utc(&body["uploaded_at"]);
    let attachments = state.attachments.read().await;
    assert_eq!(attachments.values().next().unwrap().uploaded_at, timestamp);
}

#[tokio::test]
async fn saved_draft_overwrites_and_preserves_utc_timestamp() {
    let state = Arc::new(ApiState::new(EventBus::new()).with_relaxed_rate_limits());
    let draft_id = uuid::Uuid::new_v4();
    let request = Request::builder()
        .method("POST")
        .uri("/api/tasks/drafts")
        .header("content-type", "application/json")
        .body(Body::from(json!({"id":draft_id,"title":"Example","description":"body","category":null,"priority":null,"files":[],"updated_at":"old"}).to_string()))
        .unwrap();
    let response = api_router(state.clone()).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    let timestamp = assert_legacy_utc(&body["updated_at"]);
    assert_ne!(timestamp, "old");
    let drafts = state.task_drafts.read().await;
    assert_eq!(drafts.get(&draft_id).unwrap().updated_at, timestamp);
}
