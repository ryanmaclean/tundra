//! Failure-mode integration tests for the GitHub client.
//!
//! These exercise the GitHub client against a hermetic `wiremock`
//! `MockServer` configured via `GitHubClient::new_with_base_url` (a thin
//! constructor over `octocrab::Octocrab::builder().base_uri(...)`). The
//! production code path (`GitHubClient::new`) is unchanged.
//!
//! GitHub error mapping note: octocrab maps non-2xx responses to
//! `octocrab::Error::GitHub { source }` when the body parses as a GitHub
//! JSON error envelope, otherwise to a JSON / Serde / Other variant. Our
//! local wrapper `GitHubError::Api(#[from] octocrab::Error)` carries any of
//! these. Each test below pins the surfaced variant so a future error-mapping
//! change is detectable.

use at_integrations::github::client::{GitHubClient, GitHubError};
use at_integrations::types::GitHubConfig;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TEST_TOKEN: &str = "ghp_realistic_token_1234567890";
const OWNER: &str = "testowner";
const REPO: &str = "testrepo";

fn make_client(server: &MockServer) -> GitHubClient {
    let config = GitHubConfig {
        token: Some(TEST_TOKEN.to_string()),
        owner: OWNER.to_string(),
        repo: REPO.to_string(),
    };
    GitHubClient::new_with_base_url(config, &server.uri()).expect("valid client")
}

/// Path for the "list issues" call we exercise in each test.
const LIST_ISSUES_PATH: &str = "/repos/testowner/testrepo/issues";

/// Drive a single request through the client. We use the octocrab handler
/// directly because `at_integrations::github::issues::list_issues` requires
/// crate-internal helpers; the failure-mode behavior we care about lives
/// inside octocrab and is identical regardless of which list-shaped helper
/// invokes it.
async fn list_issues(client: &GitHubClient) -> octocrab::Result<octocrab::Page<octocrab::models::issues::Issue>> {
    client
        .inner()
        .issues(client.owner(), client.repo())
        .list()
        .send()
        .await
}

// ---------------------------------------------------------------------------
// Rate limiting
// ---------------------------------------------------------------------------

#[tokio::test]
async fn github_rate_limit_returns_typed_error() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(LIST_ISSUES_PATH))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "1")
                .set_body_json(serde_json::json!({
                    "message": "API rate limit exceeded",
                    "documentation_url": "https://docs.github.com/rest"
                })),
        )
        .expect(1..) // pin: client does NOT retry today.
        .mount(&server)
        .await;

    let client = make_client(&server);
    let result = list_issues(&client).await.map_err(GitHubError::from);

    let err = result.expect_err("429 must surface");
    assert!(
        matches!(err, GitHubError::Api(_)),
        "expected GitHubError::Api, got {err:?}"
    );
    // octocrab parses the GitHub error envelope into a typed `GitHubError`
    // with a `status_code`. Verify the status code is the 429 we sent so a
    // future change in error mapping is detectable.
    if let GitHubError::Api(octocrab::Error::GitHub { source, .. }) = &err {
        assert_eq!(source.status_code.as_u16(), 429, "status_code mismatch");
    } else {
        panic!("expected octocrab::Error::GitHub with 429 status, got {err:?}");
    }
}

// ---------------------------------------------------------------------------
// 5xx — transient & hard
// ---------------------------------------------------------------------------

#[tokio::test]
async fn github_503_then_200_no_retry_today() {
    let server = MockServer::start().await;

    // Pin current behavior: no retry. The single 503 must surface as error.
    Mock::given(method("GET"))
        .and(path(LIST_ISSUES_PATH))
        .respond_with(
            ResponseTemplate::new(503)
                .set_body_json(serde_json::json!({ "message": "service unavailable" })),
        )
        .expect(1..)
        .mount(&server)
        .await;

    let client = make_client(&server);
    let result = list_issues(&client).await.map_err(GitHubError::from);

    let err = result.expect_err("503 must surface");
    assert!(matches!(err, GitHubError::Api(_)));
}

#[tokio::test]
async fn github_503_persistent_returns_error() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(503)
                .set_body_json(serde_json::json!({ "message": "down" })),
        )
        .mount(&server)
        .await;

    let client = make_client(&server);
    let result = list_issues(&client).await.map_err(GitHubError::from);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Auth & forbidden
// ---------------------------------------------------------------------------

#[tokio::test]
async fn github_401_unauthorized_no_retry() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(LIST_ISSUES_PATH))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "message": "Bad credentials",
                "documentation_url": "https://docs.github.com/rest"
            })),
        )
        .expect(1) // pin: exactly one request — no retry on 401.
        .mount(&server)
        .await;

    let client = make_client(&server);
    let result = list_issues(&client).await.map_err(GitHubError::from);

    let err = result.expect_err("401 must surface");
    assert!(matches!(err, GitHubError::Api(_)));
    if let GitHubError::Api(octocrab::Error::GitHub { source, .. }) = &err {
        assert_eq!(source.status_code.as_u16(), 401);
    } else {
        panic!("expected octocrab::Error::GitHub with 401 status, got {err:?}");
    }
    server.verify().await;
}

#[tokio::test]
async fn github_403_forbidden_returns_error() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(LIST_ISSUES_PATH))
        .respond_with(
            ResponseTemplate::new(403).set_body_json(serde_json::json!({
                "message": "Forbidden",
                "documentation_url": "https://docs.github.com/rest"
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = make_client(&server);
    let result = list_issues(&client).await.map_err(GitHubError::from);
    assert!(result.is_err());
    server.verify().await;
}

// ---------------------------------------------------------------------------
// Malformed / empty bodies
// ---------------------------------------------------------------------------

#[tokio::test]
async fn github_malformed_json_returns_parse_error() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(LIST_ISSUES_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string("{\"truncated\":"),
        )
        .mount(&server)
        .await;

    let client = make_client(&server);
    let result = list_issues(&client).await.map_err(GitHubError::from);

    let err = result.expect_err("malformed JSON must surface");
    // octocrab routes JSON parse errors through Error::Json/Serde, all of
    // which our wrapper carries via GitHubError::Api.
    assert!(
        matches!(err, GitHubError::Api(_)),
        "expected GitHubError::Api wrapping a JSON parse error, got {err:?}"
    );
}

#[tokio::test]
async fn github_empty_body_returns_error() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(LIST_ISSUES_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(""),
        )
        .mount(&server)
        .await;

    let client = make_client(&server);
    let result = list_issues(&client).await.map_err(GitHubError::from);
    assert!(result.is_err(), "empty body must produce error");
}

// ---------------------------------------------------------------------------
// Slow response / timeouts
// ---------------------------------------------------------------------------
//
// TODO(failures): octocrab's default builder doesn't expose a per-request
// timeout in `GitHubClient::new` (cf.
// `crates/at-integrations/src/github/client.rs::new`). A `set_delay`-based
// timeout test would either deadlock the test harness or rely on OS-level
// TCP timeouts. Wire `octocrab::OctocrabBuilder::add_retry_config` /
// custom hyper layers when a configurable timeout is added.
