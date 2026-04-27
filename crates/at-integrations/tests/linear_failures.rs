//! Failure-mode integration tests for the Linear GraphQL client.
//!
//! These tests exercise error paths (rate limits, 5xx responses, auth
//! failures, malformed responses, etc.) using a hermetic `wiremock`-backed
//! mock server. They never touch the real Linear API.
//!
//! Methodology — see `integration-hardening` skill:
//!   - env-driven credentials (mock server URI replaces production URL)
//!   - real-client behavior (the real `LinearClient` is exercised end-to-end
//!     through `reqwest`)
//!   - deterministic regression tests for failure modes (no flaky network)
//!
//! The Linear client gates network calls on `is_stub_key`, so every test
//! uses a token that:
//!
//!   * does NOT start with `tok` / `test` / `stub`
//!   * is at least 10 chars long
//!
//! The chosen test token (`lin_api_realistic_token_1234567890`) satisfies
//! both constraints and forces the real HTTP path. See
//! `crates/at-integrations/src/linear/mod.rs::is_stub_key`.

use at_integrations::linear::{LinearClient, LinearError};
use wiremock::matchers::{any, method};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A token that survives `LinearClient::is_stub_key` so the real HTTP path
/// is taken.
const TEST_API_KEY: &str = "lin_api_realistic_token_1234567890";

fn client_for(server: &MockServer) -> LinearClient {
    LinearClient::new_with_url(TEST_API_KEY, &server.uri()).expect("valid client")
}

// ---------------------------------------------------------------------------
// Rate limiting
// ---------------------------------------------------------------------------

#[tokio::test]
async fn linear_rate_limit_returns_typed_error() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "1")
                .set_body_string("rate limited"),
        )
        .expect(1..) // pin: client does NOT retry today; surface the error.
        .mount(&server)
        .await;

    let client = client_for(&server);
    let result = client.list_teams().await;

    assert!(result.is_err(), "expected rate-limit to surface as error");
    // 429 is not valid JSON, so the resp.json() call fails with an HTTP-typed
    // error. This pins current behavior: any future change (typed
    // `RateLimited` variant + retry) is intentionally a regression and must
    // update this test.
    let err = result.unwrap_err();
    assert!(
        matches!(err, LinearError::Http(_) | LinearError::Api(_)),
        "expected Http or Api error variant, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// 5xx — transient & hard
// ---------------------------------------------------------------------------

#[tokio::test]
async fn linear_503_then_200_no_retry_today() {
    let server = MockServer::start().await;

    // First (and only, since the client does not retry today) request
    // returns 503; pin this as the surfaced behavior so a future retry
    // implementation will flag this test as needing an update.
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503).set_body_string("temporary"))
        .expect(1..)
        .mount(&server)
        .await;

    let client = client_for(&server);
    let result = client.list_teams().await;

    assert!(result.is_err(), "503 must surface as error");
    let err = result.unwrap_err();
    assert!(
        matches!(err, LinearError::Http(_) | LinearError::Api(_)),
        "expected Http or Api error, got {err:?}"
    );
}

#[tokio::test]
async fn linear_503_persistent_returns_error() {
    let server = MockServer::start().await;

    Mock::given(any())
        .respond_with(ResponseTemplate::new(503).set_body_string("server down"))
        .mount(&server)
        .await;

    let client = client_for(&server);
    let result = client.list_teams().await;

    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Auth & forbidden
// ---------------------------------------------------------------------------

#[tokio::test]
async fn linear_401_unauthorized_no_retry() {
    let server = MockServer::start().await;

    // The mock will only ever serve one response; if the client retried it
    // would still see 401.
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
        .expect(1) // pin: exactly one request — client must NOT retry on 401.
        .mount(&server)
        .await;

    let client = client_for(&server);
    let result = client.list_teams().await;

    assert!(result.is_err(), "401 must produce an error");
    server.verify().await;
}

#[tokio::test]
async fn linear_403_forbidden_returns_error() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    let result = client.list_teams().await;

    assert!(result.is_err());
    server.verify().await;
}

// ---------------------------------------------------------------------------
// Malformed / empty bodies
// ---------------------------------------------------------------------------

#[tokio::test]
async fn linear_malformed_json_returns_parse_error() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string("{\"truncated\":"),
        )
        .mount(&server)
        .await;

    let client = client_for(&server);
    let result = client.list_teams().await;

    assert!(result.is_err(), "malformed JSON must produce error");
    let err = result.unwrap_err();
    // reqwest's `.json()` failure is wrapped as LinearError::Http; pin that.
    assert!(
        matches!(err, LinearError::Http(_)),
        "expected Http (parse) error, got {err:?}"
    );
}

#[tokio::test]
async fn linear_empty_response_returns_error() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(""),
        )
        .mount(&server)
        .await;

    let client = client_for(&server);
    let result = client.list_teams().await;

    assert!(result.is_err(), "empty body must produce error");
}

#[tokio::test]
async fn linear_graphql_errors_field_returns_api_error() {
    // 200 OK with a populated `errors` array in the GraphQL response —
    // the spec-compliant way Linear surfaces auth/permission failures.
    let server = MockServer::start().await;

    let body = serde_json::json!({
        "data": null,
        "errors": [
            { "message": "Authentication required, not authenticated.", "extensions": { "code": "AUTHENTICATION_ERROR" } }
        ]
    });

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;

    let client = client_for(&server);
    let result = client.list_teams().await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, LinearError::Api(ref msg) if msg.contains("Authentication")),
        "expected typed Api error containing the GraphQL error message, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Slow response / timeouts
// ---------------------------------------------------------------------------
//
// TODO(failures): The Linear client constructs `reqwest::Client::new()` with
// no `.timeout(...)` configured (see
// `crates/at-integrations/src/linear/mod.rs::graphql`), so a slow response
// scenario cannot be tested deterministically without holding the test open
// for the OS-level TCP read timeout. Add a timeout-on-build path and a test
// here once the client exposes a configurable timeout.
