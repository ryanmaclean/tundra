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
        .iter()
        .find(|p| p.id.to_string() == body["id"].as_str().unwrap())
        .unwrap();
    assert_eq!(stored.created_at, text);
}
