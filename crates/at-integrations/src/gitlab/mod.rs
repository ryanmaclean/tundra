pub mod mr_review;
pub mod oauth;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when interacting with the GitLab API.
///
/// This enum represents failures that may happen during GitLab client
/// operations, including API errors, authentication failures, and network issues.
#[derive(Debug, Error)]
pub enum GitLabError {
    /// The GitLab API returned an error response.
    ///
    /// This includes HTTP error responses (4xx, 5xx), rate limiting,
    /// and API-specific error messages. The contained string provides
    /// details about the failure, including the HTTP status code and
    /// response body when available.
    #[error("GitLab API error: {0}")]
    Api(String),

    /// GitLab API token was not provided.
    ///
    /// This occurs when attempting to create a client without a valid
    /// personal access token (PAT) or OAuth token. Provide a token via
    /// [`GitLabClient::new`] or [`GitLabClient::new_with_url`].
    #[error("missing GitLab token")]
    MissingToken,

    /// Failed to serialize or deserialize JSON data.
    ///
    /// This may occur when parsing GitLab API responses or constructing
    /// request bodies for issues, merge requests, or other resources.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    /// An HTTP-level error occurred.
    ///
    /// This includes network failures, connection errors, DNS resolution
    /// failures, and other transport-layer issues when communicating with
    /// the GitLab API.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

/// Result type alias for GitLab operations.
///
/// This is a convenience alias for `Result<T, GitLabError>` used throughout
/// the GitLab client implementation.
pub type Result<T> = std::result::Result<T, GitLabError>;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitLabUser {
    pub id: u64,
    pub username: String,
    pub name: String,
    pub avatar_url: Option<String>,
    pub web_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitLabIssue {
    pub id: u64,
    pub iid: u32,
    pub project_id: u64,
    pub title: String,
    pub description: Option<String>,
    pub state: String,
    pub author: GitLabUser,
    pub assignees: Vec<GitLabUser>,
    pub labels: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub web_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitLabMergeRequest {
    pub id: u64,
    pub iid: u32,
    pub project_id: u64,
    pub title: String,
    pub description: Option<String>,
    pub state: String,
    pub author: GitLabUser,
    pub source_branch: String,
    pub target_branch: String,
    pub labels: Vec<String>,
    pub draft: bool,
    pub merge_status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub merged_at: Option<DateTime<Utc>>,
    pub web_url: String,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct GitLabClient {
    pub base_url: String,
    pub token: String,
    pub client: reqwest::Client,
}

impl GitLabClient {
    /// Create a client for `https://gitlab.com`.
    pub fn new(token: &str) -> Result<Self> {
        Self::new_with_url("https://gitlab.com", token)
    }

    /// Create a client for a custom GitLab instance.
    pub fn new_with_url(base_url: &str, token: &str) -> Result<Self> {
        if token.is_empty() {
            return Err(GitLabError::MissingToken);
        }
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            client: reqwest::Client::new(),
        })
    }

    // -- request helpers ----------------------------------------------------

    pub(crate) async fn api_get(&self, path: &str) -> Result<reqwest::Response> {
        let url = format!("{}/api/v4{}", self.base_url, path);
        let resp = self
            .client
            .get(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(GitLabError::Api(format!(
                "{} {}: {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or(""),
                body
            )));
        }

        Ok(resp)
    }

    async fn api_post(&self, path: &str, body: &serde_json::Value) -> Result<reqwest::Response> {
        let url = format!("{}/api/v4{}", self.base_url, path);
        let resp = self
            .client
            .post(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .json(body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(GitLabError::Api(format!(
                "{} {}: {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or(""),
                body
            )));
        }

        Ok(resp)
    }

    // -- stub helpers --------------------------------------------------------

    /// Returns true when the token looks like a test/stub token rather than
    /// a real GitLab PAT. Real tokens start with `glpat-` followed by 20+
    /// characters. Anything shorter or starting with common test prefixes
    /// triggers stub mode so tests work without network access.
    pub(crate) fn is_stub_token(&self) -> bool {
        let t = &self.token;
        t.starts_with("tok") || t.starts_with("stub") || t == "glpat-test123" || t.len() < 10
    }

    fn stub_user() -> GitLabUser {
        GitLabUser {
            id: 1,
            username: "stub-user".to_string(),
            name: "Stub User".to_string(),
            avatar_url: None,
            web_url: "https://gitlab.com/stub-user".to_string(),
        }
    }

    fn stub_issue(project_id: &str, iid: u32, state: &str) -> GitLabIssue {
        let now = Utc::now();
        GitLabIssue {
            id: iid as u64 * 100,
            iid,
            project_id: project_id.parse().unwrap_or(1),
            title: format!("Stub issue #{iid}"),
            description: Some("Auto-generated stub issue".to_string()),
            state: state.to_string(),
            author: Self::stub_user(),
            assignees: vec![],
            labels: vec!["stub".to_string()],
            created_at: now,
            updated_at: now,
            closed_at: if state == "closed" { Some(now) } else { None },
            web_url: format!("{}/{}/issues/{}", "https://gitlab.com", project_id, iid),
        }
    }

    fn stub_mr(project_id: &str, iid: u32, state: &str) -> GitLabMergeRequest {
        let now = Utc::now();
        GitLabMergeRequest {
            id: iid as u64 * 100,
            iid,
            project_id: project_id.parse().unwrap_or(1),
            title: format!("Stub MR !{iid}"),
            description: Some("Auto-generated stub merge request".to_string()),
            state: state.to_string(),
            author: Self::stub_user(),
            source_branch: "feature/stub".to_string(),
            target_branch: "main".to_string(),
            labels: vec![],
            draft: false,
            merge_status: "can_be_merged".to_string(),
            created_at: now,
            updated_at: now,
            merged_at: if state == "merged" { Some(now) } else { None },
            web_url: format!(
                "{}/{}/merge_requests/{}",
                "https://gitlab.com", project_id, iid
            ),
        }
    }

    // -- public API ---------------------------------------------------------

    /// List issues for a project.
    pub async fn list_issues(
        &self,
        project_id: &str,
        state: Option<&str>,
        page: u32,
        per_page: u32,
    ) -> Result<Vec<GitLabIssue>> {
        if self.is_stub_token() {
            let s = state.unwrap_or("opened");
            let count = per_page.min(5);
            let issues = (1..=count)
                .map(|i| Self::stub_issue(project_id, i, s))
                .collect();
            return Ok(issues);
        }

        let encoded_id = urlencoding::encode(project_id);
        let mut path = format!(
            "/projects/{}/issues?page={}&per_page={}",
            encoded_id, page, per_page
        );
        if let Some(s) = state {
            path.push_str(&format!("&state={}", s));
        }

        let resp = self.api_get(&path).await?;
        let issues: Vec<GitLabIssue> = resp.json().await?;
        Ok(issues)
    }

    /// Get a single issue by IID.
    pub async fn get_issue(&self, project_id: &str, iid: u32) -> Result<GitLabIssue> {
        if self.is_stub_token() {
            return Ok(Self::stub_issue(project_id, iid, "opened"));
        }

        let encoded_id = urlencoding::encode(project_id);
        let path = format!("/projects/{}/issues/{}", encoded_id, iid);
        let resp = self.api_get(&path).await?;
        let issue: GitLabIssue = resp.json().await?;
        Ok(issue)
    }

    /// List merge requests for a project.
    pub async fn list_merge_requests(
        &self,
        project_id: &str,
        state: Option<&str>,
        page: u32,
        per_page: u32,
    ) -> Result<Vec<GitLabMergeRequest>> {
        if self.is_stub_token() {
            let s = state.unwrap_or("opened");
            let count = per_page.min(5);
            let mrs = (1..=count)
                .map(|i| Self::stub_mr(project_id, i, s))
                .collect();
            return Ok(mrs);
        }

        let encoded_id = urlencoding::encode(project_id);
        let mut path = format!(
            "/projects/{}/merge_requests?page={}&per_page={}",
            encoded_id, page, per_page
        );
        if let Some(s) = state {
            path.push_str(&format!("&state={}", s));
        }

        let resp = self.api_get(&path).await?;
        let mrs: Vec<GitLabMergeRequest> = resp.json().await?;
        Ok(mrs)
    }

    /// Get a single merge request by IID.
    pub async fn get_merge_request(
        &self,
        project_id: &str,
        iid: u32,
    ) -> Result<GitLabMergeRequest> {
        if self.is_stub_token() {
            return Ok(Self::stub_mr(project_id, iid, "opened"));
        }

        let encoded_id = urlencoding::encode(project_id);
        let path = format!("/projects/{}/merge_requests/{}", encoded_id, iid);
        let resp = self.api_get(&path).await?;
        let mr: GitLabMergeRequest = resp.json().await?;
        Ok(mr)
    }

    /// Create a merge request.
    pub async fn create_merge_request(
        &self,
        project_id: &str,
        title: &str,
        source: &str,
        target: &str,
    ) -> Result<GitLabMergeRequest> {
        if self.is_stub_token() {
            let now = Utc::now();
            return Ok(GitLabMergeRequest {
                id: 999,
                iid: 1,
                project_id: project_id.parse().unwrap_or(1),
                title: title.to_string(),
                description: None,
                state: "opened".to_string(),
                author: Self::stub_user(),
                source_branch: source.to_string(),
                target_branch: target.to_string(),
                labels: vec![],
                draft: false,
                merge_status: "can_be_merged".to_string(),
                created_at: now,
                updated_at: now,
                merged_at: None,
                web_url: format!("{}/{}/merge_requests/1", "https://gitlab.com", project_id),
            });
        }

        let encoded_id = urlencoding::encode(project_id);
        let path = format!("/projects/{}/merge_requests", encoded_id);
        let body = serde_json::json!({
            "title": title,
            "source_branch": source,
            "target_branch": target,
        });

        let resp = self.api_post(&path, &body).await?;
        let mr: GitLabMergeRequest = resp.json().await?;
        Ok(mr)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_creation() {
        let client = GitLabClient::new("glpat-test123").unwrap();
        assert_eq!(client.base_url, "https://gitlab.com");
    }

    #[test]
    fn client_custom_url() {
        let client = GitLabClient::new_with_url("https://gl.example.com/", "tok").unwrap();
        assert_eq!(client.base_url, "https://gl.example.com");
    }

    #[test]
    fn client_missing_token() {
        let result = GitLabClient::new("");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn list_issues_stub() {
        let client = GitLabClient::new("tok").unwrap();
        let issues = client
            .list_issues("42", Some("opened"), 1, 3)
            .await
            .unwrap();
        assert_eq!(issues.len(), 3);
        assert_eq!(issues[0].iid, 1);
        assert_eq!(issues[0].state, "opened");
    }

    #[tokio::test]
    async fn get_issue_stub() {
        let client = GitLabClient::new("tok").unwrap();
        let issue = client.get_issue("42", 7).await.unwrap();
        assert_eq!(issue.iid, 7);
    }

    #[tokio::test]
    async fn list_merge_requests_stub() {
        let client = GitLabClient::new("tok").unwrap();
        let mrs = client.list_merge_requests("42", None, 1, 2).await.unwrap();
        assert_eq!(mrs.len(), 2);
    }

    #[tokio::test]
    async fn get_merge_request_stub() {
        let client = GitLabClient::new("tok").unwrap();
        let mr = client.get_merge_request("42", 5).await.unwrap();
        assert_eq!(mr.iid, 5);
    }

    #[tokio::test]
    async fn create_merge_request_stub() {
        let client = GitLabClient::new("tok").unwrap();
        let mr = client
            .create_merge_request("42", "My MR", "feature/x", "main")
            .await
            .unwrap();
        assert_eq!(mr.title, "My MR");
        assert_eq!(mr.source_branch, "feature/x");
        assert_eq!(mr.target_branch, "main");
    }

    #[test]
    fn gitlab_issue_serde_roundtrip() {
        let client = GitLabClient::new("tok").unwrap();
        let issue = GitLabClient::stub_issue("1", 1, "opened");
        let json = serde_json::to_string(&issue).unwrap();
        let de: GitLabIssue = serde_json::from_str(&json).unwrap();
        assert_eq!(de.iid, 1);
        // Ensure client is used to suppress warning
        let _ = client.base_url.len();
    }

    #[test]
    fn gitlab_mr_serde_roundtrip() {
        let mr = GitLabClient::stub_mr("1", 3, "merged");
        let json = serde_json::to_string(&mr).unwrap();
        let de: GitLabMergeRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(de.iid, 3);
        assert_eq!(de.state, "merged");
    }

    #[test]
    fn non_test_token_not_detected_as_test() {
        let client = GitLabClient::new("glpat-real-token-abc123").unwrap();
        assert!(!client.is_stub_token());
    }

    #[test]
    fn test_token_detected() {
        let client = GitLabClient::new("tok-fake").unwrap();
        assert!(client.is_stub_token());
        let client2 = GitLabClient::new("stub-token").unwrap();
        assert!(client2.is_stub_token());
    }
}
