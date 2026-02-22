use serde::{Deserialize, Serialize};

use crate::types::{GitHubLabel, GitHubPullRequest, PrState};

use super::client::{GitHubClient, Result};

/// A file changed in a pull request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrFile {
    pub filename: String,
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
    pub patch: Option<String>,
}

/// List pull requests for the configured repository.
pub async fn list_pull_requests(
    client: &GitHubClient,
    state_filter: Option<PrState>,
    page: Option<u32>,
    per_page: Option<u8>,
) -> Result<Vec<GitHubPullRequest>> {
    let pulls_handler = client
        .octocrab
        .pulls(&client.owner, &client.repo);

    let mut handler = pulls_handler.list();

    if let Some(state) = state_filter {
        let param = match state {
            PrState::Open => octocrab::params::State::Open,
            PrState::Closed | PrState::Merged => octocrab::params::State::Closed,
        };
        handler = handler.state(param);
    }

    if let Some(p) = page {
        handler = handler.page(p);
    }

    if let Some(pp) = per_page {
        handler = handler.per_page(pp);
    }

    let page = handler.send().await?;

    let prs = page
        .items
        .into_iter()
        .map(octocrab_pr_to_github_pr)
        .collect();

    Ok(prs)
}

/// Get a single pull request by number.
pub async fn get_pull_request(
    client: &GitHubClient,
    number: u64,
) -> Result<GitHubPullRequest> {
    let pr = client
        .octocrab
        .pulls(&client.owner, &client.repo)
        .get(number)
        .await?;

    Ok(octocrab_pr_to_github_pr(pr))
}

/// Create a new pull request.
pub async fn create_pull_request(
    client: &GitHubClient,
    title: &str,
    body: Option<&str>,
    head: &str,
    base: &str,
) -> Result<GitHubPullRequest> {
    let pulls_handler = client
        .octocrab
        .pulls(&client.owner, &client.repo);

    let pr = pulls_handler
        .create(title, head, base)
        .body(body.unwrap_or(""))
        .send()
        .await?;

    Ok(octocrab_pr_to_github_pr(pr))
}

/// List files changed in a pull request.
pub async fn list_pr_files(
    client: &GitHubClient,
    number: u64,
) -> Result<Vec<PrFile>> {
    let files = client
        .octocrab
        .pulls(&client.owner, &client.repo)
        .list_files(number)
        .await?;

    let result = files
        .into_iter()
        .map(|f| PrFile {
            filename: f.filename,
            status: format!("{:?}", f.status),
            additions: f.additions,
            deletions: f.deletions,
            patch: f.patch,
        })
        .collect();

    Ok(result)
}

/// Merge a pull request by number.
pub async fn merge_pull_request(
    client: &GitHubClient,
    number: u64,
    commit_title: Option<&str>,
    merge_method: Option<&str>,
) -> Result<GitHubPullRequest> {
    // Use the REST API via octocrab's `_put` for merge
    let route = format!(
        "/repos/{}/{}/pulls/{}/merge",
        client.owner, client.repo, number
    );

    let mut body = serde_json::json!({});
    if let Some(title) = commit_title {
        body["commit_title"] = serde_json::json!(title);
    }
    if let Some(method) = merge_method {
        body["merge_method"] = serde_json::json!(method);
    }

    client
        .octocrab
        .put::<serde_json::Value, _, _>(route, Some(&body))
        .await?;

    // Fetch the updated PR to return current state
    get_pull_request(client, number).await
}

// ---- internal helpers -------------------------------------------------------

fn octocrab_pr_to_github_pr(pr: octocrab::models::pulls::PullRequest) -> GitHubPullRequest {
    let state = if pr.merged_at.is_some() {
        PrState::Merged
    } else {
        match pr.state {
            Some(octocrab::models::IssueState::Closed) => PrState::Closed,
            _ => PrState::Open,
        }
    };

    let labels = pr
        .labels
        .unwrap_or_default()
        .iter()
        .map(|l| GitHubLabel {
            name: l.name.clone(),
            color: l.color.clone(),
            description: l.description.clone(),
        })
        .collect();

    let reviewers = pr
        .requested_reviewers
        .unwrap_or_default()
        .iter()
        .map(|r| r.login.clone())
        .collect();

    let author = pr
        .user
        .as_ref()
        .map(|u| u.login.clone())
        .unwrap_or_default();

    let head_branch = pr.head.ref_field.clone();
    let base_branch = pr.base.ref_field.clone();

    let created_at = pr.created_at.unwrap_or_else(chrono::Utc::now);
    let updated_at = pr.updated_at.unwrap_or(created_at);

    GitHubPullRequest {
        number: pr.number,
        title: pr.title.unwrap_or_default(),
        body: pr.body,
        state,
        author,
        head_branch,
        base_branch,
        labels,
        reviewers,
        draft: pr.draft.unwrap_or(false),
        mergeable: pr.mergeable,
        additions: pr.additions.unwrap_or(0),
        deletions: pr.deletions.unwrap_or(0),
        changed_files: pr.changed_files.unwrap_or(0),
        created_at,
        updated_at,
        merged_at: pr.merged_at,
        html_url: pr
            .html_url
            .map(|u| u.to_string())
            .unwrap_or_default(),
    }
}
