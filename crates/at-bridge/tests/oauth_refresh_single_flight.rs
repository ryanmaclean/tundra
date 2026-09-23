//! Single-flight deduplication tests for the GitHub OAuth refresh endpoint.
//!
//! ## What these tests pin
//!
//! The `github_oauth_refresh` handler (at-bridge/src/http_api/github.rs) now
//! uses a `watch`-channel gate stored in `ApiState::github_refresh_gate` so
//! that N concurrent expired-token callers produce exactly **one** outbound
//! HTTP request.  These tests verify the four correctness properties of that
//! gate:
//!
//! 1. **Deduplication** — N=10 concurrent callers → 1 outbound request, all
//!    callers receive a successful `{"refreshed": true}` body.
//! 2. **Error fan-out** — when the single outbound request fails, *all* N
//!    concurrent callers see the error; none silently succeeds.
//! 3. **Gate reset after failure** — after a failed refresh the gate is
//!    cleared, so the next caller issues a fresh outbound request.
//! 4. **No busy-loop on 429** — a rate-limit error is returned promptly
//!    without retrying.

// Suppress the "holding a mutex across an await" lint because the ENV_MUTEX
// is intentionally held across test setup awaits to serialise env-var access.
#![allow(clippy::await_holding_lock)]

use std::sync::Arc;
use std::time::Duration;

use at_bridge::event_bus::EventBus;
use at_bridge::http_api::{api_router, ApiState};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ---------------------------------------------------------------------------
// Env-var access must be serialised across tests in the same process.
// ---------------------------------------------------------------------------
static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// RAII guard that removes a list of env vars on Drop, even on panic.
/// Without this, a test that sets GITHUB_OAUTH_CLIENT_ID and panics would
/// leak the var into the next test (which might assert missing-var behavior).
struct EnvGuard {
    keys: &'static [&'static str],
}

impl EnvGuard {
    fn new(keys: &'static [&'static str]) -> Self {
        Self { keys }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for k in self.keys {
            std::env::remove_var(k);
        }
    }
}

const OAUTH_ENV_KEYS: &[&str] = &["GITHUB_OAUTH_CLIENT_ID", "GITHUB_OAUTH_CLIENT_SECRET"];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// The JSON body the mock server returns on a successful refresh.
fn success_body() -> serde_json::Value {
    serde_json::json!({
        "access_token": "gho_fresh_access_token",
        "token_type": "bearer",
        "scope": "repo,read:user",
        "refresh_token": "ghr_fresh_refresh_token",
        "expires_in": 28800
    })
}

/// Spin up an axum server backed by an `ApiState` whose GitHub token endpoint
/// is redirected to `token_url` (a wiremock server URL).  Returns the HTTP
/// base URL of the axum server and the `Arc<ApiState>` so tests can
/// pre-populate `oauth_token_manager`.
async fn start_server_with_token_url(token_url: &str) -> (String, Arc<ApiState>) {
    let event_bus = EventBus::new();
    let state = Arc::new(
        ApiState::new(event_bus)
            .with_relaxed_rate_limits()
            .with_github_token_url(token_url),
    );
    let router = api_router(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (format!("http://{addr}"), state)
}

/// Pre-load an expired token (with a refresh token) into `state`.
async fn store_expired_token_with_refresh(state: &Arc<ApiState>) {
    state
        .oauth_token_manager
        .write()
        .await
        .store_token(
            "gho_old_expired_token",
            Some(0), // expires immediately
            Some("ghr_valid_refresh_token"),
        )
        .await;
}

// ---------------------------------------------------------------------------
// Test 1 — concurrent callers produce exactly one outbound HTTP request
// ---------------------------------------------------------------------------

/// Ten concurrent callers with an expired token must result in exactly one
/// outbound HTTP refresh request.  All ten callers must receive
/// `{"refreshed": true}`.
#[tokio::test]
async fn concurrent_expired_callers_result_in_single_http_refresh() {
    let _env_lock = ENV_MUTEX.lock().unwrap();

    let mock_server = MockServer::start().await;

    // The mock responds after a brief delay so that all 10 concurrent callers
    // arrive at the gate before the first one completes.
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(150))
                .set_body_json(success_body()),
        )
        .expect(1) // ← wiremock will assert exactly 1 request was received
        .mount(&mock_server)
        .await;

    let (base, state) = start_server_with_token_url(&mock_server.uri()).await;
    store_expired_token_with_refresh(&state).await;

    std::env::set_var("GITHUB_OAUTH_CLIENT_ID", "test_client_id");
    std::env::set_var("GITHUB_OAUTH_CLIENT_SECRET", "test_secret");
    let _env_cleanup = EnvGuard::new(OAUTH_ENV_KEYS);

    // Fire 10 concurrent refresh requests.
    const N: usize = 10;
    let client = reqwest::Client::new();
    let mut handles = Vec::with_capacity(N);
    for _ in 0..N {
        let url = format!("{base}/api/github/oauth/refresh");
        let c = client.clone();
        handles.push(tokio::spawn(async move {
            c.post(&url).send().await.expect("request failed")
        }));
    }

    let mut results = Vec::with_capacity(N);
    for h in handles {
        results.push(h.await.expect("task panicked"));
    }

    // Every caller must have received HTTP 200.
    for resp in &results {
        assert_eq!(
            resp.status(),
            200,
            "expected 200 from refresh, got {}",
            resp.status()
        );
    }

    // Parse and check every body contains "refreshed": true.
    for resp in results {
        let body: Value = resp.json().await.expect("body is JSON");
        assert_eq!(
            body["refreshed"], true,
            "expected refreshed=true, got {body}"
        );
    }

    // wiremock's `expect(1)` is verified on drop of `mock_server`.
    // Drop it explicitly so the assertion fires inside this test.
    mock_server.verify().await;
}

// ---------------------------------------------------------------------------
// Test 2 — refresh failure propagates to all concurrent callers
// ---------------------------------------------------------------------------

/// When the single outbound request fails (GitHub returns an error body),
/// all N concurrent callers must see an error response — none silently
/// receives a success.
#[tokio::test]
async fn refresh_failure_propagates_to_all_concurrent_callers() {
    let _env_lock = ENV_MUTEX.lock().unwrap();

    let mock_server = MockServer::start().await;

    // GitHub returns an OAuth error body (status 200, error in body).
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(150))
                .set_body_json(serde_json::json!({
                    "error": "invalid_grant",
                    "error_description": "The refresh token is expired or revoked."
                })),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let (base, state) = start_server_with_token_url(&mock_server.uri()).await;
    store_expired_token_with_refresh(&state).await;

    std::env::set_var("GITHUB_OAUTH_CLIENT_ID", "test_client_id");
    std::env::set_var("GITHUB_OAUTH_CLIENT_SECRET", "test_secret");
    let _env_cleanup = EnvGuard::new(OAUTH_ENV_KEYS);

    const N: usize = 10;
    let client = reqwest::Client::new();
    let mut handles = Vec::with_capacity(N);
    for _ in 0..N {
        let url = format!("{base}/api/github/oauth/refresh");
        let c = client.clone();
        handles.push(tokio::spawn(async move {
            c.post(&url).send().await.expect("request failed")
        }));
    }

    let mut success_count = 0usize;
    let mut error_count = 0usize;
    for h in handles {
        let resp = h.await.expect("task panicked");
        if resp.status().is_success() {
            success_count += 1;
        } else {
            error_count += 1;
        }
        // Consume the body (avoids dropping a partially-read response).
        let _ = resp.bytes().await;
    }

    assert_eq!(
        success_count, 0,
        "no caller should succeed when the refresh fails"
    );
    assert_eq!(
        error_count, N,
        "all {N} callers should see an error response"
    );

    mock_server.verify().await;
}

// ---------------------------------------------------------------------------
// Test 3 — subsequent call after failure issues a new outbound request
// ---------------------------------------------------------------------------

/// After a refresh fails and the gate is cleared, the next call must issue
/// a fresh outbound HTTP request rather than re-using stale failure state.
#[tokio::test]
async fn subsequent_call_after_failure_retries() {
    let _env_lock = ENV_MUTEX.lock().unwrap();

    let mock_server = MockServer::start().await;

    // First call: fail.
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "error": "invalid_grant",
            "error_description": "Refresh token expired."
        })))
        .up_to_n_times(1)
        .mount(&mock_server)
        .await;

    // Second call: succeed.
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(success_body()))
        .mount(&mock_server)
        .await;

    let (base, state) = start_server_with_token_url(&mock_server.uri()).await;
    store_expired_token_with_refresh(&state).await;

    std::env::set_var("GITHUB_OAUTH_CLIENT_ID", "test_client_id");
    std::env::set_var("GITHUB_OAUTH_CLIENT_SECRET", "test_secret");
    let _env_cleanup = EnvGuard::new(OAUTH_ENV_KEYS);

    let client = reqwest::Client::new();
    let url = format!("{base}/api/github/oauth/refresh");

    // First request — must fail.
    let resp1 = client
        .post(&url)
        .send()
        .await
        .expect("first request failed");
    assert!(
        !resp1.status().is_success(),
        "first refresh should fail, got {}",
        resp1.status()
    );
    let _ = resp1.bytes().await;

    // The gate is now cleared synchronously by LeaderGuard::finish before
    // the leader's response is sent — assert it directly rather than
    // sleeping for a spawned task.
    assert!(
        state.github_refresh_gate.lock().await.is_none(),
        "gate must be cleared deterministically by LeaderGuard::finish"
    );

    // Re-seed the refresh token (the failed refresh didn't overwrite it).
    store_expired_token_with_refresh(&state).await;

    // Second request — must succeed and issue a new outbound HTTP call.
    let resp2 = client
        .post(&url)
        .send()
        .await
        .expect("second request failed");
    assert_eq!(
        resp2.status(),
        200,
        "second refresh should succeed, got {}",
        resp2.status()
    );
    let body: Value = resp2.json().await.expect("body is JSON");
    assert_eq!(body["refreshed"], true, "expected refreshed=true: {body}");

    // Verify that wiremock received exactly 2 requests total.
    let received = mock_server.received_requests().await.unwrap();
    assert_eq!(
        received.len(),
        2,
        "expected 2 outbound requests (1 fail + 1 retry), got {}",
        received.len()
    );
}

// ---------------------------------------------------------------------------
// Test 4 — 429 rate-limit returns promptly without busy-looping
// ---------------------------------------------------------------------------

/// When the token endpoint returns HTTP 429 (rate limited), the handler must
/// return an error promptly.  This pins that the single-flight gate does NOT
/// retry on its own — one request, one error, done.
#[tokio::test]
async fn refresh_429_does_not_retry_indefinitely() {
    let _env_lock = ENV_MUTEX.lock().unwrap();

    let mock_server = MockServer::start().await;

    // Return 429 every time — but we should only see one request.
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
            "error": "rate_limited",
            "error_description": "Too many requests. Please wait."
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let (base, state) = start_server_with_token_url(&mock_server.uri()).await;
    store_expired_token_with_refresh(&state).await;

    std::env::set_var("GITHUB_OAUTH_CLIENT_ID", "test_client_id");
    std::env::set_var("GITHUB_OAUTH_CLIENT_SECRET", "test_secret");
    let _env_cleanup = EnvGuard::new(OAUTH_ENV_KEYS);

    let start = std::time::Instant::now();

    let resp = reqwest::Client::new()
        .post(format!("{base}/api/github/oauth/refresh"))
        .send()
        .await
        .expect("request failed");

    let elapsed = start.elapsed();

    // Must not be a 2xx (refresh failed).
    assert!(
        !resp.status().is_success(),
        "429 from GitHub should surface as error, got {}",
        resp.status()
    );

    let body: Value = resp.json().await.expect("body is JSON");
    assert!(
        body["error"].is_string(),
        "error field expected in body: {body}"
    );

    // Must complete within a reasonable time — no busy-loop or indefinite
    // retry.  5 seconds is very generous given the mock responds instantly.
    assert!(
        elapsed < Duration::from_secs(5),
        "refresh took too long ({elapsed:?}), possible retry loop"
    );

    mock_server.verify().await;
}
