//! Full daemon integration tests: startup, shutdown, API server binding,
//! endpoint responses, CORS headers, settings cycle, task CRUD, and WebSocket events.

use std::sync::Arc;
use std::time::Duration;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, api_router_with_auth, ApiState};
use at_bridge::protocol::BridgeMessage;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a WebSocket upgrade request with Origin header (required after CORS enforcement).
fn ws_request(url: &str) -> tokio_tungstenite::tungstenite::http::Request<()> {
    tokio_tungstenite::tungstenite::http::Request::builder()
        .uri(url)
        .header("Host", "localhost")
        .header("Origin", "http://localhost")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tokio_tungstenite::tungstenite::handshake::client::generate_key(),
        )
        .body(())
        .unwrap()
}

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

/// Start a test server with API-key auth enabled.
async fn start_authed_server(api_key: &str) -> (String, Arc<ApiState>) {
    let event_bus = EventBus::new();
    let state = Arc::new(ApiState::new(event_bus).with_relaxed_rate_limits());
    let allowed_origins = vec![
        "http://localhost".to_string(),
        "http://127.0.0.1".to_string(),
    ];
    let router = api_router_with_auth(state.clone(), Some(api_key.to_string()), allowed_origins);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind to ephemeral port");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    (format!("http://{addr}"), state)
}

fn task_payload() -> Value {
    json!({
        "title": "Integration test task",
        "bead_id": uuid::Uuid::new_v4(),
        "category": "feature",
        "priority": "high",
        "complexity": "medium"
    })
}

// ===========================================================================
// Daemon startup / shutdown
// ===========================================================================

#[tokio::test]
async fn test_daemon_startup_and_api_binding() {
    let (base, _state) = start_test_server().await;

    // Server should respond to a simple GET.
    let resp = reqwest::get(format!("{base}/api/status")).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body["version"].is_string());
}

#[tokio::test]
async fn test_daemon_api_state_defaults() {
    let event_bus = EventBus::new();
    let state = ApiState::new(event_bus);

    // KPI defaults should be zeroed out.
    let kpi = state.kpi.read().await;
    assert_eq!(kpi.total_beads, 0);
    assert_eq!(kpi.active_agents, 0);

    // Collections should be empty.
    assert!(state.beads.read().await.is_empty());
    assert!(state.agents.read().await.is_empty());
    assert!(state.tasks.read().await.is_empty());
}

#[tokio::test]
async fn test_daemon_with_cache_creates_cleanly() {
    let cache = Arc::new(at_core::cache::CacheDb::new_in_memory().await.unwrap());
    let config = at_core::config::Config::default();
    let daemon = at_daemon::daemon::Daemon::with_cache(config, cache);

    // Should be able to get handles without panicking.
    let _handle = daemon.shutdown_handle();
    let api_state = daemon.api_state();
    assert!(api_state.kpi.try_read().is_ok());
}

#[tokio::test]
async fn test_daemon_shutdown_via_handle() {
    let cache = Arc::new(at_core::cache::CacheDb::new_in_memory().await.unwrap());
    let config = at_core::config::Config::default();
    let daemon = at_daemon::daemon::Daemon::with_cache(config, cache);

    let handle = daemon.shutdown_handle();
    handle.trigger();
    // If we get here without blocking, shutdown signaling works.
    assert!(handle.is_shutting_down());
}

#[tokio::test]
async fn test_daemon_startup_with_auto_generated_key() {
    let cache = Arc::new(at_core::cache::CacheDb::new_in_memory().await.unwrap());
    let config = at_core::config::Config::default();
    let daemon = at_daemon::daemon::Daemon::with_cache(config, cache);

    // Start daemon in embedded mode, which auto-generates an API key.
    let port = daemon.start_embedded().await.unwrap();
    assert!(port > 0, "daemon should bind to a valid port");

    // Verify the API server is running by hitting the status endpoint.
    // The server has auth enabled with the auto-generated key, but we can
    // still verify basic connectivity.
    let base_url = format!("http://127.0.0.1:{port}");

    // Without the key, we should get 401 (auth required).
    let resp = reqwest::get(format!("{base_url}/api/status"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 401, "should require authentication");

    // Clean shutdown.
    daemon.shutdown();
}

// ===========================================================================
// KPI endpoint
// ===========================================================================

#[tokio::test]
async fn test_kpi_endpoint_returns_valid_json() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/kpi")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert!(body["total_beads"].is_number());
    assert!(body["backlog"].is_number());
    assert!(body["hooked"].is_number());
    assert!(body["slung"].is_number());
    assert!(body["review"].is_number());
    assert!(body["done"].is_number());
    assert!(body["failed"].is_number());
    assert!(body["active_agents"].is_number());
    assert!(body["timestamp"].is_string());
}

#[tokio::test]
async fn test_kpi_updates_when_beads_added() {
    use at_core::types::{Bead, BeadStatus, Lane};

    let (base, state) = start_test_server().await;

    // Insert beads directly so the KPI handler computes correct totals.
    {
        let mut beads = state.beads.write().await;
        for i in 0..3 {
            let mut b = Bead::new(format!("backlog-{i}"), Lane::Standard);
            b.status = BeadStatus::Backlog;
            beads.insert(b.id, b);
        }
        for i in 0..2 {
            let mut b = Bead::new(format!("done-{i}"), Lane::Standard);
            b.status = BeadStatus::Done;
            beads.insert(b.id, b);
        }
    }

    let resp = reqwest::get(format!("{base}/api/kpi")).await.unwrap();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["total_beads"], 5);
    assert_eq!(body["backlog"], 3);
    assert_eq!(body["done"], 2);
}

// ===========================================================================
// Settings GET / PUT cycle
// ===========================================================================

#[tokio::test]
async fn test_settings_get_returns_defaults() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/settings")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    // Config has at minimum an "agents" and "cache" section.
    assert!(body["agents"].is_object());
    assert!(body["cache"].is_object());
}

#[tokio::test]
async fn test_settings_patch_cycle() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // GET initial settings.
    let resp = client
        .get(format!("{base}/api/settings"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let initial: Value = resp.json().await.unwrap();
    assert!(initial["agents"].is_object());

    // PATCH a subset of settings.
    let patch = json!({
        "agents": {
            "max_concurrent": 99
        }
    });
    let resp = client
        .patch(format!("{base}/api/settings"))
        .json(&patch)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let patched: Value = resp.json().await.unwrap();
    assert_eq!(patched["agents"]["max_concurrent"], 99);
}

// ===========================================================================
// Task CRUD lifecycle through API
// ===========================================================================

#[tokio::test]
async fn test_task_crud_lifecycle() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // CREATE
    let resp = client
        .post(format!("{base}/api/tasks"))
        .json(&task_payload())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let created: Value = resp.json().await.unwrap();
    let task_id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["title"], "Integration test task");
    assert_eq!(created["phase"], "discovery");

    // READ
    let resp = reqwest::get(format!("{base}/api/tasks/{task_id}"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let read: Value = resp.json().await.unwrap();
    assert_eq!(read["title"], "Integration test task");

    // UPDATE
    let resp = client
        .put(format!("{base}/api/tasks/{task_id}"))
        .json(&json!({ "title": "Updated task title" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let updated: Value = resp.json().await.unwrap();
    assert_eq!(updated["title"], "Updated task title");

    // LIST
    let resp = reqwest::get(format!("{base}/api/tasks")).await.unwrap();
    let tasks: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["title"], "Updated task title");

    // DELETE
    let resp = client
        .delete(format!("{base}/api/tasks/{task_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Verify deletion
    let resp = reqwest::get(format!("{base}/api/tasks/{task_id}"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_task_phase_transitions_through_api() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/tasks"))
        .json(&task_payload())
        .send()
        .await
        .unwrap();
    let created: Value = resp.json().await.unwrap();
    let task_id = created["id"].as_str().unwrap();

    // Walk through valid transitions: discovery -> context_gathering -> spec_creation
    let phases = [
        "context_gathering",
        "spec_creation",
        "planning",
        "coding",
        "qa",
        "merging",
        "complete",
    ];
    for phase in &phases {
        let resp = client
            .post(format!("{base}/api/tasks/{task_id}/phase"))
            .json(&json!({ "phase": phase }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200, "transition to {phase} should succeed");
        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["phase"], *phase);
    }
}

#[tokio::test]
async fn test_task_create_rejects_empty_title() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{base}/api/tasks"))
        .json(&json!({
            "title": "",
            "bead_id": uuid::Uuid::new_v4(),
            "category": "feature",
            "priority": "medium",
            "complexity": "small"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

// ===========================================================================
// CORS headers
// ===========================================================================

#[tokio::test]
async fn test_cors_preflight() {
    let (base, _state) = start_authed_server("test-key").await;
    let client = reqwest::Client::new();

    // Test 1: Exact localhost origin (without port) should be allowed
    let resp = client
        .request(reqwest::Method::OPTIONS, format!("{base}/api/status"))
        .header("Origin", "http://localhost")
        .header("Access-Control-Request-Method", "GET")
        .send()
        .await
        .unwrap();

    assert!(
        resp.status().is_success(),
        "Localhost origin should be allowed for preflight, status: {}",
        resp.status()
    );
    let headers = resp.headers();
    assert!(
        headers.contains_key("access-control-allow-origin"),
        "CORS allow-origin header should be present for localhost"
    );
    let allow_origin = headers
        .get("access-control-allow-origin")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(
        allow_origin, "http://localhost",
        "Allow-origin should echo localhost origin exactly, got: {}",
        allow_origin
    );

    // Test 2: Exact 127.0.0.1 origin (without port) should be allowed
    let resp = client
        .request(reqwest::Method::OPTIONS, format!("{base}/api/status"))
        .header("Origin", "http://127.0.0.1")
        .header("Access-Control-Request-Method", "GET")
        .send()
        .await
        .unwrap();

    assert!(
        resp.status().is_success(),
        "127.0.0.1 origin should be allowed for preflight, status: {}",
        resp.status()
    );
    let headers = resp.headers();
    assert!(
        headers.contains_key("access-control-allow-origin"),
        "CORS allow-origin header should be present for 127.0.0.1"
    );
    let allow_origin = headers
        .get("access-control-allow-origin")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(
        allow_origin, "http://127.0.0.1",
        "Allow-origin should echo 127.0.0.1 origin exactly, got: {}",
        allow_origin
    );

    // Test 3: Localhost with non-standard port should be allowed (wildcard port matching)
    let resp = client
        .request(reqwest::Method::OPTIONS, format!("{base}/api/status"))
        .header("Origin", "http://localhost:3001")
        .header("Access-Control-Request-Method", "GET")
        .send()
        .await
        .unwrap();

    assert!(
        resp.status().is_success(),
        "Localhost with port should be allowed for preflight (wildcard port matching), status: {}",
        resp.status()
    );
    let headers = resp.headers();
    assert!(
        headers.contains_key("access-control-allow-origin"),
        "CORS allow-origin header should be present for localhost:3001"
    );
    let allow_origin = headers
        .get("access-control-allow-origin")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(
        allow_origin, "http://localhost:3001",
        "Allow-origin should echo localhost:3001 origin exactly (wildcard port matching), got: {}",
        allow_origin
    );

    // Test 4: Non-localhost origin should be rejected (no CORS headers for disallowed origin)
    let resp = client
        .request(reqwest::Method::OPTIONS, format!("{base}/api/status"))
        .header("Origin", "http://evil.com")
        .header("Access-Control-Request-Method", "GET")
        .send()
        .await
        .unwrap();

    // The preflight may succeed (200) but should not echo the disallowed origin
    let headers = resp.headers();
    if let Some(allow_origin) = headers.get("access-control-allow-origin") {
        let origin_str = allow_origin.to_str().unwrap();
        assert!(
            !origin_str.contains("evil.com"),
            "Disallowed origin should not be echoed in allow-origin header, got: {}",
            origin_str
        );
    }
}

#[tokio::test]
async fn test_cors_wildcard_port_matching() {
    let (base, _state) = start_authed_server("test-key").await;
    let client = reqwest::Client::new();

    // Test localhost with various ports - all should be allowed
    let test_origins = vec![
        "http://localhost:3000",
        "http://localhost:8080",
        "http://localhost:9000",
        "http://127.0.0.1:3000",
        "http://127.0.0.1:8080",
        "http://127.0.0.1:9000",
    ];

    for origin in test_origins {
        let resp = client
            .request(reqwest::Method::OPTIONS, format!("{base}/api/status"))
            .header("Origin", origin)
            .header("Access-Control-Request-Method", "GET")
            .send()
            .await
            .unwrap();

        assert!(
            resp.status().is_success(),
            "Localhost origin with port {} should be allowed for preflight, status: {}",
            origin,
            resp.status()
        );

        let headers = resp.headers();
        assert!(
            headers.contains_key("access-control-allow-origin"),
            "CORS allow-origin header should be present for {}",
            origin
        );

        let allow_origin = headers
            .get("access-control-allow-origin")
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(
            allow_origin, origin,
            "Allow-origin should echo the localhost origin {} exactly, got: {}",
            origin, allow_origin
        );
    }

    // Verify that non-localhost origins with ports are still rejected
    let disallowed_origins = vec!["http://evil.com:3000", "http://malicious.com:8080"];

    for origin in disallowed_origins {
        let resp = client
            .request(reqwest::Method::OPTIONS, format!("{base}/api/status"))
            .header("Origin", origin)
            .header("Access-Control-Request-Method", "GET")
            .send()
            .await
            .unwrap();

        let headers = resp.headers();
        if let Some(allow_origin) = headers.get("access-control-allow-origin") {
            let origin_str = allow_origin.to_str().unwrap();
            assert!(
                origin_str != origin,
                "Disallowed origin {} should not be echoed, got: {}",
                origin,
                origin_str
            );
        }
    }
}

#[tokio::test]
async fn test_cors_on_regular_request() {
    let (base, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{base}/api/status"))
        .header("Origin", "http://localhost:3001")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Check CORS header is present on the response.
    let headers = resp.headers();
    assert!(
        headers.contains_key("access-control-allow-origin"),
        "CORS allow-origin header should be present"
    );
}

// ===========================================================================
// WebSocket event connection
// ===========================================================================

#[tokio::test]
async fn test_websocket_connects_and_receives_event() {
    use futures_util::StreamExt;

    let (base, state) = start_test_server().await;
    let ws_url = base.replace("http://", "ws://") + "/ws";

    let (mut ws_stream, _) = tokio_tungstenite::connect_async(ws_request(&ws_url))
        .await
        .expect("failed to connect to websocket");

    // Give time for subscription to register.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Publish a test event.
    state
        .event_bus
        .publish(BridgeMessage::Event(at_bridge::protocol::EventPayload {
            event_type: "integration_test".to_string(),
            agent_id: None,
            bead_id: None,
            message: "hello from integration test".to_string(),
            timestamp: chrono::Utc::now(),
        }));

    // Receive the event.
    let msg = tokio::time::timeout(Duration::from_secs(3), ws_stream.next())
        .await
        .expect("timed out waiting for ws message")
        .expect("stream ended")
        .expect("ws error");

    let text = msg.into_text().expect("expected text message");
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["type"], "event");
    assert!(parsed["payload"]["message"]
        .as_str()
        .unwrap()
        .contains("integration test"));
}

#[tokio::test]
async fn test_events_ws_endpoint_connects() {
    use futures_util::StreamExt;

    let (base, state) = start_test_server().await;
    let ws_url = base.replace("http://", "ws://") + "/api/events/ws";

    let (mut ws_stream, _) = tokio_tungstenite::connect_async(ws_request(&ws_url))
        .await
        .expect("failed to connect to /api/events/ws");

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Publish an event and verify it arrives.
    state.event_bus.publish(BridgeMessage::GetStatus);

    // Drain messages until we find the get_status event (skip pings, heartbeats, etc.)
    let mut found = false;
    for _ in 0..5 {
        let msg = tokio::time::timeout(Duration::from_secs(3), ws_stream.next())
            .await
            .expect("timed out")
            .expect("stream ended")
            .expect("ws error");

        let text = msg.into_text().expect("expected text");
        if let Ok(parsed) = serde_json::from_str::<Value>(&text) {
            if parsed["type"] == "get_status" {
                found = true;
                break;
            }
        }
    }
    assert!(
        found,
        "expected to receive a get_status event over WebSocket"
    );
}

// ===========================================================================
// Authentication
// ===========================================================================

#[tokio::test]
async fn test_auth_rejects_without_key() {
    let (base, _state) = start_authed_server("test-secret-key").await;

    let resp = reqwest::get(format!("{base}/api/status")).await.unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_auth_accepts_valid_key() {
    let (base, _state) = start_authed_server("test-secret-key").await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{base}/api/status"))
        .header("X-API-Key", "test-secret-key")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_auth_accepts_bearer_token() {
    let (base, _state) = start_authed_server("test-secret-key").await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{base}/api/status"))
        .header("Authorization", "Bearer test-secret-key")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

// ===========================================================================
// Miscellaneous endpoints
// ===========================================================================

#[tokio::test]
async fn test_metrics_prometheus_endpoint() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/metrics")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let content_type = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        content_type.contains("text/plain"),
        "metrics should return text/plain"
    );
}

#[tokio::test]
async fn test_metrics_json_endpoint() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/metrics/json"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert!(body.is_object());
}

#[tokio::test]
async fn test_costs_endpoint() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/costs")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["input_tokens"], 0);
    assert_eq!(body["output_tokens"], 0);
}

#[tokio::test]
async fn test_convoys_endpoint_empty() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/convoys")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = resp.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn test_mcp_servers_endpoint() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/mcp/servers"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = resp.json().await.unwrap();
    assert!(!body.is_empty());
    // Each server should have name, status, tools.
    for server in &body {
        assert!(server["name"].is_string());
        assert!(server["status"].is_string());
        assert!(server["tools"].is_array());
    }
}

#[tokio::test]
async fn test_credentials_status_endpoint() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/credentials/status"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert!(body["providers"].is_array());
    assert!(body["daemon_auth"].is_boolean());
}

#[tokio::test]
async fn test_sessions_ui_endpoint() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/sessions/ui"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_notification_endpoints_via_http() {
    let (base, _state) = start_test_server().await;

    // Count should return zeroes.
    let resp = reqwest::get(format!("{base}/api/notifications/count"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["unread"], 0);
    assert_eq!(body["total"], 0);

    // List should return empty array.
    let resp = reqwest::get(format!("{base}/api/notifications"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Vec<Value> = resp.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn test_agent_sessions_endpoint_empty() {
    let (base, _state) = start_test_server().await;

    let resp = reqwest::get(format!("{base}/api/sessions")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body: Vec<Value> = resp.json().await.unwrap();
    assert!(body.is_empty());
}
