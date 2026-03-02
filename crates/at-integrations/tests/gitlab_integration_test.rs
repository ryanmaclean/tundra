//! Integration tests for GitLab: client, issues, merge requests, OAuth,
//! and MR review engine — matching the GitLab integration surfaces.
//!
//! All tests use stub implementations so no real GitLab API calls are made.

use at_integrations::gitlab::mr_review::{
    MrReviewConfig, MrReviewEngine, MrReviewFinding, MrReviewResult, MrReviewSeverity,
};
use at_integrations::gitlab::oauth::{
    GitLabOAuthClient, GitLabOAuthConfig, GitLabUserProfile, OAuthTokenResponse,
};
use at_integrations::gitlab::{
    GitLabClient, GitLabError, GitLabIssue, GitLabMergeRequest, GitLabUser,
};

// ===========================================================================
// Test helpers
// ===========================================================================

fn test_client() -> GitLabClient {
    GitLabClient::new("glpat-test123").unwrap()
}

fn test_oauth_config() -> GitLabOAuthConfig {
    GitLabOAuthConfig {
        client_id: "test_client_id".to_string(),
        client_secret: "test_secret".to_string(),
        redirect_uri: "http://localhost:3000/callback".to_string(),
        scopes: vec!["read_user".to_string(), "api".to_string()],
    }
}

// ===========================================================================
// Client creation
// ===========================================================================

#[test]
fn client_creation_default_url() {
    let client = test_client();
    assert_eq!(client.base_url, "https://gitlab.com");
    assert_eq!(client.token, "glpat-test123");
}

#[test]
fn client_creation_custom_url() {
    let client = GitLabClient::new_with_url("https://gl.example.com/", "tok").unwrap();
    assert_eq!(client.base_url, "https://gl.example.com");
}

#[test]
fn client_creation_strips_trailing_slash() {
    let client = GitLabClient::new_with_url("https://gitlab.example.com/", "tok").unwrap();
    assert_eq!(client.base_url, "https://gitlab.example.com");
}

#[test]
fn client_creation_empty_token_fails() {
    let result = GitLabClient::new("");
    assert!(result.is_err());
    match result.unwrap_err() {
        GitLabError::MissingToken => {}
        other => panic!("Expected MissingToken, got: {other}"),
    }
}

#[test]
fn client_custom_url_empty_token_fails() {
    let result = GitLabClient::new_with_url("https://example.com", "");
    assert!(result.is_err());
}

// ===========================================================================
// Issue operations (stub)
// ===========================================================================

#[tokio::test]
async fn list_issues_respects_per_page() {
    let client = test_client();
    let issues = client
        .list_issues("42", Some("opened"), 1, 3)
        .await
        .unwrap();
    assert_eq!(issues.len(), 3);
}

#[tokio::test]
async fn list_issues_caps_at_five() {
    let client = test_client();
    let issues = client.list_issues("42", None, 1, 100).await.unwrap();
    assert!(issues.len() <= 5);
}

#[tokio::test]
async fn list_issues_default_state() {
    let client = test_client();
    let issues = client.list_issues("42", None, 1, 3).await.unwrap();
    for issue in &issues {
        assert_eq!(issue.state, "opened");
    }
}

#[tokio::test]
async fn list_issues_closed_state() {
    let client = test_client();
    let issues = client
        .list_issues("42", Some("closed"), 1, 3)
        .await
        .unwrap();
    for issue in &issues {
        assert_eq!(issue.state, "closed");
        assert!(issue.closed_at.is_some());
    }
}

#[tokio::test]
async fn get_issue_by_iid() {
    let client = test_client();
    let issue = client.get_issue("42", 7).await.unwrap();
    assert_eq!(issue.iid, 7);
    assert_eq!(issue.state, "opened");
}

#[tokio::test]
async fn issue_has_all_fields() {
    let client = test_client();
    let issues = client.list_issues("42", None, 1, 1).await.unwrap();
    let issue = &issues[0];

    assert!(issue.id > 0);
    assert!(issue.iid > 0);
    assert!(issue.project_id > 0);
    assert!(!issue.title.is_empty());
    assert!(issue.description.is_some());
    assert!(!issue.state.is_empty());
    assert!(!issue.author.username.is_empty());
    assert!(!issue.web_url.is_empty());
}

// ===========================================================================
// Merge request operations (stub)
// ===========================================================================

#[tokio::test]
async fn list_merge_requests_respects_per_page() {
    let client = test_client();
    let mrs = client.list_merge_requests("42", None, 1, 2).await.unwrap();
    assert_eq!(mrs.len(), 2);
}

#[tokio::test]
async fn list_merge_requests_merged_state() {
    let client = test_client();
    let mrs = client
        .list_merge_requests("42", Some("merged"), 1, 3)
        .await
        .unwrap();
    for mr in &mrs {
        assert_eq!(mr.state, "merged");
        assert!(mr.merged_at.is_some());
    }
}

#[tokio::test]
async fn get_merge_request_by_iid() {
    let client = test_client();
    let mr = client.get_merge_request("42", 5).await.unwrap();
    assert_eq!(mr.iid, 5);
    assert_eq!(mr.state, "opened");
}

#[tokio::test]
async fn create_merge_request_returns_correct_fields() {
    let client = test_client();
    let mr = client
        .create_merge_request("42", "My Feature", "feature/x", "main")
        .await
        .unwrap();
    assert_eq!(mr.title, "My Feature");
    assert_eq!(mr.source_branch, "feature/x");
    assert_eq!(mr.target_branch, "main");
    assert_eq!(mr.state, "opened");
    assert!(!mr.draft);
}

#[tokio::test]
async fn mr_has_all_fields() {
    let client = test_client();
    let mrs = client.list_merge_requests("42", None, 1, 1).await.unwrap();
    let mr = &mrs[0];

    assert!(mr.id > 0);
    assert!(mr.iid > 0);
    assert!(!mr.title.is_empty());
    assert!(!mr.source_branch.is_empty());
    assert!(!mr.target_branch.is_empty());
    assert!(!mr.author.username.is_empty());
    assert!(!mr.web_url.is_empty());
}

// ===========================================================================
// Serde roundtrips
// ===========================================================================

#[test]
fn gitlab_user_serde_roundtrip() {
    let user = GitLabUser {
        id: 42,
        username: "dev".to_string(),
        name: "Developer".to_string(),
        avatar_url: Some("https://example.com/avatar.png".to_string()),
        web_url: "https://gitlab.com/dev".to_string(),
    };
    let json = serde_json::to_string(&user).unwrap();
    let de: GitLabUser = serde_json::from_str(&json).unwrap();
    assert_eq!(de.id, 42);
    assert_eq!(de.username, "dev");
}

#[tokio::test]
async fn gitlab_issue_serde_roundtrip() {
    let client = test_client();
    let issues = client.list_issues("1", None, 1, 1).await.unwrap();
    let json = serde_json::to_string(&issues[0]).unwrap();
    let de: GitLabIssue = serde_json::from_str(&json).unwrap();
    assert_eq!(de.iid, issues[0].iid);
}

#[tokio::test]
async fn gitlab_mr_serde_roundtrip() {
    let client = test_client();
    let mrs = client.list_merge_requests("1", None, 1, 1).await.unwrap();
    let json = serde_json::to_string(&mrs[0]).unwrap();
    let de: GitLabMergeRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(de.iid, mrs[0].iid);
}

// ===========================================================================
// MR Review Engine
// ===========================================================================

#[test]
fn review_config_defaults() {
    let cfg = MrReviewConfig::default();
    assert_eq!(cfg.severity_threshold, MrReviewSeverity::Low);
    assert_eq!(cfg.max_findings, 50);
    assert!(!cfg.auto_approve);
}

#[test]
fn review_config_serde_roundtrip() {
    let cfg = MrReviewConfig {
        severity_threshold: MrReviewSeverity::High,
        max_findings: 10,
        auto_approve: true,
    };
    let json = serde_json::to_string(&cfg).unwrap();
    let de: MrReviewConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(de.severity_threshold, MrReviewSeverity::High);
    assert_eq!(de.max_findings, 10);
    assert!(de.auto_approve);
}

#[test]
fn severity_ordering() {
    assert!(MrReviewSeverity::Critical > MrReviewSeverity::High);
    assert!(MrReviewSeverity::High > MrReviewSeverity::Medium);
    assert!(MrReviewSeverity::Medium > MrReviewSeverity::Low);
    assert!(MrReviewSeverity::Low > MrReviewSeverity::Info);
}

#[tokio::test]
async fn review_returns_findings() {
    let engine = MrReviewEngine::with_defaults();
    let result = engine.review_mr("42", 1).await;
    assert!(!result.findings.is_empty());
    assert!(!result.summary.is_empty());
}

#[tokio::test]
async fn review_severity_threshold_filters() {
    let config = MrReviewConfig {
        severity_threshold: MrReviewSeverity::High,
        max_findings: 50,
        auto_approve: false,
    };
    let engine = MrReviewEngine::new(config);
    let result = engine.review_mr("42", 1).await;
    for finding in &result.findings {
        assert!(finding.severity >= MrReviewSeverity::High);
    }
}

#[tokio::test]
async fn review_max_findings_caps_output() {
    let config = MrReviewConfig {
        severity_threshold: MrReviewSeverity::Info,
        max_findings: 2,
        auto_approve: false,
    };
    let engine = MrReviewEngine::new(config);
    let result = engine.review_mr("42", 1).await;
    assert!(result.findings.len() <= 2);
}

#[tokio::test]
async fn review_auto_approve_blocked_by_critical() {
    let config = MrReviewConfig {
        severity_threshold: MrReviewSeverity::Info,
        max_findings: 50,
        auto_approve: true,
    };
    let engine = MrReviewEngine::new(config);
    let result = engine.review_mr("42", 1).await;
    assert!(!result.approved);
}

#[tokio::test]
async fn review_result_serde_roundtrip() {
    let engine = MrReviewEngine::with_defaults();
    let result = engine.review_mr("42", 1).await;
    let json = serde_json::to_string(&result).unwrap();
    let de: MrReviewResult = serde_json::from_str(&json).unwrap();
    assert_eq!(de.findings.len(), result.findings.len());
}

#[test]
fn finding_serde_roundtrip() {
    let finding = MrReviewFinding {
        file: "src/main.rs".to_string(),
        line: 42,
        severity: MrReviewSeverity::High,
        category: "security".to_string(),
        message: "SQL injection risk".to_string(),
        suggestion: Some("Use parameterized queries".to_string()),
    };
    let json = serde_json::to_string(&finding).unwrap();
    let de: MrReviewFinding = serde_json::from_str(&json).unwrap();
    assert_eq!(de.file, "src/main.rs");
    assert_eq!(de.severity, MrReviewSeverity::High);
}

// ===========================================================================
// OAuth
// ===========================================================================

#[test]
fn oauth_authorization_url_structure() {
    let client = GitLabOAuthClient::new(test_oauth_config());
    let url = client.authorization_url("csrf_token_123");
    assert!(url.starts_with("https://gitlab.com/oauth/authorize"));
    assert!(url.contains("client_id=test_client_id"));
    assert!(url.contains("state=csrf_token_123"));
    assert!(url.contains("response_type=code"));
    assert!(url.contains("scope=read_user%20api"));
}

#[test]
fn oauth_url_encodes_special_chars() {
    let mut config = test_oauth_config();
    config.client_id = "id with spaces".to_string();
    let client = GitLabOAuthClient::new(config);
    let url = client.authorization_url("state&evil=true");
    assert!(url.contains("client_id=id%20with%20spaces"));
    assert!(url.contains("state=state%26evil%3Dtrue"));
}

#[test]
fn oauth_token_response_deserialization() {
    let json = r#"{
        "access_token": "glpat-abc123",
        "token_type": "Bearer",
        "scope": "read_user api",
        "created_at": 1700000000
    }"#;
    let resp: OAuthTokenResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.access_token, "glpat-abc123");
    assert!(resp.refresh_token.is_none());
}

#[test]
fn oauth_token_response_with_refresh() {
    let json = r#"{
        "access_token": "glpat-abc123",
        "token_type": "Bearer",
        "scope": "api",
        "refresh_token": "glrt-refresh456",
        "expires_in": 7200,
        "created_at": 1700000000
    }"#;
    let resp: OAuthTokenResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.refresh_token.as_deref(), Some("glrt-refresh456"));
    assert_eq!(resp.expires_in, Some(7200));
}

#[test]
fn oauth_config_serde_roundtrip() {
    let config = test_oauth_config();
    let json = serde_json::to_string(&config).unwrap();
    let de: GitLabOAuthConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(de.client_id, "test_client_id");
    assert_eq!(de.scopes.len(), 2);
}

#[test]
fn gitlab_user_profile_deserialization() {
    let json = r#"{
        "id": 12345,
        "username": "tanuki",
        "name": "The Tanuki",
        "email": "tanuki@gitlab.com",
        "avatar_url": "https://gitlab.com/uploads/avatar.png",
        "web_url": "https://gitlab.com/tanuki",
        "state": "active"
    }"#;
    let user: GitLabUserProfile = serde_json::from_str(json).unwrap();
    assert_eq!(user.id, 12345);
    assert_eq!(user.username, "tanuki");
    assert_eq!(user.state, "active");
}

#[test]
fn gitlab_user_profile_optional_fields() {
    let json = r#"{
        "id": 99,
        "username": "bot",
        "name": "Bot",
        "email": null,
        "avatar_url": null,
        "web_url": "https://gitlab.com/bot",
        "state": "active"
    }"#;
    let user: GitLabUserProfile = serde_json::from_str(json).unwrap();
    assert!(user.email.is_none());
    assert!(user.avatar_url.is_none());
}

// ===========================================================================
// End-to-end workflow: issues → MR → review
// ===========================================================================

#[tokio::test]
async fn e2e_issue_to_mr_to_review() {
    let client = test_client();

    // 1. List open issues
    let issues = client
        .list_issues("42", Some("opened"), 1, 5)
        .await
        .unwrap();
    assert!(!issues.is_empty());
    let issue = &issues[0];

    // 2. Create an MR to fix the issue
    let mr = client
        .create_merge_request(
            "42",
            &format!("Fix: {}", issue.title),
            "fix/stub-issue",
            "main",
        )
        .await
        .unwrap();
    assert!(mr.title.starts_with("Fix:"));

    // 3. Review the MR
    let engine = MrReviewEngine::with_defaults();
    let review = engine.review_mr("42", mr.iid).await;
    assert!(!review.findings.is_empty());

    // 4. Check if approvable
    let high_findings = review
        .findings
        .iter()
        .filter(|f| f.severity >= MrReviewSeverity::High)
        .count();
    if high_findings > 0 {
        assert!(!review.approved);
    }
}

#[tokio::test]
async fn e2e_list_all_resources() {
    let client = test_client();

    // Issues
    let issues = client.list_issues("1", None, 1, 5).await.unwrap();
    assert!(!issues.is_empty());

    // MRs
    let mrs = client.list_merge_requests("1", None, 1, 5).await.unwrap();
    assert!(!mrs.is_empty());

    // Single issue
    let issue = client.get_issue("1", 1).await.unwrap();
    assert_eq!(issue.iid, 1);

    // Single MR
    let mr = client.get_merge_request("1", 1).await.unwrap();
    assert_eq!(mr.iid, 1);
}
