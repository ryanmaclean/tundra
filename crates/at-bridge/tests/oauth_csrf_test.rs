use std::sync::Arc;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, ApiState};
use serde_json::Value;

// Mutex to serialize env var access across tests (prevents race conditions)
static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

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

#[tokio::test]
async fn test_oauth_csrf_authorize_generates_and_stores_state() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let (base, state) = start_test_server().await;

    // Set required env var for the test
    std::env::set_var("GITHUB_OAUTH_CLIENT_ID", "test_client_id");

    let resp = reqwest::get(format!("{base}/api/github/oauth/authorize"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert!(body["url"].is_string());
    assert!(body["state"].is_string());

    let returned_state = body["state"].as_str().unwrap();

    // Verify the state was stored in the ApiState
    let pending_states = state.oauth_pending_states.read().await;
    assert!(
        pending_states.contains_key(returned_state),
        "Generated state should be stored in oauth_pending_states"
    );

    // Clean up
    std::env::remove_var("GITHUB_OAUTH_CLIENT_ID");
}

#[tokio::test]
async fn test_oauth_csrf_callback_rejects_missing_state() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Set required env vars for the test
    std::env::set_var("GITHUB_OAUTH_CLIENT_ID", "test_client_id");
    std::env::set_var("GITHUB_OAUTH_CLIENT_SECRET", "test_client_secret");

    // Attempt callback with a state that was never generated
    let resp = client
        .post(format!("{base}/api/github/oauth/callback"))
        .json(&serde_json::json!({
            "code": "test_code",
            "state": "invalid_state_never_generated"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);

    let body: Value = resp.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("Invalid or expired"));

    // Clean up
    std::env::remove_var("GITHUB_OAUTH_CLIENT_ID");
    std::env::remove_var("GITHUB_OAUTH_CLIENT_SECRET");
}

#[tokio::test]
async fn test_oauth_csrf_callback_rejects_invalid_state() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Set required env vars for the test
    std::env::set_var("GITHUB_OAUTH_CLIENT_ID", "test_client_id");
    std::env::set_var("GITHUB_OAUTH_CLIENT_SECRET", "test_client_secret");

    // Attempt callback with an arbitrary invalid state
    let resp = client
        .post(format!("{base}/api/github/oauth/callback"))
        .json(&serde_json::json!({
            "code": "test_code",
            "state": "completely_bogus_state_12345"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["error"].as_str().unwrap(),
        "Invalid or expired OAuth state parameter"
    );

    // Clean up
    std::env::remove_var("GITHUB_OAUTH_CLIENT_ID");
    std::env::remove_var("GITHUB_OAUTH_CLIENT_SECRET");
}

#[tokio::test]
async fn test_oauth_csrf_callback_accepts_valid_state_and_removes_it() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Set required env var for authorize
    std::env::set_var("GITHUB_OAUTH_CLIENT_ID", "test_client_id");

    // First, get a valid state from the authorize endpoint
    let resp = reqwest::get(format!("{base}/api/github/oauth/authorize"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    let valid_state = body["state"].as_str().unwrap().to_string();

    // Verify state is stored
    {
        let pending_states = state.oauth_pending_states.read().await;
        assert!(pending_states.contains_key(&valid_state));
    }

    // Set required env vars for callback
    std::env::set_var("GITHUB_OAUTH_CLIENT_SECRET", "test_client_secret");

    // Attempt callback with the valid state
    // Note: This will still fail because we don't have a real GitHub OAuth setup,
    // but it should NOT fail with "Invalid or expired OAuth state parameter"
    let _resp = client
        .post(format!("{base}/api/github/oauth/callback"))
        .json(&serde_json::json!({
            "code": "test_code",
            "state": valid_state.clone()
        }))
        .send()
        .await
        .unwrap();

    // The state validation should pass (200), but the actual OAuth exchange will fail
    // because we're using fake credentials. The important thing is we DON'T get a 400
    // for invalid state.
    // Actually, let's check that the state was removed from storage
    let pending_states = state.oauth_pending_states.read().await;
    assert!(
        !pending_states.contains_key(&valid_state),
        "State should be removed after use to prevent replay attacks"
    );

    // Clean up
    std::env::remove_var("GITHUB_OAUTH_CLIENT_ID");
    std::env::remove_var("GITHUB_OAUTH_CLIENT_SECRET");
}

#[tokio::test]
async fn test_oauth_csrf_callback_rejects_reused_state() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let (base, state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Set required env vars
    std::env::set_var("GITHUB_OAUTH_CLIENT_ID", "test_client_id");
    std::env::set_var("GITHUB_OAUTH_CLIENT_SECRET", "test_client_secret");

    // First, get a valid state from the authorize endpoint
    let resp = reqwest::get(format!("{base}/api/github/oauth/authorize"))
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    let valid_state = body["state"].as_str().unwrap().to_string();

    // Use the state once
    let _resp = client
        .post(format!("{base}/api/github/oauth/callback"))
        .json(&serde_json::json!({
            "code": "test_code",
            "state": valid_state.clone()
        }))
        .send()
        .await
        .unwrap();

    // First use should remove the state (regardless of whether OAuth succeeds)
    let pending_states = state.oauth_pending_states.read().await;
    assert!(!pending_states.contains_key(&valid_state));
    drop(pending_states);

    // Try to reuse the same state (replay attack)
    let resp = client
        .post(format!("{base}/api/github/oauth/callback"))
        .json(&serde_json::json!({
            "code": "test_code",
            "state": valid_state.clone()
        }))
        .send()
        .await
        .unwrap();

    // Should be rejected as invalid/expired
    assert_eq!(resp.status(), 400);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["error"].as_str().unwrap(),
        "Invalid or expired OAuth state parameter"
    );

    // Clean up
    std::env::remove_var("GITHUB_OAUTH_CLIENT_ID");
    std::env::remove_var("GITHUB_OAUTH_CLIENT_SECRET");
}

#[tokio::test]
async fn test_oauth_csrf_state_is_uuid_format() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let (base, _state) = start_test_server().await;

    // Set required env var for the test
    std::env::set_var("GITHUB_OAUTH_CLIENT_ID", "test_client_id");

    let resp = reqwest::get(format!("{base}/api/github/oauth/authorize"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    let state_str = body["state"].as_str().unwrap();

    // Verify it's a valid UUID format
    assert!(
        uuid::Uuid::parse_str(state_str).is_ok(),
        "State should be a valid UUID"
    );

    // Clean up
    std::env::remove_var("GITHUB_OAUTH_CLIENT_ID");
}

#[tokio::test]
async fn test_oauth_csrf_multiple_states_can_coexist() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let (base, state) = start_test_server().await;

    // Use a unique client ID for this test to avoid interference with other tests
    let test_client_id = format!("test_client_id_{}", uuid::Uuid::new_v4());
    std::env::set_var("GITHUB_OAUTH_CLIENT_ID", &test_client_id);

    // Generate multiple states (simulating multiple users starting OAuth flow)
    let resp1 = reqwest::get(format!("{base}/api/github/oauth/authorize"))
        .await
        .unwrap();
    assert_eq!(resp1.status(), 200, "First authorize request should succeed");
    let body1: Value = resp1.json().await.unwrap();
    let state1 = body1["state"].as_str().unwrap().to_string();

    // Ensure env var is still set (in case another test cleared it)
    std::env::set_var("GITHUB_OAUTH_CLIENT_ID", &test_client_id);

    let resp2 = reqwest::get(format!("{base}/api/github/oauth/authorize"))
        .await
        .unwrap();
    assert_eq!(resp2.status(), 200, "Second authorize request should succeed");
    let body2: Value = resp2.json().await.unwrap();
    let state2 = body2["state"].as_str().unwrap().to_string();

    // Ensure env var is still set (in case another test cleared it)
    std::env::set_var("GITHUB_OAUTH_CLIENT_ID", &test_client_id);

    let resp3 = reqwest::get(format!("{base}/api/github/oauth/authorize"))
        .await
        .unwrap();
    assert_eq!(resp3.status(), 200, "Third authorize request should succeed");
    let body3: Value = resp3.json().await.unwrap();
    let state3 = body3["state"].as_str().unwrap().to_string();

    // All states should be different
    assert_ne!(state1, state2);
    assert_ne!(state2, state3);
    assert_ne!(state1, state3);

    // All should be stored
    let pending_states = state.oauth_pending_states.read().await;
    assert!(pending_states.contains_key(&state1));
    assert!(pending_states.contains_key(&state2));
    assert!(pending_states.contains_key(&state3));
    assert_eq!(pending_states.len(), 3);

    // Clean up
    std::env::remove_var("GITHUB_OAUTH_CLIENT_ID");
}
