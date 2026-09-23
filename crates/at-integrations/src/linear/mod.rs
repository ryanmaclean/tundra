pub mod sync;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when interacting with the Linear GraphQL API.
///
/// This enum represents failures that may happen during Linear client
/// operations, including GraphQL errors, authentication failures, and network issues.
#[derive(Debug, Error)]
pub enum LinearError {
    /// The Linear API returned an error response.
    ///
    /// This includes GraphQL errors, invalid queries, authorization failures,
    /// and resource-not-found errors. The contained string provides details
    /// about the failure, typically including the GraphQL error message(s)
    /// returned by the Linear API.
    #[error("Linear API error: {0}")]
    Api(String),

    /// Linear API key was not provided.
    ///
    /// This occurs when attempting to create a client without a valid
    /// API key. Provide a key via [`LinearClient::new`]. Linear API keys
    /// typically start with `lin_api_` and can be generated from the
    /// Linear settings page.
    #[error("missing Linear API key")]
    MissingApiKey,

    /// Failed to serialize or deserialize JSON data.
    ///
    /// This may occur when parsing Linear GraphQL responses or constructing
    /// request bodies for queries and mutations. Since Linear uses GraphQL,
    /// this typically involves nested JSON structures.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    /// Outbound content was refused by the output guard (prompt-injection
    /// payload). The string names the detectors that fired.
    #[error("outbound content blocked: {0}")]
    OutputBlocked(String),

    /// An HTTP-level error occurred.
    ///
    /// This includes network failures, connection errors, DNS resolution
    /// failures, and other transport-layer issues when communicating with
    /// the Linear GraphQL API endpoint.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

/// Result type alias for Linear operations.
///
/// This is a convenience alias for `Result<T, LinearError>` used throughout
/// the Linear client implementation.
pub type Result<T> = std::result::Result<T, LinearError>;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearTeam {
    pub id: String,
    pub name: String,
    pub key: String,
}

/// A Linear project within a team.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearProject {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub state: String,
    pub team_id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearIssue {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    pub state_name: String,
    pub priority: u8,
    pub team: LinearTeam,
    pub assignee_name: Option<String>,
    pub labels: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub issue_id: String,
    pub success: bool,
    pub message: String,
}

/// A page of issues returned by [`LinearClient::list_issues_page`], carrying
/// GraphQL's `pageInfo` cursor so callers can keep paging past Linear's
/// per-request cap instead of silently truncating at the first page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearIssuePage {
    pub issues: Vec<LinearIssue>,
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}

/// Linear's own maximum page size for a single `first` argument.
const LINEAR_MAX_PAGE_SIZE: u32 = 250;
/// Safety cap on the number of pages [`LinearClient::list_issues`] will walk
/// so a misbehaving API (e.g. `hasNextPage` stuck `true`) can't loop forever.
const LIST_ISSUES_MAX_PAGES: u32 = 100;

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// Default endpoint for the Linear GraphQL API.
pub(crate) const LINEAR_DEFAULT_GRAPHQL_URL: &str = "https://api.linear.app/graphql";

#[derive(Debug, Clone)]
pub struct LinearClient {
    pub api_key: String,
    pub active_team_id: Option<String>,
    /// Full GraphQL endpoint URL. Defaults to [`LINEAR_DEFAULT_GRAPHQL_URL`];
    /// override via [`LinearClient::new_with_url`] for tests / self-hosted
    /// deployments.
    pub graphql_url: String,
    /// Optional configured request timeout override. When `None`, `client`
    /// is built via [`crate::http::client`] (the crate-wide bounded default:
    /// 30s request / 10s connect timeout). When `Some(duration)`, that
    /// duration overrides the default request timeout and is wired through
    /// `reqwest::ClientBuilder::timeout` so slow servers surface as a
    /// `LinearError::Http` carrying a reqwest timeout.
    ///
    /// Kept as a marker so callers can introspect what the underlying
    /// `client` was built with; the timeout itself is wired into `client` at
    /// construction time and is not re-read on each request.
    #[allow(dead_code)]
    pub(crate) request_timeout: Option<std::time::Duration>,
    /// Shared HTTP client with sane timeouts (see `crate::http`), reused
    /// across calls instead of building a fresh `reqwest::Client` (and
    /// paying a new TLS handshake) on every `graphql()` call.
    pub(crate) client: reqwest::Client,
    /// Whether this client returns canned stub data instead of calling the
    /// real Linear API. Always `false` for a client built through
    /// [`LinearClient::new`], regardless of what the API key looks like — a
    /// bad or placeholder key must surface as a real API error, never a
    /// silent fake success. Only [`LinearClient::stub`] can set this.
    stub: bool,
}

impl LinearClient {
    pub fn new(api_key: &str) -> Result<Self> {
        Self::new_with_url(api_key, LINEAR_DEFAULT_GRAPHQL_URL)
    }

    /// Create a Linear client targeting a custom GraphQL endpoint.
    ///
    /// Primarily useful for tests against a mock HTTP server (e.g.
    /// `wiremock`). Production callers should use [`LinearClient::new`],
    /// which defaults to the public Linear API.
    pub fn new_with_url(api_key: &str, graphql_url: &str) -> Result<Self> {
        Self::new_with_url_and_timeout(api_key, graphql_url, None)
    }

    /// Create a Linear client with a custom GraphQL endpoint and an optional
    /// request timeout.
    ///
    /// When `timeout` is `None`, behavior is identical to
    /// [`LinearClient::new_with_url`] (the per-request `reqwest::Client` is
    /// built without an explicit timeout). When `Some(duration)`, the
    /// duration is applied via `reqwest::ClientBuilder::timeout` to every
    /// GraphQL request, so slow servers surface as a `LinearError::Http`.
    ///
    /// This constructor is additive: existing call sites of `new` and
    /// `new_with_url` continue to compile unchanged and exhibit identical
    /// behavior.
    pub fn new_with_url_and_timeout(
        api_key: &str,
        graphql_url: &str,
        timeout: Option<std::time::Duration>,
    ) -> Result<Self> {
        if api_key.is_empty() {
            return Err(LinearError::MissingApiKey);
        }
        // Build the reqwest client once so subsequent GraphQL requests share
        // a connection pool. `crate::http::client()` bounds the default
        // request/connect timeouts; a bare `reqwest::Client::new()` has no
        // timeout at all, so a stalled upstream would hang the request
        // handler indefinitely. `Client::builder().build()` is effectively
        // infallible for our use; on the rare TLS/system error we surface a
        // panic via `expect` rather than changing the public constructor
        // signature.
        let client = match timeout {
            Some(t) => reqwest::Client::builder()
                .timeout(t)
                .connect_timeout(crate::http::CONNECT_TIMEOUT)
                .user_agent("auto-tundra/1.0")
                .build()
                .expect("reqwest client builder"),
            None => crate::http::client(),
        };
        Ok(Self {
            api_key: api_key.to_string(),
            active_team_id: None,
            graphql_url: graphql_url.to_string(),
            request_timeout: timeout,
            client,
            stub: false,
        })
    }

    /// Create a client that returns canned data instead of ever calling the
    /// real Linear API. Only available to this crate's own tests (or
    /// downstream crates that opt into the `stub` feature), never to
    /// production code paths.
    #[cfg(any(test, feature = "stub"))]
    pub fn stub(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            active_team_id: None,
            graphql_url: LINEAR_DEFAULT_GRAPHQL_URL.to_string(),
            request_timeout: None,
            client: crate::http::client(),
            stub: true,
        }
    }

    /// Returns `true` when this client was built via [`LinearClient::stub`]
    /// and should return canned data instead of calling the real API.
    fn is_stub(&self) -> bool {
        self.stub
    }

    // -- stub helpers -------------------------------------------------------

    fn stub_team() -> LinearTeam {
        LinearTeam {
            id: "team-001".to_string(),
            name: "Engineering".to_string(),
            key: "ENG".to_string(),
        }
    }

    fn stub_issue(idx: u32, state: &str) -> LinearIssue {
        let now = Utc::now();
        LinearIssue {
            id: format!("issue-{idx:04}"),
            identifier: format!("ENG-{idx}"),
            title: format!("Stub Linear issue #{idx}"),
            description: Some("Auto-generated stub issue".to_string()),
            state_name: state.to_string(),
            priority: 2,
            team: Self::stub_team(),
            assignee_name: None,
            labels: vec!["stub".to_string()],
            created_at: now,
            updated_at: now,
            url: format!("https://linear.app/team/issue/ENG-{idx}"),
        }
    }

    // -- helpers ------------------------------------------------------------

    /// Execute a GraphQL query against the Linear API and return the parsed
    /// JSON body. Returns an `Err` if the response contains GraphQL errors.
    async fn graphql(
        &self,
        query: &str,
        variables: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<serde_json::Value> {
        let mut payload = serde_json::json!({ "query": query });
        if let Some(vars) = variables {
            let mut vars = serde_json::Value::Object(vars);
            crate::outbound::screen_json(&mut vars).map_err(LinearError::OutputBlocked)?;
            payload["variables"] = vars;
        }

        let resp = self
            .client
            .post(self.graphql_url.as_str())
            .header("Authorization", self.api_key.as_str())
            .json(&payload)
            .send()
            .await
            .map_err(LinearError::Http)?;

        let body: serde_json::Value = resp.json().await.map_err(LinearError::Http)?;

        if let Some(errors) = body.get("errors") {
            return Err(LinearError::Api(errors.to_string()));
        }

        Ok(body)
    }

    /// Parse a single JSON node into a `LinearIssue`.
    fn parse_issue(n: &serde_json::Value) -> LinearIssue {
        LinearIssue {
            id: n["id"].as_str().unwrap_or_default().to_string(),
            identifier: n["identifier"].as_str().unwrap_or_default().to_string(),
            title: n["title"].as_str().unwrap_or_default().to_string(),
            description: n["description"].as_str().map(|s| s.to_string()),
            state_name: n["state"]["name"].as_str().unwrap_or("Unknown").to_string(),
            priority: n["priority"].as_u64().unwrap_or(0) as u8,
            team: LinearTeam {
                id: n["team"]["id"].as_str().unwrap_or_default().to_string(),
                name: n["team"]["name"].as_str().unwrap_or_default().to_string(),
                key: n["team"]["key"].as_str().unwrap_or_default().to_string(),
            },
            assignee_name: n["assignee"]["name"].as_str().map(|s| s.to_string()),
            labels: n["labels"]["nodes"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|l| l["name"].as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
            created_at: chrono::DateTime::parse_from_rfc3339(
                n["createdAt"].as_str().unwrap_or("2024-01-01T00:00:00Z"),
            )
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
            updated_at: chrono::DateTime::parse_from_rfc3339(
                n["updatedAt"].as_str().unwrap_or("2024-01-01T00:00:00Z"),
            )
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
            url: n["url"].as_str().unwrap_or_default().to_string(),
        }
    }

    // -- public API ---------------------------------------------------------

    /// List a single page of issues, optionally filtered by team and state.
    ///
    /// `first` is clamped to Linear's own maximum page size (250). Pass the
    /// returned [`LinearIssuePage::end_cursor`] back in as `after` to fetch
    /// the next page while [`LinearIssuePage::has_next_page`] is `true`.
    pub async fn list_issues_page(
        &self,
        team_id: Option<&str>,
        state: Option<&str>,
        first: u32,
        after: Option<&str>,
    ) -> Result<LinearIssuePage> {
        let first = first.clamp(1, LINEAR_MAX_PAGE_SIZE);

        // Fall back to stubs during tests with fake keys.
        if self.is_stub() {
            let s = state.unwrap_or("In Progress");
            let issues = (1..=5.min(first)).map(|i| Self::stub_issue(i, s)).collect();
            return Ok(LinearIssuePage {
                issues,
                has_next_page: false,
                end_cursor: None,
            });
        }

        let query = r#"query($teamId: ID, $state: String, $first: Int, $after: String) {
            issues(filter: { team: { id: { eq: $teamId } }, state: { name: { eq: $state } } }, first: $first, after: $after) {
                nodes {
                    id
                    identifier
                    title
                    description
                    priority
                    createdAt
                    updatedAt
                    url
                    state { name }
                    team { id name key }
                    assignee { name }
                    labels { nodes { name } }
                }
                pageInfo {
                    hasNextPage
                    endCursor
                }
            }
        }"#;

        let mut variables = serde_json::Map::new();
        if let Some(tid) = team_id.or(self.active_team_id.as_deref()) {
            variables.insert("teamId".into(), serde_json::Value::String(tid.to_string()));
        }
        if let Some(s) = state {
            variables.insert("state".into(), serde_json::Value::String(s.to_string()));
        }
        variables.insert("first".into(), serde_json::Value::from(first));
        if let Some(cursor) = after {
            variables.insert(
                "after".into(),
                serde_json::Value::String(cursor.to_string()),
            );
        }

        let body = self.graphql(query, Some(variables)).await?;

        let nodes = body["data"]["issues"]["nodes"]
            .as_array()
            .ok_or_else(|| LinearError::Api("missing issues.nodes".into()))?;

        let issues = nodes.iter().map(Self::parse_issue).collect();
        let has_next_page = body["data"]["issues"]["pageInfo"]["hasNextPage"]
            .as_bool()
            .unwrap_or(false);
        let end_cursor = body["data"]["issues"]["pageInfo"]["endCursor"]
            .as_str()
            .map(|s| s.to_string());

        Ok(LinearIssuePage {
            issues,
            has_next_page,
            end_cursor,
        })
    }

    /// List all issues matching the filter, optionally filtered by team and
    /// state. Walks every page via [`LinearClient::list_issues_page`] until
    /// Linear reports no more pages (up to a safety cap of
    /// `LIST_ISSUES_MAX_PAGES` pages), instead of silently truncating at the
    /// first 50 results.
    pub async fn list_issues(
        &self,
        team_id: Option<&str>,
        state: Option<&str>,
    ) -> Result<Vec<LinearIssue>> {
        let mut all = Vec::new();
        let mut cursor: Option<String> = None;

        for _ in 0..LIST_ISSUES_MAX_PAGES {
            let page = self
                .list_issues_page(team_id, state, LINEAR_MAX_PAGE_SIZE, cursor.as_deref())
                .await?;
            all.extend(page.issues);

            if !page.has_next_page || page.end_cursor.is_none() {
                break;
            }
            cursor = page.end_cursor;
        }

        Ok(all)
    }

    /// Get a single issue by ID.
    pub async fn get_issue(&self, issue_id: &str) -> Result<LinearIssue> {
        // Fall back to stubs during tests with fake keys.
        if self.is_stub() {
            let mut issue = Self::stub_issue(1, "In Progress");
            issue.id = issue_id.to_string();
            return Ok(issue);
        }

        let query = r#"query($id: String!) {
            issue(id: $id) {
                id
                identifier
                title
                description
                priority
                createdAt
                updatedAt
                url
                state { name }
                team { id name key }
                assignee { name }
                labels { nodes { name } }
            }
        }"#;

        let mut variables = serde_json::Map::new();
        variables.insert("id".into(), serde_json::Value::String(issue_id.to_string()));

        let body = self.graphql(query, Some(variables)).await?;

        let node = &body["data"]["issue"];
        if node.is_null() {
            return Err(LinearError::Api(format!("issue not found: {issue_id}")));
        }

        Ok(Self::parse_issue(node))
    }

    /// List all teams the authenticated user has access to.
    pub async fn list_teams(&self) -> Result<Vec<LinearTeam>> {
        // Fall back to stubs during tests with fake keys.
        if self.is_stub() {
            return Ok(vec![Self::stub_team()]);
        }

        let query = r#"{ teams { nodes { id name key } } }"#;

        let body = self.graphql(query, None).await?;

        let nodes = body["data"]["teams"]["nodes"]
            .as_array()
            .ok_or_else(|| LinearError::Api("missing teams.nodes".into()))?;

        let teams = nodes
            .iter()
            .map(|n| LinearTeam {
                id: n["id"].as_str().unwrap_or_default().to_string(),
                name: n["name"].as_str().unwrap_or_default().to_string(),
                key: n["key"].as_str().unwrap_or_default().to_string(),
            })
            .collect();

        Ok(teams)
    }

    /// Update a Linear issue. Supports changing title, state, and/or description.
    /// Returns the updated issue on success.
    pub async fn update_issue(
        &self,
        issue_id: &str,
        title: Option<&str>,
        state_name: Option<&str>,
        description: Option<&str>,
    ) -> Result<LinearIssue> {
        // Fall back to stubs during tests with fake keys.
        if self.is_stub() {
            let mut issue = Self::stub_issue(1, state_name.unwrap_or("In Progress"));
            issue.id = issue_id.to_string();
            if let Some(t) = title {
                issue.title = t.to_string();
            }
            if let Some(d) = description {
                issue.description = Some(d.to_string());
            }
            return Ok(issue);
        }

        // Build the mutation input fields dynamically.
        let mut input_fields = Vec::new();
        let mut variables = serde_json::Map::new();
        variables.insert("id".into(), serde_json::Value::String(issue_id.to_string()));

        if let Some(t) = title {
            input_fields.push("title: $title");
            variables.insert("title".into(), serde_json::Value::String(t.to_string()));
        }
        if let Some(d) = description {
            input_fields.push("description: $description");
            variables.insert(
                "description".into(),
                serde_json::Value::String(d.to_string()),
            );
        }
        // For state_name we need to resolve the state ID. For now, pass it as
        // stateId if provided (callers should resolve the ID upstream).
        if let Some(s) = state_name {
            input_fields.push("stateId: $stateId");
            variables.insert("stateId".into(), serde_json::Value::String(s.to_string()));
        }

        let input_str = input_fields.join(", ");

        // Build variable declarations for the query.
        let mut var_decls = vec!["$id: String!".to_string()];
        if title.is_some() {
            var_decls.push("$title: String".to_string());
        }
        if description.is_some() {
            var_decls.push("$description: String".to_string());
        }
        if state_name.is_some() {
            var_decls.push("$stateId: String".to_string());
        }
        let var_str = var_decls.join(", ");

        let query = format!(
            r#"mutation({var_str}) {{
                issueUpdate(id: $id, input: {{ {input_str} }}) {{
                    issue {{
                        id identifier title description priority
                        createdAt updatedAt url
                        state {{ name }}
                        team {{ id name key }}
                        assignee {{ name }}
                        labels {{ nodes {{ name }} }}
                    }}
                }}
            }}"#
        );

        let body = self.graphql(&query, Some(variables)).await?;

        let node = &body["data"]["issueUpdate"]["issue"];
        if node.is_null() {
            return Err(LinearError::Api(format!(
                "update_issue failed for: {issue_id}"
            )));
        }

        Ok(Self::parse_issue(node))
    }

    /// Import issues from Linear by fetching each one.
    pub async fn import_issues(&self, issue_ids: Vec<String>) -> Result<Vec<ImportResult>> {
        let mut results = Vec::new();
        for id in issue_ids {
            match self.get_issue(&id).await {
                Ok(issue) => results.push(ImportResult {
                    issue_id: id,
                    success: true,
                    message: format!("Imported: {}", issue.title),
                }),
                Err(e) => results.push(ImportResult {
                    issue_id: id,
                    success: false,
                    message: e.to_string(),
                }),
            }
        }
        Ok(results)
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
        let client = LinearClient::new("lin_api_test123").unwrap();
        assert!(!client.api_key.is_empty());
    }

    #[test]
    fn client_missing_key() {
        let result = LinearClient::new("");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn list_issues_stub() {
        let client = LinearClient::stub("stub-key");
        let issues = client.list_issues(None, Some("Todo")).await.unwrap();
        assert_eq!(issues.len(), 5);
        assert_eq!(issues[0].state_name, "Todo");
    }

    #[tokio::test]
    async fn get_issue_stub() {
        let client = LinearClient::stub("stub-key");
        let issue = client.get_issue("custom-id").await.unwrap();
        assert_eq!(issue.id, "custom-id");
    }

    #[tokio::test]
    async fn import_issues_stub() {
        let client = LinearClient::stub("stub-key");
        let results = client
            .import_issues(vec!["a".to_string(), "b".to_string()])
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.success));
    }

    #[test]
    fn linear_issue_serde_roundtrip() {
        let issue = LinearClient::stub_issue(1, "Done");
        let json = serde_json::to_string(&issue).unwrap();
        let de: LinearIssue = serde_json::from_str(&json).unwrap();
        assert_eq!(de.identifier, "ENG-1");
        assert_eq!(de.state_name, "Done");
    }

    #[test]
    fn import_result_serde_roundtrip() {
        let r = ImportResult {
            issue_id: "x".to_string(),
            success: true,
            message: "ok".to_string(),
        };
        let json = serde_json::to_string(&r).unwrap();
        let de: ImportResult = serde_json::from_str(&json).unwrap();
        assert_eq!(de.issue_id, "x");
        assert!(de.success);
    }

    #[tokio::test]
    async fn test_list_teams_query_structure() {
        // Verify list_teams works with an explicit stub client.
        let client = LinearClient::stub("stub-key");
        let teams = client.list_teams().await.unwrap();
        assert_eq!(teams.len(), 1);
        assert_eq!(teams[0].key, "ENG");
        assert_eq!(teams[0].name, "Engineering");
    }

    /// Regression test for a stub fallback that used to infer "stub mode"
    /// from the shape of the API key (`starts_with("tok")`,
    /// `starts_with("test")`, `starts_with("stub")`, or `len() < 10`). A
    /// production client built through `new` must never silently switch
    /// into canned-data mode just because a placeholder key happens to look
    /// like a test key.
    #[test]
    fn real_constructor_is_never_a_stub_regardless_of_key_shape() {
        for key in ["tok_looks_fake", "test_key", "stub-key", "short", "lin_api_real_key_abc"] {
            let client = LinearClient::new(key).unwrap();
            assert!(
                !client.is_stub(),
                "key {key:?} incorrectly put the client into stub mode"
            );
        }
    }

    #[test]
    fn only_the_explicit_stub_constructor_is_a_stub() {
        let client = LinearClient::stub("anything");
        assert!(client.is_stub());
    }

    /// Regression test for a `list_issues` that was hard-capped at 50 with
    /// no pagination. A single stub page must not silently claim there is
    /// more data than it returned.
    #[tokio::test]
    async fn list_issues_page_reports_no_next_page_for_stub_data() {
        let client = LinearClient::stub("stub-key");
        let page = client
            .list_issues_page(None, Some("Todo"), 250, None)
            .await
            .unwrap();
        assert!(!page.has_next_page);
        assert!(page.end_cursor.is_none());
        assert_eq!(page.issues.len(), 5);
    }

    #[tokio::test]
    async fn list_issues_page_clamps_first_to_linear_max() {
        let client = LinearClient::stub("stub-key");
        // Requesting more than Linear's max page size must not panic or
        // silently request an out-of-range value.
        let page = client
            .list_issues_page(None, None, 10_000, None)
            .await
            .unwrap();
        assert!(page.issues.len() <= LINEAR_MAX_PAGE_SIZE as usize);
    }
}
