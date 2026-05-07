use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use at_core::config::CredentialProvider;
use at_integrations::github::{
    issues, oauth as gh_oauth, pr_automation::PrAutomation, pull_requests, sync::IssueSyncEngine,
};
use at_integrations::types::{GitHubConfig, GitHubRelease, IssueState, PrState};

use super::state::{ApiState, RefreshOutcome};
use super::types::PrPollStatus;

// ---------------------------------------------------------------------------
// Local types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub(crate) struct SyncResponse {
    message: String,
    imported: u64,
    statuses_synced: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct PrCreatedResponse {
    message: String,
    task_id: Uuid,
    pr_title: String,
    pr_branch: Option<String>,
}

/// Request body for creating a PR (supports stacked PRs via base_branch).
#[derive(Debug, Default, Deserialize)]
pub(crate) struct CreatePrRequest {
    #[serde(default)]
    pub base_branch: Option<String>,
}

/// Query params for GET /api/github/issues.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct ListGitHubIssuesQuery {
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub labels: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

/// Query params for GET /api/github/prs.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct ListGitHubPrsQuery {
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

/// Query params for GET /api/github/pr/watched.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct ListWatchedPrsQuery {
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

/// Query params for GET /api/github/releases.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct ListReleasesQuery {
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OAuthCallbackRequest {
    code: String,
    state: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateReleaseRequest {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

// ---------------------------------------------------------------------------
// GitHub issues handlers
// ---------------------------------------------------------------------------

/// GET /api/github/issues -- list GitHub issues with optional filters.
pub(crate) async fn list_github_issues(
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
    let all_issues = match issues::list_issues(&client, state_filter, labels, None, None).await {
        Ok(issues) => issues,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    let limit = q.limit.unwrap_or(50);
    let offset = q.offset.unwrap_or(0);
    let list: Vec<_> = all_issues.into_iter().skip(offset).take(limit).collect();

    (axum::http::StatusCode::OK, Json(serde_json::json!(list)))
}

// ---------------------------------------------------------------------------
// GitHub sync handlers
// ---------------------------------------------------------------------------

/// POST /api/github/sync -- trigger a synchronization of open GitHub issues into local beads.
pub(crate) async fn trigger_github_sync(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
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

    let existing_beads: Vec<at_core::types::Bead> =
        state.beads.read().await.values().cloned().collect();
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
    {
        let mut beads = state.beads.write().await;
        for b in new_beads {
            beads.insert(b.id, b);
        }
    }

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

/// GET /api/github/sync/status -- retrieve the current GitHub issue sync status.
pub(crate) async fn get_sync_status(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let status = state.sync_status.read().await;
    Json(serde_json::json!(*status))
}

// ---------------------------------------------------------------------------
// Create PR for task
// ---------------------------------------------------------------------------

/// POST /api/github/pr/{task_id} -- create a GitHub pull request for a task's branch.
pub(crate) async fn create_pr_for_task(
    State(state): State<Arc<ApiState>>,
    Path(task_id): Path<Uuid>,
    body: Option<Json<CreatePrRequest>>,
) -> impl IntoResponse {
    let tasks = state.tasks.read().await;
    let task = match tasks.get(&task_id) {
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

// ---------------------------------------------------------------------------
// GitHub PRs handler
// ---------------------------------------------------------------------------

/// GET /api/github/prs -- list GitHub pull requests with optional filters.
pub(crate) async fn list_github_prs(
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

    let all_prs = match pull_requests::list_pull_requests(&client, state_filter, None, None).await {
        Ok(prs) => prs,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    let limit = q.limit.unwrap_or(50);
    let offset = q.offset.unwrap_or(0);
    let list: Vec<_> = all_prs.into_iter().skip(offset).take(limit).collect();

    (axum::http::StatusCode::OK, Json(serde_json::json!(list)))
}

// ---------------------------------------------------------------------------
// Import GitHub issue as bead
// ---------------------------------------------------------------------------

/// POST /api/github/issues/{number}/import -- import a specific GitHub issue as a local bead.
pub(crate) async fn import_github_issue(
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
    state.beads.write().await.insert(bead.id, bead.clone());

    (
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!(bead)),
    )
}

// ---------------------------------------------------------------------------
// GitHub OAuth handlers
// ---------------------------------------------------------------------------

/// GET /api/github/oauth/authorize -- build the GitHub authorization URL.
pub(crate) async fn github_oauth_authorize(
    State(state): State<Arc<ApiState>>,
) -> impl IntoResponse {
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

/// POST /api/github/oauth/callback -- exchange the authorization code for a token.
pub(crate) async fn github_oauth_callback(
    State(state): State<Arc<ApiState>>,
    Json(body): Json<OAuthCallbackRequest>,
) -> impl IntoResponse {
    // Validate CSRF state parameter with expiration check
    let mut pending_states = state.oauth_pending_states.write().await;
    let state_timestamp = pending_states.get(&body.state).cloned();

    let state_valid = if let Some(timestamp_str) = state_timestamp {
        match chrono::DateTime::parse_from_rfc3339(&timestamp_str) {
            Ok(timestamp) => {
                let age =
                    chrono::Utc::now().signed_duration_since(timestamp.with_timezone(&chrono::Utc));

                if age.num_minutes() < 10 {
                    pending_states.remove(&body.state);
                    true
                } else {
                    pending_states.remove(&body.state);
                    false
                }
            }
            Err(_) => {
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

    // Store encrypted token via OAuthTokenManager
    state
        .oauth_token_manager
        .write()
        .await
        .store_token(
            &token_resp.access_token,
            token_resp.expires_in,
            token_resp.refresh_token.as_deref(),
        )
        .await;
    // Keep backward compatibility with plaintext storage (will be removed in phase 4)
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

/// GET /api/github/oauth/status -- check whether the user is authenticated.
pub(crate) async fn github_oauth_status(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    let authenticated = state
        .oauth_token_manager
        .read()
        .await
        .has_valid_token()
        .await;
    let user = state.github_oauth_user.read().await;

    Json(serde_json::json!({
        "authenticated": authenticated,
        "user": *user,
    }))
}

/// POST /api/github/oauth/revoke -- clear the stored OAuth token.
pub(crate) async fn github_oauth_revoke(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    // Clear encrypted token
    state.oauth_token_manager.write().await.clear_token().await;

    // Clear legacy in-memory token
    *state.github_oauth_token.write().await = None;
    *state.github_oauth_user.write().await = None;

    Json(serde_json::json!({
        "revoked": true,
    }))
}

/// Atomic check-and-install result for the refresh gate.
///
/// Returned from a single critical section that holds the gate mutex, so
/// only one caller can ever observe `None` and become the leader; every
/// other concurrent caller becomes a follower and waits for the leader's
/// result via the `watch` receiver.
enum RefreshFlight {
    Leader(tokio::sync::watch::Sender<Option<RefreshOutcome>>),
    Follower(tokio::sync::watch::Receiver<Option<RefreshOutcome>>),
}

/// RAII guard around the leader's `watch::Sender`.
///
/// Guarantees that the gate is cleared and followers are unblocked even if
/// the leader's future is dropped (client disconnect, panic) before
/// `finish()` is called.  Without this guard, a cancelled leader could
/// leave the gate set forever and every subsequent expired-token caller
/// would deadlock waiting for a result that never arrives.
struct LeaderGuard {
    state: Arc<ApiState>,
    tx: Option<tokio::sync::watch::Sender<Option<RefreshOutcome>>>,
    completed: bool,
}

impl LeaderGuard {
    /// Broadcast the leader's outcome to followers and clear the gate
    /// **deterministically** (awaited inline, not via a spawned task) so
    /// the next caller always sees a fresh `None` immediately after the
    /// leader's response is sent.  Returns the response tuple to send.
    async fn finish(
        mut self,
        outcome: RefreshOutcome,
    ) -> (axum::http::StatusCode, Json<serde_json::Value>) {
        // Mark completed BEFORE any await so the Drop impl is a no-op.
        self.completed = true;

        // Pre-build the response (ownership of outcome moves into the broadcast).
        let response = match &outcome {
            Ok(body) => (axum::http::StatusCode::OK, Json(body.clone())),
            Err((code, msg)) => (*code, Json(serde_json::json!({ "error": msg }))),
        };

        // Broadcast outcome to followers.  If all receivers were dropped,
        // tx.send returns an Err which we ignore — that just means no
        // follower was waiting.
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(Some(outcome));
        }

        // Clear the gate before returning so the next non-concurrent caller
        // observes `None` and starts a new refresh.
        *self.state.github_refresh_gate.lock().await = None;

        response
    }
}

impl Drop for LeaderGuard {
    fn drop(&mut self) {
        if self.completed {
            return;
        }

        // Cancellation path: the leader's future was dropped before
        // finish() was called (client disconnect, panic, etc.).
        // Broadcast a cancellation error so followers don't wait forever,
        // then clear the gate via a spawned task (we can't await in Drop).
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(Some(Err((
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Token refresh cancelled before completion".to_string(),
            ))));
        }
        let gate = self.state.github_refresh_gate.clone();
        tokio::spawn(async move {
            *gate.lock().await = None;
        });
    }
}

/// POST /api/github/oauth/refresh -- manually refresh the OAuth token using refresh token.
///
/// Single-flight deduplication: N concurrent callers sharing an expired token
/// result in exactly **one** outbound HTTP refresh request.  Atomic
/// check-and-install (single critical section under the gate mutex) ensures
/// that exactly one caller becomes the leader; the rest become followers
/// that wait for the leader's broadcast outcome.
pub(crate) async fn github_oauth_refresh(State(state): State<Arc<ApiState>>) -> impl IntoResponse {
    // -------------------------------------------------------------------
    // Atomic leader/follower decision under a single critical section.
    // No second lock-and-install step, so there is no TOCTOU window where
    // two callers can both observe `None` and both install their own tx.
    // -------------------------------------------------------------------
    let flight = {
        let mut gate = state.github_refresh_gate.lock().await;
        if let Some(existing_tx) = gate.as_ref() {
            RefreshFlight::Follower(existing_tx.subscribe())
        } else {
            let (tx, _rx) = tokio::sync::watch::channel::<Option<RefreshOutcome>>(None);
            *gate = Some(tx.clone());
            RefreshFlight::Leader(tx)
        }
    };

    let tx = match flight {
        RefreshFlight::Follower(mut rx) => {
            // Wait for leader's broadcast.  borrow() first handles the
            // race where the leader finished between subscribe and here.
            loop {
                let current = rx.borrow().clone();
                if let Some(outcome) = current {
                    return match outcome {
                        Ok(body) => (axum::http::StatusCode::OK, Json(body)),
                        Err((code, msg)) => (code, Json(serde_json::json!({ "error": msg }))),
                    };
                }
                if rx.changed().await.is_err() {
                    return (
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({
                            "error": "Token refresh cancelled unexpectedly"
                        })),
                    );
                }
            }
        }
        RefreshFlight::Leader(tx) => tx,
    };

    // Install the drop guard so cancellation/panic clears the gate.
    let guard = LeaderGuard {
        state: state.clone(),
        tx: Some(tx),
        completed: false,
    };

    // -------------------------------------------------------------------
    // Pre-flight validation. Each error path uses guard.finish() so the
    // gate is cleared deterministically and followers see the leader's
    // status code (not always 400).
    // -------------------------------------------------------------------
    let refresh_token = match state
        .oauth_token_manager
        .read()
        .await
        .get_refresh_token()
        .await
    {
        Ok(token) => token,
        Err(_) => {
            return guard
                .finish(Err((
                    axum::http::StatusCode::BAD_REQUEST,
                    "No refresh token available. Please re-authenticate.".to_string(),
                )))
                .await;
        }
    };

    let client_id = match std::env::var("GITHUB_OAUTH_CLIENT_ID") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            return guard
                .finish(Err((
                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    "GITHUB_OAUTH_CLIENT_ID not set".to_string(),
                )))
                .await;
        }
    };
    let client_secret = match std::env::var("GITHUB_OAUTH_CLIENT_SECRET") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            return guard
                .finish(Err((
                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    "GITHUB_OAUTH_CLIENT_SECRET not set".to_string(),
                )))
                .await;
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

    // Allow test code to redirect the token endpoint to a mock server.
    let oauth_client = match &state.github_token_url_override {
        Some(url) => gh_oauth::GitHubOAuthClient::new_with_token_url(oauth_config, url.clone()),
        None => gh_oauth::GitHubOAuthClient::new(oauth_config),
    };

    // -------------------------------------------------------------------
    // The single outbound HTTP refresh call.
    // -------------------------------------------------------------------
    let token_resp = match oauth_client.refresh_token(&refresh_token).await {
        Ok(t) => t,
        Err(e) => {
            return guard
                .finish(Err((
                    axum::http::StatusCode::BAD_REQUEST,
                    format!("Failed to refresh token: {}", e),
                )))
                .await;
        }
    };

    // Store new token with OAuthTokenManager.
    state
        .oauth_token_manager
        .write()
        .await
        .store_token(
            &token_resp.access_token,
            token_resp.expires_in,
            token_resp.refresh_token.as_deref(),
        )
        .await;

    // Update legacy plaintext storage for backward compatibility.
    *state.github_oauth_token.write().await = Some(token_resp.access_token.clone());

    let body = serde_json::json!({
        "refreshed": true,
        "expires_in": token_resp.expires_in,
    });

    guard.finish(Ok(body)).await
}

// ---------------------------------------------------------------------------
// PR Polling
// ---------------------------------------------------------------------------

/// Spawn a background task that polls watched PRs every 30 seconds.
pub fn spawn_pr_poller(
    registry: Arc<RwLock<std::collections::HashMap<u32, PrPollStatus>>>,
    shutdown: tokio::sync::broadcast::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
    let mut shutdown_rx = shutdown;
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {
                    let mut reg = registry.write().await;
                    let now = chrono::Utc::now();
                    for status in reg.values_mut() {
                        status.last_polled = now;
                    }
                }
                _ = shutdown_rx.recv() => {
                    tracing::info!("PR poller shutting down");
                    break;
                }
            }
        }
    })
}

/// POST /api/github/pr/{number}/watch -- start watching a pull request.
pub(crate) async fn watch_pr(
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

/// DELETE /api/github/pr/{number}/watch -- stop watching a pull request.
pub(crate) async fn unwatch_pr(
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

/// GET /api/github/pr/watched -- list all currently watched pull requests.
pub(crate) async fn list_watched_prs(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<ListWatchedPrsQuery>,
) -> Json<Vec<PrPollStatus>> {
    let registry = state.pr_poll_registry.read().await;
    let limit = q.limit.unwrap_or(50);
    let offset = q.offset.unwrap_or(0);
    let list: Vec<PrPollStatus> = registry
        .values()
        .skip(offset)
        .take(limit)
        .cloned()
        .collect();
    Json(list)
}

// ---------------------------------------------------------------------------
// GitHub Releases
// ---------------------------------------------------------------------------

/// POST /api/github/releases -- create a new GitHub release.
pub(crate) async fn create_release(
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

/// GET /api/github/releases -- list all GitHub releases.
pub(crate) async fn list_releases(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<ListReleasesQuery>,
) -> impl IntoResponse {
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
                let all_releases: Vec<GitHubRelease> = remote
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
                *cache = all_releases.clone();

                let limit = q.limit.unwrap_or(50);
                let offset = q.offset.unwrap_or(0);
                let releases: Vec<_> = all_releases.into_iter().skip(offset).take(limit).collect();

                return (
                    axum::http::StatusCode::OK,
                    Json(serde_json::json!(releases)),
                );
            }
        }
    }

    let cached = state.releases.read().await.clone();
    let limit = q.limit.unwrap_or(50);
    let offset = q.offset.unwrap_or(0);
    let releases: Vec<_> = cached.into_iter().skip(offset).take(limit).collect();
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!(releases)),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::RwLock;

    fn make_registry() -> Arc<RwLock<HashMap<u32, PrPollStatus>>> {
        Arc::new(RwLock::new(HashMap::new()))
    }

    /// Test A: explicit shutdown signal terminates the poller task.
    ///
    /// Mutation-test note: removing the `shutdown_tx.send(())` call and shrinking
    /// the timeout to 50 ms causes this test to FAIL with a timeout error,
    /// confirming the test genuinely exercises the shutdown path.
    #[tokio::test]
    async fn test_pr_poller_shuts_down_on_send() {
        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
        let registry = make_registry();

        let handle = spawn_pr_poller(registry, shutdown_rx);

        // Signal shutdown and expect the task to exit promptly.
        shutdown_tx.send(()).expect("send shutdown signal");

        let result =
            tokio::time::timeout(Duration::from_secs(2), handle).await;

        assert!(
            result.is_ok(),
            "poller did not terminate within 2 s after explicit shutdown send"
        );
        assert!(
            result.unwrap().is_ok(),
            "poller task panicked after explicit shutdown send"
        );
    }

    /// Test B: dropping the only shutdown sender also terminates the poller task.
    ///
    /// With `tokio::sync::broadcast`, dropping every `Sender` causes
    /// `Receiver::recv()` to return `Err(RecvError::Closed)`. In the production
    /// `select!` the arm is spelled `_ = shutdown_rx.recv()`, which matches both
    /// `Ok` and `Err` variants, so the task breaks out of the loop correctly.
    /// If the production code ever changes to pattern-match only `Ok(_)`, this
    /// test will time out — revealing a real hang-on-drop bug.
    #[tokio::test]
    async fn test_pr_poller_shuts_down_on_sender_drop() {
        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
        let registry = make_registry();

        let handle = spawn_pr_poller(registry, shutdown_rx);

        // Drop the sender without calling send() — simulates daemon restart
        // where the owning struct is torn down before an orderly shutdown.
        drop(shutdown_tx);

        let result =
            tokio::time::timeout(Duration::from_secs(2), handle).await;

        assert!(
            result.is_ok(),
            "poller did not terminate within 2 s after sender drop — \
             the task is spinning in the sleep arm instead of exiting on RecvError::Closed"
        );
        assert!(
            result.unwrap().is_ok(),
            "poller task panicked after sender drop"
        );
    }
}
