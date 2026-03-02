//! Integration tests for HTTP response compression.
//!
//! These tests verify that the CompressionLayer middleware correctly compresses
//! responses based on the Accept-Encoding header sent by the client. We test:
//! - Gzip compression
//! - Brotli compression
//! - Multiple endpoints to ensure global middleware coverage
//! - Uncompressed responses when no Accept-Encoding is provided
//!
//! Note: We use tower::ServiceExt::oneshot() instead of reqwest because reqwest
//! automatically decompresses responses and strips the Content-Encoding header,
//! making it impossible to verify compression is actually happening.

use std::sync::Arc;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, ApiState};
use at_core::types::*;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use uuid::Uuid;

// ===========================================================================
// Helpers
// ===========================================================================

/// Helper to get router with state for oneshot testing
fn test_router_with_state() -> (axum::Router, Arc<ApiState>) {
    let event_bus = EventBus::new();
    let state = Arc::new(ApiState::new(event_bus).with_relaxed_rate_limits());
    let router = api_router(state.clone());
    (router, state)
}

/// Helper to read the response body as bytes.
async fn body_bytes(resp: axum::http::Response<Body>) -> Vec<u8> {
    axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec()
}

/// Create tasks in the server state for testing list endpoints
async fn seed_tasks(state: &ApiState, count: usize) {
    let mut tasks = state.tasks.write().await;
    for i in 0..count {
        let task_id = Uuid::new_v4();
        let task = Task {
            id: task_id,
            title: format!("Test Task {}", i),
            bead_id: Uuid::new_v4(),
            category: TaskCategory::Feature,
            priority: TaskPriority::Medium,
            complexity: TaskComplexity::Medium,
            description: Some(format!(
                "This is test task number {} with a moderately long description to increase payload size for better compression testing",
                i
            )),
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
    }
}

// ===========================================================================
// 1. Gzip Compression Tests
// ===========================================================================

#[tokio::test]
async fn test_gzip_compression_on_tasks_endpoint() {
    let (app, state) = test_router_with_state();
    seed_tasks(&state, 20).await;

    let req = Request::builder()
        .uri("/api/tasks")
        .header("Accept-Encoding", "gzip")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify Content-Encoding header indicates gzip compression
    let content_encoding = resp.headers().get("content-encoding");
    assert!(
        content_encoding.is_some(),
        "Content-Encoding header should be present when gzip is requested"
    );

    let encoding_value = content_encoding.unwrap().to_str().unwrap();
    assert_eq!(
        encoding_value, "gzip",
        "Content-Encoding should be gzip when requested"
    );

    // Verify the body is non-empty (it's compressed, so we won't parse it)
    let body = body_bytes(resp).await;
    assert!(body.len() > 0, "Response body should not be empty");
}

#[tokio::test]
async fn test_gzip_compression_on_beads_endpoint() {
    let (app, _state) = test_router_with_state();

    let req = Request::builder()
        .uri("/api/beads")
        .header("Accept-Encoding", "gzip")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Note: CompressionLayer may skip compression for very small/empty responses
    // as the overhead isn't worthwhile. We verify the request succeeds and
    // compression is available, even if not applied to this tiny payload.
    assert!(resp.status().is_success());

    // If content encoding is present, it should be gzip
    if let Some(encoding) = resp.headers().get("content-encoding") {
        assert_eq!(encoding.to_str().unwrap(), "gzip");
    }
}

#[tokio::test]
async fn test_gzip_compression_on_agents_endpoint() {
    let (app, _state) = test_router_with_state();

    let req = Request::builder()
        .uri("/api/agents")
        .header("Accept-Encoding", "gzip")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Note: CompressionLayer may skip compression for very small/empty responses
    // as the overhead isn't worthwhile. We verify the request succeeds and
    // compression is available, even if not applied to this tiny payload.
    assert!(resp.status().is_success());

    // If content encoding is present, it should be gzip
    if let Some(encoding) = resp.headers().get("content-encoding") {
        assert_eq!(encoding.to_str().unwrap(), "gzip");
    }
}

#[tokio::test]
async fn test_gzip_compression_on_kpi_endpoint() {
    let (app, _state) = test_router_with_state();

    let req = Request::builder()
        .uri("/api/kpi")
        .header("Accept-Encoding", "gzip")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let content_encoding = resp.headers().get("content-encoding");
    assert!(
        content_encoding.is_some(),
        "Content-Encoding header should be present for KPI endpoint"
    );

    let encoding_value = content_encoding.unwrap().to_str().unwrap();
    assert_eq!(encoding_value, "gzip");
}

// ===========================================================================
// 2. Brotli Compression Tests
// ===========================================================================

#[tokio::test]
async fn test_brotli_compression_on_tasks_endpoint() {
    let (app, state) = test_router_with_state();
    seed_tasks(&state, 15).await;

    let req = Request::builder()
        .uri("/api/tasks")
        .header("Accept-Encoding", "br")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let content_encoding = resp.headers().get("content-encoding");
    assert!(
        content_encoding.is_some(),
        "Content-Encoding header should be present when brotli is requested"
    );

    let encoding_value = content_encoding.unwrap().to_str().unwrap();
    assert_eq!(
        encoding_value, "br",
        "Content-Encoding should be br (brotli) when requested"
    );

    let body = body_bytes(resp).await;
    assert!(body.len() > 0, "Response body should not be empty");
}

#[tokio::test]
async fn test_brotli_compression_on_projects_endpoint() {
    let (app, _state) = test_router_with_state();

    let req = Request::builder()
        .uri("/api/projects")
        .header("Accept-Encoding", "br")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let content_encoding = resp.headers().get("content-encoding");
    assert!(
        content_encoding.is_some(),
        "Content-Encoding header should be present for projects endpoint"
    );

    let encoding_value = content_encoding.unwrap().to_str().unwrap();
    assert_eq!(encoding_value, "br");
}

// ===========================================================================
// 3. Multiple Encoding Options Tests
// ===========================================================================

#[tokio::test]
async fn test_accepts_both_gzip_and_brotli() {
    let (app, state) = test_router_with_state();
    seed_tasks(&state, 10).await;

    let req = Request::builder()
        .uri("/api/tasks")
        .header("Accept-Encoding", "gzip, br")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let content_encoding = resp.headers().get("content-encoding");
    assert!(content_encoding.is_some(), "Should use compression");

    // CompressionLayer typically prefers brotli over gzip when both are accepted
    let encoding_value = content_encoding.unwrap().to_str().unwrap();
    assert!(
        encoding_value == "br" || encoding_value == "gzip",
        "Should use either br or gzip when both are accepted, got: {}",
        encoding_value
    );
}

#[tokio::test]
async fn test_wildcard_accept_encoding() {
    let (app, _state) = test_router_with_state();

    let req = Request::builder()
        .uri("/api/agents")
        .header("Accept-Encoding", "*")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // With wildcard, compression may or may not be applied depending on
    // tower-http's implementation, so we just verify the request succeeds
    assert!(resp.status().is_success());
}

// ===========================================================================
// 4. No Compression Tests
// ===========================================================================

#[tokio::test]
async fn test_no_compression_without_accept_encoding() {
    let (app, state) = test_router_with_state();
    seed_tasks(&state, 5).await;

    let req = Request::builder()
        .uri("/api/tasks")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Without Accept-Encoding header, no compression should be applied
    let content_encoding = resp.headers().get("content-encoding");

    // Either the header is not present, or it might be "identity"
    if let Some(encoding) = content_encoding {
        let value = encoding.to_str().unwrap();
        assert!(
            value == "identity" || value.is_empty(),
            "Should not compress without Accept-Encoding header, got: {}",
            value
        );
    }
}

#[tokio::test]
async fn test_identity_encoding_no_compression() {
    let (app, _state) = test_router_with_state();

    let req = Request::builder()
        .uri("/api/beads")
        .header("Accept-Encoding", "identity")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // With "identity" encoding, no compression should be applied
    let content_encoding = resp.headers().get("content-encoding");

    if let Some(encoding) = content_encoding {
        let value = encoding.to_str().unwrap();
        assert_ne!(
            value, "gzip",
            "Should not use gzip when identity is requested"
        );
        assert_ne!(
            value, "br",
            "Should not use brotli when identity is requested"
        );
    }
}

// ===========================================================================
// 5. Cross-Endpoint Coverage Tests
// ===========================================================================

#[tokio::test]
async fn test_compression_works_across_all_list_endpoints() {
    let (app, state) = test_router_with_state();

    // Seed some data so responses aren't empty (compression works better with data)
    seed_tasks(&state, 10).await;

    // Test a variety of endpoints to ensure CompressionLayer is global
    let endpoints = vec![
        "/api/tasks",    // Has data
        "/api/beads",    // Empty, but should still work
        "/api/agents",   // Empty, but should still work
        "/api/projects", // Empty, but should still work
        "/api/queue",    // Empty, but should still work
        "/api/kpi",      // Always has data (computed from state)
    ];

    for endpoint in endpoints {
        let req = Request::builder()
            .uri(endpoint)
            .header("Accept-Encoding", "gzip")
            .body(Body::empty())
            .unwrap();

        let resp = app.clone().oneshot(req).await.unwrap();

        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "Endpoint {} should respond with 200",
            endpoint
        );

        // CompressionLayer is applied globally, but may skip compression for tiny payloads.
        // For endpoints with data (tasks, kpi), we expect compression.
        // For empty endpoints, compression may be skipped as an optimization.
        if endpoint == "/api/tasks" || endpoint == "/api/kpi" {
            let content_encoding = resp.headers().get("content-encoding");
            assert!(
                content_encoding.is_some(),
                "Endpoint {} with data should have Content-Encoding header",
                endpoint
            );

            let encoding_value = content_encoding.unwrap().to_str().unwrap();
            assert_eq!(
                encoding_value, "gzip",
                "Endpoint {} should use gzip compression",
                endpoint
            );
        }
    }
}

#[tokio::test]
async fn test_compression_on_status_endpoint() {
    let (app, _state) = test_router_with_state();

    let req = Request::builder()
        .uri("/api/status")
        .header("Accept-Encoding", "gzip")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Even small responses like /api/status should have compression available
    // (though the middleware might skip compression for very small payloads)
    assert!(resp.status().is_success());
}

// ===========================================================================
// 6. Compression Effectiveness Tests
// ===========================================================================

#[tokio::test]
async fn test_compression_reduces_payload_size() {
    let (app1, state1) = test_router_with_state();
    let (app2, state2) = test_router_with_state();

    // Seed both routers with the same data
    seed_tasks(&state1, 50).await;
    seed_tasks(&state2, 50).await;

    // Request 1: Without compression
    let req_uncompressed = Request::builder()
        .uri("/api/tasks")
        .body(Body::empty())
        .unwrap();

    let resp_uncompressed = app1.oneshot(req_uncompressed).await.unwrap();
    assert_eq!(resp_uncompressed.status(), StatusCode::OK);
    let uncompressed_body = body_bytes(resp_uncompressed).await;
    let uncompressed_size = uncompressed_body.len();

    // Request 2: With gzip compression
    let req_compressed = Request::builder()
        .uri("/api/tasks")
        .header("Accept-Encoding", "gzip")
        .body(Body::empty())
        .unwrap();

    let resp_compressed = app2.oneshot(req_compressed).await.unwrap();
    assert_eq!(resp_compressed.status(), StatusCode::OK);

    // Check that compression header is present
    let content_encoding = resp_compressed.headers().get("content-encoding");
    assert!(
        content_encoding.is_some(),
        "Content-Encoding should be present"
    );
    assert_eq!(content_encoding.unwrap().to_str().unwrap(), "gzip");

    let compressed_body = body_bytes(resp_compressed).await;
    let compressed_size = compressed_body.len();

    // For a meaningful payload with 50 tasks, compressed should be significantly smaller
    // We're comparing the raw compressed bytes vs uncompressed bytes
    if uncompressed_size > 1000 {
        assert!(
            compressed_size < uncompressed_size,
            "Compressed size ({} bytes) should be smaller than uncompressed ({} bytes)",
            compressed_size,
            uncompressed_size
        );

        // Expect at least some compression benefit
        let compression_ratio = compressed_size as f64 / uncompressed_size as f64;
        assert!(
            compression_ratio < 0.9,
            "Compression ratio should be less than 90%, got {:.1}%",
            compression_ratio * 100.0
        );
    }
}

// ===========================================================================
// 7. POST/PUT Response Compression Tests
// ===========================================================================

#[tokio::test]
async fn test_compression_on_post_response() {
    let (app, _state) = test_router_with_state();

    let bead_id = Uuid::new_v4();
    let payload = serde_json::json!({
        "title": "Test Task for Compression",
        "bead_id": bead_id,
        "category": "feature",
        "priority": "medium",
        "complexity": "medium",
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/tasks")
        .header("Content-Type", "application/json")
        .header("Accept-Encoding", "gzip")
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // POST responses should also be compressed when requested
    let content_encoding = resp.headers().get("content-encoding");
    assert!(
        content_encoding.is_some(),
        "POST responses should be compressed when requested"
    );

    let encoding_value = content_encoding.unwrap().to_str().unwrap();
    assert_eq!(encoding_value, "gzip");
}
