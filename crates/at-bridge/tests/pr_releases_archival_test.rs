use std::sync::Arc;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, ApiState, PrPollStatus};
use serde_json::Value;

/// Spin up an API server on a random port, return the base URL and state.
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

// ---------------------------------------------------------------------------
// Feature 1: PR Poll Status
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_pr_poll_status_serialization_roundtrip() {
    let status = PrPollStatus {
        pr_number: 42,
        state: "open".to_string(),
        mergeable: Some(true),
        checks_passed: Some(false),
        last_polled: chrono::Utc::now(),
    };

    let json = serde_json::to_string(&status).unwrap();
    let deserialized: PrPollStatus = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.pr_number, 42);
    assert_eq!(deserialized.state, "open");
    assert_eq!(deserialized.mergeable, Some(true));
    assert_eq!(deserialized.checks_passed, Some(false));
}

#[tokio::test]
async fn test_watch_and_list_prs() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Watch PR #10
    let resp = client
        .post(format!("{base}/api/github/pr/10/watch"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["pr_number"], 10);
    assert_eq!(body["state"], "open");

    // Watch PR #20
    let resp = client
        .post(format!("{base}/api/github/pr/20/watch"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // List watched
    let resp = client
        .get(format!("{base}/api/github/pr/watched"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 2);
}

#[tokio::test]
async fn test_unwatch_pr() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Watch then unwatch
    client
        .post(format!("{base}/api/github/pr/5/watch"))
        .send()
        .await
        .unwrap();

    let resp = client
        .delete(format!("{base}/api/github/pr/5/watch"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Unwatch non-existent returns 404
    let resp = client
        .delete(format!("{base}/api/github/pr/999/watch"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// ---------------------------------------------------------------------------
// Feature 2: GitHub Releases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_release_struct() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/github/releases"))
        .json(&serde_json::json!({
            "tag_name": "v1.0.0",
            "name": "First Release",
            "body": "Initial stable release",
            "draft": false,
            "prerelease": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["tag_name"], "v1.0.0");
    assert_eq!(body["name"], "First Release");
    assert_eq!(body["draft"], false);
    assert_eq!(body["prerelease"], false);
    assert!(body["created_at"].is_string());
    assert!(body["html_url"].is_string());
}

#[tokio::test]
async fn test_list_releases() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Empty initially
    let resp = client
        .get(format!("{base}/api/github/releases"))
        .send()
        .await
        .unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();
    assert!(body.is_empty());

    // Create two releases
    for tag in &["v0.1.0", "v0.2.0"] {
        client
            .post(format!("{base}/api/github/releases"))
            .json(&serde_json::json!({
                "tag_name": tag,
                "draft": true,
                "prerelease": true
            }))
            .send()
            .await
            .unwrap();
    }

    let resp = client
        .get(format!("{base}/api/github/releases"))
        .send()
        .await
        .unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 2);
}

// ---------------------------------------------------------------------------
// Feature 3: Task Archival
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_archive_and_unarchive_task() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let id = uuid::Uuid::new_v4();

    // Archive
    let resp = client
        .post(format!("{base}/api/tasks/{id}/archive"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // List archived
    let resp = client
        .get(format!("{base}/api/tasks/archived"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 1);

    // Archive the same ID again — should be idempotent
    client
        .post(format!("{base}/api/tasks/{id}/archive"))
        .send()
        .await
        .unwrap();

    let resp = client
        .get(format!("{base}/api/tasks/archived"))
        .send()
        .await
        .unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(body.len(), 1);

    // Unarchive
    let resp = client
        .post(format!("{base}/api/tasks/{id}/unarchive"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = client
        .get(format!("{base}/api/tasks/archived"))
        .send()
        .await
        .unwrap();
    let body: Vec<Value> = resp.json().await.unwrap();
    assert!(body.is_empty());
}
