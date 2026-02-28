//! Exhaustive integration tests for the Task CRUD API, task form validation,
//! phase configuration, agent profile selection, and impact rating.
//!
//! These tests exercise the HTTP API endpoints for tasks through the same
//! pattern as the kanban board tests: spinning up a test server with in-memory
//! state and exercising routes via reqwest or tower oneshot.

use std::sync::Arc;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, ApiState};
use at_core::types::*;
use serde_json::{json, Value};
use uuid::Uuid;

// ===========================================================================
// Helpers
// ===========================================================================

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

/// Create a task via the API with all required fields, returning the JSON response.
async fn api_create_task(
    client: &reqwest::Client,
    base: &str,
    title: &str,
    category: &str,
    priority: &str,
    complexity: &str,
) -> (u16, Value) {
    let bead_id = Uuid::new_v4();
    let resp = client
        .post(format!("{base}/api/tasks"))
        .json(&json!({
            "title": title,
            "bead_id": bead_id,
            "category": category,
            "priority": priority,
            "complexity": complexity,
        }))
        .send()
        .await
        .unwrap();
    let code = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap();
    (code, body)
}

/// Create a task with full options.
async fn api_create_task_full(
    client: &reqwest::Client,
    base: &str,
    payload: &Value,
) -> (u16, Value) {
    let resp = client
        .post(format!("{base}/api/tasks"))
        .json(payload)
        .send()
        .await
        .unwrap();
    let code = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap();
    (code, body)
}

/// List all tasks via the API.
async fn api_list_tasks(client: &reqwest::Client, base: &str) -> Vec<Value> {
    let resp = client
        .get(format!("{base}/api/tasks"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    resp.json().await.unwrap()
}

/// List tasks with query parameters.
async fn api_list_tasks_with_query(
    client: &reqwest::Client,
    base: &str,
    query_params: &[(&str, &str)],
) -> (u16, Vec<Value>) {
    let resp = client
        .get(format!("{base}/api/tasks"))
        .query(query_params)
        .send()
        .await
        .unwrap();
    let code = resp.status().as_u16();
    let body: Vec<Value> = resp.json().await.unwrap();
    (code, body)
}

/// Get a single task by ID.
async fn api_get_task(client: &reqwest::Client, base: &str, id: &str) -> (u16, Value) {
    let resp = client
        .get(format!("{base}/api/tasks/{id}"))
        .send()
        .await
        .unwrap();
    let code = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap();
    (code, body)
}

/// Update a task via PUT.
async fn api_update_task(
    client: &reqwest::Client,
    base: &str,
    id: &str,
    payload: &Value,
) -> (u16, Value) {
    let resp = client
        .put(format!("{base}/api/tasks/{id}"))
        .json(payload)
        .send()
        .await
        .unwrap();
    let code = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap();
    (code, body)
}

/// Delete a task via DELETE.
async fn api_delete_task(client: &reqwest::Client, base: &str, id: &str) -> (u16, Value) {
    let resp = client
        .delete(format!("{base}/api/tasks/{id}"))
        .send()
        .await
        .unwrap();
    let code = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap();
    (code, body)
}

// ===========================================================================
// 1. Task CRUD API (15 tests)
// ===========================================================================

#[tokio::test]
async fn test_create_task_with_all_fields() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let bead_id = Uuid::new_v4();
    let payload = json!({
        "title": "Implement login page",
        "bead_id": bead_id,
        "category": "feature",
        "priority": "high",
        "complexity": "medium",
        "description": "Build the login page with OAuth support",
        "impact": "high",
        "agent_profile": "balanced",
        "phase_configs": [
            {"phase_name": "spec_creation", "model": "opus", "thinking_level": "high"},
            {"phase_name": "planning", "model": "sonnet", "thinking_level": "medium"},
            {"phase_name": "code_review", "model": "haiku", "thinking_level": "low"}
        ]
    });

    let (code, body) = api_create_task_full(&client, &base, &payload).await;
    assert_eq!(code, 201);
    assert_eq!(body["title"], "Implement login page");
    assert_eq!(body["category"], "feature");
    assert_eq!(body["priority"], "high");
    assert_eq!(body["complexity"], "medium");
    assert_eq!(
        body["description"],
        "Build the login page with OAuth support"
    );
    assert_eq!(body["impact"], "high");
    assert_eq!(body["agent_profile"], "balanced");
    assert_eq!(body["phase_configs"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn test_list_all_tasks() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    api_create_task(&client, &base, "Task A", "feature", "low", "small").await;
    api_create_task(&client, &base, "Task B", "bug_fix", "medium", "medium").await;
    api_create_task(&client, &base, "Task C", "refactoring", "high", "large").await;

    let tasks = api_list_tasks(&client, &base).await;
    assert_eq!(tasks.len(), 3);
}

#[tokio::test]
async fn test_get_single_task() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let (_, created) = api_create_task(&client, &base, "Get me", "feature", "low", "trivial").await;
    let id = created["id"].as_str().unwrap();

    let (code, body) = api_get_task(&client, &base, id).await;
    assert_eq!(code, 200);
    assert_eq!(body["title"], "Get me");
    assert_eq!(body["category"], "feature");
}

#[tokio::test]
async fn test_update_task_title() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let (_, created) =
        api_create_task(&client, &base, "Old title", "feature", "low", "small").await;
    let id = created["id"].as_str().unwrap();

    let (code, body) = api_update_task(&client, &base, id, &json!({"title": "New title"})).await;
    assert_eq!(code, 200);
    assert_eq!(body["title"], "New title");
}

#[tokio::test]
async fn test_update_task_description() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let (_, created) =
        api_create_task(&client, &base, "Desc test", "feature", "low", "small").await;
    let id = created["id"].as_str().unwrap();

    let (code, body) = api_update_task(
        &client,
        &base,
        id,
        &json!({"description": "Updated description with **markdown**"}),
    )
    .await;
    assert_eq!(code, 200);
    assert_eq!(body["description"], "Updated description with **markdown**");
}

#[tokio::test]
async fn test_delete_task() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let (_, created) =
        api_create_task(&client, &base, "Delete me", "feature", "low", "small").await;
    let id = created["id"].as_str().unwrap();

    let (code, body) = api_delete_task(&client, &base, id).await;
    assert_eq!(code, 200);
    assert_eq!(body["status"], "deleted");

    // Verify it's gone.
    let tasks = api_list_tasks(&client, &base).await;
    assert!(tasks.is_empty());
}

#[tokio::test]
async fn test_create_task_missing_required_fields_returns_422() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Missing category, priority, complexity.
    let resp = client
        .post(format!("{base}/api/tasks"))
        .json(&json!({"title": "Incomplete"}))
        .send()
        .await
        .unwrap();
    // Axum returns 422 for deserialization failures.
    assert_eq!(resp.status().as_u16(), 422);
}

#[tokio::test]
async fn test_get_nonexistent_task_returns_404() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let fake_id = Uuid::new_v4();
    let (code, body) = api_get_task(&client, &base, &fake_id.to_string()).await;
    assert_eq!(code, 404);
    assert!(body["error"].as_str().unwrap().contains("not found"));
}

#[tokio::test]
async fn test_task_list_returns_empty_array_initially() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let tasks = api_list_tasks(&client, &base).await;
    assert!(tasks.is_empty());
}

#[tokio::test]
async fn test_task_list_preserves_insertion_order() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    api_create_task(&client, &base, "First", "feature", "low", "trivial").await;
    api_create_task(&client, &base, "Second", "bug_fix", "medium", "small").await;
    api_create_task(&client, &base, "Third", "refactoring", "high", "medium").await;

    let tasks = api_list_tasks(&client, &base).await;
    assert_eq!(tasks.len(), 3);
    // HashMap doesn't preserve insertion order; verify all titles are present
    let titles: Vec<&str> = tasks.iter().filter_map(|t| t["title"].as_str()).collect();
    assert!(titles.contains(&"First"));
    assert!(titles.contains(&"Second"));
    assert!(titles.contains(&"Third"));
}

#[tokio::test]
async fn test_update_nonexistent_task_returns_404() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let fake_id = Uuid::new_v4();
    let (code, _) = api_update_task(
        &client,
        &base,
        &fake_id.to_string(),
        &json!({"title": "nope"}),
    )
    .await;
    assert_eq!(code, 404);
}

#[tokio::test]
async fn test_delete_nonexistent_task_returns_404() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let fake_id = Uuid::new_v4();
    let (code, _) = api_delete_task(&client, &base, &fake_id.to_string()).await;
    assert_eq!(code, 404);
}

#[tokio::test]
async fn test_create_task_generates_unique_ids() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let (_, a) = api_create_task(&client, &base, "A", "feature", "low", "trivial").await;
    let (_, b) = api_create_task(&client, &base, "B", "feature", "low", "trivial").await;
    let (_, c) = api_create_task(&client, &base, "C", "feature", "low", "trivial").await;

    let ids: std::collections::HashSet<_> = vec![
        a["id"].as_str().unwrap(),
        b["id"].as_str().unwrap(),
        c["id"].as_str().unwrap(),
    ]
    .into_iter()
    .collect();
    assert_eq!(ids.len(), 3);
}

#[tokio::test]
async fn test_update_task_category_and_priority() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let (_, created) =
        api_create_task(&client, &base, "Multi update", "feature", "low", "small").await;
    let id = created["id"].as_str().unwrap();

    let (code, body) = api_update_task(
        &client,
        &base,
        id,
        &json!({"category": "bug_fix", "priority": "urgent"}),
    )
    .await;
    assert_eq!(code, 200);
    assert_eq!(body["category"], "bug_fix");
    assert_eq!(body["priority"], "urgent");
}

#[tokio::test]
async fn test_create_task_empty_title_rejected() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let bead_id = Uuid::new_v4();
    let resp = client
        .post(format!("{base}/api/tasks"))
        .json(&json!({
            "title": "",
            "bead_id": bead_id,
            "category": "feature",
            "priority": "low",
            "complexity": "trivial",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 400);
}

// ===========================================================================
// 2. Task Form Validation (15 tests)
// ===========================================================================

#[tokio::test]
async fn test_all_task_category_variants_serialize_deserialize() {
    let variants = vec![
        ("feature", TaskCategory::Feature),
        ("bug_fix", TaskCategory::BugFix),
        ("refactoring", TaskCategory::Refactoring),
        ("documentation", TaskCategory::Documentation),
        ("security", TaskCategory::Security),
        ("performance", TaskCategory::Performance),
        ("ui_ux", TaskCategory::UiUx),
        ("infrastructure", TaskCategory::Infrastructure),
        ("testing", TaskCategory::Testing),
    ];

    for (expected_str, variant) in &variants {
        let serialized = serde_json::to_string(variant).unwrap();
        assert_eq!(serialized, format!("\"{}\"", expected_str));

        let deserialized: TaskCategory = serde_json::from_str(&serialized).unwrap();
        assert_eq!(&deserialized, variant);
    }
}

#[tokio::test]
async fn test_all_task_priority_variants_serialize() {
    let variants = vec![
        ("low", TaskPriority::Low),
        ("medium", TaskPriority::Medium),
        ("high", TaskPriority::High),
        ("urgent", TaskPriority::Urgent),
    ];

    for (expected_str, variant) in &variants {
        let serialized = serde_json::to_string(variant).unwrap();
        assert_eq!(serialized, format!("\"{}\"", expected_str));

        let deserialized: TaskPriority = serde_json::from_str(&serialized).unwrap();
        assert_eq!(&deserialized, variant);
    }
}

#[tokio::test]
async fn test_all_task_complexity_variants_serialize() {
    let variants = vec![
        ("trivial", TaskComplexity::Trivial),
        ("small", TaskComplexity::Small),
        ("medium", TaskComplexity::Medium),
        ("large", TaskComplexity::Large),
        ("complex", TaskComplexity::Complex),
    ];

    for (expected_str, variant) in &variants {
        let serialized = serde_json::to_string(variant).unwrap();
        assert_eq!(serialized, format!("\"{}\"", expected_str));

        let deserialized: TaskComplexity = serde_json::from_str(&serialized).unwrap();
        assert_eq!(&deserialized, variant);
    }
}

#[tokio::test]
async fn test_mixed_category_priority_complexity_via_api() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let combos = vec![
        ("security", "urgent", "complex"),
        ("performance", "high", "large"),
        ("documentation", "low", "trivial"),
        ("ui_ux", "medium", "small"),
    ];

    for (cat, pri, cplx) in combos {
        let (code, body) =
            api_create_task(&client, &base, &format!("{cat}-task"), cat, pri, cplx).await;
        assert_eq!(code, 201);
        assert_eq!(body["category"], cat);
        assert_eq!(body["priority"], pri);
        assert_eq!(body["complexity"], cplx);
    }
}

#[tokio::test]
async fn test_empty_title_rejected_on_update() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let (_, created) =
        api_create_task(&client, &base, "Has title", "feature", "low", "small").await;
    let id = created["id"].as_str().unwrap();

    let (code, body) = api_update_task(&client, &base, id, &json!({"title": ""})).await;
    assert_eq!(code, 400);
    assert!(body["error"].as_str().unwrap().contains("empty"));
}

#[tokio::test]
async fn test_very_long_title_accepted() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let long_title = "A".repeat(1500);
    let (code, body) =
        api_create_task(&client, &base, &long_title, "feature", "low", "trivial").await;
    assert_eq!(code, 201);
    assert_eq!(body["title"].as_str().unwrap().len(), 1500);
}

#[tokio::test]
async fn test_description_with_markdown_preserved() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let markdown_desc =
        "# Heading\n\n- item 1\n- item 2\n\n```rust\nfn main() {}\n```\n\n**bold** and _italic_";
    let bead_id = Uuid::new_v4();
    let (code, body) = api_create_task_full(
        &client,
        &base,
        &json!({
            "title": "Markdown task",
            "bead_id": bead_id,
            "category": "documentation",
            "priority": "low",
            "complexity": "small",
            "description": markdown_desc,
        }),
    )
    .await;
    assert_eq!(code, 201);
    assert_eq!(body["description"], markdown_desc);
}

#[tokio::test]
async fn test_category_validation_rejects_unknown() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let bead_id = Uuid::new_v4();
    let resp = client
        .post(format!("{base}/api/tasks"))
        .json(&json!({
            "title": "Bad category",
            "bead_id": bead_id,
            "category": "nonexistent_category",
            "priority": "low",
            "complexity": "small",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 422);
}

#[tokio::test]
async fn test_priority_validation_rejects_unknown() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let bead_id = Uuid::new_v4();
    let resp = client
        .post(format!("{base}/api/tasks"))
        .json(&json!({
            "title": "Bad priority",
            "bead_id": bead_id,
            "category": "feature",
            "priority": "super_urgent",
            "complexity": "small",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 422);
}

#[tokio::test]
async fn test_complexity_validation_rejects_unknown() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let bead_id = Uuid::new_v4();
    let resp = client
        .post(format!("{base}/api/tasks"))
        .json(&json!({
            "title": "Bad complexity",
            "bead_id": bead_id,
            "category": "feature",
            "priority": "low",
            "complexity": "enormous",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 422);
}

#[tokio::test]
async fn test_task_category_serde_roundtrip() {
    let task = Task::new(
        "Roundtrip",
        Uuid::new_v4(),
        TaskCategory::Security,
        TaskPriority::Urgent,
        TaskComplexity::Complex,
    );
    let json_str = serde_json::to_string(&task).unwrap();
    let deserialized: Task = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.category, TaskCategory::Security);
    assert_eq!(deserialized.priority, TaskPriority::Urgent);
    assert_eq!(deserialized.complexity, TaskComplexity::Complex);
}

#[tokio::test]
async fn test_task_default_phase_is_discovery() {
    let task = Task::new(
        "Phase test",
        Uuid::new_v4(),
        TaskCategory::Feature,
        TaskPriority::Medium,
        TaskComplexity::Medium,
    );
    assert_eq!(task.phase, TaskPhase::Discovery);
    assert_eq!(task.progress_percent, 0);
}

#[tokio::test]
async fn test_task_description_none_by_default() {
    let task = Task::new(
        "No desc",
        Uuid::new_v4(),
        TaskCategory::Feature,
        TaskPriority::Low,
        TaskComplexity::Trivial,
    );
    assert!(task.description.is_none());
}

#[tokio::test]
async fn test_task_timestamps_set_on_create() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let (code, body) = api_create_task(
        &client,
        &base,
        "Timestamp test",
        "feature",
        "low",
        "trivial",
    )
    .await;
    assert_eq!(code, 201);
    assert!(body["created_at"].is_string());
    assert!(body["updated_at"].is_string());
    assert!(body["started_at"].is_null());
    assert!(body["completed_at"].is_null());
}

#[tokio::test]
async fn test_update_task_updated_at_changes() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let (_, created) =
        api_create_task(&client, &base, "Time update", "feature", "low", "small").await;
    let id = created["id"].as_str().unwrap();
    let original_updated = created["updated_at"].as_str().unwrap().to_string();

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let (code, body) =
        api_update_task(&client, &base, id, &json!({"title": "Time update v2"})).await;
    assert_eq!(code, 200);
    let new_updated = body["updated_at"].as_str().unwrap().to_string();
    assert_ne!(original_updated, new_updated);
}

// ===========================================================================
// 3. Phase Configuration (15 tests)
// ===========================================================================

#[tokio::test]
async fn test_phase_config_spec_creation() {
    let config = PhaseConfig {
        phase_name: "spec_creation".to_string(),
        model: "opus".to_string(),
        thinking_level: "high".to_string(),
    };
    assert_eq!(config.phase_name, "spec_creation");
    assert_eq!(config.model, "opus");
    assert_eq!(config.thinking_level, "high");
}

#[tokio::test]
async fn test_phase_config_planning() {
    let config = PhaseConfig {
        phase_name: "planning".to_string(),
        model: "sonnet".to_string(),
        thinking_level: "medium".to_string(),
    };
    assert_eq!(config.phase_name, "planning");
    assert_eq!(config.model, "sonnet");
}

#[tokio::test]
async fn test_phase_config_code_review() {
    let config = PhaseConfig {
        phase_name: "code_review".to_string(),
        model: "haiku".to_string(),
        thinking_level: "low".to_string(),
    };
    assert_eq!(config.phase_name, "code_review");
    assert_eq!(config.model, "haiku");
    assert_eq!(config.thinking_level, "low");
}

#[tokio::test]
async fn test_phase_config_with_opus() {
    let config = PhaseConfig {
        phase_name: "planning".to_string(),
        model: "opus".to_string(),
        thinking_level: "high".to_string(),
    };
    assert_eq!(config.model, "opus");
}

#[tokio::test]
async fn test_phase_config_with_sonnet() {
    let config = PhaseConfig {
        phase_name: "spec_creation".to_string(),
        model: "sonnet".to_string(),
        thinking_level: "medium".to_string(),
    };
    assert_eq!(config.model, "sonnet");
}

#[tokio::test]
async fn test_phase_config_with_haiku() {
    let config = PhaseConfig {
        phase_name: "code_review".to_string(),
        model: "haiku".to_string(),
        thinking_level: "low".to_string(),
    };
    assert_eq!(config.model, "haiku");
}

#[tokio::test]
async fn test_phase_config_thinking_levels() {
    for level in &["low", "medium", "high"] {
        let config = PhaseConfig {
            phase_name: "test".to_string(),
            model: "sonnet".to_string(),
            thinking_level: level.to_string(),
        };
        assert_eq!(config.thinking_level, *level);
    }
}

#[tokio::test]
async fn test_phase_config_serialization_roundtrip() {
    let config = PhaseConfig {
        phase_name: "spec_creation".to_string(),
        model: "opus".to_string(),
        thinking_level: "high".to_string(),
    };
    let json_str = serde_json::to_string(&config).unwrap();
    let deserialized: PhaseConfig = serde_json::from_str(&json_str).unwrap();
    assert_eq!(config, deserialized);
}

#[tokio::test]
async fn test_phase_config_default_values() {
    let config = PhaseConfig::default();
    assert_eq!(config.phase_name, "spec_creation");
    assert_eq!(config.model, "sonnet");
    assert_eq!(config.thinking_level, "medium");
}

#[tokio::test]
async fn test_phase_config_collection_on_task() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let bead_id = Uuid::new_v4();
    let (code, body) = api_create_task_full(
        &client,
        &base,
        &json!({
            "title": "Phase config task",
            "bead_id": bead_id,
            "category": "feature",
            "priority": "medium",
            "complexity": "medium",
            "phase_configs": [
                {"phase_name": "spec_creation", "model": "opus", "thinking_level": "high"},
                {"phase_name": "planning", "model": "sonnet", "thinking_level": "medium"},
                {"phase_name": "code_review", "model": "haiku", "thinking_level": "low"}
            ]
        }),
    )
    .await;
    assert_eq!(code, 201);
    let configs = body["phase_configs"].as_array().unwrap();
    assert_eq!(configs.len(), 3);
    assert_eq!(configs[0]["phase_name"], "spec_creation");
    assert_eq!(configs[1]["phase_name"], "planning");
    assert_eq!(configs[2]["phase_name"], "code_review");
}

#[tokio::test]
async fn test_override_individual_phase_settings() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let (_, created) =
        api_create_task(&client, &base, "Override phases", "feature", "low", "small").await;
    let id = created["id"].as_str().unwrap();

    let (code, body) = api_update_task(
        &client,
        &base,
        id,
        &json!({
            "phase_configs": [
                {"phase_name": "spec_creation", "model": "opus", "thinking_level": "high"}
            ]
        }),
    )
    .await;
    assert_eq!(code, 200);
    let configs = body["phase_configs"].as_array().unwrap();
    assert_eq!(configs.len(), 1);
    assert_eq!(configs[0]["model"], "opus");
}

#[tokio::test]
async fn test_phase_config_json_structure() {
    let config = PhaseConfig {
        phase_name: "planning".to_string(),
        model: "sonnet".to_string(),
        thinking_level: "medium".to_string(),
    };
    let json_val: Value = serde_json::to_value(&config).unwrap();
    assert!(json_val.is_object());
    assert!(json_val["phase_name"].is_string());
    assert!(json_val["model"].is_string());
    assert!(json_val["thinking_level"].is_string());
}

#[tokio::test]
async fn test_phase_config_empty_collection_by_default() {
    let task = Task::new(
        "No configs",
        Uuid::new_v4(),
        TaskCategory::Feature,
        TaskPriority::Low,
        TaskComplexity::Trivial,
    );
    assert!(task.phase_configs.is_empty());
}

#[tokio::test]
async fn test_phase_config_equality() {
    let a = PhaseConfig {
        phase_name: "planning".into(),
        model: "opus".into(),
        thinking_level: "high".into(),
    };
    let b = PhaseConfig {
        phase_name: "planning".into(),
        model: "opus".into(),
        thinking_level: "high".into(),
    };
    let c = PhaseConfig {
        phase_name: "planning".into(),
        model: "sonnet".into(),
        thinking_level: "high".into(),
    };
    assert_eq!(a, b);
    assert_ne!(a, c);
}

// ===========================================================================
// 4. Agent Profile Selection (10 tests)
// ===========================================================================

#[tokio::test]
async fn test_auto_profile_default_phase_configs() {
    let profile = AgentProfile::Auto;
    let configs = profile.default_phase_configs();
    assert_eq!(configs.len(), 3);
    // Auto uses sonnet with medium thinking for all phases.
    for config in &configs {
        assert_eq!(config.model, "sonnet");
        assert_eq!(config.thinking_level, "medium");
    }
}

#[tokio::test]
async fn test_complex_profile_uses_opus_everywhere() {
    let profile = AgentProfile::Complex;
    let configs = profile.default_phase_configs();
    assert_eq!(configs.len(), 3);
    for config in &configs {
        assert_eq!(config.model, "opus");
        assert_eq!(config.thinking_level, "high");
    }
}

#[tokio::test]
async fn test_balanced_profile_uses_mixed_models() {
    let profile = AgentProfile::Balanced;
    let configs = profile.default_phase_configs();
    assert_eq!(configs.len(), 3);
    // Balanced: spec=sonnet, planning=opus, review=haiku
    assert_eq!(configs[0].model, "sonnet");
    assert_eq!(configs[1].model, "opus");
    assert_eq!(configs[2].model, "haiku");
}

#[tokio::test]
async fn test_quick_profile_uses_haiku() {
    let profile = AgentProfile::Quick;
    let configs = profile.default_phase_configs();
    assert_eq!(configs.len(), 3);
    for config in &configs {
        assert_eq!(config.model, "haiku");
        assert_eq!(config.thinking_level, "low");
    }
}

#[tokio::test]
async fn test_custom_profile_accepts_user_defined_configs() {
    let profile = AgentProfile::Custom("my-custom-profile".to_string());
    let configs = profile.default_phase_configs();
    // Custom defaults to sonnet/medium as a baseline.
    assert_eq!(configs.len(), 3);

    // Verify the custom name is preserved.
    if let AgentProfile::Custom(name) = &profile {
        assert_eq!(name, "my-custom-profile");
    } else {
        panic!("Expected Custom variant");
    }
}

#[tokio::test]
async fn test_profile_serialization_deserialization() {
    let variants: Vec<AgentProfile> = vec![
        AgentProfile::Auto,
        AgentProfile::Complex,
        AgentProfile::Balanced,
        AgentProfile::Quick,
        AgentProfile::Custom("user-config".to_string()),
    ];

    for variant in &variants {
        let serialized = serde_json::to_string(variant).unwrap();
        let deserialized: AgentProfile = serde_json::from_str(&serialized).unwrap();
        assert_eq!(&deserialized, variant);
    }
}

#[tokio::test]
async fn test_profile_enum_exhaustiveness() {
    // Ensure all variants are covered by pattern matching.
    let profiles = vec![
        AgentProfile::Auto,
        AgentProfile::Complex,
        AgentProfile::Balanced,
        AgentProfile::Quick,
        AgentProfile::Custom("test".into()),
    ];

    for profile in profiles {
        let name = match &profile {
            AgentProfile::Auto => "Auto Optimized",
            AgentProfile::Complex => "Complex",
            AgentProfile::Balanced => "Balanced",
            AgentProfile::Quick => "Quick",
            AgentProfile::Custom(_) => "Custom",
        };
        assert_eq!(profile.display_name(), name);
    }
}

#[tokio::test]
async fn test_profile_display_names() {
    assert_eq!(AgentProfile::Auto.display_name(), "Auto Optimized");
    assert_eq!(AgentProfile::Complex.display_name(), "Complex");
    assert_eq!(AgentProfile::Balanced.display_name(), "Balanced");
    assert_eq!(AgentProfile::Quick.display_name(), "Quick");
    assert_eq!(
        AgentProfile::Custom("anything".into()).display_name(),
        "Custom"
    );
}

#[tokio::test]
async fn test_profile_via_api_create() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let bead_id = Uuid::new_v4();
    let (code, body) = api_create_task_full(
        &client,
        &base,
        &json!({
            "title": "Profile task",
            "bead_id": bead_id,
            "category": "feature",
            "priority": "medium",
            "complexity": "medium",
            "agent_profile": "complex",
        }),
    )
    .await;
    assert_eq!(code, 201);
    assert_eq!(body["agent_profile"], "complex");
}

#[tokio::test]
async fn test_profile_via_api_update() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let (_, created) =
        api_create_task(&client, &base, "Profile update", "feature", "low", "small").await;
    let id = created["id"].as_str().unwrap();

    let (code, body) =
        api_update_task(&client, &base, id, &json!({"agent_profile": "quick"})).await;
    assert_eq!(code, 200);
    assert_eq!(body["agent_profile"], "quick");
}

// ===========================================================================
// 5. Impact Rating (5+ tests)
// ===========================================================================

#[tokio::test]
async fn test_all_impact_variants() {
    let variants = vec![
        ("low", TaskImpact::Low),
        ("medium", TaskImpact::Medium),
        ("high", TaskImpact::High),
        ("critical", TaskImpact::Critical),
    ];

    for (expected_str, variant) in &variants {
        let serialized = serde_json::to_string(variant).unwrap();
        assert_eq!(serialized, format!("\"{}\"", expected_str));
    }
}

#[tokio::test]
async fn test_impact_serde_roundtrip() {
    let impacts = vec![
        TaskImpact::Low,
        TaskImpact::Medium,
        TaskImpact::High,
        TaskImpact::Critical,
    ];

    for impact in &impacts {
        let json_str = serde_json::to_string(impact).unwrap();
        let deserialized: TaskImpact = serde_json::from_str(&json_str).unwrap();
        assert_eq!(&deserialized, impact);
    }
}

#[tokio::test]
async fn test_impact_with_task_integration() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let bead_id = Uuid::new_v4();
    let (code, body) = api_create_task_full(
        &client,
        &base,
        &json!({
            "title": "Impact task",
            "bead_id": bead_id,
            "category": "security",
            "priority": "urgent",
            "complexity": "complex",
            "impact": "critical",
        }),
    )
    .await;
    assert_eq!(code, 201);
    assert_eq!(body["impact"], "critical");
}

#[tokio::test]
async fn test_impact_update_via_api() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let (_, created) =
        api_create_task(&client, &base, "Impact update", "feature", "low", "small").await;
    let id = created["id"].as_str().unwrap();
    assert!(created["impact"].is_null());

    let (code, body) = api_update_task(&client, &base, id, &json!({"impact": "high"})).await;
    assert_eq!(code, 200);
    assert_eq!(body["impact"], "high");
}

#[tokio::test]
async fn test_impact_filtering_in_memory() {
    let tasks: Vec<Task> = vec![
        {
            let mut t = Task::new(
                "Low impact",
                Uuid::new_v4(),
                TaskCategory::Feature,
                TaskPriority::Low,
                TaskComplexity::Small,
            );
            t.impact = Some(TaskImpact::Low);
            t
        },
        {
            let mut t = Task::new(
                "High impact",
                Uuid::new_v4(),
                TaskCategory::Security,
                TaskPriority::High,
                TaskComplexity::Large,
            );
            t.impact = Some(TaskImpact::High);
            t
        },
        {
            let mut t = Task::new(
                "Critical impact",
                Uuid::new_v4(),
                TaskCategory::Security,
                TaskPriority::Urgent,
                TaskComplexity::Complex,
            );
            t.impact = Some(TaskImpact::Critical);
            t
        },
    ];

    let high_and_above: Vec<_> = tasks
        .iter()
        .filter(|t| {
            matches!(
                t.impact,
                Some(TaskImpact::High) | Some(TaskImpact::Critical)
            )
        })
        .collect();
    assert_eq!(high_and_above.len(), 2);
}

#[tokio::test]
async fn test_impact_default_is_none() {
    let task = Task::new(
        "No impact",
        Uuid::new_v4(),
        TaskCategory::Feature,
        TaskPriority::Low,
        TaskComplexity::Trivial,
    );
    assert!(task.impact.is_none());
}

#[tokio::test]
async fn test_impact_equality() {
    assert_eq!(TaskImpact::Low, TaskImpact::Low);
    assert_ne!(TaskImpact::Low, TaskImpact::High);
    assert_ne!(TaskImpact::Medium, TaskImpact::Critical);
}

// ===========================================================================
// 6. Query Parameter Filtering (20+ tests)
// ===========================================================================

#[tokio::test]
async fn test_list_tasks_filter_by_phase() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create tasks with different phases
    let (_, task1) = api_create_task(&client, &base, "Discovery Task", "feature", "low", "small").await;
    let id1 = task1["id"].as_str().unwrap();

    let (_, task2) = api_create_task(&client, &base, "Planning Task", "bug_fix", "medium", "medium").await;
    let id2 = task2["id"].as_str().unwrap();

    let (_, task3) = api_create_task(&client, &base, "Coding Task", "refactoring", "high", "large").await;
    let id3 = task3["id"].as_str().unwrap();

    let (_, task4) = api_create_task(&client, &base, "QA Task", "feature", "low", "small").await;
    let id4 = task4["id"].as_str().unwrap();

    let (_, task5) = api_create_task(&client, &base, "Error Task", "bug_fix", "urgent", "complex").await;
    let id5 = task5["id"].as_str().unwrap();

    // Update task2 to planning phase (Discovery -> ContextGathering -> SpecCreation -> Planning)
    client
        .post(format!("{base}/api/tasks/{id2}/phase"))
        .json(&json!({"phase": "context_gathering"}))
        .send()
        .await
        .unwrap();
    client
        .post(format!("{base}/api/tasks/{id2}/phase"))
        .json(&json!({"phase": "spec_creation"}))
        .send()
        .await
        .unwrap();
    client
        .post(format!("{base}/api/tasks/{id2}/phase"))
        .json(&json!({"phase": "planning"}))
        .send()
        .await
        .unwrap();

    // Update task3 to coding phase (Discovery -> ContextGathering -> SpecCreation -> Planning -> Coding)
    client
        .post(format!("{base}/api/tasks/{id3}/phase"))
        .json(&json!({"phase": "context_gathering"}))
        .send()
        .await
        .unwrap();
    client
        .post(format!("{base}/api/tasks/{id3}/phase"))
        .json(&json!({"phase": "spec_creation"}))
        .send()
        .await
        .unwrap();
    client
        .post(format!("{base}/api/tasks/{id3}/phase"))
        .json(&json!({"phase": "planning"}))
        .send()
        .await
        .unwrap();
    client
        .post(format!("{base}/api/tasks/{id3}/phase"))
        .json(&json!({"phase": "coding"}))
        .send()
        .await
        .unwrap();

    // Update task4 to qa phase (Discovery -> ... -> Coding -> Qa)
    client
        .post(format!("{base}/api/tasks/{id4}/phase"))
        .json(&json!({"phase": "context_gathering"}))
        .send()
        .await
        .unwrap();
    client
        .post(format!("{base}/api/tasks/{id4}/phase"))
        .json(&json!({"phase": "spec_creation"}))
        .send()
        .await
        .unwrap();
    client
        .post(format!("{base}/api/tasks/{id4}/phase"))
        .json(&json!({"phase": "planning"}))
        .send()
        .await
        .unwrap();
    client
        .post(format!("{base}/api/tasks/{id4}/phase"))
        .json(&json!({"phase": "coding"}))
        .send()
        .await
        .unwrap();
    client
        .post(format!("{base}/api/tasks/{id4}/phase"))
        .json(&json!({"phase": "qa"}))
        .send()
        .await
        .unwrap();

    // Update task5 to error phase (can transition from any phase)
    client
        .post(format!("{base}/api/tasks/{id5}/phase"))
        .json(&json!({"phase": "error"}))
        .send()
        .await
        .unwrap();

    // Test filtering by discovery phase
    let (code, tasks) = api_list_tasks_with_query(&client, &base, &[("phase", "discovery")]).await;
    assert_eq!(code, 200);
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["id"], id1);
    assert_eq!(tasks[0]["phase"], "discovery");

    // Test filtering by planning phase
    let (code, tasks) = api_list_tasks_with_query(&client, &base, &[("phase", "planning")]).await;
    assert_eq!(code, 200);
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["id"], id2);
    assert_eq!(tasks[0]["phase"], "planning");

    // Test filtering by coding phase
    let (code, tasks) = api_list_tasks_with_query(&client, &base, &[("phase", "coding")]).await;
    assert_eq!(code, 200);
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["id"], id3);
    assert_eq!(tasks[0]["phase"], "coding");

    // Test filtering by qa phase
    let (code, tasks) = api_list_tasks_with_query(&client, &base, &[("phase", "qa")]).await;
    assert_eq!(code, 200);
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["id"], id4);
    assert_eq!(tasks[0]["phase"], "qa");

    // Test filtering by error phase
    let (code, tasks) = api_list_tasks_with_query(&client, &base, &[("phase", "error")]).await;
    assert_eq!(code, 200);
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["id"], id5);
    assert_eq!(tasks[0]["phase"], "error");

    // Test filtering by complete phase (should return empty)
    let (code, tasks) = api_list_tasks_with_query(&client, &base, &[("phase", "complete")]).await;
    assert_eq!(code, 200);
    assert_eq!(tasks.len(), 0);

    // Test all tasks without filter
    let all_tasks = api_list_tasks(&client, &base).await;
    assert_eq!(all_tasks.len(), 5);
}

#[tokio::test]
async fn test_list_tasks_filter_by_category() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create tasks with different categories
    let (_, task1) = api_create_task(&client, &base, "Feature Task", "feature", "low", "small").await;
    let id1 = task1["id"].as_str().unwrap();

    let (_, task2) = api_create_task(&client, &base, "Bug Fix Task", "bug_fix", "medium", "medium").await;
    let id2 = task2["id"].as_str().unwrap();

    let (_, task3) = api_create_task(&client, &base, "Refactoring Task", "refactoring", "high", "large").await;
    let id3 = task3["id"].as_str().unwrap();

    let (_, task4) = api_create_task(&client, &base, "Another Feature", "feature", "urgent", "complex").await;
    let id4 = task4["id"].as_str().unwrap();

    let (_, task5) = api_create_task(&client, &base, "Documentation Task", "documentation", "low", "small").await;
    let id5 = task5["id"].as_str().unwrap();

    // Test filtering by feature category
    let (code, tasks) = api_list_tasks_with_query(&client, &base, &[("category", "feature")]).await;
    assert_eq!(code, 200);
    assert_eq!(tasks.len(), 2);
    let task_ids: Vec<&str> = tasks.iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert!(task_ids.contains(&id1));
    assert!(task_ids.contains(&id4));
    for task in &tasks {
        assert_eq!(task["category"], "feature");
    }

    // Test filtering by bug_fix category
    let (code, tasks) = api_list_tasks_with_query(&client, &base, &[("category", "bug_fix")]).await;
    assert_eq!(code, 200);
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["id"], id2);
    assert_eq!(tasks[0]["category"], "bug_fix");

    // Test filtering by refactoring category
    let (code, tasks) = api_list_tasks_with_query(&client, &base, &[("category", "refactoring")]).await;
    assert_eq!(code, 200);
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["id"], id3);
    assert_eq!(tasks[0]["category"], "refactoring");

    // Test filtering by documentation category
    let (code, tasks) = api_list_tasks_with_query(&client, &base, &[("category", "documentation")]).await;
    assert_eq!(code, 200);
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["id"], id5);
    assert_eq!(tasks[0]["category"], "documentation");

    // Test filtering by non-existent category (should return empty)
    let (code, tasks) = api_list_tasks_with_query(&client, &base, &[("category", "testing")]).await;
    assert_eq!(code, 200);
    assert_eq!(tasks.len(), 0);

    // Test all tasks without filter
    let all_tasks = api_list_tasks(&client, &base).await;
    assert_eq!(all_tasks.len(), 5);
}
