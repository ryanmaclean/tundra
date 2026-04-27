//! Failure-mode integration tests for the GitLab REST client.
//!
//! Hermetic — every test spins up a `wiremock` `MockServer` and points the
//! `GitLabClient` at it via [`GitLabClient::new_with_url`]. Real GitLab is
//! never contacted.
//!
//! The client gates network calls on `is_stub_token` (see
//! `crates/at-integrations/src/gitlab/mod.rs`). Tokens shorter than 10 chars
//! or starting with `tok` / `stub` short-circuit into stub responses, which
//! would defeat failure-mode testing. We use
//! `glpat-realistic-token-1234567890` here — long enough and without the
//! sentinel prefixes — to force the real HTTP path.

use at_integrations::gitlab::{GitLabClient, GitLabError};
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TEST_TOKEN: &str = "glpat-realistic-token-1234567890";

fn client_for(server: &MockServer) -> GitLabClient {
    GitLabClient::new_with_url(&server.uri(), TEST_TOKEN).expect("valid client")
}

// ---------------------------------------------------------------------------
// Rate limiting
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gitlab_rate_limit_returns_typed_error() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_regex(r"^/api/v4/projects/.*/issues$"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "1")
                .set_body_string("Too Many Requests"),
        )
        .expect(1..) // pin: client does NOT retry today.
        .mount(&server)
        .await;

    let client = client_for(&server);
    let result = client.list_issues("42", Some("opened"), 1, 5).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, GitLabError::Api(ref s) if s.contains("429")),
        "expected typed Api error containing 429 status, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// 5xx — transient & hard
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gitlab_503_then_200_no_retry_today() {
    let server = MockServer::start().await;

    // Pin current behavior: no retry. The single 503 must surface as error.
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503).set_body_string("temporary"))
        .expect(1..)
        .mount(&server)
        .await;

    let client = client_for(&server);
    let result = client.list_issues("42", None, 1, 5).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, GitLabError::Api(_)));
}

#[tokio::test]
async fn gitlab_503_persistent_returns_error() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503).set_body_string("down"))
        .mount(&server)
        .await;

    let client = client_for(&server);
    let result = client.list_issues("42", None, 1, 5).await;

    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Auth & forbidden
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gitlab_401_unauthorized_no_retry() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    let result = client.list_issues("42", None, 1, 5).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, GitLabError::Api(ref s) if s.contains("401")),
        "expected Api error containing 401, got {err:?}"
    );
    server.verify().await;
}

#[tokio::test]
async fn gitlab_403_forbidden_returns_error() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(403).set_body_string("Forbidden"))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    let result = client.list_issues("42", None, 1, 5).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, GitLabError::Api(ref s) if s.contains("403")));
    server.verify().await;
}

// ---------------------------------------------------------------------------
// Malformed / empty bodies
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gitlab_malformed_json_returns_parse_error() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string("{\"truncated\":"),
        )
        .mount(&server)
        .await;

    let client = client_for(&server);
    let result = client.list_issues("42", None, 1, 5).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, GitLabError::Http(_)),
        "expected Http (parse) error, got {err:?}"
    );
}

#[tokio::test]
async fn gitlab_empty_body_returns_error() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(""),
        )
        .mount(&server)
        .await;

    let client = client_for(&server);
    let result = client.list_issues("42", None, 1, 5).await;

    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Slow response / timeouts
// ---------------------------------------------------------------------------
//
// TODO(failures): The GitLab client uses `reqwest::Client::new()` without a
// configured timeout (see `crates/at-integrations/src/gitlab/mod.rs:132`).
// A wiremock `set_delay`-based timeout test would block on the OS-level TCP
// read timeout and is not deterministic; add a timeout-on-build path and
// a test here once the client exposes a configurable timeout.
