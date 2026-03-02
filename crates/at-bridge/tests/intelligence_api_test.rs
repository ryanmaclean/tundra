//! Integration tests for Intelligence API endpoints (Insights, Ideation, Memory).

use std::sync::Arc;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, ApiState};
use serde_json::{json, Value};

/// Spin up an API server on a random port, return the base URL and shared state.
async fn start_test_server() -> (String, Arc<ApiState>) {
    let event_bus = EventBus::new();
    let state = Arc::new(ApiState::new(event_bus).with_relaxed_rate_limits());
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

// ===========================================================================
// Insights Endpoints
// ===========================================================================

#[tokio::test]
async fn test_get_insights_sessions_empty() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/insights/sessions"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = resp.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn test_post_insights_session_creates_new() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/insights/sessions"))
        .json(&json!({
            "title": "Test Chat Session",
            "model": "claude-3"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["title"], "Test Chat Session");
    assert_eq!(body["model"], "claude-3");
    assert!(body["id"].is_string());
    assert!(body["created_at"].is_string());
    assert!(body["messages"].as_array().unwrap().is_empty());

    // Verify it shows up in listing
    let resp = reqwest::get(format!("{base}/api/insights/sessions"))
        .await
        .unwrap();
    let sessions: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["title"], "Test Chat Session");
}

#[tokio::test]
async fn test_post_insights_message_returns_response() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create a session first
    let resp = client
        .post(format!("{base}/api/insights/sessions"))
        .json(&json!({
            "title": "Message Test",
            "model": "claude-3"
        }))
        .send()
        .await
        .unwrap();
    let session: Value = resp.json().await.unwrap();
    let session_id = session["id"].as_str().unwrap();

    // Add a message to the session
    let resp = client
        .post(format!(
            "{base}/api/insights/sessions/{session_id}/messages"
        ))
        .json(&json!({
            "content": "Explain the project structure"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["ok"], true);
}

#[tokio::test]
async fn test_delete_insights_session() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create a session
    let resp = client
        .post(format!("{base}/api/insights/sessions"))
        .json(&json!({
            "title": "Doomed Session",
            "model": "model"
        }))
        .send()
        .await
        .unwrap();
    let session: Value = resp.json().await.unwrap();
    let session_id = session["id"].as_str().unwrap();

    // Delete it
    let resp = client
        .delete(format!("{base}/api/insights/sessions/{session_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["deleted"], true);

    // Verify it is gone
    let resp = reqwest::get(format!("{base}/api/insights/sessions"))
        .await
        .unwrap();
    let sessions: Vec<Value> = resp.json().await.unwrap();
    assert!(sessions.is_empty());

    // Deleting again returns 404
    let resp = client
        .delete(format!("{base}/api/insights/sessions/{session_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// ===========================================================================
// Ideation Endpoints
// ===========================================================================

#[tokio::test]
async fn test_get_ideation_ideas_empty() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/ideation/ideas"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = resp.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn test_post_ideation_generate_creates_ideas() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/ideation/generate"))
        .json(&json!({
            "category": "performance",
            "context": "slow database queries need optimization"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["analysis_type"], "performance");
    let ideas = body["ideas"].as_array().unwrap();
    assert_eq!(ideas.len(), 1);
    assert!(ideas[0]["title"].as_str().unwrap().contains("performance"));
    assert!(ideas[0]["id"].is_string());
    assert!(ideas[0]["description"]
        .as_str()
        .unwrap()
        .contains("slow database queries"));

    // Verify ideas show up in listing
    let resp = reqwest::get(format!("{base}/api/ideation/ideas"))
        .await
        .unwrap();
    let all_ideas: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(all_ideas.len(), 1);
}

#[tokio::test]
async fn test_post_ideation_convert_to_task() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Generate an idea first
    let resp = client
        .post(format!("{base}/api/ideation/generate"))
        .json(&json!({
            "category": "code_improvement",
            "context": "refactor auth module"
        }))
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    let idea_id = body["ideas"][0]["id"].as_str().unwrap();

    // Convert to task
    let resp = client
        .post(format!("{base}/api/ideation/ideas/{idea_id}/convert"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let bead: Value = resp.json().await.unwrap();
    assert!(bead["title"].as_str().unwrap().contains("code_improvement"));
    assert!(bead["description"].is_string());
    assert!(bead["id"].is_string());

    // Convert non-existent idea returns 404
    let fake_id = uuid::Uuid::new_v4();
    let resp = client
        .post(format!("{base}/api/ideation/ideas/{fake_id}/convert"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// ===========================================================================
// Memory Endpoints
// ===========================================================================

#[tokio::test]
async fn test_get_memory_empty() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/memory")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = resp.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn test_post_memory_creates_entry() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/memory"))
        .json(&json!({
            "key": "api_url",
            "value": "http://localhost:3000",
            "category": "api_route",
            "source": "config"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let body: Value = resp.json().await.unwrap();
    assert!(body["id"].is_string());

    // Verify it shows up in listing
    let resp = reqwest::get(format!("{base}/api/memory")).await.unwrap();
    let entries: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["key"], "api_url");
    assert_eq!(entries[0]["value"], "http://localhost:3000");
    assert_eq!(entries[0]["category"], "api_route");
}

#[tokio::test]
async fn test_get_memory_search_by_query() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Add multiple memory entries
    client
        .post(format!("{base}/api/memory"))
        .json(&json!({
            "key": "db_url",
            "value": "postgres://localhost:5432",
            "category": "service_endpoint",
            "source": "env"
        }))
        .send()
        .await
        .unwrap();

    client
        .post(format!("{base}/api/memory"))
        .json(&json!({
            "key": "cache_url",
            "value": "redis://localhost:6379",
            "category": "service_endpoint",
            "source": "env"
        }))
        .send()
        .await
        .unwrap();

    client
        .post(format!("{base}/api/memory"))
        .json(&json!({
            "key": "log_level",
            "value": "debug",
            "category": "env_var",
            "source": "config"
        }))
        .send()
        .await
        .unwrap();

    // Search for "localhost" — should match db_url and cache_url
    let resp = reqwest::get(format!("{base}/api/memory/search?q=localhost"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let results: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(results.len(), 2);

    // Search for "debug" — should match log_level only
    let resp = reqwest::get(format!("{base}/api/memory/search?q=debug"))
        .await
        .unwrap();
    let results: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["key"], "log_level");

    // Search for something that matches nothing
    let resp = reqwest::get(format!("{base}/api/memory/search?q=nonexistent_xyz"))
        .await
        .unwrap();
    let results: Vec<Value> = resp.json().await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_delete_memory_entry() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create entry
    let resp = client
        .post(format!("{base}/api/memory"))
        .json(&json!({
            "key": "temp_key",
            "value": "temp_value",
            "category": "pattern",
            "source": "test"
        }))
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    let entry_id = body["id"].as_str().unwrap();

    // Delete it
    let resp = client
        .delete(format!("{base}/api/memory/{entry_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["deleted"], true);

    // Verify it is gone
    let resp = reqwest::get(format!("{base}/api/memory")).await.unwrap();
    let entries: Vec<Value> = resp.json().await.unwrap();
    assert!(entries.is_empty());

    // Deleting again returns 404
    let resp = client
        .delete(format!("{base}/api/memory/{entry_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_memory_categories() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Test various memory categories that map to the UI categories:
    // PR Reviews -> pattern, Sessions -> convention, Codebase -> architecture,
    // Patterns -> pattern, Gotchas -> keyword
    let categories = vec![
        ("pattern", "review_pattern", "Always check error handling"),
        (
            "convention",
            "naming_convention",
            "Use snake_case for functions",
        ),
        (
            "architecture",
            "codebase_structure",
            "Monorepo with 10 crates",
        ),
        ("keyword", "gotcha_async", "Watch for async deadlocks"),
        ("dependency", "tokio_version", "tokio 1.x required"),
    ];

    for (category, key, value) in &categories {
        let resp = client
            .post(format!("{base}/api/memory"))
            .json(&json!({
                "key": key,
                "value": value,
                "category": category,
                "source": "test"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201);
    }

    // Verify all entries exist
    let resp = reqwest::get(format!("{base}/api/memory")).await.unwrap();
    let entries: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(entries.len(), categories.len());

    // Verify categories are stored correctly
    let cats: Vec<String> = entries
        .iter()
        .map(|e| e["category"].as_str().unwrap().to_string())
        .collect();
    assert!(cats.contains(&"pattern".to_string()));
    assert!(cats.contains(&"convention".to_string()));
    assert!(cats.contains(&"architecture".to_string()));
    assert!(cats.contains(&"keyword".to_string()));
    assert!(cats.contains(&"dependency".to_string()));
}

// ===========================================================================
// Additional edge cases
// ===========================================================================

#[tokio::test]
async fn test_post_message_to_nonexistent_session_returns_error() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let resp = client
        .post(format!("{base}/api/insights/sessions/{fake_id}/messages"))
        .json(&json!({
            "content": "hello?"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_multiple_ideation_categories() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let categories = vec![
        "code_improvement",
        "quality",
        "performance",
        "security",
        "ui_ux",
        "documentation",
    ];

    for cat in &categories {
        let resp = client
            .post(format!("{base}/api/ideation/generate"))
            .json(&json!({
                "category": cat,
                "context": format!("context for {cat}")
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201);
    }

    // Verify all ideas are listed
    let resp = reqwest::get(format!("{base}/api/ideation/ideas"))
        .await
        .unwrap();
    let all: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(all.len(), categories.len());
}

#[tokio::test]
async fn test_multiple_insights_sessions_with_messages() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create two sessions
    let resp = client
        .post(format!("{base}/api/insights/sessions"))
        .json(&json!({"title": "Session A", "model": "m1"}))
        .send()
        .await
        .unwrap();
    let sa: Value = resp.json().await.unwrap();
    let id_a = sa["id"].as_str().unwrap();

    let resp = client
        .post(format!("{base}/api/insights/sessions"))
        .json(&json!({"title": "Session B", "model": "m2"}))
        .send()
        .await
        .unwrap();
    let sb: Value = resp.json().await.unwrap();
    let id_b = sb["id"].as_str().unwrap();

    // Add messages to session A
    client
        .post(format!("{base}/api/insights/sessions/{id_a}/messages"))
        .json(&json!({"content": "Message for A"}))
        .send()
        .await
        .unwrap();

    // Add messages to session B
    client
        .post(format!("{base}/api/insights/sessions/{id_b}/messages"))
        .json(&json!({"content": "Message for B"}))
        .send()
        .await
        .unwrap();

    // Verify both sessions exist
    let resp = reqwest::get(format!("{base}/api/insights/sessions"))
        .await
        .unwrap();
    let sessions: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(sessions.len(), 2);
}

#[tokio::test]
async fn test_delete_nonexistent_memory_returns_404() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let resp = client
        .delete(format!("{base}/api/memory/{fake_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_delete_nonexistent_session_returns_404() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let resp = client
        .delete(format!("{base}/api/insights/sessions/{fake_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}
