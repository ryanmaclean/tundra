use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use at_core::config::CredentialProvider;

use super::state::ApiState;
use super::types::{
    ImportLinearBody, ListGitLabIssuesQuery, ListGitLabMrsQuery, ListLinearIssuesQuery,
    ReviewGitLabMrBody,
};

/// GET /api/gitlab/issues -- retrieve issues from a GitLab project.
pub(crate) async fn list_gitlab_issues(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<ListGitLabIssuesQuery>,
) -> impl IntoResponse {
    let cfg = state.settings_manager.load_or_default();
    let int = &cfg.integrations;

    let token = CredentialProvider::from_env(&int.gitlab_token_env);
    if token.as_ref().is_none_or(|t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "GitLab token not configured. Set the environment variable.",
                "env_var": int.gitlab_token_env,
            })),
        );
    }

    let project_id = q
        .project_id
        .clone()
        .or_else(|| int.gitlab_project_id.clone())
        .unwrap_or_default();
    if project_id.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "GitLab project ID is required (query param project_id or settings.integrations.gitlab_project_id).",
            })),
        );
    }

    let base_url = int
        .gitlab_url
        .clone()
        .unwrap_or_else(|| "https://gitlab.com".to_string());

    let client = match at_integrations::gitlab::GitLabClient::new_with_url(
        &base_url,
        token.as_deref().unwrap_or_default(),
    ) {
        Ok(c) => c,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    // Convert limit/offset to page/per_page for GitLab API
    let per_page = q
        .limit
        .map(|l| l as u32)
        .or(q.per_page)
        .unwrap_or(20);
    let page = q
        .offset
        .map(|o| (o as u32 / per_page) + 1)
        .or(q.page)
        .unwrap_or(1);

    match client
        .list_issues(&project_id, q.state.as_deref(), page, per_page)
        .await
    {
        Ok(issues) => (axum::http::StatusCode::OK, Json(serde_json::json!(issues))),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// GET /api/gitlab/merge-requests -- retrieve merge requests from a GitLab project.
pub(crate) async fn list_gitlab_merge_requests(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<ListGitLabMrsQuery>,
) -> impl IntoResponse {
    let cfg = state.settings_manager.load_or_default();
    let int = &cfg.integrations;

    let token = CredentialProvider::from_env(&int.gitlab_token_env);
    if token.as_ref().is_none_or(|t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "GitLab token not configured. Set the environment variable.",
                "env_var": int.gitlab_token_env,
            })),
        );
    }

    let project_id = q
        .project_id
        .clone()
        .or_else(|| int.gitlab_project_id.clone())
        .unwrap_or_default();
    if project_id.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "GitLab project ID is required (query param project_id or settings.integrations.gitlab_project_id).",
            })),
        );
    }

    let base_url = int
        .gitlab_url
        .clone()
        .unwrap_or_else(|| "https://gitlab.com".to_string());

    let client = match at_integrations::gitlab::GitLabClient::new_with_url(
        &base_url,
        token.as_deref().unwrap_or_default(),
    ) {
        Ok(c) => c,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    // Convert limit/offset to page/per_page for GitLab API
    let per_page = q
        .limit
        .map(|l| l as u32)
        .or(q.per_page)
        .unwrap_or(20);
    let page = q
        .offset
        .map(|o| (o as u32 / per_page) + 1)
        .or(q.page)
        .unwrap_or(1);

    match client
        .list_merge_requests(&project_id, q.state.as_deref(), page, per_page)
        .await
    {
        Ok(mrs) => (axum::http::StatusCode::OK, Json(serde_json::json!(mrs))),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// POST /api/gitlab/merge-requests/{iid}/review -- perform automated code review on a GitLab merge request.
pub(crate) async fn review_gitlab_merge_request(
    State(state): State<Arc<ApiState>>,
    Path(iid): Path<u32>,
    body: Option<Json<ReviewGitLabMrBody>>,
) -> impl IntoResponse {
    let cfg = state.settings_manager.load_or_default();
    let int = &cfg.integrations;

    let token = CredentialProvider::from_env(&int.gitlab_token_env);
    if token.as_ref().is_none_or(|t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "GitLab token not configured. Set the environment variable.",
                "env_var": int.gitlab_token_env,
            })),
        );
    }

    let req = body.map(|b| b.0).unwrap_or_default();
    let project_id = req
        .project_id
        .or_else(|| int.gitlab_project_id.clone())
        .unwrap_or_default();
    if project_id.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "GitLab project ID is required (request body project_id or settings.integrations.gitlab_project_id).",
            })),
        );
    }

    let base_url = int
        .gitlab_url
        .clone()
        .unwrap_or_else(|| "https://gitlab.com".to_string());
    let client = match at_integrations::gitlab::GitLabClient::new_with_url(
        &base_url,
        token.as_deref().unwrap_or_default(),
    ) {
        Ok(c) => c,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    let mut config = at_integrations::gitlab::mr_review::MrReviewConfig::default();
    if let Some(v) = req.severity_threshold {
        config.severity_threshold = v;
    }
    if let Some(v) = req.max_findings {
        config.max_findings = v;
    }
    if let Some(v) = req.auto_approve {
        config.auto_approve = v;
    }

    let engine = at_integrations::gitlab::mr_review::MrReviewEngine::with_client(config, client);
    let result = engine.review_mr(&project_id, iid).await;
    (axum::http::StatusCode::OK, Json(serde_json::json!(result)))
}

// ---------------------------------------------------------------------------
// Linear integration
// ---------------------------------------------------------------------------

/// GET /api/linear/issues -- retrieve issues from a Linear team.
pub(crate) async fn list_linear_issues(
    State(state): State<Arc<ApiState>>,
    Query(q): Query<ListLinearIssuesQuery>,
) -> impl IntoResponse {
    let cfg = state.settings_manager.load_or_default();
    let int = &cfg.integrations;

    let token = CredentialProvider::from_env(&int.linear_api_key_env);
    if token.as_ref().is_none_or(|t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "Linear API key not configured. Set the environment variable.",
                "env_var": int.linear_api_key_env,
            })),
        );
    }

    let client =
        match at_integrations::linear::LinearClient::new(token.as_deref().unwrap_or_default()) {
            Ok(c) => c,
            Err(e) => {
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                );
            }
        };

    let team = q.team_id.as_deref().or(int.linear_team_id.as_deref());
    if team.is_none() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Linear team_id is required (query param team_id or settings.integrations.linear_team_id).",
            })),
        );
    }

    let limit = q.limit.unwrap_or(50);
    let offset = q.offset.unwrap_or(0);

    match client.list_issues(team, q.state.as_deref()).await {
        Ok(issues) => {
            let paginated: Vec<_> = issues.into_iter().skip(offset).take(limit).collect();
            (axum::http::StatusCode::OK, Json(serde_json::json!(paginated)))
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// POST /api/linear/import -- import Linear issues by IDs and create corresponding tasks.
pub(crate) async fn import_linear_issues(
    State(state): State<Arc<ApiState>>,
    Json(body): Json<ImportLinearBody>,
) -> impl IntoResponse {
    let cfg = state.settings_manager.load_or_default();
    let int = &cfg.integrations;

    let token = CredentialProvider::from_env(&int.linear_api_key_env);
    if token.as_ref().is_none_or(|t| t.is_empty()) {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "Linear API key not configured. Set the environment variable.",
                "env_var": int.linear_api_key_env,
            })),
        );
    }

    let client =
        match at_integrations::linear::LinearClient::new(token.as_deref().unwrap_or_default()) {
            Ok(c) => c,
            Err(e) => {
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                );
            }
        };

    match client.import_issues(body.issue_ids).await {
        Ok(results) => (axum::http::StatusCode::OK, Json(serde_json::json!(results))),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}
