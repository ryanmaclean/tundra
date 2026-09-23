use at_core::types::{Bead, BeadStatus, Lane};
use serde_json::json;
use uuid::Uuid;

use crate::types::{GitHubIssue, GitHubLabel, IssueState};

use super::client::{GitHubClient, Result};

/// Items requested per page when following pagination (GitHub's maximum).
const AUTO_PAGINATE_PER_PAGE: u8 = 100;

/// Safety cap on pages followed by a single call (100 x 100 = 10k items).
pub const MAX_PAGES: usize = 100;

/// List issues for the configured repository.
///
/// Pull requests (which GitHub's issues endpoint also returns) are always
/// filtered out.
///
/// - `page = None`: follows `Link: rel="next"` pagination and returns every
///   matching issue (up to [`MAX_PAGES`] pages; `per_page` defaults to 100).
/// - `page = Some(n)`: returns only that single page.
pub async fn list_issues(
    client: &GitHubClient,
    state_filter: Option<IssueState>,
    labels: Option<Vec<String>>,
    page: Option<u32>,
    per_page: Option<u8>,
) -> Result<Vec<GitHubIssue>> {
    list_issues_inner(client, state_filter, labels, page, per_page, None).await
}

/// List every issue (not PR) updated at or after `since`, newest-updated
/// first, following pagination. Uses the API's `since` parameter rather than
/// filtering a truncated first page client-side.
pub async fn list_issues_updated_since(
    client: &GitHubClient,
    state_filter: Option<IssueState>,
    since: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<GitHubIssue>> {
    list_issues_inner(client, state_filter, None, None, None, Some(since)).await
}

async fn list_issues_inner(
    client: &GitHubClient,
    state_filter: Option<IssueState>,
    labels: Option<Vec<String>>,
    page: Option<u32>,
    per_page: Option<u8>,
    since: Option<chrono::DateTime<chrono::Utc>>,
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

    if let Some(since) = since {
        handler = handler
            .since(since)
            .sort(octocrab::params::issues::Sort::Updated);
    }

    if let Some(p) = page {
        handler = handler.page(p);
        if let Some(pp) = per_page {
            handler = handler.per_page(pp);
        }
        let first = handler.send().await?;
        return Ok(convert_issues(first.items));
    }

    handler = handler.per_page(per_page.unwrap_or(AUTO_PAGINATE_PER_PAGE));
    let mut current = handler.send().await?;
    let mut items = std::mem::take(&mut current.items);
    let mut pages = 1;
    while pages < MAX_PAGES {
        match client
            .octocrab
            .get_page::<octocrab::models::issues::Issue>(&current.next)
            .await?
        {
            Some(mut next) => {
                items.append(&mut next.items);
                current = next;
                pages += 1;
            }
            None => break,
        }
    }
    if pages >= MAX_PAGES && current.next.is_some() {
        tracing::warn!(
            owner = %client.owner,
            repo = %client.repo,
            max_pages = MAX_PAGES,
            "GitHub issue listing truncated at page cap"
        );
    }

    Ok(convert_issues(items))
}

/// Drop pull requests and convert the remaining octocrab issues.
fn convert_issues(items: Vec<octocrab::models::issues::Issue>) -> Vec<GitHubIssue> {
    items
        .into_iter()
        .filter(|i| i.pull_request.is_none())
        .map(octocrab_issue_to_github_issue)
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GitHubConfig;
    use serde_json::Value;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn author_json() -> Value {
        let u = "https://api.github.com/users/octo";
        json!({
            "login": "octo", "id": 1, "node_id": "U1",
            "avatar_url": u, "gravatar_id": "", "url": u, "html_url": u,
            "followers_url": u, "following_url": u, "gists_url": u,
            "starred_url": u, "subscriptions_url": u, "organizations_url": u,
            "repos_url": u, "events_url": u, "received_events_url": u,
            "type": "User", "site_admin": false, "patch_url": null
        })
    }

    fn issue_json(number: u64, is_pr: bool) -> Value {
        let u = format!("https://api.github.com/repos/o/r/issues/{number}");
        let mut v = json!({
            "id": number, "node_id": format!("I{number}"),
            "url": u, "repository_url": "https://api.github.com/repos/o/r",
            "labels_url": u, "comments_url": u, "events_url": u,
            "html_url": format!("https://github.com/o/r/issues/{number}"),
            "number": number, "state": "open", "state_reason": null,
            "title": format!("Issue {number}"), "body": null,
            "user": author_json(), "labels": [], "assignees": [],
            "author_association": "OWNER", "locked": false, "comments": 0,
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-02T00:00:00Z"
        });
        if is_pr {
            v["pull_request"] = json!({
                "url": u, "html_url": u, "diff_url": u, "patch_url": u
            });
        }
        v
    }

    /// Serve `pages` (1-based by the `page` query param) with GitHub-style
    /// `Link: rel="next"` headers. Returns the base URI and a log of request
    /// targets.
    async fn mock_github(
        pages: Vec<Vec<Value>>,
    ) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log2 = log.clone();
        let base2 = base.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let mut buf = vec![0u8; 16384];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let target = req
                    .lines()
                    .next()
                    .and_then(|l| l.split_whitespace().nth(1))
                    .unwrap_or("")
                    .to_string();
                log2.lock().unwrap().push(target.clone());
                let page: usize = target
                    .split(['?', '&'])
                    .find_map(|kv| kv.strip_prefix("page="))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1);
                let body = serde_json::to_string(
                    pages.get(page - 1).cloned().unwrap_or_default().as_slice(),
                )
                .unwrap();
                let link = if page < pages.len() {
                    format!(
                        "link: <{base2}/repos/o/r/issues?per_page=100&page={}>; rel=\"next\"\r\n",
                        page + 1
                    )
                } else {
                    String::new()
                };
                let resp = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n{link}content-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
                let _ = sock.shutdown().await;
            }
        });
        (base, log)
    }

    fn client_for(base: &str) -> GitHubClient {
        GitHubClient::new_with_base_uri(
            GitHubConfig {
                token: Some("ghp_test".into()),
                owner: "o".into(),
                repo: "r".into(),
            },
            base,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn list_issues_follows_pagination_and_drops_pull_requests() {
        // Page 1: 100 items with 10 PRs; page 2: 35 items with 5 PRs.
        let page1: Vec<Value> = (1..=100).map(|n| issue_json(n, n % 10 == 0)).collect();
        let page2: Vec<Value> = (101..=135).map(|n| issue_json(n, n % 7 == 0)).collect();
        let (base, log) = mock_github(vec![page1, page2]).await;

        let issues = list_issues(&client_for(&base), Some(IssueState::Open), None, None, None)
            .await
            .unwrap();

        let prs_on_page2 = (101..=135).filter(|n| n % 7 == 0).count();
        assert_eq!(issues.len(), 135 - 10 - prs_on_page2);
        assert!(issues.iter().all(|i| i.number % 10 != 0 || i.number > 100));
        assert!(issues.iter().any(|i| i.number == 135), "second page not read");
        let log = log.lock().unwrap();
        assert_eq!(log.len(), 2, "expected two page requests: {log:?}");
        assert!(log[0].contains("per_page=100"), "{log:?}");
    }

    #[tokio::test]
    async fn explicit_page_fetches_single_page() {
        let page1: Vec<Value> = (1..=3).map(|n| issue_json(n, n == 2)).collect();
        let page2: Vec<Value> = (4..=5).map(|n| issue_json(n, false)).collect();
        let (base, log) = mock_github(vec![page1, page2]).await;

        let issues = list_issues(&client_for(&base), None, None, Some(1), Some(3))
            .await
            .unwrap();
        let numbers: Vec<u64> = issues.iter().map(|i| i.number).collect();
        assert_eq!(numbers, vec![1, 3]);
        assert_eq!(log.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn updated_since_sends_since_and_sort() {
        let (base, log) = mock_github(vec![vec![issue_json(1, false), issue_json(2, true)]]).await;
        let since = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let issues = list_issues_updated_since(&client_for(&base), None, since)
            .await
            .unwrap();
        assert_eq!(issues.len(), 1);
        let log = log.lock().unwrap();
        assert!(log[0].contains("since=2026-01-01"), "{log:?}");
        assert!(log[0].contains("sort=updated"), "{log:?}");
    }
}
