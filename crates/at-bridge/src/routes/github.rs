//! GitHub integration API endpoints.
//!
//! Exposes GitHub integration features including issue syncing, pull request
//! management, OAuth authentication, and release management over HTTP/JSON using Axum.

use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use at_core::config::CredentialProvider;
use at_core::types::Bead;
use at_integrations::github::{
    issues, oauth as gh_oauth, pr_automation::PrAutomation, pull_requests, sync::IssueSyncEngine,
};
use at_integrations::types::{GitHubConfig, GitHubRelease, IssueState, PrState};

use crate::http_api::{ApiState, PrPollStatus};

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct SyncResponse {
    message: String,
    imported: u64,
    statuses_synced: u64,
}

#[derive(Debug, Serialize)]
struct PrCreatedResponse {
    message: String,
    task_id: Uuid,
    pr_title: String,
    pr_branch: Option<String>,
}

/// Request body for creating a PR (supports stacked PRs via base_branch).
#[derive(Debug, Default, Deserialize)]
struct CreatePrRequest {
    /// Target branch for the PR. Defaults to "main". For stacked PRs, set to the parent branch (e.g. "feature/parent").
    #[serde(default)]
    pub base_branch: Option<String>,
}

/// Query params for GET /api/github/issues.
#[derive(Debug, Default, Deserialize)]
struct ListGitHubIssuesQuery {
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub labels: Option<String>,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub per_page: Option<u8>,
}

#[derive(Debug, Default, Deserialize)]
struct ListGitHubPrsQuery {
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub per_page: Option<u8>,
}

#[derive(Debug, Deserialize)]
struct OAuthCallbackRequest {
    code: String,
    state: String,
}

#[derive(Debug, Deserialize)]
struct CreateReleaseRequest {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the GitHub integration sub-router.
///
/// All routes are mounted under `/api/github/` — the caller is responsible for
/// nesting or merging this into the top-level router.
pub fn github_router() -> Router<Arc<ApiState>> {
    Router::new()
        // Sync
        .route("/api/github/sync", post(trigger_github_sync))
        .route("/api/github/sync/status", get(get_sync_status))
        // Issues
        .route("/api/github/issues", get(list_github_issues))
        .route(
            "/api/github/issues/{number}/import",
            post(import_github_issue),
        )
        // Pull Requests
        .route("/api/github/prs", get(list_github_prs))
        .route("/api/github/pr/{task_id}", post(create_pr_for_task))
        .route("/api/github/pr/{number}/watch", post(watch_pr))
        .route(
            "/api/github/pr/{number}/watch",
            axum::routing::delete(unwatch_pr),
        )
        .route("/api/github/pr/watched", get(list_watched_prs))
        // OAuth
        .route("/api/github/oauth/authorize", get(github_oauth_authorize))
        .route("/api/github/oauth/callback", post(github_oauth_callback))
        .route("/api/github/oauth/status", get(github_oauth_status))
        .route("/api/github/oauth/revoke", post(github_oauth_revoke))
        // Releases
        .route("/api/github/releases", post(create_release))
        .route("/api/github/releases", get(list_releases))
}

// ---------------------------------------------------------------------------
// Sync handlers
// ---------------------------------------------------------------------------

async fn trigger_github_sync(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let config = state.settings_manager.load_or_default();
    let int = &config.integrations;
    let token = CredentialProvider::from_env(&int.github_token_env);
    let owner = int.github_owner.as_deref().unwrap_or("").to_string();
    let repo = int.github_repo.as_deref().unwrap_or("").to_string();

    if token.as_ref().is_none_or(|t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "GitHub token not configured. Set the environment variable.",
                "env_var": int.github_token_env,
            })),
        );
    }
    if owner.is_empty() || repo.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "GitHub owner and repo must be set in settings (integrations).",
            })),
        );
    }

    let gh_config = GitHubConfig { token, owner, repo };
    let client = match at_integrations::github::client::GitHubClient::new(gh_config) {
        Ok(c) => c,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    {
        let mut status = state.sync_status.write().await;
        status.is_syncing = true;
    }

    let existing_beads: Vec<Bead> = state.beads.read().await.clone();
    let engine = IssueSyncEngine::new(client);
    let new_beads = match engine.import_open_issues(&existing_beads).await {
        Ok(b) => b,
        Err(e) => {
            let mut status = state.sync_status.write().await;
            status.is_syncing = false;
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    let imported_count = new_beads.len() as u64;
    state.beads.write().await.extend(new_beads);

    {
        let mut status = state.sync_status.write().await;
        status.is_syncing = false;
        status.last_sync_time = Some(chrono::Utc::now());
        status.issues_imported = status.issues_imported.saturating_add(imported_count);
    }

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(SyncResponse {
            message: "Sync completed".to_string(),
            imported: imported_count,
            statuses_synced: 0,
        })),
    )
}

async fn get_sync_status(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let status = state.sync_status.read().await;
    Json(serde_json::json!(*status))
}

// ---------------------------------------------------------------------------
// Issues handlers
// ---------------------------------------------------------------------------

/// GET /api/integrations/github/issues — list GitHub issues with optional state filter.
async fn list_github_issues(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<ListGitHubIssuesQuery>,
) -> impl IntoResponse {
    let config = state.settings_manager.load_or_default();
    let int = &config.integrations;
    let token = CredentialProvider::from_env(&int.github_token_env);
    let owner = int.github_owner.as_deref().unwrap_or("").to_string();
    let repo = int.github_repo.as_deref().unwrap_or("").to_string();

    if token.as_ref().is_none_or(|t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "GitHub token not configured. Set the environment variable.",
                "env_var": int.github_token_env,
            })),
        );
    }
    if owner.is_empty() || repo.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "GitHub owner and repo must be set in settings (integrations).",
            })),
        );
    }

    let gh_config = GitHubConfig { token, owner, repo };
    let client = match at_integrations::github::client::GitHubClient::new(gh_config) {
        Ok(c) => c,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    let state_filter = q
        .state
        .as_deref()
        .and_then(|s| match s.to_lowercase().as_str() {
            "open" => Some(IssueState::Open),
            "closed" => Some(IssueState::Closed),
            _ => None,
        });
    let labels: Option<Vec<String>> = q.labels.as_deref().filter(|s| !s.is_empty()).map(|s| {
        s.split(',')
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    });
    let list = match issues::list_issues(&client, state_filter, labels, q.page, q.per_page).await {
        Ok(issues) => issues,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    (axum::http::StatusCode::OK, Json(serde_json::json!(list)))
}

async fn import_github_issue(
    State(state): State<Arc<ApiState>>,
    Path(number): Path<u64>,
) -> impl IntoResponse {
    let config = state.settings_manager.load_or_default();
    let int = &config.integrations;
    let token = CredentialProvider::from_env(&int.github_token_env);
    let owner = int.github_owner.as_deref().unwrap_or("").to_string();
    let repo = int.github_repo.as_deref().unwrap_or("").to_string();

    if token.as_ref().is_none_or(|t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "GitHub token not configured. Set the environment variable.",
                "env_var": int.github_token_env,
            })),
        );
    }
    if owner.is_empty() || repo.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "GitHub owner and repo must be set in settings (integrations).",
            })),
        );
    }

    let gh_config = GitHubConfig { token, owner, repo };
    let client = match at_integrations::github::client::GitHubClient::new(gh_config) {
        Ok(c) => c,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    let issue = match issues::get_issue(&client, number).await {
        Ok(i) => i,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    let bead = issues::import_issue_as_task(&issue);
    state.beads.write().await.push(bead.clone());

    (
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!(bead)),
    )
}

// ---------------------------------------------------------------------------
// Pull Requests handlers
// ---------------------------------------------------------------------------

async fn list_github_prs(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<ListGitHubPrsQuery>,
) -> impl IntoResponse {
    let config = state.settings_manager.load_or_default();
    let int = &config.integrations;
    let token = CredentialProvider::from_env(&int.github_token_env);
    let owner = int.github_owner.as_deref().unwrap_or("").to_string();
    let repo = int.github_repo.as_deref().unwrap_or("").to_string();

    if token.as_ref().is_none_or(|t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "GitHub token not configured. Set the environment variable.",
                "env_var": int.github_token_env,
            })),
        );
    }
    if owner.is_empty() || repo.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "GitHub owner and repo must be set in settings (integrations).",
            })),
        );
    }

    let gh_config = GitHubConfig { token, owner, repo };
    let client = match at_integrations::github::client::GitHubClient::new(gh_config) {
        Ok(c) => c,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    let state_filter = q
        .state
        .as_deref()
        .and_then(|s| match s.to_lowercase().as_str() {
            "open" => Some(PrState::Open),
            "closed" => Some(PrState::Closed),
            "merged" => Some(PrState::Merged),
            _ => None,
        });

    let list =
        match pull_requests::list_pull_requests(&client, state_filter, q.page, q.per_page).await {
            Ok(prs) => prs,
            Err(e) => {
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                );
            }
        };

    (axum::http::StatusCode::OK, Json(serde_json::json!(list)))
}

/// POST /api/tasks/{task_id}/pr — create a GitHub pull request for a task's branch.
async fn create_pr_for_task(
    State(state): State<Arc<ApiState>>,
    Path(task_id): Path<Uuid>,
    body: Option<Json<CreatePrRequest>>,
) -> impl IntoResponse {
    let tasks = state.tasks.read().await;
    let task = match tasks.iter().find(|t| t.id == task_id) {
        Some(t) => t.clone(),
        None => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "task not found"})),
            );
        }
    };
    drop(tasks);

    if task.git_branch.is_none() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Task has no branch. Create a worktree for this task first.",
            })),
        );
    }

    let config = state.settings_manager.load_or_default();
    let int = &config.integrations;
    let token = CredentialProvider::from_env(&int.github_token_env);
    let owner = int.github_owner.as_deref().unwrap_or("").to_string();
    let repo = int.github_repo.as_deref().unwrap_or("").to_string();

    if token.as_ref().is_none_or(|t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "GitHub token not configured. Set the environment variable.",
                "env_var": int.github_token_env,
            })),
        );
    }
    if owner.is_empty() || repo.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "GitHub owner and repo must be set in settings (integrations).",
            })),
        );
    }

    let gh_config = GitHubConfig { token, owner, repo };
    let client = match at_integrations::github::client::GitHubClient::new(gh_config) {
        Ok(c) => c,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    let base_branch = body
        .as_ref()
        .and_then(|b| b.base_branch.as_deref())
        .filter(|s| !s.is_empty())
        .unwrap_or("main");
    let automation = PrAutomation::new(client);
    let pr = match automation.create_pr_for_task(&task, base_branch).await {
        Ok(p) => p,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    let response = PrCreatedResponse {
        message: "PR created".to_string(),
        task_id: task.id,
        pr_title: pr.title.clone(),
        pr_branch: Some(pr.head_branch.clone()),
    };

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "message": response.message,
            "task_id": response.task_id,
            "pr_title": response.pr_title,
            "pr_branch": response.pr_branch,
            "pr_base_branch": base_branch,
            "pr_number": pr.number,
            "pr_url": pr.html_url,
        })),
    )
}

/// POST /api/github/pr/{number}/watch — start watching a pull request for status updates.
async fn watch_pr(
    State(state): State<Arc<ApiState>>,
    Path(number): Path<u32>,
) -> impl IntoResponse {
    let status = PrPollStatus {
        pr_number: number,
        state: "open".to_string(),
        mergeable: None,
        checks_passed: None,
        last_polled: chrono::Utc::now(),
    };
    let mut registry = state.pr_poll_registry.write().await;
    registry.insert(number, status.clone());
    (axum::http::StatusCode::OK, Json(serde_json::json!(status)))
}

/// DELETE /api/github/pr/{number}/watch — stop watching a pull request.
async fn unwatch_pr(
    State(state): State<Arc<ApiState>>,
    Path(number): Path<u32>,
) -> impl IntoResponse {
    let mut registry = state.pr_poll_registry.write().await;
    if registry.remove(&number).is_some() {
        (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({"removed": number})),
        )
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "PR not watched"})),
        )
    }
}

/// GET /api/github/pr/watched — list all currently watched pull requests.
async fn list_watched_prs(State(state): State<Arc<ApiState>>) -> Json<Vec<PrPollStatus>> {
    let registry = state.pr_poll_registry.read().await;
    Json(registry.values().cloned().collect())
}

// ---------------------------------------------------------------------------
// OAuth handlers
// ---------------------------------------------------------------------------

/// GET /api/github/oauth/authorize — build the GitHub authorization URL.
async fn github_oauth_authorize(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let client_id = match std::env::var("GITHUB_OAUTH_CLIENT_ID") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "GITHUB_OAUTH_CLIENT_ID not set",
                })),
            );
        }
    };

    let redirect_uri = std::env::var("GITHUB_OAUTH_REDIRECT_URI")
        .unwrap_or_else(|_| "http://localhost:3000/api/github/oauth/callback".into());

    let scopes = std::env::var("GITHUB_OAUTH_SCOPES")
        .unwrap_or_else(|_| "repo,read:user,user:email".into())
        .split(',')
        .map(|s| s.trim().to_string())
        .collect::<Vec<_>>();

    let oauth_config = gh_oauth::GitHubOAuthConfig {
        client_id,
        client_secret: String::new(), // not needed for URL generation
        redirect_uri,
        scopes,
    };

    let oauth_client = gh_oauth::GitHubOAuthClient::new(oauth_config);
    let csrf_state = uuid::Uuid::new_v4().to_string();
    let url = oauth_client.authorization_url(&csrf_state);

    // Store the state for CSRF validation during callback
    let timestamp = chrono::Utc::now().to_rfc3339();
    state
        .oauth_pending_states
        .write()
        .await
        .insert(csrf_state.clone(), timestamp);

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "url": url,
            "state": csrf_state,
        })),
    )
}

/// POST /api/github/oauth/callback — exchange the authorization code for a token.
async fn github_oauth_callback(
    State(state): State<Arc<ApiState>>,
    Json(body): Json<OAuthCallbackRequest>,
) -> impl IntoResponse {
    // Validate CSRF state parameter with expiration check
    let mut pending_states = state.oauth_pending_states.write().await;
    let state_timestamp = pending_states.get(&body.state).cloned();

    let state_valid = if let Some(timestamp_str) = state_timestamp {
        // Parse the timestamp and check if it's within 10 minutes
        match chrono::DateTime::parse_from_rfc3339(&timestamp_str) {
            Ok(timestamp) => {
                let age =
                    chrono::Utc::now().signed_duration_since(timestamp.with_timezone(&chrono::Utc));

                if age.num_minutes() < 10 {
                    // Valid and not expired - remove it to prevent replay
                    pending_states.remove(&body.state);
                    true
                } else {
                    // Expired - also remove to clean up
                    pending_states.remove(&body.state);
                    false
                }
            }
            Err(_) => {
                // Invalid timestamp format - remove and reject
                pending_states.remove(&body.state);
                false
            }
        }
    } else {
        false
    };
    drop(pending_states);

    if !state_valid {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Invalid or expired OAuth state parameter" })),
        );
    }

    let client_id = match std::env::var("GITHUB_OAUTH_CLIENT_ID") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "GITHUB_OAUTH_CLIENT_ID not set" })),
            );
        }
    };
    let client_secret = match std::env::var("GITHUB_OAUTH_CLIENT_SECRET") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "GITHUB_OAUTH_CLIENT_SECRET not set" })),
            );
        }
    };
    let redirect_uri = std::env::var("GITHUB_OAUTH_REDIRECT_URI")
        .unwrap_or_else(|_| "http://localhost:3000/api/github/oauth/callback".into());

    let scopes = std::env::var("GITHUB_OAUTH_SCOPES")
        .unwrap_or_else(|_| "repo,read:user,user:email".into())
        .split(',')
        .map(|s| s.trim().to_string())
        .collect::<Vec<_>>();

    let oauth_config = gh_oauth::GitHubOAuthConfig {
        client_id,
        client_secret,
        redirect_uri,
        scopes,
    };

    let oauth_client = gh_oauth::GitHubOAuthClient::new(oauth_config);

    let token_resp = match oauth_client.exchange_code(&body.code).await {
        Ok(t) => t,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    // Fetch user info with the new token.
    let user = match oauth_client.get_user(&token_resp.access_token).await {
        Ok(u) => serde_json::to_value(&u).unwrap_or_default(),
        Err(e) => {
            tracing::warn!("failed to fetch GitHub user after OAuth: {e}");
            serde_json::json!(null)
        }
    };

    // Store token and user in shared state.
    *state.github_oauth_token.write().await = Some(token_resp.access_token.clone());
    *state.github_oauth_user.write().await = Some(user.clone());

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "authenticated": true,
            "user": user,
            "scope": token_resp.scope,
            "expires_in": token_resp.expires_in,
        })),
    )
}

/// GET /api/github/oauth/status — check whether the user is authenticated.
async fn github_oauth_status(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let token = state.github_oauth_token.read().await;
    let user = state.github_oauth_user.read().await;

    let authenticated = token.is_some();

    Json(serde_json::json!({
        "authenticated": authenticated,
        "user": *user,
    }))
}

/// POST /api/github/oauth/revoke — clear the stored OAuth token.
async fn github_oauth_revoke(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    *state.github_oauth_token.write().await = None;
    *state.github_oauth_user.write().await = None;

    Json(serde_json::json!({
        "revoked": true,
    }))
}

// ---------------------------------------------------------------------------
// Releases handlers
// ---------------------------------------------------------------------------

/// POST /api/releases — create a new GitHub release with tag, name, body, and metadata.
async fn create_release(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<CreateReleaseRequest>,
) -> impl IntoResponse {
    let config = state.settings_manager.load_or_default();
    let int = &config.integrations;
    let token = CredentialProvider::from_env(&int.github_token_env);
    let owner = int.github_owner.as_deref().unwrap_or("").to_string();
    let repo = int.github_repo.as_deref().unwrap_or("").to_string();

    if token.as_ref().is_none_or(|t| t.is_empty()) || owner.is_empty() || repo.is_empty() {
        let release = GitHubRelease {
            tag_name: req.tag_name,
            name: req.name,
            body: req.body,
            draft: req.draft,
            prerelease: req.prerelease,
            created_at: chrono::Utc::now(),
            html_url: format!("local://releases/{}", chrono::Utc::now().timestamp_millis()),
        };
        let mut releases = state.releases.write().await;
        releases.retain(|r| r.tag_name != release.tag_name);
        releases.push(release.clone());
        return (
            axum::http::StatusCode::CREATED,
            Json(serde_json::json!(release)),
        );
    }

    let gh_config = GitHubConfig { token, owner, repo };
    let client = match at_integrations::github::client::GitHubClient::new(gh_config) {
        Ok(c) => c,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    let route = format!("/repos/{}/{}/releases", client.owner(), client.repo());
    let payload = serde_json::json!({
        "tag_name": req.tag_name,
        "name": req.name,
        "body": req.body,
        "draft": req.draft,
        "prerelease": req.prerelease,
    });

    let created: serde_json::Value = match client.inner().post(route, Some(&payload)).await {
        Ok(v) => v,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": format!("GitHub release create failed: {e}") })),
            );
        }
    };

    let release = GitHubRelease {
        tag_name: created
            .get("tag_name")
            .and_then(|v| v.as_str())
            .unwrap_or(payload["tag_name"].as_str().unwrap_or_default())
            .to_string(),
        name: created
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| payload["name"].as_str().map(|s| s.to_string())),
        body: created
            .get("body")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| payload["body"].as_str().map(|s| s.to_string())),
        draft: created
            .get("draft")
            .and_then(|v| v.as_bool())
            .unwrap_or(payload["draft"].as_bool().unwrap_or(false)),
        prerelease: created
            .get("prerelease")
            .and_then(|v| v.as_bool())
            .unwrap_or(payload["prerelease"].as_bool().unwrap_or(false)),
        created_at: created
            .get("created_at")
            .and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now),
        html_url: created
            .get("html_url")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
    };

    let mut releases = state.releases.write().await;
    releases.retain(|r| r.tag_name != release.tag_name);
    releases.push(release.clone());
    (
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!(release)),
    )
}

/// GET /api/releases — list all GitHub releases for the configured repository.
async fn list_releases(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let config = state.settings_manager.load_or_default();
    let int = &config.integrations;
    let token = CredentialProvider::from_env(&int.github_token_env);
    let owner = int.github_owner.as_deref().unwrap_or("").to_string();
    let repo = int.github_repo.as_deref().unwrap_or("").to_string();

    if token.as_ref().is_some_and(|t| !t.is_empty()) && !owner.is_empty() && !repo.is_empty() {
        let gh_config = GitHubConfig { token, owner, repo };
        if let Ok(client) = at_integrations::github::client::GitHubClient::new(gh_config) {
            let route = format!("/repos/{}/{}/releases", client.owner(), client.repo());
            if let Ok(remote) = client
                .inner()
                .get::<Vec<serde_json::Value>, _, _>(&route, None::<&()>)
                .await
            {
                let releases: Vec<GitHubRelease> = remote
                    .into_iter()
                    .map(|r| GitHubRelease {
                        tag_name: r
                            .get("tag_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        name: r
                            .get("name")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        body: r
                            .get("body")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        draft: r.get("draft").and_then(|v| v.as_bool()).unwrap_or(false),
                        prerelease: r
                            .get("prerelease")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                        created_at: r
                            .get("created_at")
                            .and_then(|v| v.as_str())
                            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                            .unwrap_or_else(chrono::Utc::now),
                        html_url: r
                            .get("html_url")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                    })
                    .collect();

                let mut cache = state.releases.write().await;
                *cache = releases.clone();
                return (
                    axum::http::StatusCode::OK,
                    Json(serde_json::json!(releases)),
                );
            }
        }
    }

    let cached = state.releases.read().await.clone();
    (axum::http::StatusCode::OK, Json(serde_json::json!(cached)))
}
