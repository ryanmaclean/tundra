use at_core::types::{Bead, BeadStatus, Lane};
use serde_json::json;
use uuid::Uuid;

use crate::types::{GitHubIssue, GitHubLabel, IssueState};

use super::client::{GitHubClient, Result};

/// List issues for the configured repository.
pub async fn list_issues(
    client: &GitHubClient,
    state_filter: Option<IssueState>,
    labels: Option<Vec<String>>,
    page: Option<u32>,
    per_page: Option<u8>,
) -> Result<Vec<GitHubIssue>> {
    let issue_handler = client.octocrab.issues(&client.owner, &client.repo);

    let mut handler = issue_handler.list();

    if let Some(state) = state_filter {
        let param = match state {
            IssueState::Open => octocrab::params::State::Open,
            IssueState::Closed => octocrab::params::State::Closed,
        };
        handler = handler.state(param);
    }

    // Bind labels outside the if-let so the borrow lives long enough.
    let label_list = labels.unwrap_or_default();
    if !label_list.is_empty() {
        handler = handler.labels(&label_list);
    }

    if let Some(p) = page {
        handler = handler.page(p);
    }

    if let Some(pp) = per_page {
        handler = handler.per_page(pp);
    }

    let page = handler.send().await?;

    let issues = page
        .items
        .into_iter()
        .map(octocrab_issue_to_github_issue)
        .collect();

    Ok(issues)
}

/// Get a single issue by number.
pub async fn get_issue(client: &GitHubClient, number: u64) -> Result<GitHubIssue> {
    let issue = client
        .octocrab
        .issues(&client.owner, &client.repo)
        .get(number)
        .await?;

    Ok(octocrab_issue_to_github_issue(issue))
}

/// Create a new issue.
pub async fn create_issue(
    client: &GitHubClient,
    title: &str,
    body: Option<&str>,
    labels: Option<Vec<String>>,
) -> Result<GitHubIssue> {
    let issue_handler = client.octocrab.issues(&client.owner, &client.repo);

    let mut builder = issue_handler.create(title);

    if let Some(b) = body {
        builder = builder.body(b);
    }

    if let Some(label_list) = labels {
        builder = builder.labels(label_list);
    }

    let issue = builder.send().await?;

    Ok(octocrab_issue_to_github_issue(issue))
}

/// Update an existing issue.
pub async fn update_issue(
    client: &GitHubClient,
    number: u64,
    title: Option<&str>,
    body: Option<&str>,
    state: Option<IssueState>,
    labels: Option<Vec<String>>,
) -> Result<GitHubIssue> {
    let issue_handler = client.octocrab.issues(&client.owner, &client.repo);

    let mut builder = issue_handler.update(number);

    if let Some(t) = title {
        builder = builder.title(t);
    }

    if let Some(b) = body {
        builder = builder.body(b);
    }

    if let Some(s) = state {
        let param = match s {
            IssueState::Open => octocrab::models::IssueState::Open,
            IssueState::Closed => octocrab::models::IssueState::Closed,
        };
        builder = builder.state(param);
    }

    let label_list = labels.unwrap_or_default();
    if !label_list.is_empty() {
        builder = builder.labels(&label_list);
    }

    let issue = builder.send().await?;

    Ok(octocrab_issue_to_github_issue(issue))
}

/// Convert a GitHub issue into an `at_core::types::Bead`.
pub fn import_issue_as_task(issue: &GitHubIssue) -> Bead {
    let status = match issue.state {
        IssueState::Open => BeadStatus::Backlog,
        IssueState::Closed => BeadStatus::Done,
    };

    Bead {
        id: Uuid::new_v4(),
        title: issue.title.clone(),
        description: issue.body.clone(),
        status,
        lane: Lane::Standard,
        priority: 0,
        agent_id: None,
        convoy_id: None,
        created_at: issue.created_at,
        updated_at: issue.updated_at,
        hooked_at: None,
        slung_at: None,
        done_at: if issue.state == IssueState::Closed {
            Some(issue.updated_at)
        } else {
            None
        },
        git_branch: None,
        metadata: Some(json!({
            "source": "github",
            "issue_number": issue.number,
            "html_url": issue.html_url,
            "author": issue.author,
            "labels": issue.labels.iter().map(|l| &l.name).collect::<Vec<_>>(),
        })),
    }
}

// ---- internal helpers -------------------------------------------------------

fn octocrab_issue_to_github_issue(issue: octocrab::models::issues::Issue) -> GitHubIssue {
    let state = match issue.state {
        octocrab::models::IssueState::Open => IssueState::Open,
        octocrab::models::IssueState::Closed => IssueState::Closed,
        _ => IssueState::Open,
    };

    let labels = issue
        .labels
        .iter()
        .map(|l| GitHubLabel {
            name: l.name.clone(),
            color: l.color.clone(),
            description: l.description.clone(),
        })
        .collect();

    let assignees = issue.assignees.iter().map(|a| a.login.clone()).collect();

    let author = issue.user.login.clone();

    GitHubIssue {
        number: issue.number,
        title: issue.title,
        body: issue.body,
        state,
        labels,
        assignees,
        author,
        created_at: issue.created_at,
        updated_at: issue.updated_at,
        comments: issue.comments as u64,
        html_url: issue.html_url.to_string(),
    }
}
