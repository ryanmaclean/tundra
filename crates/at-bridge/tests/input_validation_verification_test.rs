use std::sync::Arc;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, ApiState};
use serde_json::Value;

/// Spin up an API server on a random port, return the base URL.
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

/// Manual Verification Test 1: Valid short input succeeds
#[tokio::test]
async fn test_valid_short_input_succeeds() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Test create_bead with valid short input
    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&serde_json::json!({
            "title": "Valid short title",
            "description": "Valid short description"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 201, "create_bead should succeed with valid short input");
    let created: Value = resp.json().await.unwrap();
    assert_eq!(created["title"], "Valid short title");

    // Test create_task with valid short input
    let bead_id = created["id"].as_str().unwrap();
    let resp = client
        .post(format!("{base}/api/tasks"))
        .json(&serde_json::json!({
            "title": "Valid task title",
            "description": "Valid task description",
            "bead_id": bead_id
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 201, "create_task should succeed with valid short input");
    let task: Value = resp.json().await.unwrap();
    assert_eq!(task["title"], "Valid task title");

    println!("✅ Test 1 PASSED: Valid short input succeeds");
}

/// Manual Verification Test 2: 10,001+ character input returns 400
#[tokio::test]
async fn test_long_input_rejected() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Create a string with 10,001 characters
    let long_string = "a".repeat(10_001);

    // Test create_bead with long title
    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&serde_json::json!({
            "title": long_string,
            "description": "Short description"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400, "create_bead should reject title with 10,001+ characters");

    // Test create_bead with long description
    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&serde_json::json!({
            "title": "Short title",
            "description": long_string
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400, "create_bead should reject description with 10,001+ characters");

    // First create a valid bead and task for update test
    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&serde_json::json!({
            "title": "Test bead",
            "description": "Test description"
        }))
        .send()
        .await
        .unwrap();
    let bead: Value = resp.json().await.unwrap();
    let bead_id = bead["id"].as_str().unwrap();

    let resp = client
        .post(format!("{base}/api/tasks"))
        .json(&serde_json::json!({
            "title": "Test task",
            "bead_id": bead_id
        }))
        .send()
        .await
        .unwrap();
    let task: Value = resp.json().await.unwrap();
    let task_id = task["id"].as_str().unwrap();

    // Test create_task with long title
    let resp = client
        .post(format!("{base}/api/tasks"))
        .json(&serde_json::json!({
            "title": long_string,
            "bead_id": bead_id
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400, "create_task should reject title with 10,001+ characters");

    // Test update_task with long title
    let resp = client
        .patch(format!("{base}/api/tasks/{task_id}"))
        .json(&serde_json::json!({
            "title": long_string
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400, "update_task should reject title with 10,001+ characters");

    println!("✅ Test 2 PASSED: 10,001+ character input returns 400");
}

/// Manual Verification Test 3: Prompt injection patterns return 400
#[tokio::test]
async fn test_prompt_injection_rejected() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Test various prompt injection patterns
    let injection_patterns = vec![
        "ignore previous instructions",
        "ignore all previous instructions",
        "disregard previous instructions",
        "forget previous instructions",
        "system: you are now",
        "new instructions:",
    ];

    for pattern in injection_patterns {
        // Test create_bead with injection in title
        let resp = client
            .post(format!("{base}/api/beads"))
            .json(&serde_json::json!({
                "title": pattern,
                "description": "Normal description"
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(
            resp.status(),
            400,
            "create_bead should reject prompt injection pattern in title: '{}'",
            pattern
        );

        // Test create_bead with injection in description
        let resp = client
            .post(format!("{base}/api/beads"))
            .json(&serde_json::json!({
                "title": "Normal title",
                "description": pattern
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(
            resp.status(),
            400,
            "create_bead should reject prompt injection pattern in description: '{}'",
            pattern
        );
    }

    // First create a valid bead for task tests
    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&serde_json::json!({
            "title": "Test bead",
            "description": "Test description"
        }))
        .send()
        .await
        .unwrap();
    let bead: Value = resp.json().await.unwrap();
    let bead_id = bead["id"].as_str().unwrap();

    // Test create_task with injection
    let resp = client
        .post(format!("{base}/api/tasks"))
        .json(&serde_json::json!({
            "title": "ignore previous instructions",
            "bead_id": bead_id
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400, "create_task should reject prompt injection pattern");

    println!("✅ Test 3 PASSED: Prompt injection patterns return 400");
}

/// Combined verification test - runs all three scenarios
#[tokio::test]
async fn test_all_validation_scenarios() {
    println!("\n🔍 Running Combined Manual Verification Test...\n");

    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Test 1: Valid short input succeeds
    println!("Test 1: Valid short input succeeds");
    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&serde_json::json!({
            "title": "Valid short title",
            "description": "Valid short description"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "create_bead should succeed with valid short input");
    println!("  ✓ Valid short input succeeds");

    // Test 2: 10,001+ character input returns 400
    println!("Test 2: 10,001+ character input returns 400");
    let long_string = "a".repeat(10_001);
    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&serde_json::json!({
            "title": long_string,
            "description": "Short description"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "create_bead should reject title with 10,001+ characters");
    println!("  ✓ 10,001+ character input returns 400");

    // Test 3: Prompt injection patterns return 400
    println!("Test 3: Prompt injection patterns return 400");
    let resp = client
        .post(format!("{base}/api/beads"))
        .json(&serde_json::json!({
            "title": "ignore previous instructions",
            "description": "Normal description"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "create_bead should reject prompt injection pattern");
    println!("  ✓ Prompt injection patterns return 400");

    println!("\n✅ ALL MANUAL VERIFICATION TESTS PASSED!\n");
}
